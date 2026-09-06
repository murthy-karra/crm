#!/usr/bin/env bash
# Slice 011b targeted 50k count-plan evidence.
#
# This harness deliberately creates one fresh crm_011b_perf_* database, applies
# the current migration binary, seeds only synthetic rows there, records four
# EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) plans, and drops that database by
# default. It never seeds, truncates, migrates, or otherwise changes the shared
# development database. Artifacts contain only synthetic IDs/counts and plans.
#
# Run only when the primary DB operator has serialized this against other
# database work:
#   /tmp/crm-011b-perf/run_perf.sh
#
# Optional: KEEP_PERF_DB=1 retains the generated database for manual review.
# Optional: PERF_PSQL_MODE=host|docker selects psql transport. `auto` prefers
# host psql and falls back to the existing development-postgres-1 container.
set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The harness lives outside the checkout. Run it from the repository root as
# documented, or set CRM_REPO_ROOT explicitly; never guess a shared database.
repo_root="${CRM_REPO_ROOT:-$PWD}"
if [[ ! -f "$repo_root/backend/Cargo.toml" || ! -d "$repo_root/backend/crates/crm-api/migrations" ]]; then
  echo "run from the repository root or set CRM_REPO_ROOT to that root" >&2
  exit 1
fi
repo_root="$(cd "$repo_root" && pwd)"
env_file="$repo_root/.env"
run_stamp="$(date +%Y%m%d_%H%M%S)_$$"
run_dir="$script_dir/runs/$run_stamp"
sql_dir="$run_dir/sql"
plans_dir="$run_dir/plans"

if [[ ! -f "$env_file" ]]; then
  echo "missing $env_file; copy .env.example and configure local development first" >&2
  exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to apply the current migration binary" >&2
  exit 1
fi

# Source only for this process; do not print any values or enable xtrace.
set -a
# shellcheck disable=SC1090
source "$env_file"
set +a
: "${MIGRATION_DATABASE_URL:?MIGRATION_DATABASE_URL is required}"

perf_db="crm_011b_perf_${run_stamp}"
# Generated locally, but retain this guard before any CREATE/DROP statement.
if [[ ! "$perf_db" =~ ^crm_011b_perf_[0-9]{8}_[0-9]{6}_[0-9]+$ ]]; then
  echo "refusing an unexpected performance database name" >&2
  exit 1
fi

# Replace only the path of MIGRATION_DATABASE_URL. The secret-bearing URL stays
# in process memory/environment and is never echoed or written to artifacts.
perf_db_url="$(PERF_DB_NAME="$perf_db" MIGRATION_DATABASE_URL="$MIGRATION_DATABASE_URL" python3 - <<'PY'
import os
from urllib.parse import quote, urlsplit, urlunsplit

url = urlsplit(os.environ['MIGRATION_DATABASE_URL'])
if url.scheme not in {'postgres', 'postgresql'} or not url.netloc:
    raise SystemExit('MIGRATION_DATABASE_URL must be a PostgreSQL URL')
print(urlunsplit((url.scheme, url.netloc, '/' + quote(os.environ['PERF_DB_NAME'], safe=''), url.query, url.fragment)))
PY
)"

psql_mode="${PERF_PSQL_MODE:-auto}"
container_name="${PERF_POSTGRES_CONTAINER:-development-postgres-1}"
case "$psql_mode" in
  auto)
    if command -v psql >/dev/null 2>&1; then
      psql_mode=host
    elif command -v docker >/dev/null 2>&1 && docker inspect "$container_name" >/dev/null 2>&1; then
      psql_mode=docker
    else
      echo "need host psql or the running Docker container $container_name" >&2
      exit 1
    fi
    ;;
  host)
    command -v psql >/dev/null 2>&1 || { echo "PERF_PSQL_MODE=host requires psql" >&2; exit 1; }
    ;;
  docker)
    command -v docker >/dev/null 2>&1 || { echo "PERF_PSQL_MODE=docker requires docker" >&2; exit 1; }
    docker inspect "$container_name" >/dev/null 2>&1 || { echo "Docker container $container_name is not available" >&2; exit 1; }
    ;;
  *)
    echo "PERF_PSQL_MODE must be auto, host, or docker" >&2
    exit 1
    ;;
esac

admin_psql() {
  # stdin is SQL against the existing migrator database solely for CREATE/DROP.
  if [[ "$psql_mode" == host ]]; then
    PSQLRC=/dev/null psql -X -qAt -v ON_ERROR_STOP=1 "$MIGRATION_DATABASE_URL"
  else
    docker exec -i "$container_name" sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"'
  fi
}

db_psql_file() {
  local sql_file="$1"
  if [[ "$psql_mode" == host ]]; then
    PSQLRC=/dev/null psql -X -qAt -v ON_ERROR_STOP=1 "$perf_db_url" -f "$sql_file"
  else
    docker exec -i "$container_name" sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$1"' sh "$perf_db" < "$sql_file"
  fi
}

db_plan_file() {
  local sql_file="$1"
  local output_file="$2"
  if [[ "$psql_mode" == host ]]; then
    PSQLRC=/dev/null psql -X -qAt -v ON_ERROR_STOP=1 "$perf_db_url" -f "$sql_file" > "$output_file"
  else
    docker exec -i "$container_name" sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$1"' sh "$perf_db" < "$sql_file" > "$output_file"
  fi
}

created=false
cleanup() {
  local status=$?
  trap - EXIT
  if [[ "$created" == true ]]; then
    if [[ "${KEEP_PERF_DB:-0}" == 1 ]]; then
      echo "==> retaining isolated database $perf_db (KEEP_PERF_DB=1)"
    else
      # The name is generated and checked above. Disconnect only this database
      # before dropping it; no shared database is touched.
      printf "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '%s' AND pid <> pg_backend_pid();\nDROP DATABASE IF EXISTS \"%s\";\n" "$perf_db" "$perf_db" \
        | admin_psql >/dev/null 2>&1 || echo "warning: could not drop isolated database $perf_db" >&2
      echo "==> dropped isolated database $perf_db"
    fi
  fi
  exit "$status"
}
trap cleanup EXIT

mkdir -p "$sql_dir" "$plans_dir"

# This fixture has 50,000 People: 49,500 with a synthetic phone and 500
# without one. `has_phone=true` is dense/capped; `has_phone=false` is sparse
# and absence-proving. Both plans bind exactly the same matrix values in the
# count and existing filtered-summary shapes.
cat > "$sql_dir/seed.sql" <<'SQL'
\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE perf_seed_person (id uuid PRIMARY KEY, ordinal integer NOT NULL) ON COMMIT DROP;
INSERT INTO perf_seed_person (id, ordinal)
SELECT gen_random_uuid(), value FROM generate_series(1, 50000) AS value;

INSERT INTO organization (id, name, status, intake_slug, intake_token, intake_routing_mode)
VALUES (
  '11111111-1111-4111-8111-111111111111',
  'Slice 011b synthetic performance fixture',
  'active',
  'slice-011b-perf',
  'a1b2c3d4',
  'unassigned'
);
INSERT INTO stage (id, organization_id, name, position)
VALUES (
  '22222222-2222-4222-8222-222222222222',
  '11111111-1111-4111-8111-111111111111',
  'Synthetic',
  1
);

INSERT INTO person (id, organization_id, first_name, last_name, stage_id, created_at, updated_at)
SELECT seed.id,
       '11111111-1111-4111-8111-111111111111',
       'Synthetic',
       lpad(seed.ordinal::text, 5, '0'),
       '22222222-2222-4222-8222-222222222222',
       now() - ((seed.ordinal % 365) * interval '1 day'),
       now() - ((seed.ordinal % 365) * interval '1 day')
FROM perf_seed_person AS seed;

INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value, created_at)
SELECT '11111111-1111-4111-8111-111111111111',
       seed.id,
       'phone',
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       now()
FROM perf_seed_person AS seed
WHERE seed.ordinal <= 49500;
COMMIT;

ANALYZE organization;
ANALYZE stage;
ANALYZE person;
ANALYZE contact_method;

SELECT 'people=' || count(*) FROM person;
SELECT 'with_phone=' || count(*) FROM contact_method WHERE kind = 'phone';
SELECT 'without_phone=' || (SELECT count(*) FROM person) - count(*) FROM contact_method WHERE kind = 'phone';
SQL

# Exact 011b capped ID-only static matrix from
# crm-app/src/domain/person/queries.rs::count_filtered_matches. All bindings
# other than $19 are disabled (NULL); psql substitutes one synthetic org ID and
# either true or false for has_phone. Keep this in step with source if the
# static matrix changes.
cat > "$sql_dir/count.sql" <<'SQL'
\set ON_ERROR_STOP on
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT count(*) AS count
FROM (
  SELECT p.id
  FROM person p
  JOIN stage s ON s.id = p.stage_id
  LEFT JOIN LATERAL (
    SELECT i2.source
    FROM inquiry i2
    WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
    ORDER BY i2.received_at DESC, i2.id DESC
    LIMIT 1
  ) latest_src ON true
  LEFT JOIN LATERAL (
    SELECT max(i3.received_at) AS ts
    FROM inquiry i3
    WHERE i3.person_id = p.id AND i3.organization_id = p.organization_id
  ) last_inquiry_ts ON true
  LEFT JOIN LATERAL (
    SELECT max(ca.occurred_at) AS ts
    FROM contact_attempted ca
    WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
  ) last_contact_ts ON true
  LEFT JOIN LATERAL (
    SELECT max(cc.occurred_at) AS ts
    FROM correspondence_captured cc
    WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
      AND cc.direction = 'inbound'
  ) last_inbound_ts ON true
  WHERE p.organization_id = '11111111-1111-4111-8111-111111111111'::uuid
    AND (NULL::uuid[] IS NULL OR p.stage_id = ANY(NULL::uuid[]))
    AND (NULL::uuid[] IS NULL OR p.assigned_user_id = ANY(NULL::uuid[])
         OR (NULL::boolean AND p.assigned_user_id IS NULL))
    AND (NULL::text[] IS NULL OR latest_src.source = ANY(NULL::text[]))
    AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
    AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
    AND (NULL::boolean IS NULL OR (p.created_at IS NULL) = NULL::boolean)
    AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
    AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
    AND (NULL::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = NULL::boolean)
    AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
    AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
    AND (NULL::boolean IS NULL OR (last_contact_ts.ts IS NULL) = NULL::boolean)
    AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
    AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
    AND (NULL::boolean IS NULL OR (last_inbound_ts.ts IS NULL) = NULL::boolean)
    AND (NULL::boolean IS NULL OR (EXISTS (
      SELECT 1 FROM correspondence_captured cc2
      WHERE cc2.person_id = p.id AND cc2.organization_id = p.organization_id
        AND cc2.direction = 'inbound'
    )) = NULL::boolean)
    AND (:'has_phone'::boolean IS NULL OR (EXISTS (
      SELECT 1 FROM contact_method cm3
      WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
        AND cm3.kind = 'phone'
    )) = :'has_phone'::boolean)
    AND (NULL::boolean IS NULL OR (EXISTS (
      SELECT 1 FROM contact_method cm4
      WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
        AND cm4.kind = 'email'
    )) = NULL::boolean)
  LIMIT 501
) capped;
SQL

# Existing 011a filtered_summaries static shape with the identical matrix
# bindings as count.sql. Its full projection/order is intentional: plans show
# what the existing People request does under the same dense/sparse predicate.
cat > "$sql_dir/summary.sql" <<'SQL'
\set ON_ERROR_STOP on
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT
  p.id, p.first_name, p.last_name, p.created_at,
  s.id AS stage_id, s.name AS stage_name,
  u.id AS assigned_user_id, u.display_name AS assigned_user_display_name,
  (SELECT cm.value FROM contact_method cm
    WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
    ORDER BY cm.created_at ASC LIMIT 1) AS primary_email,
  (SELECT cm.value FROM contact_method cm
    WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
    ORDER BY cm.created_at ASC LIMIT 1) AS primary_phone,
  (SELECT count(*) FROM inquiry i
    WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS inquiry_count,
  (SELECT max(i.received_at) FROM inquiry i
    WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS last_inquiry_at
FROM person p
JOIN stage s ON s.id = p.stage_id
LEFT JOIN app_user u ON u.id = p.assigned_user_id
LEFT JOIN LATERAL (
  SELECT i2.source
  FROM inquiry i2
  WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
  ORDER BY i2.received_at DESC, i2.id DESC
  LIMIT 1
) latest_src ON true
LEFT JOIN LATERAL (
  SELECT max(i3.received_at) AS ts
  FROM inquiry i3
  WHERE i3.person_id = p.id AND i3.organization_id = p.organization_id
) last_inquiry_ts ON true
LEFT JOIN LATERAL (
  SELECT max(ca.occurred_at) AS ts
  FROM contact_attempted ca
  WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
) last_contact_ts ON true
LEFT JOIN LATERAL (
  SELECT max(cc.occurred_at) AS ts
  FROM correspondence_captured cc
  WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
    AND cc.direction = 'inbound'
) last_inbound_ts ON true
WHERE p.organization_id = '11111111-1111-4111-8111-111111111111'::uuid
  AND (NULL::uuid[] IS NULL OR p.stage_id = ANY(NULL::uuid[]))
  AND (NULL::uuid[] IS NULL OR p.assigned_user_id = ANY(NULL::uuid[])
       OR (NULL::boolean AND p.assigned_user_id IS NULL))
  AND (NULL::text[] IS NULL OR latest_src.source = ANY(NULL::text[]))
  AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (p.created_at IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_contact_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_inbound_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM correspondence_captured cc2
    WHERE cc2.person_id = p.id AND cc2.organization_id = p.organization_id
      AND cc2.direction = 'inbound'
  )) = NULL::boolean)
  AND (:'has_phone'::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM contact_method cm3
    WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
      AND cm3.kind = 'phone'
  )) = :'has_phone'::boolean)
  AND (NULL::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM contact_method cm4
    WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
      AND cm4.kind = 'email'
  )) = NULL::boolean)
ORDER BY p.created_at DESC, p.id ASC
LIMIT 501;
SQL

# The metrics reader deliberately reports actual plan evidence rather than
# expectations. Root buffer fields are shown without summing descendants,
# which avoids double-counting buffers across plan nodes.
cat > "$run_dir/summarize_plans.py" <<'PY'
#!/usr/bin/env python3
import json
import pathlib
import sys

for raw in sorted(pathlib.Path(sys.argv[1]).glob('*.json')):
    payload = json.loads(raw.read_text())
    entry = payload[0]
    plan = entry['Plan']
    fields = [
        ('planning_ms', entry.get('Planning Time')),
        ('execution_ms', entry.get('Execution Time')),
        ('root_actual_rows', plan.get('Actual Rows')),
        ('root_actual_loops', plan.get('Actual Loops')),
        ('root_shared_hit_blocks', plan.get('Shared Hit Blocks')),
        ('root_shared_read_blocks', plan.get('Shared Read Blocks')),
        ('root_temp_read_blocks', plan.get('Temp Read Blocks')),
        ('root_temp_written_blocks', plan.get('Temp Written Blocks')),
    ]
    print(raw.name)
    for key, value in fields:
        print(f'  {key}: {value}')
PY

revision="$(git -C "$repo_root" rev-parse --short HEAD 2>/dev/null || printf 'unavailable')"
cat > "$run_dir/manifest.txt" <<EOF_MANIFEST
Slice 011b targeted plan evidence
source_revision=$revision
database_name=$perf_db
fixture_people=50000
fixture_with_phone=49500
fixture_without_phone=500
predicates=dense has_phone=true; sparse/absence has_phone=false
plans=count + existing filtered_summaries shape, same matrix values per predicate
psql_transport=$psql_mode
results_note=No timing or plan claim is pre-recorded. metrics.txt is generated only from this run's JSON plans.
EOF_MANIFEST

existing="$(printf "SELECT 1 FROM pg_database WHERE datname = '%s';\n" "$perf_db" | admin_psql)"
if [[ -n "$existing" ]]; then
  echo "refusing to reuse an existing database named $perf_db" >&2
  exit 1
fi

echo "==> creating isolated performance database $perf_db"
printf 'CREATE DATABASE "%s" OWNER crm_migrator;\n' "$perf_db" | admin_psql >/dev/null
created=true

echo "==> applying current migrations to isolated database"
(
  cd "$repo_root"
  MIGRATION_DATABASE_URL="$perf_db_url" cargo run --manifest-path backend/Cargo.toml -p crm-api --bin migrate
)

echo "==> seeding 50,000 synthetic People in isolated database"
db_psql_file "$sql_dir/seed.sql" > "$run_dir/seed-summary.txt"

run_plan() {
  local kind="$1"
  local label="$2"
  local value="$3"
  local rendered="$sql_dir/${kind}-${label}.sql"
  # Only the non-secret boolean scenario value is substituted into a copy.
  sed "s/:'has_phone'::boolean/${value}::boolean/g" "$sql_dir/${kind}.sql" > "$rendered"
  db_plan_file "$rendered" "$plans_dir/${kind}-${label}.json"
}

echo "==> recording dense count and filtered-summary plans"
run_plan count dense_has_phone_true true
run_plan summary dense_has_phone_true true

echo "==> recording sparse count and filtered-summary plans"
run_plan count sparse_has_phone_false false
run_plan summary sparse_has_phone_false false

python3 "$run_dir/summarize_plans.py" "$plans_dir" > "$run_dir/metrics.txt"

echo "==> evidence written to $run_dir"
echo "    Inspect metrics.txt and the JSON plan node trees before making any performance claim."

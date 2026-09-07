#!/usr/bin/env bash
set -euo pipefail
umask 077
repo_root=/Users/karrad/projects/crm
env_file="$repo_root/.env"
container_name=development-postgres-1
if [[ ! -f "$env_file" ]]; then
  echo "missing local environment" >&2
  exit 1
fi
if ! docker inspect "$container_name" >/dev/null 2>&1; then
  echo "required local postgres container unavailable" >&2
  exit 1
fi
set -a
source "$env_file"
set +a
: "${MIGRATION_DATABASE_URL:?required}"
stamp="$(date +%Y%m%d_%H%M%S)_$$"
perf_db="crm_011c_perf_${stamp}"
if [[ ! "$perf_db" =~ ^crm_011c_perf_[0-9]{8}_[0-9]{6}_[0-9]+$ ]]; then
  echo "unexpected generated database name" >&2
  exit 1
fi
perf_url="$(PERF_DB_NAME="$perf_db" MIGRATION_DATABASE_URL="$MIGRATION_DATABASE_URL" python3 - <<'PY'
import os
from urllib.parse import quote, urlsplit, urlunsplit
url = urlsplit(os.environ['MIGRATION_DATABASE_URL'])
if url.scheme not in {'postgres', 'postgresql'} or not url.netloc:
    raise SystemExit('invalid local migration URL')
print(urlunsplit((url.scheme, url.netloc, '/' + quote(os.environ['PERF_DB_NAME'], safe=''), url.query, url.fragment)))
PY
)"
admin_psql() {
  docker exec -i "$container_name" sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"'
}
db_psql_file() {
  local file="$1"
  docker exec -i "$container_name" sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$1"' sh "$perf_db" < "$file"
}
cleanup() {
  status=$?
  trap - EXIT
  if [[ "${KEEP_PERF_DB:-0}" != 1 && "${created:-false}" == true ]]; then
    printf "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '%s' AND pid <> pg_backend_pid();\nDROP DATABASE IF EXISTS \"%s\";\n" "$perf_db" "$perf_db" | admin_psql >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT
if [[ -n "$(printf "SELECT 1 FROM pg_database WHERE datname = '%s';\n" "$perf_db" | admin_psql)" ]]; then
  echo "generated database already exists" >&2
  exit 1
fi
printf 'CREATE DATABASE "%s" OWNER crm_migrator;\n' "$perf_db" | admin_psql >/dev/null
created=true
(
  cd "$repo_root"
  MIGRATION_DATABASE_URL="$perf_url" cargo run --quiet --manifest-path backend/Cargo.toml -p crm-api --bin migrate
) > /tmp/crm-011c-perf/migrate.log 2>&1
db_psql_file /tmp/crm-011c-perf/sql/seed.sql > /tmp/crm-011c-perf/seed-summary.txt
printf '%s' "$perf_url" > /tmp/crm-011c-perf/.perf-url
chmod 600 /tmp/crm-011c-perf/.perf-url
printf '%s\n' "$perf_db" > /tmp/crm-011c-perf/database-name.txt
echo "isolated database created and seeded"

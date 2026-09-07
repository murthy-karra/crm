# Reproduce the isolated prototype

Run only from the local workspace that has the existing development PostgreSQL container and its gitignored `.env`. The setup script obtains its migration URL from that local `.env`; it never prints or records it in this recipe.

Hold the repository's sole database-operation lane. These instructions target
the measured checkout at `/Users/karrad/projects/crm` (base `9d62e86`); the
Cargo manifest intentionally points at its existing `crm-app` crate. Confirm
its source hashes against `MANIFEST.json` before reproducing. Work from a
scratch copy so reruns do not overwrite the retained repository evidence:

```sh
cd /Users/karrad/projects/crm
mkdir -p /tmp/crm-011c-perf/metrics /tmp/crm-011c-perf/plans
cp -R docs/design/perf/slice-011c-2026-09-06/{Cargo.toml,Cargo.lock,setup.sh,src,sql,revisions} /tmp/crm-011c-perf/
```

```sh
cd /Users/karrad/projects/crm
KEEP_PERF_DB=1 /tmp/crm-011c-perf/setup.sh
```

The script generates the only permitted database name, stores it privately in `.perf-url`, and writes `database-name.txt`. Do not copy, print, or commit `.perf-url`.

Run the preserved exact baseline first. These copies change only the temporary harness directory. Each command reads the private URL without printing it. `PERF_JIT_OFF=1` is prototype-only `SET LOCAL jit = off`, not a server setting.

```sh
cp /tmp/crm-011c-perf/revisions/sql-baseline/{builtin.sql,source_prefix.sql,source_membership.sql,metadata.sql} /tmp/crm-011c-perf/sql/
perf_url_value=$(< /tmp/crm-011c-perf/.perf-url)
PERF_MODE=smoke PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=plans PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=plans_jit_off PERF_PLAN_LABEL=jit_off PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=critical PERF_LABEL=critical PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=critical PERF_LABEL=critical-jit-off PERF_JIT_OFF=1 PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
# Restore the preserved candidate SQL, then run candidate artifacts.
cp /tmp/crm-011c-perf/revisions/sql-candidate/{builtin.sql,source_prefix.sql,source_membership.sql,metadata.sql} /tmp/crm-011c-perf/sql/
PERF_MODE=plans_jit_off PERF_PLAN_LABEL=candidate_guards_jit_off PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=critical PERF_LABEL=critical-candidate-guards-jit-off PERF_JIT_OFF=1 PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=parity PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=remaining PERF_LABEL=remaining-candidate-guards-jit-off PERF_JIT_OFF=1 PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
PERF_MODE=five_c20_raw PERF_JIT_OFF=1 PERF_DATABASE_URL="$perf_url_value" PERF_ARTIFACT_DIR=/tmp/crm-011c-perf \
  cargo run --quiet --manifest-path=/tmp/crm-011c-perf/Cargo.toml
```

After saving only sanitized reports, plans, SQL, harness, and hashes, remove the generated database. This refuses every name except the exact guarded pattern and verifies removal. It connects to the existing container's configured development database; it never uses or prints a credential URL.

```sh
db_name_value=$(< /tmp/crm-011c-perf/database-name.txt)
if ! [[ "$db_name_value" =~ ^crm_011c_perf_[0-9]{8}_[0-9]{6}_[0-9]+$ ]]; then
  echo 'refusing non-guarded database name' >&2
  exit 64
fi
printf "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '%s' AND pid <> pg_backend_pid();\nDROP DATABASE IF EXISTS \"%s\";\n" "$db_name_value" "$db_name_value" |
  docker exec -i development-postgres-1 sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"'
printf "SELECT count(*) FROM pg_database WHERE datname = '%s';\nSELECT count(*) FROM pg_stat_activity WHERE datname = '%s';\n" "$db_name_value" "$db_name_value" |
  docker exec -i development-postgres-1 sh -c 'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"'
python3 - <<'PY'
from pathlib import Path
for name in ('.perf-url', 'database-name.txt'):
    p = Path('/tmp/crm-011c-perf') / name
    if p.exists():
        p.unlink()
PY
```

The final two counts must both be `0` before declaring cleanup complete.

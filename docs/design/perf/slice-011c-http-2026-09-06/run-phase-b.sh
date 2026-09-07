#!/usr/bin/env bash
# Slice 011c Phase B runner (coordinator-owned). Builds the optimized opt-in
# harness, hashes the exact binary and every manifest source file, records
# host facts, then executes the single ignored harness test once against a
# sqlx-managed ephemeral database. Never prints credential URLs.
set -uo pipefail
repo=/Users/karrad/projects/crm
out="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
step="${1:-all}"
cd "$repo"
set -a; source .env; set +a
: "${MIGRATION_DATABASE_URL:?MIGRATION_DATABASE_URL missing}"
: "${POSTGRES_USER:?POSTGRES_USER missing}"
cd "$repo/backend"

build() {
  echo "==> build optimized harness $(date '+%H:%M:%S')"
  cargo test --release --locked -p crm-api --test db_today_http_perf --features perf-harness --no-run > "$out/build.log" 2>&1
  rc=$?
  echo "BUILD_EXIT=$rc" >> "$out/build.log"
  [ "$rc" -eq 0 ] || { echo "build failed"; return 1; }
  bin_rel=$(grep -oE '\(target/release/deps/db_today_http_perf-[0-9a-f]+\)' "$out/build.log" | tail -1 | tr -d '()')
  [ -n "$bin_rel" ] || { echo "executable path not found in build log"; return 1; }
  bin="$repo/backend/$bin_rel"
  printf '%s\n' "$bin" > "$out/binary-path.txt"
  shasum -a 256 "$bin" | awk '{print $1}' > "$out/build-sha256.txt"
  : > "$out/source-sha256.txt"
  while IFS= read -r f; do
    [[ -z "$f" || "$f" == \#* ]] && continue
    shasum -a 256 "$f" >> "$out/source-sha256.txt"
  done < crates/crm-api/tests/fixtures/today_http_perf_manifest.txt
  {
    echo "captured_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "git_head=$(git rev-parse HEAD)"
    echo "git_status_lines=$(git status --short | wc -l | tr -d ' ')"
    echo "profile=release (cargo default; no custom profile in backend/Cargo.toml)"
    echo "features=perf-harness (implies test-support)"
    echo "binary=$bin_rel"
    echo "binary_sha256=$(cat "$out/build-sha256.txt")"
    echo "rustc=$(rustc --version)"; echo "cargo=$(cargo --version)"
    echo "os=$(sw_vers -productVersion) build $(sw_vers -buildVersion) $(uname -m)"
    echo "cpus=$(sysctl -n hw.ncpu) mem_bytes=$(sysctl -n hw.memsize)"
    echo "postgres=$(docker exec development-postgres-1 psql -U "$POSTGRES_USER" -d postgres -Atc 'select version()' 2>&1)"
  } > "$out/environment.txt"
  echo "build ok: $(cat "$out/build-sha256.txt")"
}

run() {
  bin=$(cat "$out/binary-path.txt")
  build_hash=$(cat "$out/build-sha256.txt")
  [ -x "$bin" ] || { echo "binary missing"; return 1; }
  echo "==> run harness $(date '+%H:%M:%S') (load: $(sysctl -n vm.loadavg))"
  echo "run_started_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$out/environment.txt"
  DATABASE_URL="$MIGRATION_DATABASE_URL" CRM_SLICE_011C_BUILD_HASH="$build_hash" \
    "$bin" --ignored --exact slice_011c_authenticated_http_performance_harness --nocapture --test-threads=1 \
    > "$out/run.log" 2>&1
  rc=$?
  echo "RUN_EXIT=$rc" >> "$out/run.log"
  echo "run_finished_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ') exit=$rc" >> "$out/environment.txt"
  echo "==> run finished exit=$rc $(date '+%H:%M:%S')"
  ls -la "$repo/backend/target/slice-011c-http/" 2>/dev/null
}

case "$step" in
  build) build ;;
  run) run ;;
  all) build && run ;;
  *) echo "usage: $0 build|run|all"; exit 64 ;;
esac

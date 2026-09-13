# Mobile 001 and Slice 010e1 — Combined integration verification

Date: 2026-09-12 (America/Los_Angeles). This record covers the combined backend
and repository gates. Native simulator/emulator and actual browser evidence have
separate owners; this record does not claim either platform is finished or released.

## Source and bounded review

Initial integration commit: `d6e7c747f6f24ccb13781aacd7d75dc3cd1b2e2c`, branch
`codex/mobile-migration-integration`. It includes mobile `feab066`, the legacy
upgrade assertion correction `5568d70`, and 010e1 `6eac323`/`fc1acc5`.
During the gate the coordinator's only runtime worktree difference was two
text corrections in `CoreChangeReportRows.vue`: always describe observation
references as representative and give user-facing review guidance. Backend source
at the initial gate and archive creation was unchanged. A file-by-file SHA-256
manifest and that patch were captured before the gate in the local evidence
directory below. The coordinator subsequently committed the Web changes and
browser evidence as `1d3561075924f31502913e8203e4d924b8f7e923`; that commit changes
no backend source. The final backend difference is limited to the two test
corrections below.

The independent review was limited to the four integration seams, comparing both
parent implementations with the merged source:

- `crm-api/src/lib.rs` retains both routers, mobile generation cleanup, and the
  core-change worker.
- `crm-api/src/auth/workspace_http.rs` retains mobile's 20-second admission budget
  and applies `no-store` to both mobile and core-change responses, including
  upstream authorization denials.
- `crm-api/src/routes/mod.rs` exports both modules.
- `crm-api/tests/all.rs` registers both DB suites.

Both seam diffs are additive. No runtime corrections were made by this verifier.
An existing upgrade-test assertion and a reader-plan fixture were corrected after
observed failures and coordinator assignment, as described below. This is not
another broad implementation review.

## Environment and reproducible commands

Tools: Rust/Cargo 1.98.0; cargo-nextest 0.9.143; macOS aarch64. Existing healthy
local PostgreSQL and Centrifugo containers were used without resetting services.
Credentials were sourced privately from
`/private/tmp/crm-mobile001-qa/runtime.env`; no values are copied into this record.
The live schema check owned only the fresh `crm_sqlx_prepare_check` database and
dropped it afterward. SQLx tests use the assigned isolated migrator URL and create
their own test databases. Test concurrency is capped at eight, with no retries.

Local evidence directory:
`/private/tmp/crm-mobile-migration-combined-d6e7c74/`. It contains `check.log`,
`check-db.log`, `archive-run.log`, `run-db.sh`, `run-archive.sh`, the source
manifest/patch, both seam diffs, and the retained archive/extraction. The
`corrections-*` and `corrected-*` files record final Clippy, archive build,
two-test rerun, checksums and plans; `run-corrected.sh` pins its command.
It contains no copied env file.

Commands actually executed (the private environment is sourced only for the DB
steps; these are the standard `scripts/check-db` schema and test steps with an
immutable nextest archive substituted for the mutable build-directory runner):

```sh
NEXTEST_TEST_THREADS=8 SQLX_OFFLINE=true ./scripts/check

# backend/, with the assigned private environment loaded
prepare_check_url="${MIGRATION_DATABASE_URL%/*}/crm_sqlx_prepare_check"
cargo sqlx database create --database-url "$prepare_check_url"
cargo sqlx migrate run --source crates/crm-api/migrations --database-url "$prepare_check_url"
CARGO_TARGET_DIR="$repo_root/backend/target/sqlx-prepare-check" \
  DATABASE_URL="$prepare_check_url" SQLX_OFFLINE=false \
  cargo sqlx prepare --check --workspace
cargo sqlx database drop -y --database-url "$prepare_check_url"
export DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true
cargo nextest archive --workspace --locked --archive-file "$proof_dir/combined-tests.tar.zst"
mkdir -p "$proof_dir/extracted"
cargo nextest run --archive-file "$proof_dir/combined-tests.tar.zst" \
  --extract-to "$proof_dir/extracted" --workspace-remap "$repo_root/backend" \
  --run-ignored only --no-fail-fast --test-threads 8 --retries 0

# After the two test-only corrections; no full gate or SQLx repetition
rustfmt --edition 2021 --check \
  crates/crm-api/tests/db_activity_upgrade.rs crates/crm-api/tests/db_import_readers.rs
cargo clippy -p crm-api --test all --locked -- -D warnings
cargo nextest archive -p crm-api --test all --locked \
  --archive-file "$proof_dir/corrected-tests.tar.zst"
mkdir -p "$proof_dir/corrected-extracted"
cargo nextest run --archive-file "$proof_dir/corrected-tests.tar.zst" \
  --extract-to "$proof_dir/corrected-extracted" --workspace-remap "$repo_root/backend" \
  --run-ignored only --no-fail-fast --test-threads 2 --retries 0 \
  --success-output immediate -E 'test(db_activity_upgrade::) | test(db_import_readers::)'
```

Here `repo_root` is `/Users/karrad/projects/crm` and `proof_dir` is the evidence
directory above. The archive and extracted executables are outside every Cargo
target directory. Concurrent builds therefore cannot unlink this run's binaries.

## Results

`scripts/check`: **PASS**, exit 0, 196 seconds. Includes formatting, Clippy with
warnings denied, production compilation, resolved dependency fences, 34 preflight
Python tests, 968 Rust tests, five compile-fail doctests, Web lint/typecheck,
1,181 Web tests in 86 files, Web production build, and 11 email-worker tests.
Nextest marked the passing legacy
`health::ready_returns_503_without_database_url` test as leaky; no test failed.
The 966 ignored tests are intentionally excluded from this first gate.

The coordinator discovered that the existing development Web preview served the
default root `web/dist` output. This gate's background build had finished before
the coordinator restored the released assets. Further Web verification builds use
an isolated output directory. Restoration evidence belongs to the coordinator;
this build was not an authorized deployment.

Live SQLx schema check: **PASS**. Applied the complete combined migration set
to the fresh throwaway DB, checked the committed SQLx cache against it, and
dropped that DB. The subsequent archive was built from unchanged backend source
at `d6e7c747f6f24ccb13781aacd7d75dc3cd1b2e2c`. Its SHA-256 is
`d52a4cb02c996a7f01bc276ae0502ce581e6b580e1f195cd54a8a7fc7daed11c`.

The first archive launch exited 96 before running tests: nextest required the
explicit extraction directory to exist. Creating that empty external directory
fixed the harness setup; the same archive was used without rebuilding or
repeating SQLx. The failed launch is preserved in `check-db.log`.

Archived full DB suite: **completed**, nextest run
`a45e04bc-ddcb-4721-84ef-7acfd884e961`, 966 tests in 689.655 seconds: 964 passed,
two failed, exit 100. The slow 25,000-Person snapshot test passed. There were no
spawn errors, including during the subsequent Cargo rebuild. All five
`db_core_change_reports` and all seven `db_mobile` tests passed. The only two
failures are classified below and passed after their test-only corrections.

The run exposed a remaining assertion in
`db_activity_upgrade::populated_010f1_upgrade_preserves_native_and_import_state`.
Its explicit initial-revision checks, comparison of every old column, startup and
HTTP read assertions all passed; the final comparison still compared the full
upgraded rows to the old schema. The correction freezes the complete upgraded
rows before normalizing a clone for the old-field comparison, then compares the
post-read rows to that complete upgraded snapshot. This preserves checks on every
old and new field and changes no business behavior. The full original archive
continued without fail-fast, so this failure did not hide later regressions.

The second observed failure was the exact index-name assertion in
`db_import_readers::imported_contact_order_is_consistent_for_direct_readers_and_task_only`.
Its behavioral assertions passed. The fixture had 25,000 filler People and
100,000 contacts but only one task; PostgreSQL chose the new `mobile_task_page`
index, examined one matching task without removing any rows, and completed the
query in 0.18 ms. Both contact lookups still used their primary-history index and
returned one row. The captured plan is
`import-readers-task-only-plan.json` in the local evidence directory. With explicit
coordinator assignment, the fixture now adds 25,000 future open tasks for the same
Organization and assignee, then analyzes `task`. This makes the due-time predicate
selective. The original due-index and contact-bound assertions remain unchanged;
the correction adds no runtime change or planner switch.

Final affected checks: **PASS**. Formatting and `git diff --check` passed;
test-target Clippy passed in 23.54 seconds. The corrected archive SHA-256 is
`0c244d46f19a728cf700ae6559c5356a82a4e0430d6f2ed168fc9c96baad5bb9`.
Its source is the original backend plus `backend-corrections.patch`, SHA-256
`27be0c2eee635b38b7d36e02e6305be0b8c48714c3242c84a12b1b935b1bc59c`.
After the full suite ended, nextest run
`f429baa3-ff88-4a67-b52c-066d641fcf57` ran the two corrected tests: **two passed**,
exit 0, 16.024 seconds, with zero retries. The strengthened reader plan uses
`task_org_assignee_due_open_idx` with Organization, assignee and due-time index
conditions, one returned task, and one row from each indexed contact lookup;
execution was 0.15 ms (`corrected-task-only-plan.json`). The upgrade test passed
all old-field, initial-revision, startup, HTTP-read and complete post-read
preservation assertions.

Thus all 966 selected DB tests have passing evidence: 964 from the complete
combined run and the two affected tests from the corrected source. The original
run's failures remain recorded; no claim is made that its first invocation was
fully green. Production source and SQLx metadata did not change between these
runs, so the repository and live-schema passes remain applicable.

Earlier migration-only DB execution reached 401 passing tests before Cargo
removed its runner during a concurrent rebuild. Those spawn errors were harness
failures, not passing assertions, and that interrupted run did not establish a
complete suite pass. This combined immutable run replaces the need to repeat both
branches independently. Earlier Mobile 001 activity guard failures and the
legacy upgrade field-extension assertion were corrected before this source
revision. This combined verification confirms the trigger correction and
completes the remaining upgrade assertion correction described above.

This verification does not authorize live FUB/customer processing, activation,
app distribution, publishing branches, or deployment (D-074/D-075).

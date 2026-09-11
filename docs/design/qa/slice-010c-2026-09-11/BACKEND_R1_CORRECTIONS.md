# Slice 010c backend round 1 correction checkpoint

2026-09-11. Addendum to the immutable initial `BACKEND_CHECKPOINT.md`.
This is targeted correction evidence for the first bounded implementation review,
not an additional review round or final release/verification claim.

## Corrected findings

1. Stage labels and source-user emails containing NUL no longer enter optional
   PostgreSQL text suggestion queries. Their exact encrypted source evidence is
   retained. Qualified existing-stage/member choices remain executable decisions.
2. HTTP confirmation resolves the scoped, current-admin, actor/input-bound durable
   receipt before consulting current release evidence. Missing/expired reports
   still block genuinely new confirmation; altered inputs still conflict.
3. Shared import command and initial worker claim transactions install the same
   two-second row-lock timeout before membership/Organization locks. Failed waits
   roll back and release workspace locks without advancing lease or work admission.
4. Detailed command receipts size their own final retained-byte counters to a
   bounded fixed point before exact settlement. Cancellation returns/replays zero
   work/control reservations and the actual committed retained total, including
   the receipt itself; its previously reserved control capacity still pays for it.
5. Added the scoped mapping-field route under the owned import contract. It reads
   the exact retained `Mapping.source`, supports the existing complete field
   allowlist and UTF-8 segments, and has a distinct run/plan/mapping/field cursor
   purpose. It exposes no raw capture, source request or mutation.

## Targeted results

With the isolated provider-disabled worktree environment and
`DATABASE_URL="$MIGRATION_DATABASE_URL"` pointing only to the dedicated SQLx test
master, `SQLX_OFFLINE=true cargo test --manifest-path backend/Cargo.toml -p crm-api
--test all --features test-support --locked db_import_ -- --ignored --nocapture
--test-threads=1` passed **27/27**, 116.47 seconds test / 41.41 seconds build.

The suite now contains ten gate/accounting tests, six HTTP tests, ten source tests
and one reader test. Added proofs cover actual held membership/Organization row
waits and initial claim cleanup, both NUL source fields, exact stage/user field
inspection with 1–2 KiB UTF-8 source values, member/foreign denial, wrong endpoint/
field/mapping cursor rejection, a real current-executable/database preflight
report that expires and is removed, genuinely new confirmation denial, and exact
first/replayed cancel counters compared with persisted ledgers. Existing source
qualification, >2 MiB retention/admission, lease, cancellation, identity and indexed
contact reader checks remain passing. The real report fixture is synthetic and
private; it does not claim a production fleet was inspected or retired.

`SQLX_OFFLINE=true cargo clippy --manifest-path backend/Cargo.toml -p crm-api
--all-targets --features test-support --locked -- -D warnings` passed in 18.96s.
`cargo fmt --manifest-path backend/Cargo.toml --all --check` and `git diff --check`
passed. No schema or SQLx macro statement changed in this correction pass.

No Cargo/DB process remains active. The backend is refrozen for targeted reviewer
confirmation before Web. Comprehensive query-plan/performance evidence, Web and
final full gates remain the previously agreed Step 6 work; the unregistered
support-owned `db_import_plans.rs` is outside this correction scope. No live source,
provider, shared crm_dev/main runtime, Git or deployment action occurred.

## Correction source hashes

These eight entries replace their corresponding hashes in the initial 147-file
checkpoint. All other initial entries remain unchanged at this refreeze.

```text
c090b20c8454c6754c6881469a25e896dc098955c4cb170c2dd85911905234be  backend/crates/crm-app/src/auth/workspace.rs
8332b67487254ab5936160cc5f97801cfcc98a8ccb04dd3fd7119f88b59c7256  backend/crates/crm-app/src/domain/migration/imports.rs
48030277e087e37412919f264352bc1bdffb141dbc3fbdfd8aed7c0156b3ea5d  backend/crates/crm-app/src/domain/migration/import_worker.rs
a221e49cda8b888b60d5028816ef63bf4c100f2b07596ace77b7f12d0d71ef78  backend/crates/crm-api/src/routes/migration_imports.rs
eb678be88807956dbc72f6be926ee09862dc84efd60ea75fa08320bbfd2dd2bb  backend/crates/crm-api/tests/db_import_gate.rs
23662f5e00c9445e7b2e48b49caecdcb8352402046029d062d8d34e4a1036d90  backend/crates/crm-api/tests/db_import_source.rs
7b5c93098110a22584d2d1af9e75799b888be1b7adaf5062e532c5d7dc3439a1  backend/crates/crm-api/tests/db_import_http.rs
0c22f38b4db1e02554d71021a0bb0b01b417fbaea10a8dea851181fc425d3bfd  docs/specs/SLICE_010c_CONTRACT.md
```

## Retained log hashes

```text
0e756b165a4dc7d71c6be4bdc00d90c7b51ef471413824975f673e3998896d4b  /private/tmp/crm-010c-r1-import.log
28116c997a6fd60f49874ef5521c533f275a50ecd078d51bd839d7a458858c93  /private/tmp/crm-010c-r1-clippy.log
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  /private/tmp/crm-010c-r1-fmt.log
```

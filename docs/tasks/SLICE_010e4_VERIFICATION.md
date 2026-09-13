# Slice 010e4 — Implementation verification

**In progress — D-080.** This is isolated retained-synthetic evidence, not live
FUB qualification, activation or deployment. Evidence files are under
`/private/tmp/crm-mobile004-010e4/`. Final review, browser acceptance, performance
and combined gates must be completed before closing the slice.

## Retained-source execution

The execution fixture drives a real confirmed 010c import, a real 010e3 admission
and a later retained 010e1 report. Business lifecycle cases do not fabricate a
successful admission. Synthetic concurrent native edits and fault injection are
explicit test actions.

Checkpoint `dd97b6c`, integrated at `0692e8f`, adds closed settlement outcomes,
distinct missing/untrustworthy observation handling, immutable provenance-byte
accounting, cohort-only sealing and the required sparse/cohort indexes in additive
schema versions `20260930000004` and `00005`.

The grouped execution run passed eight of nine cases in 75.25 seconds. The
remaining catalog-abuse assertion expected workspace error `P010C`, while the
database denied the write earlier with privilege error `42501`. The corrected
focused permit test passed. Retain the original failure and corrected evidence:
`migration/admitted-people-refresh-execution.log` and `migration/permit-fence.log`.
The retained-observation case also passed independently in 14.04 seconds
(`migration/retained-observation.log`). An earlier manually assembled malformed
fixture was replaced by actual duplicate retained observations; it is not claimed
as passing acceptance.

Covered behavior includes completed and partially cancelled admission cohorts;
untouched original, unsettled and sibling records; same-contact distinct People;
successive refresh baselines; local and replaced-contact holds; mixed eligible,
already-current, missing and held outcomes; exact-boundary remainder; source
identity mismatch; real item/lease permits denying off-target and extra fields,
notes/tasks/catalog/unrelated facts and ordinary mobile writes; and atomic
stage-revision/fact/result/baseline rollback.

## Read and recovery contracts

The coordinator's `0e7e1a1` test build passed both new read/recovery SQLx cases in
13.03 seconds (`integration/recovery-db.log`). Each used a disposable database,
the migrator connection and serial execution. Coverage includes preparation
pause/retry before confirmation, immutable-plan re-preview independent of lifecycle
revision, exact replay and stale revision rejection, admission-scoped list/detail
and no-store, exact eligible-count confirmation, execution pause/retry, terminal
cancel rejection, and truthful cancelled/settled filtered item displays.

The compile passed with one macOS linker warning about the large test binary's
unwind table (`integration/recovery-build.log`); it was not a Rust semantic error.
Final-tree gates remain separate from these checkpoint tests.

Server-qualified selected-report availability is integrated from `18aa9a4` and
`6d8b806`. It shares preparation qualification, holds current admin/Organization
authority through its bounded read transaction, validates sealed report evidence,
returns opaque foreign-resource errors and fences the Web selection by its exact
request identity. The owner reports two focused DB/API passes after correcting
the test connection from the application URL to `MIGRATION_DATABASE_URL`.
These lane outputs exist only in the task's tool transcript, not retained log
files: `cargo check -p crm-app -p crm-api`, the API test `--no-run`, Web Vitest
92 files/1,238 tests, the initial `42501` setup failure and the corrected two-test
DB/API pass. No raw log path is asserted for them; final gates will retain logs.

## Review and remaining evidence

First independent implementation review identified lifecycle/read, settlement,
storage and availability gaps; corrections and focused tests are being integrated.
The copied report-group traversal was unreachable. Enabling it would contradict
spec §3's exact successful-admission cohort, so the implementation removes it and
seals after walking all successful cohort identities. Original/report-only/sibling
records remain outside this refresh population.

The browser harness binds isolated API3103 and production Web5174 to
`crm_010e4_qa`, with explicit test keys/readiness and a retained synthetic reader.
It uses production routes/commands and bounded worker scheduling. It is not
production startup or workload-compatibility evidence. Its first actual preview
exposed admission-list/overview defects, now corrected in source. Confirmation,
partial cancellation, remainder, provenance, preservation and 390px acceptance
are still pending.

Exact physical accounting must include source keys/IDs, checkpoint and provenance
as well as encrypted plan/contact/result/baseline/receipt envelopes. The final
audit will distinguish immutable snapshot evidence from legitimate shared
snapshot/Organization ledger changes; it will not label those ledger deltas as
source corruption or omit them from reconciliation.

Accounting checkpoint `9601962` and coordinator `2936f54` include these fields,
signed checkpoint deltas and an additive full-footprint reconciliation in
`20260930000006`. The 51-Person checkpoint accounting case passed in 7.52 seconds
(`migration/checkpoint-physical-ledger.log`). The existing browser fixture was
upgraded through `00004`–`00006` using the normal SQLx migrator: its refresh and
plan IDs remained unchanged, state stayed `ready` and settled count stayed zero.
Every normalized source/native fingerprint remained identical. Exactly 27 bytes
were added to each corresponding run/snapshot/Organization ledger; reservations
were unchanged. Evidence: `migration/retained-preview-ledger-upgrade.log`,
`ledger-before.json`, `ledger-after-upgrade.json`,
`preservation-before-normalized.jsonl` and
`preservation-after-ledger-upgrade.jsonl` in the migration evidence directory.
Normalization excludes only legitimate snapshot byte counters and audits the
Organization ledger separately; it preserves all source evidence columns.

The second/final review identified two additional corrections in progress:
preparation must not load 50 potentially large payloads in one work unit, and a
mapping target removed after confirmation must settle as held instead of raising
an unhandled missing-row error. Focused bounded-input and mapping-race regressions
are required before final disposition.

Remaining: physical accounting correction and retained-preview upgrade; final
integrated browser and row/byte reconciliation; one 25k hot-plan and paired Person
read pass; second/final independent review; repository, SQLx and DB gates.

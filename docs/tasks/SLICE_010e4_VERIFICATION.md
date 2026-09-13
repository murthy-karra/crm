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

The second/final review identified bounded preparation, exact mapping and natural
identity/baseline proof, missing targets and commit-time source/target validation
for every baseline-advancing item. Corrections are being integrated within this
same review round. Remaining targeted repairs distinguish prior settled B from
obsolete admission envelopes, missing source instructions from explicit mapping
choices, and per-Person evidence holds from cryptographic recovery pauses.

Additive `00007`–`00009` bind mapping IDs, source account and semantic HMAC;
`00010` reconciles the previously omitted receipt digest bytes. Their actual saved
QA upgrade and final physical audit remain pending. A focused stage run at an
intermediate snapshot failed in 4.71 seconds because a new SQL INT4 expression was
decoded as BIGINT; the explicit SQL cast is corrected in `82022fd`, and the affected
execution tests must rerun (`migration/final-focused-stage.log`).

Coordinator readiness fixes `ddf0027`/`775e63a` require every new proof column with
its exact type/nullability in both runtime and release preflight. At integrated
`932e71e`, the disposable-database regression passed in 1.46 seconds: complete
schema succeeds; each missing proof column and incompatible type/nullability
fails at startup and confirmation; rollback restores readiness. Evidence:
`integration/readiness-db.log`. The 48 preflight tests passed at `ddf0027`
(`integration/readiness-preflight.log`). The candidate test compile passed in
2m04s (`integration/evidence-candidate-build.log`), with the already recorded
large-test-binary unwind warning. This is not final-tree execution evidence.

Remaining: final targeted worker regressions and review disposition; integrated
browser and row/byte reconciliation; one 25k hot-plan and paired Person read pass;
repository, SQLx and DB gates.


## Reviewed browser and proof evidence

The second/final independent review is **code READY at `872e896`, contingent on
verification**. No third review was opened. The coordinator's reviewed test build
passed in 2m02s (`integration/reviewed-candidate-build.log`); its copied executable
and SHA are `migration/api-bin/admitted-ui-reviewed` and
`migration/reviewed-api.sha256`. Production refresh code is `872e896`; subsequent
lane changes through `c5e1f9c` add/correct test fixtures. The eight proof/read/
availability cases passed in 45.67 seconds (`integration/final-proof-read-db.log`),
including mixed already-current native/identity/source loss, initial identity holds
without B, omitted instructions across successive B, recovery and exact schema
readiness. Later test additions for numeric primary order and immutable admission
fact/provenance bytes remain part of the final DB gate.

The existing saved QA fixture was normally migrated through `00007`–`00010`.
Its ready preview and settled count remained unchanged; 108 table fingerprints
matched. The receipt digest added exactly 32 bytes to the feature/snapshot/
Organization ledgers, with reservations unchanged. Evidence:
`migration/retained-preview-proof-upgrade.log`, `proof-upgrade-verification.json`
and `physical-after-proof-upgrade.jsonl`.

Actual production-Web/API browser acceptance passed at desktop and 390px:
re-preview, paged comparison, exact confirmation, one business update, cancellation,
exact-report remainder, completion, settled/held filters, original admission
provenance and return navigation. Every successful traversal had zero page errors
and document width equal to the 390px viewport. The original ready resource
`95757f18-78a2-4c64-a919-9141f5a6596d` is now cancelled with two outcomes (one
business update and one local hold); remainder
`be9bd2be-7c35-4abf-96c1-511a1973f477` completed with one update, one verified no-op
and one local hold. Exactly two People were updated across both resources.

Evidence: `migration/browser-repreview-reviewed.log`, `browser-confirm-first.log`,
`browser-cancel-reviewed.log`, `browser-remainder-reviewed.log`,
`browser-confirm-remainder.log`, `browser-completed-reviewed.log`,
`browser-provenance-canonical.log` and `worker-step-trace.jsonl`. The first
provenance-link check exposed an incorrect `/migration` redirect that discarded
its fragment. `53f60c4` uses the canonical named route; the actual browser retest
passed. Earlier failures remain in `browser-provenance-reviewed.log` and
`browser-provenance-final.log`.

Exact reconciliation (`migration/final-browser-reconciliation.json`,
`physical-final.jsonl`, `native-final.jsonl`, `preservation-final.jsonl`) verifies
69,791 and 95,309 physical/recorded bytes for the two resources, zero reservations,
and an equal 139,447-byte increase in feature/snapshot/Organization ledgers since
the proof-column upgrade. All 108 compared table fingerprints reconcile. Snapshot
byte counters are audited separately. The existing history read-model revisions
increase from the authorized Person/contact writes; their exact increments are
reconstructed from settled contact rows using the existing triggers, preserving
all counters and source history payloads (`history-read-model-reconciliation.sql`
and `.json`). They are not silently discarded from the preservation comparison.

Person106 has 57 ordered contacts; Person107 retains its distinct shared address;
Person108 retains its local name and no contacts. The retained browser fixture
used Boolean `isPrimary`, which the existing numeric-only source contract marks
unqualified and therefore preserves source order: contact1 is primary here.
The earlier expectation of contact57 was a fixture-audit error, not a passing
numeric-primary claim. Numeric `0`/`1` primary selection and later omitted-contact
preservation are covered by the expanded commit-proof regression in the final
DB gate. No retained fixture payload was rewritten to change this outcome.

The first repository gate stopped on `large_enum_variant` and a test module placed
before production items (`integration/final-check.log`). `53f60c4` boxes the internal
source record and moves the digest test module; the corrected gate is running.
Final focused lane cases, D-050 performance, repository, SQLx and DB gates remain.

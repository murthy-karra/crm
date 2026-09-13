# Slice 010e4 — Implementation verification

**COMPLETE — D-080, 2026-09-13.** Implementation, both independent review rounds,
focused regressions, browser acceptance, performance and combined gates pass.
Evidence is isolated and retained synthetic; it does not qualify live FUB,
activation or deployment. Paths below are relative to
`/private/tmp/crm-mobile004-010e4/` unless stated otherwise.

## Implementation and review

The execution fixture drives a real confirmed 010c import, real 010e3 admission
and later retained 010e1 report. Successful admission results define the exact
cohort, including committed results of cancelled admissions. Original People,
unsettled admissions and sibling cohorts remain outside it. Synthetic concurrent
native edits and fault injection are explicit test actions.

The second/final independent review is **code READY at `872e896`, contingent on
verification**. Both allowed review rounds are used; no third review was opened.
Corrections cover closed outcomes, lifecycle/read recovery, bounded preparation,
physical accounting, selected-report availability, exact mapping/natural identity/
baseline proof, missing targets and source revalidation for every item that can
advance a baseline. Successor B comes from its settled feature baseline; absent
initial identity is held without creating an authoritative baseline. Omitted
instructions preserve the applicable baseline instead of inventing a mapping.

The coordinator's reviewed test build passed in 2m02s
(`integration/reviewed-candidate-build.log`). The copied browser executable and
SHA are `migration/api-bin/admitted-ui-reviewed` and `migration/reviewed-api.sha256`.
Its production refresh code is `872e896`; subsequent production changes box an
internal record for Clippy and repair canonical Web navigation. Later lane commits
through `8aa0e62` correct or extend regression fixtures.

## Correctness and recovery

Eight integrated proof/read/availability cases passed in 45.67 seconds on the
reviewed executable (`integration/final-proof-read-db.log`). They cover mixed
already-current native/identity/source loss, initial identity holds without B,
successive omitted instructions, preparation and execution pause/retry, immutable
re-preview, actor/request replay, exact confirmation counts, no-store tenant reads,
terminal lifecycle errors, filtered results and schema readiness. Every new proof
column is checked for required type/nullability at startup and confirmation.
The preflight suite has 48 passing cases.

Execution cases cover complete and partially cancelled cohorts, same-contact
separate People, successive baselines, local and replaced-contact holds, mixed
settled/no-op/missing/held outcomes, exact-boundary remainder, source mismatch,
item/lease/Organization permits, ordinary mobile denial during review, and atomic
stage-revision/fact/result/baseline rollback. The last three focused lane cases
passed: missing/deactivated selected target (9.28s,
`migration/final-mapping-missing-c5e1f9c.log`), >8MiB live-current projection held
without mutation/result (12.17s, `migration/final-large-live-passed.log`), and a
same-target sibling mapping cannot replace the selected mapping (4.50s,
`migration/final-sibling-c5e1f9c.log`). The last lane source is `8aa0e62`.
Numeric primary `0`/`1`, later omitted-contact preservation and exact immutable
`person_admitted`/`person_admission_provenance` byte checks pass in the full DB gate.

Earlier checkpoints retain useful failure evidence, not final-tree substitutes:

- `migration/admitted-people-refresh-execution.log`: eight of nine passed; the
  catalog-abuse assertion expected `P010C` but was safely denied earlier with
  `42501`. Corrected `migration/permit-fence.log` passed. Retained-observation
  and 51-Person physical/checkpoint cases passed in 14.04s and 7.52s
  (`retained-observation.log`, `checkpoint-physical-ledger.log`).
- `integration/recovery-db.log`: two read/recovery cases passed in 13.03s at
  `0e7e1a1`; the later eight-case run supersedes this checkpoint. Availability's
  owner corrected an application-URL `42501` setup error and passed two DB/API
  tests. Those owner outputs exist in the task transcript, not invented log paths.
- `migration/final-focused-stage.log`: INT4/BIGINT decoding failed before the
  explicit cast fix. Later execution evidence includes this correction.
- `migration/final-focused-f2ecf6e.log`: a 9MiB source-name fixture failed existing
  upstream projection limits. It was removed, without relaxing source limits.
  `final-large-live-c5e1f9c.log` then exposed PostgreSQL's indexed-row limit;
  the corrected local fixture uses bounded contact rows. `final-large-live-corrected.log`
  attempted confirmation of an all-held preview; the final test correctly verifies
  ready/held preservation without confirmation. A separate fixture accidentally
  deactivated the initiating actor; the corrected selected-target case passes.

## Browser acceptance and preservation

The isolated retained QA database `crm_010e4_qa` was upgraded using the normal SQLx
migrator through additive migrations `20260930000004`–`00010`. Applied migrations
were not edited. The saved preview and settled count were preserved. The first
ledger upgrade added exactly 27 bytes; the later receipt-digest upgrade added
exactly 32 bytes to each feature/snapshot/Organization ledger, with reservations
unchanged and all 108 normalized table fingerprints identical. Evidence:
`migration/retained-preview-ledger-upgrade.log`, `ledger-before.json`,
`ledger-after-upgrade.json`, `retained-preview-proof-upgrade.log`,
`proof-upgrade-verification.json` and `physical-after-proof-upgrade.jsonl`.

Actual production-Web/API acceptance passed at desktop and 390px: re-preview,
57-contact paged comparison, exact confirmation, one business update, cancellation,
exact-report remainder, completion, settled/held filters, original admission
provenance and return navigation. Successful traversals had zero page errors and
no document-width overflow. The original resource
`95757f18-78a2-4c64-a919-9141f5a6596d` is cancelled with two outcomes (one update,
one local hold); remainder `be9bd2be-7c35-4abf-96c1-511a1973f477` completed with
one update, one verified no-op and one local hold. Exactly two People were updated.

Logs in `migration/`: `browser-repreview-reviewed.log`, `browser-confirm-first.log`,
`browser-cancel-reviewed.log`, `browser-remainder-reviewed.log`,
`browser-confirm-remainder.log`, `browser-completed-reviewed.log`,
`browser-provenance-canonical.log`, `browser-provenance-viewport.log` and
`worker-step-trace.jsonl`. Inspected screenshots include
`mobile-repreview-comparison.png`, `mobile-completed-viewport.png` and
`mobile-original-admission-provenance.png`. The first preview exposed list/overview
contract defects, corrected before confirmation. Provenance navigation initially
lost its fragment through the `/migration` redirect; `53f60c4` uses the canonical
named route. Both failed attempts remain in `browser-provenance-reviewed.log` and
`browser-provenance-final.log`; actual browser retests pass. The owned migration
API was stopped after acceptance, before performance measurement.

`migration/final-browser-reconciliation.json`, `physical-final.jsonl`,
`native-final.jsonl` and `preservation-final.jsonl` verify exact recorded/physical
sizes of 69,791 and 95,309 bytes, zero reservations, and the same 139,447-byte
increase in feature/snapshot/Organization ledgers since the proof upgrade.
All 108 compared table fingerprints reconcile. Snapshot byte counters are audited
separately. Authorized native writes increment the existing history read-model
revisions; `history-read-model-reconciliation.sql` and `.json` derive those exact
increments from settled contact rows and existing triggers. No history payload
or revision discrepancy is silently discarded.

Person106 has 57 ordered contacts; Person107 retains its separate shared address;
Person108 retains its local name and no contacts. The browser fixture's Boolean
`isPrimary` is unqualified under the existing numeric-only contract, which preserves
source order: contact1 is primary. The earlier contact57 expectation was an audit
fixture error. No retained payload was rewritten; numeric primary behavior is
verified separately in the expanded database regression.

## Final gates

At `a58d55e`, `scripts/check` passed in 25s: 48 preflight, 989 ordinary Rust,
five compile-fail documentation, 1,238 Web and 11 email-worker tests, plus format,
Clippy, production compile, dependency fences, Web lint/typecheck and isolated
production build (`integration/final-check-passed.log`). Existing large-test-binary
unwind and Web chunk-size warnings are retained and nonfatal.

Failed gate attempts are retained: `integration/final-check.log` (large enum and
module ordering, fixed in `53f60c4`), `final-check-corrected.log` (Vue attribute
order, fixed in `0f66318`), and `final-check-complete.log` (four unhandled router
errors despite all Web assertions passing). `a58d55e` registers the canonical
route in test fixtures and asserts its destination; the complete gate then passes.

## D-050 performance

Both gates ran once, serially without the migration worker or another DB gate,
using the `perf-harness` executable built from `a58d55e`. Build log:
`integration/final-perf-build.log`; SHA:
`4d08912e27cd06f058ec7f85b23db0e9525fd628683e596308fd088c4669082f`.

`db_admitted_refresh_perf::admitted_refresh_25k_hot_plans` passed in 34.46s
(`migration/final-hot-plans.log`, `hot-plans.json`). A real retained admission/
preview supplies roots; 25k synthetic relational-volume People and 50 active
members supply scale. Those added rows are inert, not source qualification or
worker-throughput evidence. SQL is extracted from current production statements.
All six plans used the required bounded indexes: cohort/result/item rows examined
were 1/25/at most 51, and claim examined 1. Execution times (ms): cohort 0.082,
all 0.073, sparse 0.180, results 0.075, cancelled 4.236, claim 0.139.

`db_activity_person_detail_perf::operational_person_detail_matches_cd3b010` passed
in 73.38s (`migration/final-person-paired.log`,
`person-paired/person-detail-paired.json`). The 25k-Person/50-member fixture had
unchanged counts; both ordinary HTTP arms had complete byte/JSON equality and
verified entry points. Five warmups and 40 measured requests per arm alternated
AB/BA with concurrency 1. Baseline/current p95 was **16.33/20.60ms**, below the
**41.33ms** limit (baseline plus max(25ms, 10%)). All observations were exact.
The frozen helper is attributed to `cd3b010`, not relabeled as the published
baseline. `migration/person-read-source-attribution.json` separately proves that
the five current ordinary reader bodies are unchanged from published `44dcf52`.
This is a local paired budget pass, not a maximum-capacity claim.

## SQLx and final database gate

`scripts/sqlx-prepare` passed (70s, `integration/final-sqlx-prepare.log`),
removing one obsolete pre-stage-revision Person-lock cache entry in `c7d2720`.
Fresh-schema `sqlx prepare --check` also passed. Its production-target check
warns about queries retained for test targets by the all-targets cache generation.

The first full database gate stopped after 29 passes and one historical-fixture
failure (`integration/final-check-db.log`): current note/task commands use the
shared Person lock, which requires `stage_revision` absent from the old schema.
`7930b83` seeds valid historical note/task rows directly and verifies exact new
stage/catalog revision defaults alongside complete pre-existing-row preservation.
No production code or applied migration changed. `scripts/check` passed again
in 182s at `7930b83` (`integration/final-check-upgrade-fixture.log`).

The corrected full `scripts/check-db` passed at `7930b83`: **1,034/1,034 DB tests**
in 1,023.375s, 1,161s including fresh-schema/cache verification and rebuild
(`integration/final-check-db-upgrade-fixture.log`). Four test processes ran at a
time, each with its own disposable database; `DATABASE_URL` was the private
migrator connection. This includes all new migration/mobile cases, the historical
upgrade correction, tenant isolation, permits, failure paths and workspace gates.
No failures were retried or ignored in this successful run. The 989 non-DB tests
are intentionally outside this DB-only invocation and passed in `scripts/check`.
Subsequent changes are handoff documentation only.

Cargo/Web outputs stayed separate from shared development and all DB gates were
serialized. Completed writer worktrees and the migration API/Web preview were
closed; evidence, QA databases and the native QA API were preserved. Physical
phones, dependent-family imports, live source qualification, activation and
release remain outside this implementation scope.

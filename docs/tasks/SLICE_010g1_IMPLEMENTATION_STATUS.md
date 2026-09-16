# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- User requested “commit and proceed”; foundation checkpoint **`8b5959e`** is
  committed; accounting checkpoint **`0ef822f`** is also committed. History
  correction storage is committed as **`047e3e5`**; native delta planning is
  committed as **`cbae4c6`**. Cohort preparation is committed as **`b949ffe`**. The combined feature is unfinished. No merge/push/deploy.
- Foundation: comparison policies, source ordering, bounded encrypted evidence,
  exact decimal counts, draft persistence ownership and native/lease guards.
- Accounting now measures family and shared evidence separately. One frozen plan
  pays shared costs; original head storage ownership survives head replacement.
  Core-derived charges reach the selected snapshot and Organization ledgers;
  history plans use their independent plan and Organization budgets.
- Reservation/settlement functions preserve cancellation capacity, reject stale
  unit leases, and atomically charge/refund capacity. Deferred checks reject an
  application commit with unsettled evidence or inconsistent reservations.
- History storage now has three typed correction tables, immutable first-owner
  bootstrap, a current head, encrypted deletable displays, and date-bucket-aware
  counts/erasure. All five tables remain application SELECT-only pending the
  version-aware reader and execution-admission stage.
- The retained-history adapter reuses existing identity/canonical HMAC purposes;
  body-only changes create different semantics with unchanged metadata. Displays
  use a separate, version-bound AEAD purpose and a 4 KiB plaintext limit.
- Native delta planning now builds atomic metadata and note/task update
  proposals. Metadata retains ownership separately from equality, preserves local
  links, checks complete alias absence, reports source gaps, and counts qualified
  clears/removals. Activity preserves immutable fields and native local state,
  validates role mappings, and counts completion/reopen without inventing actors.
  These are pure preparation components; persistence execution is not wired.
- Cohort preparation now has a bounded transactional page runner. It freezes
  original/admitted/recovered identity proofs behind a workspace boundary, records
  exclusions, fences current admin/lease ownership, and settles exact byte charges
  with the checkpoint. Database guards also enforce the frozen identity/terminal
  boundary and reject application cohort/progress writes without a live payer
  claim. Dispatcher integration remains pending.
- Core capture indexing now retains authenticated page/cursor evidence and every
  source occurrence before Person filtering, with scoped raw references, lease
  fences and atomic accounting/checkpoints. It reuses the existing metadata and
  activity parsers and performs no source calls. Dispatcher/classification and
  plan-revision reuse remain unwired; history indexing is still pending.
- New positively applied original/admitted activity results retain exact encrypted
  native after-state/revision. A first-result discovery adapter checks frozen
  cohort/identity/manifest/result ownership and current equality/revision; missing
  legacy evidence holds. Metadata and previous-refresh baseline adapters remain.
- No refresh HTTP commands, family worker or Web workflow are exposed yet.

## Evidence and isolation

- Foundation: 12 focused Rust tests and 2 database regressions passed; fresh schema,
  direct retained-size check and formatting passed. Initial syntax/hash failures
  were corrected. This is targeted foundation evidence, not final package gates.
- Accounting: all 3 strengthened database regressions passed, including the
  deferred application-commit guard, expired-lease takeover, exact-once refunds,
  snapshot/Organization balances and catalog-driven byte inventory. Fresh schema
  application and formatting also passed. Logs:
  `/private/tmp/010g1-accounting-db-tests.log` and
  `/private/tmp/010g1-accounting-schema.log`.
- History: all 14 focused Rust tests and 6 database regressions passed; fresh
  schema, formatting and diff checks passed. The Rust tests cover namespace
  continuity, body-only semantic
  changes, metadata privacy and version-bound encryption. Database storage tests
  exercise two successive corrections for all three fact types, preserved first
  ownership, invalid head/Person writes, immutable facts, known/unknown counts,
  suppression, and exact erasure refunds across completed plans. Fixture failures
  (reinstalling the permanent marker and settling before terminal-state changes)
  were corrected; they are not ignored checks. Final logs:
  `/private/tmp/010g1-history-policy-tests.log`,
  `/private/tmp/010g1-history-db-tests.log`,
  `/private/tmp/010g1-history-legacy-tests.log` and
  `/private/tmp/010g1-history-schema.log`. These are targeted storage/adapter
  checks, not evidence of a working refresh HTTP/worker/Web flow.
- Native delta planning: all 28 focused family tests passed, including 8 new
  metadata and 6 new activity tests. Checks cover ownership, alias conflicts,
  missing/null/empty data, serialization, local edits/ABA, capacity, role mappings,
  task completion/reopen, and immutable fields. Clippy (`crm-app --lib`, warnings
  denied), formatting, and diff checks passed. The existing database 20-tag limit
  and idempotent-reapplication regression passed. An intermediate test-helper
  signature compile failure was corrected and rerun successfully. Logs:
  `/private/tmp/010g1-native-delta-tests.log`,
  `/private/tmp/010g1-native-delta-clippy.log` and
  `/private/tmp/010g1-native-tag-regression.log`. No refresh execution or UI claim.
- Cohort preparation: all 6 family database regressions passed, including
  application-role paging, admission/recovery proof selection, terminal cutoff,
  wrong Organization/token rejection, capacity rollback, injected checkpoint
  failure rollback, replay and exact metering. The initial fixture-count assertion
  was corrected (one original plus one admitted Person). Migration application,
  `crm-app` Clippy with warnings denied, formatting and diff checks passed. Logs:
  `/private/tmp/010g1-cohort-db-tests.log`,
  `/private/tmp/010g1-cohort-clippy.log`, and
  `/private/tmp/010g1-cohort-schema.log`. Full workflow gates remain outstanding.
- Cohort database fences: the strengthened 6-test family suite passed, including
  direct application inserts and progress updates without lease context. Migration
  application, formatting and diff checks passed. Logs:
  `/private/tmp/010g1-cohort-fences-db-tests.log` and
  `/private/tmp/010g1-cohort-fences-schema.log`.
- Core indexing/activity baseline checkpoint: all 9 family database tests, the
  existing original activity fidelity regression (plain/HTML long notes and task
  date/role policy), and all 29 focused Rust tests passed. Tests cover authenticated
  pagination, conflicting Person occurrences, inaccessible detail, corrupted source
  HMAC, injected checkpoint rollback, replay, exact byte inventory/settlement,
  original/admitted baseline discovery and wrong-cohort rejection. Fresh schema,
  `crm-app --lib` Clippy with warnings denied, formatting and diff checks passed.
  Initial request serialization/trigger-record-shape errors and synthetic fixture
  assumptions were corrected and rerun. Logs:
  `/private/tmp/010g1-core-index-db-tests.log`,
  `/private/tmp/010g1-after-state-original-regression.log`,
  `/private/tmp/010g1-index-baseline-unit.log`,
  `/private/tmp/010g1-index-baseline-clippy.log`, and
  `/private/tmp/010g1-core-index-schema.log`. No end-to-end refresh claim.
- Rust target `/private/tmp/crm-010g1-target-20260915`; intended Web output
  `/private/tmp/crm-010g1-web-dist-20260915`; synthetic DB
  `crm_010g1_schema_20260915`. Database tests run serially.
- Shared 010e5 runtime, native stores, local 010e6 and release artifacts preserved.
  No live FUB/customer processing. Linker reports the large `__eh_frame` warning.

Remaining: final capability inventories and refresh-owned initial identities;
history source indexing, index plan-revision reuse, legacy/metadata/prior-refresh
baselines; cohort/index dispatcher integration; metadata/activity/history execution; typed
commands and bounded readers; common Web workflow; independent implementation
review; full database/browser/performance/final gates. Do not report 010g1 done.

# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- User requested “commit and proceed”; foundation checkpoint **`8b5959e`** is
  committed; accounting checkpoint **`0ef822f`** is also committed. History
  correction storage is committed with this checkpoint; the combined feature is
  unfinished. No merge/push/deploy.
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
- Rust target `/private/tmp/crm-010g1-target-20260915`; intended Web output
  `/private/tmp/crm-010g1-web-dist-20260915`; synthetic DB
  `crm_010g1_schema_20260915`. Database tests run serially.
- Shared 010e5 runtime, native stores, local 010e6 and release artifacts preserved.
  No live FUB/customer processing. Linker reports the large `__eh_frame` warning.

Remaining: final capability inventories and refresh-owned initial identities;
source/cohort/baseline discovery; metadata/activity/history execution; typed
commands and bounded readers; common Web workflow; independent implementation
review; full database/browser/performance/final gates. Do not report 010g1 done.

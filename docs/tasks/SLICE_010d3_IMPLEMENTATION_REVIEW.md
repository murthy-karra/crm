# Slice 010d3 implementation review

## Round 1 — source review complete; final-gate evidence remains required

Reviewed the current uncommitted Slice010d3 implementation against D-086,
[the accepted specification](../specs/SLICE_010d3.md),
[the frozen contract](SLICE_010d3_CONTRACT.md),
[the implementation brief](SLICE_010d3_IMPL.md), the accepted planning-review
reader correction, D-015, and D-050. This was a read-only source and existing-log
review. I did not start a database, run a build, run tests, or drive the Web UI.

### Verdict

I found **no remaining source-level blocker** in the reviewed 010d3 envelope.
The implementation is suitable to enter the required final gates, but it is
**not yet safe to declare accepted or release-ready** because the required
final runtime, current preflight, Web journey, and D-050 evidence is incomplete.

### Reviewed implementation

- The admitted-history source binds one terminal successful admission cohort to
  one retained, same-parent/account completed-with-gaps capture; preparation
  reads retained authenticated capture data and freezes the binding rather than
  starting source work ([admitted_history_source.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/migration/admitted_history_source.rs:9),
  [admitted_history.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/migration/admitted_history.rs:96)).
- The additive migration keeps the global history identity and fact relations,
  adds mutually exclusive admitted owner tuples, and applies an independent
  `crm.admitted_history_reader` predicate beside the original history reader
  ([20261005000001_fub_admitted_history.sql](/Users/karrad/projects/crm/backend/crates/crm-api/migrations/20261005000001_fub_admitted_history.sql:114),
  [20261005000001_fub_admitted_history.sql](/Users/karrad/projects/crm/backend/crates/crm-api/migrations/20261005000001_fub_admitted_history.sql:149)).
- The worker reacquires the shared workspace guard on its actual unit
  transaction, revalidates the binding/lease, uses a per-manifest private
  permit, and settles fact/display/identity/result work in that transaction
  ([admitted_history_worker.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/migration/admitted_history_worker.rs:227),
  [admitted_history_worker.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/migration/admitted_history_worker.rs:699)).
- The HTTP routes are strict-body, no-store admin routes, and the bounded query
  readers use authenticated scope-bound cursors
  ([admitted_history_imports.rs](/Users/karrad/projects/crm/backend/crates/crm-api/src/routes/admitted_history_imports.rs:45),
  [admitted_history_queries.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/migration/admitted_history_queries.rs:66)).
- The Web panel keeps request identity/scope checks, ignores late results,
  polls only active roots, and presents metadata-only pagination and lifecycle
  controls ([AdmittedHistoryImportPanel.vue](/Users/karrad/projects/crm/web/src/components/migration/AdmittedHistoryImportPanel.vue:26),
  [AdmittedHistoryImportPanel.vue](/Users/karrad/projects/crm/web/src/components/migration/AdmittedHistoryImportPanel.vue:69)).

### Required final-gate evidence

These are evidence gaps, not new implementation defects:

1. Run the complete current-tree migration/authority/accounting/replay/reader/
   erasure suite and retain the final logs. The existing isolated run proves four
   focused admitted-history cases only; it reports 1,214 filtered tests at
   [/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/h3-runtime-current4.log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/h3-runtime-current4.log).
2. Re-run the release-preflight unit suite on the current tree. The status record
   says its 51/51 result predates the two admitted-history-specific cases
   ([MOBILE_007_010d3_IMPLEMENTATION_STATUS.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md:38)).
3. Supply D-050's required paired p95 comparison and realistic
   `EXPLAIN (ANALYZE, BUFFERS)` evidence for the changed review queries; no such
   current evidence was available to this review.
4. Execute the real API/Web administrator journey at desktop and 390px through
   preview, held/coverage acknowledgement, confirm, progress, cancel, Resume,
   and exact remainder. The supplied Web log is a 1,301-test Vitest pass, not an
   interactive API journey ([/private/tmp/crm-010d3-web-vitest.log](/private/tmp/crm-010d3-web-vitest.log)).

### Existing evidence inspected

The supplied current isolated runtime log reports the four focused admitted-history
tests passing, including cancellation/remainder, exact replay, source authority,
and independent reader stamps. The matching build log shows the current focused
test binary compiled successfully; it has a linker compact-unwind warning only
([h3-runtime-build4.log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/h3-runtime-build4.log)).
Those results support continued final-gate work but do not replace the gaps above.

## Authored evidence closure — 2026-09-15

This closes the four evidence requests above; it is not another source-review
round. The [integrated verification record](MOBILE_007_010d3_FINAL_VERIFICATION.md)
identifies the frozen backend sources and final runners. All original 1,111 DB
cases have passing results across the initial run and documented targeted
recovery, with two additional migration cases covered by the final focused run.
The initial timing-test failure remains recorded. Current preflight passes 53/53.

The [performance record](MOBILE_007_010d3_PERFORMANCE.md) is READY, including
bounded query plans and complete paired Person/Today parity and latency checks.
The [Web journey](SLICE_010d3_WEB_EVIDENCE.md) passes the actual desktop/390px
preview, held acknowledgement, confirm, progress, cancellation, Resume and exact
remainder flows. The migration lane's requested final evidence is complete; the
paired implementation's remaining native gates are tracked separately.

# Mobile007 / 010d3 — integrated verification

**IMPLEMENTED AND VERIFIED — 2026-09-15.** Backend, Web, both native journeys,
process-restart recovery, installed Mobile006 upgrades and D-050 gates pass.
This accepts the implemented D-086 scope; publication and deployment are separate.

## Scope and evidence identity

D-086 accepts both implementations and isolated synthetic verification. Integration
branch: `codex/mobile007-history010d3`, based on `edac8c3975bd457c436171ce47075689cfcc14ef`
plus the integration changes. Publication and deployment are a separate step.

Private evidence root: `/private/tmp/crm-mobile007-010d3-yyoxjx50`.
`logs/backend-production-final-manifest.json` records 362 backend source/migration
files with aggregate SHA-256
`23df8d8e82bcfcc5eb7e41d85835488f8afd527ec46e08a34a764cf21c019a92`.
The final focused acceptance runner is `acceptance-final-runner`, SHA-256
`0aed8d566e3a4864e0df8f1f1076a75558a68949d8655b70eda762b95ef9222b`.
It includes the later overlap/cancelled-cohort, executor-takeover and search-boundary
tests. The separate full-suite runner started before those test-only additions.
`logs/native-final-manifest.json` records the 15 native source/schema files and
six installed/verification binaries. Final source checks confirm that all 362
backend production files still match the recorded manifest.

- [Implementation and owned resources](MOBILE_007_010d3_IMPLEMENTATION_STATUS.md).
- [Web journey](SLICE_010d3_WEB_EVIDENCE.md).
- [Native checks and journeys](MOBILE_007_NATIVE_EVIDENCE.md).
- [Final iOS recovery, journey and installed upgrade](MOBILE_007_IOS_EVIDENCE.md).
- [Performance protocol and results](MOBILE_007_010d3_PERFORMANCE.md).
- [Mobile implementation review](MOBILE_007_IMPLEMENTATION_REVIEW.md).
- [Migration implementation review](SLICE_010d3_IMPLEMENTATION_REVIEW.md).

## Shared gates

| Gate | Evidence / current result |
|---|---|
| `scripts/check` | PASS: `logs/check-final.log`; 994 Rust tests, five doctests, 1,303 Web tests in 98 files, 11 email-worker tests, formatting, Clippy, production compilation, boundary checks, Web lint/typecheck/build. This run precedes the final remainder-cursor change. |
| Post-cursor backend checks | PASS: `logs/backend-final-gates.log`; formatting and Clippy after the production cursor change. Final test/fixture formatting and workspace Clippy also PASS in `logs/backend-test-final-lint2.log` (5m including build-lock wait/dependency checking). The first attempt, `logs/backend-test-final-lint.log`, stopped on formatting in the newly added UI fixture; it was formatted before the passing retry. |
| `scripts/sqlx-prepare` | PASS: `logs/sqlx-prepare-final.log`; no SQLx cache changes. |
| Full DB coverage | All 1,111 cases now have passing results. Initial `logs/check-db-final.log` stopped after 1,032 passes and one tight cleanup-timing assertion failed; 78 cases were skipped and one passing case reported a lingering handle. The unchanged failed test passed alone in `logs/db-today-recovery-retry.log` (2.67s). The 78 skipped cases plus the lingering-handle case passed 79/79 in `logs/db-remaining-final.log` (129.22s); `logs/db-remaining-inventory.json` lists them. The initial script failure is retained, not relabeled as a clean full-script pass. Its fresh-schema SQLx cache check passed. |
| Final focused additions | PASS 7/7, 142.57s: `logs/final-focused-acceptance.log`, using the pinned runner above. This includes five migration cases and two expanded search cases, adding two distinct migration tests beyond the original 1,111-test inventory. |
| Release preflight | PASS 53/53: `logs/preflight-current7.log`; includes the independent admitted-history capability and durable confirmed-root checks. |
| Web final journey | PASS: [Web evidence](SLICE_010d3_WEB_EVIDENCE.md) includes 50/77 cancellation and exact remainder, held acknowledgement/result, real release-readiness pause/Resume, pagination and desktop/390px views. Its owned API worker and browser polling are stopped. |
| Native final journeys | PASS: Android direct instrumentation `/tmp/mobile007-round2-android-discovery-note-ui4.log` (36.181s); iOS `/tmp/mobile007-ios-complete-ui12.log` (179.813s). Both verify exact matching, same-name distinctions, requested intent recovery, complete 101-Person download, pinned reason and offline note editing. iOS additionally verifies submitted note persistence after process termination. |
| Installed upgrades | PASS on both platforms using an actual installed Mobile006 binary followed by the current app without uninstall/key reset. iOS schema 9→10 preserves the original database inode, full old byte inventory and key. Android schema 8→9 preserves the key, 100 People, pending envelope, receipt and draft; seed/verify logs `/tmp/mobile007-android-schema8-seed2.log` (1.265s) and `/tmp/mobile007-android-schema8-verify.log` (1.327s). |
| D-050 | PASS: [performance evidence](MOBILE_007_010d3_PERFORMANCE.md). Mobile007 exact-query plans (18.36s), admitted-history plans (40.03s), and the complete Person/Today paired run (123.66s) pass. Person p95 20.3765→22.8405ms, limit 45.3765ms; Today 71.537125→72.272875ms, limit 96.537125ms. Both preserve complete response parity. The earlier Person-only run omitted the Today opt-in and is retained as incomplete protocol evidence. |

## Migration acceptance mapping

The final focused run and the existing full regression suite jointly verify the
following boundaries. A named test is a coverage reference, not an unrun pass.

| IDs | Coverage |
|---|---|
| H3-01, H3-02 | `admitted_history_cancelled_cohort_excludes_sibling_and_unsettled_people`, `source_authority_and_root_plan_cursor_scope_fail_closed`, and the three-family fixture use genuine admission/capture commands, exact retained cohorts, excluded original/sibling/unsettled People, ambiguous relationships and zero source-reader calls. Existing capture reconciliation rejects incomplete/conflicting captures before qualification. |
| H3-03 | `admitted_history_original_identity_overlap_preserves_first_owner_in_both_orders` and `admitted_history_three_families_and_exact_replay` exercise original/admitted first-owner competition, admitted replay, suppression tombstones and no ownership adoption or resurrection. |
| H3-04, H3-07 | Three-family coverage compares native Person rows, checks metadata-only history/provenance and separate record/executor clocks, and verifies suppression revisions and stale cursors. Existing history/timeline tests retain dense, tied, unknown and backdated reader behavior. |
| H3-05, H3-06 | `admitted_history_partial_cancel_remainder_and_full_budget_control` checks a real 50/77 partial commit, preserved settlements, inherited durable cursor, full-budget cancel and exact root/Organization accounting. `admitted_history_recovery_integrity_and_http_boundaries` covers wrong-key pause, fresh-worker readiness, explicit executor takeover and current authority before replay. |
| H3-08 | `independent_reader_stamps_and_cancelled_zero_write_root_remain_durable` exercises actual old reader/worker rejection; `admitted_history_readiness_requires_enabled_authority_and_erasure_triggers` disables a required trigger and proves inventory rejection. |
| H3-09 | Real router and typed authority cases cover member, foreign and revoked access, strict request bounds and no-store errors. Mobile held-workspace denial and existing authorization/logging suites preserve ordinary workspace gates. |
| H3-10 | Actual populated Web desktop/390px journey and D-050 evidence are recorded separately. |

## Mobile acceptance mapping

| IDs | Coverage |
|---|---|
| M7-01, M7-08 | Actual iOS/Android isolated API search/save/restart/download/offline journeys pass; platform evidence distinguishes process restart from Activity recreation. Exact matching also uses the real HTTP fixture below. |
| M7-02, M7-03 | `mobile007_search_matching_bounds_and_no_mutation` and `mobile007_search_shape_authority_and_held_workspace`: literal Unicode/wildcard bounds; exact email/phone; 0/25/26 results; deterministic duplicate-name IDs; null contacts/names; oversized stored rows; no context/receipt/reconciliation mutation; tenant/context/lease/membership/workspace checks and no-store outer errors. |
| M7-04, M7-05 | Account/query epoch fences and existing reconciliation crash/expiry tests, plus new native pin revision/staging/restart tests, preserve precise requested versus sealed states. |
| M7-06, M7-07 | Native requested-intent recovery lists, generic whole-set pin failure, cancellation, latest sealed selection reasons and in-place encrypted schema migration; platform evidence owns actual populated-store inventories. |

## Review and limits

Both planning reviews are READY. Mobile implementation review used its two allowed
rounds; its requested recovery-list correction is closed with authored executed
evidence. Android cold-process reopen also passes in
`/tmp/mobile007-round2-android-cold-start-ui.log` (6.458s).
No third review was opened. Migration implementation round 1 found no source blocker
and requires the final evidence recorded here.

All source data is synthetic/retained. Simulator/emulator evidence does not imply
physical-device distribution or calling acceptance. Laptop performance checks do not
claim production capacity. Existing shared API3000/Web5173/native API3106 services
and their databases are outside these verification resources.

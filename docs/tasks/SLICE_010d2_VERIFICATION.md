# Slice 010d2 — Implementation verification

**IMPLEMENTED / VERIFIED WITH ISOLATED SYNTHETIC DATA — 2026-09-12.** D-072 authorizes implementation and isolated
synthetic verification. This record does not authorize or claim Git integration,
deployment, live FUB processing, readable bodies or workspace activation.

Verification source was the then-uncommitted `codex/slice-010d2-history-timeline`, based on
`27f3fe4654ace5840412e1364ea36974a1266ad8`, in
`/Users/karrad/projects/crm-worktrees/slice-010d2`. The later explicitly authorized
[release](SLICE_010d2_RELEASE.md) committed, merged, pushed and deployed that
implementation and removed the worktree. These original checks retain their attribution.
The [approved specification](../specs/SLICE_010d2.md) and
[concrete contract](SLICE_010d2_CONTRACT.md) define the implementation.
The [source manifest](../design/qa/slice-010d2-2026-09-12/source-sha256.txt) pins
changed/new implementation and test files; the baseline identifies unchanged inputs.

## Scope and implementation

- Retained-only preparation, confirmation, application, cancellation, resumption
  and owned storage allowances. The first confirmation permanently anchors the
  completed People parent to one retained capture and interpretation.
- Separate append-only FUB event/call/text record facts, with encrypted deletable
  metadata, exact identity/canonical matching and permanent erasure tombstones.
  Source timestamps and actor roles remain distinct from the local import actor.
- Bounded v2 Person summaries, inquiry and timeline pages, maintained revisions
  and counts, consistent snapshots, and explicit refresh after changes.
- Both old complete readers are fenced before history volume is added. A
  transaction-local compiled capability and independent release preflight protect
  old API/worker compatibility and recovery selection.
- Administrator Web preview, confirmation, progress, retry, budget and paged
  metadata review. Existing paged notes/tasks retain their approved behavior.

Changed source is one additive migration; `crm-app`'s new
`history_import{,_source,_queries,_store,_worker}` and `history_review` modules;
API routes, wiring, workspace/error handling and legacy-reader fences; Web import,
review and People-prefetch paths; and the Python release-preflight tooling.
Five new DB test/support modules are `db_history_import`,
`db_history_import_authority`, `db_history_import_support`, `db_history_review`
and `db_history_timeline_compat`. No dependency, secret or original 010d1 parser
change is included.

## Review

One bounded independent implementation review found four actionable issues:

| Finding | Correction |
|---|---|
| W03: initial import page could arrive after status revision advanced | Compare every response to the current revision; hide stale pages and require refresh. |
| B01: legacy Person summary aggregated inquiries before its fence | Perform only scoped Person existence before the complete-reader guards. A DB test removes inquiry SELECT to prove the early fence. |
| B04: preparation could rebuild hidden metadata for a previously erased Person | Detect primary and captured participant erasure before interpretation; retain a permanent tombstone and no display payload. |
| B05: rejected readiness and oversized reads had reversed HTTP classifications | Readiness rejection is 409; oversized/corrupt reads are 503; command allowance conflicts remain 409. |

The reviewers inspected the targeted corrections: four applied findings, none
deferred, within one R1 and its correction confirmations. Additional integration
fixes compare timeline-detail revisions before rendering, keep old pages hidden
after a failed core refresh, show readiness guidance when server action flags are
false, and route the People inspector prefetch through v2. These are not a second
independent review. Review source inspection is distinct from execution below.

## Executable evidence

Results are attributed to the working tree used by each runner. The coordinator
ran backend, database, performance and actual-browser checks; the Web writer ran
the targeted client checks. Earlier focused runs are not a final-tree gate.

| Check | Observed result and qualification |
|---|---|
| SQLx preparation | **PASS** — `./scripts/sqlx-prepare`; `/private/tmp/crm-010d2-final-sqlx-prepare.log` ends `sqlx-prepare complete`. Coordinator reports no `.sqlx` diff. |
| Focused DB5 | **PASS: 14/14**, 15.810s, nextest run `8f79d26b-98fc-493b-ab37-e1f5d826743d`; `/private/tmp/crm-010d2-focused-db-5.log`. Eight import lifecycle tests, two HTTP authority tests, three compatibility tests and the imported-reader test. |
| Earlier native-reader checks | Four named reader tests passed in DB1; that overall run had **three failures out of seven**. Their later-tree coverage is recorded in the final DB gate below. |
| Web targeted checks | **PASS:** final affected seven-file Vitest batch **159/159**, plus `pnpm typecheck` and targeted ESLint. Earlier broader ten-file batch **184/184** and People-view batch **107/107** passed; these overlap and are not additive totals. Numeric ordinal DTO alignment subsequently passed typecheck/lint. |
| Opt-in performance | Original collector completed **1/1**, but plan inspection found excessive work; affected collector then passed **1/1**, 19.64s. A separate post-fix contact-detail case passed **1/1**, 12.90s. Scope below. |
| Production-build actual API browser | **PASS**, first walkthrough: 112 retained records; partial cancellation and same-plan completion; desktop and 390px review. [Evidence](../design/qa/slice-010d2-2026-09-12/README.md). |
| Final `./scripts/check` | **PASS**, attempt 3, 68s — `/private/tmp/crm-010d2-final-check-3.log`: 31 preflight tests, 963 Rust tests, five doctests, 84 Web files/1,164 tests and 11 email-worker tests; formatting, clippy, production checks, lint, typecheck and build all passed. Later test-only additions passed formatting and `cargo clippy -p crm-api --test all --locked --features perf-harness -- -D warnings` in `/private/tmp/crm-010d2-final-affected-static-2.log`; database execution follows below. |
| Final `./scripts/check-db` | **PASS: 953/953**, 476.377s test execution / 553s total — `/private/tmp/crm-010d2-final-check-db-1.log`. Includes live-schema SQLx prepare-check, all nine import cases, authority, all five reader cases and the deterministic confirmation race. One passing slow test and one passing LEAK annotation are qualified below. |
| Targeted final evidence | **PASS** — deterministic old-reader/first-Confirm race passed in the final DB run. Post-fix contact-detail EXPLAIN passed in `/private/tmp/crm-010d2-history-review-detail-plan-1.log`; [structured evidence](../design/qa/slice-010d2-2026-09-12/detail-plan.json). These additions changed tests only. |

The final affected Web command was run from `web/`:

```sh
pnpm exec vitest run src/components/PersonHistoryReview.test.ts src/components/migration/HistoryImportPanel.test.ts src/components/migration/HistoryImportRecords.test.ts src/components/PersonActivityReview.test.ts src/components/PersonPreview.test.ts src/views/PeopleView.test.ts src/api/historyReview.test.ts
pnpm typecheck
```

DB5 used the `crm-api::all` nextest binary with the focused history filters and
ignored SQLx tests enabled. The full gate covered all registered default-feature
DB tests. Opt-in performance, old-binary and browser checks retain their separate
execution evidence. No test was rerun solely to write this record.

## T1–T8 mapping

Test names below refer to the current repository source; their execution status
is the applicable row above.

| Gate | Concrete evidence | Status / limit |
|---|---|---|
| T1 — authority | [HTTP authority suite](../../backend/crates/crm-api/tests/db_history_import_authority.rs): `full_router_import_authority_covers_every_read_and_write_without_source_io` checks all four scoped GETs and all five POSTs for anonymous, member, platform-only and foreign-admin callers, positive same-Org reads, exact body-free/no-store errors and unchanged run/receipt/anchor/ledger state. `malformed_requests_and_current_revocation_cannot_replay_a_confirmed_receipt` performs real HTTP Prepare/Confirm/exact replay, then demotes that actor before the same receipt replay; all five POSTs reject unknown fields and >8KiB whitespace-padded valid JSON. | Both passed DB5. A foreign administrator's unfiltered own-Org list correctly returns empty 200; foreign scoped resources return 404. |
| T2 — fidelity | [Import tests](../../backend/crates/crm-api/tests/db_history_import.rs): `retained_three_families_preserve_native_truth_exact_bytes_and_actor_bound_receipts`; `nonenumerated_repeat_variant_and_invalid_source_capture_cannot_prepare`; `groups_excluded_and_unmapped_are_held_while_disconnected_retained_positive_imports`; erased-before-preparation test. [Interpreter tests](../../backend/crates/crm-app/src/domain/migration/history_import_source.rs) cover independent dates/roles, exact canonical numeric values, unknown fields and inert body-free metadata. | Named DB cases passed DB5. Pure tests passed final check 3 after correcting two numeric-string expectations. All-held ready-plan rejection passed in the final DB run. Invalid/repeat/variant source inputs remain ineligible upstream; see qualification below. |
| T3 — effects | `retained_three_families…` and `cancelled_attempt_reuses_frozen_plan_and_two_workers_converge_without_duplicates` compare the explicit `frozen()` native/Person/contact/note/task/correspondence and original parent/capture row inventory. Fake core/history reader counters remain unchanged. The original performance collector compares fixed-clock ordinary Today results before/during/after the separate review-Org import. | Focused preservation tests passed DB5; ordinary Today payload checks passed the collector. These claims cover the named row/query inventory and synthetic readers, not live provider delivery. |
| T4 — retry/races | `cancelled_attempt_reuses_frozen_plan_and_two_workers_converge_without_duplicates` commits 50/125 facts, cancels, reuses the unchanged plan/anchor, and runs two independent worker sessions to finish with 75 inserted + 50 already imported. `retained_three_families…` covers actor/body-bound replay; corruption/recovery tests cover rollback, lease/admission and explicit takeover. Web uncertain-result tests replay the exact reviewed body across fresh status reads. | Focused cases passed DB5 and Web batch. This is bounded rollback/recovery and concurrency evidence, not arbitrary process-kill fault injection at every statement. |
| T5 — storage/erasure | `recovery_readiness_wrong_key_and_lowered_budget_pause_before_work_and_preserve_cancel`, `corrupt_retained_display_pauses_without_partial_facts_and_resume_adopts_current_admin`, `display_suppression_invalidates_import_cursors_and_never_resurrects_facts`, and `person_erased_before_preparation_creates_only_holds_and_permanent_tombstones`; exact owning-run and Org logical-byte measurement, control/sibling reservations, suppression and permanent tombstones. | Passed DB5. Synthetic erasure controls do not complete customer erasure/key/backup policy. |
| T6 — readers | [Reader suite](../../backend/crates/crm-api/tests/db_history_review.rs): `all_native_families_and_inquiries_are_bounded_body_free_and_fully_traversable`; `current_authority_cursor_scope_and_partial_parent_are_enforced`; `maintained_counts_metadata_changes_and_backdated_inserts_require_refresh`; `committed_write_between_families_cannot_mix_snapshot_rows_and_revision`; `imported_families_known_unknown_filters_provenance_and_suppression`. Complete-reader test revokes inquiry SELECT to prove the fence occurs before aggregation. | All five reader cases and the fence case passed together in the final DB run. Row/loop measurements are qualified below. No operational call fold is reconstructed. |
| T7 — compatibility | [Compatibility suite](../../backend/crates/crm-api/tests/db_history_timeline_compat.rs): `timeline_anchor_fences_both_complete_readers_before_any_fact`; `timeline_compiled_read_capability_is_actual_connection_and_transaction_local`; `timeline_actual_capture_only_api_artifact_fails_closed`. [Preflight tests](../../scripts/tests/test_migration_release_preflight.py) separately exercise timeline capability, retained anchors, current/recovery worker selection, missing/old reports, schema/profile/freshness and CLI-only rejection. | Three DB cases passed DB5; preflight tests passed final check 3. The deterministic race passed in the final DB run: both old readers held shared permits while blocked after their guards; Confirm waited on both reader PIDs, then both readers returned their exact pre-anchor bodies and subsequent reads returned 409. Actual old-artifact behavior is described below. No shared deployment was retired or changed by this verification. |
| T8 — Web | [Walkthrough](../design/qa/slice-010d2-2026-09-12/README.md) and [results](../design/qa/slice-010d2-2026-09-12/walkthrough.jsonl): four acknowledgements, real confirmation, worker-readiness pause, explicit Resume, 50-fact partial cancellation, same-plan confirmation and completion with 62 inserted + 50 already imported. Event filter/paging, metadata dialog/Escape and undated text at 390px. Targeted Vue tests cover late Person/actor/Org/role/session/workspace responses, stale revision refresh, bounded Previous/More, exact uncertain request recovery and separate notes/tasks. | Browser, focused checks and final full Web gate passed. Zero page errors, no displayed body sentinel or horizontal overflow in the tested 390px flow; not an exhaustive accessibility audit. |

## Actual old artifact and performance scope

The compatibility runner launched the supplied deployed 010d1 executable via
`CRM_010D2_OLD_API` against a disposable SQLx database, with synthetic credentials
and an isolated loopback port. The coordinator verified the original release
binary SHA-256 as
`7890b6d978c1025cfa7243a1332a5e3718c9ba15a1edb4ea461f8ea664f49b62`.
Both old complete Person routes returned 200 before
confirmation and closed 503 `unavailable` responses after the zero-fact anchor,
including after cancellation. The upgraded API returns the specified 409
`history_review_required`. This proves actual old-reader denial, not retroactive
old-startup detection: a later deployment must still inventory/drain/retire
unsupported artifacts and select compatible recovery. The test owns and removes
its child process/directory.

The actual-browser fixture also completed and cleaned up: its private credentials
were removed, browser cookie/context closed and owned API/Web ports 13012/15175
stopped. Shared development services were not changed.

[Performance evidence](../design/qa/slice-010d2-2026-09-12/performance.json) records
the exact opt-in command, log hashes and measured source-file hashes. Each review
and ordinary Org has 25,000 People and 50 members; the review distribution has
75,501 native contact rows and a dense 504-row target, but **only three external
imported facts**. This is not a populated 75,000-external-fact qualification.

The original collector emitted 32 hot-query plans and passed output assertions;
inspection found three 75,501-row contact metadata scans and an inquiry plan
reading 501 rows. Those observations remain failures of the original plan shape.
The affected rerun verifies corrected contact first/continuation and inquiry
queries: each returns 51 rows, no temporary blocks, and bounded indexed work
(maximum contact node loops 51). Remaining unchanged plans retain their original
attribution. A separate [post-fix contact-detail run](../design/qa/slice-010d2-2026-09-12/detail-plan.json)
then measured the actual detail SQL on 25,000 People and 75,501 contact facts:
one row, one loop per node, eight shared-buffer hits, no reads or temporary blocks,
and 0.065ms execution. Positive/negative superseded controls passed. This closes
the detail-plan measurement gap without repeating the paired workload.

The paired regression measures the matched HTTP component: actual session/Auth
extraction, old versus new workspace guard, unchanged fixed-clock Today route and
fully drained body, on the same upgraded schema. It excludes the full production
router's unrelated middleware and network. Five concurrent loads, five warmups
and 20 observations per arm produced equal payloads; old/new p95 were
120.431/124.057ms, below the D-050 relative limit of 145.431ms. This is not an
old-schema or production-capacity benchmark. Eight native writer probes compare
single EXPLAIN inserts with 21 new revision triggers disabled only in the baseline
transaction, then roll back and restore all triggers; their timings are descriptive
single-pair observations, not statistical overhead estimates.

## Failed-run history

Original failed runs are retained in private local logs
`/private/tmp/crm-010d2-backend-first-check.log`, `backend-check-2.log` and
`focused-db-1.log` through subsequent numbered runs (all with the
`crm-010d2-` prefix). Failures exposed a borrowed-value compile error, test integer
type mismatches, a generic trigger referencing a nonexistent record field, an
inferred INT4 manifest position, and a PL/pgSQL record/alias collision. Test
expectations were also corrected for exact canonical numeric notation and
shared Organization totals versus a sibling's owned reservation. DB1 finished
4 passed/3 failed; DB2 did not compile; DB3 finished 0 passed/9 failed; DB4 finished
5 passed/4 failed; DB5 finished 14 passed/0 failed. Do not relabel the earlier runs.

The first performance attempt failed before measurement; the next completed but
exposed the plan-shape issue above. The affected rerun's first attempt had two
harness compile errors (`ApiError` Debug and digest LowerHex), then its second
attempt passed. Final `scripts/check` attempt 1 stopped with 503 passing tests and
two stale canonical-duration expectation failures before completing the suite;
`/private/tmp/crm-010d2-final-check-1.log` remains the failure evidence. Attempt 2
then failed formatting only after the assertion corrections; the coordinator ran
`cargo fmt` and attempt 3 passed. The successful rerun does not erase either
earlier failure. An extra all-target `perf-harness` clippy invocation failed on
34 unused-code warnings in the existing `db_today_feeds_http_perf` fixture;
no changed source was implicated. The required default gate had passed, and the
affected `--test all --features perf-harness` target passed unchanged. Both logs
are retained as `final-affected-static.log` and `final-affected-static-2.log`
with the same `/private/tmp/crm-010d2-` prefix. Final DB execution belongs above.

The final DB gate's existing `db_snapshot_scale::shared_phone_25000_people_has_bounded_frozen_groups_and_indexed_queries`
test passed in 88.630s after a slow annotation. The existing
`db_metadata_import_gate::metadata_gate_current_ceiling_pause_explicit_retry_and_cancel_at_exhaustion`
passed with a nextest LEAK annotation. One exact isolated follow-up passed in
8.329s without that annotation (`/private/tmp/crm-010d2-final-db-leak-followup.log`).
The runner annotation's cause was not diagnosed and is not relabeled as a source
fix. No owned test runner remained in the process inventory; owned browser/API
ports were closed. No additional full gate was run.

## Qualification limits

The original 010d1 terminal rules reject captures containing invalid identities,
equal repeats or conflicting variants. A real negative fixture must remain
ineligible for preparation; these inputs are not represented as successfully
completed captures merely to exercise the downstream classifier.

This slice does not qualify live API completeness, vendor pagination, readable
message/note bodies, customer erasure operations or per-Person keys. Original
encrypted raw pages retain the existing shared-page erasure hold. Synthetic
suppression/tombstone verification does not complete that separate runbook.
Repair, delta import, cutover and workspace activation remain separate work.

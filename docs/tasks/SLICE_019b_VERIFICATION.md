# Slice 019b — Verification record

Release follow-up: the user subsequently authorized “commit, deploy, merge
with main and delete loose branches.” That authorization supersedes the
implementation-only release restrictions below. The verification results
remain the evidence for this unchanged source tree.

Status: implemented, committed, merged into local main, and deployed to the
shared development runtime. See the [release record](SLICE_019b_RELEASE.md)
for revision, deployment checks and branch/worktree cleanup.

Implementation checkpoint: the two implementation review/fix rounds are complete.
The browser walkthrough and all code/SQLx/database gates passed. The single
paired performance run passed all thirteen request-p95 limits and all eighteen
inspected plans. The user approved the
specification and implementation on 2026-09-10; no further contract approval is
pending.

## Delivered scope and tree

Five custom-field clauses within the existing twenty-clause filter now work
across People, saved lists, Today sources and admin Today rules. Text, exact
numeric ranges, calendar dates, choices and presence share one Rust vocabulary
and fourteen static SQL statements (fifteen matrices). Negative predicates include
empty values. Archived fields create repairable invalidity; held archived
options remain valid. Web editors use local drafts, explicit Apply/Enter,
field-specific identity, accessible controls and shared metadata. Custom value
changes refresh dependent views through local settling and realtime.

- Spec: [SLICE_019b.md](../specs/SLICE_019b.md); brief: [SLICE_019b_IMPL.md](SLICE_019b_IMPL.md).
- Verification checkout: `/Users/karrad/projects/crm-worktrees/019b` (subsequently removed after merge).
- Branch: `codex/slice-019b-custom-field-filters`.
- Base/current commit at the verification checkpoint: `6bad52a32376001d4082ac21aad29699462e3ceb`.
- Code-tree SHA-256 after final fixes and SQLx preparation:
  `a75ee70b9b3b51021ca00853109e71ff8c222ca46ad1f91c0937976fabf88e0a`.
- [Per-file code hashes](../design/qa/slice-019b-2026-09-10/code-tree.json)
  and [tracked/untracked changes](../design/qa/slice-019b-2026-09-10/changed-files.txt).
- At this verification checkpoint the changes were uncommitted and the shared
  runtime was untouched. Subsequent release actions are in the release record.
  No migration, dependency addition or Operator tool/schema expansion occurred.

Astra high authored the plan and independent reviews. Terra high implemented
backend and Web, with sequential primary-writer handoff and later disjoint Web
ownership. The coordinator implemented thin Web validation/description helpers
and focused tests, integrated results, ran verification and owned all records.
No file had concurrent writers. Per-agent token/cost measurements were not
available and are not estimated.

## Changed files

- Rust `person/filter.rs` and query bindings; all fourteen Person/Today SQL
  statements; scoped custom-field reference and label queries; saved-list,
  Today-feed and API/Operator error/description integration.
- Fourteen replacement SQLx entries, custom-filter DB/Operator/preview tests,
  and the opt-in paired request harness with frozen baseline SQL fixtures.
- Web `filter.ts`, API types/count validation, `FilterBar.vue` and the new
  `CustomFilterEditor.vue`; People/Today/Fields integration, mutation settling,
  realtime invalidation and focused tests.
- Approved spec/brief, decision and source-spec pointers, project state,
  verification, screenshots, gate logs and performance artifacts.

The [exact tracked/untracked inventory](../design/qa/slice-019b-2026-09-10/changed-files.txt)
includes every file, including new fixtures and evidence.

## Required checks

| Command | Result |
|---|---|
| `./scripts/check` | Passed in 40 seconds: format, workspace Clippy, ordinary production-shape check, crate fences, **831 Rust tests**, **5 doctests**, **878 Web tests / 51 files**, Web lint/typecheck/build, **11 worker tests**. |
| `./scripts/sqlx-prepare` | Passed in 31 seconds; only the fourteen affected offline query entries changed. |
| `./scripts/check-db` | Passed in 320 seconds: SQLx metadata/schema check and **785/785 DB tests**, including authorization, privacy, lifecycle, idempotency, deadlines and reconnect recovery. |
| Single §10 authenticated performance invocation | Passed once in 134 seconds: **13/13 paired p95 budgets**, 1,040 complete measured requests plus 150 complete warmups, complete body parity and **18 inspected plans**. |
| `git diff --check` | Passed after the final source fixes; repeated with final records before handoff. |

Database gate nextest ID: `34da9f10-d0eb-434a-8259-b4c2c6111fd3`
(785 passed, 831 non-DB tests correctly skipped; test phase 274.785 seconds).

Code gate nextest ID: `670c1521-59a8-43bf-9904-359c4095985e` (831 passed,
785 DB tests correctly skipped in this non-DB gate). Retained [gate logs and exit statuses](../design/qa/slice-019b-2026-09-10/gates/README.md)
record exit 0 for all four final commands. Private originals remain at
`/private/tmp/crm-019b-final-{check,sqlx,db,perf}.{log,status}`. The first code-gate attempt stopped at formatting: standalone
rustfmt had reformatted the new DB test differently from the workspace. The
workspace formatter corrected it and the full code gate was rerun successfully;
`crm-019b-final-check-format-attempt.{log,status}` retains that failed attempt.

Before the final gates, executed focused evidence included 81 filter unit tests,
21 updated custom/Operator/request-span DB tests, 10 preview DB tests and 184
legacy filter/list/Today regressions. Offline perf-feature compilation passed.
The final Web fixes passed 42 FilterBar tests, 11 direct custom-editor tests,
40 query tests including staggered success/error settling, plus the view and
realtime integration suites. These focused results supplement the complete
final gates; they are not substituted for them.

## Review dispositions

Two planning review rounds completed before the user's approval. The separate
implementation budget was also exactly two rounds: Part A backend/harness,
then Part B integration/editor plus verification of the first fixes. No third
formal review was run. The second report required the last three corrections
below; the writers applied them and the coordinator inspected their diffs and
executed the affected tests/browser paths and final gates.

| ID | Fixed issue / verification |
|---|---|
| I019B-01 | Omit absent numeric/date range properties instead of serializing null; persisted one-sided lifecycle test. |
| I019B-02 | Reject unpadded dates before decoding; canonical/date boundary tests. |
| I019B-03 | Resolve custom labels for feed previews through bounded scoped lookups. |
| I019B-04 | Redact `FilterNames` Debug output; label sentinel assertion. |
| I019B-05 | Use returned saved-list fixture metadata instead of a nonexistent owner column. |
| I019B-06 | Explicitly map literal SQLx `?`/`!` aliases in frozen runtime row decoders; frozen SQL bytes unchanged. |
| I019B-07 | Correct relative budget to `old + max(old / 10, 25ms)`. |
| I019B-08 | Classify actual Today complete/partial/unavailable envelopes in both measured arms and untimed preflight. |
| I019B-09 | Independently exercise both person-state matrices with different five-slot criteria and inquiry/reply reasons. |
| I019B-10 | Test four-type presence/absence, nonexistent/real foreign fields and custom label/operand telemetry sentinels. |
| I019B-11 | Preserve invalidation keys as stale without refetch while a sibling Person mutation is pending; staggered custom-first/note-last success and uncertain-error tests. |
| I019B-12 | Distinct resolved-field remove-button names, including neutral unavailable fallback; test and browser confirmation. |
| I019B-13 | Search empty state considers custom matches/loading/errors; test and browser confirmation. |

Round 1 checkpoint: `205ba8871be4dfd8ecc99839cc2c20480d5b621157c33f8a630a8b2281bcdda4`.
Post-round-1 backend: `cc1b30eadfa628e0436f65910f86f81b0ea952abede5a48d9c0cadde727b9185`.
Round 2 checkpoint: `0acbbed5e06df023acac86d86dad1ac47d15178cd5877214f19a9d136939531f`.
The reviewer checked the corresponding per-file hashes. Final fixes are included
in the delivered-tree hash above; the reviewer did not claim to run tests or
measure performance.

## Browser and authenticated walkthrough

Passed with three synthetic People, six custom definitions and member/admin
accounts in two Organizations. [Eight screenshots and walkthrough details](../design/qa/slice-019b-2026-09-10/README.md)
cover all four editors, two independent text fields, literal matching,
Apply/Enter/Escape/outside behavior and focus, invalid ranges, a 390 × 844
viewport, the fifth-field limit, saved-list creation, Today connection, local
value clearing and realtime restoration, archive repair and restoration,
admin preview with locked anchors, and Organization isolation.

The [lifecycle record](../design/qa/slice-019b-2026-09-10/lifecycle.json) proves:
archived-field count is 422 `invalid_field`; revision remains **1** through
archive/restore; restoration returns **1** match. An archived Warm option
continues to match and is selectable with its Archived marker. An admin could
not read the member's personal list; archive copy exposed no private list count.
Thirteen earlier authenticated HTTP smoke cases also passed and are retained.

QA used API `3001`, Vite `5180`, separate Centrifugo `8001` and isolated database
`crm_slice019b_qa`. All QA servers were stopped before DB/performance gates;
the synthetic DB and private launcher files are retained for review. Main
API/Web, source data and shared runtime configuration were untouched. Live
Operator inference was disabled; executed tool/DB tests verify the saved-list
Operator and untrusted-description/privacy behavior instead.

## Criterion-to-test mapping

1. **Strict custom wire contract.** Rust unit tests: `custom_field_clauses_round_trip_and_bind_in_wire_order`, `custom_field_structural_limits_and_invalid_operands_fail_closed`, `custom_wire_shape_rejects_null_duplicate_and_noncanonical_operands`, `custom_slot_and_choice_caps_are_exact`, `one_sided_custom_ranges_round_trip_without_null_operands`, and `custom_date_bounds_require_exact_canonical_lexemes` in `domain::person::filter::tests`. Thin Web coverage: `custom filter wire validation` and `caps custom fields by identity, preserving two independent fields of the same type` in `web/src/lib/filter.test.ts`.

2. **Typed membership, absence, literal matching and ranges.** DB acceptance module `db_custom_field_filters`: `custom_field_filters_preserve_literals_bounds_absence_and_field_identity` and `custom_presence_tests_cover_all_types_and_zero`; legacy/custom mixed regression in `db_people_filter::custom_field_filter_text_negative_includes_absent_and_archived_field_is_invalid`.

3. **Reference trust, precedence and closed envelopes.** DB: `custom_field_filter_reference_errors_are_non_leaking` (including random, foreign, and wrong-type field comparators); `preview_missing_subject_precedes_invalid_custom_reference`; stale stored-read handling in `saved_lists::stored_unknown_and_stale_filters_fail_closed_but_typed_stale_rows_can_repair`. The custom fixture also covers option/reference handling; field archive lifecycle is covered by `custom_ranges_survive_saved_lifecycle_and_field_archive_restore`.

4. **All fourteen static statement paths, five slots, caps and parity.** DB: `custom_filter_reaches_every_statement_family_with_five_slots`; saved-list count parity/caps: `saved_count_matches_people_for_every_filter_axis_mixed_and_exact_caps`; existing People cap/order regressions: `cap_truncates_to_500_with_created_at_desc_id_asc_ordering_under_a_filter` and `ordering_ties_break_by_id_asc_under_a_filter`.

5. **Legacy compatibility and mixed filters.** Existing DB legacy axis/count suites remain in `db_people_filter` and `db_saved_lists`; new mixed custom coverage is in `custom_filter_reaches_every_statement_family_with_five_slots` and the customized feed preview tests. Web Today test `keeps Today anchors beside an applied custom clause` pins the locked-anchor payload shape.

6. **List/source/feed lifecycle, archive/restore, option ID semantics and deadline.** DB: `custom_ranges_survive_saved_lifecycle_and_field_archive_restore`; `today_source_uses_one_absolute_deadline_through_savepoint_membership_and_release`; `today_source_recovery_deadline_survives_a_malformed_tail_until_final_commit`; `preview_resolves_custom_field_labels`; `preview_missing_subject_precedes_invalid_custom_reference`. Web: Fields archive test `explains the saved-filter consequence and sends archived: true on confirm`, plus `renames an option inline and archives it` and `restores an archived option`.

7. **FilterBar/People/Today UI behavior.** Web: `FilterBar custom-field editors (SLICE_019b §§6–9)` suite, `custom filter draft editor` suite, `passes custom-field loading state and retries the definitions query`, parameterized `pauses an $error custom-field list until removing the invalid clause`, `forwards custom definitions to the locked editor and retries them independently`, and `keeps Today anchors beside an applied custom clause`. These cover custom-field identity, apply/cancel/validation, caps, archived/missing repair, loading/retry, saved-list repair, and locked Today anchors. Browser walkthrough passed; see the evidence below.

8. **Cache/realtime behavior.** Web `Slice 019b custom-field cache settlement` suite: `invalidates the full Organization branch after a field or option update`, `settles set and clear across every custom-filter-dependent key`, `keeps custom-value keys stale until a later pending note releases the hold`, and its uncertain-error twin. Realtime: `maps custom_field_changed to every custom-filter-dependent surface`; note-only narrow coverage remains `maps note_changed to the Person detail only`.

9. **Operator trust and privacy.** DB: `custom_saved_list_description_is_untrusted_and_read_only` and `span_capture_contains_filter_kinds_and_no_names_ids_or_day_counts`; operator snapshot/fences are exercised by the ordinary check's crate-boundary and test gates.

10. **Performance and delivered-tree gates.** Performance harness: `db_custom_field_filter_perf::slice_019b_authenticated_request_performance` passed once. All 13 paired request budgets and 18 inspected plans passed; raw timings, parity, plans and provenance are retained in the [performance archive](../design/perf/slice-019b-2026-09-10/README.md). Final `sqlx-prepare`, `check-db` and ordinary delivered-tree `check` passed.

## SQL and performance evidence

The fourteen frozen SQL texts match `6bad52a` byte for byte (including the
5,246-byte inline count). Current SQLx metadata was regenerated without schema
changes. Bind inventory: eight People sorts **72**; count **71**; source
membership **73**; source candidates **75**; person-state **145** (two independent
five-slot matrices); call membership **73**; call-only **74**. All fifteen
matrices append five nine-value slots and retain old parameter positions.

The one opt-in run uses normal authenticated loopback requests in the same
build/fixture/clock, with a test-only fixed server-side frozen/live dispatch.
It covers eight sorts, mixed stage/tag list count and Today with zero/five
sources, with separate concurrency-five samples. Each arm has five serial
warmups and forty measured requests per case; Today also has two warmup and
eight measured five-request waves. The budget is paired request p95 growth
of `max(25ms, old p95 * 10%)`; no absolute latency or above-envelope gate.
Custom predicates use parity tests plus eighteen full EXPLAIN ANALYZE/BUFFERS
plans (fourteen hot statements and four bounded metadata reads), rather than
a fabricated old custom-filter comparator. Fixture: 25,000 People, 50 members,
50 definitions, five values per Person and a second Organization.

**Passed.** The [performance archive](../design/perf/slice-019b-2026-09-10/README.md)
retains the exact invocation, raw 1,040 measured requests and 150 warmups,
13 paired comparisons, 18 full plans, buffer summaries, provenance and hashes.
All requests were successful and complete, and ordered response hashes matched.
Zero-source Today serial p95 increased 19.348 ms, within its accepted 25 ms
allowance; the report does not claim every path became faster. Indexed value
scans execute once per fixed subplan, not once per Person. Person-state has
40 such scans; the other thirteen hot statements have five each. Diagnostic
scan flags and their interpretation are disclosed in the archive. No broader
production-capacity or forced-generic-plan claim is made.

## Limits and release notes

API and Web must ship together: older binaries fail closed on the new custom
kinds, and newly stored definitions are unreadable after rollback until support
is restored. Definition changes follow the accepted no-push precedent; other
agents receive authoritative metadata on refetch/navigation/reconnect within
the one-active-tab envelope. Criteria remain ordinary erasable configuration,
not Person-value history. No broader capacity claim follows from the local
25k-person benchmark. The later authorized commit, merge, deployment and cleanup are recorded in
[SLICE_019b_RELEASE.md](SLICE_019b_RELEASE.md); no push was performed.

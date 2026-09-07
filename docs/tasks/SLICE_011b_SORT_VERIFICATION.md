# Slice 011b-sort — Verification evidence

Status: **VERIFIED ON THE FINAL TREE (2026-09-06, late evening); awaiting the
user's commit and merge approvals.** Every claim names its runner and actual
result. Push and deployment are separate approvals.

## Target and runners

- Branch `slice-011b-sort` from `main` at `31c9980`; uncommitted working tree.
- Specification [SLICE_011b_SORT.md](../specs/SLICE_011b_SORT.md) (planner:
  `crm-planner` subagent; independent specification review: `crm-reviewer`,
  READY-WITH-FIXES, eight corrections applied before approval); decision
  D-048; implementation approved by the user at the gate the same evening.
- Lanes: `sort-backend` (Claude Sonnet 5, everything under `backend/` plus
  `scripts/perf`) and `sort-web` (Claude Sonnet 5, everything under `web/`)
  in parallel with disjoint files; `sort-web-fixes` (Claude Sonnet 5) for the
  Web review findings. Coordinator, sole gate runner, performance measurer
  and evidence owner: Claude Fable 5.1. Independent implementation reviews:
  `crm-reviewer` (Web, backend) and `crm-tester` (adversarial analysis).

## Lane hand-backs (as reported by each lane and spot-checked)

- **sort-backend:** `PersonSort` type with unit tests; the canonical matrix
  moved to `person/sql/filtered_summaries.sql` with seven sorted copies and a
  text-parity unit test; `filtered_summaries_sorted` dispatch; `sort` in the
  typed People query extractor with span recording; migration
  `20260907000001_saved_list_sort.sql` (two nullable columns, three CHECKs,
  no tombstone constraint); commands, fingerprint rule, reads and fail-closed
  decode; 19 DB tests in `tests/db_people_sort.rs`; saved-list, schema, Today
  parity and service-free route tests; `scripts/perf bench` gains fourteen
  sorted cases. Lane checks: `./scripts/sqlx-prepare` passed; `cargo fmt`,
  `clippy -D warnings` clean; 688 non-ignored tests passed; targeted ignored
  set 123 passed after one fixture fix (a double-inserted membership). Cache:
  7 entries replaced (the canonical People entry and six saved-list
  statements that now name the sort columns) and 14 added; `list_summaries`,
  the count statement and every Today entry untouched. A mechanical
  `sort: None` was added to command literals in thirteen existing test files
  outside the nominal list, with no logic change, so the crate compiles.
- **sort-web:** `lib/sort.ts`, wire types and the key factory extension,
  sortable DataTable headers, the Added column, URL sync, dirty-draft and
  dialog integration, cap message; 50 new tests. Lane checks: lint, typecheck,
  build clean; 502 tests in 40 files passed.

## Performance gate (coordinator, spec §4)

Passed. Evidence in
[docs/design/perf/slice-011b-sort-2026-09-06/](../design/perf/slice-011b-sort-2026-09-06/README.md):
same-run `4-clause combo` p95 343.6 ms against sorted unfiltered p95 of
20.0 ms (`created.asc`), 126.9–132.0 ms (name), 64.5–65.1 ms (stage),
72.1–72.8 ms (assignee) on the retained 100,060-Person organization; on the
seventh prepared execution every statement kept its custom plan (the generic
plan was costlier), no node looped over the organization, and the
per-output-row subselects ran at most 502 times. Neither pre-declared lever
was needed.

## Live browser check (coordinator)

Passed against the branch stack, evidence in
[docs/design/qa/slice-011b-sort-2026-09-06/](../design/qa/slice-011b-sort-2026-09-06/README.md):
accessible header controls, `?sort=name.asc` then `name.desc` with
`aria-sort`, the Save as summary "Sorted by Name (Z–A)", a created list
persisting and reopening with `name.desc`, deletion, and narrow layouts at
1100 and 700 px with the inspector open and no page-level overflow.

## Independent implementation review: Web

`crm-reviewer`: no P1 or P2 defect; contract exactly per §6; verified the
key shapes, URL sync, dirty-draft model, shared-reader rights, default
normalization and DataTable accessibility. Findings, all dispositioned to
lane `sort-web-fixes`: one optional hardening (a sort click must not mark a
URL-origin filter as user-origin) and four test gaps (literal key-shape
assertions, `created.desc` normalization at page level, a sort-driven 409, a
sort-only Back/Forward); the narrow-layout evidence requested is the live
check above. A forward-compatibility note (an unparseable stored token would
be coerced to default by the client) is recorded as LATER: the server fails
closed and never emits such a token today.

## Independent implementation review: backend

`crm-reviewer`: no P1 or P2 defect. Verified by reading: tenant isolation of
all seven sorted statements (each differs from the canonical text in exactly
its top-level `ORDER BY` line; the literal Organization predicate and the two
joins unchanged); default paths byte-identical (absent or `created.desc`
routes to the untouched statements; the canonical file's text and describe
metadata equal the old inline entry); the ORDER BY clauses verbatim per §3;
the typed extractor's 400 before pool acquisition (verified by inspection;
DB tests pin every malformed form); the fingerprint rule; revision bump only
on real change; shared-list rights; delete clearing both columns; the
fail-closed shape and count 422; Today untouched; the migration exactly as
specified; span recording of the static token only; the mechanical
`sort: None` insertions changing no logic; the fourteen bench cases.
Findings, dispositioned to lane `sort-backend-fixes` (tests and one comment;
no production change): an HTTP body-level sort test, the `first_name`
secondary key and the `created_at DESC` tie link, and a misleading
extractor-order comment in `tests/people.rs`. Two notes recorded: if the
`latest_src` guard were ever adopted, the parity test's tolerance, all seven
files and the cache must move together (not needed after the gate); and a
structurally unsupported filter with a valid stored sort reports
`sort: null`, now stated in spec §5.

### Offline cache accounting

`git status --porcelain backend/.sqlx`: 7 replaced, 14 added, 0 modified
tracked entries. Replaced, with the reason the review confirmed:

| Old entry | Statement | Reason |
|---|---|---|
| `3b7e3084a11a…` | People filtered summary matrix | Text moved verbatim into `person/sql/filtered_summaries.sql`; the file's bytes are the new entry's query; describe metadata identical |
| `2628c59ac453…` | `INSERT INTO saved_list …` | Now names `sort_key`, `sort_direction` (§5) |
| `485c6b1e4f87…` | `UPDATE saved_list SET name, filter …` | Same |
| `bba820aba674…` | Tombstone `UPDATE saved_list SET name = NULL …` | Clears the sort columns (§5) |
| `2cacd8852737…` | Retry-token row lookup | Reads the sort columns |
| `69db1bb187b9…`, `c7b1b449209d…` | `visible_live_row`, `visible_live_row_for_update` | Read the sort columns |

`list_summaries`, the count statement and every Today entry are untouched
(no `M` entries).

## Adversarial test analysis

`crm-tester`: the backend is tight (403 and revision checks precede the
equal-definition no-op; the no-op compares raw stored columns; the
Organization advisory lock serialises racing saves; `created_at` is NOT NULL;
the seven statements differ from canonical on exactly one line). One
defect-like finding: the Web coerced an unrecognised stored sort token to the
default, where the server fails closed, so a later rename could silently clear
the author's sort under version skew; dispositioned to lane `sort-web-fixes`
as a fail-closed baseline. Test additions dispositioned to the two fix lanes:
backend HTTP body contract and precedence with a sort, absent-sort-on-PUT
semantics, symmetric retry replay, the repair path after an unreadable pair,
cross-Organization 404 parity with a sort, a collation pin (`amy` before
`Bob`), ties and exact-cap behaviour at the 500 boundary, NULL-heavy data
under `NULLS LAST`, a `source` clause under sort, and the service-free 503 on
the sorted path; Web out-of-order sorted responses, sort-only URL-origin 400,
actor and Organization switch, rapid double click, and realtime prefix
invalidation of a sorted key. Items the analysis confirmed as already covered
(racing saves, demoted members, member create into shared, Today parity,
CHECK constraints, repeated-parameter handling) are left as they are.

## Fix lanes

- **sort-backend-fixes:** fifteen tests added and one comment corrected, no
  production, migration or cache change: the HTTP body contract for `sort`
  (round trip, malformed and non-string shapes, 400 before 404 and before
  403, absent-sort replay), absent-sort-on-PUT semantics, symmetric retry
  replay, the repair path after an unreadable stored pair, the foreign
  sorted-list 404 parity, the `first_name` secondary key, the `created_at
  DESC` tie link, the collation pin, ties and exact-cap behaviour at 500,
  the NULL-heavy tail, a `source` clause under sort, and the service-free
  503 on the sorted path. Lane checks: `cargo fmt --all --check` and
  `clippy -D warnings` clean; targeted ignored set `db_people_sort` plus
  `db_saved_lists`: 50 passed; the service-free addition: 1 passed.
- **sort-web-fixes:** two behaviour corrections (a sort click no longer marks
  a URL-origin filter as user-origin, so it still auto-degrades; an
  unparseable non-null stored sort token now fails closed exactly like an
  unreadable filter) and sixteen test additions: the literal key shapes,
  `created.desc` normalization at page level and the click back to default,
  a sort-only 409 keeping the draft, Back/Forward with sort alone, a sort
  click triggering the filter's 422, the fail-closed stored token,
  out-of-order sorted responses, a rapid double click, a sort-only
  URL-origin 400, Organization switch on a named list, a same-actor `/me`
  refresh preserving `?sort=`, and realtime prefix invalidation of a cached
  sorted key. Lane checks: lint, typecheck, build clean; full suite 518
  passed in 40 files (from 502).

## Acceptance criteria (spec §11) mapped to evidence

| Criterion | Evidence |
|---|---|
| 1. Parsing | `person/sort.rs` unit tests: all eight tokens round-trip, malformed forms rejected, `created.desc` normalizes to none. |
| 2. Statement parity | `queries.rs::sort_sql_parity_tests` (each file minus its top-level `ORDER BY` equals the canonical text; exact ORDER BY pins); offline cache accounting above. |
| 3. Default-order parity | `db_people_sort::default_order_is_byte_identical_with_and_without_explicit_created_desc`; existing People order and cap tests untouched. |
| 4. Every key and direction | `created_key_orders_both_directions`, `name_key_orders_both_directions_with_missing_last_and_empty_string_pin`, `name_key_orders_by_first_name_as_the_secondary_key_with_missing_first_name_last`, `stage_key_orders_by_pipeline_position_both_directions`, `assignee_key_orders_both_directions_with_unassigned_last`, `non_created_keys_tie_break_by_created_at_desc_then_id_asc`, `non_created_keys_apply_created_at_desc_before_id_asc_for_name_and_stage`, `name_asc_collation_orders_amy_before_bob_not_by_byte_order`. |
| 5. Sort before truncation | `sort_applies_before_truncation_so_the_alphabetical_500_differs_from_the_newest_500`, `ties_at_the_500_boundary_drop_the_expected_single_row`, `null_heavy_tail_respects_nulls_last_and_truncates_the_oldest_null_row`. |
| 6. Composition | `sort_composes_with_a_positive_clause_me_and_empty_clauses`, `sort_composes_with_a_source_clause_binding_the_latest_source_probe`. |
| 7. Error contract | `tests/people.rs` service-free 401/503 precedence including `list_people_with_valid_sort_returns_503_when_database_unreachable`; DB-backed `malformed_sort_tokens_are_400_before_pool_acquisition`, `empty_sort_param_is_400`, `repeated_sort_param_is_400`, `foreign_organization_stage_with_a_malformed_sort_is_400_not_422`, `unknown_extra_query_param_with_a_valid_sort_is_still_200`, `sort_without_filter_records_no_filter_kinds_on_the_span`. |
| 8. Tenant isolation | `name_asc_never_returns_another_organizations_alphabetically_first_person`; `saved_list_http_hides_a_foreign_sorted_list_with_the_same_404_as_hidden_personal`. |
| 9. Saved lists | `db_saved_lists.rs`: definition/revision/no-op/default-as-NULL, replay absent vs `created.desc`, replay with the same explicit sort and the symmetric conflict, member sort-only PUT 403, bad sort 400 before 403 and before 404, non-string shapes 400, absent sort on PUT normalizes, delete clears both columns, unreadable pair fails closed on detail and count, owner PUT repairs it; `db_schema.rs` CHECK pins. |
| 10. Today | `db_today_sources::sorted_list_and_its_unsorted_duplicate_produce_identical_today_bodies` (also the unreadable-sort list still evaluates). |
| 11. Telemetry | `sort_without_filter_records_no_filter_kinds_on_the_span`; the route records the static token only. |
| 12. Web | `lib/sort.test.ts` (17), `DataTable.test.ts` (8), `queries.test.ts` key shapes (4), `events.test.ts` prefix invalidation (1), `PeopleView.test.ts` sort suite (24 plus the fix-lane additions); keyboard activation pinned through the native button; narrow layout evidenced live at 1100 and 700 px with the inspector open. |
| 13. Performance gate | Passed, archived under `docs/design/perf/slice-011b-sort-2026-09-06/`. |
| 14. Final gates and review | See "Final-tree gates" below; independent reviews and analysis recorded above. |

## Final-tree gates (coordinator, 2026-09-06 23:13–23:16 local)

Run once, in the required order, after every lane had handed back and with
no other suite, build or browser active:

| Command | Actual result |
|---|---|
| `./scripts/sqlx-prepare` | Passed (8 s); regenerated the offline cache with no change to the working tree beyond the lanes' 7 replaced and 14 added entries. |
| `./scripts/check` | Passed, 30 s: fmt, clippy `-D warnings`, production-shape check, crate fences, 689 non-ignored Rust tests, 5 doctests, Web lint, typecheck, 518 Vitest tests in 40 files, Web build, email-worker tests. |
| `./scripts/check-db` | Passed, 154 s: prepare-check clean, 459 of 459 ignored database tests (423 before this rung plus 36 new). |
| `git diff --check` | Clean. |

Runtime note: the shared development API on port 3000 has been running the
branch binary since the performance gate (`crm_dev` migrated to
`20260907000001`); after the merge it is the merged code. Nothing else was
started; the perf organization is retained as before.

## Working tree at handoff (77 paths; base `31c9980`)

Modified tracked files (44):

- `backend/crates/crm-api/src/lib.rs`
- `backend/crates/crm-api/src/routes/people.rs`
- `backend/crates/crm-api/src/routes/saved_lists.rs`
- `backend/crates/crm-api/tests/all.rs`
- `backend/crates/crm-api/tests/db_saved_lists.rs`
- `backend/crates/crm-api/tests/db_schema.rs`
- `backend/crates/crm-api/tests/db_today_source_acceptance.rs`
- `backend/crates/crm-api/tests/db_today_source_contracts.rs`
- `backend/crates/crm-api/tests/db_today_source_deadlines.rs`
- `backend/crates/crm-api/tests/db_today_source_failures.rs`
- `backend/crates/crm-api/tests/db_today_source_filter_parity.rs`
- `backend/crates/crm-api/tests/db_today_source_hooks.rs`
- `backend/crates/crm-api/tests/db_today_source_operator.rs`
- `backend/crates/crm-api/tests/db_today_source_races.rs`
- `backend/crates/crm-api/tests/db_today_source_settings.rs`
- `backend/crates/crm-api/tests/db_today_source_telemetry.rs`
- `backend/crates/crm-api/tests/db_today_sources.rs`
- `backend/crates/crm-api/tests/fixtures/today_http_perf_fixture.rs`
- `backend/crates/crm-api/tests/people.rs`
- `backend/crates/crm-app/src/domain/person/mod.rs`
- `backend/crates/crm-app/src/domain/person/queries.rs`
- `backend/crates/crm-app/src/domain/saved_list/commands.rs`
- `backend/crates/crm-app/src/domain/saved_list/queries.rs`
- `docs/decisions/DECISION_LOG.md`
- `docs/design/UI_STYLE.md`
- `docs/plans/PROJECT_STATE.md`
- `docs/plans/SLICE_011_LADDER.md`
- `docs/specs/SLICE_002.md`
- `docs/specs/SLICE_011a.md`
- `docs/specs/SLICE_011b.md`
- `docs/specs/SLICE_011c.md`
- `scripts/perf`
- `web/src/api/queries.test.ts`
- `web/src/api/queries.ts`
- `web/src/api/savedLists.test.ts`
- `web/src/api/types.ts`
- `web/src/components/DataTable.vue`
- `web/src/components/SavedListDialog.test.ts`
- `web/src/components/SavedListDialog.vue`
- `web/src/realtime/events.test.ts`
- `web/src/types/tanstack-table.d.ts`
- `web/src/views/PeopleTodaySources.test.ts`
- `web/src/views/PeopleView.test.ts`
- `web/src/views/PeopleView.vue`

Replaced offline-cache entries (7, accounted for above):

- `backend/.sqlx/query-2628c59ac453fc3ff0680777f1b2d5ce67ea16fc921bca4f6ead6e2cd05ba020.json`
- `backend/.sqlx/query-2cacd8852737787619e6b33d4f58db2b0fe1d183772c2bb8b29a875483469b04.json`
- `backend/.sqlx/query-3b7e3084a11a2d1cd5afa7f9d854ff33ab7ae5180bfd9d7538fe689cfcf81577.json`
- `backend/.sqlx/query-485c6b1e4f871ce1daf0e0be2ff3b4e26e45c0d3f8dfe849c937fb38b109dca0.json`
- `backend/.sqlx/query-69db1bb187b984b3732789fe750b62b5ec89d5e81409418f9ab8384da8724e0b.json`
- `backend/.sqlx/query-bba820aba674be5286efa61d3ca8ccadc14cc4a21199afa379442f9609e6792a.json`
- `backend/.sqlx/query-c7b1b449209d15c153fa15f58b9271b37f56988f7110671a3fd4edc9bb7296cc.json`

New files (26):

- `backend/.sqlx/query-11be9b9c9bb755a9fca1e9ec085424a8237f350f492835d3d2dc59565eab292f.json`
- `backend/.sqlx/query-17adf9b64d0a07c4baae8d88be5a14b8439f000ff7c8a72980de5dd26f32ea28.json`
- `backend/.sqlx/query-1d6a220bdde599c1e026014ceebcbcc86c3ff0af9e11c6855b240c2e979f5cd2.json`
- `backend/.sqlx/query-1dcf3fc468141cc775a641de91cc790512f895ae6136d33e50275c0c4f757ed8.json`
- `backend/.sqlx/query-25622396cad9a327562d5e642771c7c62559335d39973f2faf7ba29a0edff32f.json`
- `backend/.sqlx/query-521263a2ea3ad85783d12ce13ebd13cb0e901d221ac24dde88fc3752fb8b4065.json`
- `backend/.sqlx/query-70bd41ea6a65f69182a9d02decb9000a392e187af095b40c37f57a965a29a06a.json`
- `backend/.sqlx/query-91f46b7ec641cfafbed606c64508b3ce04e674411bb368a9d5c6b7be19d9fd2d.json`
- `backend/.sqlx/query-9cc6fed31b271f13b7073de22ee4230eba8a916357505f8dd67ccef411ed338e.json`
- `backend/.sqlx/query-a332e8a79b33b1304af7f61ab6475814e416e7e4b44823c39d2b2049d4cdec8f.json`
- `backend/.sqlx/query-b5e52eebf7eb13eac2bffcf9194edf708be1456d300a4af22a44b868b5bedf03.json`
- `backend/.sqlx/query-be054e6f0c8ab6156520f65dce5e181857ac4822244632bf87ca8c5009bdc44c.json`
- `backend/.sqlx/query-de7a8d3102577530408635c4e6829c80f6f47540cf0ded7dd7a0d1114aaf066d.json`
- `backend/.sqlx/query-f2168950a5e25a84609a1b960cac468b5a4e03f917337f856f3528102f89e340.json`
- `backend/crates/crm-api/migrations/20260907000001_saved_list_sort.sql`
- `backend/crates/crm-api/tests/db_people_sort.rs`
- `backend/crates/crm-app/src/domain/person/sort.rs`
- `backend/crates/crm-app/src/domain/person/sql/`
- `docs/design/perf/slice-011b-sort-2026-09-06/`
- `docs/design/qa/slice-011b-sort-2026-09-06/`
- `docs/specs/SLICE_011b_SORT.md`
- `docs/tasks/SLICE_011b_SORT_IMPL.md`
- `docs/tasks/SLICE_011b_SORT_VERIFICATION.md`
- `web/src/components/DataTable.test.ts`
- `web/src/lib/sort.test.ts`
- `web/src/lib/sort.ts`

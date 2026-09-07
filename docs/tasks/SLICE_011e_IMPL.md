# Slice 011e — Tags implementation brief

**Status: SPECIFICATION APPROVED 2026-09-07 (D-051 recorded); e1 awaits the
Phase 6 implementation gate.**
The [specification](../specs/SLICE_011e.md) is authoritative for every
contract; this brief sequences the work. Two rungs, **e1** then **e2**, each
one lane, one writer, one short-lived branch from `main`, merged through the
coordinator before the next starts. Model assignment follows the recorded
pattern (`docs/prompts/MODEL_ROUTING.md`): the `implement` profile writes
the lane; the coordinator (Fable) runs review, test analysis and the
once-only final-tree gates. Set the assignment and confirm the base is
current `main` when the user says start.

## Read first

AGENTS.md (§4.3, §4.6, §4.8, §11); DECISION_LOG D-004, D-005, D-007, D-019,
D-023, D-043, D-045, D-046, D-047, D-050; the 011 ladder; SLICE_011e.md in
full; SLICE_011a §4, SLICE_011b §§2–5, SLICE_011c §§3/5, SLICE_011d §2 and
§4 (command posture), SLICE_002 §§5/6, SLICE_003 §6, SLICE_005 §5;
`docs/prompts/05-implement.md`; `docs/design/UI_STYLE.md`. Then inspect the
code named below before writing. Report a contract or decision conflict to
the coordinator; never resolve it locally.

## Outcome

Any member can create, apply and remove tags on a Person; admins rename and
delete any tag and a creator renames or deletes their own unused tag
(D-051); the Person detail and Operator show tags; after e2 the filter
vocabulary has `tags` and `not_tags` everywhere it is accepted, with
`invalid_tag` flowing through saved lists, sources and system feeds like
`invalid_stage`.

## e1 — model, commands, routes, Person page, admin page

Branch `slice-011e-tags`. Owns `backend/**` and `web/**` in sequence.

Key files: `crm-api/migrations/20260909000001_tag.sql` (new);
`crm-app/src/ids.rs` (`TagId`, the `SavedListId` pattern);
new `crm-app/src/domain/tag/{mod.rs,model.rs,commands.rs,queries.rs,error.rs}`
(copy the shape of `domain/saved_list/`; advisory lock `tags:<org>` like
`saved-lists:`; admin `FOR SHARE` re-check like
`domain/today/system_feeds/commands.rs`); `crm-app/src/domain/person/queries.rs`
(`lock_person` reuse); `crm-app/src/realtime/events.rs` (`PersonChange::TagsChanged`);
`crm-api/src/error.rs` (`TagLimitReached`, `PersonTagLimitReached`,
`TagNameTaken` → 409 codes); new `crm-api/src/routes/tags.rs` and the two
person-tag routes in `routes/people.rs`; `routes/mod.rs` registration; the
detail handler's `tags` field; `crm-operator/src/views.rs` (`PersonDetail.tags`)
and `crm-api/src/operator/backend.rs`; tests: new `tests/db_tags.rs`,
extended `db_people.rs` (detail shape, People rows unchanged), `db_schema.rs`,
`db_admin.rs` (admin-route enumeration), `db_realtime.rs`, operator tests.

Web: `src/api/types.ts` (`Tag`, `TagRef`, `PersonDetailResponse.tags`),
`src/realtime/events.ts` (the closed `PersonChange` token union gains
`'tags_changed'`), `src/api/queries.ts` (tags query, mutations,
`queryKeys.tags`), `src/views/PersonDetailView.vue` (chip row, Add tag
popover), `src/components/PersonPreview.vue` (read-only chips), new
`src/views/TagsView.vue` + test (controls follow each row's `can_manage`),
`src/router.ts` (`/manage/tags`, a member route, **no** `requiresOrgAdmin`),
`src/components/AppShell.vue` (Manage entry visible to members).

Order of work, each step gated by its own tests before the next:

1. Migration, `TagId`, `db_schema.rs`; `./scripts/db-migrate` on the lane DB.
2. `tag` module: model, queries (`list_for_organization`, `list_for_person`,
   `exists`, `names_for`), five commands with the spec's limits, locks
   (advisory `tags:<org>` on create/rename/delete; tag `FOR UPDATE` on
   rename/delete; person `FOR UPDATE` and tag `FOR SHARE` on add/remove),
   the rule-1 permission check (admin, or creator with zero `person_tag`
   rows, decided under the row lock; `Forbidden` otherwise) and
   idempotency; `db_tags.rs` for spec §9.2–9.5 including the concurrent
   create, the add-racing-delete case, the creator-loses-permission-after-
   concurrent-apply case and the demoted-admin case.
3. Routes and errors (§5 table and precedence), detail `tags`,
   `TagsChanged` publish on changed rows only; `./scripts/sqlx-prepare`;
   tests §9.6–9.7 and the platform-only 401 enumeration in `db_admin.rs`
   (the tag routes are member routes and do not join the admin-route
   enumeration).
4. Operator `PersonDetail.tags` as `UntrustedText`; operator tests §9.8;
   crate fences.
5. Web types, queries, Person page chips and popover, preview chips, admin
   page, router and nav; Vitest §9.9.
6. Walkthrough §9.10 against the lane's own API; record under
   `docs/design/qa/slice-011e-<date>/`.

Rules: static SQL only, literal Organization predicates; no tombstone, no
fact table, no `PersonSummary` change; PUT not PATCH; no realtime event on
rename/delete; no tag names in logs or on the channel; D-045 controls and
UI_STYLE §5 (40px targets, accessible names). Run `./scripts/sqlx-prepare`,
`./scripts/check` and `./scripts/check-db` per round (source
`~/.nvm/nvm.sh` first in a non-interactive shell). Report actual results.

## e2 — vocabulary extension

Branch `slice-011e-tag-clauses` from `main` after e1 merges. Owns
`backend/**` and `web/**` in sequence.

Key files: `crm-app/src/domain/person/filter.rs` (two `Clause` variants,
`kind_label`, Serialize/Deserialize arms with the canonical-uuid pre-check
on `tag_ids`, `validate`, `validate_references` and
`validate_references_until`, `PersonFilterParams` + `to_query_params`,
`FilterNames.tag_names`, `describe`, `kinds_field`, unit tests);
the **fourteen** statements: `crm-app/src/domain/person/sql/filtered_summaries*.sql`
(eight), `person/queries.rs` (`count_filtered_matches`),
`crm-app/src/domain/today/{source_membership,source_candidates}.sql`, and
`crm-app/src/domain/today/system_feeds/sql/{person_state,call_membership,call_only}.sql`
(`person_state.sql` carries the chain twice, one per person-state feed) with
their bindings in `system_feeds/evaluate.rs`; `crm-api/src/error.rs`
(`InvalidTag` → 422 `invalid_tag`). Every site naming the closed
`filter_error` code set gains an explicit arm, wildcards replaced:
`domain/saved_list/error.rs`, `domain/saved_list/queries.rs`
(`SavedListFilterError` and its string form), `domain/today/model.rs`
(`TodaySourceIssueError`), `domain/today/mod.rs` (the source-issue span
`outcome` and `TodaySourceIssueError` conversion with `_ =>` arms,
`SourceEvaluation`, the during-evaluation issue path), `domain/today/sources.rs`
(`filter_error()`), `domain/today/system_feeds/{error.rs,mod.rs}`
(`InvalidTag` → `InvalidDefinition`); the two `FilterNames` loaders
(`saved_list/queries.rs`, `system_feeds/queries.rs`); tests:
`db_people_filter.rs`, `db_people_sort.rs`, `db_saved_lists.rs`,
`db_today_source_filter_parity.rs`, `db_today_sources.rs`,
`db_today_source_failures.rs`, `db_today_system_feeds.rs`,
`db_today_feed_equivalence.rs`.

Web: `src/api/types.ts` (`FilterClause` variants, `'invalid_tag'` in the
`SavedListFilterError` and `TodaySourceIssueError` unions;
`TodayFeedFilterError` aliases the first), `src/lib/filter.ts` (kinds,
labels, multi-value list, `defaultClauseFor`), `src/components/FilterBar.vue`
(multi-select editor reuse with tag options and labels, `tags` props like
`stages`), `src/views/PeopleView.vue` (pass tags to the FilterBar; the
resolvable-references check gains tags), `src/views/TodayFeedsView.vue`
(pass tags to the FilterBar), `src/views/TodayView.vue` (the `invalid_tag`
notice sentence), Vitest for each.

Order of work:

1. `filter.rs` vocabulary, validation, params, describe; unit tests.
2. The fourteen statements, `./scripts/sqlx-prepare`; parity tests per axis
   (§9.13, including feed evaluation) and semantics (§9.12, including a
   feed carrying a `tags` clause).
3. `invalid_tag` across saved lists, sources and system feeds with every
   site above; both name loaders; tests §9.14–9.16 including the
   deleted-before-enumeration and deleted-during-evaluation source cases.
4. Web chips, types, repair check and notices; Vitest §9.17.
5. Performance evidence §8 under `docs/design/perf/slice-011e-<date>/`
   (paired no-clause regression on the three named statements; two
   EXPLAINs).

Rules: predicates appended in the same position in all fourteen statements,
after the 011d predicates and before each statement's tail predicate; no
dynamic SQL; `list_summaries` untouched; no new index (the e1 primary key
and index serve the probe); AND-of-tags not built; no `_ =>` arm may absorb
`InvalidTag`.

## Coordinator

Owns this brief, the spec, the ladder, PROJECT_STATE, the D-051 entry
(recorded), amendment pointers in 002/003/005/
011a/011b/011c/011d, the once-only final-tree gates per rung, the file-list
audit against `git status` per round, the reviewer and tester runs (two
rounds maximum, D-050), and the commit and merge gates with the user.

## Checkpoints requiring the coordinator

- Any deviation from spec §§2–5 contracts (stop and report; AGENTS §11).
- e1 step 3 complete: routes live, before Web work starts.
- e2 step 2 parity results before the `invalid_tag` wiring.
- Any need to touch a file outside the rung's scope, or to add an index.

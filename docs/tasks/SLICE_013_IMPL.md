# Slice 013 — Operator `filter_people` / `run_saved_list`: implementation brief

**Status: SPECIFICATION APPROVED 2026-09-08; lane started the same day in
`../crm-worktrees/013` on `slice-013-operator-filter`.**
[SLICE_013.md](../specs/SLICE_013.md) is authoritative for every contract;
this brief sequences the work. One lane, one writer, worktree
`../crm-worktrees/013` on `slice-013-operator-filter` from `main`. A
sibling lane (Slice 012) concurrently changes the SQL text of the filter
statements and adds a migration; this lane only CALLS those statements and
must not edit them. Model assignment per `docs/prompts/MODEL_ROUTING.md`:
`implement` profile writes; the coordinator runs review, test analysis and
the once-only final-tree gates.

## Read first

AGENTS.md §5 and §11; DECISION_LOG D-028, D-029, D-034, D-046, D-048, D-050,
D-051; SLICE_013.md in full; SLICE_005 (tools, views, loop, adapter, prompt,
tests), SLICE_006b (how a tool was last added), SLICE_011a §§4/5/7,
SLICE_011b §§2/4/5, SLICE_011e §§4/5; docs/prompts/05-implement.md. Then
inspect: `crm-operator/src/{backend.rs,tools.rs,views.rs,service.rs,lib.rs}`,
`crm-operator/prompts/system.md`, `crm-operator/tests/snapshots/tool_definitions.json`,
`crm-api/src/operator/backend.rs`, `crm-api/src/routes/operator.rs`,
`crm-app/src/domain/person/filter.rs` (public clause structs,
`FilterDefinition`, `validate`, `validate_references`, `describe`,
`FilterNames`, `to_query_params`, `kinds_field`),
`crm-app/src/domain/person/queries.rs` (`filtered_summaries`,
`filtered_summaries_sorted` signatures only), `crm-app/src/domain/saved_list/queries.rs`
(`list_saved_lists`, `saved_list_detail`, `filter_names`),
`crm-app/src/domain/{stage.rs,admin/queries.rs,tag/queries.rs}` (the three
list reads), tests `crm-api/tests/db_operator.rs`, `operator.rs`,
`operator_deps.rs`, `crm-operator/src/service.rs` tests (`FakeBackend`,
scripted provider).

## Outcome

The Operator answers criteria questions ("my investors not contacted in 30
days") and runs saved lists by name through two read-only tools, resolving
names server-side, asking for clarification instead of guessing, showing the
same cards the drawer renders, with no Web change and nothing PII in
telemetry.

## Owned files

`crm-operator/src/backend.rs` (trait + input types `PeopleFilterSpec`,
`SavedListSelector`), `tools.rs` (two definitions, schemas, parsing, caps),
`views.rs` (`FilterOutcome`, `FilterResult`, `SavedListRef`), `service.rs`
(dispatch, `MAX_REFERENCES` 25, tool-aware not-found detail, fake backends),
`lib.rs`, `prompts/system.md`, `tests/snapshots/tool_definitions.json`;
`crm-api/src/operator/backend.rs` (the two adapter methods), new
`crm-api/src/operator/filter.rs` (pure name resolution over the three list
shapes, unit-tested without a database); new
`crm-api/tests/db_operator_filter.rs`. Granted touches:
`crm-api/src/routes/operator.rs` (move `auth` into the spawned task; pass it
to `SqlxToolBackend::new`), `crm-api/tests/all.rs` (register the module in
alphabetical position after `db_operator_call`, never at the end; Slice 012
registers its files in their own alphabetical positions), and, if the lane
prefers it to re-declaring, moving the `requests_json` helper from
`db_operator.rs` into `tests/common`. Nothing else.

## Order of work

1. **Contracts first**: input types, `FilterOutcome` views, the two tool
   definitions with schemas and parsing (`known_properties`,
   `parse_invocation`; `parse_limit` gains a default so absent `limit` is 10
   for these tools while search/today keep default = max), the snapshot
   update, `kind_label` coverage test, parser cap tests (spec §8.1–8.3), a
   comment in the "no trusted ids" schema test naming `list_id` beside
   `person_id`/`contact_method_id`. Fake backends (`FakeBackend` and the
   sleeping backend) implement the trait methods and their `seen`
   assertions cover them.
2. **Name resolver** (`operator/filter.rs`): exact match first, then
   case-insensitive trimmed match; stage collision fails closed as unknown;
   active members only; `me`/`unassigned` tokens win; unknown and ambiguous
   collection with echoes clipped to 80 chars and control-stripped like
   `search_people.query`; builds `FilterDefinition` from the public clause
   structs; pure unit tests.
3. **Adapter**: resolve → `validate` → `describe` → `to_query_params(actor)`
   → `filtered_summaries` or `filtered_summaries_sorted` (from
   `SavedListDetail.sort`); NO `validate_references` (spec §2); saved-list
   resolution via `list_saved_lists`/`saved_list_detail` with the request's
   `AuthContext` (moved into the spawned task in `routes/operator.rs` and
   passed to `SqlxToolBackend::new`; used only for those two reads);
   `filter_error`/no-filter → `ListInvalid`; `count`, `more_than_500`,
   `limit`; `FilterError::Database` → `Backend`; not-found handling.
   `./scripts/sqlx-prepare` (expected: no new entries).
4. **Service and prompt**: dispatch, `ledger_name` arms for both tools,
   clarification as `Ok` (resets `consecutive_malformed`), tool-aware
   not-found detail, `MAX_REFERENCES` 25 with the reference-cap test fixture
   grown to ≥ 25 ids, the span fields declared as `tracing::field::Empty`
   and recorded (spec §6), prompt rules and the "eight tools" count with its
   string-pinned test, the prompt-rule test.
5. **Database tests** `db_operator_filter.rs` for spec §8.4–8.13 including
   the D-046 matrix (own personal, shared as member and admin, another
   member's personal → not_found incl. admin, duplicate names → candidates
   → `list_id`), foreign names and ids byte-identical, `me` for two actors,
   never-contacted inclusion, caps 30/10 and 501, `invalid_tag` list,
   stored sort honoured, no `operator_proposal` row, PII-free spans and
   prompts (`requests_json`).
6. Gates: `./scripts/check` (crate fences included) and `./scripts/check-db`.

## Rules

No edits to `person/filter.rs`, `person/queries.rs`, `saved_list/**`,
migrations, `.sqlx`, or anything under `web/` (existing Vitest must pass
unchanged); no new SQL statements (the three list reads and the two filter
statements suffice); no proposal or write path; never log names, ids,
sources or day counts; keep `search_people` behaviour unchanged. Gates per
round from the worktree root in the background, polled inside one call.
Checkpoint-commit per step with the `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>` trailer; never commit on `main`, push, merge or
rewrite history; kill only exact PIDs you started.

## Checkpoints requiring the coordinator

- After step 1 (contracts and snapshot), before the adapter.
- Any need for a new SQL statement, a `types.ts` change, or a file outside
  the ownership boundary.
- Rebase onto `main` after Slice 012 merges (coordinator-initiated); re-run
  the gates afterwards.

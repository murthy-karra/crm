# Slice 016 — Tasks implementation brief

**Status: SPECIFICATION APPROVED by the user on 2026-09-09 ("commit and
then Go for 016a"); rung 016a starts the same day in
`../crm-worktrees/tasks-1` on `slice-016a-tasks`. Rung 016b awaits its own
gate after 016a merges.** The [specification](../specs/SLICE_016.md) is
authoritative for every contract; this brief sequences the work. Two
rungs, **016a** then **016b**, each one lane, one writer, one short-lived
branch from `main`, merged through the coordinator before the next starts.
Model assignment follows the recorded pattern (`docs/prompts/MODEL_ROUTING.md`):
the `implement` profile writes the lane; the coordinator (Fable) runs
review, test analysis, the once-only final-tree gates and the commit and
merge gates. **Only the coordinator runs `./scripts/check-db`.**

## Read first

AGENTS.md (§4.3, §4.6, §4.8, §9, §11); DECISION_LOG D-015, D-021, D-022,
D-023, D-027, D-029, D-033, D-050, D-051, D-053, **D-054**, O-004;
SLICE_016.md in full; SLICE_015.md §§2/3/5/6 and
docs/tasks/SLICE_015_VERIFICATION.md (the review lessons: shared recording
publisher for no-publish assertions, per-row `can_manage` assertions, mock
swaps before the click, equal-timestamp tie-break tests, capture tests
with positive controls); SLICE_002 §§2/5/6, SLICE_003 §6, SLICE_005 §5;
`docs/prompts/05-implement.md`; `docs/design/UI_STYLE.md`. Then inspect the
code named below before writing. Report a contract or decision conflict to
the coordinator; never resolve it locally.

## Outcome

Any member creates a typed, dated, assigned task on a Person; the
assignee, the creator or an admin edits, completes, reopens, snoozes or
deletes it; open tasks show on the Person page's Tasks card and in the
Operator's person view as untrusted titles; completed tasks show in the
timeline; a `task_changed` event keeps other tabs current. After 016b, due
tasks reach Today as ranked reasons and as the Tasks panel.

## 016a — model, commands, routes, Person page, Operator read

Branch `slice-016a-tasks`. Owns `backend/**` and `web/**` in sequence;
nothing under `docs/` except the QA record and this brief's status line,
which is coordinator-owned.

Key files (backend, `backend/crates/`):

- `crm-api/migrations/20260913000001_task.sql` (new; spec §2 verbatim,
  both indexes).
- `crm-app/src/ids.rs` (`TaskId`, the `NoteId` pattern).
- New `crm-app/src/domain/task/{mod.rs,model.rs,commands.rs,queries.rs,error.rs}`
  (copy the shape of `domain/note/`; `TaskTitle::parse` pure in `model.rs`;
  `TaskKind` closed enum with `as_str`/`from_db_str`; `lock_current_membership`
  duplicated from `note::commands` or lifted, lane's choice; a `FOR SHARE`
  read of a supplied assignee's membership requiring `status = 'active'`;
  `SnoozeTask` implemented as `UpdateTask` with one field; command structs
  carrying a title have no `Debug` derive; `Task` has a redacting `Debug`).
- `crm-app/src/domain/mod.rs`; `domain/person/queries.rs`
  (`task_completed` history entries, rank 8, detail
  `{title, kind, due_at, assignee, created_by, can_manage: false}`;
  `history_for_person` gains the ninth source); `realtime/events.rs`
  (`PersonChange::TaskChanged`).
- New `crm-api/src/routes/tasks.rs` (six routes; `PersonIdPath` on POST,
  `PersonTaskIdsPath` pair on the rest; per-route `DefaultBodyLimit` on
  POST and PUT; `deny_unknown_fields`), `routes/mod.rs`, `lib.rs`,
  `routes/people.rs` (detail `tasks[]` with `can_manage`; the
  `task_completed` `can_manage` overwrite from `AuthContext.role` and the
  detail's `assignee.id` / `created_by.id`), `src/error.rs`
  (`TaskError → ApiError`; `InvalidAssignee` reused; no new code).
- `crm-operator/src/views.rs` (`TaskView`, `PersonDetail.tasks`, redacting
  `Debug`), `crm-operator/prompts/system.md` ("task titles" in the
  untrusted-text parenthetical), `crm-api/src/operator/backend.rs`
  (`open_for_person` → `tasks`, ten cap in the `open_for_person` order;
  filter `kind == "task_completed"` out of `history` before `MAX_HISTORY`;
  no `history_detail` arm).
- Tests: new `crm-api/tests/db_tasks.rs` (registered alphabetically in
  `tests/all.rs`), extended `db_schema.rs`, `db_admin.rs` (401 enumeration
  with explicit POST/PUT/DELETE), `db_people.rs`, `db_realtime.rs`,
  `db_operator.rs` (eleven-task cap test, sentinels, the `CaptureWriter`
  capture test with positive controls and sentinels in the rejected title
  and the 422 body).

Key files (web, `web/src/`): `api/types.ts` (`Task`, `TaskKind`,
`TaskCompletedDetail`, the `HistoryEntry` arm, `'task_changed'`),
`api/queries.ts` (six mutations keyed with `personMutationKey`;
`settlePersonMutation` gains an array form; settle on
`[queryKeys.person, queryKeys.today]` in 016a, the tasks prefix joins in
016b), `realtime/events.ts` (`task_changed` → person + today only),
`lib/errors.ts` (task copy: `malformed_request`, `invalid_assignee` "That
member is not active", `forbidden`), `views/PersonDetailView.vue` and its
test (Tasks card above History, add form, per-row Complete/Edit/Delete only
where `can_manage`, inline edit, ConfirmDialog, completed rows with Reopen,
date-only → local end of day, an untouched date re-sends the stored
instant), `components/PersonPreview.vue` (compile touch only).

Order of work, each step gated by its own tests before the next:

1. Migration, `TaskId`, `TaskKind`, `db_schema.rs`; spec §12.1 including
   the CHECK matrix, composite-FK rejections, the partial unique index, the
   cascade and the 500 four-byte-code-point acceptance.
2. `task` module: `TaskTitle::parse` with unit tests (§12.2); queries
   (`open_for_person`, `completed_history`); six commands with the spec's
   lock order (person `FOR UPDATE` → task `FOR UPDATE` → own membership
   `FOR SHARE` → permission → changed-assignee `FOR SHARE` check), the
   `changed: false` paths, the tombstone, publication only on changed
   writes; `db_tasks.rs` for §12.3–12.5 including the in-flight
   deactivation on a second connection, both `tokio::join!` races, the
   same-Organization mismatched path 404s, the multi-membership 404, the
   assignee-who-reassigns case, the completed-task update and snooze rules,
   and the rank-8 tie-break with equal explicit timestamps.
3. Routes and errors (§4 table and precedence, 403 before 422), detail
   `tasks[]`, the `task_completed` kind with the route overwrite,
   `TaskChanged`; `./scripts/sqlx-prepare`; tests §12.6 and the 401
   enumeration. **Checkpoint: report to the coordinator when the routes are
   live, before Web work starts** (end your turn with the report; the
   coordinator resumes you).
4. Operator `PersonDetail.tasks`, history filter, prompt parenthetical;
   §12.7 including the capture test; crate fences.
5. Web types, queries, realtime branch, Person page Tasks card and
   completed rows; Vitest §12.8 (fixed `TZ` for the date tests).
6. Walkthrough (§12.17's 016a portion: create, edit, complete from the
   Person page, reopen, third-member read-only, admin reassign from a
   deactivated member, second Organization sees nothing, Operator lists the
   task) against the lane's own API and Web from the worktree (never ports
   3000/5173, never `crm_dev`; database `crm_slice016_qa` created on the dev
   Postgres as the `crm_dev` role via `docker exec development-postgres-1
   psql -U crm_dev`, migrated with the worktree's `./scripts/db-migrate`
   under env overrides, API on `127.0.0.1:31016`, Web on `51016`, seeded
   through the HTTP API); record under `docs/design/qa/slice-016a-<date>/`
   with screenshots taken into the worktree (the Playwright MCP server's
   working directory is the main checkout; move files and delete the stray
   `.playwright-mcp/` directory there). LiveKit is down: place no calls.

Rules: static SQL only, literal Organization predicates; the task lookup
binds `id`, `organization_id` **and** `person_id`; no fact table, no
`person.updated_at` bump, no D-052 column, no filter clause, no Today
change in 016a, no `PersonSummary` change; PUT not PATCH; tombstone not
hard delete; `task_changed` only on changed writes; **no title in spans
(all six `skip_all`), logs, error envelopes, the realtime payload or the
ledger**; D-045 controls and UI_STYLE §5 (40 px targets, accessible
names); pessimistic keyed mutations.

A checkpoint commit per step with the `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>` trailer. Per round, under the shared gate lock
(`mkdir /private/tmp/claude-501/crm-gate.lock`, `rmdir` after, also on
failure): `source ~/.nvm/nvm.sh; ./scripts/sqlx-prepare && ./scripts/check`,
started in the background and **polled inside one bounded call** (a loop of
`sleep 15` and a grep of the log, up to 40 times) so the turn never yields
mid-run. **Never run `./scripts/check-db`** (the coordinator runs it; two
overlapping db-backed runs collide on test-database names). Never bind
ports 5173 or 3000; never `pkill -f`; kill only exact PIDs you started;
never commit on `main`, push, merge or rewrite history; never claim a check
passed unless you saw it. Report the changed-file list reconciled against
`git status` and `git diff --stat main..HEAD`.

## 016b — Today (after 016a merges; own gate)

Branch `slice-016b-today` from `main`. Owns `backend/**` and `web/**` in
sequence. Key files: `crm-app/src/domain/today/{mod,model}.rs` (the axis
after `evaluate_feeds_builtins` and the unrecoverable check, before
`builtin_ids`; own savepoint and `SOURCE_BUDGET`; recovery seeding
`final_recovery_deadline`; the three `TodayQueryPhase` values; all-or-
nothing; `TaskOverdue`/`TaskDue` reasons with redacting `Debug`; tier
raise and ordering; `truncated_task`), `today/sql/task_*.sql` (new; `now`
bound as a parameter), `domain/task/queries.rs` (`open_for_assignee`),
`crm-api/src/routes/tasks.rs` (`GET /api/tasks?scope=mine`, fail closed),
`crm-api/src/operator/explain.rs` (`reason_text` fixed lines,
`reasons_json` untrusted arms), `crm-operator/src/views.rs`
(`ORDERING_RULE` clause), tests `db_today_task_axis.rs` (new),
`db_today_builtin_parity.rs`, `db_today_source_failures.rs` (phases), the
Operator Today tests and a capture test; Web `TodayView.vue` (+test),
`api/{types,queries}.ts` (`TaskWithPerson`, `TodayFeedKey | 'task_due'`,
`queryKeys.tasks(orgId, actorId)`, settle array incl. the tasks prefix),
`lib/todayFeeds.ts` (label and generic fallback), `realtime/events.ts`
(tasks prefix). Order: axis statements and parity → merge, tiers, order,
truncation → failure phases → `GET /api/tasks` → Operator → Web → §11
evidence → walkthrough §12.17. Not started until the user approves the
016b gate.

## Coordinator

Owns this brief, the spec, PROJECT_STATE, D-054 (recorded), the amendment
pointers (016a: SLICE_002 §§2/5, SLICE_003 §6, SLICE_005 §5; 016b: 003 §3,
011c §5, 011d §§1/5/6), the once-only final-tree gates per rung, the
file-list audit against `git status` per round, the reviewer and tester
runs (two rounds maximum per rung, D-050), and the commit and merge gates
with the user.

## Checkpoints requiring the coordinator

- Any deviation from spec §§2–4, 6, 7 contracts (stop and report; AGENTS §11).
- 016a step 3 complete: routes live, before Web work starts.
- Any need to touch a file outside the rung's scope, to add an index, or to
  change an existing statement's text.

# Slice 016a (Tasks: model, commands, routes, Person page) — Verification record

Coordinator-owned evidence for [SLICE_016.md](../specs/SLICE_016.md) rung
016a and the [implementation brief](SLICE_016_IMPL.md) under the D-050
budget. Branch `slice-016a-tasks` from `main` at `a5c8758`, worktree
`../crm-worktrees/tasks-1`, one lane (Claude Sonnet 5, `implement`
profile), coordinated by Claude Fable 5.1 on 2026-09-09.

| Commit | Step | Content |
|---|---|---|
| `7388106` | 1 | Migration `20260913000001_task.sql` byte-faithful to spec §2 (nine CHECKs, the Person composite FK with `ON DELETE CASCADE`, four membership composite FKs without cascade, the Person index, the partial assignee/due index for Today, the partial unique import key, `GRANT SELECT, INSERT, UPDATE` with no `DELETE`); `TaskId`; `db_schema.rs` CHECK matrix, composite-FK rejections, cascade and 500-astral-code-point acceptance, resurrection guard, grants and indexes. |
| `7c2fd8b` | 2 | `crm-app/src/domain/task/` (model with `TaskTitle::parse` and the closed `TaskKind`, queries, six commands, error). Lock order person `FOR UPDATE` → task `FOR UPDATE` bound on `id`, `organization_id`, `person_id`, `deleted_at IS NULL` → own membership `FOR SHARE` → rule-1 verdict (admin, assignee or creator) → changed-assignee active-membership `FOR SHARE` check, so 403 precedes 422; `UpdateTask` full replace of four fields never touching the completion columns; complete/reopen/snooze target-state idempotent, snooze on a completed task `changed: false`; tombstone touching exactly three columns; `PersonChange::TaskChanged` after commit on changing writes only; `task_completed` history kind (rank 8, at `completed_at`, detail with `assignee`, `created_by`); no `Debug` on title-carrying commands, redacting `Debug` on `Task`; spans `skip_all`. |
| `4978c2f` | 3 | `routes/tasks.rs` (six routes; `PersonIdPath` on POST, `PersonTaskIdsPath` pair elsewhere; per-route 128 KiB limit on POST, PUT and snooze; `deny_unknown_fields`; JSON rejection mapped without its message); `TaskError → ApiError` reusing `invalid_assignee`; the detail's `tasks[]` and the `task_completed` `can_manage` overwrite from the viewer's role and the row's assignee/creator ids. `db_admin.rs` 401 enumeration with explicit POST/PUT/DELETE; `db_realtime.rs` payload-has-no-title pin; `db_people.rs` People rows, `person.updated_at`, D-052 columns and Today untouched. |
| `90a3726` | 4 | Operator `PersonDetail.tasks` (open, at most ten, `open_for_person` order, `UntrustedText`); `history` filters `task_completed` before `MAX_HISTORY`; no `history_detail` arm; prompt parenthetical gains "task titles". `db_operator.rs` eleven-task cap test in both shapes, sentinel test, `CaptureWriter` capture test across all six commands, a rejected title, a 422 body, a 403, a 404 and the tool call, with positive controls. |
| `f8bd4d3` | 5 | Web: `Task`, `TaskKind`, `TaskCompletedDetail`, the `HistoryEntry` arm, `'task_changed'` → person and today only; six pessimistic mutations keyed with `personMutationKey`, settled through the new array form of `settlePersonMutation`; task error copy; a nine-line `formatDateOnly` helper; the Tasks card above History (Add form with kind default, active-only assignee picker defaulting to the viewer, date-only to local end of day client-side, untouched dates re-sent verbatim; per-row Complete/Edit/Delete only where `can_manage`; inline Edit with an inert Cancel/Escape while pending; ConfirmDialog with the exact copy); completed rows in History with Reopen; `PersonPreview.vue` narrowing. |
| `83a5542` | 6 | Walkthrough screenshots (ten) under `docs/design/qa/slice-016a-2026-09-09/`; the written record was added by the coordinator. |
| `2c19a8d` | round 1 | Backend fixes (below). |
| `b06fa3e` | round 2 | Web fixes and test tightenings (below). |

## Lane gates (own tree, per round)

At `90a3726`: `sqlx-prepare && check` green (773 Rust, 677 Vitest, 33 s);
the coordinator's `check-db` 677 of 677 (249 s; thirty new DB tests). The
lane wrote steps 1–4 before its first gate and then split the work into four
step-scoped commits (a process deviation, recorded; the final gate covers
everything). Two bugs the lane caught in its own targeted runs before
landing: a schema test that set `deleted_by_user_id = NULL` on a tombstone
(its own CHECK), and cap-test sentinels that were substrings of each other
plus a NULL-due batch whose explicit `created_at` raced the `updated_at`
default.

## Review round 1 (of two), backend on `90a3726`

Reviewer READY WITH FIXES; tester READY WITH FIXES. Verified by both:
migration byte-faithful; validator strictly narrower than the CHECK; lock
order and the three-column lookup; 403 before 422; full-replace update
never touching completion; idempotent complete/reopen/snooze; tombstone
snapshot; publish-after-commit only on changing writes; ids-only realtime;
`skip_all` spans; Operator filter before truncation; real Organization-B
rows and the multi-membership case in the tenant tests; no lock cycle
against `SetMemberStatus`. Findings applied in the round-1 batch:

- **defect (contract):** the create receipt echoed the caller's `due_at`
  rather than the stored microsecond-truncated instant, so a client
  re-sending the receipt would get `changed: true`; three assertions
  passed only because this machine's clock has microsecond resolution. The
  insert now returns the stored values and the tests use a
  microsecond-truncated helper.
- **spec invariant:** `CreateTask` did not re-read the actor's own
  membership `FOR SHARE`; it now does, as the other five commands do, so a
  deactivated actor gets 403 with no row.
- **TRUST:** `TaskView` derived `Debug` and would print a title through a
  `PersonDetail` dump; it now carries a redacting impl.
- tests: per-viewer `can_manage` inside each detail row and each history
  entry with a plain-member assignee; publication counts and the
  `task_changed` variant on every changing write incl. delete, and
  none on the 403/422/in-flight paths; third-member 403 on complete,
  reopen, snooze and delete with row and publisher snapshots; `due_at`
  offset normalisation, date-only 400, kind case/null/unknown/omitted;
  route precedence (malformed uuid before 401, malformed body before 404,
  unknown fields on PUT and snooze); positive controls in the
  Person-untouched test; cap ordering and the imported-shape view fields;
  complete-twice snapshot with a second actor; imported-row admin
  reassignment; update-level 422 for an inactive and a foreign assignee.

Accepted as recorded: the create path's default-to-actor assignee is not
separately validated (the actor's own re-read now covers it); two
`too_many_arguments` allows on a ten- and an eight-argument function (the
`create_person` precedent); complete and reopen leave `updated_at`
untouched (completion is not an edit; the Web must not rely on
`updated_at` moving).

## Review round 2 (of two), Web on `f8bd4d3`

Reviewer READY WITH FIXES; tester "not ready" on the same items, all
small. Verified by both: types match §4; `invalidationsFor` gives
`task_changed` person and today only; six pessimistic keyed mutations
settled through the array form on person and today; the Tasks card and Add
form with the active-only assignee picker, date-only to local end of day
under a pinned `TZ` (both spec instants, also green under Asia/Kolkata),
untouched dates re-sent verbatim, per-row `can_manage` gating, ConfirmDialog
copy and pending state, completed rows with Reopen, text-only rendering,
no new colours; `PersonPreview.vue` renders no tasks. Findings applied in
the round-2 batch:

- **defect (contract):** a Reopen failure had no place to render (the
  error slot lived only in the Tasks-card row); the History row now shows
  it.
- **defect (SEEN_HERE):** Complete and Reopen had no pending guard, so a
  double click before re-render sent two requests; both handlers now
  return while pending.
- **defect (contract):** editing an imported task with no assignee posted
  `assignee_user_id: ""` (a 400 with the wrong copy); the edit form now
  defaults to the viewer and shows an inactive current assignee as an
  "(inactive)" option.
- Save 404 while editing: the editor unmounted with a one-macrotask flash;
  now a dismissible alert "This task was deleted by someone else." above
  the card (the draft is not preserved; titles are short; the coordinator's
  D-050 choice against porting the note editor's gone panel).
- tests: the `task_changed` Vitest with full-array equality; the add-422
  members refetch asserted by count growth; untouched Save asserting the
  returned `changed: false`; edit-422, Save-403 with the `can_manage`
  flip; date + time, Edit prefill, cleared date, and the local-midnight
  badge boundary; mixed-History per-row Reopen; null assignee rendering.

Accepted as recorded: the four "one primary" note assertions now exclude
the task Add button alongside the note composer (a two-testid filter, the
015 posture); `web/src/lib/format.ts` gained a nine-line date helper
outside the brief's list; four unrelated tests gained `tasks: []` because
the detail response field is non-optional.

## Confirmation on `b06fa3e` (read-only, both rounds spent)

Every production fix landed and no scope leaked (`2c19a8d`: the task
commands and queries, the Operator view, one `.sqlx` entry and tests;
`b06fa3e`: the Person page and its tests only). Seven consecutive runs of
the Person-page and realtime Vitest files: 116 of 116 each, no flake.
Findings, handled without a third round:

- **corrected by the coordinator (test-only):** the `due_in` test helper
  returned a nanosecond instant while Postgres stores microseconds, so one
  assertion comparing a stored `due_at` to the local value passed only on
  this machine's microsecond clock; the helper now truncates to
  microseconds.
- **recorded LATER, the lane's round-2 report overclaimed:** no Vitest
  exercises the new Reopen error slot (no `taskReopen` error case), the
  `isPending` guards on Complete and Reopen (no double-dispatch test), or
  the edit form's null-assignee default and "(inactive)" option (the
  existing assertion uses a fixture already assigned to the viewer). The
  production lines are trivially safe and verified by reading.
- **recorded LATER:** on a Save 403 whose refetch flips `can_manage`, the
  task editor closes in the same tick, so the explanation and the draft
  vanish together (the note editor stays open with its error); the test's
  GET release gate is what lets the assertion see the copy. A one-branch
  change if a later round exists.
- **recorded LATER:** the due badge reads the clock non-reactively, so a
  tab open across local midnight shows stale badges until re-render;
  `editingTask` can outlive its unmounted form when another actor completes
  the task mid-edit (a stale draft resurfaces on reopen).

## Final-tree gates (coordinator, once)

Run by the coordinator from the worktree root under the shared gate lock,
in one sequence, each once, on the final tree (the record commit on top
of `b06fa3e` plus the microsecond-truncation test fix), 2026-09-09; an
identical sequence had already passed on `b06fa3e` itself (773 / 703 /
684):

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; no metadata change (the eleven new statements were already cached) |
| `./scripts/check` | all checks passed, 33 s: fmt, clippy, cargo check, crate fences, **773** Rust tests, doc tests, Web lint/typecheck/**703** Vitest (48 files)/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 231 s: **684 of 684** DB-backed tests on the first run (thirty-seven more than `main`'s 647); neither known flake occurred |

## Recorded LATER (D-050)

- `is_control` does not reject U+2028/U+2029 (line and paragraph
  separators) or zero-width-only titles; the CHECK forbids only `\n`; the
  Web input element cannot produce them.
- `TaskError::Database` derives `Debug` (the 015 reasoning: unreachable via
  the validator).
- The ledger sentinel assertion is vacuous by schema.
- Composite-FK rejections assert `is_err()` rather than SQLSTATE `23503`.
- Concurrent snoozes, update-vs-complete and note-vs-task commands on one
  Person are serialised by `lock_person` (unreachable as distinct outcomes).
- `{history:?}` in test failure messages prints titles (test-only).
- Rung 016b (the Today axis and panel) and everything in spec §1's LATER
  list.

## Merge readiness

Source `slice-016a-tasks` at its head (the `due_in` fix, the walkthrough
record and this record on top of `b06fa3e`); destination `main` (no commits since the branch base `a5c8758`, so
no conflict is possible). Migration impact: one additive migration
`20260913000001_task.sql` (one table, three indexes, grants; no data
change); `crm_dev` must be migrated with `./scripts/db-migrate` after the
merge, with the user's approval; the dev API must be restarted (new routes
and the Operator view) and `dev-web-prod` rebuilt and relaunched for the
Web changes. The `crm_slice016_qa` database is left on the dev Postgres and
can be dropped afterwards. Unresolved risks: the LATER list above. Rung
016b (the Today axis and panel) needs its own gate. Push and deployment are
not authorized by this record.

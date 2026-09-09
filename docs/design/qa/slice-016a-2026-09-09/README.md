# Slice 016a (Tasks) — live walkthrough (spec §12.17, 016a portion)

Recorded 2026-09-09 by the implementation lane against its own API
(`127.0.0.1:31016`) and Web dev server (`127.0.0.1:51016`), driven with the
Playwright MCP browser tools; never the shared dev runtime, never ports
3000/5173. Database: a fresh `crm_slice016_qa` on the dev Postgres, migrated
with the worktree's `./scripts/db-migrate` under env overrides, seeded
through `scripts/seed_dev.py` against the QA API plus one hand-invited and
then deactivated Acme member for the reassignment scenario (no direct
database writes, D-021). LiveKit is down; the Call button was never used.
The written record below was assembled by the coordinator from the lane's
report and the screenshots (the lane committed the screenshots only).

Person used: **Grace Hopper** (`grace-016a@example.test`) in Acme Realty.

| # | Scenario | Result | Evidence |
|---|---|---|---|
| 1 | Alice signs in | PASS | `01-login.png` |
| 2 | Alice creates a task on the Person (title, kind, due today, assignee defaulting to herself) | PASS | `02-task-created.png` |
| 3 | Alice edits the task | PASS | `03-task-edited.png` |
| 4 | Alice completes it from the Person page; the Tasks card reads "No open tasks." and the task appears in History as completed | PASS | `04-task-completed-in-history.png` (the card state is in frame; the History row is below the fold of that capture and is shown reopened in the next) |
| 5 | Alice reopens it from History; it returns to the Tasks card | PASS | `05-task-reopened.png` |
| 6 | Carol (neither assignee nor creator) sees the task without Complete, Edit or Delete | PASS | `06-carol-read-only.png` |
| 7 | The admin reassigns a task held by a deactivated member to Carol | PASS | `07-reassigned-to-carol.png` |
| 8 | Bob, in a second Organization, sees nothing for the Person id | PASS | `08-cross-org-isolation.png` |
| 9 | The Operator lists the open tasks by title | PASS on the second phrasing | `09-operator-lists-task.png` (asked "What tasks are open for Grace Hopper?", the model answered about her Today item and did not list tasks), `09b-operator-lists-task-followup.png` (asked "List Grace Hopper's open tasks by title.", it listed both open titles) |

## Observations

- Row 9: the `tasks` field reached the model in both turns (the second
  answer quotes both titles); the first answer chose to summarise the Today
  item instead. That is model routing behaviour on a loosely phrased
  question, not a Tasks data or contract defect. Recorded for whoever next
  tunes the Operator prompt.
- Reopening removes the `task_completed` History entry rather than adding a
  reopen entry (spec §1 rule 5, by design).
- Today was not exercised: rung 016a leaves Today untouched; the task
  reasons and the Tasks panel are rung 016b.
- The QA API and Web processes were stopped by their exact PIDs; the
  `crm_slice016_qa` database is left for the coordinator.

# Slice 016b (Tasks on Today) — live walkthrough (spec §12.17)

Recorded 2026-09-09 by the implementation lane against its own API
(`127.0.0.1:31016`) and Web dev server (`127.0.0.1:51016`), driven with the
Playwright MCP browser tools; never the shared dev runtime, never ports
3000/5173. Database: a fresh `crm_slice016b_qa` on the dev Postgres
(`ALTER DATABASE ... OWNER TO crm_migrator` first, matching
`infra/development/postgres/provision-roles.sql`, since Postgres 15+
restricts `CREATE` on schema `public` to the DB owner), migrated with the
worktree's `./scripts/db-migrate` under env overrides, seeded through
`scripts/seed_dev.py` against the QA API plus one hand-invited and then
deactivated Acme member (Eve Evans) for the reassignment scenario — no
direct database writes (D-021). LiveKit is down; the Call button was never
used. The QA API and Web processes were stopped by their exact PIDs
(42948, 43375); `crm_slice016b_qa` is left for the coordinator.

Person used: **Grace Hopper** (`grace-016b@example.test`) in Acme Realty.
Second-Organization check used **Bob Baker** (`bob@best.test`) in Best
Realty.

| # | Scenario (spec §12.17) | Result | Evidence |
|---|---|---|---|
| 1 | Alice creates two tasks on Grace Hopper: one due today, one due next week | PASS | `01-tasks-created-on-person.png` |
| 2 | Today shows Grace Hopper with the `task_due` reason/badge; the Tasks panel lists the first (due-today) task only | PASS | `02-today-badge-and-panel.png` |
| 3 | Alice snoozes the due-today task to "tomorrow end of day" | PASS WITH OBSERVATION (see below) | `03-after-snooze-to-tomorrow.png`, `03b-person-page-task-snoozed-to-tomorrow.png` |
| 4 | Alice completes a task from Today; it leaves both the Tasks panel and the ranked queue and appears in History as completed | PASS (via a substitute task; see Observations) | `04-completed-from-today-in-history.png` |
| 5 | Carol (not assignee, not creator) sees the task on the Person page without Complete/Edit/Delete controls | PASS | `05-carol-read-only.png` |
| 6 | The admin deactivates Eve (who holds a task on Grace Hopper) and reassigns that task to Carol; it appears on Carol's Today | PASS | `06a-eve-deactivated.png`, `06b-task-reassigned-to-carol.png`, `06c-carol-today-reassigned-task.png` |
| 7 | Bob, in a second Organization (Best Realty), sees nothing for Grace Hopper's Person id | PASS | `07-bob-isolation-person-not-found.png` |
| 8 | The Operator lists the open task and explains the Today reason | PASS (natural-language reply references the title; see Observations) | `08-operator-explains-task-reason.png` |

## Observations

- **Scenario 3, snooze vs. the 24-hour admission window (not a defect):**
  snoozing the due-today task to "tomorrow end of day" while real wall-clock
  time was ~09:00 America/Los_Angeles put the new `due_at` (tomorrow
  23:59:59 local) roughly 39 hours out — outside the Today axis's `due_at
  <= now + 24h` admission window (spec §1). The task correctly disappeared
  from both the ranked queue's task reason and the Tasks panel once
  snoozed; the Person page confirmed the underlying due-date mutation was
  correct (moved to Sep 10, 2026). This is exactly the spec-correct,
  already-unit-tested boundary behaviour, not a bug — but it means the
  literal walkthrough wording ("it stays in the panel's Due soon group...
  and stays on Today") does not hold whenever the snooze target lands
  outside the 24h window relative to the current wall clock, which depends
  on what time of day the walkthrough is run. Recorded here honestly per
  the 016a QA record's own precedent of noting nuances rather than
  smoothing over them.
- **Scenario 4, substitute task:** because the snoozed task from Scenario 3
  was no longer on Today, a third task (due today) was created on Grace
  Hopper specifically to demonstrate "complete from Today" faithfully. That
  task was completed via the Today ranked queue's Complete button; it left
  the Tasks panel and the ranked row's task reason, and a `task_completed`
  History entry appeared on the Person page.
- **Scenario 6, task assignee vs. Person assignee:** the reassignment target
  is the *task's* `assignee_user_id` (the axis membership key), not the
  Person's own assignee — Grace Hopper's Person-level assignee stayed Alice
  throughout. The task edit form correctly rendered Eve's option as "Eve
  Evans (inactive)" after deactivation (016a's Edit-form precedent), and
  reassigning it to Carol Chen made the task (and its `task_due` reason,
  priority Normal) appear on Carol's Today immediately.
- **Scenario 8, title in the Operator's reply (not a defect):** the fixed,
  templated `explain_priority` tool output (`reason_text`) is asserted at
  the Rust unit-test level to never embed the task title — only
  `reasons_json` carries it, tagged `UntrustedText` (spec §12.14). The
  Operator's own free-form natural-language *reply* to "Why is Grace Hopper
  on my Today list?" did quote the title ("a follow‑up task titled
  'Prepare the closing checklist'"), because the LLM is allowed to
  synthesize prose from the sanitized untrusted data it's given — that is
  the intended design (safe-but-untrusted data, not the fixed explanation
  line itself), and the response's `tool_calls` show `search_people` then
  `explain_priority` both succeeding rather than the model inventing data.
- The QA API and Web processes were stopped by their exact PIDs; the
  `crm_slice016b_qa` database is left for the coordinator.

## Addendum: Today observed on the shared development runtime (coordinator, 2026-09-09)

After the merge (`faa2878`), the dev API restart and the `dev-web-prod`
rebuild, the coordinator signed in as Alice at `127.0.0.1:5173`, created a
task due today on Skip Carolson (a seeded fixture) from the Person page,
and opened Today: the Tasks panel listed it under Due soon with the kind
label, the title, 11:59 PM, Complete and Tomorrow
(`screenshots/09-today-panel-dev-runtime.png`; the group headings are no
longer uppercase after round 2), and the ranked queue carried the task
reason. Completing it from the panel removed it from Today after the
refetch. The test task is completed, not deleted; it is visible in the
Person's History.


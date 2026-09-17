# Stateful Web E2E: tasks and follow-up

Implements catalog 06a–06c as one dependent ten-step family, `tasks-v1`.
Authority: AGENTS.md, D-015, D-023, D-050, D-054 and SLICE_016.
No application code or shared contracts change.

The common operational seed provisions two Organizations and four members through
real administration commands. A single inquiry creates an Alice-owned Person;
no tasks exist initially. The seed uses no business SQL writes.

Admin creates overdue work assigned to Blair from the Person page. Blair's Today
receives the real Centrifugo invalidation and authoritative task refetch. Alice
can read the Person but cannot manage this task. Forbidden same-Organization and
foreign-Organization writes, and foreign assignee creation, leave state intact.

Blair completes from Today; the Person timeline gains a completion. Admin reopens
it; the timeline entry disappears and Blair's work returns, including after reload.
Repeated completion/reopen/snooze and completed-task snooze verify target-state
idempotency. Snooze asserts tomorrow at local end of day and computes actual
rolling 24-hour eligibility from the API's query timestamp. Blair edits and
transfers to Alice, loses management permission, and Alice receives the task.
Alice completes it while her independent unanswered-inquiry reason remains.
Admin reopens and deletes; all reads lose the task content and PostgreSQL retains
the empty-title tombstone and deleter attribution.

Read-only database assertions preserve Person ownership/activity columns across
all task mutations. Browser traces, videos, HTTP, DB, realtime, and mock evidence
use the shared harness. Final exact data canaries and Organization-only channel
checks feed parallel-isolation verification; external request count must be zero.

Run: `./scripts/e2e --family tasks`. The coordinator owns image builds and parallel
execution evidence; syntax checks passed during authoring. Initial runtime run
`185fc6511fa6` passed all ten steps with cleanup `verified_empty`; the relationships
and lists families ran concurrently (their initial authoring failures are retained
in that run). A bounded independent read-only review found no concrete issues in
the tasks family or common operational seed. Final combined-suite evidence follows
below.

Limits: this family does not cover Operator task proposals/Undo, reminders,
imported unassigned tasks, deactivated assignee recovery, no-due/future task forms,
performance limits, exact clock-edge ties or transaction races. Existing domain
and component tests remain responsible for those cases; this is stateful user
journey coverage, not a replacement for them.

## Final combined verification

Run `949f3e952fb3` passed all six families (61 steps), with three browser families
active simultaneously, all images reused and every environment removed. See
[expansion verification](WEB_E2E_EXPANSION.md) for authoritative evidence and the
shared recorder fixes discovered during parallel execution.

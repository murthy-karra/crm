# Slice 016b (Tasks on Today: the built-in axis and the Tasks panel) — Verification record

Coordinator-owned evidence for [SLICE_016.md](../specs/SLICE_016.md) rung
016b and the [implementation brief](SLICE_016_IMPL.md) §016b under the
D-050 budget. Branch `slice-016b-today` from `main` at `a5b301e` (which
contains rung 016a), worktree `../crm-worktrees/tasks-2`, one lane (Claude
Sonnet 5, `implement` profile), coordinated by Claude Fable 5.1 on
2026-09-09.

| Commit | Step | Content |
|---|---|---|
| `973aa21` | axis | `today/sql/task_membership.sql` (statement a) and `task_only.sql` (statement b), bound by the new `today/task_axis.rs`, wired into `today::query` after `evaluate_feeds_builtins` and the call-feed unrecoverable check and before `builtin_ids` and the list-source `k`, on its own `SAVEPOINT today_task_axis` and `SOURCE_BUDGET`, the same statement-timeout discipline, rollback grace seeding `final_recovery_deadline`, unrecoverable handling mirroring the call feed; `TodayReason::TaskOverdue`/`TaskDue` with a redacting `Debug` on the whole enum; task-only items with `waiting_since: null`; the tier raise (`normal` → `high` on overdue, `low` untouched); reason placement (list reasons before task reasons before `call_outcome_needed`); `builtin_truncated` composition; the three `TodayQueryPhase` values; all-or-nothing failure with the `task_due` issue token; telemetry `task_candidate_count`/`task_axis_ms`. The five pre-existing Today statements byte-identical to `main` (sha256 in the perf archive; confirmed by the reviewer's diff). |
| `126abfd` | panel read, Operator | `open_for_assignee`, `TaskWithPerson`/`PersonRef`, `GET /api/tasks?scope=mine` failing closed on every other query shape, fetch 201 return 200; `reason_text` fixed lines from `due_at` and `kind` (absolute RFC 3339, the sibling arms' convention), `reasons_json` carrying the title as `UntrustedText`, `ORDERING_RULE` clause verbatim. |
| `bd8b9e7` | §11 evidence | The `EXPLAIN` gate caught the planner substituting the per-Person index for the assignee index at 25,000 People ("Rows Removed by Filter: 25000"); `task_only.sql` now isolates the assignee-scoped scan in a `MATERIALIZED` CTE so the base table is accessed once. Archive under `docs/design/perf/slice-016b-2026-09-09/`: statement sha256 identity, a 25,000-Person zero-task equivalence run, plans at 5,000 and 25,000 People (single `Index Scan using task_org_assignee_due_open_idx`, 30 rows, 403 vs 404 buffers, 0.43 vs 0.47 ms). |
| `bf4fab1`, `3278b5c` | gates | `cargo fmt`; a needless lifetime flagged by clippy. |
| `b16f111` | walkthrough | The §12.17 record with eleven screenshots under `docs/design/qa/slice-016b-2026-09-09/` against the lane's own API and Web on `crm_slice016b_qa`; two honest observations: a morning snooze to "tomorrow end of day" lands beyond the 24-hour window, so the task leaves Today and the panel until it re-enters (spec-correct; recorded LATER as a product nuance), and the Operator's free-form reply quotes the title while its fixed line does not. |
| `3f55f53` | round 1 | Backend fixes (below). |
| `6689fe4` | round 2 | Today page fixes and test tightenings (below). |
| `dc2b608` | Web | The Today page's task badge, the Waiting cell's "Due <relative>", Complete on task-reason items, the Tasks panel (Overdue / Due soon against `generated_at`, Complete, Snooze to tomorrow, empty and truncated states), `queryKeys.tasks(orgId, actorId)`, the tasks prefix in the settle set and in `invalidationsFor`, the issue-key union with a label and a generic fallback, `reasonLabel`'s default arm. |

## Lane gates (own tree, per round)

At `3278b5c`: `sqlx-prepare && check` green (773 Rust, 677 Vitest, 35 s;
two earlier runs fixed a fmt drift and a clippy lint); the coordinator's
`check-db` 708 of 708 (254 s; twenty-four new DB tests). A targeted sweep
of the Today, task, Operator, admin, People and realtime suites: 368 of 368
twice. One 016a test that pinned "tasks never reach Today" was rewritten to
assert the 016b behaviour while keeping the Person-row, People-list and
D-052 invariants (verified faithful by the reviewer). The lane made three
substantive checkpoints rather than one per brief step (recorded).

## Review round 1 (of two), backend on `3278b5c`

Reviewer READY WITH FIXES; tester no production defect. Verified by both:
statement shape and bindings (Organization literal, assignee before the
earliest, `now` as a parameter, no rule-7 constraint, retained set
excluded, `LIMIT k+1`), the axis position and discipline, all-or-nothing
by construction, the merge and ordering rules, the redacting `Debug`, the
Operator title path, the panel route's fail-closed query parsing, the
`db_people.rs` rewrite. Findings applied in the round-1 batch:

- **spec self-conflict, resolved by the coordinator:** the draft's §5 said
  task-only items carry `latest_inquiry: null` unconditionally, while rule
  8, D-054 and the 011c pointer say "for zero-inquiry People, the list-only
  precedent"; the lane implemented the literal §5 and did not stop. The
  precedent wins (a real `InquiryRef` and `last_inquiry_at` when the
  Person has an inquiry; `null` only when none); §5 amended, the statement
  gains the latest-inquiry LATERAL, one test on an answered-inquiry Person.
- **test-support clock (SEEN_HERE, the 016a microsecond lesson):**
  `EvaluationClock::Fixed(now)` kept nanoseconds while the SQL bound a
  microsecond `$3`, so two boundary tests passed only on this machine's
  clock; the fixed clock is now truncated to microseconds and the tests use
  a truncated helper.
- must-close tests: the same user holding a task in another Organization
  for the axis and the panel; failure-phase tests made non-vacuous with
  retained items and a byte-identical no-task baseline (and the raise not
  leaking); `SET LOCAL` restoration after each phase and
  `statement_timeout` on the success path; a real SQL cancellation via
  `pg_sleep` under the axis budget; the `low` case asserting the reason
  vector; panel truncation at 201, boundaries via `open_for_assignee` with
  a fixed clock, more malformed shapes, a deactivated viewer, 401 before
  400; the `ORDERING_RULE` clause pinned by a literal; the `today.task_axis`
  span positive control and a parsed-result assertion that no explanation
  field carries the sentinel; Operator tool JSON compared to HTTP on a
  raised and a task-only item, `get_next_work_item` returning the task-only
  Person.

Accepted as recorded: `reason_text` uses absolute RFC 3339 (the spec's
"<relative>" was illustrative and `reason_text` has no clock); the queue's
tie-break `id` is the Person id and the panel's the task id (spec §5
amended; parity is set-based); `rank.rs`, `test_support.rs` and the task
module's `TaskWithPerson`/`PersonRef` touched outside the brief's list.

## Review round 2 (of two), Web on `dc2b608`

Reviewer READY WITH FIXES; tester "not ready" on one demonstrated defect.
Verified by both: types and the issue-key union; `queryKeys.tasks` and
the prefix in the settle set and in `invalidationsFor` (full-array
Vitest); the badge, the Waiting cell, Complete on any task-reason item,
POST-then-GET order on the single-action paths; the panel groups, empty
and truncated states; the label and generic fallback for the issue
notice; titles rendered as text; D-045 tints and 40 px controls. Findings
applied in the round-2 batch:

- **defect (functional, demonstrated by the tester):** the Today page
  shared one Complete hook and one Snooze hook and swapped their
  person-key ref synchronously before mutating; vue-query applies the key
  change after the call and resets the observer, detaching the in-flight
  mutation, so only one Complete and one Snooze worked per page load,
  later clicks were inert, errors never rendered, and the mutation ran
  under an empty person key. Every existing test did one action per
  mount and could not see it. Fixed so each action is keyed for its own
  Person with its callbacks running; sequential-action, 403 and
  mutation-key tests added.
- **defect (contract, SEEN_HERE):** a Complete failure from the ranked
  queue had no renderer, and a panel 404's refetch removed the row with
  its alert; a dismissible alert above the queue, keyed by task id and
  independent of rows, now explains 503 and 404 on both surfaces.
- **BOUNDARY:** the Overdue grouping compared ISO strings lexically and
  misplaced a whole-second due time against a fractional `generated_at`;
  a pure `groupTasks` helper now compares instants, with boundary tests.
- UI_STYLE: uppercase group headings removed; Refresh refetches the panel;
  the actor/session watch clears pending state; the fallback feed message
  goes through the guarded label lookup.
- tests: the grouping test's tautology and shared titles replaced by
  per-group membership and the link href; the snooze body asserted under
  fake timers installed before mount on both spec instants; the due cell
  past local midnight; the item leaving only after the refetch; a
  `set_outcome` item with a task reason; exact Waiting text; a
  `truncated: false` negative; rendering safety.

Accepted as recorded: `PreviewTodayFeedDialog.vue` (an exhaustiveness
compile touch) and a new `lib/tasks.ts` outside the brief's list; two
unrelated tests gained a `/tasks?scope=mine` stub.

## Confirmation on `6689fe4` (read-only, both rounds spent)

Every production fix landed and no scope leaked (`3f55f53`: the axis
module, its statement and `.sqlx`, plus tests and a test-only pin in the
Operator crate; `6689fe4`: `web/` only). The mutation-key fix uses
`await nextTick()` before `.mutate()` so the reactive key is applied first;
correct for the pinned vue-query and proven by the `isMutating` and
sequential-action tests, recorded as a timing mechanism rather than a
structural one. Three consecutive Vitest runs of the Today, realtime and
tasks files: 65 of 65 each. The lane's context had compacted before the
batches and it reconstructed them from a summary; roughly half the
requested test sub-parts did not land. Handled without a third round:

- **corrected by the coordinator (test-only):** the latest-inquiry
  hydration (round-1 fix A) landed with no positive test; a test now
  proves a task-only item on an answered-inquiry Person carries the real
  `InquiryRef` and a consistent `last_inquiry_at`, with the no-task
  absence as its positive control. The final gate's `check` also caught a
  1-in-6 flake in the Today "held while pending" test (a single macrotask
  wait racing the settle refetch's own macrotask); the assertion now polls
  for the refetch (8 of 8 afterwards).
- **recorded LATER, the lane's reconstruction dropped them:** the
  failure-baseline fixture still lacks the raised item and the
  byte-identical items comparison (the raise-under-failure invariant is
  unproven by test; all-or-nothing holds by construction); `SET LOCAL`
  restoration is asserted for one phase only; the cancellation test's
  elapsed bound is loose; the panel's fixed-clock boundaries, three
  malformed shapes, the deactivated viewer and the platform-only 401 are
  untested; the capture test calls `reason_text` directly rather than
  scanning parsed tool results; `ahead`/`ordering_rule` are not compared
  on the raised and task-only items and `get_next_work_item` returning the
  task-only Person is asserted nowhere; the Web error banner is tested for
  two of four surface/status combinations.

## Final-tree gates (coordinator, once)

On the final tree (the completion commit on top of `6689fe4`), 2026-09-09,
from the worktree root under the shared gate lock, in one sequence, each
once. An identical sequence on `6689fe4` itself had passed `sqlx-prepare`
(clean) and `check-db` (715 of 715) but failed `check` on the Vitest flake
above (746 of 747), which is why the completion commit exists:

FINAL_GATES_TABLE

## Recorded LATER (D-050)

- The remaining 199/200/201 × 0–3 cap grid cells; the call-prefix-truncated
  skip; a dedicated "task-only items count against the list cap" test;
  task-only order with three or more items and equal `due_at`; a retained
  `high` item with an overdue task unchanged; a retained `normal` item
  with list + task + call reasons; `+24 h + 1 s` compared to a baseline;
  the exhausted-recovery test asserting the issue; the 5 s bound.
- `reason_text` emits `+00:00` where every other wire timestamp is `Z`, and
  "a follow_up task".
- The `NOT EXISTS` self-anti-join over the materialized CTE is quadratic in
  the viewer's in-window tasks (BEYOND_ENVELOPE: dozens by premise; the
  FUB-import overdue horizon is the recorded trigger).
- On the success path the axis leaves a transaction-local
  `statement_timeout` set after `RELEASE`, as the call feed does.
- The 25,000-Person equivalence run had no inquiries or calls (an empty
  queue compared to an empty queue); the real-content equivalence is the
  smaller db test.
- A snooze to "tomorrow end of day" made in the morning moves the task
  beyond the 24-hour window, so it disappears from Today and the panel
  until the window catches up (spec-correct; the industry semantics of
  snooze; consider a "Due soon" lookahead of 48 hours or a snoozed-tasks
  group if users find it surprising).
- The lane's context compacted before it applied the two fix batches and
  it reconstructed them from a summary; the confirmation pass verified
  each item against the original lists.
- The D-054 exception itself: closed by the task clause family and
  feed-four rung.

## Merge readiness

Source `slice-016b-today` at its head (the completion commit on top of
`6689fe4`: the positive latest-inquiry test, the deflaked Today test, this
record); destination `main` (no commits since the branch base `a5b301e`, so
no conflict is possible). Migration impact: none (016b adds no migration;
`.sqlx` gains the axis and panel statements). After the merge, with the
user's approval: restart the dev API (the Today query and the new route)
and rebuild `dev-web-prod` (the Today page). The `crm_slice016b_qa`
database is left on the dev Postgres and can be dropped. Unresolved risks:
the LATER lists above, chiefly the untested raise-under-failure invariant.
Push and deployment are not authorized by this record.

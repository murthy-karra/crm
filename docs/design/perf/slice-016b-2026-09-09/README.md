# Slice 016b performance evidence (spec §11, D-050)

Executed 2026-09-09 in the worktree `crm-worktrees/tasks-2`
(`slice-016b-today`), against `#[sqlx::test]`-managed ephemeral scratch
databases (never `crm_dev`), created and torn down by the harness itself.
No server was started on port 3000 or 5173. One run, retained in full in
[harness-stdout.txt](harness-stdout.txt) and split per scale under
[plans/](plans/). Full host/toolchain facts in
[environment.txt](environment.txt); statement-identity evidence in
[source-sha256.txt](source-sha256.txt).

**Scope note, disclosed up front**, the same posture as the
[slice-011e-2026-09-08](../slice-011e-2026-09-08/README.md) archive: this
does not reuse the Slice 011c/011d Phase B apparatus. Spec §11 states the
016b gate is narrower than that apparatus was built for ("016a adds no
hot statement. 016b's axis is two index range scans bounded by the
viewer's own dated open tasks... plus a `person` join on at most 201
rows"), so a small, purpose-built harness was written instead: one
Organization, People seeded directly via batch SQL (`generate_series`),
no HTTP layer. The harness code was a temporary, uncommitted addition to
`backend/crates/crm-api/tests/zz_perf_slice016b_temp.rs` (with a matching
temporary `[[test]]` stanza in `crm-api/Cargo.toml`), removed after
capture; only this archive is committed with the slice.

## Gate 1 — paired equivalence (spec §11 item 1)

Two parts, per the spec text ("the person-state, call and source
statements are byte-identical in text"):

**Part A — statement text identity.** [source-sha256.txt](source-sha256.txt)
sha256-compares `person_state.sql`, `call_membership.sql`, `call_only.sql`,
`source_candidates.sql` and `source_membership.sql` against `main` at
`a5b301e` (016b's branch point, already containing 016a). All five are
byte-identical — 016b's lane never touched any of them, confirmed by
checksum rather than inspection alone.

**Part B — response equivalence at book scale.** An Organization with
25,000 People (D-050's stated envelope ceiling), zero tasks anywhere.
`today::query_at` for the admin, at that scale:

```
admin Today with zero tasks at 25,000 People: 0 items, truncated=false, sources.status=Complete
PASS: no task_due issue, no task_* reason anywhere -- byte-identical to a no-axis run
```

(The 25,000 People carry no inquiries or calls by construction — out of
scope for what this evidence needs to show, which is the axis's own
zero-task no-op, not the unrelated built-in queues; those are exercised
directly, at much smaller scale but with real inquiry/call/list content,
by `db_today_task_axis.rs`'s
`zero_task_organization_today_is_unaffected_for_admin_and_member` — 22/22
db tests passing at commit time, see the implementation report.) No
`task_due` `system_feed_issues` entry, no `TodayReason::TaskOverdue`/
`TaskDue` on any item, `sources.status` stays `Complete` — exactly the
pre-016b shape, at the stated envelope's People count, not just in the
unit-test-scale fixture.

## Gate 2 — plan shape and growth with People (spec §11 item 2)

`EXPLAIN (ANALYZE, BUFFERS)` of `task_only.sql` (the task-only prefix
statement), same viewer holding the same 30 open, dated tasks (per spec:
"dozens, not thousands"), at two People counts — 5,000 and 25,000 — with
one filler task per Person (i.e. 5,000 and 25,000 unrelated open, dated
tasks respectively, assigned to a different member) so the index actually
has to discriminate by assignee rather than matching a near-empty table:

| People (+ 1 filler task each) | Index used against `task` | Buffers (total) | Execution Time |
|---:|---|---:|---:|
| 5,000 | `task_org_assignee_due_open_idx` (single scan, 30 rows) | 403 | 0.427 ms |
| 25,000 | `task_org_assignee_due_open_idx` (single scan, 30 rows) | 404 | 0.473 ms |

Full plans: [plans/task_only_5000_people.txt](plans/task_only_5000_people.txt),
[plans/task_only_25000_people.txt](plans/task_only_25000_people.txt).

Both plans show exactly one access to the base `task` table — a `CTE
Scan`-driving `Index Scan using task_org_assignee_due_open_idx`, `Index
Cond: (organization_id = ... AND assignee_user_id = ... AND due_at <=
...)`, fetching precisely the 30 matching rows — with the rest of the
plan (the person/stage/app_user joins, the effective-attempt LATERAL) at
constant cost regardless of the 5x People difference. Buffers and
execution time are flat within measurement noise: no growth, super-linear
or otherwise, with People.

**A finding recorded and fixed, not merely reported.** The first
statement text drafted (`SELECT DISTINCT ON (person_id) ... ORDER BY
person_id, due_at, id`, the `call_only.sql` precedent) does **not** hold
this property on PostgreSQL 18: because `task_org_person_due_idx`
(`organization_id, person_id, due_at, id` — no assignee/open-only guard)
already produces the exact `(person_id, due_at, id)` order the `DISTINCT
ON` needs, the planner sometimes prefers it over the assignee-scoped
partial index plus an explicit sort — even inside a `MATERIALIZED` CTE,
and even inside a correlated `NOT EXISTS` self-anti-join sharing the same
predicates on both sides, both tried and rejected as intermediate fixes.
At the 25,000-Person scale this produced a full-table-style scan of every
open, dated task in the Organization (`Rows Removed by Filter: 25000`),
which is exactly the super-linear-growth failure this gate exists to
catch. The shipped statement instead materializes the viewer's own
matching rows FIRST (`WITH mine AS MATERIALIZED (... WHERE
organization_id = $1 AND assignee_user_id = $2 AND ... AND due_at <=
$3)`), making that the *only* access to the base table, then computes
"earliest per Person" as a `NOT EXISTS` self-anti-join entirely over that
already-small, in-memory CTE — eliminating the competing-index choice
rather than trying to out-cost it. See the comment above the `mine` CTE
in `backend/crates/crm-app/src/domain/today/sql/task_only.sql` for the
full reasoning. Every `db_today_task_axis.rs` test (22/22) was re-run
against the shipped statement text and passes unchanged — the rewrite is
a pure access-path change, not a semantic one.

## Absolute latency (reported, never gated)

Both plan-shape samples: sub-millisecond `Execution Time` (0.43–0.47 ms)
for the whole `task_only.sql` statement including the `person`/`stage`/
`app_user` joins and the effective-attempt LATERAL, at both People counts,
on the laptop described in [environment.txt](environment.txt). Not a
production capacity claim (D-050): reported for trend-watching only.

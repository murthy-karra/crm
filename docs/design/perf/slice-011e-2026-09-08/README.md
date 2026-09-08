# Slice 011e e2 — step 5 performance evidence (spec §8, D-050)

Executed 2026-09-08 in the worktree `crm-worktrees/011e-e2`
(`slice-011e-tag-clauses`), against a `#[sqlx::test]`-managed ephemeral
scratch database (never `crm_dev`), created and torn down by the harness
itself. No server was started on port 3000 or 5173. One run, retained in
full in [harness-stdout.txt](harness-stdout.txt). Full host/toolchain
facts in [environment.txt](environment.txt).

**Scope note, disclosed up front.** This archive does **not** reuse the
Slice 011c/011d Phase B apparatus (`TodayHttpPerfFixture`, the
50,000-Person history-rich fixture gated on `CRM_SLICE_011C_BUILD_HASH`,
the full HTTP request-driver matrix). D-050's gate for this slice is
narrower than that apparatus was built for — a paired relative-regression
timing comparison of three named statements with no tag clause, plus one
plan-shape `EXPLAIN` each for a `tags` and a `not_tags` clause — so a
smaller, purpose-built harness was written instead: one Organization,
25,000 People (D-050's stated envelope ceiling) and 200 tags (D-050's
stated per-Organization tag ceiling) seeded directly via batch SQL
inserts, no HTTP layer, no frozen historical query, no build-hash
apparatus. This is a deliberate, disclosed scope reduction matching
D-050's own principle ("no slice spends more than one benchmark run on
performance unless the paired regression fails") — not a shortcut on the
numbers themselves: every timing and every `EXPLAIN` below is a real
query executed against a real, freshly `VACUUM (ANALYZE)`d Postgres 18.6
instance with the real production statement text, nothing estimated or
fabricated. The harness code itself was a temporary, uncommitted addition
to `backend/crates/crm-api/tests/db_today_feeds_http_perf.rs` (removed
after capture); only this archive is committed with the slice.

The fixture's `person` rows carry no inquiries, contact attempts, or
correspondence (out of scope for what this evidence needs to show), so
`person_state.sql`'s own "at least one inquiry" gate (`latest.id IS NOT
NULL`) admits zero rows for both statement versions in Part 1 below —
expected and immaterial to the comparison, which is about the ADDED
tag-clause predicates' cost when absent (both versions scan and filter
the identical row set and agree on the identical, empty, result).

## Part 1 — paired relative regression, no tag clause (D-050 gate 1)

Same fixture, same connection pool, same fixed clock, same organization
and viewer id, 15 samples each per statement per version (3 discarded as
warm-up, 12 kept), nearest-rank p95. `old` is the exact pre-e2 statement
text at commit `c629ac0` (the branch point before any Slice 011e e2
change); `new` is the current statement text with `tag_ids_any`/
`tag_ids_none` both bound `NULL` (no `tags`/`not_tags` clause present).
"Payload equal" compares the ordered row-identity list (`id` column) for
`filtered_summaries`/`person_state`, and the scalar count for
`count_filtered_matches`.

| Statement | old p95 | new p95 | Allowed (max(25 ms, 10%)) | Within allowed | Payload equal |
|---|---:|---:|---:|:---:|:---:|
| `filtered_summaries` | 18 ms | 18 ms | 25 ms | **yes** | **yes** (501 rows) |
| `count_filtered_matches` | 1 ms | 1 ms | 25 ms | **yes** | **yes** (501/501) |
| `person_state` | 15 ms | 16 ms | 25 ms | **yes** | **yes** (0/0 rows) |

All three statements pass D-050 gate 1 with wide margin — the two
NULL-guarded added predicates per statement (four for `person_state`,
one chain per person-state feed) add no measurable cost when absent, as
expected for a `($n::uuid[] IS NULL OR ...)` guard the planner can fold
away.

## Part 2 — plan shape: `filtered_summaries` with `tags` / `not_tags` (D-050 gate 2)

Fixture tagging distribution: all 200 tags, each applied to a **disjoint**
120-Person slice (200 × 120 = 24,000 `person_tag` rows total; every
Person carries at most one tag) — deliberately NOT one tag covering a
large fraction of the Organization, so a single tag's selectivity
(120/25,000 ≈ 0.48%) reflects a real, narrow segment rather than a
contrived "20% of everyone" case. `VACUUM (ANALYZE)` was run on `person`,
`tag`, `person_tag`, `stage` and `app_user` immediately before capture.
Both `EXPLAIN (ANALYZE, BUFFERS)` runs bind the real organization id, all
other axes NULL, and one real tag id each (`tags: [tag_ids[0]]`,
`not_tags: [tag_ids[1]]` — two different, disjoint 120-Person tags).

| Clause | Matching People | Plan shape for the tag predicate | Execution time |
|---|---:|---|---:|
| `tags: [tag]` (0.48% selectivity) | 120 | `Hash Join`/`Bitmap Heap Scan on person` filtered by a **hashed subplan**: `Seq Scan on person_tag pt` (`Filter: tag_id = ANY(...)`, cost 567.64, 24,000-row table, executed **once**) | 4.6 ms |
| `not_tags: [tag]` (99.52% match) | 501 (capped) | Same shape: `Seq Scan on person_tag pt2` (cost 567.64, executed once), `NOT (ANY(...))` filter on the outer scan | 29.1 ms |

Full plans:
[plans/filtered_summaries_tags.txt](plans/filtered_summaries_tags.txt),
[plans/filtered_summaries_not_tags.txt](plans/filtered_summaries_not_tags.txt).

**Finding, stated plainly (not what was anticipated in the brief).** The
planner does **not** choose a per-Person `person_tag` primary-key/index
probe (an `Index Scan`/`Bitmap Index Scan` driven once per candidate
Person) for either clause. It instead flattens the correlated
`EXISTS`/`NOT EXISTS` into a **hashed semi-join**: `person_tag` is
sequentially scanned and hashed **exactly once** per query execution
(cost 567.64, the same cost in both directions since both tags cover the
identical 120-row slice size), then every `person` row is probed against
that in-memory hash in O(1). This was confirmed to be the planner's
*consistent* choice, not a one-off: the same shape appeared at three
different `person_tag` sizes tried while building this fixture (5,050
rows with one tag at ~20%/~0.2% selectivity; 12,000 rows across 40 tags
at ~1.2% each; 24,000 rows across all 200 tags at ~0.48% each) — Postgres
prefers hashing a "person_tag"-sized table once over 25,000 individual
index probes at every cardinality tested, which is the cost-correct
choice at this envelope (a full scan-and-hash of a few-thousand- to
tens-of-thousands-row table is cheap, and it strictly dominates repeated
per-row index lookups at 25,000 candidate rows).

This still satisfies D-050's actual requirement — **no super-linear
growth with People**: total cost is `O(|person| + |person_tag|)` (a scan
of each once), never `O(|person| × |person_tag|)`; the `person_tag`
scan's cost (567.64) scales with `person_tag`'s own row count, confirmed
directly by comparing the three fixture sizes tried above (cost rose from
~120 to ~285 to ~567 in step with row count, staying flat with respect to
the unchanged 25,000-Person `person` table). The `person_tag_org_tag_
person_idx` index (`(organization_id, tag_id, person_id)`) remains
available and would become the planner's preferred access path at a
sufficiently different data distribution (a much larger `person_tag`
relative to `person`, or `enable_hashjoin` disabled) — the same kind of
caveat the Slice 011d archive gave for its own `enable_mergejoin`
finding: a cost-based choice, re-evaluated by the planner on real
statistics, not a hard guarantee against every future fixture shape.

## Absolute numbers (reported per D-050, never gated)

All timings above are absolute, uncapped, laptop measurements, reported
for trend-watching only. D-050 supersedes any earlier absolute-latency
requirement; nothing in this archive is a pass/fail on absolute ms.

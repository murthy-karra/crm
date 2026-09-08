# Slice 012 — step 5 performance evidence (spec §7, D-050)

Executed 2026-09-08 in the worktree `crm-worktrees/012`
(`slice-012-activity-columns`), against a `#[sqlx::test]`-managed
ephemeral scratch database (never `crm_dev`). No server was started on
port 3000 or 5173. One benchmark run, retained in full in
[harness-stdout.txt](harness-stdout.txt) (round 1 review capture — Part 2
below was regenerated with `plan_cache_mode = force_generic_plan`; Part 1
was NOT re-run per the coordinator's instruction, so its numbers are the
original capture). Full host/toolchain facts in
[environment.txt](environment.txt); statement-text provenance in
[source-sha256.txt](source-sha256.txt).

**Scope note, disclosed up front**, mirroring the Slice 011e e2 archive's
own precedent: this does **not** reuse the Slice 011c/011d Phase B
apparatus (`TodayHttpPerfFixture`, the 50,000-Person history-rich fixture,
the full HTTP request-driver matrix). D-050's gate for this slice is
narrower than that apparatus was built for — a paired relative-regression
timing comparison of four statements' frozen-vs-live text, plus three
plan-shape `EXPLAIN`s — so a smaller, purpose-built harness was used
instead: one Organization, 25,000 People (D-050's stated envelope
ceiling), seeded via batch SQL lifted (and scaled down from 50,000) from
`today_http_perf_fixture.rs::seed_people_and_history` — same shape
(skewed assignment, historical + waiting inquiries, corrected contact
attempts, inbound correspondence, a caller's ended calls). Unlike the
011e e2 archive, this harness (`slice_012_performance_evidence` in
`db_today_feeds_http_perf.rs`) is **kept and committed** with this slice
(the coordinator's step 5 instruction for this round), behind its
`perf-harness` feature gate, at zero cost to normal builds.

Every timing and every `EXPLAIN` below is a real query executed against a
real, freshly `VACUUM (ANALYZE)`d PostgreSQL 18.6 instance with the real
production statement text (the exact live `.sql` files and the frozen
`b45b04f`/`61b08ac` copies at `tests/fixtures/statements_b45b04f/`),
nothing estimated or fabricated.

## Part 1 — paired relative regression, frozen vs live (D-050 gate 1)

Not re-run for round 1 (the coordinator's explicit instruction — only
gate 2 was recaptured). Same fixture (25,000 People, one Organization),
same connection pool, same fixed clock (`2026-09-08T12:00:00Z`), same
organization and viewer id, 15 samples each per statement per binding (3
discarded as warm-up, 12 kept), nearest-rank p95. "frozen" is the exact
pre-switch text at commit `b45b04f`/`61b08ac`
(`tests/fixtures/statements_b45b04f/`); "live" is the current post-switch
text (commit `9f02ead`). "Payload equal" compares the ordered
row-identity list (`id` column) for `filtered_summaries`/
`source_candidates`/`person_state`, and the scalar `(count, truncated)`
pair for `count_filtered_matches`.

| Statement | Binding | frozen p95 | live p95 | Allowed (max(25 ms, 10%)) | Within allowed | Payload equal |
|---|---|---:|---:|---:|:---:|:---:|
| `filtered_summaries` (default sort) | `last_contact` never | 22 ms | **5 ms** | 25 ms | **yes** | **yes** |
| `count_filtered_matches` | `last_contact` never | 8 ms | **1 ms** | 25 ms | **yes** | **yes** |
| `source_candidates` | `last_contact` never | 46 ms | **2 ms** | 25 ms | **yes** | **yes** |
| `filtered_summaries` (default sort) | `last_contact` not_within_days 7 | 6 ms | 5 ms | 25 ms | **yes** | **yes** |
| `count_filtered_matches` | `last_contact` not_within_days 7 | 1 ms | 0 ms | 25 ms | **yes** | **yes** |
| `source_candidates` | `last_contact` not_within_days 7 | 48 ms | **1 ms** | 25 ms | **yes** | **yes** |
| `person_state` (both feeds) | canonical feed definitions, both enabled, 24h fresh | 353 ms | **22 ms** | 35 ms | **yes** | **yes** |

All seven paired comparisons pass D-050 gate 1 with wide margin; the
absence-proving axes (`last_contact never`, and especially the
per-Person-probe-heavy `source_candidates` and `person_state`) show the
large improvement the spec anticipated — `source_candidates` under `never`
drops from 46 ms to 2 ms (23×), and `person_state` from 353 ms to 22 ms
(16×) — because the four `LEFT JOIN LATERAL max(...)` probes per candidate
Person are gone, replaced by a plain column read on an already-fetched
row. The `not_within_days 7` binding shows a smaller (still real) win
because that axis was never absence-proving to begin with (PERF_BASELINE's
own finding: positive/indexable predicates were already flat). **Note
(round 1 review):** both frozen and live sides of every Part 1 comparison
also ran as PREPARE-then-repeated-EXECUTE within the harness's `timed()`
helper (15 executions each), so — unlike Part 2's single-shot capture
below — these ARE representative of custom-plan behavior across several
executions, though not proven to reach the generic plan either; the
relative (frozen-vs-live) comparison is what D-050 gates, and both sides
ran under the identical plan-selection regime, so this asymmetry does not
undermine Part 1's conclusion.

## Part 2 — plan shape (D-050 gate 2)

**Superseded once, in place, after round 1 review.** The original capture
of this section used `EXPLAIN (ANALYZE, BUFFERS) EXECUTE s012_fs(...)`
immediately after `PREPARE`, which — for the first few executions of a
freshly prepared statement — is a **custom plan**: Postgres inlines the
actual bound constants (in particular, which of the many `$n IS NULL`
guards are true) and prunes accordingly. That is not necessarily the plan
a statement prepared once and executed repeatedly with varying arguments
(as `sqlx`'s connection-pooled prepared statements are) settles on in
production; Postgres's own `plan_cache_mode = auto` heuristic re-plans
custom for the first five executions and adopts the **generic** plan —
the one that cannot see the bound values at plan time — only if its
estimated cost isn't materially worse. The three plans below are
recaptured with `SET LOCAL plan_cache_mode = force_generic_plan` set
before the `PREPARE`s in the same transaction, so what is archived now is
unambiguously the generic plan, not a best case.

**Finding, stated plainly (materially different from the original
custom-plan capture, and worth flagging even though plan shape is
report-only under D-050, never gated).** The generic plan is dramatically
more expensive than the custom plan for all three cases. Forced to treat
every `$n IS NULL` guard as a live unknown rather than a known-NULL
constant, the planner can no longer prune `filtered_summaries` down to an
index-sargable `created_at` scan and falls back to a full `Seq Scan on
person` (cost 944,170) evaluating the entire 20-clause guard expression
per row; `person_state`'s `latest` LATERAL (guarded on `$5 IS NOT NULL OR
$27 IS NOT NULL`, both NULL in the canonical binding) can no longer fold
that guard to a constant-`false` `One-Time Filter` either, so it now
actually runs an index probe for all 20,333 gate-passing candidates
instead of zero.

| Case | Plan shape (generic, this capture) | Execution time (custom, original capture) | Execution time (generic, this capture) |
|---|---|---:|---:|
| `filtered_summaries`, `last_contact never` | `Seq Scan on person` (cost 944,170.61), the full guard expression evaluated as a row `Filter`, no index use | 3.5 ms | **382.2 ms** |
| `filtered_summaries`, `last_contact not_within_days 7` | Same: `Seq Scan on person`, full guard `Filter` | 2.9 ms | **393.1 ms** |
| `person_state` (both feeds, canonical) | `Seq Scan on person p_1` for the base candidate set (unchanged in shape from the custom plan — that scan was never index-backed either way), but the `latest` LATERAL now also runs unconditionally instead of folding to `false` | 25.1 ms | **1,660.1 ms** |

Full plans (generic, this capture, superseding the original three files):
[plans/filtered_summaries_never.txt](plans/filtered_summaries_never.txt),
[plans/filtered_summaries_not_within_7.txt](plans/filtered_summaries_not_within_7.txt),
[plans/person_state.txt](plans/person_state.txt).

**The gated `waiting` probe's `loops`, confirmed not to exceed the gated
count (spec §7 gate 2's explicit requirement) — UNCHANGED between custom
and generic plans.** In the `person_state` generic plan, `Seq Scan on
person p_1` still returns **20,333** rows — the exact count of People
passing the §1 rule 7 "at least one inquiry" gate. The `waiting`
LATERAL's outer `Limit` node still runs **`loops=20,333`** — one loop per
gated candidate, never more — identically to the original custom-plan
capture. Inside it, the actual `Index Only Scan using
inquiry_org_person_received_idx` still runs only **`loops=5,293`**,
because the `waiting` probe's OWN gate (`p_1.last_inquiry_at >
COALESCE(p_1.last_contact_at, '-infinity')`) compares two already-known
COLUMNS on the current row, not a bound PARAMETER — it folds to a
`One-Time Filter` under both custom and generic planning alike, so this
specific pin holds regardless of plan-cache mode. (The unrelated `latest`
LATERAL discussed above is guarded on bound parameters instead, which is
exactly why only that one lost its fold under the generic plan.)

**What this finding does and does not mean.** D-050 gates only two
things — paired relative regression (Part 1, unaffected by this finding)
and "plan shape... showing index use and no super-linear growth" as a
report, with "absolute p95... never gated." This capture shows the
*generic* plan does not exhibit clean index use for `filtered_summaries`
at this envelope; whether production actually runs the generic or the
custom plan for these statements in practice depends on `sqlx`'s
connection-pooled prepared-statement lifecycle and Postgres's `auto`
heuristic over the real mix of bound values across many requests, which
this harness does not attempt to reproduce (that would need a long-lived
connection executing a realistic, varied sequence of real requests — a
different, larger harness). Recorded for the record, not silently fixed
or hidden. A candidate follow-up (e.g., confirming empirically which mode
`sqlx`'s pooled connections actually settle into under production-shaped
traffic, or pinning `plan_cache_mode` explicitly for these statements) is
a later lever, out of this slice's scope.

## Report-only figures (spec §7, not gated)

- **Backfill block duration**, extracted from the migration by its
  markers and re-run over the full 25,000-Person populated fixture (all
  four columns nulled first, as the migrator role): **323 ms** (original
  capture) / **310 ms** (round 1 review re-capture, same test) — both
  report-only, consistent.
- **Seed duration** (batch-SQL insert of 25,000 People plus their
  inquiry/contact/correspondence/call history, one transaction, WITH the
  three `person_touch_*` triggers active — every history-table `INSERT`
  above 60% of the book fires one): **2,118 ms** (original), **2,058 ms**
  (round 1 review re-capture) — consistent across three total captures
  (2,132 / 2,118 / 2,058 ms). No triggerless baseline was re-measured at
  this same 25k scale (that would require a second, migration-less
  database); reported instead against the pre-Slice-012 PERF_BASELINE.md
  figure — **104–127 leads/s sustained** through the full live HTTP
  intake pipeline (one row at a time, per-Organization
  advisory-lock-serialized, a categorically different workload from this
  batch `INSERT ... SELECT` seed) — as the closest available "before"
  reference; a like-for-like triggerless batch-seed timing is not
  available and is not reproduced here to avoid fabricating a number. The
  batch-seed shape itself (25,000 rows across `person`, `inquiry` ×2,
  `contact_attempted` ×2, `correspondence_raw`/`correspondence_captured`,
  `call`) completing in ~2.1 s means the trigger's added per-row cost (one
  guarded, often-no-op `UPDATE person ... WHERE ... AND (col IS NULL OR
  col < NEW.ts)` on an already-locked-by-transaction row) is not
  observably significant at this envelope.
- **Today HTTP serial p95** (12 samples, `GET /api/today` in-process, the
  seeded viewer's own book): **34 ms** in both the original and round 1
  review captures, reported as trend against the archived [Slice 011d
  Feeds/concentrated/zero-source/serial
  figure](../slice-011d-2026-09-07/README.md) of **177 ms at 50,000
  People**. Not a valid apples-to-apples comparison (different People
  count, different book shape/concentration, different fixture) — cited
  only as directional confirmation that Today has not regressed;
  PERF_BASELINE's own finding was that Today cost scales with book size ×
  history, and this run's book and history are both smaller than the
  011d archive's concentrated-book case.

## Round 1 review disclosures

- **The frozen `count_filtered_matches` text omits two SQL comments**
  present in the live text at `61b08ac`
  (`-- docs/specs/SLICE_011d.md §2: same three derived predicates...` and
  `-- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags...`,
  `backend/crates/crm-app/src/domain/person/queries.rs`). The frozen copy
  in `tests/fixtures/statements_b45b04f/person_sql.rs` was typed out by
  hand rather than extracted verbatim, and the two comment blocks were
  dropped in that process. The EXECUTABLE SQL text — every clause,
  parameter position, and predicate — is identical; comments are not sent
  to Postgres and do not affect the `.sqlx` cache hash's semantics or any
  query result, so this has no effect on the equivalence gate (which
  compares RESULTS, never text, by design) or on any measurement in this
  archive. Noted here for the record rather than silently left
  undisclosed; not fixed in this lane (no behavioral effect, and D-052's
  fix-round scope is these ten items only).
- **The "25 vs 27 parameters" claim in the prior report is retracted, not
  reproduced.** The claim was: the pre-existing (011e-era, unrelated to
  this slice) test `d050_plan_shape_evidence_call_statements_and_filtered_summaries`
  in `db_today_feeds_http_perf.rs` binds exactly 25 `.bind()` calls to
  `filtered_summaries.sql`'s `EXPLAIN`, while the live file references
  parameters up to `$27` (the two `SLICE_011e` tag clauses), which —
  if that code path is actually EXECUTED — should produce a Postgres
  wire-protocol error citing an unbound parameter. I counted the `.bind()`
  calls and the highest `$n` in the file (both confirmed still true by
  inspection today), but never actually ran that specific test to capture
  a real error message — the claim was inferred from static reading, not
  reproduced. The tester's independent check reports no failure and that
  the target compiles at HEAD (compilation was never in question — the
  `.bind()` count is a runtime concern, invisible to `rustc`, only
  surfacing if that ignored, `perf-harness`-gated test is actually
  executed against a real database). Since I cannot produce the exact
  command and error text the coordinator asked for without running that
  50,000-Person-fixture test myself (which I have not done, and doing so
  is outside this fix round's scope), the honest position is: **retracted
  as unverified**, not confirmed. That older test is, either way, not
  touched by this lane per the coordinator's explicit instruction.

## Reproducing

Part 1 (paired regression) and the report-only figures:

```sh
cd backend
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
cargo nextest run --locked --features perf-harness -p crm-api \
  --test db_today_feeds_http_perf --run-ignored only \
  -E 'test(slice_012_performance_evidence)' --no-capture
```

Part 2 (plan shape, forced generic) is captured by the same command — the
harness's gate 2 section now sets `plan_cache_mode = force_generic_plan`
before its `PREPARE`s.

# Slice 012 — step 5 performance evidence (spec §7, D-050)

Executed 2026-09-08 in the worktree `crm-worktrees/012`
(`slice-012-activity-columns`), against a `#[sqlx::test]`-managed
ephemeral scratch database (never `crm_dev`). No server was started on
port 3000 or 5173. One benchmark run, retained in full in
[harness-stdout.txt](harness-stdout.txt). Full host/toolchain facts in
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

Same fixture (25,000 People, one Organization), same connection pool,
same fixed clock (`2026-09-08T12:00:00Z`), same organization and viewer
id, 15 samples each per statement per binding (3 discarded as warm-up, 12
kept), nearest-rank p95. "frozen" is the exact pre-switch text at commit
`b45b04f`/`61b08ac` (`tests/fixtures/statements_b45b04f/`); "live" is the
current post-switch text (commit `9f02ead`). "Payload equal" compares the
ordered row-identity list (`id` column) for `filtered_summaries`/
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
own finding: positive/indexable predicates were already flat).

## Part 2 — plan shape (D-050 gate 2)

`EXPLAIN (ANALYZE, BUFFERS)` after `VACUUM (ANALYZE)` on `person`,
`inquiry`, `contact_attempted`, `call`, `correspondence_captured`,
`contact_method`, taken through `PREPARE`/`EXECUTE` of the exact live
`.sql` text (`PREPARE s012_fs AS <filtered_summaries.sql text>` /
`PREPARE s012_ps AS <person_state.sql text>`, then `EXPLAIN (ANALYZE,
BUFFERS) EXECUTE s012_fs(...)`), so the archived plan is the generic plan
a repeatedly-prepared production statement runs, not a possibly-optimized
first-use custom plan.

| Case | Plan shape | Execution time |
|---|---|---:|
| `filtered_summaries`, `last_contact never` | `Index Scan Backward using person_organization_created_idx` (the `Sec1 rule 6` design: `last_contact_at IS NULL` applied as a row `Filter`, not index-sargable, over an already-indexed `created_at` scan — no full-table scan) | 3.5 ms |
| `filtered_summaries`, `last_contact not_within_days 7` | Same shape: `Index Scan Backward using person_organization_created_idx`, `Filter: (COALESCE(last_contact_at,...) <= ...)` | 2.9 ms |
| `person_state` (both feeds, canonical) | `Seq Scan on person` (`Filter: last_inquiry_at IS NOT NULL AND organization_id = ...`, 20,333 of 25,000 rows pass) feeding the `waiting` LATERAL probe, `Sort`/`Limit` to the 201-row `capped` prefix, then the effective-attempt anti-join and `latest`/`effective_attempt` hydration LATERALs run only over that 201-row prefix | 25.1 ms |

Full plans: [plans/filtered_summaries_never.txt](plans/filtered_summaries_never.txt),
[plans/filtered_summaries_not_within_7.txt](plans/filtered_summaries_not_within_7.txt),
[plans/person_state.txt](plans/person_state.txt).

**The gated `waiting` probe's `loops`, confirmed not to exceed the gated
count (spec §7 gate 2's explicit requirement).** In the `person_state`
plan, the outer `Seq Scan on person p_1` (`Filter: last_inquiry_at IS NOT
NULL AND organization_id = ...`) returns **20,333** rows — the exact
count of People passing the §1 rule 7 "at least one inquiry" gate. The
`waiting` LATERAL's outer `Limit` node (wrapping the per-Person probe)
runs **`loops=20,333`** — one loop per gated candidate, never more. Inside
it, the actual `Index Only Scan using inquiry_org_person_received_idx`
(the real index descent) runs only **`loops=5,293`** — a strict subset,
because the new pseudo-constant gate (`p_1.last_inquiry_at >
COALESCE(p_1.last_contact_at, '-infinity')`) is folded into a `One-Time
Filter` on the enclosing `Result` node, which short-circuits to `rows=0`
without touching the index at all for the 15,040 People whose last
inquiry is not after their last contact. `loops` therefore sits at exactly
the ceiling for the outer probe and strictly below it for the real index
work — confirmed empirically, not merely argued from the SQL text.

## Report-only figures (spec §7, not gated)

- **Backfill block duration**, extracted from the migration by its
  markers and re-run over the full 25,000-Person populated fixture (all
  four columns nulled first, as the migrator role):
  **323 ms**.
- **Seed duration** (batch-SQL insert of 25,000 People plus their
  inquiry/contact/correspondence/call history, one transaction, WITH the
  three `person_touch_*` triggers active — every history-table `INSERT`
  above 60% of the book fires one): **2,118 ms** (a second capture in an
  earlier compile-verification run measured 2,132 ms — consistent). No
  triggerless baseline was re-measured at this same 25k scale (that would
  require a second, migration-less database); reported instead against
  the pre-Slice-012 PERF_BASELINE.md figure — **104–127 leads/s sustained**
  through the full live HTTP intake pipeline (one row at a time, per-
  Organization advisory-lock-serialized, a categorically different
  workload from this batch `INSERT ... SELECT` seed) — as the closest
  available "before" reference; a like-for-like triggerless batch-seed
  timing is not available and is not reproduced here to avoid fabricating
  a number. The batch-seed shape itself (25,000 rows across `person`,
  `inquiry` ×2, `contact_attempted` ×2, `correspondence_raw`/
  `correspondence_captured`, `call`) completing in ~2.1 s means the
  trigger's added per-row cost (one guarded, often-no-op `UPDATE person
  ... WHERE ... AND (col IS NULL OR col < NEW.ts)` on an already-locked-
  by-transaction row) is not observably significant at this envelope.
- **Today HTTP serial p95** (12 samples, `GET /api/today` in-process,
  the seeded viewer's own book): **34 ms**, reported as trend against the
  archived [Slice 011d Feeds/concentrated/zero-source/serial
  figure](../slice-011d-2026-09-07/README.md) of **177 ms at 50,000
  People**. Not a valid apples-to-apples comparison (different People
  count, different book shape/concentration, different fixture) — cited
  only as directional confirmation that Today has not regressed;
  PERF_BASELINE's own finding was that Today cost scales with book size
  × history, and this run's book and history are both smaller than the
  011d archive's concentrated-book case.

## Reproducing

```sh
cd backend
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
cargo nextest run --locked --features perf-harness -p crm-api \
  --test db_today_feeds_http_perf --run-ignored only \
  -E 'test(slice_012_performance_evidence)' --no-capture
```

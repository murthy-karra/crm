# Slice 011b — 50,000-Person count plans

Measured on 2026-09-06 by the Terra implementation lane, using a newly
generated PostgreSQL database on the local Docker development server. The
fixture contains 50,000 synthetic People, 49,500 with a phone and 500 without.
It has no inquiry/contact history. The harness applied the current migrations
and dropped its generated database after the run; shared development data was
not changed.

Each measurement is one `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` execution.
The count and existing filtered-summary queries use the same static predicate
matrix, with only `has_phone` enabled. The saved-list count projects IDs up to
501 before aggregation; the summary retains its existing full projection and
ordering. These are SQL plans with literal parameter values, not HTTP latency,
load-test results or evidence for every prepared-plan/cache configuration.

| Predicate/query | Execution ms | Limit rows | Root shared hits |
|---|---:|---:|---:|
| Dense: has phone, count | 12.489 | 501 | 1,728 |
| Dense: has phone, summary | 17.010 | 501 | 7,869 |
| Sparse: no phone, count | 21.841 | 500 | 2,385 |
| Sparse: no phone, summary | 30.720 | 500 | 56,766 |

All four plans reported zero shared reads and zero temporary reads/writes.
Root buffer totals are reported directly, without summing nested nodes.

The dense count stopped after 501 matching Person rows. Its phone subplan
still scanned 49,500 contact rows, so the output cap does not bound all query
work. The sparse count examined all 50,000 People and rejected 49,500 to prove
there were exactly 500 matches. The summary's sparse ordered Person index scan
also rejected 49,500 rows and accounted for 50,756 shared hits before its
remaining projection work. The count avoids the summary's contact/history
projection and ordering; it took less time and fewer buffer hits in both
measured cases. No material regression was observed in these paired cases.

The existing [performance baseline](../../PERF_BASELINE.md) measured full HTTP
requests over 100,000 People with substantial history. Its absolute timings
are not comparable to this smaller SQL-only fixture. These plans agree with
its identified limitation: absence predicates can still require scanning the
whole candidate set. Slice 011b's 25-definition pages and four-request count
limit bound browser fan-out, not database work across all concurrent agents.
The existing activity/read-model and pool-capacity follow-up remains relevant.

## Evidence and reproduction

- [Run manifest and source hashes](manifest.txt), [fixture counts](seed-summary.txt)
  and [raw metrics](metrics.txt).
- Dense [count plan](plans/count-dense_has_phone_true.json) and
  [summary plan](plans/summary-dense_has_phone_true.json).
- Sparse [count plan](plans/count-sparse_has_phone_false.json) and
  [summary plan](plans/summary-sparse_has_phone_false.json).
- Exact rendered SQL and synthetic seed are in `sql/`.
- [The executed harness](run_perf.sh) creates a guarded `crm_011b_perf_*`
  database, applies current migrations, measures these four cases, and drops
  that database by default. Run from the repository root with
  `bash docs/design/perf/slice-011b-2026-09-06/run_perf.sh` only while holding
  the repository's sole database-operation lane. It needs the existing local
  `.env`, Cargo, and host psql or the development PostgreSQL container. It
  writes fresh evidence under its adjacent `runs/` directory; do not commit
  incidental rerun output without reviewing it.

The manifest's original `source_revision` is the base HEAD, `1635fc4`; the
measured code was the uncommitted Slice 011b tree. Appended source hashes
identify its count-query and migration files. No real credentials or customer
content are included in these artifacts.

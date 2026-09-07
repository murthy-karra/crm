# Slice 011c — Phase B authenticated HTTP evidence

Executed on 2026-09-06 (evening, local time) by the coordinator (Claude Fable
5.1) as the sole database lane, against the declared [protocol](PROTOCOL.md),
on the same MacBook Pro (Apple M1 Max, 10 cores, 64 GB, on battery, PostgreSQL
18.6 in the OrbStack development container) that measured Phase A. Two runs
were made; **both are retained in full** with their raw artifacts, independent
recounts, build and per-file source hashes. Nothing about the workload,
fixture, budgets, caps or pool size was changed between them.

| Run | Build | Outcome | Where |
|---|---|---|---|
| 1 (19:39 local, 234 s) | `9a6ef7a5…` (tree before the proposed planner change) | **Failed**: 11 of 15 series passed; every concentrated series with sources had 10 of 20 requests time out at the pool per concurrency-20 wave and its serial p95 above the cap. Root cause found afterwards, below. | [run-1-failed/](run-1-failed/) |
| 2 (20:07 local, 144 s) | `c07e21b0…` (with the proposed `SET LOCAL enable_mergejoin = off`) | **Final arm passed every criterion**: 522 of 522 measured attempts complete, 1,201 whole-source evaluations, every request p95 within its cap, whole-source p95 ≤ 321 ms, enumeration ≤ 25 ms, feed-pool headroom ≥ 414 ms. The frozen-original baseline's concentrated series was itself disrupted by the hazard (20 pool timeouts), so its HTTP pairing is invalid in this run. | [run-2/](run-2/) |

Per-series tables rendered from each artifact by [summarize_run.py](summarize_run.py):
[run 1](run-1-failed/summary.md), [run 2](run-2/summary.md). The coordinator's
[independent recount](recompute_requests.py) of each raw artifact agrees with
the harness (215 waves, 1,595 attempts, 0 failed joins, both runs). Sentinel
grep over both artifacts finds no fixture name, list name, password or filter
value; the harness's own sentinel report passed in both runs.

## Final-arm results (run 2)

| Case | c1 p95 / cap | c10 p95 / cap | c20 p95 / cap | Whole-source p95 / max | Feed-pool wait p95 / max | Headroom to 2 s |
|---|---:|---:|---:|---:|---:|---:|
| Concentrated 30k book, zero sources | 223 / 1,250 | 441 / 2,500 | 782 / 4,500 | – | 482 / 500 | 1,500 |
| Concentrated, one dense source | 322 / 1,250 | 662 / 2,500 | 1,319 / 4,500 | 309 / 335 | 727 / 899 | 1,101 |
| Concentrated, one absence source | 302 / 1,250 | 595 / 2,500 | 1,072 / 4,500 | 188 / 201 | 595 / 611 | 1,389 |
| Concentrated, five overlapping sources | 680 / 1,250 | 1,332 / 2,500 | 2,710 / 4,500 | 286 / 358 | 1,424 / 1,586 | 414 |
| Concentrated five-source independent repeat (2 × 20) | – | – | 2,725 / 4,500 | 287 / 341 | 1,488 / 1,557 | 443 |
| Typical 2,000 book, zero / five sources | 28 / 41 | 60 / 96 | 103 / 167 | – / 13 | ≤ 100 | ≥ 1,900 |
| Partial built-ins, zero / five sources | 10 / 546 | 21 / 1,015 | 43 / 2,121 | – / 321 | ≤ 1,203 | ≥ 797 |
| Empty built-ins, zero / five sources | 6 / 505 | 11 / 1,020 | 19 / 1,910 | – / 275 | ≤ 1,097 | ≥ 903 |

All values in milliseconds, nearest-rank percentiles over every measured
attempt including any failure (there were none in the final arm). The repeat
is the protocol's critical case: 40 of 40 requests and 200 of 200 source
evaluations completed, pool-wait p95/max 1,488/1,557 ms, 443 ms below the
two-second acquisition timeout (Phase A's prototype repeat: 1,622/1,662 ms).

Paired zero-source regression (final p95 versus the frozen original at the
same fixture, clock, build and concurrency; allowed = original + max(25 ms, 10%)):

| Book | Run 2 pairing | Run 1 pairing |
|---|---|---|
| Typical | exact payload and hash parity; 524→28, 844→60, 1,464→103 ms, all within allowed | exact; within allowed |
| Partial built-ins | exact; 409→10, 671→21, 1,129→43 ms | exact; within allowed |
| Empty built-ins | exact; 377→6, 695→11, 1,524→19 ms | exact; within allowed |
| Concentrated | **invalid**: the original series completed only 48 of 68 attempts (20 pool timeouts in its concurrency-20 waves), so payload stability and parity could not be established from it | exact payload and hash parity from a complete original series; 726→235, 1,260→439, 2,372→786 ms, all within allowed |

The concentrated pairing therefore rests on run 1 (a different build: the
final query text is identical, only the planner setting differs, and the
frozen-fixture database parity tests prove the setting changes no result) and
on run 2's final-arm numbers, which sit far inside run 1's allowed limits.
Under the declared protocol this is a reported limitation, not a passed
same-run check. The original baseline keeps its original planning policy by
specification, so in the vacuumed regime described below it cannot complete
twenty concurrent requests on a ten-connection pool; a further run would only
change the outcome by luck of autovacuum timing and was not attempted.

## Root cause of run 1 and the proposed planning change

Analysis on run 1's retained fixture database (transcripts in
[run-1-failed/diagnosis/](run-1-failed/diagnosis/)):

- The built-in candidates statement served the first two concentrated series
  at about 205 ms, then about 1,800–2,200 ms from the middle of the one-dense
  series onward, on identical data, parameters and role. The boundary matches
  the fixture's first autovacuum pass (19:40:24 local, about 64 s after
  seeding; PostgreSQL's launcher wakes every 60 s, so the moment is random
  within a minute of seeding), which fills the visibility map and invalidates
  cached plans.
- With a full visibility map the planner replans the per-Person
  effective-contact LATERAL from a Nested Loop Anti Join into a Merge Anti Join
  whose inner side is an index-only scan of the whole
  `contact_attempted_corrects_once` index once per Person (about 1,500 entries
  × 27,282 loops; 679k buffer hits; 2.2 s). Custom and generic plans both
  chose it; disabling index scans gave 300 ms; forcing `relallvisible` to zero
  or a later manual VACUUM that re-estimated `reltuples` flipped it back to
  the 210 ms plan, showing a knife-edge cost tie rather than a stable choice.
- The frozen original query takes 2.6–2.7 s in the same state. This is a
  pre-existing Slice 003/009 Today hazard exposed by the benchmark's timing,
  and production tables are always in the vacuumed regime.
- `SET enable_mergejoin = off` for the transaction pins the fast plan at
  212–223 ms in either regime and leaves the source statements unchanged
  (dense prefix 124 ms, absence 60 ms, combined 121 ms, no-phone 27 ms;
  membership probes sub-millisecond).

The change is implemented transaction-locally beside the approved JIT setting,
with restoration proven after success, failure, rollback and pooled reuse in
`tests/db_today_source_settings.rs`, and result parity proven by the four
frozen-fixture comparisons. It is recorded in specification §8 as a
**proposed fourth planning change pending the user's approval**; run 2 is the
measurement of that proposal.

## Limits

Fixture acceptance on one developer laptop with a ten-connection pool, not a
production capacity claim. Desktop background load was present (load average
about 4–6 before each run; the run itself pushes it to about 17). The
original-arm concentrated pairing limitation above stands. Query plans were
captured from the retained run-1 database after the run, not during it.

# Slice 011c — Isolated Today source feasibility

Measured 2026-09-06 by the Terra / xhigh feasibility lane, against a generated
PostgreSQL database using current product migrations and synthetic history.
**This is a SQL prototype, not HTTP latency or final implementation evidence.**
The original query design failed under concurrent load. Transaction-local JIT
off plus two narrowly scoped SQL corrections completed all **522 measured
normal-case requests** in the selected candidate matrix and repeat. Full result
parity passed on the synthetic fixtures. This supports the bounded implementation
proposal; final HTTP checks and a production capacity assessment remain distinct.

## Fixture and method

The corrected fixture contains 50,000 People, 46,064 inquiries, 52,306 contact
records (including 2,906 corrections), and 2,843 inbound correspondence records.
The critical viewer has a 30,000-Person assigned book but only 100 retained
built-in Today items, leaving space for ordered source candidates. Earlier
smokes with unintended inbound reminders or metadata errors are excluded.

The harness runs the existing built-in SQL, saved-source metadata lookup,
production filter decoding/reference validation, bounded built-in membership
checks and ordered, fully hydrated source prefixes in one read-only repeatable-
read transaction. It uses one timestamp and one connection per request, with
serial source evaluation and Rust candidate deduplication. It does not build
the complete application DTO/reasons or serve HTTP/Web/Operator requests.

The pool has ten connections and a two-second acquisition timeout. Source
enumeration has a proposed 250 ms allowance; each known source receives its
own proposed 500 ms allowance, with SQL cancellation and savepoint recovery.
These are measured prototype settings, not an accepted production SLA.
Prepared statements are persistent. Six warm-up waves of ten requests precede
each case; eight serial requests and two waves each at concurrency 10 and 20
supply the recorded summaries. Per-connection prepared-plan counters were not
captured in this initial run. Small sample p95 values are descriptive, not a
statistical guarantee about production traffic.

## Initial results

All rows below use the same concentrated book and corrected fixture. p95
includes **all attempts**, including partial and failed attempts; a lower
number in a failing case does not imply better complete-result latency.

| Sources | Concurrent requests | Complete / attempts | Partial | Failed | Request p95 ms | Pool wait p95 ms |
|---|---:|---:|---:|---:|---:|---:|
| None | 1 | 8 / 8 | 0 | 0 | 734 | 0.4 |
| None | 10 | 20 / 20 | 0 | 0 | 1,639 | 0.8 |
| None | 20 | 40 / 40 | 0 | 0 | 3,048 | 1,601 |
| One dense | 1 | 8 / 8 | 0 | 0 | 1,107 | 0.4 |
| One dense | 10 | 6 / 20 | 14 | 0 | 2,183 | 0.6 |
| One dense | 20 | 12 / 40 | 22 | 6 | 3,574 | 2,001 |
| One absence | 10 | 4 / 20 | 16 | 0 | 1,981 | 0.6 |
| Five overlapping | 1 | 8 / 8 | 0 | 0 | 2,804 | 0.5 |
| Five overlapping | 10 | 0 / 20 | 20 | 0 | 4,606 | 0.7 |
| Five overlapping | 20 | 0 / 40 | 20 | 20 | 4,238 | 2,003 |

At concurrency 10, five sources produced 85 source timeouts across 20
requests. At concurrency 20, they produced 83 source timeouts across the
20 requests that acquired a connection, with 20 other requests failing pool
acquisition. These outcomes do not meet the requirement that partial notices
handle failures rather than routine operation. The complete zero-source
baseline establishes that adding source work materially worsens this workload.

An intentional `pg_sleep` timeout rolled back the first source and allowed
the following four sources to complete on the same connection and snapshot.
That demonstrates one recovery path; it does not prove every production
cancellation, network failure or client behavior.

## Bounded planning comparison

Single `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` comparisons of the same query
shapes show substantial just-in-time compilation overhead. Disabling JIT only
for the isolated transaction reduced these measurements:

| Query | Default execution ms | Included JIT ms | JIT off execution ms |
|---|---:|---:|---:|
| Built-in, concentrated book | 841.030 | 436.960 | 443.796 |
| Dense non-B prefix plus hydration | 404.386 | 210.572 | 197.796 |
| Absence non-B prefix plus hydration | 418.218 | 206.325 | 202.579 |

These are separately prepared EXPLAIN statements with bound values, not
`EXPLAIN EXECUTE` captures of each warmed pool connection's cached plan. They
do not establish which generic/custom plan every benchmark request used.
These single plan executions are not the repeated request p95 figures above.
They justify a bounded concurrency comparison, not a conclusion that turning
JIT off resolves the failure. PostgreSQL documents this compilation overhead
and its visibility in EXPLAIN in [When to JIT?](https://www.postgresql.org/docs/18/jit-decision.html).
The later prototype uses transaction-local settings; no global database or
shared development configuration was changed.

Independent plan review also identified avoidable latest-source probes on an
absence filter and inquiry lookups that could use existing index prefixes
with explicit same-Organization predicates. These are hypotheses for measured
static SQL corrections, not evidence that persisted activity state is needed.

The repeated [JIT-off comparison](metrics/report-critical-jit-off.json) improved
the five-source case substantially, but did not pass the whole checkpoint:

| Concurrent requests | Complete / attempts | Partial | Failed | Request p95 ms | Pool wait p95 ms |
|---|---:|---:|---:|---:|---:|
| 1 | 8 / 8 | 0 | 0 | 1,024 | 0.4 |
| 10 | 20 / 20 | 0 | 0 | 2,311 | 0.7 |
| 20 | 24 / 40 | 0 | 16 | 3,242 | 2,002 |

The five-source failures in this comparison were pool-acquisition timeouts;
its admitted requests did not have source timeouts. The one-absence-source
case also retained one partial result in 40 attempts at concurrency 20. The
comparison therefore supports testing the small static SQL corrections; it
does not justify accepting routine failures or merely raising pool capacity.

## Combined bounded query candidate

The next variant applies the unused-Source lookup guard to the new source
prefix query and adds same-Organization predicates to exactly three built-in
inquiry probes (latest, waiting and count), together with transaction-local
JIT off. It changes no filter, ordering, cap, pool limit, index or persisted
activity state. The parity and supplemental checks below passed; these remain
prototype results for the proposed implementation plan.

The [combined critical matrix](metrics/report-critical-candidate-guards-jit-off.json)
completed all 272 measured requests across zero, one dense, one absence and
five overlapping sources, with no unexpected partial or failed request.
The five-source results were:

| Concurrent requests | Complete / attempts | Request p95 ms | Pool wait p95 ms | Source p95 ms |
|---|---:|---:|---:|---:|
| 1 | 8 / 8 | 822 | 0.5 | 172 |
| 10 | 20 / 20 | 1,903 | 0.8 | 393 |
| 20 | 40 / 40 | 3,553 | 1,967 | 400 |

The concurrency-20 pool wait is close to its 2,000 ms timeout. This is a reason
to retain the raw repeat and final HTTP gate, not a production-capacity
guarantee. Extra application work or environmental variation could exhaust that
earlier run's margin; the improved repeat below does not erase it.

Single candidate plans show the built-in query at 285.5 ms (479,141 root
shared hits), the dense prefix at 184.5 ms, and the absence prefix at 88.5 ms
(152,547 root shared hits). These retain the same 100 built-in and 101 prefix
rows. See the candidate [built-in plan](plans/builtin_concentrated_candidate_guards_jit_off.json),
[dense plan](plans/source_dense_non_B_candidate_guards_jit_off.json) and
[absence plan](plans/source_absence_non_B_candidate_guards_jit_off.json).

The [remaining matrix](metrics/report-remaining-candidate-guards-jit-off.json)
also completed all 210 measured requests without partial results, pool errors
or source timeouts. Five-source p95 values were:

| Queue shape | Concurrency 1 ms | Concurrency 10 ms | Concurrency 20 ms |
|---|---:|---:|---:|
| Representative 2,000-Person book, full/truncated B | 41 | 69 | 143 |
| 100-Person book, partial B | 534 | 1,148 | 2,217 |
| Empty B | 623 | 1,121 | 2,207 |

The full/truncated case correctly skips non-B prefixes; it cannot substitute
for the spare-queue measurements. The supplemental matrix used five serial
samples and one wave each at concurrency 10 and 20 after its warm-ups.

[Parity evidence](metrics/parity-candidate.json) records equal full selected
row payloads and explicit output order for the original and corrected built-in
queries across four viewers (100/201/100/0 rows), every actual source definition,
and a valid narrow `assigned_to: me` plus Source definition. Source comparisons
include the last-contact sort key. Both sides ran in one read-only repeatable-
read snapshot with a common clock. These fixtures do not replace production
edge-case and authorization tests.

The [raw concurrency-20 repeat](metrics/five-source-c20-raw-candidate-guards-jit-off.json)
retains every request from two further twenty-request waves. All **40 requests
and 200 source attempts completed**, with zero timeouts. Request p95/max was
3,017/3,082 ms; pool wait p95/max was 1,622/1,662 ms. The raw file's SHA-256 is
`6c2f5149c7c0e8dba86662999f6305912514851f7f5d14b61aa07c16fd3edec1`.
Both the earlier tighter-margin result and this repeat are retained. No failed
candidate run was discarded or averaged away.

## Evidence, reproduction and cleanup

- [Initial critical summaries and forced-timeout record](metrics/report-critical.json).
- [First valid five-source smoke](metrics/smoke.json): 2,528 ms total, B=100,
  all five sources complete, 202 distinct non-built-in candidates before the
  remaining 100-slot cap. This was a smoke, not the repeated warm baseline.
- Built-in [default plan](plans/builtin_concentrated_default.json) and
  [JIT-off plan](plans/builtin_concentrated_jit_off.json).
- Dense source [default plan](plans/source_dense_non_B_default.json) and
  [JIT-off plan](plans/source_dense_non_B_jit_off.json).
- Absence source [default plan](plans/source_absence_non_B_default.json) and
  [JIT-off plan](plans/source_absence_non_B_jit_off.json).

The initial matrix retained aggregate summaries and the forced-timeout record,
not every individual request record. The raw final burst record above provides
independently recomputable per-attempt evidence; all other sample limitations
remain explicit.

The [reproduction recipe](REPRODUCE.md), [setup script](setup.sh),
[harness](src/main.rs), Cargo manifest/lockfile, synthetic seed and SQL snapshots
are retained beside these reports. `sql/` and `revisions/sql-candidate/` hold
the selected variant; `revisions/sql-baseline/` and the preserved earlier
harness hold the original query behavior. The original baseline checksum list
retains its historical `/tmp` paths. Use the final relative
[artifact checksums](ARTIFACT_SHA256.txt) and [source manifest](MANIFEST.json)
for the packaged evidence. Preliminary hashes from abandoned, unexecuted
projections were excluded.

The [cleanup record](cleanup.json), verified by Terra, reports removal of the
generated database with zero matching databases and sessions. The temporary
credential URL file, migration log, build output and test processes were also
removed. Shared development data/services were preserved. No production code
or migration was changed; the SQL/Rust files here are benchmark artifacts.

The [specification's Phase B gate](../../../specs/SLICE_011c.md) still requires
complete authenticated HTTP results, explicit request/source latency bounds,
all raw samples, pool-margin reporting and actual implementation parity,
authorization and recovery tests. This evidence supports the reviewed proposal,
not completed application implementation or production acceptance.

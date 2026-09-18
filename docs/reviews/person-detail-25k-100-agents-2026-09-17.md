# Person detail: 25,000-Person pool and 100 agents

The initial run, `e1183d2d462a`, was launched with:

```sh
python3 -m e2e.load_person --agents 100 --pool-size 25000 --duration-seconds 30
```

The one-agent and hundred-agent conditions each ran for 30 seconds. Every agent
used an independent deterministic pseudo-random stream over the entire 25,000
Person pool, with one authenticated HTTP request in flight. Requests traversed
nginx, session authentication, Axum, SQLx, PostgreSQL and the normal Person-detail
route. The pool consists of valid Organization-scoped `person` rows and matching
stages created directly in the isolated benchmark database. It intentionally has
no inquiries, history, contact methods, tags, fields or tasks: serially creating
25,000 People through the intake command would measure its Organization-wide
advisory lock rather than the requested read workload.

| Condition | Requests | Valid 200 | Client timeouts | Throughput | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| One agent | 4,074 | 4,074 | 0 | 135.8/s | 7.5 ms | 10.9 ms | 15.7 ms | 68.1 ms |
| 100 agents | 14,007 | 13,980 | 27 | 464.0/s | 200.4 ms | 291.1 ms | 339.2 ms | 10.017 s |

All 27 failures were client-side timeouts at approximately ten seconds. The run
does not prove whether their corresponding server requests later completed. The
100-agent phase had 12,352 requests over 100 ms, 104 over 500 ms, and 47 over two
seconds. It performed about 1.01 million top-level application-role PostgreSQL
executions (roughly 72 per HTTP request); that count includes transaction control
and background work.

PostgreSQL CPU averaged 103.2% and peaked at 131.4% in 500 ms Docker samples;
API CPU averaged 166.8% and peaked at 183.3%. PostgreSQL had no physical block
reads after the first phase, no temporary-file allocation, no deadlocks and no
observed lock waiters. The data pool therefore did not create a disk bottleneck in
this run.

The notable limit was connection behavior: the application pool held ten
connections throughout the hundred-agent phase, and up to all ten were sampled as
`idle in transaction`. The read middleware opens and holds a guard transaction,
then the Person-detail handler acquires another pooled connection for its own work.
At high concurrency this creates severe pool pressure; the request path can require
two connections at once. This finding is independent of whether data is cached.

## Authorized single-transaction refactor

Run `a649272ac1ed` repeated the identical workload after Person detail was
changed to reuse the request's authenticated context and perform its protected
read plus all page data reads in one connection/transaction. The E2E SQL profile
of the real lead journey independently reduced `GET /api/people/{id}` from 55 to
27 SQLx executions. The route's authorization and workspace/history/activity
gates remain in place; composed query helpers now receive the already-authorized
connection instead of independently opening another protected read.

| Condition | Requests | Valid 200 | Client timeouts | Throughput | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| One agent | 6,558 | 6,558 | 0 | 218.6/s | 4.6 ms | 6.2 ms | 8.3 ms | 43.0 ms |
| 100 agents | 28,381 | 28,381 | 0 | 909.6/s | 86.6 ms | 120.4 ms | 1.113 s | 7.709 s |

The 100-agent condition sustained about 1.96 times the prior throughput and
returned every request before the ten-second client deadline. PostgreSQL recorded
no physical block reads, temporary-file allocation or deadlocks. The pool still
reached ten connections and some were idle in transaction because the read itself
is a transaction; the refactor removes the prior second, overlapping transaction
per Person-detail request.

Evidence: [SQL profile](../../.e2e/runs/f6ef3cd8cd7f/leads-1-a1/sql-profile.json),
[load summary](../../.e2e/runs/a649272ac1ed/personload-1-a1/load-summary.json),
and [100-agent request sample](../../.e2e/runs/a649272ac1ed/personload-1-a1/load-hundred-agents.json).

## Purpose-built detail read model

The final branch revision replaces the remaining stable-section fanout with one
Organization-scoped JSON snapshot and replaces the timeline fanout with one
ordered union across every available history source. A single compatibility
probe preserves rolling-schema support for the optional admitted/recovered
history tables. Workspace reader configuration and membership authorization now
share one statement; scoped existence and the history/activity completeness
gates execute inside the snapshot statement. The real, history-bearing lead
journey measured **5 SQLx executions** at p50 and p95 for
`GET /api/people/{id}`, down from the original 55.

Final-tree run `956081efd69a` repeated the 25,000-Person random-access workload:

| Condition | Requests | Valid 200 | Client timeouts | Throughput | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| One agent | 10,089 | 10,089 | 0 | 336.3/s | 2.8 ms | 4.0 ms | 5.5 ms | 43.7 ms |
| 100 agents | 32,836 | 32,836 | 0 | 1,063.2/s | 75.7 ms | 100.4 ms | 1.094 s | 2.321 s |

Relative to the original path, one-agent throughput increased 2.48 times and
100-agent throughput increased 2.29 times; 100-agent p95 fell from 291.1 ms to
100.4 ms. Top-level PostgreSQL executions during the 100-agent phase fell from
about 72 to about 7 per attempted HTTP request, including transaction control
and background work. Every response was valid. The pool remained fixed at ten
connections, with no sampled lock waiters, no physical block reads in the
100-agent phase, no temporary-file allocation and no deadlocks. Since 100
simultaneous members is outside D-050's v1 envelope, the long tail is reported
for trend watching rather than used as a release gate.

Final evidence: [SQL profile](../../.e2e/runs/68c243ca24db/leads-1-a1/sql-profile.json),
[all-family E2E](../../.e2e/runs/990c934b3f28),
[load summary](../../.e2e/runs/956081efd69a/personload-1-a1/load-summary.json),
and [100-agent request sample](../../.e2e/runs/956081efd69a/personload-1-a1/load-hundred-agents.json).

This is an intentionally adversarial, cache-resident, data-light read workload. It
does not represent full page fanout, realtime sessions, realistic timeline size,
write contention, production hardware or a formal capacity result. The 25,000-row
pool and random selection eliminate the earlier ten-record cycling artifact, but a
realistic history-bearing projection remains the next meaningful comparison.

The benchmark verified foreign-Organization denial before loading and verified the
final pool count after loading. It exited successfully and removed every owned
container, network and volume. Evidence: [summary](../../.e2e/runs/e1183d2d462a/personload-1-a1/load-summary.json), [100-agent request sample](../../.e2e/runs/e1183d2d462a/personload-1-a1/load-hundred-agents.json), and [cleanup record](../../.e2e/runs/e1183d2d462a/personload-1-a1/run.json).

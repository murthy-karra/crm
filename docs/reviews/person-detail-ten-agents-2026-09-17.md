# Person-detail load: 10 concurrent agents — 2026-09-17

**PostgreSQL handled this bounded test without HTTP errors or invalid responses.**
Run `48e3de456ec0`, base revision `82b185d` plus existing SQL profiling additions
(disabled during this run). All API/Web/client images were reused from their
matching content hashes. This is a diagnostic measurement under D-050; the user
explicitly requested ten concurrent agents. No production capacity claim follows.

## Workload

- One isolated API and PostgreSQL, reached through the existing E2E nginx.
- Ten distinct member accounts in one Organization, each reading a different
  Person; ten Persons total, one inquiry and four history facts each.
- All identities and People created through real application APIs.
- Warmed Person endpoints. Each agent continuously issues a GET with one request
  in flight and no think time. Client validates Person ID, contacts, inquiry and
  history counts on every successful response and disposes response buffers.
- Playwright authenticated HTTP contexts; browser rendering, websocket sessions,
  full-page request fanout and large-book performance are outside this run.
- One agent for 15 seconds, ten for 30 seconds, one for 10 seconds afterward.
- Docker VM: 10 CPUs, 16,819,609,600 bytes RAM, aarch64, Docker 29.4.0.
  Shared developer host; no container-specific CPU limits were introduced.

## Results

| Phase | Requests | Valid HTTP 200 | Requests/s | p50 ms | p95 ms | p99 ms | Max ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 agent, baseline | 2,122 | 2,122 | 141.4 | 7.3 | 10.0 | 13.3 | 66.9 |
| 10 agents | 17,599 | 17,599 | 586.4 | 16.4 | 23.8 | 29.4 | 1,063.3 |
| 1 agent, recovery | 1,638 | 1,638 | 163.7 | 5.4 | 9.2 | 11.8 | 20.4 |

All 21,359 requests returned valid HTTP 200 responses. In the ten-agent phase,
three requests exceeded 500 ms (and these were the only requests above 100 ms).
The run did not isolate their cause. Recovery latency returned to the initial
range without a persistent backlog.

| Phase | PostgreSQL mean CPU | API mean CPU | PostgreSQL executions/s |
|---|---:|---:|---:|
| Baseline | 40.5% | 45.4% | 10,202 |
| Ten agents | 132.5% | 150.5% | 42,236 |
| Recovery | 42.1% | 46.6% | 11,807 |

Docker CPU uses **100% = one CPU core**. Ten-agent PostgreSQL usage averaged
about 1.33 cores, peaking in samples at 135.7%; API usage averaged 1.51 cores.
PostgreSQL's last ten-agent memory sample was 176.8 MiB. There were 15 resource
samples in that phase; these are sampled measurements, not exact maxima.

`pg_stat_statements` recorded 1,267,671 top-level application-role executions in
the ten-agent phase, including transaction-control statements and background work.
This is approximately 72 per HTTP request and is not directly comparable with the
55-event SQLx count, which omits several transaction-control paths. Trigger-internal
statements are excluded by the default top-level tracking mode. No SQL text or
bound parameters were exported into the measurement artifacts.

The ten-agent phase had zero physical block reads, zero temporary bytes and zero
deadlocks in database counter deltas. No lock waiters appeared in 500 ms activity
samples. The API had ten database connections; sampled idle-in-transaction count
reached ten. That includes the application's read guard transactions and does not
establish a leak. This was a small, cache-resident workload with minimal history.

## Interpretation and limits

The result does **not** support claiming PostgreSQL melts down at ten agents.
The repeated-query overhead is real, but PostgreSQL handled the small indexed
reads well here. Ten clients generated about 4.1 times the one-client throughput,
with higher per-request latency. This does not isolate whether database CPU,
application scheduling, client overhead, pool limits or networking set the limit.

This does not establish capacity for 10,000 agents, a 25,000-Person book, substantial
history, concurrent mutations or full browser page fanout. No capacity extrapolation
or architectural rewrite is justified solely by this small run.

## Verification and reuse

Run `python3 -m e2e.load_person`. This is an explicitly invoked measurement, not
part of normal test gates. The script reuses E2E image matching, provisioning,
seeding, network isolation, redaction and ownership-checked teardown. SQLx verbose
profiling is off. Only its owned PostgreSQL instance loads `pg_stat_statements`,
and only its owned audit role gains statistics-reading permission.

The run exited 0, cross-Organization access returned 404, final Person/inquiry/fact
counts were unchanged, all resource samples succeeded, and cleanup was
`verified_empty`. Python syntax and `git diff --check` passed. After this run,
the host launcher received metadata-only additions for future script hashes and
Docker resource capture; the actual tested Node workload is preserved in evidence.

Evidence: [load summary](../../.e2e/runs/48e3de456ec0/personload-1-a1/load-summary.json),
[ten-agent samples](../../.e2e/runs/48e3de456ec0/personload-1-a1/load-ten-agents.json),
[resource summary](../../.e2e/runs/48e3de456ec0/personload-1-a1/resource-summary.json),
[cleanup record](../../.e2e/runs/48e3de456ec0/personload-1-a1/run.json).

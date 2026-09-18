# Person detail: 10 versus 100 database connections

The matched history-heavy runs used 100 authenticated agents, 25,000 People,
a 1,000-Person active cohort, and 30 seconds per condition. Each response had
60 timeline entries and a median encoded size of 36,719 bytes. Both runs returned
only valid HTTP 200 responses and preserved the cross-Organization denial.

| API pool | Requests | Throughput | p50 | p95 | p99 | Maximum |
|---:|---:|---:|---:|---:|---:|---:|
| 10 | 27,934 | 902.5/s | 85.7 ms | 123.1 ms | 1.115 s | 7.726 s |
| 100 | 28,488 | 922.3/s | 86.5 ms | 116.1 ms | 1.116 s | 7.711 s |

Raising the pool ceiling tenfold improved throughput by 2.2% and p95 by 5.7%.
It did not improve p50, p99, or the roughly 7.7-second maximum. Application
connections rose from a constant 10 to an average of 63.1 and a maximum of 71.
Average sampled active PostgreSQL sessions rose only from 1.27 to 1.71, while
idle-in-transaction sessions rose from 5.5 to 8.8 and peaked at 31.

The larger pool also increased measured PostgreSQL execution time per request
from 1.23 ms to 1.50 ms, a 22.7% increase. During the concurrent phase, sampled
API CPU rose from 201.7% to 216.7% and PostgreSQL CPU rose from 145.8% to 165.5%.
Neither run recorded a deadlock, temporary-file allocation, or sampled lock
waiter.

The ten-connection pool was not the main throughput limit in this workload.
One hundred connections consumed substantially more database sessions and CPU
for a small throughput gain, and it left the long latency tail unchanged. A
moderate pool should be tested before choosing a production value; 20 and 30 are
the useful next comparison points. The repeated transaction rollbacks and sampled
idle-in-transaction sessions should also be investigated before adding more
connections.

The application now exposes `CRM_DATABASE_MAX_CONNECTIONS`, default 10 and
bounded from 1 to 200. The workspace-read admission limit remains one half of
the configured pool, so this comparison raised that limit from 5 to 50 as well
as increasing the SQLx pool ceiling.

Evidence: [10-connection summary](../../.e2e/runs/be4becce7214/personload-1-a1/load-summary.json),
[10-connection activity](../../.e2e/runs/be4becce7214/personload-1-a1/load-hundred-agents.json),
[100-connection summary](../../.e2e/runs/bafc85e7e9c3/personload-1-a1/load-summary.json),
[100-connection activity](../../.e2e/runs/bafc85e7e9c3/personload-1-a1/load-hundred-agents.json),
and [100-connection run record](../../.e2e/runs/bafc85e7e9c3/personload-1-a1/run.json).

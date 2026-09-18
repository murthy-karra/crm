# Person detail: history-heavy active cohort

Run `be4becce7214` used the final five-execution Person-detail implementation:

```sh
python3 -m e2e.load_person --fixture history-heavy --agents 100 \
  --pool-size 25000 --heavy-pool-size 1000 --duration-seconds 30
```

The Organization contained 25,000 People. Random requests targeted a 1,000-Person
active cohort; every target had 3 contacts, 5 tags, 12 live notes, 12 open tasks,
8 completed tasks, 20 contact-attempt facts, 10 stage facts and 10 assignment
facts. Every response validator required the requested Person id, 60 ordered
timeline entries, 12 open tasks, 3 contacts and 5 tags. The median response body
was 36,719 bytes.

| Condition | Requests | Valid 200 | Throughput | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|---:|---:|
| One agent | 6,207 | 6,207 | 206.9/s | 4.4 ms | 7.1 ms | 11.6 ms | 66.1 ms |
| 100 agents | 27,934 | 27,934 | 902.5/s | 85.7 ms | 123.1 ms | 1.115 s | 7.726 s |

Compared with the final data-light run (`956081efd69a`), the heavy response reduced
one-agent throughput by 38.5% and 100-agent throughput by 15.1%. One-agent p95
rose from 4.0 to 7.1 ms; 100-agent p95 rose from 100.4 to 123.1 ms. PostgreSQL
client-visible execution count stayed at about seven per attempted HTTP request,
including transaction control and background work, while measured PostgreSQL
execution time per 100-agent request rose from about 0.34 ms to 1.23 ms. The
remaining cost is query work and moving/encoding a 36.7 KB response, rather than
another round-trip explosion.

The 100-agent phase recorded 53 physical block reads after the baseline warm-up,
no temporary-file allocation, no deadlocks and no sampled lock waiters. The ten-
connection application pool averaged 5.5 idle-in-transaction connections and
peaked at nine; it returned every request before the ten-second client deadline.

This is a substantial-history read test, not a production capacity claim. Its
1,000-Person active cohort is intentionally more realistic than giving all 25,000
People identical dense histories, but it becomes largely cache-resident after the
baseline phase. Production-shaped hardware and a distribution derived from real
customer history remain necessary for capacity planning.

The matched [10-versus-100 connection comparison](person-detail-pool-10-vs-100-2026-09-17.md)
found only a 2.2% throughput gain from the larger pool, with no improvement to
the p99 or maximum latency.

Evidence: [summary](../../.e2e/runs/be4becce7214/personload-1-a1/load-summary.json),
[100-agent samples](../../.e2e/runs/be4becce7214/personload-1-a1/load-hundred-agents.json),
[container samples](../../.e2e/runs/be4becce7214/personload-1-a1/container-stats.jsonl),
and [cleanup record](../../.e2e/runs/be4becce7214/personload-1-a1/run.json).

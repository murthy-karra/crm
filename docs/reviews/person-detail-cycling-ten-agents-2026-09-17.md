# Person-detail load with cycling targets — 2026-09-17

Run `91b0ef791f7e`: `python3 -m e2e.load_person --total-seconds 30`.
15 seconds with one agent, then 15 seconds with ten distinct agents in the same
Organization. Every agent cycles through all ten Persons and advances on every
request. Initial positions are staggered; independent completion rates can later
overlap. All eleven agent/phase sequences visited all ten Persons and were checked
against the exact cyclic sequence. Every HTTP 200 payload matched its requested
Person and expected contacts, inquiry and history counts.

| Phase | Successful requests | Requests/s | Median ms | p95 ms | p99 ms | Max ms |
|---|---:|---:|---:|---:|---:|---:|
| One agent, 15 seconds | 2,245 | 149.6 | 7.0 | 9.7 | 12.7 | 25.3 |
| Ten agents, 15 seconds | 8,754 | 582.9 | 16.6 | 24.1 | 29.5 | 113.6 |

All 10,999 requests succeeded with valid payloads. Load durations including final
in-flight completions were 15.005 and 15.019 seconds. Setup, monitoring snapshots
and teardown are outside the scheduled 30 seconds of load.

During ten-agent load, sampled PostgreSQL CPU averaged 133.0% (1.33 CPU cores),
peaking at 134.8%. API CPU averaged 147.9%. PostgreSQL recorded 630,576 top-level
application-role executions, including transaction-control and background work.
There were zero physical block reads, temporary bytes or deadlocks in that phase.

Scope remains a tiny warmed book (ten Persons, one inquiry/four facts each),
continuous authenticated HTTP requests through nginx and the real API, one request
in flight per agent, no think time. This does not include browser rendering,
websocket sessions, full-page fanout or large-book behavior. Docker has ten CPUs
and approximately 16 GiB memory on a shared developer machine. SQLx per-query
profiling was off. The three matching API/Web/client images were reused.

Foreign-Organization access returned 404 before load. Final Person/inquiry/fact
counts remained intact. The run exited 0 and owned resource cleanup was verified.
Python syntax and whitespace checks passed. No application implementation changed.

Evidence: [summary and per-agent coverage](../../.e2e/runs/91b0ef791f7e/personload-1-a1/load-summary.json),
[individual ten-agent requests](../../.e2e/runs/91b0ef791f7e/personload-1-a1/load-ten-agents.json),
[run metadata and cleanup](../../.e2e/runs/91b0ef791f7e/personload-1-a1/run.json).
The exact tested Node workload is preserved as `workload.mjs` alongside the evidence.

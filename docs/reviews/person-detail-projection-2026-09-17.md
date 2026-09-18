# Person detail current-state projection

The Person-detail read now loads current state from
`person_detail_projection`, loads immutable history from its existing union,
adds viewer-specific task permissions in Rust, and explicitly commits the
authorized read transaction. Typed interactive commands and migration workers
rebuild the current-state projection inside the same transaction as canonical
changes. Canonical relational and fact tables remain authoritative.

All ten stateful Web journey families passed in run `5318c24cf391` with verified
cleanup. Focused projection runs also passed leads, relationships, tasks,
correspondence, and migration before the combined run.

The matched history-heavy benchmark used 100 authenticated agents, 25,000
People, a 1,000-Person active cohort, 60 timeline entries per response, a median
36,719-byte response, the default ten-connection pool, and 30 seconds per
condition.

| Implementation | Requests | Throughput | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|---:|
| Relational current-state aggregation | 27,934 | 902.5/s | 85.7 ms | 123.1 ms | 1.115 s | 7.726 s |
| Current-state projection | 27,663 | 877.0/s | 88.9 ms | 128.8 ms | 1.121 s | 2.162 s |

Every projection response was valid and the foreign-Organization request was
denied. The projection run used about 0.924 ms of measured PostgreSQL execution
time per request, down from 1.226 ms, a 24.6% reduction. Sampled PostgreSQL CPU
fell from 145.8% to 118.0%. Sampled API CPU rose from 201.7% to 211.9%, consistent
with the remaining history and JSON work dominating this local closed-loop test.
Throughput and central percentiles were effectively flat within run-to-run
variation; this change reduces database cost rather than making a 36.7 KB
response materially cheaper to encode and transfer.

Explicit transaction completion changed the 100-agent database counters from
27,995 rollbacks for 27,934 requests to six rollbacks for 27,663 requests.
Average sampled idle-in-transaction connections fell from 5.5 to 2.7. No
deadlocks, temporary-file allocation, or sampled lock waiters occurred.

The current-state projection does not copy history. History remains immutable
and is assembled separately so its growth never rewrites the JSONB projection.
The next independent optimization would be a bounded/paginated history contract;
it is not required for projection correctness.

The final lead SQL profile removed the pre-production schema-compatibility probe
and retained the fail-closed history/activity review boundary as one combined
query. All 23 ordinary successful Person-detail reads used exactly six logged
SQLx executions: authentication, workspace authorization, the combined review
fence, projection lookup, history union, and explicit commit. Short denied/error
paths exited after three to five executions. The heavy benchmark above used one
schema-compatibility probe where the final tree uses the combined review fence;
its capacity figures remain directional, while the final-tree journey below is
the authoritative execution-count and correctness evidence.

Evidence: [projection summary](../../.e2e/runs/376ba5440747/personload-1-a1/load-summary.json),
[projection activity](../../.e2e/runs/376ba5440747/personload-1-a1/load-hundred-agents.json),
[projection cleanup](../../.e2e/runs/376ba5440747/personload-1-a1/run.json),
and [all-family E2E run](../../.e2e/runs/5318c24cf391/summary.json).
Final-tree execution evidence: [lead SQL profile](../../.e2e/runs/9bb9871f541b/leads-1-a1/sql-profile.md).

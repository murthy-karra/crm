# E2E SQL execution measurements — 2026-09-17

Measured run: `39d9a6d67a25`, base revision `82b185d` plus the opt-in profiling
changes. The exact application input fingerprints and immutable image IDs are in
[build-cache.json](../../.e2e/runs/39d9a6d67a25/build-cache.json); source metadata
is in [source.json](../../.e2e/runs/39d9a6d67a25/source.json).

## Findings

Some individual migration HTTP requests execute more than 100 SQLx operations.
The highest observed count was **136** for People-import confirmation. Ordinary
CRM requests in these journeys peaked at **55** for Person detail. The lead
journey executed **3,750** operations across **258** HTTP requests, including
multiple actors, realtime refetches, denials and recovery checks.

| Request | Observed SQLx executions |
|---|---:|
| POST /api/migrations/fub/imports/{id}/confirm | 136 |
| POST /api/migrations/fub/family-refreshes | 115 |
| POST /api/migrations/fub/activity-imports/{id}/confirm | 111 |
| POST /api/migrations/fub/metadata-imports/{id}/confirm | 110 |
| POST /api/migrations/fub/core-change-reports | 106 |
| GET /api/people/{id} — full successful lead detail response | 55 |
| GET /api/today — successful lead journey requests | 35–37 |
| POST /api/inquiries — highest in lead journey | 33 |
| POST /api/people/{id}/stage — lead journey | 15 |

The sampled 55-execution Person-detail response contained 27 selects, 18
transaction-local setup executions, 9 workspace-check executions and 1 explicit
transaction-control execution. Repeated context setup/read guards are a concrete
optimization candidate. Any consolidation must preserve transaction-local
capabilities and tenant/workspace authorization; this task changes no domain
behavior and does not remove checks.

## Journey totals

HTTP totals cover requests started during the recorded journey window and exclude
health endpoints. Seed and API startup are outside this window. Unscoped work is
reported separately; it includes background workers, polling and pool maintenance.

| Family | HTTP requests | HTTP executions | Maximum/request | Unscoped executions |
|---|---:|---:|---:|---:|
| calls | 174 | 3,375 | 55 | 256 |
| correspondence | 140 | 2,442 | 55 | 167 |
| leads | 258 | 3,750 | 55 | 334 |
| lists | 327 | 3,828 | 55 | 355 |
| migration | 1232 | 42,112 | 136 | 60,586 |
| operator | 58 | 1,242 | 55 | 783 |
| relationships | 292 | 5,532 | 55 | 467 |
| routing | 177 | 2,333 | 55 | 220 |
| tasks | 184 | 3,736 | 55 | 287 |
| workspace | 289 | 2,364 | 55 | 548 |

Across all ten families: **3,131 HTTP requests and 70,714 HTTP-scoped executions**.
All query events with a non-null request ID had a matching request record.

Migration alone recorded 60,586 unscoped executions. Its synthetic fixture runs
accelerated worker ticks and repeated run-once calls, so this is not a per-Person
import cost or a prediction of production worker load. See the existing
[migration fixture boundaries](../tasks/WEB_E2E_REMAINING.md).

The lead step “Change stage and hand work to another agent” accumulated 740
executions across 42 requests. A step is a multi-action, multi-actor verification
sequence, not one user click. Realtime-driven refetches are included.

## Metric limits

These are SQLx 0.8.6 `QueryLogger` execution events, **not exact wire round trips**.
The locally inspected driver omits BEGIN/savepoint creation and drop-triggered
rollback from this logger. Prepared statement setup may introduce additional
protocol exchanges; a single execution may contain multiple statements. Trigger
SQL stays inside PostgreSQL and is not counted as additional client executions.
Failed/cancelled requests remain in the data, including requests with zero queries.

Elapsed times are client-observed SQLx execution durations; they include database
work/waiting and transfer/driver overhead, not a measurement of network latency or
server CPU alone. No load-capacity or production latency claim follows from this
small synthetic E2E run. This is diagnostic evidence under D-050, not a new
performance gate or a comparison against a pgrx implementation.

## Implementation and verification

Run again with:

```sh
./scripts/e2e --all --concurrency 4 --timeout 900 --sql-profile
```

Profiling is off by default. The isolated APIs enable `CRM_SQL_PROFILE=1` and
write allowlisted numeric/category events plus method/route templates. SQL text,
parameters, query strings and bodies are excluded from profiling. The normal
formatter cannot emit SQLx query text while profiling is on. Each family produces
`sql-profile.json` (including individual requests and step counts) and
`sql-profile.md`. Missing/correlationally empty telemetry fails profiling.

- E2E command above exited 0: **10 families / 103 steps passed**, four concurrent
  environments, 159.9-second attempt window (builds excluded).
- All 45 family pairs passed isolation checks; every environment reports
  `verified_empty` cleanup. The three Web/browser targets were reused; the API
  and migration API were rebuilt once with the instrumentation.
- 12 Python runner/profile tests passed, including overlapping request
  attribution, separation of setup/background work, and missing-telemetry failure.
- Rust telemetry visitor regression passed (1 test); SQL fields cannot populate
  the retained profile fields except the allowlisted category/numeric duration.
- `cargo clippy -p crm-api --lib -- -D warnings`, Cargo format check and
  `git diff --check` passed. Rust checks used an isolated `/tmp` target directory.
- The existing service-free `scripts/check` now includes the profile parser tests.

Evidence: [run summary](../../.e2e/runs/39d9a6d67a25/summary.json),
[lead report](../../.e2e/runs/39d9a6d67a25/leads-1-a1/sql-profile.md),
[migration report](../../.e2e/runs/39d9a6d67a25/migration-1-a1/sql-profile.md).
The detailed reports are retained in the ignored local run directory.

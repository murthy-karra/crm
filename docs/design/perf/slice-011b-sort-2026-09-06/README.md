# Slice 011b-sort — performance gate evidence

Run 2026-09-06 (late evening) by the coordinator (Claude Fable 5.1) against
the retained 100,060-Person "Perf Test Realty" organization in the development
database, with the branch's debug `crm-api` binary serving port 3000, as
[ENVIRONMENT.txt](ENVIRONMENT.txt) records. Specification §4 defines the gate:
same-run comparison with the `4-clause combo` case, subselect loops at most
501 per request, no LATERAL node looping over the organization, and plans
observed after six prior executions of each prepared statement.

## Latency, same run (`./scripts/perf bench`, ms, 15 iterations, 500 rows, truncated)

| Case | Unfiltered p95 | With one `assigned_to` clause p95 |
|---|---:|---:|
| `4-clause combo` (existing baseline case, filtered) | 343.6 | – |
| `created.asc` | 20.0 | 19.8 |
| `name.asc` | 132.0 | 55.8 |
| `name.desc` | 126.9 | 54.5 |
| `stage.asc` | 64.5 | 37.6 |
| `stage.desc` | 65.1 | 37.9 |
| `assignee.asc` | 72.1 | 38.4 |
| `assignee.desc` | 72.8 | 36.9 |

Every sorted case is far inside the band; the whole table with the unchanged
existing cases is in [bench.txt](bench.txt).

## Plans on the seventh execution ([plans-explain-analyze.txt](plans-explain-analyze.txt))

| Statement | Unfiltered exec | One clause exec | Plan | Nodes looping > 501 |
|---|---:|---:|---|---|
| `created.asc` | 3.9 ms | 4.2 ms | custom | only the per-output-row subselects at 502 loops |
| `name.asc` / `name.desc` | 98.9 / 99.9 ms | 25.8 / 25.1 ms | custom | none |
| `stage.asc` / `stage.desc` | 47.3 / 47.5 ms | 16.4 / 15.4 ms | custom | none |
| `assignee.asc` / `assignee.desc` | 52.5 / 52.1 ms | 14.9 / 15.6 ms | custom | none |

Observation: after the fifth execution PostgreSQL 18 compared its generic
plan against the average custom plan and kept the custom plan for every
statement (the NULL clause parameters fold the unused LATERAL probes away,
which a generic plan cannot do). So the `latest_src` guard and the
`plan_cache_mode` lever, both pre-declared in the specification, were not
needed and were not applied. This is a property of the planner's cost
comparison, which statistics could change; both levers stay recorded.

## Limits

Developer laptop, debug build, one organization, fifteen samples per case;
not a production capacity claim. The `4-clause combo` figure here (343.6 ms)
is the same-run comparison the specification requires; the 2026-08-29
baseline document measured 318 ms for that case on a since-reseeded org.

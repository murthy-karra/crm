# Mobile006 — hot-query inventory

The realistic Mobile006 plan run passed: 18 exact statements, zero failures.
The paired Person/Today comparison also passes, completing M6-09.

## New bounded statements

| Path | Source-equivalent statement | Bound/index expectation |
|---|---|---|
| Typed metadata mutation | `SELECT role FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' FOR SHARE` | Membership's Org/User key, one row. |
| Typed metadata mutation | `SELECT metadata_revision FROM person WHERE organization_id=$1 AND id=$2` | Person Org/ID primary lookup, one row. |
| Typed metadata mutation | `SELECT revision FROM mobile_metadata_catalog WHERE organization_id=$1` | Catalog Org primary key, one row. |
| Current/component tag read | `SELECT t.id,t.name ... WHERE pt.organization_id=$1 AND pt.person_id=$2 ORDER BY t.id LIMIT 101` | Person-tag scoped traversal; 101 rows admit a complete ≤100-row section. |
| Current/component value read | `SELECT field_id,... FROM person_custom_field_value WHERE organization_id=$1 AND person_id=$2 ORDER BY field_id LIMIT 101` | Person/value key traversal; 101 rows admit a complete ≤100-row section. |
| Catalog snapshot | tags, fields, and options each use their documented ordering and `LIMIT 10001` | Bounded admission; 10,001st row rejects before snapshot copy. |
| Catalog pages | generation-scoped tags/fields/options keyset pages | Existing generation keys with 101-row lookahead and 100-row output. |

The composer then uses the existing typed tag/value prepare/apply statements. Its
new three reads above are intentionally dynamic SQLx queries because they are
bounded by trusted Org/Person identifiers and run in the existing transaction;
they are included here rather than omitted from plan review.

## Comparison protocol

Capture the exact SQL source hashes, plan JSON, returned-row bounds, and the
paired Person/Today comparison on the final integrated revision. A failed or stale
plan remains a failure; no laptop timing is treated as capacity evidence.

## Executed Mobile006 plan evidence

Private root: `/private/tmp/crm-mobile006-010f4-thyhauvv`.
`integration/mobile006-hotplans-3.json` records 18 exact SQL/source hashes,
`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` plans and enforced output bounds, with
zero failures. `logs/integration/mobile006-hotplans-db-3.log` passed in 109.113s
including compilation (29.23s test). New composer/catalog statements run as
`crm_app`, except the explicitly identified existing SECURITY DEFINER owner path.

The fixture has 25,000 People, 50 active members, 200 tags, 50 live plus 75 archived
fields, 150 options, 25,019 Person/tag links and 25,049 values. The first two
attempts had harness errors (column name and JSON numeric-plan parsing); their
failed logs are retained and the corrected third attempt supplies the evidence.
This is plan-shape evidence, not production capacity or a laptop latency promise.

## Final paired comparison — PASS

`logs/integration/paired-person-today-final-2.log` passed in 100.917s (100.90s test),
using the frozen executable recorded in `integration/final-perf-binary-r4.json`
at `4bf6053`. The 25,000-Person/50-member fixture includes 75,000 tag links and
100,000 custom-field values. Both arms share the current authentication stack;
this is a reader regression comparison, not a full historical-binary benchmark.

| Read | Baseline p95 | Current p95 | Allowed current p95 | Evidence |
|---|---:|---:|---:|---|
| Person detail, cd3b010 reader bodies | 19.443ms | 21.921ms | 44.443ms | `integration/paired-person-today-final-2/person-detail-paired.json` |
| Today, 9eaeb0a baseline | 72.095ms | 64.413ms | 97.095ms | `integration/paired-person-today-final-2/mobile006-today-paired.json` |

Each arm has 40 measured samples with alternating AB/BA order; Person uses five
warmups and exact complete JSON/byte equality, Today uses ten warmups and the
same fixed clock/fixture with complete response parity. Both p95 limits and all
response/entrypoint checks pass. Native API activity and other heavy checks were
paused during measurement, then restored.

The first attempt stopped before seeding or timing because two shared workspace
helper hashes predated the required admitted-activity compatibility stamp.
`4bf6053` pins those exact `3c01177` helpers with explicit scope notes. The three
frozen cd3b010 reader bodies, fixtures, sample counts, parity and latency gates
were unchanged. This failed setup attempt is retained; there is one completed
paired measurement. These laptop timings are regression evidence, not capacity.

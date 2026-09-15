# Mobile006 — hot-query inventory

This is the source inventory for M6-09. It is not performance-pass evidence.
The coordinator owns one paired Person/Today run on the final integrated source;
each statement below still needs one `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` as
`crm_app` in the owned realistic-book run.

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

## Final-run requirements

Capture the exact SQL source hashes, plan JSON, returned-row bounds, and the
paired Person/Today comparison on the final integrated revision. A failed or stale
plan remains a failure; no laptop timing is treated as capacity evidence.

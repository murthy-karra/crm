# Mobile005 / 010f3 — D-050 performance evidence

**Paired Today and Mobile005 query-plan gates PASS**,2026-09-14, coordinator.
This is the single paired run for the two accepted plans. Migration-specific
query plans and logical/physical storage inventory are recorded separately in
[SLICE_010f3_VERIFICATION.md](SLICE_010f3_VERIFICATION.md). Final combined gates
and independent implementation reviews remain required.

## Runner and source

Root `83f4fc86be7c922689c2b6ce343871e51c4b8ce6`, clean during measurement.
Private database `crm_mobile_005_perf`;25,000 People,50 members,50,123 contact rows,
125 contacts on the inspected Person. One concurrent Today reader. Android and
migration build/DB activity was paused; the runner held both private DB and native
API verification locks. Shared services and their artifacts were preserved.

```text
python3 /private/tmp/crm-mobile005-010f3/run-db-check.py --lane integration \
  --log mobile005-d050-final1.log \
  --env-file /private/tmp/crm-mobile005-010f3/integration/perf.env -- \
  python3 /private/tmp/crm-mobile005-010f3/integration/run-mobile-performance.py
```

Exit0,15.96s including migration, contact seeding, compilation, the paired read
and11 hot statements. Artifacts below are under
`/private/tmp/crm-mobile005-010f3/integration`. Runner scripts retain stdout/partial
plan evidence before asserting a verdict; no failed measurement was discarded.
No second paired run or planner-toggle comparison was performed.

## Single same-build Today comparison

The retained reference is accurately labeled `9eaeb0a`, not the later published
head. Inspection of `9eaeb0a..e36c9b3` shows only the added mobile-transaction entry
and clock branch; the ordinary fixed-clock Today path used here is unchanged.
`e36c9b3..83f4fc8` has no Today changes. The paired reference shares the unchanged
rank/source modules and runs in the same executable, database and clock as current
code. Its application-role query durations exclude connection acquisition.

Fixed clock:2026-09-14T18:00:00Z. Ten warmups then40 samples per side, alternating
AB/BA order,100 returned items. All serialized DTOs are byte-identical.

| Metric | Reference | Current |
|---|---:|---:|
| Median read |19.712ms|19.363ms|
| p95 read |22.110ms|23.384ms|

p95 increase1.274ms; D-050 allowance is `max(25ms,10%)` =25ms: **PASS**.
Absolute timings describe this development machine; they are not production
capacity claims. Evidence: `mobile005-d050-final1.log`,
`mobile005-today-pair.json`, `mobile005-today-pair-stdout.txt`.
DTO SHA-256: `80623f4a9833258c1eebabfe88c0b8061155d5b75e9f2f9b989bcd49aae74cef`.
Executable SHA-256: `f7c80ccb42a4191836e8ab6a4fae8cebe46ed42e7f636e9fb2ee1981767bbe44`.
Toolchain:rustc1.98.0 (`88d9e12ae`,2026-08-18); reader role:`crm_app`.

## One plan per changed mobile hot statement

`prepare-mobile-hot-plans.py` extracts11 exact statements and hashes their four
source files. `run-mobile-hot-plans.py` verifies those hashes, then runs each
`EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON)` once as `crm_app` inside a rolled-back
transaction. No planner settings are changed.

| Query group | Statements | Observed access |
|---|---:|---|
| Current profile initial/final Person and cursor binding |3|One-row primary-key scans|
| Current profile first/imported/native contact pages |3|Person-scoped `mobile_contact_page`;125/75/25 candidate rows,101/75/25 emitted|
| Reconciliation contact page and summary |2|Contact/Person indexes and indexed primary-method lookups; small bounded stage/member catalogs|
| Typed details contact lock and final revision |2|125 contacts under the Person index; one-row Person lookup|
| Shared command Person lock |1|One-row primary-key scan|

No full People/contact-table scan or join growth across the25,000-Person book.
Small stage/member catalog scans remain bounded by those catalogs. Measured
statement execution ranges0.034–0.149ms, reported only for context. The complete
plans, buffer counts and source hashes are retained in `mobile005-hot-plans.json`
and `mobile005-hot-plan-statements.json`.

Later merges may reuse this evidence only while these exact source statements
and the measured Today path remain unchanged. New migration statements receive
their own required plan evidence; they do not justify repeating unchanged reads.

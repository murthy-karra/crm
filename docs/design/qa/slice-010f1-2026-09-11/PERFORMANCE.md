# Slice 010f1 query-plan and storage observations

**Synthetic query-plan checks passed, 2026-09-11.** The successful collector run
emitted **90 plans across 59 distinct SQL hashes and passed all 246 checks**.
The 59 SQL sources comprise 58 Rust statements and the database operation-permit
expression; 62 static call sites expand into the 90 measured variants.
See the [summary JSON](checks/query-plan-summary.json),
[complete plans and measurements](checks/query-plans.json), and
[collector implementation](../../../../backend/crates/crm-api/tests/db_metadata_import_plans.rs).

The backend checkpoint records 88.85 seconds for the test, including 87.851
seconds of fixture preparation, after 30.06 seconds of compilation. These are
run timings, not request latency targets or an absolute laptop performance gate.
Final repository repeats and the browser walkthrough also passed; their separate
evidence is linked from the [QA index](README.md).

## What was measured

The opt-in collector runs in a disposable isolated SQLx database. It starts with
a real completed parent and a child that reaches a ready plan, then inserts inert
bulk fixtures through the migrator. Copied ciphertext is not rebound to new
authenticated-encryption identities. No worker, decryptor or source reader runs
against the bulk fixture. Those rows support SQL/cardinality/storage-shape
measurements, not executable fidelity, authorization or retention-accounting
proof.

Each plan records the extracted production SQL, SHA-256, bindings and
`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` output. Checks cover claim/lease and
ledger locks, source qualification, catalog dependencies, aliases, planned and
committed pages, field/provenance reads, native per-Person probes and the database
permit expression. The assertions check returned/examined rows, required index
use, and absence of disk sorts, temporary-block spills and batched hashes.
Small native catalogs may legitimately use bounded sequential scans.

The fixture uses random UUIDv4 identifiers for paged metadata and history,
independent of semantic source order. Deep pages use actual scoped rank 24,000
(rank 50 for mappings). Twenty-five held People are independently distributed
through that UUID order. Sparse and empty filters use their actual remaining
keyset-tail bounds; this does not promise constant page-sized work for every
filter selectivity. For example, the successful `results_people_deep` plan
examined 52 rows against a remaining-tail bound of 1,035.

| Fixture scope | Observed cardinality |
|---|---:|
| Native People / active Organization members | 25,000 / 50 |
| Native tags / fields / options | 10 / 10 / 50 |
| Native Person tag links / custom-field values | 250,000 / 100,000 |
| Current-plan sources / mappings | 25,012 / 72 |
| Current-plan aliases / manifests | 250,001 / 25,001 |
| Current-plan operation descriptors / results | 350,002 / 25,035 |
| Child imports including terminal history | 257 |

These are the observed counts, including the small real fixture where applicable.
Whole-relation counts below additionally include superseded plans and synthetic
history, so they intentionally differ from current-plan counts.

## Native text widths and physical sizes

Of the 100,000 native text values, 99,999 have distinct deterministic ASCII values
of 150–500 characters, averaging 325.02 characters. One 15-character exact-match
probe remains. Digest-derived segments avoid an unusually compressible repeated
character fixture. These are moderate/dense synthetic widths, not maximum-width
or worst-case source data.

| Measurement across all 100,000 values | Minimum | Mean | Maximum | Sum |
|---|---:|---:|---:|---:|
| Text characters / UTF-8 octets | 15 | 325.01 | 500 | 32,501,265 |
| `pg_column_size(text_value)` bytes | 16 | 329.01 | 504 | 32,901,262 |
| `pg_column_size` complete value-row bytes | 144 | 462.51 | 640 | 46,250,824 |

The following are **whole-relation physical measurements in the isolated
database**, not a per-Organization ledger, incremental slice cost or complete
database footprint. Bytes are exact. “Table” is `pg_table_size`, including its
associated storage; “Indexes” is `pg_indexes_size`; “Total” is
`pg_total_relation_size`. Row-width sums are a separate measurement and must not
be added to these physical totals.

| Relation | Rows | Table bytes | Index bytes | Total bytes |
|---|---:|---:|---:|---:|
| `person` | 25,000 | 3,694,592 | 4,022,272 | 7,716,864 |
| `organization_membership` | 50 | 16,384 | 16,384 | 32,768 |
| `tag` | 10 | 16,384 | 49,152 | 65,536 |
| `person_tag` | 250,000 | 25,321,472 | 46,612,480 | 71,933,952 |
| `custom_field` | 10 | 16,384 | 81,920 | 98,304 |
| `custom_field_option` | 50 | 16,384 | 49,152 | 65,536 |
| `person_custom_field_value` | 100,000 | 47,603,712 | 7,569,408 | 55,173,120 |
| `migration_import` | 257 | 163,840 | 204,800 | 368,640 |
| `migration_import_plan` | 258 | 180,224 | 98,304 | 278,528 |
| `migration_import_source` | 25,006 | 17,113,088 | 4,685,824 | 21,798,912 |
| `migration_import_manifest` | 25,002 | 7,626,752 | 6,602,752 | 14,229,504 |
| `migration_import_result` | 25,001 | 14,671,872 | 8,019,968 | 22,691,840 |
| `migration_import_identity` | 25,001 | 3,768,320 | 2,998,272 | 6,766,592 |
| `migration_snapshot_capture` | 259 | 163,840 | 188,416 | 352,256 |
| `migration_snapshot_record` | 25,014 | 12,099,584 | 6,021,120 | 18,120,704 |
| `migration_metadata_import` | 257 | 180,224 | 278,528 | 458,752 |
| `migration_metadata_plan` | 258 | 229,376 | 98,304 | 327,680 |
| `migration_metadata_source` | 25,270 | 17,293,312 | 8,200,192 | 25,493,504 |
| `migration_metadata_mapping` | 18,506 | 12,771,328 | 16,203,776 | 28,975,104 |
| `migration_metadata_choice` | 18,504 | 3,825,664 | 3,325,952 | 7,151,616 |
| `migration_metadata_alias` | 250,002 | 41,844,736 | 95,985,664 | 137,830,400 |
| `migration_metadata_manifest` | 25,258 | 29,597,696 | 12,443,648 | 42,041,344 |
| `migration_metadata_operation` | 350,004 | 68,321,280 | 104,914,944 | 173,236,224 |
| `migration_metadata_identity` | 70 | 49,152 | 16,384 | 65,536 |
| `migration_metadata_result` | 25,291 | 29,622,272 | 11,845,632 | 41,467,904 |
| `migration_metadata_issue` | 62 | 16,384 | 16,384 | 32,768 |
| `migration_metadata_receipt` | 2 | 32,768 | 16,384 | 49,152 |
| **All 27 measured relations** | — | **336,257,024** | **340,566,016** | **676,823,040** |

The totals are approximately 320.68 MiB of table storage and 324.79 MiB of indexes,
645.47 MiB combined. They include the fixture's retained history and existing
parent/native relations. They provide no Organization quota, storage-capacity,
bytes-per-Person or maximum-dataset extrapolation. WAL, replicas, backups and
future operational churn/bloat costs are excluded from this measurement scope.

## Authentic child ledger, kept separate

Before inert seeding, the real child was proposed with a ready plan for one
eligible Person, one tag, one field, one tag link and one value, with no held
items. Its logical ledger was **13,327 retained bytes and 65,536 reserved bytes**.
These are application accounting counters for that tiny executable fixture.
They are neither measured physical allocation nor a retention-accounting result
for the 25,000-Person inert book. Do not scale them to infer source storage,
Organization allowances or quotas. Executable accounting/fidelity proof remains
in the focused source, gate and acceptance suites.

## Initial failure and correction

The first run is preserved as failed evidence. PostgreSQL 18 emitted integral
`Actual Rows` values as JSON floating-point numbers, which the collector's integer
accessor rejected. The test helper now accepts numeric values only after checking
that they are finite, nonnegative and integral, then compares the exact expected
count.

Four scan-bound checks also failed on the original fixture. Sequential fixed UUID
prefixes artificially correlated scoped rows with global primary-key order and
put unrelated history in separate ranges. The fixture now uses the production
UUIDv4 distribution for paged metadata/history, with cursor anchors derived from
actual scoped ranks. Semantic source positions remain unchanged.

| Plan label | First run: rows examined | Unchanged bound | Successful run: rows examined |
|---|---:|---:|---:|
| `alias_first` | 503 | 51 | 51 |
| `alias_deep` | 501 | 51 | 51 |
| `records_first` | 120 | 51 | 51 |
| `results_first` | 139 | 51 | 51 |

All four now use the existing scoped indexes. The measured production SQL hashes
are identical between runs. The corrections were confined to the collector and
fixture: no production query/index/contract change, bound relaxation or planner
forcing was used. Shared CRM reader SQL/behavior is unchanged, so the slice does
not require a paired full-CRM reader benchmark under
[the approved D-050 verification scope](../../../specs/SLICE_010f1.md#9-acceptance-and-verification).
This collector does not benchmark full CRM readers, decryption, response sizing
or concurrent request throughput.

## Evidence provenance

The published JSON is extracted from successful raw `metadata-plans-2.log` output,
with every SQL hash checked, and agrees with the separately parsed private
`metadata-plans-2-observations.json` and `backend-verification-checkpoint.json`.
The checkpoint records unchanged reviewed
production code, identical measured SQL hashes, 246 passing checks and the
preserved first failure. Original private logs remain outside the repository; checked copies and their
hashes are listed in [the log inventory](checks/log-inventory.json).

| Artifact | SHA-256 |
|---|---|
| [Summary JSON](checks/query-plan-summary.json) | `81603dcf264573ccec74c9b2d5b5cf284f27fe341160d53b012c7d58809683ae` |
| [Full plans JSON](checks/query-plans.json) | `7c077fe67d71e296b5c312fb9184388003c80fc25a54b3bba6222e051cc75ac4` |
| Private successful observations | `1d72e3353b6a0451e9a6fdcdc040e7bbf40b79c15f1ddf41f042de1aa3e10ed5` |
| Private backend checkpoint | `09d0541e0e658d552c2d8721dab868c9d11068e243743a5d4a3eab4adf46704b` |

No live FUB/customer validation, activation or deployment is established by these
synthetic measurements.

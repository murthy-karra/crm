# Slice 010f3 — verification record

Implementation remains in progress on `codex/migration-010f3`. The bounded admitted
execution milestone is implemented and tested; staged preparation now has focused
DB and plan-shape evidence. Complete transport/lifecycle/remainder, Web/browser acceptance, independent implementation
review and final integrated gates remain required. No review round has been used.

## Implemented execution checkpoint

`c5ae1d2` replaces the scaffold with typed catalog/Person units and exact accounting;
`e822241` integrates coordinator readiness `75b40fd`; `4dd5e69` adds the native
baseline/permit/rollback proofs; `ff19365` preserves captured choice order and
corrects the bitmap-index assertion. Migration `20261002000002` supplies unique
unit receipts/checkpoints and tightens the table/claim/Person/lease allowlist.

The admitted worker uses 010f1 Text/Number/Date/Choice interpretation, creates a
new choice field with its bounded options atomically, uses explicit option
mappings, and never overwrites/adopts changed/equal local cells or resurrects
removed tags. Native rows, shared claims, immutable encrypted unit results,
checkpoint and exact owner reservation settlement commit together. Catalog and
Person work advance through persisted keysets. Compatible original catalog
writers create/consult the shared claim in their native transaction. Preparation
now binds the selected report's actual snapshot/sequence and uses the same
`tag-group`/`tag-raw`/field/option HMAC purposes as 010f1.

## Executed evidence

All DB commands use the serialized private runner:
`python3 /private/tmp/crm-mobile005-010f3/run-db-check.py --lane migration --log NAME -- cargo test -p crm-api --test all FILTER -- --ignored --test-threads=1`.
The runner supplies isolated credentials and target
`/private/tmp/crm-mobile005-010f3/migration/target`; SQLx creates disposable synthetic
test databases. Logs below live in that same lane evidence directory. No shared
service build artifacts, source account, deployment or native QA stores changed.

| Check | Result | Tested source and evidence |
|---|---|---|
| `cargo check -p crm-app` | passed | Execution implementation compile: `worker-check.log`, `worker-check2.log`, `worker-check3.log`. |
| `cargo clippy -p crm-app --all-targets -- -D warnings` | passed | `admitted-worker-clippy2.log`; first `admitted-worker-clippy.log` reported explicit-scope helper argument-count lints, resolved with the established narrowly documented allow attributes. |
| Preparation, typed values, atomic catalog failure | 2 passed / 1 assertion failed, corrected | `admitted-worker-db1.log`: native decimal was correctly `123456.1250`; test incorrectly expected six displayed fractional digits. No native truncation/rounding failure. |
| Five admitted DB tests | passed | `admitted-worker-db2.log`: all four types plus tags, source-free execution, exact byte deltas/replay, unchanged Person/source identity rowsets, frozen equal/different/appeared/removed cells, catalog rollback/retry and missing-index readiness rejection. |
| Six admitted DB tests | passed | `admitted-worker-db3.log`: additionally proves wrong table/claim/unit/Org/lease and cancel fencing, and injected result failure after both catalog **and Person** native writes with zero partial native/claim/result/checkpoint/ledger changes. |
| Seven admitted tests including D-050 cardinality fixture | passed at `ff19365` | `admitted-worker-db5-perf.log`; command adds `--features perf-harness` and `--nocapture`. Prior `admitted-worker-db4-perf.log` had 6 passes and one test-helper failure because its index detector did not recognize a Bitmap Index Scan beneath a Bitmap Heap Scan. The observed production plan was indexed; helper corrected before rerun. |
| Exact original/admitted shared-claim reuse after readiness | passed at `1d2d2b7` | `admitted-original-claims-db3.log`; first two attempts paused before the additive identity-key guard correction. |
| `cargo fmt --all`, `git diff --check` | passed at checkpoints | Owned backend/test/document changes; final-tree formatting is still a coordinator gate. |
| Original metadata private-gate suite after shared claims | passed at `1d2d2b7` | All six in `original-metadata-composition-db.log`. |

The old stale migration-grant blocker is resolved: the current isolated tests
apply `20261002000002` and can read immutable original metadata identity evidence.
No UPDATE privilege or row UPDATE lock was added to that immutable table.

## D-050 execution-query evidence

`admitted-worker-db5-perf.log` records six exact production SQL statements with
`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` on a 25,000-Person/50-member synthetic
book with four typed values and one tag per admitted Person. The enlarged rows
are explicitly inert cardinality fixtures, not billable/processable encrypted
execution input; real preparation/execution is separately proved on real retained
synthetic captures.

The late Person query seeks directly beyond the persisted checkpoint through
`migration_admitted_metadata_manifest_work` (one row, five shared hit blocks,
zero filtered rows). Person operations use their unit index; receipt probes use
the unique root/Org/unit index; native value/tag baselines use their composite
primary keys. The seven-row catalog naturally receives a small sequential scan,
with seven indexed receipt probes instead of scanning all Person results.
Measured times are reported in the log, never used as absolute laptop gates.

The copied manifest/operation/result retained-column sum is **49,773,796 bytes**;
those three physical relations total **124,837,888 bytes** in this fixture. These
are different measures and exclude other evidence/native tables; neither number
is a customer quota or production capacity prediction. Preparation query evidence is recorded below. Coordinator owns the one paired
Person/Today regression after integration; this lane did not duplicate it.

## Remaining implementation and evidence

The staged checkpoint adds schema `20261002000004`: source observations are
checked in capture/ordinal order, at most 100 indexed records per step; fields
and options use one descriptor per step; tag spelling follows captured source
order; cohort manifests use admission-result keysets and retain missing targets;
Person field operations advance through bounded mapping keysets. Each step
reserves at most 64 MiB, accumulates exact charged-column deltas without rescanning
the settled prefix, then commits its lease/checkpoint and all three ledgers
atomically. Handover readiness commits before later preview work. Typed list,
detail and plan readers require current tenant admin authority.

`admitted-preparation-db1.log` passed the eight existing admitted tests after the
initial staging refactor. `admitted-preparation-db2.log` passed one and failed two
new fixture assertions: an empty custom-field page advanced before the injected
source fault, and upstream provenance FKs prevented synthetic Person deletion.
The corrected tests inject the first nonempty step and deliberately remove only
test-database native-reference constraints while preserving immutable evidence.
`admitted-preparation-db3.log` passed all four focused preparation tests, including
the seventeenth conflicting observation and exact failed-step rollback/retry.

Its six D-050 preparation plans use a 25,000-Person/50-member inert cardinality
fixture. Capture, record, source-field and capture-ordered tag lookups use their
indexes; the four-field mapping query scans its seven-row catalog. Inspection
found that the cohort's correlated EXISTS hashed all 25,000 identities despite
its indexed outer keyset. The changed lateral LIMIT 1 query in
`admitted-preparation-db4.log` instead uses `migration_admitted_metadata_admission_work`,
`person_pkey` and `migration_import_identity_target`: one result row, 9 shared
hits and 2 reads (observed 0.05 ms, not an absolute gate). The test originally
required the identity PK specifically and rejected this equivalent bounded
index. Its actual plan is retained in
`backend/crates/crm-api/tests/fixtures/admitted_metadata_cohort_plan.json`; the
corrected assertion is verified against that recorded plan, without another
EXPLAIN. The other five unchanged preparation plans reuse db3 evidence. The old
execution performance case is skipped in subsequent functional checks.

`admitted-preparation-check2.log`, `admitted-preparation-clippy.log` and
`admitted-preparation-clippy2.log` passed. `admitted-preparation-check.log` records
an initial command launched outside the backend Cargo workspace. `admitted-preparation-db5.log` passes ten functional/recorded-plan tests; its
new cross-tenant fixture omitted required Organization intake fields. The
intermediate `admitted-preparation-db6-admin.log` records the second missing
field; the complete corrected fixture passes in
`admitted-preparation-db7-admin.log`. Together these establish all eleven
functional/recorded-plan checks on the staged implementation. No application
change was needed for the fixture corrections. The db5 command used
`--include-ignored` and skipped both EXPLAIN fixtures; db7 selected only the
current-tenant-admin reader test.

Still implement the frozen concrete transport contract in
`SLICE_010f3_CONTRACT.md`: complete source/mapping/record/result/issue/segment and
provenance reads, immutable replacement plans (including selected-boundary
replacement), counted subset confirmation, durable lifecycle response receipts,
retry/cancel and exact remainder. The current simpler DTOs and one-time mapping
transition are not the final Web contract. Full bounded reader/request payload
limits, retained fan-out for the new observation inventory, and final
cross-checks remain part of that work.

Required remaining tests include failed handover rollback and rerun accounting,
old already-running writer rejection, multi-cohort collisions/tombstones/archival,
source variants/incomplete evidence, budget/key/expiry failures, full-tag-set
limits, HTTP no-store and source-free reader negatives, desktop/390px browser
workflow and preservation reconciliation. Final `scripts/check`,
`scripts/sqlx-prepare`, `scripts/check-db`, paired Person/Today performance and
cross-slice upgrade checks remain coordinator-owned and sequential.

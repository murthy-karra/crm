# Slice 010f3 — verification record

Implementation remains in progress on `codex/migration-010f3`. The bounded admitted
execution milestone is implemented and tested; staged preparation, complete
transport/lifecycle/remainder, Web/browser acceptance, independent implementation
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
| Exact original/admitted shared-claim reuse after readiness | running/pending | `admitted-original-claims-db.log`; not claimed passed until completed. |
| `cargo fmt --all`, `git diff --check` | passed at checkpoints | Owned backend/test/document changes; final-tree formatting is still a coordinator gate. |
| Previous original metadata private-gate suite | passed before this worker milestone | All six at `5e06b2c`; logs `db-metadata-gate.log` (initial failure) and `db-metadata-gate-rerun.log` (pass). A current-source rerun is pending. |

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
is a customer quota or production capacity prediction. New preparation/query
statements still need their own shape evidence. Coordinator owns the one paired
Person/Today regression after integration; this lane did not duplicate it.

## Remaining implementation and evidence

Preparation currently executes the full successful cohort and definition set in
one transaction; removing the former 100-row fail-closed cap did not implement
staging. Replace that with bounded fenced keysets/checkpoints, all-observation
qualification, retained tombstone/exclusion manifests, exact incremental charges
and complete typed admin read permits. Freeze and implement the concrete staged
DTO/lifecycle contract in `SLICE_010f3_CONTRACT.md`, then implement complete
source/mapping/record/result/issue/segment/provenance reads, immutable replacement
plans, explicit subset confirmation, retry/cancel and exact remainder.

Required remaining tests include failed handover rollback and rerun accounting,
old already-running writer rejection, multi-cohort collisions/tombstones/archival,
source variants/incomplete evidence, budget/key/expiry failures, full-tag-set
limits, HTTP no-store and source-free reader negatives, desktop/390px browser
workflow and preservation reconciliation. Final `scripts/check`,
`scripts/sqlx-prepare`, `scripts/check-db`, paired Person/Today performance and
cross-slice upgrade checks remain coordinator-owned and sequential.

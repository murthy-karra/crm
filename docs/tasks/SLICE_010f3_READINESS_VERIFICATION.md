# Slice010f3 staged preparation readiness verification

Coordinator-owned shared readiness proof,2026-09-14. Tested source: root `be15cb0`
plus the readiness inventory and regression committed with this record.
Migration implementation evidence remains in `SLICE_010f3_VERIFICATION.md`.
All evidence below is relative to `/private/tmp/crm-mobile005-010f3/integration`.

Runtime readiness and release preflight require all13 admitted metadata tables,
staged plan/source/manifest columns, nullable unresolved Person targets, byte-accounting
function and five enabled normal/always triggers, and five valid ready work indexes.
The shared identity-claim guard must also run in normal sessions; replica-only and
disabled triggers fail readiness. Existing atomic result-unit requirements remain.

Commands and results:

- `python3 -m unittest discover -s scripts/tests -p test_migration_release_preflight.py`:
  **51 passed**, `preflight-preparation-schema2.log`.
- `run-db-check.py --lane integration --log admitted-preparation-readiness-db2.log --
  cargo test -p crm-api --test all --locked
  db_admitted_metadata_readiness::admitted_metadata_readiness_requires_complete_preparation_schema
  -- --exact --ignored`: **1 passed**,54.73s total,1.32s test. The DB regression
  checks12 incomplete variants in rolled-back transactions and complete readiness
  before/after, preserving the private source DB and shared services.

The first DB attempt (`admitted-preparation-readiness-db.log`) failed because its
assertion expected the later capability error instead of the earlier common
schema-incompatibility error. The gate rejected the missing table correctly.
The corrected assertion passes; that failure log remains retained.

These focused checks do not replace the final combined gates or independent review.

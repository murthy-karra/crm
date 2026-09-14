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

## Immutable plan/source bindings

After integrating migration `380bcfb` at root `437e3d3`, the inventory additionally
requires each plan's non-null snapshot/report/output revision/capture boundary,
nullable previous-plan reference, preparation kind, receipt-owned snapshot, the
unique selected-target index, choice keyset index and validated choices-phase
constraint. Runtime and release preflight use the same checks.

`preflight-plan-bindings.log`:51 passed. The same isolated DB command above with
`--log admitted-plan-readiness-db1.log` passes1 test containing22 rollback-only
incomplete variants,171.87s total /1.55s test. Complete readiness succeeds before
and after. Tested tree is `437e3d3` plus the readiness change committed with this
section. The initial12-variant proof is superseded by this expanded proof.

## Complete review transport

After transport `6a2f866` integrated at root `3ada9bf`, readiness also requires
the alias table (14 tables total), admission-plan ownership, correctly nullable
header/counter columns, both normally enabled mapping-count triggers, all12 reader
indexes and the exact live-root uniqueness predicate. Cancelled unconfirmed roots
must be excluded from that predicate; confirmed roots remain unique.

The preflight unittest command passes51 tests. The private command
`run-db-check.py --lane integration --log admitted-transport-handover-readiness1.log --
cargo test -p crm-api --test all --features perf-harness --locked
db_admitted_metadata_ -- --ignored --skip db_admitted_metadata_http_boundary::
--skip db_admitted_metadata_ui_fixture::` compiled and ran3 tests in188.57s.
The expanded readiness test passes all36 rollback-only variants; the populated
handover test also passes. The legacy-session test **fails** before handover:
`crm_metadata_insert_allowed` tries to cast an empty permit string to JSON despite
the registry being unready. This is a production defect awaiting the migration
writer's additive correction, not a passing combined check.

The populated handover proof uses an actual completed original metadata import,
injects a failure after the first claim is charged, and verifies rollback of all
claims, readiness and owner ledgers. Its successful retry charges both claims
exactly once, preserves original plans/results/identity evidence, and replays the
same preparation receipt without another source call or charge. The remaining
legacy-session proof and full outer HTTP boundary test remain completion gates.

## Exact continuation and outer HTTP boundaries

Migration checkpoint `fd016a1` adds the guarded proof parser; the unchanged
legacy-session test passes in the migration lane's
`admitted-remainder-db2-legacy.log` (1 test,5.50s). This supersedes the earlier
parser failure without changing the regression or resetting the old session.

Readiness now also requires exact-continuation ownership/reference columns,
execute-unit and held-settled counters, five successor/work indexes and all three
remainder preparation phases. The preflight suite passes51 tests in
`preflight-remainder-readiness.log`. On root `a7a01c5` plus this readiness change,
the focused private command from the previous section, with log
`admitted-final-readiness-http1.log` and `--skip db_admitted_metadata_handover::`
instead of the HTTP skip, runs two tests in119.79s. The readiness test passes
all50 rollback-only variants. The outer HTTP test initially fails because the
shared admin guard registered `{id}` instead of the route's `{person}` template,
causing a member provenance read to return409 rather than403 in review mode.

Correcting those two server-owned template strings preserves the normal admin
denial. The unchanged HTTP test then passes in `admitted-http-boundary2.log`,
44.71s total /3.96s test, using `cargo test -p crm-api --test all --features
perf-harness --locked
db_admitted_metadata_http_boundary::admitted_metadata_full_router_denials_are_source_free_and_not_cacheable
-- --exact --ignored` through the same private helper. It checks204 responses:
all admitted endpoints and provenance, operational/review modes, admin/member/
anonymous identities, malformed/oversized bodies and unknown authority fields.
Every response is no-store; denials make no source calls, imports, plans, receipts
or registry handover. Both initial failure logs remain retained.

Final combined gates and independent review remain pending.

## Final evidence-limit schema

After tested fidelity checkpoint `1e9ed69`, runtime/preflight also require the
non-null oversized-Person marker, nullable32-byte field-name key, its validated
length constraint and partial lookup index, the extra-values preparation phase,
and both insert/update sides of field-name byte accounting. The private focused
readiness command above passes all61 rollback-only incomplete variants, with full
readiness before/after: `admitted-fidelity-readiness-db1.log`,1 test,175.57s total /
1.60s test. `preflight-fidelity-readiness.log` records51 passing preflight tests.
Source is the integrated `1e9ed69` implementation plus the shared readiness change
committed with this record. No new schema or wire contract was introduced here.

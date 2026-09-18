# Migration coverage, repeated refresh, and reconciliation verification

Status: integration verification in progress. This record does not yet declare
the branch ready to merge.

## Scope

Branch `codex/migration-completion`, based on local main `d3b9f39`. The bounded
[integration brief](../tasks/MIGRATION_COMPLETION_INTEGRATION.md) owns the scope.
Three Terra implementation lanes produced the coverage inventory, repeated
refresh regression, and initial reconciliation draft. A Sol lane completed the
backend reader; a separate Sol reviewer performs independent review.

No database migration, source fetch, activation policy, deletion policy or new
worker is introduced. The report combines historical initial import evidence
and latest generic family-refresh plans. Later People core refresh/mapping
repair remains in existing detailed panels. Unsupported destination domains
and live FUB qualification remain open, explicitly represented as gaps.

## Functional evidence already obtained

- Four coverage taxonomy unit tests passed in the coverage lane.
- Three focused reconciliation PostgreSQL tests passed in the backend lane:
  administrator/workspace/tenant boundaries, populated family ledgers, and
  recovery lineage. Log: `/private/tmp/crm-reconciliation-db-final.log`.
- The repeated three-capture Activity regression passed after correcting the
  existing-head update, before additional terminal/accounting/report assertions.
  Log: `/tmp/crm-migration-repeat-db.log` (1 passed, 18.89 seconds).
- All 46 affected family-refresh PostgreSQL tests passed with the same runtime
  fix, excluding the opt-in long-running browser fixture.
  Log: `/tmp/crm-migration-family-regression-db.log` (427.75 seconds).
- Six focused reconciliation Vue tests passed after the evidence-display
  repairs. The independent reviewer also ran the full Web suite after those
  repairs: 104 files / 1,334 tests passed. Browser acceptance remains pending.
- The strengthened repeated-cycle regression passed: 1 test, 25.68 seconds,
  `/tmp/crm-migration-repeat-final.log`. It additionally checks terminal manifests,
  plan counts/positions/bytes, zero reservations, and preserving the earlier
  note result after a task-only remainder.

The family regression used the real typed commands, worker execution, guards
and database constraints. The existing-head fix is committed as `205d831`:
conditional UPDATE for an expected existing head, INSERT for a first head,
exactly one affected row required, unchanged transaction and storage payer.

## Failures found and repaired

- The new repeated-refresh fixture initially omitted explicit mappings and
  moved a JSON value before cloning it; both test-fixture errors were fixed.
- A subsequent refresh exposed `stale initial family head`: PostgreSQL's
  BEFORE INSERT guard ran before an UPSERT could select UPDATE. The conditional
  update above repaired the application path without weakening DB guards.
- Reader validation caught an unnecessary workspace row lock requiring a
  privilege the application role intentionally lacks; existing authorization
  locks and consistent snapshot remain in place.
- An outer workspace denial bypassed route-local cache headers. Scoped outer
  middleware now applies `no-store` to the report's success and error responses.
- Production-image compilation caught a middleware import gated to test
  support. Fully qualifying the Axum middleware fixes the production shape.
- The full gate found `manual_contains` in the new coverage test; corrected.
- Browser run `8f41a8a56cdf` stopped during the failed production build. Run
  `a442297e929d` stopped because build inputs changed while the image was
  building. Neither run is browser acceptance evidence; final execution must
  use a stable tree.

## Independent review

First substantive review round completed with CHANGES REQUIRED. The bounded
repair set is:

- P1: deduplicate stable history facts across terminal roots.
- P1: deduplicate admission People across runs; settled identity is authoritative,
  live previously-imported identities are Already current, and original exclusions
  remain Excluded.
- P2: use a stable earliest-run cohort anchor and coherent evidence tuple.
- P2: bound warnings by family/code and cover all represented source snapshots.
- P2: label initial outcomes separately and disclose omitted People refresh/repair
  summaries. Repaired in the UI.
- P2: display cohort/latest and warning evidence references. Repaired and tested.
- P2: strengthen repeated-cycle terminal/accounting checks. Repaired and passed.

The reviewer found no blocker in the narrow refresh-head mutation fix, static
coverage inventory, authorization/workspace/tenant/snapshot/cache/response-size
fences, or stale-response lifecycle. Round two will verify the backend repairs
and final acceptance evidence; no third review round is planned.

## Remaining gates

- Integrate and verify review repairs.
- Run the strengthened repeated-cycle PostgreSQL regression and final reader
  tests serially.
- Inspect exact query plans at 25,000 People / 50 memberships.
- Complete the full repository gate and real migration Playwright journey;
  inspect the 390px screenshots.
- Record the independent review disposition, final tested tree, and residuals.

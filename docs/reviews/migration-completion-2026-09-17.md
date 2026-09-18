# Migration coverage, repeated refresh, and reconciliation verification

Status: scoped implementation verified on `codex/migration-completion`.
Merged and published on `main` at `8fac1dc`; shared-development deployment is
tracked in [the release record](../tasks/READ_OPTIMIZATION_RELEASE_2026-09-17.md).

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
- The same regression passed again after the lineage counting repairs: 1 test,
  20.50 seconds, `/tmp/crm-migration-repeat-final-reviewed.log`.
- All five reconciliation database cases passed after the deadlock repair,
  including a held-Organization-row-lock regression: 55.92 seconds,
  `/tmp/crm-reconciliation-final-lock-regression.log`.
- The final migration browser run `c2f53c28447d` passed all sixteen steps with
  the deadlock repair and `verified_empty` cleanup. The unchanged 390px panel
  and evidence-reference layout were visually inspected in the earlier passing
  run `5babfff39af2`; the final run also produced both screenshots. Evidence:
  `.e2e/runs/c2f53c28447d/migration-1-a1/`.

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
- Browser run `c4f92c297be2` exposed a real lock-order deadlock between the
  report's import-then-Organization row locks and the existing import reader's
  Organization-then-import locks. Commit `b5a6471` removes unnecessary report
  row locks. REPEATABLE READ, the shared workspace advisory lock and membership
  authority lock remain. A deterministic concurrent-row-lock regression and
  the final browser rerun passed.

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
fences, or stale-response lifecycle. Two substantive review rounds were used.

Round two approved the lineage repairs after five focused DB cases passed.
The later deadlock was found by acceptance execution; the reviewer inspected
that narrow fix and confirmed that workspace/authority fences and snapshot
consistency remain intact. This was an acceptance-fix inspection, not another
substantive review round. The reviewer independently matched all seven SQL hashes
to the final root and approved the plan gate. Final integration checks passed.

## Plan evidence

One successful final `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` run covers all seven
exact exported report statements, including the final lock-free root lookup.
The fixture contains 25,000 People, 50 members, 25,000 selected rows per initial
ledger, 25,000 distinct refresh cohorts, 75,000 refresh manifests/results across
three families, and 25,000 warning captures. Superseded initial plan rows are
reported separately, not silently excluded from total retained volume.

The plan fixture uses existing development administrator credentials only to
seed inert cardinality in a disposable database transaction. It restores normal
trigger behavior before EXPLAIN and rolls back all scale rows. No application
or migrator privilege was changed. This fixture proves plan shape, not mutation,
encryption or source fidelity. Normal functional tests retain their real guards.

| Statement | Execution ms | Output rows |
| --- | ---: | ---: |
| Root | 0.066 | 1 |
| Cohorts | 76.392 | 1 |
| Metadata totals | 46.972 | 1 |
| Activity totals | 99.947 | 2 |
| History totals | 154.114 | 3 |
| Latest refresh | 1,301.142 | 6 |
| Source warnings | 34.571 | 6 |

Inspection found bounded ledger scans/hash joins and indexed cohort probes,
not a full-ledger scan repeated for each Person. Whole-tenant aggregate scans
are expected when the fixture's rows all belong to the requested tenant.
The latest-refresh query performs a fixed six-family expansion and uses
`family_refresh_manifest_cohort_page`. History sorting spilled 636/637 temporary
read/write blocks; latest refresh spilled 1,509/1,512. These costs and timings
are reported trends, not absolute latency or production-capacity gates.

Raw plans, exact SQL hashes and bindings are retained under
`/private/tmp/crm-reconciliation-plan-evidence/`; the successful exit-zero log is
`/private/tmp/crm-reconciliation-plans-final.log` (66.18 seconds including setup).
Earlier fixture-only failures (privileged setup, confirmed-vs-retained counting,
and decimal EXPLAIN row parsing) are retained in separately named failure logs.
They are not passing plan evidence.

This is a new administrator report, with no prior endpoint to pair against.
Person and Today read paths are unchanged; no new capacity claim or benchmark
matrix is introduced.

## Final integration gate and residuals

The final `./scripts/check` after production fix `b5a6471`, fixture integration
`2d6b5a7`, and opt-in feature gating passed with exit zero in 144 seconds:
1,047 Rust tests, five doctests, 1,334 Web tests, format/lint, production
compilation, Web build, runner tests and email-worker tests. Log:
`/tmp/crm-migration-final-check.log`. Build outputs were isolated under
`/tmp/crm-merge-ready-target` and `/tmp/crm-migration-integration-web`.

The scale fixture and its helpers are behind the existing `perf-harness`
feature, keeping privileged scale setup out of the ordinary ignored DB suite.
Its final feature-specific lint/compilation also passed:
`cargo clippy -p crm-api --test all --features perf-harness --locked -- -D warnings`,
log `/tmp/crm-reconciliation-perf-feature-check.log`. The feature guard and
equivalent range-contains lint correction do not change the measured statements
or fixture data; the completed plan evidence remains applicable.

Focused DB and browser acceptance above are the database evidence for this
change; a full `scripts/check-db` run is not claimed.

Live FUB qualification, missing destination domains, privacy/erasure policy and
workspace activation remain deferred. Coverage flags expose these gaps; neither
synthetic acceptance nor this report declares complete migration fidelity or
cutover readiness. Shared development remains unchanged.

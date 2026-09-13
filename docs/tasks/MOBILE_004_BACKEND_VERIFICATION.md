# Mobile 004 — Backend verification

Verified, 2026-09-13. Focused evidence below covers backend `4edf763`.
Native acceptance, migration interaction, independent reviews and final combined
gates are complete in the [integration record](MOBILE_004_010e4_IMPLEMENTATION_STATUS.md).
No release or deployment claim. Full logs are retained under
`/private/tmp/crm-mobile004-010e4/integration/`.

## Correctness evidence

- `cargo test -p crm-api --test all --features test-support --no-run` passed with
  isolated `CARGO_TARGET_DIR=/private/tmp/crm-mobile004-010e4/mobile/target` and
  debug information/incremental compilation disabled. The resulting test binary
  was executed directly with the migrator connection and serial test execution;
  each SQLx test owns a disposable database.
- `all-cafd446f04f37a4c db_mobile:: --ignored --test-threads=1`: **18 passed** in
  107.30 seconds (`mobile-db-tests.log`). Includes existing Mobile001–003 cases,
  25k selection/cleanup, authority, tenant isolation and derived-trigger guards.
- Mobile004 cases cover stage-specific ABA/no-op behavior, durable replay after
  later changes, catalog paging beyond 100 entries and review hold. Added fault
  injection proves receipt failure rolls back stage, both revisions, fact,
  receipt and publication. Concurrent duplicate submission produces one fact and
  publication; no-op produces neither. Foreign/deleted targets remain opaque.
- Catalog transaction rollback preserves the revision and seal; committed
  insert/delete invalidates an opted-in generation while legacy generation
  semantics remain intact. Another actor's context and review-held workspace
  cannot use the new ordinary current-stage/catalog reads.
- Three targeted existing stage/tenant/realtime tests passed in 10.45 seconds
  (`stage-regression-tests.log`):
  `assign_and_stage_commands_are_fact_precise_and_history_orders_stably`,
  `cross_organization_people_stages_and_unresolved_are_isolated`, and
  `assign_stage_and_contact_commands_publish_only_when_changed`.
- Release-preflight Python tests: **48 passed** (`preflight-feature.log`).
  `bash -n scripts/check` and `git diff --check` passed.

## Runtime

Native acceptance uses synthetic database `crm_mobile_004`, API port 3102 and a
copied isolated executable. Startup health/readiness passed with additive schemas
through `20260930000003`. Shared services and installed demo identities are
preserved. The first combined build found duplicate readiness helper methods from
the merge; `4edf763` removes them, and the subsequent build passed. Failed build
output remains in `native-api-build.log`; the successful retry is
`native-api-build2.log`.

## D-050 evidence

The independent `crm_mobile_004_perf` fixture contains 25,000 People, 50 members
and 111 stages in the test Organization. The existing `mobile_today_perf` example,
built at `4edf763` with `test-support`, passed its single same-build alternating
pair at fixed clock `2026-09-13T18:00:00Z`: 10 warmups and 40 measured requests
per side, one concurrent load. Baseline/current p95 were **145.48/122.37 ms**;
the allowed increase was 25 ms. All 100-item DTOs had identical SHA-256
`c384097badc17d626924b0e819dee1c3360cdd6150faf69f0d1b592a834e3638`.
Results: `mobile-today-pair.json` and `perf-build.log` in the integration evidence
directory. These laptop timings are relative evidence, not production capacity.

The retained helper identifies its source as `9eaeb0a`, not this slice's parent.
The coordinator inspected its ordinary fixed-clock path against `44dcf52`:
intervening changes add the mobile transaction entry point/clock branch without
changing the selected ordinary execution path. This slice does not modify Today.
The attribution is retained rather than relabelling the older frozen helper.

`mobile-hot-plans.sql` and `.log` record one normal-planner
`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` pass for current stage, catalog read/
revision/page, Person summary, stage lock and receipt revision. Person access uses
`person_id_organization_id_key`; first/next catalog pages use generation-scoped
indexes across 100 synthetic generations and 11,100 stage snapshot rows. Small
catalog/Organization/member tables use bounded sequential scans independent of
the People count. The initial summary plan's contact scans covered only the 100
contacts in the base fixture. The coordinator corrected that fixture gap by
populating 49,900 contacts and rerunning only the affected summary plan
(`mobile-contact-plan.sql` / `.log`): both contact lookups use
`contact_method_history_review_primary`, each returning one row; Person uses its
primary key. Execution was 0.116 ms. The paired benchmark was not repeated.

Migration interaction and final repository/SQLx/database gates pass at the source
recorded in the [integration record](MOBILE_004_010e4_IMPLEMENTATION_STATUS.md).
That record also links both completed native acceptance records and review fixes.

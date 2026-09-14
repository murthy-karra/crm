# Mobile 005 backend verification

Writer branch: `codex/mobile-005-backend`.  Base: `fcc05b3`; shared checkpoint
merged during implementation: `a565145`.  All build output used
`/private/tmp/crm-mobile005-010f3/mobile/target`; database gates used the
serialized Mobile helper and isolated `crm_mobile005_gate` environment.

## Passed

- `cargo fmt --check` — passed after final formatting.
- `cargo check -p crm-app -p crm-api` — passed after final pagination change.
- `cargo test -p crm-app domain::mobile::operations::tests --lib` — 2 passed:
  strict nullable-name parsing, original operation-array add ordinals, bounds,
  unknown fields and empty patch rejection.
- `cargo test -p crm-app realtime::events --lib` — 7 passed, including snake
  case serialization of the additive IDs-only `details_changed` variant.
- `python3 /private/tmp/crm-mobile005-010f3/run-db-check.py --lane mobile --log
  db_mobile004.log -- cargo test -p
  crm-api --test all db_mobile::mobile004_stage_receipts_catalog_and_review_hold
  -- --ignored --test-threads=1` — passed (1 test) against the additive schema.
- The same helper with `db_mobile005-final.log` and filter
  `db_mobile::mobile005_details_receipt_replay_and_revision_scope` — passed (1
  test): capability, add mapping/replay, name/contact revision, edit/remove,
  stage non-conflict and person-delete cascade. A prior rerun failed only
  because the test attempted an app-role Person DELETE; it was corrected to use
  the migrator role for the explicit cascade assertion, then passed.
- `git diff --check` — passed.

## Integrated checks at 382aadc

Coordinator runner: private `run-db-check.py --lane integration`, isolated
`crm_mobile005_integration_gate`, Cargo output in `integration/target`. Commands
used `cargo test -p crm-api --test all FILTER -- --ignored --test-threads=1`.

- Filter `mobile005`: 8/8 passed in `integration/mobile005-integrated-tests3.log`.
  This covers the capture/profile lock overlap, stale/ABA and unrelated-writer
  behavior, add/edit/remove/normalized swaps with stable contact IDs/order,
  validation and rollback, exact/no-op receipt replay, concurrent replay,
  IDs-only publication/suppression and publisher failure after commit, intake
  lock ordering, ordinary typed-command review hold, foreign Person/contact
  denial and same-Organization household contacts.
- Paging uses 151 oversized imported contacts totaling about1.2MiB across at
  least three bounded current-profile pages, with the sealed generation contact
  representation, unchanged lease, stale-cursor denial and individual over-limit
  failure. Untouched imported content survives a valid changed-name operation.
- Filters `db_mobile::atomic_replay_conflict_dependency_and_old_web_commands`
  and `db_capture_unmatched::link_optionally_adds_the_counterparty_as_a_contact_method`:
  both passed in `integration/mobile005-legacy-and-capture-regression.log`.
  Legacy receipts omit the new mapping field; new Person-details receipts always
  include it, even when empty. Capture contact creation advances the details
  revision; exact capture replay does not.
- `cargo fmt --all --check` passed on382aadc.

Prior attempts remain retained: `mobile005-integrated-tests.log` had one failure
because an app-role query was not visible in `pg_stat_activity`; the assertion
now checks the known blocker PID through `pg_blocking_pids`. The second attempt
had one fixture failure from a missing required tag actor; that fixture was
corrected before the final8/8 pass.

## Remaining integrated coverage

The coordinator subsequently added explicit details-revision assertions to the
existing original/admitted refresh proofs. Four selected tests passed through
`cargo nextest run -p crm-api --test all --run-ignored only --test-threads 1`:
original refresh, admitted refresh, stage-only admitted refresh with settlement
rollback, and original-to-admission execution. Log:
`integration/mobile005-import-refresh-composition.log` (24.367s test execution).
These prove old private permits compose with the new derived revisions, real
name/contact changes advance details and stage-only changes preserve the token.

An additive follow-up migration, `20261001000002`, closes contact revision overflow
without editing the already-applied first Mobile005 migration. The test
`mobile005_revision_overflow_rolls_back_contacts_but_allows_erasure` passed in
`integration/mobile005-overflow.log`: name/add/edit/remove at the maximum token
roll back data and receipt; erasure cascades still succeed. Explicit contact locks
now use UUID order after the parent/intake locks. Both changes are included in
the coordinator checkpoint following382aadc.

New010f3 metadata composition, D-050 and combined final gates remain pending.
`scripts/check-db` and `scripts/sqlx-prepare` now accept
`CRM_CHECK_ENV_FILE` for the isolated database environment and
`CRM_SQLX_TARGET_DIR` for the separate online-schema build. Default behavior is
unchanged; the coordinator uses these overrides without replacing root `.env`
or shared release artifacts. Native acceptance and independent implementation
review are recorded separately. No deployment or publication has occurred.

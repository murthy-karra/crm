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
  /private/tmp/crm-mobile005-010f3/mobile/db_mobile004.log -- cargo test -p
  crm-api --test all db_mobile::mobile004_stage_receipts_catalog_and_review_hold
  -- --ignored --test-threads=1` — passed (1 test) against the additive schema.
- The same helper with `db_mobile005-final.log` and filter
  `db_mobile::mobile005_details_receipt_replay_and_revision_scope` — passed (1
  test): capability, add mapping/replay, name/contact revision, edit/remove,
  stage non-conflict and person-delete cascade. A prior rerun failed only
  because the test attempted an app-role Person DELETE; it was corrected to use
  the migrator role for the explicit cascade assertion, then passed.
- `git diff --check` — passed.

## Remaining integrated coverage

The coordinator owns the capture/profile-overlap and all-writer review-guard
composition checks. It should also run the final DB/API case for current-profile
multi-page byte accumulation and cursor traversal after integration; this lane
implemented the bounded behavior and ran compile checks, but did not run a
separate >512-KiB database fixture. Publication-failure recovery and full final
gates (`scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`) remain
coordinator-only sequential checks. No runtime services, native stores, shared
release output, `.sqlx`, deployment, or secrets were modified.

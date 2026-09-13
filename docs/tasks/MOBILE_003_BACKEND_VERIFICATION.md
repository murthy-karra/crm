# Mobile 003 backend verification

Executed on 2026-09-13 in the `codex/mobile-003-backend` worktree. Every
database-backed command below used SQLx's isolated ephemeral database created
from `MIGRATION_DATABASE_URL`; no shared API, demo service, installed native
store, `crm_mobile_003`, or customer data was used.

## Focused results

| Command | Result | Evidence covered |
|---|---|---|
| `CARGO_TARGET_DIR=/private/tmp/crm-mobile003-backend-target cargo fmt --check` | passed | Rust formatting |
| `CARGO_TARGET_DIR=/private/tmp/crm-mobile003-backend-target cargo check -p crm-app -p crm-api` | passed | application/API compilation with unchanged offline SQLx cache |
| `CARGO_TARGET_DIR=/private/tmp/crm-mobile003-backend-target cargo test -p crm-app log_contact_attempt --lib` | passed, 1 test | closed contact enums retain their existing serde mapping |
| `DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/crm-mobile003-backend-target cargo test -p crm-api --test all mobile003_contact_attempt_receipt_time_and_replay -- --ignored --nocapture` | passed, 1 test | real router/app-role path: capability, concurrent replay, one fact/receipt, exact receipt shape, fact envelope, microsecond occurrence normalization, no fabricated Person revision, payload mismatch, future rejection without marker, and revised retry |
| same harness, `mobile002_edit_revisions_receipts_and_current_reads` | passed, 1 test | Mobile 002 note/task operation and receipt compatibility after the additive receipt CHECK change |
| same harness, `current_authority_cross_org_receipts_and_workspace_hold` | passed, 1 test | existing live authority, cross-Organization, receipt, and review-hold behavior |
| same harness, `log_contact_attempt_writes_exactly_one_fact_with_full_envelope` | passed, 1 test | pre-existing Web manual-contact wrapper remains one fact with its historical envelope/clocks |
| same harness, `legacy_contact_occurrence_is_sampled_after_person_lock_release` | passed, 1 test | lock contention proves the legacy wrapper samples occurrence time only after the Person lock releases |
| same harness, `correcting_an_answered_call_writes_one_correction_row_with_the_call_envelope` | passed, 1 test | call-correction fact insertion preserves correction semantics |

The test command sources the repository `.env` privately and never prints a
connection string or credential. The test harness creates and removes its own
database for each `#[sqlx::test]` invocation.

## Coordinator integration follow-up

After integrating the foundation and legacy lock-clock correction, the coordinator
generated typed SQLx metadata against a freshly migrated throwaway database.
The first prepare exposed a missing closing SQL alias quote; `fef40d8` fixes it
and records the successfully generated metadata. The isolated API/migrator build
passed and API3102 accepted a real HTTP contact operation and returned the same
fact on replay, with `resource_type=contact_attempt` and null committed revision.

With the repository `.env` sourced privately, the command
`DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/crm-mobile003-integrated-target cargo test -p crm-api --test all mobile003_ -- --ignored --nocapture`
passed all 3 tests (3.94 seconds test execution). Beyond the original receipt/time
test, new cases prove that a two-day-old contact cannot satisfy a one-day-old
Inquiry; a later contact does, and a subsequent older upload cannot move the
maximum activity time backwards. Every seal equals the ordinary Today query at
the same evaluation instant while the Person revision remains unchanged.

A deliberately failing receipt trigger rolls back fact, receipt and activity
date. The contact-specific receipt path rejects cross-Organization targets, other
actors' contexts, review-held workspaces and inactive members. After reauthorization
and Person deletion, replay/lookup return opaque 404 while the original fact and
operation marker remain and no second fact appears. These tests use their own
ephemeral databases, separately from native runtime fixtures.

## Paired Today measurement

The coordinator ran the existing `mobile_today_perf` example with
`--features perf-harness --locked`, the isolated integrated Cargo target and
`MOBILE_PERF_CLOCK=2026-09-13T12:00:00Z` against the retained synthetic
`crm_mobile_003` database. Both paths ran in one build, alternating order, with
10 warmups and 40 measured samples per side. All 100 DTOs matched exactly
(SHA-256 `ea39b3f152b5bd5a6580eabd9ad71978a7369963ea6a08d8420e6fdd8f062bcc`).
Baseline `9eaeb0a` median/p95 was 51.07/122.84 ms; current was 48.43/131.89 ms.
The p95 increase was 9.05 ms, below the 25 ms allowance: passed. Raw output is
`/private/tmp/crm-mobile003-qa/today-paired.json`. This is synthetic local
performance evidence, not a production or physical cellular measurement.

## Remaining integration limits

This establishes the backend/API contract and isolated PostgreSQL behavior.
It does not establish iOS or Android protected-store behavior,
physical-device behavior, or a final coordinated repository
database gate. Those belong to the dependent native and coordinator lanes.

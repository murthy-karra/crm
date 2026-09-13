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
| same harness, `correcting_an_answered_call_writes_one_correction_row_with_the_call_envelope` | passed, 1 test | call-correction fact insertion preserves correction semantics |

The test command sources the repository `.env` privately and never prints a
connection string or credential. The test harness creates and removes its own
database for each `#[sqlx::test]` invocation.

## Limits

This establishes the backend/API contract and isolated PostgreSQL behavior.
It does not establish iOS or Android protected-store behavior, actual API3102
runtime behavior, physical-device behavior, or a final coordinated repository
database gate. Those belong to the dependent native and coordinator lanes.

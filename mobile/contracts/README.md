# Mobile 001 native fixtures

These JSON files are actual outputs and inputs from the isolated PostgreSQL/Axum
proof in `backend/crates/crm-api/tests/db_mobile.rs`. They contain synthetic data.
`error_cases.json` is a hand-authored table of the frozen error contract and
expected client recovery behavior; it is not a claim that native UI recovery
has already been tested. The other JSON files are captured proof outputs.
They are decoding and persistence fixtures; opaque cursors expire and cannot be
sent to a different running database. The frozen wire rules are in
`docs/tasks/MOBILE_001_CONTRACT.md`.

`bootstrap`, add/create/complete requests and receipts, replay and payload mismatch
belong to the atomic operation scenario. `reconciliation_bootstrap`, reconciliation,
summary/notes/tasks pages, seal and generation_changed belong to a separate download
scenario. Their actor/Organization/context IDs intentionally differ: never combine
those accounts' stores or apply one scenario's response to the other's database.

The download fixture has 100 selected People, 1,000 notes and 1,000 tasks. Page
files contain the first bounded page; the test traverses every page and tests 100
queued operations with lost-response retries. `selection_25000_plan` records a
separate bounded PostgreSQL measurement, excluding literal UUID filter arrays.
It is development evidence, not a customer-capacity guarantee.

To regenerate, source the isolated mobile lane `.env`, set DATABASE_URL to its
MIGRATION_DATABASE_URL, SQLX_OFFLINE=true, and MOBILE_FIXTURE_DIR to this absolute
directory; then run the ignored db_mobile tests. Never use a customer/shared
runtime for these tests.

Native promotion rules: all three components must be complete and the generation
sealed. Persist pages with checkpoints; serialize acknowledgment and promotion;
retain accepted overlays until a complete active Person bundle reaches its receipt
person_revision. An old seal or generic not_found never permits dropping pending
work. Test malformed identity, seven-day expiry, process termination and a failed
local write separately on each native platform.

//! Aggregation hub (test-binary consolidation chunk, docs/tasks/
//! TEST_BINARY_CONSOLIDATION.md): pulls every `tests/*.rs` integration
//! test file into ONE compilation unit via `#[path]`, so cargo
//! compiles+links once instead of once per file (crm-api/Cargo.toml
//! sets `autotests = false` to stop cargo auto-discovering each file as
//! its own binary). `livekit_telephony.rs` stays its own separate
//! `[[test]]` target (see crm-api/Cargo.toml) — it is excluded from the
//! default run and invoked by name only from `scripts/check-telephony`,
//! and must never build as part of the default `cargo test`/`nextest
//! run`.
//!
//! Every module below is a straight `#[path]` reference to the original
//! file, unmodified except for the 26 files that used to declare their
//! own `mod common;` (referencing the shared harness at
//! `tests/common/mod.rs`).
//!
//! mod-common resolution note: a bare `mod common;` inside a file loaded
//! via `#[path]` actually resolves relative to *that file's own
//! directory on disk* (here, `tests/`, since every aggregated file and
//! `common/` are direct siblings), not to its logical position in this
//! crate's new module tree — verified empirically (clean-build compiles,
//! and a decoy `tests/db_people/common.rs` placed at the tree-implied
//! location was ignored in favor of the real `tests/common/mod.rs`)
//! before scaling from 3 files to all 40. So the leave-it-as-is approach
//! DOES compile and run correctly. It was still changed, for a different
//! reason: `cargo clippy --all-targets -D warnings` (part of
//! `./scripts/check`) rejects loading the same file as a module 26 times
//! (`clippy::duplicate_mod`, part of `clippy::all`). Fix: `common` is
//! declared ONCE, here, and every formerly-`mod common;` file now
//! reaches it via `crate::common::` instead of a local `mod common;` +
//! `common::` (mechanical rename, no behavior change — `common` has no
//! state, so one shared instance vs. 26 copies is not observable). See
//! docs/tasks/TEST_BINARY_CONSOLIDATION.md for the full writeup.

#[path = "common/mod.rs"]
mod common;

#[path = "capture.rs"]
mod capture;

#[path = "centrifugo_realtime.rs"]
mod centrifugo_realtime;

#[path = "db_admin.rs"]
mod db_admin;

#[path = "db_calls.rs"]
mod db_calls;

#[path = "db_capture_address.rs"]
mod db_capture_address;

#[path = "db_capture_receive.rs"]
mod db_capture_receive;

#[path = "db_capture_unmatched.rs"]
mod db_capture_unmatched;

#[path = "db_contact_attempts.rs"]
mod db_contact_attempts;

#[path = "db_identity.rs"]
mod db_identity;

#[path = "db_inbound_email.rs"]
mod db_inbound_email;

#[path = "db_inbound_email_intake.rs"]
mod db_inbound_email_intake;

#[path = "db_intake.rs"]
mod db_intake;

#[path = "db_intake_address.rs"]
mod db_intake_address;

#[path = "db_intake_extraction.rs"]
mod db_intake_extraction;

#[path = "db_intake_rotation.rs"]
mod db_intake_rotation;

#[path = "db_intake_round_robin.rs"]
mod db_intake_round_robin;

#[path = "db_intake_settings.rs"]
mod db_intake_settings;

#[path = "db_intake_system_routing.rs"]
mod db_intake_system_routing;

#[path = "db_intake_workbench.rs"]
mod db_intake_workbench;

#[path = "db_operator.rs"]
mod db_operator;

#[path = "db_operator_call.rs"]
mod db_operator_call;

#[path = "db_people.rs"]
mod db_people;

#[path = "db_people_filter.rs"]
mod db_people_filter;

#[path = "db_realtime.rs"]
mod db_realtime;

#[path = "db_schema.rs"]
mod db_schema;

#[path = "db_today.rs"]
mod db_today;

#[path = "db_today_client_replied.rs"]
mod db_today_client_replied;

#[path = "health.rs"]
mod health;

#[path = "inbound_email.rs"]
mod inbound_email;

#[path = "intake.rs"]
mod intake;

#[path = "intake_settings.rs"]
mod intake_settings;

#[path = "intake_workbench.rs"]
mod intake_workbench;

#[path = "operator.rs"]
mod operator;

#[path = "operator_deps.rs"]
mod operator_deps;

#[path = "people.rs"]
mod people;

#[path = "realtime.rs"]
mod realtime;

#[path = "session.rs"]
mod session;

#[path = "stages.rs"]
mod stages;

#[path = "telephony.rs"]
mod telephony;

#[path = "today.rs"]
mod today;

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

#[path = "db_calls_corrections.rs"]
mod db_calls_corrections;

#[path = "db_calls_outcome_today.rs"]
mod db_calls_outcome_today;

#[path = "db_capture_address.rs"]
mod db_capture_address;

#[path = "db_capture_receive.rs"]
mod db_capture_receive;

#[path = "db_capture_unmatched.rs"]
mod db_capture_unmatched;

#[path = "db_contact_attempts.rs"]
mod db_contact_attempts;

#[path = "db_custom_fields.rs"]
mod db_custom_fields;

#[path = "db_custom_field_filters.rs"]
mod db_custom_field_filters;

#[cfg(feature = "perf-harness")]
#[path = "db_custom_field_filter_perf.rs"]
mod db_custom_field_filter_perf;

#[path = "db_identity.rs"]
mod db_identity;

#[path = "db_inbound_email.rs"]
mod db_inbound_email;

#[path = "db_inbound_email_intake.rs"]
mod db_inbound_email_intake;

#[path = "db_inquiry_append_only.rs"]
mod db_inquiry_append_only;

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

#[path = "db_notes.rs"]
mod db_notes;

#[path = "db_operator.rs"]
mod db_operator;

#[path = "db_operator_call.rs"]
mod db_operator_call;

#[path = "db_operator_filter.rs"]
mod db_operator_filter;

#[path = "db_operator_task.rs"]
mod db_operator_task;

#[path = "db_people.rs"]
mod db_people;

#[path = "db_people_filter.rs"]
mod db_people_filter;

#[path = "db_people_sort.rs"]
mod db_people_sort;

#[path = "db_person_last_activity.rs"]
mod db_person_last_activity;

#[path = "db_realtime.rs"]
mod db_realtime;

#[path = "db_schema.rs"]
mod db_schema;

#[path = "db_statement_equivalence.rs"]
mod db_statement_equivalence;

#[path = "db_saved_lists.rs"]
mod db_saved_lists;

#[path = "db_saved_lists_sort.rs"]
mod db_saved_lists_sort;

#[path = "db_saved_lists_tags.rs"]
mod db_saved_lists_tags;

#[path = "db_tags.rs"]
mod db_tags;

#[path = "db_tasks.rs"]
mod db_tasks;

#[path = "db_today.rs"]
mod db_today;

#[path = "db_today_builtin_parity.rs"]
mod db_today_builtin_parity;

#[path = "db_today_client_replied.rs"]
mod db_today_client_replied;

#[path = "db_today_sources.rs"]
mod db_today_sources;

#[path = "db_today_source_acceptance.rs"]
mod db_today_source_acceptance;

#[path = "db_today_source_contracts.rs"]
mod db_today_source_contracts;

#[path = "db_today_source_filter_parity.rs"]
mod db_today_source_filter_parity;

#[path = "db_today_source_operator.rs"]
mod db_today_source_operator;

#[path = "db_today_source_races.rs"]
mod db_today_source_races;

#[path = "db_today_source_failures.rs"]
mod db_today_source_failures;

#[path = "db_today_source_hooks.rs"]
mod db_today_source_hooks;

#[path = "db_today_source_settings.rs"]
mod db_today_source_settings;

#[path = "db_today_source_deadlines.rs"]
mod db_today_source_deadlines;

#[path = "db_today_source_telemetry.rs"]
mod db_today_source_telemetry;

mod db_today_system_feed_commands;

#[path = "db_today_system_feed_evaluation.rs"]
mod db_today_system_feed_evaluation;

#[path = "db_today_system_feed_preview.rs"]
mod db_today_system_feed_preview;

#[path = "db_today_system_feeds.rs"]
mod db_today_system_feeds;

#[path = "db_today_feed_equivalence.rs"]
mod db_today_feed_equivalence;

#[path = "db_today_system_feed_call_failures.rs"]
mod db_today_system_feed_call_failures;

#[path = "db_today_feeds_http.rs"]
mod db_today_feeds_http;

#[path = "db_today_task_axis.rs"]
mod db_today_task_axis;

#[path = "db_today_task_axis_failures.rs"]
mod db_today_task_axis_failures;

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

#[path = "saved_lists.rs"]
mod saved_lists;

#[path = "session.rs"]
mod session;

#[path = "stages.rs"]
mod stages;

#[path = "telephony.rs"]
mod telephony;

#[path = "today.rs"]
mod today;

#[path = "db_migration.rs"]
mod db_migration;

mod db_snapshot_http;

mod db_snapshot_worker;

mod db_snapshot_scale;

#[cfg(feature = "perf-harness")]
mod db_import_contact_perf;

mod db_workspace_background;
mod db_workspace_http;

mod db_import_http;
mod db_import_source;
#[path = "fixtures/import_support.rs"]
mod import_support;

mod db_import_readers;

mod db_import_gate;

#[cfg(feature = "perf-harness")]
mod db_import_plans;

mod db_metadata_import_http;
mod db_metadata_import_source;

mod db_metadata_import_gate;

#[cfg(feature = "perf-harness")]
mod db_metadata_import_plans;

mod db_metadata_import_r1;

mod db_metadata_import_acceptance;
mod db_metadata_import_concurrency;

mod db_activity_review;

mod db_activity_source;

mod db_activity_lifecycle;

#[cfg(feature = "perf-harness")]
mod db_activity_import_plans;

#[cfg(feature = "perf-harness")]
mod db_activity_person_detail_perf;

mod db_activity_races;

mod db_activity_r1_control;
mod db_activity_r1_source;

mod db_activity_upgrade;

mod db_history_capture;
mod db_history_capture_authority;
#[cfg(feature = "perf-harness")]
mod db_history_capture_plans;
mod db_history_capture_read_boundaries;
mod db_history_capture_release;
mod db_history_capture_support;
mod db_history_import;
mod db_history_import_authority;
mod db_history_import_support;
mod db_history_review;
mod db_history_timeline_compat;

#[path = "db_core_change_reports.rs"]
mod db_core_change_reports;

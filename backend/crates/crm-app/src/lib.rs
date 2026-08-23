//! The application layer (docs/specs/SLICE_006a.md, D-028 §1/§5):
//! domain commands and queries, realtime publishing, telephony, and the
//! non-HTTP parts of auth — everything `crm-api` mounts routes over and
//! everything a future `crm-operator -> crm-app` edge may reach (006b).
//! No Axum, no `crm-operator`, no `crm-api` (fenced by
//! crm-api/tests/operator_deps.rs).

pub mod auth;
pub mod config;
pub mod domain;
pub mod realtime;
pub mod telephony;

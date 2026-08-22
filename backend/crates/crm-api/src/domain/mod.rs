//! Slice 002 domain layer (docs/specs/SLICE_002.md §4): typed application
//! commands, the fact envelope, and the read models/queries that back them.
//! Business mutations pass through `commands::*` only (AGENTS.md §4.8).

pub mod commands;
pub mod contact;
pub mod envelope;
pub mod facts;
pub mod inquiry;
pub mod person;
pub mod raw_payload;
pub mod stage;

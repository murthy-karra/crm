//! Typed application commands (AGENTS.md §4.8, docs/specs/SLICE_002.md §4).
//! The Web application, native clients, public API, automation, and the AI
//! Operator all use this same command layer — no second mutation path.

pub mod assign_person;
pub mod change_person_stage;
pub mod receive_inquiry;

pub use assign_person::{assign_person, AssignPerson};
pub use change_person_stage::{change_person_stage, ChangePersonStage};
pub use receive_inquiry::{
    receive_inquiry, ReceiveInquiry, ReceiveInquiryOutcome, RoutingStrategy,
};

use crate::domain::raw_payload::crypto::CryptoError;

#[derive(Debug)]
pub enum CommandError {
    PersonNotFound,
    InvalidAssignee,
    InvalidStage,
    NoStagesConfigured,
    Crypto,
    /// Data read back from our own database didn't match an expected
    /// shape (e.g. an enum-like column value with no Rust variant) — a
    /// data-integrity problem, not a client error. Fail closed rather than
    /// panic (`RoutingStrategy::from_str`'s only caller).
    Corrupt,
    /// `receive_inquiry`'s bounded retry loop around
    /// `pg_try_advisory_xact_lock` exhausted its wall-clock budget without
    /// acquiring the per-Organization intake lock — distinct from
    /// `Database` so it maps to its own `503 intake_busy` (with
    /// `Retry-After`) rather than the generic `unavailable`. The
    /// `raw_payload` row is left exactly as Phase A wrote it (`pending`);
    /// a re-POST retries from scratch.
    IntakeBusy,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CommandError {
    fn from(err: sqlx::Error) -> Self {
        CommandError::Database(err)
    }
}

impl From<CryptoError> for CommandError {
    fn from(_: CryptoError) -> Self {
        CommandError::Crypto
    }
}

impl CommandError {
    /// A stable, PII-free tag for logging (docs/specs/SLICE_002.md §8):
    /// never the variant's `Display`/`Debug` text — for `Database`, that
    /// embeds the inner `sqlx::Error`, which callers must not log verbatim
    /// on the intake failure paths this is used for.
    pub fn kind(&self) -> &'static str {
        match self {
            CommandError::PersonNotFound => "person_not_found",
            CommandError::InvalidAssignee => "invalid_assignee",
            CommandError::InvalidStage => "invalid_stage",
            CommandError::NoStagesConfigured => "no_stages_configured",
            CommandError::Crypto => "crypto",
            CommandError::Corrupt => "corrupt",
            CommandError::IntakeBusy => "intake_busy",
            CommandError::Database(_) => "database",
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::PersonNotFound => write!(f, "person not found"),
            CommandError::InvalidAssignee => write!(f, "invalid assignee"),
            CommandError::InvalidStage => write!(f, "invalid stage"),
            CommandError::NoStagesConfigured => write!(f, "organization has no stages configured"),
            CommandError::Crypto => write!(f, "crypto operation failed"),
            CommandError::Corrupt => write!(f, "unexpected data read back from the database"),
            CommandError::IntakeBusy => {
                write!(f, "timed out waiting for the organization's intake lock")
            }
            CommandError::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for CommandError {}

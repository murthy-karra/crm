//! Typed application commands (AGENTS.md §4.8, docs/specs/SLICE_002.md §4).
//! The Web application, native clients, public API, automation, and the AI
//! Operator all use this same command layer — no second mutation path.

pub mod assign_person;
pub mod change_person_stage;
pub mod correct_call_outcome;
pub mod dial_call;
pub mod hangup_call;
pub mod log_contact_attempt;
pub mod receive_inquiry;
pub mod start_call;

pub use assign_person::{assign_person, AssignPerson};
pub use change_person_stage::{change_person_stage, ChangePersonStage};
pub use correct_call_outcome::{
    correct_call_outcome, CallOutcomeCorrection, CorrectCallOutcome, CorrectedAttemptRef,
    CorrectionResult,
};
pub use dial_call::dial_call;
pub use hangup_call::hangup_call;
pub use log_contact_attempt::{
    log_contact_attempt, ContactAttemptRef, ContactChannel, ContactOutcome, LogContactAttempt,
};
pub use receive_inquiry::{
    receive_inquiry, ReceiveInquiry, ReceiveInquiryOutcome, RoutingStrategy,
};
pub use start_call::{start_call, StartCall};

use uuid::Uuid;

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

/// Errors of the Slice 006 call commands (docs/specs/SLICE_006.md §3,
/// §5), kept apart from `CommandError` the way `AdminCommandError` is.
#[derive(Debug)]
pub enum CallError {
    /// Foreign or nonexistent Person — byte-identical 404.
    PersonNotFound,
    /// Nonexistent, foreign, another Person's, or non-phone contact method
    /// — identical 422.
    InvalidContactMethod,
    /// The caller already has an active call (the partial unique index).
    CallInProgress {
        call_id: Uuid,
    },
    /// `dial` on a call that is not `placing` or was already dialed.
    InvalidCallState,
    /// Foreign or nonexistent call — 404.
    CallNotFound,
    /// Not the caller (`dial`/`hangup`).
    Forbidden,
    /// The provider failed at `start` (room creation).
    TelephonyUnavailable,
    /// `correct_call_outcome` on a call that never reached the callee —
    /// no `contact_attempted` row with `causation_id = call.id`
    /// (docs/specs/SLICE_006c.md §3) — 422.
    NoContactAttempt,
    /// `23505` on `contact_attempted_corrects_once`: the head was
    /// corrected by a writer that did not hold the call lock
    /// (docs/specs/SLICE_006c.md §3; unreachable from the command itself)
    /// — 409.
    CorrectionConflict,
    /// Data read back from our own database didn't match an expected
    /// shape (see `CommandError::Corrupt`).
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CallError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::Decode(_) => CallError::Corrupt,
            other => CallError::Database(other),
        }
    }
}

impl CallError {
    /// PII-free tag for spans/logs (never the `Database` payload).
    pub fn kind(&self) -> &'static str {
        match self {
            CallError::PersonNotFound => "person_not_found",
            CallError::InvalidContactMethod => "invalid_contact_method",
            CallError::CallInProgress { .. } => "call_in_progress",
            CallError::InvalidCallState => "invalid_call_state",
            CallError::CallNotFound => "call_not_found",
            CallError::Forbidden => "forbidden",
            CallError::TelephonyUnavailable => "telephony_unavailable",
            CallError::NoContactAttempt => "no_contact_attempt",
            CallError::CorrectionConflict => "correction_conflict",
            CallError::Corrupt => "corrupt",
            CallError::Database(_) => "database",
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Database(err) => write!(f, "database error: {err}"),
            other => write!(f, "{}", other.kind()),
        }
    }
}

impl std::error::Error for CallError {}

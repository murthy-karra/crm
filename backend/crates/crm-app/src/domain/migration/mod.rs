//! Slice 010a's bounded Follow Up Boss access assessment.  This module has
//! no import commands and the reader trait deliberately exposes only GET
//! probes from the fixed profile.

pub mod commands;
pub mod crypto;
pub(crate) mod import_display;
pub(crate) mod import_source;
pub mod import_worker;
pub mod imports;
pub mod metadata;
pub(crate) mod metadata_model;
pub(crate) mod metadata_queries;
pub(crate) mod metadata_source;
pub(crate) mod metadata_store;
pub mod metadata_worker;
pub mod reader;
pub mod snapshot;
pub(crate) mod snapshot_compare;
pub mod snapshot_preview;
pub mod snapshot_source;
pub mod snapshot_worker;
pub(crate) mod store;
pub mod worker;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FUB_PROFILE_V1: &str = "fub_assessment_v1";
pub const CHECKS: [&str; 19] = [
    "people_excluding_trash",
    "people_including_trash",
    "users",
    "stages",
    "custom_fields",
    "identity",
    "notes",
    "tasks",
    "tags",
    "inquiries_history",
    "calls",
    "texts",
    "emails",
    "recordings",
    "addresses",
    "relationships",
    "appointments",
    "deals",
    "automation_settings",
];

pub fn is_probed_check(key: &str) -> bool {
    matches!(
        key,
        "identity"
            | "people_excluding_trash"
            | "people_including_trash"
            | "users"
            | "stages"
            | "custom_fields"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentState {
    Queued,
    Running,
    WaitingRetry,
    Paused,
    Completed,
    Cancelled,
}
impl AssessmentState {
    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "waiting_retry" => Self::WaitingRetry,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingRetry => "waiting_retry",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::WaitingRetry)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pending,
    Running,
    WaitingRetry,
    Paused,
    Completed,
    Cancelled,
}
impl CheckState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::WaitingRetry => "waiting_retry",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Partial,
    Unavailable,
    NotChecked,
    CompleteForQuery,
}
impl Coverage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
            Self::NotChecked => "not_checked",
            Self::CompleteForQuery => "complete_for_query",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionView {
    pub id: Uuid,
    pub source_account_id: i64,
    pub source_display_name: Option<String>,
    pub source_access_scope: String,
    pub status: String,
    pub revision: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentCheckView {
    pub check_key: String,
    pub state: String,
    pub reported_total: Option<String>,
    pub retrieved_count: String,
    pub destination_readiness: String,
    pub reason_codes: Vec<String>,
    pub next_action: String,
    pub coverage: String,
    pub error_code: Option<String>,
    pub attempts: i32,
    pub observed_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentView {
    pub destination_organization_id: Uuid,
    pub source_account_id: i64,
    pub source_display_name: Option<String>,
    pub source_access_scope: String,
    pub id: Uuid,
    pub connection_id: Uuid,
    pub connection_revision: i32,
    pub profile_version: String,
    pub state: String,
    pub pause_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub checks: Vec<AssessmentCheckView>,
}

#[derive(Debug)]
pub enum MigrationError {
    NotFound,
    Forbidden,
    Conflict,
    InvalidInput,
    InvalidCredential,
    ImportConflict,
    ImportExpired,
    ImportBusy,
    InvalidImportChoice,
    SourceNotEligible,
    StorageLimit,
    WorkspaceNotEmpty,
    ReleaseNotReady,
    SourceAccountMismatch,
    ReaderUnavailable,
    Database(sqlx::Error),
    Crypto,
}
impl From<sqlx::Error> for MigrationError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}
impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotFound => "not found",
            Self::Forbidden => "forbidden",
            Self::Conflict => "conflict",
            Self::InvalidInput => "invalid input",
            Self::InvalidCredential => "invalid credential",
            Self::ImportExpired => "import expired",
            Self::ImportConflict => "import conflict",
            Self::ImportBusy => "import busy",
            Self::InvalidImportChoice => "invalid import choice",
            Self::SourceNotEligible => "source not eligible",
            Self::StorageLimit => "storage limit",
            Self::WorkspaceNotEmpty => "workspace not empty",
            Self::ReleaseNotReady => "release not ready",
            Self::SourceAccountMismatch => "source account mismatch",
            Self::ReaderUnavailable => "reader unavailable",
            Self::Database(_) => "database",
            Self::Crypto => "crypto",
        })
    }
}
impl std::error::Error for MigrationError {}

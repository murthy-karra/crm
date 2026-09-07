//! Stable failures from system-feed commands, preview and reads
//! (docs/specs/SLICE_011d.md §4, §6). The HTTP adapter owns status/envelope
//! mapping; this layer deliberately carries no Axum types. Shape mirrors
//! `saved_list::SavedListError` (011b) closely by design.

use crate::domain::person::filter::FilterError;

#[derive(Debug)]
pub enum TodayFeedError {
    Unauthenticated,
    Forbidden,
    MalformedRequest,
    NotFound,
    Conflict,
    InvalidStage,
    InvalidAssignee,
    /// The anchor-clause, `assigned_to: [me]`, or `fresh_within_hours`
    /// rule (spec §4) was violated. Structural/reference filter errors get
    /// their own codes above; this one is specific to the §1 feed rules.
    InvalidFeedRule,
    RevisionExhausted,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for TodayFeedError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl From<FilterError> for TodayFeedError {
    fn from(value: FilterError) -> Self {
        match value {
            FilterError::Malformed => Self::MalformedRequest,
            FilterError::InvalidStage => Self::InvalidStage,
            FilterError::InvalidAssignee => Self::InvalidAssignee,
            FilterError::Database(error) => Self::Database(error),
        }
    }
}

impl TodayFeedError {
    /// Safe classification for spans/logs. Never expose a wrapped SQL
    /// error, definition JSON, or subject items.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::NotFound => "not_found",
            Self::Conflict => "today_feed_conflict",
            Self::InvalidStage => "invalid_stage",
            Self::InvalidAssignee => "invalid_assignee",
            Self::InvalidFeedRule => "invalid_feed_rule",
            Self::RevisionExhausted => "revision_exhausted",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

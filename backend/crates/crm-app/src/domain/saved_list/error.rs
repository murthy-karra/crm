use crate::domain::person::filter::FilterError;

/// Stable failures from saved-list commands and reads. The HTTP adapter owns
/// status/envelope mapping; this layer deliberately carries no Axum types.
#[derive(Debug)]
pub enum SavedListError {
    Unauthenticated,
    NotFound,
    Forbidden,
    MalformedRequest,
    Conflict,
    RequestConflict,
    Deleted,
    LimitReached,
    TodaySourceLimitReached,
    InvalidStage,
    InvalidAssignee,
    /// docs/specs/SLICE_011e.md §4b.
    InvalidTag,
    UnsupportedFilter,
    RevisionExhausted,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SavedListError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl From<FilterError> for SavedListError {
    fn from(value: FilterError) -> Self {
        match value {
            FilterError::Malformed => Self::MalformedRequest,
            FilterError::InvalidStage => Self::InvalidStage,
            FilterError::InvalidAssignee => Self::InvalidAssignee,
            FilterError::InvalidTag => Self::InvalidTag,
            FilterError::Database(error) => Self::Database(error),
        }
    }
}

impl SavedListError {
    /// Safe classification for spans/logs. Never expose a wrapped SQL error,
    /// list name, filter, retry token, or fingerprint.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "unauthenticated",
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::Conflict => "saved_list_conflict",
            Self::RequestConflict => "saved_list_request_conflict",
            Self::Deleted => "saved_list_deleted",
            Self::LimitReached => "saved_list_limit_reached",
            Self::TodaySourceLimitReached => "today_source_limit_reached",
            Self::InvalidStage => "invalid_stage",
            Self::InvalidAssignee => "invalid_assignee",
            Self::InvalidTag => "invalid_tag",
            Self::UnsupportedFilter => "unsupported_filter",
            Self::RevisionExhausted => "revision_exhausted",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

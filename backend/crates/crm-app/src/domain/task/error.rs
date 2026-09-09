/// Stable failures from task commands and reads (docs/specs/SLICE_016.md
/// §3, §9). The HTTP adapter owns status/envelope mapping; this layer
/// deliberately carries no Axum types (the `tag::TagError`/`NoteError`
/// shape). Never carries a task title — only ids and fixed classification
/// labels ever reach a span or log through this type (AGENTS.md §9,
/// docs/specs/SLICE_016.md §1 rule 7).
#[derive(Debug)]
pub enum TaskError {
    NotFound,
    Forbidden,
    MalformedRequest,
    InvalidAssignee,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for TaskError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl TaskError {
    /// Safe classification for spans/logs. Never a wrapped SQL error or a
    /// task title (AGENTS.md §9 — task titles are never logged).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::InvalidAssignee => "invalid_assignee",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

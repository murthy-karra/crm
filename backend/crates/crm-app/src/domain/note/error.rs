/// Stable failures from note commands and reads (docs/specs/SLICE_015.md
/// §3, §6). The HTTP adapter owns status/envelope mapping; this layer
/// deliberately carries no Axum types (the `tag::TagError` shape). Never
/// carries a note body — only ids and fixed classification labels ever
/// reach a span or log through this type (AGENTS.md §9, D-053 §2).
#[derive(Debug)]
pub enum NoteError {
    NotFound,
    Forbidden,
    MalformedRequest,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for NoteError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl NoteError {
    /// Safe classification for spans/logs. Never a wrapped SQL error or a
    /// note body (AGENTS.md §9 — note bodies are never logged).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

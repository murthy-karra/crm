/// Stable failures from tag commands and reads (docs/specs/SLICE_011e.md
/// §3, §6). The HTTP adapter owns status/envelope mapping; this layer
/// deliberately carries no Axum types (the `saved_list::SavedListError`
/// shape).
#[derive(Debug)]
pub enum TagError {
    NotFound,
    Forbidden,
    MalformedRequest,
    TagLimitReached,
    PersonTagLimitReached,
    TagNameTaken,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for TagError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl TagError {
    /// Safe classification for spans/logs. Never expose a wrapped SQL
    /// error or a tag name (AGENTS.md §9 — tag names are never logged).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::TagLimitReached => "tag_limit_reached",
            Self::PersonTagLimitReached => "person_tag_limit_reached",
            Self::TagNameTaken => "tag_name_taken",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

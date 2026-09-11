/// Stable failures from custom-field commands and reads (docs/specs/
/// SLICE_019.md §3, §9). The HTTP adapter owns status/envelope mapping;
/// this layer deliberately carries no Axum types (the
/// `tag::TagError`/`task::TaskError` shape). Never carries a field label,
/// option label, or a Person's value — only ids and fixed classification
/// labels ever reach a span or log through this type (AGENTS.md §9,
/// docs/specs/SLICE_019.md §9: "labels and values never appear in spans,
/// logs, error envelopes, the ledger or the realtime payload").
#[derive(Debug)]
pub enum CustomFieldError {
    NotFound,
    Forbidden,
    MalformedRequest,
    /// 50 live definitions per Organization (D-050).
    LimitReached,
    /// A live field's label collides case-insensitively with another live
    /// field.
    LabelTaken,
    /// A value's variant, or a supplied `options` list, disagrees with the
    /// field's `field_type`.
    TypeMismatch,
    /// A value or option-list failed pure-function validation (the number
    /// pattern, the date window, the option-list bounds).
    InvalidValue,
    /// 50 live options per field (D-050).
    OptionLimitReached,
    /// A live option's label collides case-insensitively with another live
    /// option of the same field.
    OptionLabelTaken,
    /// A value write targeted a field that is currently archived.
    FieldArchived,
    /// A choice value named an option that is archived, belongs to a
    /// different field, or does not exist — byte-identical for all three
    /// (docs/specs/SLICE_019.md §3).
    UnknownOption,
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CustomFieldError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl CustomFieldError {
    /// Safe classification for spans/logs. Never a wrapped SQL error, a
    /// field/option label, or a Person's value (AGENTS.md §9).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::MalformedRequest => "malformed_request",
            Self::LimitReached => "custom_field_limit_reached",
            Self::LabelTaken => "custom_field_label_taken",
            Self::TypeMismatch => "type_mismatch",
            Self::InvalidValue => "invalid_value",
            Self::OptionLimitReached => "option_limit_reached",
            Self::OptionLabelTaken => "option_label_taken",
            Self::FieldArchived => "field_archived",
            Self::UnknownOption => "unknown_option",
            Self::Corrupt => "corrupt",
            Self::Database(_) => "database",
        }
    }
}

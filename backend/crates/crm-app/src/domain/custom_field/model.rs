use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{CustomFieldId, CustomFieldOptionId};

use super::error::CustomFieldError;

/// The closed `field_type` enum (docs/specs/SLICE_019.md §2, §13): Follow
/// Up Boss's four kinds, minus `isRecurring`/`hideIfEmpty` (not modelled
/// in 019a). Decoding a *stored* row goes through [`FieldType::from_db_str`]
/// rather than `serde_json` (the `TaskKind`/`Role` precedent) — a
/// `serde`-only round trip would silently accept any string `serde` itself
/// considers valid instead of failing closed on a corrupt/unexpected
/// stored value. The type is immutable after creation (spec §3): no
/// command ever writes this column after `CreateCustomField`'s INSERT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Number,
    Date,
    Choice,
}

impl FieldType {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldType::Text => "text",
            FieldType::Number => "number",
            FieldType::Date => "date",
            FieldType::Choice => "choice",
        }
    }

    /// `None` for an unrecognized stored value — a read path fails closed
    /// (`CustomFieldError::Corrupt`), never guesses.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "text" => Some(FieldType::Text),
            "number" => Some(FieldType::Number),
            "date" => Some(FieldType::Date),
            "choice" => Some(FieldType::Choice),
            _ => None,
        }
    }
}

/// A Person's value for one custom field (docs/specs/SLICE_019.md §3,
/// §4): the wire shape is externally tagged by construction — serde's
/// default enum representation for a single-field tuple variant is
/// `{"<rename>": <value>}`, and deserializing an object with zero, two,
/// or an unrecognized key fails closed without any extra attribute (the
/// spec's "the inner externally tagged enum rejects zero, two or unknown
/// keys by construction"). `Number` carries the client's or the server's
/// decimal text VERBATIM (docs/specs/SLICE_019.md §2: "numbers cross the
/// Rust boundary as text" — the workspace has no decimal crate and
/// `Cargo.*` is not owned by this slice); `Choice` is named to avoid
/// shadowing `std::option::Option` at every call site, and (de)serializes
/// as a bare UUID under the `option_id` key via `CustomFieldOptionId`'s
/// own `#[serde(transparent)]`.
#[derive(Clone, Serialize, Deserialize)]
pub enum CustomFieldValue {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "number")]
    Number(String),
    #[serde(rename = "date")]
    Date(NaiveDate),
    #[serde(rename = "option_id")]
    Choice(CustomFieldOptionId),
}

impl CustomFieldValue {
    pub fn field_type(&self) -> FieldType {
        match self {
            CustomFieldValue::Text(_) => FieldType::Text,
            CustomFieldValue::Number(_) => FieldType::Number,
            CustomFieldValue::Date(_) => FieldType::Date,
            CustomFieldValue::Choice(_) => FieldType::Choice,
        }
    }
}

/// No `Debug` derive: this type carries a Person's custom-field value —
/// text, a number string, a date, or an option id — and rule 9 (spec §9)
/// requires it never reach a span, log, error envelope, the ledger, or
/// the realtime payload. A stray `?value`/`{:?}` must be a compile error,
/// not a redaction someone could forget.
impl std::fmt::Debug for CustomFieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Even the redacted form never reaches a real log call site (no
        // command or view type derives `Debug` through this impl without
        // hand-redacting its own fields first — see `commands.rs`'s
        // command structs and this module's `CustomField`/`Value`), but a
        // panic message or test-failure formatter can still reach here,
        // so the fallback itself stays label/value-free.
        write!(f, "CustomFieldValue({})", self.field_type().as_str())
    }
}

/// One option on a `choice` field (docs/specs/SLICE_019.md §4): `{"id",
/// "label","position","archived_at"}`. `Debug` is a hand-written,
/// redacting impl (never `#[derive(Debug)]`, the `crm_app::domain::task::
/// Task` pattern): this struct carries an admin-authored label, so a
/// stray `?option`/`{:?}` in a log, panic, or test-failure message must
/// never print it, only its length.
#[derive(Clone, Serialize)]
pub struct CustomFieldOption {
    pub id: CustomFieldOptionId,
    pub label: String,
    pub position: i32,
    pub archived_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for CustomFieldOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomFieldOption")
            .field("id", &self.id)
            .field("label_chars", &self.label.chars().count())
            .field("position", &self.position)
            .field("archived_at", &self.archived_at)
            .finish()
    }
}

/// The exact `CustomField` response shape (docs/specs/SLICE_019.md §4):
/// `{"id","label","field_type","position","archived_at","person_count",
/// "options"}` (options ordered `position, id`; `[]` for non-choice
/// types). `Debug` is a hand-written, redacting impl, the same reason as
/// `CustomFieldOption` above — `options: Vec<CustomFieldOption>` composes
/// safely because each option's own redacting `Debug` is what actually
/// runs (the `TaskWithPerson`/`Task` precedent).
#[derive(Clone, Serialize)]
pub struct CustomField {
    pub id: CustomFieldId,
    pub label: String,
    pub field_type: FieldType,
    pub position: i32,
    pub archived_at: Option<DateTime<Utc>>,
    pub person_count: i64,
    pub options: Vec<CustomFieldOption>,
}

impl std::fmt::Debug for CustomField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomField")
            .field("id", &self.id)
            .field("label_chars", &self.label.chars().count())
            .field("field_type", &self.field_type)
            .field("position", &self.position)
            .field("archived_at", &self.archived_at)
            .field("person_count", &self.person_count)
            .field("options", &self.options)
            .finish()
    }
}

/// The exact `GET /api/people/{id}` `custom_fields[]` / `PUT`/`DELETE
/// …/custom-fields/{field_id}` response row shape (docs/specs/SLICE_019.md
/// §4): `{"field_id","label","field_type","value","option_label",
/// "updated_at"}`. `Debug` is a hand-written, redacting impl: this struct
/// carries both an admin-authored label and a Person's value (rule 9).
#[derive(Clone, Serialize)]
pub struct Value {
    pub field_id: CustomFieldId,
    pub label: String,
    pub field_type: FieldType,
    pub value: CustomFieldValue,
    pub option_label: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Value")
            .field("field_id", &self.field_id)
            .field("label_chars", &self.label.chars().count())
            .field("field_type", &self.field_type)
            .field("value", &self.value)
            .field(
                "option_label_chars",
                &self.option_label.as_ref().map(|s| s.chars().count()),
            )
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// The number pattern (docs/specs/SLICE_019.md §2): `^-?[0-9]{1,15}
/// (\.[0-9]{1,4})?$`, a pure function (no `regex` dependency — `Cargo.*`
/// is not owned by this slice and the workspace enables no decimal type
/// for sqlx). Matches exactly what `NUMERIC(19, 4)` can hold: up to 15
/// integer digits, an optional 1–4 digit fractional part. Rejects
/// scientific notation, a leading `+`, thousands separators, and a bare
/// decimal point with no digits on either side.
pub fn validate_number_pattern(raw: &str) -> Result<(), CustomFieldError> {
    let unsigned = raw.strip_prefix('-').unwrap_or(raw);
    let (int_part, frac_part) = match unsigned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (unsigned, None),
    };
    let digits_only = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if int_part.is_empty() || int_part.len() > 15 || !digits_only(int_part) {
        return Err(CustomFieldError::InvalidValue);
    }
    if let Some(frac) = frac_part {
        if frac.is_empty() || frac.len() > 4 || !digits_only(frac) {
            return Err(CustomFieldError::InvalidValue);
        }
    }
    Ok(())
}

/// The `text_value` CHECK (docs/specs/SLICE_019.md §2): trimmed of space,
/// tab, CR, LF (`btrim(text_value, E' \t\r\n')`, not Rust's Unicode-aware
/// `trim()` — kept narrow to match the CHECK exactly, the `TaskTitle`
/// precedent's stated reasoning in reverse: this validator must accept
/// exactly what the CHECK accepts, since the trimmed string is what gets
/// stored), 1–500 code points, no newline.
pub fn validate_text_value(raw: &str) -> Result<String, CustomFieldError> {
    let trimmed = raw.trim_matches([' ', '\t', '\r', '\n']);
    let count = trimmed.chars().count();
    if count == 0 || count > 500 || trimmed.contains('\n') {
        return Err(CustomFieldError::InvalidValue);
    }
    Ok(trimmed.to_string())
}

/// The calendar-date window (docs/specs/SLICE_019.md §2 limits):
/// 1900-01-01 to 2200-12-31 inclusive.
pub fn validate_date_range(date: NaiveDate) -> Result<(), CustomFieldError> {
    let min = NaiveDate::from_ymd_opt(1900, 1, 1).expect("1900-01-01 is a valid calendar date");
    let max = NaiveDate::from_ymd_opt(2200, 12, 31).expect("2200-12-31 is a valid calendar date");
    if date < min || date > max {
        return Err(CustomFieldError::InvalidValue);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_as_str_and_from_db_str_round_trip() {
        for ft in [
            FieldType::Text,
            FieldType::Number,
            FieldType::Date,
            FieldType::Choice,
        ] {
            assert_eq!(FieldType::from_db_str(ft.as_str()), Some(ft));
        }
        assert_eq!(FieldType::from_db_str("unknown"), None);
    }

    #[test]
    fn number_pattern_accepts_the_spec_examples() {
        assert!(validate_number_pattern("12.50").is_ok());
        assert!(validate_number_pattern("12.5").is_ok());
        assert!(validate_number_pattern("-4").is_ok());
        assert!(validate_number_pattern("0").is_ok());
        assert!(validate_number_pattern("0.0001").is_ok());
        assert!(validate_number_pattern(&"9".repeat(15)).is_ok());
    }

    #[test]
    fn number_pattern_rejects_the_spec_examples() {
        assert!(validate_number_pattern("1e5").is_err());
        assert!(validate_number_pattern("1.23456").is_err());
        assert!(validate_number_pattern(&"1".repeat(16)).is_err());
        assert!(validate_number_pattern("").is_err());
        assert!(validate_number_pattern(".").is_err());
        assert!(validate_number_pattern(".5").is_err());
        assert!(validate_number_pattern("5.").is_err());
        assert!(validate_number_pattern("+5").is_err());
        assert!(validate_number_pattern("1,000").is_err());
        assert!(validate_number_pattern("-").is_err());
        assert!(validate_number_pattern("--5").is_err());
    }

    #[test]
    fn text_value_trims_space_tab_cr_lf_and_bounds_length() {
        assert_eq!(
            validate_text_value("  \t Referral \r\n").unwrap(),
            "Referral"
        );
        assert!(validate_text_value("").is_err());
        assert!(validate_text_value("   ").is_err());
        assert!(validate_text_value(&"a".repeat(500)).is_ok());
        assert!(validate_text_value(&"a".repeat(501)).is_err());
        assert!(validate_text_value("a\nb").is_err());
    }

    #[test]
    fn date_range_accepts_the_window_and_rejects_outside_it() {
        assert!(validate_date_range(NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()).is_ok());
        assert!(validate_date_range(NaiveDate::from_ymd_opt(2200, 12, 31).unwrap()).is_ok());
        assert!(validate_date_range(NaiveDate::from_ymd_opt(1899, 12, 31).unwrap()).is_err());
        assert!(validate_date_range(NaiveDate::from_ymd_opt(2201, 1, 1).unwrap()).is_err());
    }

    #[test]
    fn custom_field_value_serializes_to_the_spec_wire_shape() {
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(CustomFieldValue::Text("hi".into())).unwrap(),
            json!({"text": "hi"})
        );
        assert_eq!(
            serde_json::to_value(CustomFieldValue::Number("12.5".into())).unwrap(),
            json!({"number": "12.5"})
        );
        assert_eq!(
            serde_json::to_value(CustomFieldValue::Date(
                NaiveDate::from_ymd_opt(2026, 9, 10).unwrap()
            ))
            .unwrap(),
            json!({"date": "2026-09-10"})
        );
        let option_id = CustomFieldOptionId::new(uuid::Uuid::nil());
        assert_eq!(
            serde_json::to_value(CustomFieldValue::Choice(option_id)).unwrap(),
            json!({"option_id": "00000000-0000-0000-0000-000000000000"})
        );
    }

    #[test]
    fn custom_field_value_rejects_zero_two_and_unknown_keys() {
        let zero: Result<CustomFieldValue, _> = serde_json::from_str("{}");
        assert!(zero.is_err());
        let two: Result<CustomFieldValue, _> =
            serde_json::from_str(r#"{"text":"a","number":"1"}"#);
        assert!(two.is_err());
        let unknown: Result<CustomFieldValue, _> = serde_json::from_str(r#"{"bogus":"a"}"#);
        assert!(unknown.is_err());
        let wrong_type: Result<CustomFieldValue, _> = serde_json::from_str(r#"{"number":5}"#);
        assert!(wrong_type.is_err());
    }

    #[test]
    fn custom_field_value_debug_never_prints_the_value() {
        let value = CustomFieldValue::Text("SECRET-VALUE".into());
        let debug = format!("{value:?}");
        assert!(!debug.contains("SECRET-VALUE"));
    }
}

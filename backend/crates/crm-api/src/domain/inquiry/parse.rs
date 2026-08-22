//! `generic_v1` payload parsing (docs/specs/SLICE_002.md §3) and the
//! `source` field validation from the HTTP contract (spec §5).

use serde::Deserialize;

use crate::domain::contact;

const MESSAGE_MAX_BYTES: usize = 4096;

/// A validated intake `source`: lowercased, trimmed, matching
/// `^[a-z0-9_]{1,64}$` (docs/specs/SLICE_002.md §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source(String);

impl Source {
    pub fn parse(raw: &str) -> Option<Source> {
        let trimmed = raw.trim().to_lowercase();
        if trimmed.is_empty() || trimmed.len() > 64 {
            return None;
        }
        if !trimmed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
        Some(Source(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedReason {
    InvalidJson,
    NotAnObject,
    NoContactMethod,
}

impl UnresolvedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UnresolvedReason::InvalidJson => "invalid_json",
            UnresolvedReason::NotAnObject => "not_an_object",
            UnresolvedReason::NoContactMethod => "no_contact_method",
        }
    }
}

#[derive(Deserialize)]
struct RawLead {
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    message: Option<String>,
    external_id: Option<String>,
}

/// A successfully parsed `generic_v1` lead. `email`/`phone` are the
/// normalized forms used for identify and storage; `raw_email`/`raw_phone`
/// are the as-received strings stored in `contact_method.value`.
pub struct ParsedLead {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub raw_email: Option<String>,
    pub raw_phone: Option<String>,
    pub message: Option<String>,
    pub external_id: Option<String>,
}

/// Never derived automatically: must never print plaintext contact
/// information or message content (docs/specs/SLICE_002.md §8).
impl std::fmt::Debug for ParsedLead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedLead")
            .field("has_email", &self.email.is_some())
            .field("has_phone", &self.phone.is_some())
            .finish()
    }
}

/// Truncates `s` to at most `max_bytes` UTF-8 bytes without splitting a
/// multi-byte character.
fn truncate_to_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

/// Parses raw JSON bytes as `generic_v1` (docs/specs/SLICE_002.md §3): not
/// valid JSON -> `invalid_json`; not a JSON object (or a shape that does
/// not match the expected fields) -> `not_an_object`; no normalizable email
/// and no normalizable phone -> `no_contact_method`.
pub fn parse(bytes: &[u8]) -> Result<ParsedLead, UnresolvedReason> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| UnresolvedReason::InvalidJson)?;
    if !value.is_object() {
        return Err(UnresolvedReason::NotAnObject);
    }
    let raw: RawLead = serde_json::from_value(value).map_err(|_| UnresolvedReason::NotAnObject)?;

    let email = raw.email.as_deref().and_then(contact::normalize_email);
    let phone = raw.phone.as_deref().and_then(contact::normalize_phone);
    if email.is_none() && phone.is_none() {
        return Err(UnresolvedReason::NoContactMethod);
    }

    Ok(ParsedLead {
        first_name: non_empty(raw.first_name),
        last_name: non_empty(raw.last_name),
        email,
        phone,
        raw_email: raw.email,
        raw_phone: raw.phone,
        message: raw
            .message
            .map(|m| truncate_to_bytes(&m, MESSAGE_MAX_BYTES)),
        external_id: raw.external_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Source --------------------------------------------------------

    #[test]
    fn source_trims_and_lowercases() {
        let source = Source::parse("  Zillow_Import  ").unwrap();
        assert_eq!(source.as_str(), "zillow_import");
    }

    #[test]
    fn source_rejects_empty() {
        assert!(Source::parse("   ").is_none());
    }

    #[test]
    fn source_rejects_disallowed_characters() {
        assert!(Source::parse("zillow!").is_none());
        assert!(Source::parse("zillow.com").is_none());
        assert!(Source::parse("zillow com").is_none());
    }

    #[test]
    fn source_rejects_over_64_chars() {
        assert!(Source::parse(&"a".repeat(65)).is_none());
    }

    #[test]
    fn source_accepts_64_chars() {
        assert!(Source::parse(&"a".repeat(64)).is_some());
    }

    // --- parse -----------------------------------------------------------

    #[test]
    fn parse_rejects_invalid_json() {
        let err = parse(b"{not valid json").unwrap_err();
        assert_eq!(err, UnresolvedReason::InvalidJson);
    }

    #[test]
    fn parse_rejects_non_object_json() {
        let err = parse(b"[1,2,3]").unwrap_err();
        assert_eq!(err, UnresolvedReason::NotAnObject);
        let err = parse(b"\"just a string\"").unwrap_err();
        assert_eq!(err, UnresolvedReason::NotAnObject);
    }

    #[test]
    fn parse_rejects_no_contact_method() {
        let err = parse(br#"{"first_name":"Ada"}"#).unwrap_err();
        assert_eq!(err, UnresolvedReason::NoContactMethod);
    }

    #[test]
    fn parse_rejects_unnormalizable_contact_methods() {
        let err = parse(br#"{"email":"not-an-email","phone":"555"}"#).unwrap_err();
        assert_eq!(err, UnresolvedReason::NoContactMethod);
    }

    #[test]
    fn parse_accepts_email_only() {
        let lead = parse(br#"{"email":"Ada@Example.com"}"#).unwrap();
        assert_eq!(lead.email.as_deref(), Some("ada@example.com"));
        assert_eq!(lead.phone, None);
        assert_eq!(lead.raw_email.as_deref(), Some("Ada@Example.com"));
    }

    #[test]
    fn parse_accepts_phone_only() {
        let lead = parse(br#"{"phone":"(555) 555-0100"}"#).unwrap();
        assert_eq!(lead.phone.as_deref(), Some("+15555550100"));
        assert_eq!(lead.email, None);
    }

    #[test]
    fn parse_truncates_message_to_4kib() {
        let long_message = "x".repeat(5000);
        let body = serde_json::json!({ "email": "ada@example.com", "message": long_message });
        let lead = parse(body.to_string().as_bytes()).unwrap();
        assert_eq!(lead.message.unwrap().len(), MESSAGE_MAX_BYTES);
    }

    #[test]
    fn parse_leaves_short_message_untouched() {
        let body = serde_json::json!({ "email": "ada@example.com", "message": "hello" });
        let lead = parse(body.to_string().as_bytes()).unwrap();
        assert_eq!(lead.message.as_deref(), Some("hello"));
    }

    #[test]
    fn parse_treats_blank_names_as_absent() {
        let body = serde_json::json!({ "email": "ada@example.com", "first_name": "   " });
        let lead = parse(body.to_string().as_bytes()).unwrap();
        assert_eq!(lead.first_name, None);
    }

    // --- Debug redaction ---------------------------------------------------

    #[test]
    fn parsed_lead_debug_never_prints_contact_info() {
        let lead = parse(br#"{"email":"ada@example.com","phone":"555-555-0100","message":"call me at ada@example.com"}"#)
            .unwrap();
        let debug_output = format!("{lead:?}");
        assert!(!debug_output.contains("ada@example.com"));
        assert!(!debug_output.contains("555-555-0100"));
        assert!(!debug_output.contains("call me"));
        assert!(debug_output.contains("has_email: true"));
        assert!(debug_output.contains("has_phone: true"));
    }
}

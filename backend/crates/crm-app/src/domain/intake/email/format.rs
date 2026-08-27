//! The pinned-format registry (docs/specs/SLICE_007d.md §4b): a static
//! slice of `EmailFormat`s tried in declaration order, first match wins.
//! `matches()` must key on the format's real sender domain, never content
//! alone — the D-036 forgery mitigation.

use crate::domain::contact;
use crate::domain::inquiry::parse::{self, ParsedLead, Source, UnresolvedReason};
use crate::domain::intake::email::formats::cypress_bay::CypressBayContact;
use crate::domain::intake::email::forward::SenderTrust;
use crate::domain::intake::email::mime::ParsedMail;

/// A pinned format's best-effort field scan. Deliberately infallible
/// (docs/specs/SLICE_007d.md §4b): missing fields are `None`; the only
/// downstream failure is "no normalizable contact method"
/// (`no_contact_method`), decided by [`to_parsed_lead`].
pub struct ExtractedLead {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub message: Option<String>,
    /// The detected inquiry source (e.g. `"website"`); `inquiry.source`
    /// takes this while `raw_payload.source` stays `"email"` (ladder
    /// cross-rung row 8–13).
    pub source: &'static str,
}

pub trait EmailFormat: Send + Sync {
    /// Static, span-safe identifier (e.g. `"cypress_bay_contact_v1"`) —
    /// the only format-derived value observability may record
    /// (docs/specs/SLICE_007d.md §8).
    fn name(&self) -> &'static str;
    /// Sender-domain restriction AND template marker, both required
    /// (D-036). Never content alone. The message as delivered — when a
    /// later rung extracts Authentication-Results verdicts (carried on
    /// `SenderTrust::Direct`), tightening happens on this arm.
    fn matches_direct(&self, mail: &ParsedMail) -> bool;
    /// The same question for an unwrapped forwarded view, whose
    /// From/Subject are quoted text under the forwarder's control
    /// (D-040). A REQUIRED method — every format, present and future,
    /// answers the forwarded arm explicitly; matching here is never
    /// inherited from `matches_direct`, and never tightened implicitly
    /// when the direct arm later consumes verdicts
    /// (docs/specs/SLICE_007h1.md §3).
    fn matches_forwarded(&self, mail: &ParsedMail, depth: u8) -> bool;
    fn extract(&self, mail: &ParsedMail) -> ExtractedLead;
}

/// The registry: declaration order, first match wins. 007h appends here.
static FORMATS: &[&dyn EmailFormat] = &[&CypressBayContact];

pub fn detect(mail: &ParsedMail, trust: SenderTrust) -> Option<&'static dyn EmailFormat> {
    FORMATS
        .iter()
        .find(|f| match trust {
            SenderTrust::Direct => f.matches_direct(mail),
            SenderTrust::ForwardedClaim { depth } => f.matches_forwarded(mail, depth),
        })
        .copied()
}

/// `ExtractedLead → ParsedLead`, mirroring the `generic_v1` normalization
/// (docs/specs/SLICE_007d.md §4b): `normalize_email`/`normalize_phone`
/// with raw values preserved, message truncated at the shared
/// `MESSAGE_MAX_BYTES`, no external id this rung (§12b). No normalizable
/// contact method → the existing `no_contact_method` reason.
pub fn to_parsed_lead(extracted: ExtractedLead) -> Result<(Source, ParsedLead), UnresolvedReason> {
    // The source is a static string every registered format owns; a
    // format shipping an invalid one is a programming error caught by the
    // registry test below, but a read path must not panic — fail closed
    // as unrecognized.
    let source =
        Source::parse(extracted.source).ok_or(UnresolvedReason::EmailUnrecognizedFormat)?;

    let email = extracted
        .email
        .as_deref()
        .and_then(contact::normalize_email);
    let phone = extracted
        .phone
        .as_deref()
        .and_then(contact::normalize_phone);
    if email.is_none() && phone.is_none() {
        return Err(UnresolvedReason::NoContactMethod);
    }

    let lead = ParsedLead {
        first_name: extracted.first_name.filter(|s| !s.trim().is_empty()),
        last_name: extracted.last_name.filter(|s| !s.trim().is_empty()),
        email: email.map(|e| e.to_string()),
        phone: phone.map(|p| p.to_string()),
        raw_email: extracted.email,
        raw_phone: extracted.phone,
        message: extracted
            .message
            .map(|m| parse::truncate_to_bytes(&m, parse::MESSAGE_MAX_BYTES)),
        external_id: None,
    };
    Ok((source, lead))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered format must declare a `source` the shared
    /// validator accepts — otherwise `to_parsed_lead`'s fail-closed
    /// branch would silently turn its every extraction into
    /// `email_unrecognized_format`.
    #[test]
    fn every_registered_format_has_a_valid_source_and_unique_name() {
        let empty = ParsedMail {
            from_addr: None,
            from_display: None,
            subject: None,
            date: None,
            text_body: None,
        };
        let mut names = std::collections::HashSet::new();
        for format in FORMATS {
            assert!(names.insert(format.name()), "duplicate {}", format.name());
            let lead = format.extract(&empty);
            assert!(
                Source::parse(lead.source).is_some(),
                "{} declares an invalid source {:?} — its every extraction \
                 would silently fail closed as email_unrecognized_format",
                format.name(),
                lead.source
            );
        }
    }

    #[test]
    fn to_parsed_lead_normalizes_and_preserves_raw_values() {
        let (source, lead) = to_parsed_lead(ExtractedLead {
            first_name: Some("Jordan".into()),
            last_name: Some("Ellis".into()),
            email: Some("Jordan.Ellis@Example.com".into()),
            phone: Some("(555) 555-0142".into()),
            message: Some("hello".into()),
            source: "website",
        })
        .expect("valid contact");
        assert_eq!(source.as_str(), "website");
        assert_eq!(lead.email.as_deref(), Some("jordan.ellis@example.com"));
        assert_eq!(lead.phone.as_deref(), Some("+15555550142"));
        assert_eq!(lead.raw_email.as_deref(), Some("Jordan.Ellis@Example.com"));
        assert_eq!(lead.raw_phone.as_deref(), Some("(555) 555-0142"));
        assert_eq!(lead.external_id, None);
    }

    #[test]
    fn to_parsed_lead_without_normalizable_contact_is_no_contact_method() {
        let err = to_parsed_lead(ExtractedLead {
            first_name: Some("Jordan".into()),
            last_name: None,
            email: Some("not-an-email".into()),
            phone: Some("555".into()),
            message: None,
            source: "website",
        })
        .unwrap_err();
        assert_eq!(err, UnresolvedReason::NoContactMethod);
    }

    #[test]
    fn to_parsed_lead_truncates_message_like_generic_v1() {
        let (_, lead) = to_parsed_lead(ExtractedLead {
            first_name: None,
            last_name: None,
            email: Some("a@example.com".into()),
            phone: None,
            message: Some("x".repeat(5000)),
            source: "website",
        })
        .unwrap();
        assert_eq!(lead.message.unwrap().len(), parse::MESSAGE_MAX_BYTES);
    }
}

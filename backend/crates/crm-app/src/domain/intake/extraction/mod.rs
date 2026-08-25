//! LLM lead extraction (docs/specs/SLICE_007f.md §4): the `LeadExtractor`
//! seam and ALL of its semantics — the strict schema, the confidence
//! gate, anti-hallucination matching, and the normalization funnel — live
//! here in the domain crate. The adapter (crm-api) only builds a prompt
//! and moves bytes; it can never change what counts as a valid lead.

pub mod worker;

use async_trait::async_trait;

use crate::domain::contact;
use crate::domain::inquiry::parse::{self, ParsedLead, Source};

/// D-038's input scope, enforced by what this struct can carry: subject,
/// the sender's domain, and capped body text — never the full sender
/// address, the recipient/intake address, the Organization's name, or
/// any agent identifier.
pub struct ExtractionInput {
    /// Capped at [`SUBJECT_MAX_BYTES`].
    pub subject: Option<String>,
    /// Domain only, never the full address.
    pub sender_domain: Option<String>,
    /// Body text, truncated so the total input stays ≤ [`INPUT_MAX_BYTES`].
    pub text: String,
    pub truncated: bool,
}

/// Never derived: this is lead content (docs/specs/SLICE_007f.md §8).
impl std::fmt::Debug for ExtractionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractionInput")
            .field("has_subject", &self.subject.is_some())
            .field("has_sender_domain", &self.sender_domain.is_some())
            .field("text_len", &self.text.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// The raw, unvalidated model output.
pub struct ExtractorReply {
    /// Never logged.
    pub content: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

impl std::fmt::Debug for ExtractorReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractorReply")
            .field("content_len", &self.content.len())
            .finish()
    }
}

/// Transport-class failures — mirrors the provider's error shape without
/// naming crm-operator (D-034). None of these count toward the quality
/// cap or ever go terminal (docs/specs/SLICE_007f.md §4a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractorError {
    Timeout,
    Unavailable,
    RateLimited,
    /// The reply could not be read at the transport level (oversize,
    /// wire-shape) — distinct from a readable reply that fails the
    /// schema, which is a quality failure.
    Malformed,
}

#[async_trait]
pub trait LeadExtractor: Send + Sync {
    async fn extract(&self, input: &ExtractionInput) -> Result<ExtractorReply, ExtractorError>;
    /// The ledger's `provider` column (`'groq'` | `'fake'`).
    fn provider(&self) -> &'static str;
    /// The ledger's `model` column.
    fn model(&self) -> &str;
}

/// Total input budget (docs/specs/SLICE_007f.md §5; the D-038 cap).
pub const INPUT_MAX_BYTES: usize = 16 * 1024;
/// Subject cap within the budget.
pub const SUBJECT_MAX_BYTES: usize = 256;
/// The confidence gate, inclusive (spec §4c).
pub const CONFIDENCE_FLOOR: f32 = 0.7;
/// `normalize_phone`'s floor, restated here because the anti-hallucination
/// matcher runs before normalization (spec §4c).
pub const PHONE_MIN_DIGITS: usize = 10;

/// The strict reply schema. Unknown fields are tolerated; wrong types
/// fail the parse (`schema_invalid`).
#[derive(serde::Deserialize)]
pub struct LeadClaim {
    pub is_lead: bool,
    pub confidence: f32,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

impl std::fmt::Debug for LeadClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeadClaim")
            .field("is_lead", &self.is_lead)
            .field("confidence", &self.confidence)
            .field("has_email", &self.email.is_some())
            .field("has_phone", &self.phone.is_some())
            .finish()
    }
}

/// A claim that survived every gate. Owned strings so the completion
/// closure can construct a fresh `ParsedLead` on every invocation
/// (`complete_intake`'s lock-retry loop may call it more than once and
/// `ParsedLead` is deliberately not `Clone` — spec §4b).
#[derive(Clone)]
pub struct ValidatedLead {
    first_name: Option<String>,
    last_name: Option<String>,
    email_raw: Option<String>,
    phone_raw: Option<String>,
    message: Option<String>,
}

impl ValidatedLead {
    /// Builds the `(Source, ParsedLead)` the shared `complete_intake`
    /// closure returns — per invocation, from owned fields, mirroring
    /// `format::to_parsed_lead`'s normalization funnel.
    pub fn to_parsed(&self) -> (Source, ParsedLead) {
        let source = Source::parse("email").expect("'email' is a valid source");
        let email = self.email_raw.as_deref().and_then(contact::normalize_email);
        let phone = self.phone_raw.as_deref().and_then(contact::normalize_phone);
        let lead = ParsedLead {
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            email,
            phone,
            raw_email: self.email_raw.clone(),
            raw_phone: self.phone_raw.clone(),
            message: self
                .message
                .as_deref()
                .map(|m| parse::truncate_to_bytes(m, parse::MESSAGE_MAX_BYTES)),
            external_id: None,
        };
        (source, lead)
    }
}

/// Quality-class validation outcomes (ledger tags; spec §4a). Each counts
/// toward the 3-attempt cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityFailure {
    SchemaInvalid,
    LowConfidence,
    HallucinatedContact,
    NoContactMethod,
}

impl QualityFailure {
    pub fn ledger_tag(self) -> &'static str {
        match self {
            QualityFailure::SchemaInvalid => "schema_invalid",
            QualityFailure::LowConfidence => "low_confidence",
            QualityFailure::HallucinatedContact => "hallucinated_contact",
            QualityFailure::NoContactMethod => "no_contact_method",
        }
    }
}

/// The verdict on one model reply.
pub enum ClaimVerdict {
    /// A lead, gated and validated — ready for `complete_intake`.
    Lead {
        validated: ValidatedLead,
        confidence: f32,
    },
    /// Confidently not a lead — terminal `not_a_lead`.
    NotALead { confidence: f32 },
    /// A quality failure; `confidence` when it parsed in-range.
    Failed {
        failure: QualityFailure,
        confidence: Option<f32>,
    },
}

/// The whole gauntlet (spec §4c): schema → confidence → is_lead →
/// anti-hallucination → normalization. Pure, so every branch is
/// unit-testable with no worker or provider.
pub fn validate_reply(input: &ExtractionInput, reply_content: &str) -> ClaimVerdict {
    let claim: LeadClaim = match serde_json::from_str(reply_content) {
        Ok(claim) => claim,
        Err(_) => {
            return ClaimVerdict::Failed {
                failure: QualityFailure::SchemaInvalid,
                confidence: None,
            }
        }
    };

    // Out-of-range confidence is a schema violation, with the ledger's
    // confidence left NULL so its CHECK can never reject the insert.
    if !claim.confidence.is_finite() || !(0.0..=1.0).contains(&claim.confidence) {
        return ClaimVerdict::Failed {
            failure: QualityFailure::SchemaInvalid,
            confidence: None,
        };
    }

    // The gate is inclusive at 0.7 and applies to BOTH verdicts: an
    // unconfident "not a lead" is not trusted either (spec §4c).
    if claim.confidence < CONFIDENCE_FLOOR {
        return ClaimVerdict::Failed {
            failure: QualityFailure::LowConfidence,
            confidence: Some(claim.confidence),
        };
    }

    if !claim.is_lead {
        return ClaimVerdict::NotALead {
            confidence: claim.confidence,
        };
    }

    // Anti-hallucination: extracted contacts must appear, normalized, in
    // what the model was shown (subject + text). A violation fails the
    // whole attempt — never silently drops a field (spec safe default i).
    let haystack_lower = {
        let mut h = String::new();
        if let Some(subject) = &input.subject {
            h.push_str(&subject.to_lowercase());
            h.push('\n');
        }
        h.push_str(&input.text.to_lowercase());
        h
    };
    let email = claim
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    if let Some(email) = email {
        if !haystack_lower.contains(&email.to_lowercase()) {
            return ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                confidence: Some(claim.confidence),
            };
        }
    }
    let phone = claim
        .phone
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    if let Some(phone) = phone {
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        // ≥10 digits (normalize_phone's floor), and the digit sequence
        // must appear in the input with only phone-typical separators
        // stripped — NOT all non-digits, which would let a number be
        // synthesized across unrelated digit runs (spec §4c).
        if digits.len() < PHONE_MIN_DIGITS
            || !strip_phone_separators(&haystack_lower).contains(&digits)
        {
            return ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                confidence: Some(claim.confidence),
            };
        }
    }

    // The existing normalization funnel decides usability.
    let normalized_email = email.and_then(contact::normalize_email);
    let normalized_phone = phone.and_then(contact::normalize_phone);
    if normalized_email.is_none() && normalized_phone.is_none() {
        return ClaimVerdict::Failed {
            failure: QualityFailure::NoContactMethod,
            confidence: Some(claim.confidence),
        };
    }

    // The model's reply is untrusted: a reply echoing NUL (or other
    // control bytes) into a name/message would hit Postgres 22021 inside
    // complete_intake and loop (adversarial finding C1, the 007d NUL
    // lesson applied to the OUTPUT side). Sanitize here — names lose all
    // control chars, the message keeps newlines.
    let sanitize = |v: &Option<String>, keep_newline: bool| {
        v.as_deref().map(str::trim).and_then(|raw| {
            let cleaned: String = raw
                .chars()
                .filter(|c| (keep_newline && *c == '\n') || !c.is_control())
                .collect();
            let trimmed = cleaned.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    };
    ClaimVerdict::Lead {
        validated: ValidatedLead {
            first_name: sanitize(&claim.first_name, false),
            last_name: sanitize(&claim.last_name, false),
            email_raw: email.map(str::to_string),
            phone_raw: phone.map(str::to_string),
            message: sanitize(&claim.message, true),
        },
        confidence: claim.confidence,
    }
}

/// Removes only phone-typical separators: space, `-`, `.`, `(`, `)`, `+`.
fn strip_phone_separators(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')' | '+'))
        .collect()
}

/// Builds the D-038-scoped input from a parsed mail: subject (capped),
/// sender domain only, body truncated to the total budget.
pub fn build_input(mail: &crate::domain::intake::email::ParsedMail) -> ExtractionInput {
    let mut truncated = false;
    let subject = mail.subject.as_deref().map(|s| {
        if s.len() > SUBJECT_MAX_BYTES {
            truncated = true;
            parse::truncate_to_bytes(s, SUBJECT_MAX_BYTES)
        } else {
            s.to_string()
        }
    });
    let sender_domain = mail
        .from_addr
        .as_deref()
        .and_then(|addr| addr.rsplit_once('@'))
        .map(|(_, domain)| domain.to_string());

    let subject_len = subject.as_deref().map_or(0, str::len);
    let text_budget = INPUT_MAX_BYTES.saturating_sub(subject_len);
    let body = mail.text_body.as_deref().unwrap_or("");
    let text = if body.len() > text_budget {
        truncated = true;
        parse::truncate_to_bytes(body, text_budget)
    } else {
        body.to_string()
    };

    ExtractionInput {
        subject,
        sender_domain,
        text,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(subject: Option<&str>, text: &str) -> ExtractionInput {
        ExtractionInput {
            subject: subject.map(str::to_string),
            sender_domain: Some("eospia.com".into()),
            text: text.into(),
            truncated: false,
        }
    }

    const LEAD_TEXT: &str =
        "New inquiry from Jordan Ellis. Reach them at Jordan.Ellis@Example.com or (555) 555-0142.";

    fn reply(json: serde_json::Value) -> String {
        json.to_string()
    }

    #[test]
    fn a_valid_lead_passes_and_normalizes() {
        let verdict = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "first_name": "Jordan", "last_name": "Ellis",
                "email": "jordan.ellis@example.com",
                "phone": "555-555-0142",
                "message": "New inquiry"
            })),
        );
        let ClaimVerdict::Lead {
            validated,
            confidence,
        } = verdict
        else {
            panic!("expected Lead");
        };
        assert_eq!(confidence, 0.9);
        let (source, lead) = validated.to_parsed();
        assert_eq!(source.as_str(), "email");
        assert_eq!(lead.email.as_deref(), Some("jordan.ellis@example.com"));
        assert_eq!(lead.phone.as_deref(), Some("+15555550142"));
        // The closure runs per lock-retry iteration: to_parsed must work
        // repeatedly.
        let (_, lead2) = validated.to_parsed();
        assert_eq!(lead2.email.as_deref(), Some("jordan.ellis@example.com"));
    }

    #[test]
    fn schema_violations_fail_closed() {
        for bad in [
            "not json at all",
            r#"{"is_lead": "yes", "confidence": 0.9}"#, // wrong type
            r#"{"confidence": 0.9}"#,                   // missing is_lead
        ] {
            let verdict = validate_reply(&input(None, LEAD_TEXT), bad);
            assert!(matches!(
                verdict,
                ClaimVerdict::Failed {
                    failure: QualityFailure::SchemaInvalid,
                    confidence: None
                }
            ));
        }
    }

    #[test]
    fn out_of_range_confidence_is_schema_invalid_with_null_confidence() {
        for c in ["1.5", "-0.1", "null"] {
            let verdict = validate_reply(
                &input(None, LEAD_TEXT),
                &format!(r#"{{"is_lead": true, "confidence": {c}}}"#),
            );
            assert!(matches!(
                verdict,
                ClaimVerdict::Failed {
                    failure: QualityFailure::SchemaInvalid,
                    confidence: None
                }
            ));
        }
    }

    #[test]
    fn the_gate_is_inclusive_at_point_seven_for_both_verdicts() {
        let below = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.69,
                "email": "jordan.ellis@example.com"
            })),
        );
        assert!(matches!(
            below,
            ClaimVerdict::Failed {
                failure: QualityFailure::LowConfidence,
                ..
            }
        ));
        // An unconfident "not a lead" is not trusted either.
        let unconfident_spam = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({ "is_lead": false, "confidence": 0.5 })),
        );
        assert!(matches!(
            unconfident_spam,
            ClaimVerdict::Failed {
                failure: QualityFailure::LowConfidence,
                ..
            }
        ));
        let at = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.7,
                "email": "jordan.ellis@example.com"
            })),
        );
        assert!(matches!(at, ClaimVerdict::Lead { .. }));
        let confident_spam = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({ "is_lead": false, "confidence": 0.95 })),
        );
        assert!(matches!(confident_spam, ClaimVerdict::NotALead { .. }));
    }

    #[test]
    fn hallucinated_contacts_fail_the_whole_attempt() {
        // An email that never appears in the input.
        let verdict = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "invented@example.com"
            })),
        );
        assert!(matches!(
            verdict,
            ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                ..
            }
        ));
        // A phone that never appears — even alongside an honest email:
        // never silently dropped.
        let verdict = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "jordan.ellis@example.com",
                "phone": "999-888-7777"
            })),
        );
        assert!(matches!(
            verdict,
            ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                ..
            }
        ));
    }

    #[test]
    fn honest_reformatting_passes_the_matcher() {
        // Input has "(555) 555-0142"; the model answers "+1 555.555.0142"
        // — same digit sequence modulo phone separators... note the
        // leading 1 is NOT in the input, so use the exact digits.
        let verdict = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "JORDAN.ELLIS@example.com",   // case differs
                "phone": "555.555.0142"                 // separators differ
            })),
        );
        assert!(matches!(verdict, ClaimVerdict::Lead { .. }));
        // Subject participates in the haystack.
        let verdict = validate_reply(
            &input(Some("call carol@example.com"), "no contacts in the body"),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "carol@example.com"
            })),
        );
        assert!(matches!(verdict, ClaimVerdict::Lead { .. }));
    }

    #[test]
    fn short_phones_and_cross_run_synthesis_are_rejected() {
        // <10 digits fails even if the digits appear.
        let verdict = validate_reply(
            &input(None, "code 12345 and 6789"),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": null, "phone": "12345"
            })),
        );
        assert!(matches!(
            verdict,
            ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                ..
            }
        ));
        // Digits separated by WORDS in the input cannot be synthesized
        // into one number (only phone separators are stripped).
        let verdict = validate_reply(
            &input(None, "order 55555 shipped, invoice 50142 due"),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": null, "phone": "5555550142"
            })),
        );
        assert!(matches!(
            verdict,
            ClaimVerdict::Failed {
                failure: QualityFailure::HallucinatedContact,
                ..
            }
        ));
    }

    #[test]
    fn control_bytes_in_reply_fields_are_stripped_never_stored() {
        // Adversarial finding C1: a model echoing NUL into a name or
        // message must not be able to poison the Postgres insert.
        let verdict = validate_reply(
            &input(None, LEAD_TEXT),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "jordan.ellis@example.com",
                "first_name": "Jor\u{0000}dan",
                "last_name": "El\u{0007}lis",
                "message": "line one\u{0000}\nline two"
            })),
        );
        let ClaimVerdict::Lead { validated, .. } = verdict else {
            panic!("expected Lead");
        };
        let (_, lead) = validated.to_parsed();
        assert_eq!(lead.first_name.as_deref(), Some("Jordan"));
        assert_eq!(lead.last_name.as_deref(), Some("Ellis"));
        assert_eq!(lead.message.as_deref(), Some("line one\nline two"));
        for field in [&lead.first_name, &lead.last_name, &lead.message] {
            assert!(!field.as_deref().unwrap_or("").contains('\u{0000}'));
        }
    }

    #[test]
    fn no_normalizable_contact_is_a_quality_failure() {
        let verdict = validate_reply(
            &input(None, "reach me at not-an-email or 555"),
            &reply(serde_json::json!({
                "is_lead": true, "confidence": 0.9,
                "email": "not-an-email"
            })),
        );
        assert!(matches!(
            verdict,
            ClaimVerdict::Failed {
                failure: QualityFailure::NoContactMethod,
                ..
            }
        ));
    }

    #[test]
    fn extractor_reply_debug_never_prints_content() {
        let reply = ExtractorReply {
            content: "SECRET reply text".into(),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
        };
        let debug = format!("{reply:?}");
        assert!(!debug.contains("SECRET"));
        assert!(debug.contains("content_len"));
    }

    #[test]
    fn subject_longer_than_the_cap_truncates_and_flags() {
        use crate::domain::intake::email::ParsedMail;
        let mail = ParsedMail {
            from_addr: Some("a@eospia.com".into()),
            from_display: None,
            subject: Some("s".repeat(SUBJECT_MAX_BYTES * 2)),
            date: None,
            text_body: Some("short body".into()),
        };
        let input = build_input(&mail);
        assert!(input.truncated);
        assert_eq!(
            input.subject.as_deref().map(str::len),
            Some(SUBJECT_MAX_BYTES)
        );
    }

    #[test]
    fn build_input_scopes_and_truncates() {
        use crate::domain::intake::email::ParsedMail;
        let mail = ParsedMail {
            from_addr: Some("noreply@leads.eospia.com".into()),
            from_display: Some("Eospia Leads".into()),
            subject: Some("New lead".into()),
            date: None,
            text_body: Some("x".repeat(INPUT_MAX_BYTES * 2)),
        };
        let input = build_input(&mail);
        assert_eq!(input.sender_domain.as_deref(), Some("leads.eospia.com"));
        assert_eq!(input.subject.as_deref(), Some("New lead"));
        assert!(input.truncated);
        assert!(input.subject.as_deref().map_or(0, str::len) + input.text.len() <= INPUT_MAX_BYTES);
        // Nothing beyond the three fields exists to leak: the struct has
        // no other content-bearing members (D-038 scope by construction).
        let debug = format!("{input:?}");
        assert!(!debug.contains("eospia"));
        assert!(!debug.contains("New lead"));
    }
}

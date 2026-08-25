//! The cypressbayrealty.com contact-form template — the first pinned
//! format (docs/specs/SLICE_007d.md §4b). We author the form, so this
//! module IS the template's definition: From
//! `"Cypress Bay Realty" <forms@cypressbayrealty.com>`, Subject
//! `New contact form submission`, plain-text body of labeled lines
//! `Name:` / `Email:` / `Phone:` / `Message:` (Message consumes the
//! remainder, multi-line).

use crate::domain::intake::email::format::{EmailFormat, ExtractedLead};
use crate::domain::intake::email::mime::ParsedMail;

const SENDER_DOMAIN: &str = "cypressbayrealty.com";
const SUBJECT_MARKER: &str = "New contact form submission";

pub struct CypressBayContact;

impl EmailFormat for CypressBayContact {
    fn name(&self) -> &'static str {
        "cypress_bay_contact_v1"
    }

    /// Sender **domain** equality (case-insensitive; we control the form
    /// but the mailer's local part may vary) AND the subject equals the
    /// marker after trim, exact case — both required (D-036). A
    /// "Fwd: "-prefixed subject deliberately does not match: forwarded
    /// copies of form mail are not the pinned flow
    /// (docs/specs/SLICE_007d.md §4b).
    fn matches(&self, mail: &ParsedMail) -> bool {
        let domain_ok = mail
            .from_addr
            .as_deref()
            .and_then(|addr| addr.rsplit_once('@'))
            // from_addr is already lowercased by the mime wrapper.
            .is_some_and(|(_, domain)| domain == SENDER_DOMAIN);
        let subject_ok = mail
            .subject
            .as_deref()
            .is_some_and(|s| s.trim() == SUBJECT_MARKER);
        domain_ok && subject_ok
    }

    /// Line-anchored, exact-case labels; values trimmed; `Message:`
    /// consumes every remaining line (docs/specs/SLICE_007d.md §4b).
    /// Best-effort by design: missing fields are `None`.
    fn extract(&self, mail: &ParsedMail) -> ExtractedLead {
        let mut name: Option<String> = None;
        let mut email: Option<String> = None;
        let mut phone: Option<String> = None;
        let mut message: Option<String> = None;

        if let Some(body) = mail.text_body.as_deref() {
            let mut lines = body.lines();
            while let Some(line) = lines.next() {
                if let Some(value) = line.strip_prefix("Name:") {
                    name = non_empty(value);
                } else if let Some(value) = line.strip_prefix("Email:") {
                    email = non_empty(value);
                } else if let Some(value) = line.strip_prefix("Phone:") {
                    phone = non_empty(value);
                } else if let Some(value) = line.strip_prefix("Message:") {
                    let mut collected = value.trim().to_string();
                    for rest in lines.by_ref() {
                        collected.push('\n');
                        collected.push_str(rest);
                    }
                    let collected = collected.trim().to_string();
                    message = Some(collected).filter(|m| !m.is_empty());
                }
            }
        }

        // Name splits on first whitespace; a single word is first_name
        // only (docs/specs/SLICE_007d.md §4b).
        let (first_name, last_name) = match name {
            Some(full) => match full.split_once(char::is_whitespace) {
                Some((first, rest)) => (
                    Some(first.to_string()),
                    Some(rest.trim().to_string()).filter(|s| !s.is_empty()),
                ),
                None => (Some(full), None),
            },
            None => (None, None),
        };

        ExtractedLead {
            first_name,
            last_name,
            email,
            phone,
            message,
            source: "website",
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(from: Option<&str>, subject: Option<&str>, body: Option<&str>) -> ParsedMail {
        ParsedMail {
            from_addr: from.map(|s| s.to_lowercase()),
            from_display: None,
            subject: subject.map(str::to_string),
            date: None,
            text_body: body.map(str::to_string),
        }
    }

    const BODY: &str = "Name: Jordan Ellis\nEmail: jordan.ellis@example.com\nPhone: (555) 555-0142\nMessage: Interested in the listing.\nIs it still available?";

    // --- matches() matrix (docs/specs/SLICE_007d.md §10) ---------------

    #[test]
    fn matches_right_domain_and_subject() {
        let m = mail(
            Some("forms@cypressbayrealty.com"),
            Some("New contact form submission"),
            Some(BODY),
        );
        assert!(CypressBayContact.matches(&m));
    }

    #[test]
    fn matches_is_domain_not_full_address_and_case_insensitive() {
        // The mime wrapper lowercases; a different local part still
        // matches (we control the form, the mailer may vary).
        let m = mail(
            Some("No-Reply@CypressBayRealty.com"),
            Some("New contact form submission"),
            Some(BODY),
        );
        assert!(CypressBayContact.matches(&m));
    }

    #[test]
    fn right_subject_wrong_domain_does_not_match() {
        // D-036: the forgery mitigation — content alone never matches.
        let m = mail(
            Some("forms@eospia.com"),
            Some("New contact form submission"),
            Some(BODY),
        );
        assert!(!CypressBayContact.matches(&m));
        // A lookalike domain that merely *contains* the real one.
        let m = mail(
            Some("forms@notcypressbayrealty.com"),
            Some("New contact form submission"),
            Some(BODY),
        );
        assert!(!CypressBayContact.matches(&m));
    }

    #[test]
    fn right_domain_wrong_subject_does_not_match() {
        let m = mail(
            Some("forms@cypressbayrealty.com"),
            Some("Monthly newsletter"),
            Some(BODY),
        );
        assert!(!CypressBayContact.matches(&m));
        // Exact equality after trim: a forwarded prefix fails by design.
        let m = mail(
            Some("forms@cypressbayrealty.com"),
            Some("Fwd: New contact form submission"),
            Some(BODY),
        );
        assert!(!CypressBayContact.matches(&m));
        let m = mail(Some("forms@cypressbayrealty.com"), None, Some(BODY));
        assert!(!CypressBayContact.matches(&m));
    }

    #[test]
    fn subject_is_trimmed_before_comparison() {
        let m = mail(
            Some("forms@cypressbayrealty.com"),
            Some("  New contact form submission  "),
            Some(BODY),
        );
        assert!(CypressBayContact.matches(&m));
    }

    // --- extract() -----------------------------------------------------

    #[test]
    fn extracts_all_fields_with_multiline_message() {
        let m = mail(None, None, Some(BODY));
        let lead = CypressBayContact.extract(&m);
        assert_eq!(lead.first_name.as_deref(), Some("Jordan"));
        assert_eq!(lead.last_name.as_deref(), Some("Ellis"));
        assert_eq!(lead.email.as_deref(), Some("jordan.ellis@example.com"));
        assert_eq!(lead.phone.as_deref(), Some("(555) 555-0142"));
        assert_eq!(
            lead.message.as_deref(),
            Some("Interested in the listing.\nIs it still available?")
        );
        assert_eq!(lead.source, "website");
    }

    #[test]
    fn missing_fields_are_none_and_single_word_name_is_first_name_only() {
        let m = mail(None, None, Some("Name: Cher\nEmail: cher@example.com"));
        let lead = CypressBayContact.extract(&m);
        assert_eq!(lead.first_name.as_deref(), Some("Cher"));
        assert_eq!(lead.last_name, None);
        assert_eq!(lead.phone, None);
        assert_eq!(lead.message, None);
    }

    #[test]
    fn labels_are_line_anchored_and_exact_case() {
        // "name:" (wrong case) and an indented label are body text, not
        // fields.
        let m = mail(
            None,
            None,
            Some("name: nope\n  Email: nope@example.com\nEmail: real@example.com"),
        );
        let lead = CypressBayContact.extract(&m);
        assert_eq!(lead.first_name, None);
        assert_eq!(lead.email.as_deref(), Some("real@example.com"));
    }

    #[test]
    fn empty_body_extracts_nothing() {
        let m = mail(None, None, None);
        let lead = CypressBayContact.extract(&m);
        assert_eq!(lead.first_name, None);
        assert_eq!(lead.email, None);
        assert_eq!(lead.phone, None);
        assert_eq!(lead.message, None);
    }
}

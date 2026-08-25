//! The MIME boundary (docs/specs/SLICE_007d.md §4a): the only module in
//! the workspace allowed to `use mail_parser`. Everything past this
//! signature sees owned std/chrono types only.

use chrono::{DateTime, Utc};
use mail_parser::MessageParser;

/// The minimal view of an email this rung needs. Owned types only — no
/// `mail_parser` type may leak past this struct.
pub struct ParsedMail {
    /// First `From` mailbox address, lowercased. The value format
    /// matchers key their sender-domain restriction on.
    pub from_addr: Option<String>,
    pub from_display: Option<String>,
    pub subject: Option<String>,
    /// The message's own `Date` header. Informational only: `received_at`
    /// is always receipt time (docs/specs/SLICE_007d.md §4a; ladder
    /// cross-rung row 8–13), never this.
    pub date: Option<DateTime<Utc>>,
    /// `text/plain` body preferred; mail-parser's HTML→text conversion as
    /// the fallback for HTML-only mail.
    pub text_body: Option<String>,
}

/// Never derived: subject, sender, and body are lead content and must not
/// reach logs via an accidental `{:?}` (docs/specs/SLICE_007d.md §8).
impl std::fmt::Debug for ParsedMail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedMail")
            .field("has_from", &self.from_addr.is_some())
            .field("has_subject", &self.subject.is_some())
            .field("has_body", &self.text_body.is_some())
            .finish()
    }
}

/// `None` = `email_unparsed`. mail-parser is extremely lenient
/// (`MessageParser::parse` returns `Some` for almost any bytes — it
/// treats the first line of even binary garbage as a header), so the
/// operational definition of "unparseable" is explicit here: the parser
/// returned `None`, or nothing email-shaped could be extracted at all —
/// no From address, no Subject, and no body text
/// (docs/specs/SLICE_007d.md §4a).
pub fn parse(raw: &[u8]) -> Option<ParsedMail> {
    let message = MessageParser::default().parse(raw)?;

    let text_body: Option<String> = message
        .body_text(0)
        .map(|t| strip_nul(&t))
        .filter(|t| !t.trim().is_empty());

    let from = message.from().and_then(|a| a.first());
    let from_addr = from
        .and_then(|a| a.address())
        .map(|a| strip_nul(&a.trim().to_lowercase()))
        .filter(|a| !a.is_empty());
    let from_display = from
        .and_then(|a| a.name())
        .map(|n| strip_nul(n.trim()))
        .filter(|n| !n.is_empty());

    let subject = message
        .subject()
        .map(strip_nul)
        .filter(|s| !s.trim().is_empty());

    let date = message
        .date()
        .and_then(|d| DateTime::<Utc>::from_timestamp(d.to_timestamp(), 0));

    if from_addr.is_none() && subject.is_none() && text_body.is_none() {
        return None;
    }

    Some(ParsedMail {
        from_addr,
        from_display,
        subject,
        date,
        text_body,
    })
}

/// Postgres `TEXT` cannot hold 0x00 (error 22021), and mail-parser
/// preserves NUL through every transfer-encoding — so an in-format email
/// with a NUL in any field would otherwise turn the Phase-B insert into
/// an attacker-triggerable 503 plus a permanently stuck `pending` row.
/// Stripped here, at the fence, so no format present or future can pass
/// one through (adversarial finding, SLICE_007d verification).
fn strip_nul(s: &str) -> String {
    if s.contains('\0') {
        s.replace('\0', "")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &[u8] = b"From: \"Cypress Bay Realty\" <Forms@CypressBayRealty.com>\r\n\
To: leads-k7f3q2wd@acme-realty.elysianfeld.com\r\n\
Subject: New contact form submission\r\n\
Date: Tue, 25 Aug 2026 09:30:00 -0700\r\n\
Message-ID: <test-1@cypressbayrealty.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Name: Jordan Ellis\r\n\
Email: jordan.ellis@example.com\r\n\
Phone: (555) 555-0142\r\n\
Message: Interested in the listing.\r\n";

    #[test]
    fn parses_plain_text_mail_with_lowercased_from() {
        let mail = parse(PLAIN).expect("parses");
        assert_eq!(
            mail.from_addr.as_deref(),
            Some("forms@cypressbayrealty.com")
        );
        assert_eq!(mail.from_display.as_deref(), Some("Cypress Bay Realty"));
        assert_eq!(mail.subject.as_deref(), Some("New contact form submission"));
        assert!(mail.date.is_some());
        assert!(mail.text_body.as_deref().unwrap().contains("Jordan Ellis"));
    }

    #[test]
    fn html_only_mail_falls_back_to_converted_text() {
        let html: &[u8] = b"From: forms@cypressbayrealty.com\r\n\
Subject: New contact form submission\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Name: Jordan Ellis</p><p>Email: jordan.ellis@example.com</p></body></html>\r\n";
        let mail = parse(html).expect("parses");
        let body = mail.text_body.expect("converted body");
        assert!(body.contains("Jordan Ellis"));
        assert!(!body.contains("<p>"), "tags must not survive conversion");
    }

    #[test]
    fn structureless_bytes_are_unparseable() {
        // Nothing email-shaped extracted (no From, no Subject, no body):
        // the §4a operational definition. mail-parser itself accepts all
        // of these (it treats the first line as a header), so the
        // wrapper's own gate is what keeps `email_unparsed` reachable.
        assert!(parse(b"").is_none());
        assert!(parse(b"\x00\x01\x02\x03").is_none());
        assert!(parse(b"hello world this is not an email").is_none());
    }

    #[test]
    fn header_date_is_parsed_but_missing_date_is_fine() {
        let no_date: &[u8] = b"From: a@b.com\r\nSubject: x\r\n\r\nbody\r\n";
        let mail = parse(no_date).expect("parses");
        assert!(mail.date.is_none());
        assert_eq!(mail.text_body.as_deref(), Some("body\r\n"));
    }

    #[test]
    fn nul_bytes_are_stripped_from_every_field() {
        // NUL in the body, subject, and From display — Postgres TEXT
        // rejects 0x00, so nothing leaving this wrapper may carry one.
        let with_nul: &[u8] = b"From: \"Cy\x00press\" <forms@cypressbayrealty.com>\r\n\
Subject: New contact \x00form submission\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Name: Jor\x00dan Ellis\r\n\
Email: jordan.ellis@example.com\r\n";
        let mail = parse(with_nul).expect("parses");
        assert!(!mail.subject.as_deref().unwrap().contains('\0'));
        assert!(!mail.text_body.as_deref().unwrap().contains('\0'));
        assert!(!mail.from_display.as_deref().unwrap().contains('\0'));
        assert_eq!(mail.subject.as_deref(), Some("New contact form submission"));
        assert!(mail.text_body.as_deref().unwrap().contains("Jordan Ellis"));
    }

    #[test]
    fn debug_never_prints_content() {
        let mail = parse(PLAIN).unwrap();
        let debug = format!("{mail:?}");
        assert!(!debug.contains("Jordan"));
        assert!(!debug.contains("cypressbayrealty"));
        assert!(!debug.contains("contact form"));
        assert!(debug.contains("has_from: true"));
    }
}

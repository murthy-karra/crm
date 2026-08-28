//! The MIME boundary (docs/specs/SLICE_007d.md §4a): the only module in
//! the workspace allowed to `use mail_parser`. Everything past this
//! signature sees owned std/chrono types only.

use chrono::{DateTime, Utc};
use mail_parser::MessageParser;

/// Recipient-list cap (Slice 009, docs/specs/SLICE_009.md §7): the capture
/// direction ladder only ever needs to know WHICH of a bounded set of
/// addresses matched, so an absurdly long To/Cc list (real or adversarial)
/// is truncated here rather than processed unbounded downstream.
const RECIPIENT_CAP: usize = 25;
/// Message-ID/References entry cap in bytes (Slice 009, docs/specs/SLICE_009.md
/// §7): these values are stored (normalized) on the `correspondence_captured`
/// fact row, so an oversized or adversarial header value is capped at the
/// fence, the same discipline `MESSAGE_MAX_BYTES` applies to lead messages.
pub(crate) const MESSAGE_ID_MAX_BYTES: usize = 512;

/// The minimal view of an email this rung needs. Owned types only — no
/// `mail_parser` type may leak past this struct. `Clone` (Slice 009): the
/// capture pipeline needs the OUTER parse's `message_id`/`references` after
/// `forward::resolve` has consumed a mail value to produce the (possibly
/// different) inner view — see `domain/capture/pipeline.rs`. `Default`:
/// every field defaults sensibly (`None`/empty `Vec`) — lets test fixtures
/// written before Slice 009 added these fields stay unedited by appending
/// `..Default::default()`.
#[derive(Clone, Default)]
pub struct ParsedMail {
    /// First `From` mailbox address, lowercased. The value format
    /// matchers key their sender-domain restriction on.
    pub from_addr: Option<String>,
    pub from_display: Option<String>,
    pub subject: Option<String>,
    /// The message's own `Date` header. Informational only for intake:
    /// `received_at` is always receipt time (docs/specs/SLICE_007d.md §4a;
    /// ladder cross-rung row 8–13), never this. Slice 009's capture
    /// pipeline is the first consumer that uses it (the CC path's
    /// `occurred_at`, clamped to receipt time — docs/specs/SLICE_009.md §4).
    pub date: Option<DateTime<Utc>>,
    /// `text/plain` body preferred; mail-parser's HTML→text conversion as
    /// the fallback for HTML-only mail.
    pub text_body: Option<String>,
    /// Slice 009 additions (docs/specs/SLICE_009.md §7), additive —
    /// intake consumers (`format.rs`/`formats/*.rs`) are unaffected and
    /// need not read them. Normalized: brackets stripped, capped at
    /// `MESSAGE_ID_MAX_BYTES`, NUL-stripped.
    pub message_id: Option<String>,
    pub in_reply_to: Vec<String>,
    pub references: Vec<String>,
    /// Lowercased bare addresses, no display names; NUL-stripped; capped
    /// at `RECIPIENT_CAP`.
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
}

/// Never derived: subject, sender, and body are lead content and must not
/// reach logs via an accidental `{:?}` (docs/specs/SLICE_007d.md §8).
/// Slice 009 extension: counts only for the new fields too — never the
/// message-ids or addresses themselves.
impl std::fmt::Debug for ParsedMail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedMail")
            .field("has_from", &self.from_addr.is_some())
            .field("has_subject", &self.subject.is_some())
            .field("has_body", &self.text_body.is_some())
            .field("has_message_id", &self.message_id.is_some())
            .field("references_count", &self.references.len())
            .field("to_count", &self.to_addrs.len())
            .field("cc_count", &self.cc_addrs.len())
            .finish()
    }
}

/// Strips surrounding `<...>` (mail-parser's own `parse_id` already does
/// this, but the invariant is enforced here too — defensively, and so it
/// holds regardless of the parser's internals), NUL-strips, and caps at
/// `MESSAGE_ID_MAX_BYTES`. `None` if nothing is left.
fn normalize_msg_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('<').trim_end_matches('>');
    let cleaned = strip_nul(trimmed);
    let capped = crate::domain::inquiry::parse::truncate_to_bytes(&cleaned, MESSAGE_ID_MAX_BYTES);
    if capped.is_empty() {
        None
    } else {
        Some(capped)
    }
}

/// Lowercased, trimmed, NUL-stripped bare addresses from an `Address`
/// header (`To`/`Cc`), capped at `RECIPIENT_CAP`. Display names are
/// dropped — capture's ladder only ever compares the address itself.
fn normalize_addr_list(addr: Option<&mail_parser::Address<'_>>) -> Vec<String> {
    let Some(addr) = addr else {
        return Vec::new();
    };
    addr.iter()
        .filter_map(|a| a.address())
        .map(|a| strip_nul(&a.trim().to_lowercase()))
        .filter(|a| !a.is_empty())
        .take(RECIPIENT_CAP)
        .collect()
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

    let message_id = message.message_id().and_then(normalize_msg_id);
    // Count-capped like recipients (adversarial L3): only first/last are
    // ever consumed, but an uncapped Vec from attacker headers is
    // inconsistent with the fence's cap discipline. First and last
    // entries must both survive the cap, so keep head + tail halves.
    let in_reply_to = cap_id_list(
        message
            .in_reply_to()
            .as_text_list()
            .map(|ids| ids.iter().filter_map(|id| normalize_msg_id(id)).collect())
            .unwrap_or_default(),
    );
    let references = cap_id_list(
        message
            .references()
            .as_text_list()
            .map(|ids| ids.iter().filter_map(|id| normalize_msg_id(id)).collect())
            .unwrap_or_default(),
    );
    let to_addrs = normalize_addr_list(message.to());
    let cc_addrs = normalize_addr_list(message.cc());

    Some(ParsedMail {
        from_addr,
        from_display,
        subject,
        date,
        text_body,
        message_id,
        in_reply_to,
        references,
        to_addrs,
        cc_addrs,
    })
}

/// Postgres `TEXT` cannot hold 0x00 (error 22021), and mail-parser
/// preserves NUL through every transfer-encoding — so an in-format email
/// with a NUL in any field would otherwise turn the Phase-B insert into
/// an attacker-triggerable 503 plus a permanently stuck `pending` row.
/// Stripped here, at the fence, so no format present or future can pass
/// one through (adversarial finding, SLICE_007d verification).
/// Keeps at most `ID_LIST_CAP` entries, preserving BOTH ends of the
/// chain (capture derives thread_key from the first entry and forward
/// message_id from the last — both must survive capping).
const ID_LIST_CAP: usize = 50;
fn cap_id_list(mut ids: Vec<String>) -> Vec<String> {
    if ids.len() > ID_LIST_CAP {
        let tail = ids.split_off(ids.len() - ID_LIST_CAP / 2);
        ids.truncate(ID_LIST_CAP / 2);
        ids.extend(tail);
    }
    ids
}

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

    // --- Slice 009 additions (docs/specs/SLICE_009.md §7) --------------

    #[test]
    fn message_id_is_normalized_bracket_free() {
        let mail = parse(PLAIN).expect("parses");
        assert_eq!(
            mail.message_id.as_deref(),
            Some("test-1@cypressbayrealty.com"),
            "brackets stripped"
        );
    }

    #[test]
    fn to_is_extracted_and_lowercased_with_no_cc_present() {
        let mail = parse(PLAIN).expect("parses");
        assert_eq!(
            mail.to_addrs,
            vec!["leads-k7f3q2wd@acme-realty.elysianfeld.com".to_string()]
        );
        assert!(mail.cc_addrs.is_empty());
        assert!(mail.references.is_empty());
        assert!(mail.in_reply_to.is_empty());
    }

    const THREADED: &[u8] = b"From: Agent Person <Agent@Example.com>\r\n\
To: Client One <client1@example.com>, client2@example.com\r\n\
Cc: Agent@Example.com, Colleague@Example.com\r\n\
Subject: Re: Following up\r\n\
Message-ID: <reply-123@example.com>\r\n\
In-Reply-To: <original-1@example.com>\r\n\
References: <original-1@example.com> <thread-2@example.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Thanks for the update.\r\n";

    #[test]
    fn to_and_cc_are_extracted_lowercased_with_multiple_recipients() {
        let mail = parse(THREADED).expect("parses");
        assert_eq!(
            mail.to_addrs,
            vec![
                "client1@example.com".to_string(),
                "client2@example.com".to_string()
            ]
        );
        assert_eq!(
            mail.cc_addrs,
            vec![
                "agent@example.com".to_string(),
                "colleague@example.com".to_string()
            ]
        );
    }

    #[test]
    fn references_and_in_reply_to_are_extracted_bracket_free_in_header_order() {
        let mail = parse(THREADED).expect("parses");
        assert_eq!(mail.in_reply_to, vec!["original-1@example.com".to_string()]);
        assert_eq!(
            mail.references,
            vec![
                "original-1@example.com".to_string(),
                "thread-2@example.com".to_string(),
            ],
            "first entry is thread_key's source, last is the forward-dedup source (spec §5)"
        );
        assert_eq!(mail.message_id.as_deref(), Some("reply-123@example.com"));
    }

    #[test]
    fn missing_references_and_message_id_are_empty_not_erroring() {
        let mail = parse(PLAIN).expect("parses");
        // PLAIN carries a Message-ID but no References/In-Reply-To.
        assert!(mail.references.is_empty());
        assert!(mail.in_reply_to.is_empty());

        let no_id: &[u8] = b"From: a@b.com\r\nSubject: x\r\n\r\nbody\r\n";
        let mail = parse(no_id).expect("parses");
        assert_eq!(mail.message_id, None);
    }

    #[test]
    fn recipient_list_is_capped() {
        let mut raw = String::from("From: a@b.com\r\nTo: ");
        let addrs: Vec<String> = (0..40).map(|i| format!("r{i}@example.com")).collect();
        raw.push_str(&addrs.join(", "));
        raw.push_str("\r\nSubject: many recipients\r\n\r\nbody\r\n");
        let mail = parse(raw.as_bytes()).expect("parses");
        assert_eq!(mail.to_addrs.len(), 25, "capped at RECIPIENT_CAP");
        assert_eq!(mail.to_addrs[0], "r0@example.com");
    }

    #[test]
    fn message_id_nul_bytes_are_stripped_and_length_is_capped() {
        let with_nul: &[u8] =
            b"From: a@b.com\r\nMessage-ID: <abc\x00def@example.com>\r\nSubject: x\r\n\r\nbody\r\n";
        let mail = parse(with_nul).expect("parses");
        assert_eq!(mail.message_id.as_deref(), Some("abcdef@example.com"));

        let long_id = format!("<{}@example.com>", "x".repeat(1000));
        let raw = format!("From: a@b.com\r\nMessage-ID: {long_id}\r\nSubject: x\r\n\r\nbody\r\n");
        let mail = parse(raw.as_bytes()).expect("parses");
        assert_eq!(
            mail.message_id.as_deref().unwrap().len(),
            MESSAGE_ID_MAX_BYTES
        );
    }

    #[test]
    fn debug_never_prints_the_new_fields_content() {
        let mail = parse(THREADED).unwrap();
        let debug = format!("{mail:?}");
        assert!(!debug.contains("example.com"));
        assert!(!debug.contains("client1"));
        assert!(debug.contains("has_message_id: true"));
        assert!(debug.contains("to_count: 2"));
        assert!(debug.contains("cc_count: 2"));
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

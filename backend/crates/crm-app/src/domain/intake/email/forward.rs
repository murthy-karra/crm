//! Forwarded-wrapper unwrapping (docs/specs/SLICE_007h1.md §3): pure
//! text processing on [`ParsedMail`] — no MIME-crate access (the fence in
//! `mod.rs` holds). A `ForwardStyle` mini-registry mirrors `format.rs` so
//! later styles (Outlook inline, message/rfc822 attachments) are additive
//! rungs; this rung pins the Gmail inline forward, English locale.
//!
//! Trust: unwrapping yields a [`SenderTrust::ForwardedClaim`] view whose
//! From/Subject/body are QUOTED TEXT under the forwarder's control
//! (D-040). Authentication verdicts, whenever a later rung extracts
//! them, live only on [`SenderTrust::Direct`] — `ForwardedClaim` has no
//! capacity for them, so inner content inheriting outer authentication
//! is a compile error, never a review hope.

use crate::domain::intake::email::mime::ParsedMail;

/// How the current mail view relates to what was actually delivered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SenderTrust {
    /// The RFC 822 From of the message as delivered — the future (and
    /// only) home of Authentication-Results verdicts (D-040; supersedes
    /// SLICE_007g §6's `ParsedMail.authenticated_sender` wording).
    Direct,
    /// The From line is quoted body text a forwarder typed or pasted.
    /// Deliberately no verdict field.
    ForwardedClaim { depth: u8 },
}

/// Iterative unwrap bound (docs/specs/SLICE_007h1.md §3): deeper nesting
/// keeps the deepest reached view; never unbounded work.
pub const MAX_DEPTH: u8 = 3;

/// The outcome of [`resolve`]: the view downstream consumers (format
/// detection, LLM input building) operate on, plus provenance. `style`
/// is a static registry name — the only style-derived value spans may
/// record (mirrors the `format` field rule, SLICE_007d §8).
pub struct Resolved {
    pub mail: ParsedMail,
    pub trust: SenderTrust,
    pub style: Option<&'static str>,
}

/// One recognized forwarding decoration. `unwrap_once` returns the inner
/// view only when the style's conservative trigger is fully satisfied —
/// anything less is `None` and the caller keeps the current view
/// (whole-message fallback, byte-identical to pre-007h1 behavior).
trait ForwardStyle: Send + Sync {
    fn name(&self) -> &'static str;
    fn unwrap_once(&self, mail: &ParsedMail) -> Option<ParsedMail>;
}

/// Declaration order, first match wins — later h-rungs append here.
static STYLES: &[&dyn ForwardStyle] = &[&GmailInline];

/// The single shared unwrap seam (docs/specs/SLICE_007h1.md §3): both
/// `parse_payload` and the extraction worker resolve through this one
/// function, so pinned matching and the LLM can never see different
/// messages. Callers run format detection on the DIRECT view first and
/// resolve only on no-match (reviewer S-2 — a genuine direct mail whose
/// body quotes a banner must keep its deterministic parse).
pub fn resolve(mail: ParsedMail) -> Resolved {
    let mut current = mail;
    let mut depth: u8 = 0;
    let mut style_name: Option<&'static str> = None;
    while depth < MAX_DEPTH {
        let unwrapped = STYLES
            .iter()
            .find_map(|s| s.unwrap_once(&current).map(|inner| (s.name(), inner)));
        let Some((name, inner)) = unwrapped else {
            break;
        };
        current = inner;
        depth += 1;
        style_name = Some(name);
    }
    let trust = if depth == 0 {
        SenderTrust::Direct
    } else {
        SenderTrust::ForwardedClaim { depth }
    };
    Resolved {
        mail: current,
        trust,
        style: style_name,
    }
}

/// The Gmail inline forward, English locale: a banner line of dashes
/// around the words "Forwarded message", then a `From:`/`Date:`/
/// `Subject:`/`To:` block, a blank line, and the inner body. Localized
/// banners deliberately do not match (whole-message fallback).
struct GmailInline;

impl ForwardStyle for GmailInline {
    fn name(&self) -> &'static str {
        "gmail_inline_v1"
    }

    /// Conservative trigger (docs/specs/SLICE_007h1.md §3): banner AND a
    /// parseable inner `From:` address AND a non-empty inner body — else
    /// no-op. A "Fwd:" subject alone, a banner with no address, or a
    /// banner with an empty remainder all fall through untouched.
    fn unwrap_once(&self, mail: &ParsedMail) -> Option<ParsedMail> {
        let body = mail.text_body.as_deref()?;
        let mut lines = body.lines();
        // Find the banner; everything before it (a forwarder's preamble
        // note) is dropped from the inner view (safe default (e)).
        lines.by_ref().find(|line| is_banner(line))?;

        // The quoted header block: prefixed lines until the first blank.
        let mut from_line: Option<&str> = None;
        let mut subject_line: Option<&str> = None;
        for line in lines.by_ref() {
            if line.trim().is_empty() {
                break;
            }
            if let Some(v) = line.strip_prefix("From:") {
                from_line = Some(v);
            } else if let Some(v) = line.strip_prefix("Subject:") {
                subject_line = Some(v);
            }
            // Date:/To:/Cc: are recognized-but-unused; unknown lines are
            // tolerated (Gmail wraps long headers) and simply skipped.
        }

        let (from_addr, from_display) = parse_from(from_line?)?;
        let inner_body: String = {
            let rest: Vec<&str> = lines.collect();
            rest.join("\n").trim().to_string()
        };
        if inner_body.is_empty() {
            return None;
        }

        Some(ParsedMail {
            from_addr: Some(from_addr),
            from_display,
            subject: subject_line
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            // The quoted Date line is a localized human string, not RFC
            // 2822 — informational only and unparsed; `received_at`
            // remains receipt time regardless (SLICE_007d §4a).
            date: None,
            text_body: Some(inner_body),
        })
    }
}

fn is_banner(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("----------") && trimmed.contains("Forwarded message")
}

/// `"Display Name" <addr@host>` or a bare `addr@host`. Returns the
/// address lowercased (matching the mime wrapper's contract that
/// `from_addr` is pre-lowercased for domain matching).
fn parse_from(value: &str) -> Option<(String, Option<String>)> {
    let value = value.trim();
    if let (Some(start), Some(end)) = (value.find('<'), value.rfind('>')) {
        if start < end {
            let addr = value[start + 1..end].trim();
            if is_addr_shaped(addr) {
                let display = value[..start].trim().trim_matches('"').trim().to_string();
                let display = if display.is_empty() {
                    None
                } else {
                    Some(display)
                };
                return Some((addr.to_lowercase(), display));
            }
        }
        return None;
    }
    if is_addr_shaped(value) {
        return Some((value.to_lowercase(), None));
    }
    None
}

fn is_addr_shaped(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(char::is_whitespace)
        // Nested/stray brackets (`<a@x>y<b@y>`) must not propagate as an
        // address (reviewer F4) — reject rather than guess.
        && !s.contains(['<', '>'])
        && s.rsplit_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
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

    const GMAIL_FWD_BODY: &str = "\
---------- Forwarded message ---------\n\
From: Maya Lindqvist <Maya.L@Example.com>\n\
Date: Mon, Aug 24, 2026 at 5:12\u{202f}PM\n\
Subject: Looking at 12 Harbor Lane\n\
To: <agent@gmail.com>\n\
\n\
Hi, we would love a viewing this week.\n\
Reach me at (415) 555-0173.\n";

    #[test]
    fn gmail_forward_unwraps_to_the_inner_view_with_forwarded_trust() {
        let m = mail(
            Some("agent@gmail.com"),
            Some("Fwd: Looking at 12 Harbor Lane"),
            Some(GMAIL_FWD_BODY),
        );
        let resolved = resolve(m);
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        assert_eq!(resolved.style, Some("gmail_inline_v1"));
        assert_eq!(
            resolved.mail.from_addr.as_deref(),
            Some("maya.l@example.com"),
            "inner address, lowercased"
        );
        assert_eq!(
            resolved.mail.from_display.as_deref(),
            Some("Maya Lindqvist")
        );
        assert_eq!(
            resolved.mail.subject.as_deref(),
            Some("Looking at 12 Harbor Lane"),
            "inner subject, no Fwd: prefix"
        );
        let body = resolved.mail.text_body.as_deref().unwrap();
        assert!(body.starts_with("Hi, we would love"));
        assert!(body.contains("(415) 555-0173"));
        assert!(!body.contains("Forwarded message"));
        assert!(resolved.mail.date.is_none(), "quoted Date is not parsed");
    }

    #[test]
    fn preamble_above_the_banner_is_dropped() {
        let with_preamble = format!("FYI — new lead, please follow up!\n\n{GMAIL_FWD_BODY}");
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(&with_preamble)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        assert!(!resolved.mail.text_body.as_deref().unwrap().contains("FYI"));
    }

    // --- The conservative trigger: every miss is a no-op ---------------

    #[test]
    fn fwd_subject_without_a_banner_is_a_no_op() {
        let m = mail(
            Some("agent@gmail.com"),
            Some("Fwd: totally a forward, trust me"),
            Some("Just an ordinary body with no banner."),
        );
        let resolved = resolve(m);
        assert_eq!(resolved.trust, SenderTrust::Direct);
        assert_eq!(resolved.style, None);
        assert_eq!(resolved.mail.from_addr.as_deref(), Some("agent@gmail.com"));
        assert_eq!(
            resolved.mail.text_body.as_deref(),
            Some("Just an ordinary body with no banner.")
        );
    }

    #[test]
    fn banner_without_a_parseable_inner_from_is_a_no_op() {
        for from_line in [
            "",                         // header block missing entirely
            "From: not an address\n",   // no @
            "From: two words@x.com\n",  // whitespace inside
            "From: <@nodomainlocal>\n", // empty local / undotted domain
        ] {
            let body = format!(
                "---------- Forwarded message ---------\n{from_line}Subject: x\n\ninner body\n"
            );
            let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(&body)));
            assert_eq!(resolved.trust, SenderTrust::Direct, "case: {from_line:?}");
        }
    }

    #[test]
    fn banner_with_an_empty_inner_body_is_a_no_op() {
        let body = "---------- Forwarded message ---------\n\
From: real@example.com\n\
Subject: x\n\
\n\
   \n";
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(body)));
        assert_eq!(resolved.trust, SenderTrust::Direct);
    }

    #[test]
    fn missing_body_is_a_no_op() {
        let resolved = resolve(mail(Some("agent@gmail.com"), Some("Fwd: x"), None));
        assert_eq!(resolved.trust, SenderTrust::Direct);
    }

    // --- Nesting and bounds --------------------------------------------

    fn wrap_once(inner: &str) -> String {
        format!(
            "---------- Forwarded message ---------\n\
From: hop@example.com\n\
Subject: hop\n\
\n\
{inner}"
        )
    }

    #[test]
    fn nested_forwards_unwrap_to_the_innermost_within_the_cap() {
        let innermost = "---------- Forwarded message ---------\n\
From: origin@example.com\n\
Subject: the original\n\
\n\
the original text\n";
        let twice = wrap_once(innermost);
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(&twice)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 2 });
        assert_eq!(
            resolved.mail.from_addr.as_deref(),
            Some("origin@example.com")
        );
        assert_eq!(
            resolved.mail.text_body.as_deref(),
            Some("the original text")
        );
    }

    #[test]
    fn depth_is_capped_and_pathological_many_banner_bodies_stay_bounded() {
        // 40 nested wrappers: the cap keeps work bounded and the result
        // is the deepest view reached, never a panic.
        let mut body = String::from("innermost text");
        for _ in 0..40 {
            body = wrap_once(&body);
        }
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(&body)));
        assert_eq!(
            resolved.trust,
            SenderTrust::ForwardedClaim { depth: MAX_DEPTH }
        );
        // Still wrapped MAX_DEPTH levels in — the remaining banners are
        // just body text of the deepest view.
        assert!(resolved
            .mail
            .text_body
            .as_deref()
            .unwrap()
            .contains("Forwarded message"));
    }

    // --- From-line shapes ----------------------------------------------

    #[test]
    fn bare_address_from_line_parses_without_display() {
        let (addr, display) = parse_from(" someone@example.com ").expect("parses");
        assert_eq!(addr, "someone@example.com");
        assert_eq!(display, None);
    }

    #[test]
    fn angle_bracket_from_uppercases_are_normalized() {
        let (addr, display) = parse_from("\"Ann O'Neil\" <Ann.ONeil@Example.COM>").expect("parses");
        assert_eq!(addr, "ann.oneil@example.com");
        assert_eq!(display.as_deref(), Some("Ann O'Neil"));
    }

    /// The poison-row regression pin (adversarial, 007d lesson):
    /// attacker-shaped From lines must neither panic (a Phase-B panic =
    /// a stuck `pending` row) nor smuggle the pinned domain into a
    /// match it shouldn't have. Multibyte chars hugging the `<`/`>`
    /// slice boundaries, RTL overrides, fullwidth brackets, nested and
    /// stray brackets, and lone `@`s all resolve cleanly.
    #[test]
    fn adversarial_from_lines_never_panic_and_never_forge_the_pinned_domain() {
        let from_lines = [
            "αβγ<δεζ@ηθ.ι>κλμ",
            "日本語 <名前@例え.テスト> 日本語",
            "\u{202e}evil\u{202d} <a@\u{202e}moc.ytlaeryabsserpyc\u{202d}>",
            "＜fullwidth@brackets.example＞",
            "<a@evil.com>x<b@cypressbayrealty.com>",
            "a<b<c<d@e.co>f>g>",
            ">multi< <a@b.co",
            "<<forms@cypressbayrealty.com>",
            "@",
            "<@>",
            "<>",
            "a@b",
            "café@münchen",
        ];
        for from_line in from_lines {
            let body =
                format!("---------- Forwarded message ---------\nFrom: {from_line}\n\ninner\n");
            // Must not panic; if it unwraps at all, the claimed domain
            // must never be exactly the pinned one.
            let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(&body)));
            if let Some(addr) = resolved.mail.from_addr.as_deref() {
                let domain = addr.rsplit_once('@').map(|(_, d)| d);
                assert_ne!(
                    domain,
                    Some("cypressbayrealty.com"),
                    "forged via From line {from_line:?}"
                );
                assert!(
                    !addr.contains(['<', '>']),
                    "brackets propagated from {from_line:?}"
                );
            }
        }
    }
}

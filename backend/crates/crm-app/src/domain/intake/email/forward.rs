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

use chrono::{DateTime, Utc};

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

        // The quoted header block: raw lines until the first blank, THEN
        // folded (Slice 009, docs/specs/SLICE_009.md §7a): Gmail wraps long
        // header lines (a long To/Cc list) across multiple physical lines,
        // continuations prefixed by whitespace per RFC 2822 §2.2.3 — the
        // pre-009 per-line loop had no folding step at all, so a wrapped
        // recipient list silently lost everything past the first line.
        let mut header_lines: Vec<&str> = Vec::new();
        for line in lines.by_ref() {
            if line.trim().is_empty() {
                break;
            }
            header_lines.push(line);
        }
        let folded = fold_continuations(header_lines);

        let mut from_line: Option<&str> = None;
        let mut date_line: Option<&str> = None;
        let mut subject_line: Option<&str> = None;
        let mut to_line: Option<&str> = None;
        let mut cc_line: Option<&str> = None;
        for line in &folded {
            if let Some(v) = line.strip_prefix("From:") {
                from_line = Some(v.trim());
            } else if let Some(v) = line.strip_prefix("Date:") {
                date_line = Some(v.trim());
            } else if let Some(v) = line.strip_prefix("Subject:") {
                subject_line = Some(v.trim());
            } else if let Some(v) = line.strip_prefix("To:") {
                to_line = Some(v.trim());
            } else if let Some(v) = line.strip_prefix("Cc:") {
                cc_line = Some(v.trim());
            }
            // Unknown lines are tolerated and simply skipped.
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
            subject: subject_line.filter(|s| !s.is_empty()).map(str::to_string),
            // Slice 009 (docs/specs/SLICE_009.md §7b): a best-effort parse
            // of Gmail's quoted human-readable Date line. Unparseable
            // (any locale but English, any unrecognized shape) -> `None`
            // — the capture pipeline's declared fallback is receipt time
            // + `backdated=false` (spec §4), never a panic or a guess.
            date: date_line.and_then(parse_inner_date),
            text_body: Some(inner_body),
            // Slice 009 (docs/specs/SLICE_009.md §7a): the inner
            // recipients, REQUIRED for outbound retroactive forwards (the
            // capture ladder's step 3 needs them). message_id/references
            // are deliberately left empty here — forwards derive their
            // fact row's message_id/thread_key from the OUTER (delivered)
            // mail's own References chain, never from this quoted text
            // (docs/specs/SLICE_009.md §5; see domain/capture/pipeline.rs).
            to_addrs: to_line.map(extract_addrs).unwrap_or_default(),
            cc_addrs: cc_line.map(extract_addrs).unwrap_or_default(),
            ..Default::default()
        })
    }
}

fn is_banner(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("----------") && trimmed.contains("Forwarded message")
}

/// RFC 2822 §2.2.3 unfolding, restricted to the quoted header block
/// (Slice 009, docs/specs/SLICE_009.md §7a): a line starting with a space
/// or tab continues the previous logical header line. A leading
/// continuation line (no preceding header — malformed input) is kept as
/// its own entry rather than dropped, so it still shows up as an "unknown
/// line" the caller's match tolerates, never panics on.
fn fold_continuations(lines: Vec<&str>) -> Vec<String> {
    let mut folded: Vec<String> = Vec::new();
    for line in lines {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');
        if is_continuation {
            if let Some(last) = folded.last_mut() {
                last.push(' ');
                last.push_str(line.trim());
                continue;
            }
        }
        folded.push(line.to_string());
    }
    folded
}

/// English month abbreviations, in `parse_inner_date`'s expected order —
/// `MONTHS[0]` is January, matching `chrono`'s 1-based month numbering
/// via `position() + 1`.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Parses Gmail's quoted forward Date line — English locale only (rung
/// scope, docs/specs/SLICE_009.md §7b): `"<Weekday>, <Mon> <D>, <YYYY> at
/// <H>:<MM> <AM/PM>"`, e.g. `"Mon, Aug 24, 2026 at 5:12 PM"`. Gmail places
/// a narrow no-break space (U+202F) before AM/PM; normalized to a regular
/// space first. Hand-rolled rather than a `chrono` format string
/// (consistent with this module's existing `parse_from`/`is_addr_shaped`
/// style — pure, auditable text processing, no surprises from a
/// strptime-style padding rule) — deliberately conservative: ANY
/// unexpected shape returns `None` rather than guessing, since the only
/// consumer's fallback (receipt time, `backdated=false`) is always safe.
/// The result has NO timezone (Gmail's quoted string carries none) and is
/// interpreted as UTC — informational placement only; the capture
/// pipeline's future-Date clamp (spec §4) bounds any resulting drift to
/// the present, never the other direction.
fn parse_inner_date(raw: &str) -> Option<DateTime<Utc>> {
    let normalized = raw.replace('\u{202f}', " ");
    let s = normalized.trim();

    // Drop an optional leading "<Weekday>, " — not used for anything.
    let after_weekday = match s.split_once(", ") {
        Some((_, rest)) => rest,
        None => s,
    };

    let (month_str, rest) = after_weekday.split_once(' ')?;
    let month = MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(month_str))? as u32
        + 1;

    let (day_str, rest) = rest.split_once(',')?;
    let day: u32 = day_str.trim().parse().ok()?;

    let (year_str, rest) = rest.trim().split_once(" at ")?;
    let year: i32 = year_str.trim().parse().ok()?;

    let (time_str, ampm) = rest.trim().rsplit_once(' ')?;
    let (hour_str, minute_str) = time_str.split_once(':')?;
    let mut hour: u32 = hour_str.trim().parse().ok()?;
    let minute: u32 = minute_str.trim().parse().ok()?;
    match ampm.trim().to_ascii_uppercase().as_str() {
        "PM" => {
            if hour != 12 {
                hour += 12;
            }
        }
        "AM" => {
            if hour == 12 {
                hour = 0;
            }
        }
        _ => return None,
    }

    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let time = chrono::NaiveTime::from_hms_opt(hour, minute, 0)?;
    Some(chrono::NaiveDateTime::new(date, time).and_utc())
}

/// Splits a comma-separated address-list header value into individual
/// entries, respecting quoted display names and angle-bracket address
/// forms so a comma inside `"Doe, Jane" <jane@x.com>` does not split the
/// entry. Best-effort text processing (no RFC 5322 grammar), consistent
/// with this module's pure-text-processing charter.
fn split_addr_list(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut angle_depth = 0i32;
    let mut in_quotes = false;
    let mut start = 0usize;
    for (i, ch) in value.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '<' if !in_quotes => angle_depth += 1,
            '>' if !in_quotes => angle_depth -= 1,
            ',' if !in_quotes && angle_depth <= 0 => {
                parts.push(value[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = value[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

/// A quoted `To:`/`Cc:` header value -> lowercased bare addresses, capped
/// (Slice 009, docs/specs/SLICE_009.md §7a). Reuses `parse_from` per
/// entry (it already handles both `"Name" <addr>` and a bare `addr`) and
/// discards the display name — the capture ladder only ever compares
/// addresses.
fn extract_addrs(value: &str) -> Vec<String> {
    split_addr_list(value)
        .into_iter()
        .filter_map(|part| parse_from(part).map(|(addr, _)| addr))
        .take(25)
        .collect()
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
    use chrono::TimeZone;

    fn mail(from: Option<&str>, subject: Option<&str>, body: Option<&str>) -> ParsedMail {
        ParsedMail {
            from_addr: from.map(|s| s.to_lowercase()),
            from_display: None,
            subject: subject.map(str::to_string),
            date: None,
            text_body: body.map(str::to_string),
            ..Default::default()
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
        // Slice 009 (docs/specs/SLICE_009.md §7b): the quoted Date line IS
        // now parsed (the U+202F narrow no-break space before "PM"
        // normalized away) — supersedes the pre-009 "quoted Date is not
        // parsed" assertion this fixture used to pin.
        assert_eq!(
            resolved.mail.date,
            Some(
                chrono::Utc
                    .with_ymd_and_hms(2026, 8, 24, 17, 12, 0)
                    .unwrap()
            )
        );
        // Slice 009 (docs/specs/SLICE_009.md §7a): the inner To is also
        // extracted now.
        assert_eq!(resolved.mail.to_addrs, vec!["agent@gmail.com".to_string()]);
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

    // --- Slice 009 additions (docs/specs/SLICE_009.md §7) --------------

    #[test]
    fn parses_the_gmail_quoted_date_with_the_narrow_no_break_space() {
        assert_eq!(
            parse_inner_date("Mon, Aug 24, 2026 at 5:12\u{202f}PM"),
            Some(Utc.with_ymd_and_hms(2026, 8, 24, 17, 12, 0).unwrap())
        );
    }

    #[test]
    fn parses_the_gmail_quoted_date_with_a_regular_space() {
        assert_eq!(
            parse_inner_date("Mon, Aug 24, 2026 at 5:12 PM"),
            Some(Utc.with_ymd_and_hms(2026, 8, 24, 17, 12, 0).unwrap())
        );
    }

    #[test]
    fn parses_am_and_the_noon_midnight_edge() {
        assert_eq!(
            parse_inner_date("Wed, Jan 1, 2026 at 12:00 AM"),
            Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
            "12 AM is midnight"
        );
        assert_eq!(
            parse_inner_date("Wed, Jan 1, 2026 at 12:00 PM"),
            Some(Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()),
            "12 PM is noon"
        );
        assert_eq!(
            parse_inner_date("Thu, Feb 2, 2026 at 9:05 AM"),
            Some(Utc.with_ymd_and_hms(2026, 2, 2, 9, 5, 0).unwrap())
        );
    }

    #[test]
    fn unparseable_inner_dates_fall_back_to_none() {
        for bad in [
            "",
            "not a date at all",
            "Mon, Aug 24, 2026",             // missing time
            "Aug 24, 2026 at 5:12 PM",       // missing weekday: mis-splits on the first ", "
            "Mon, Xxx 24, 2026 at 5:12 PM",  // unrecognized month
            "Mon, Aug 24, 2026 at 5:12 XX",  // unrecognized AM/PM
            "Mon, Aug 32, 2026 at 5:12 PM",  // invalid day
            "Mon, Aug 24, 2026 at 25:12 PM", // invalid hour
            "lundi 24 août 2026 à 17:12",    // non-English locale
        ] {
            assert_eq!(parse_inner_date(bad), None, "case: {bad:?}");
        }
    }

    #[test]
    fn continuation_lines_fold_into_the_previous_header() {
        // A wrapped To: header (Gmail's own line-folding style): the
        // continuation line is indented with a space. Pre-009 this
        // silently dropped everything past the first physical line.
        // NOTE: built with explicit `\n` (no backslash-newline source
        // continuation) — the latter strips leading whitespace from the
        // continued line, which would silently eat the very indentation
        // this test exists to exercise.
        let body = "---------- Forwarded message ---------\nFrom: origin@example.com\nTo: first@example.com,\n second@example.com,\n third@example.com\nSubject: fold test\n\ninner body\n";
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(body)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        assert_eq!(
            resolved.mail.to_addrs,
            vec![
                "first@example.com".to_string(),
                "second@example.com".to_string(),
                "third@example.com".to_string(),
            ]
        );
    }

    #[test]
    fn inner_to_and_cc_are_extracted_with_display_names_and_multiple_entries() {
        let body = "---------- Forwarded message ---------\n\
From: Origin Person <origin@example.com>\n\
Date: Mon, Aug 24, 2026 at 5:12 PM\n\
Subject: multi recipient\n\
To: \"Doe, Jane\" <jane@example.com>, plain@example.com\n\
Cc: another@example.com\n\
\n\
inner body\n";
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(body)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        // The quoted comma inside "Doe, Jane" must not split the entry.
        assert_eq!(
            resolved.mail.to_addrs,
            vec![
                "jane@example.com".to_string(),
                "plain@example.com".to_string()
            ]
        );
        assert_eq!(
            resolved.mail.cc_addrs,
            vec!["another@example.com".to_string()]
        );
    }

    #[test]
    fn absent_to_and_cc_lines_yield_empty_lists_not_a_no_op() {
        let body = "---------- Forwarded message ---------\n\
From: origin@example.com\n\
Subject: no recipients quoted\n\
\n\
inner body\n";
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(body)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        assert!(resolved.mail.to_addrs.is_empty());
        assert!(resolved.mail.cc_addrs.is_empty());
    }

    #[test]
    fn inner_view_carries_no_message_id_or_references() {
        // Slice 009 (docs/specs/SLICE_009.md §5): forwards derive
        // message_id/thread_key from the OUTER mail's own References
        // chain, never from the quoted inner text — the inner view's
        // these fields must stay empty so nothing downstream accidentally
        // reads them instead.
        let resolved = resolve(mail(Some("agent@gmail.com"), None, Some(GMAIL_FWD_BODY)));
        assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
        assert_eq!(resolved.mail.message_id, None);
        assert!(resolved.mail.references.is_empty());
        assert!(resolved.mail.in_reply_to.is_empty());
    }
}

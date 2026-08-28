//! The capture processing pipeline (docs/specs/SLICE_009.md §5):
//! store-raw-first (Phase A, `store::insert_pending`, done by the caller),
//! then one transaction — parse, direction ladder, match, insert fact
//! row(s) (+ auto-attempt), mark raw processed, publish realtime. Shared
//! between the live receive path (`domain/capture/receive.rs`) and the
//! unmatched-link endpoint (`domain/capture/commands.rs`) via
//! [`parse_metadata`] and [`insert_fact_and_maybe_attempt`], so a
//! link-created row is derived through EXACTLY the same metadata
//! computation as a live one (D-042.4 consistency, spec §8).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::capture::ladder::Direction;
use crate::domain::capture::queries;
use crate::domain::commands::{ContactChannel, ContactOutcome};
use crate::domain::contact;
use crate::domain::envelope::{FactEnvelope, Origin};
use crate::domain::facts::{self, ContactAttemptedFact};
use crate::domain::intake::email::{forward, mime, SenderTrust};
use crate::ids::{CorrelationId, OrganizationId, PersonId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Cc,
    Forward,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Via::Cc => "cc",
            Via::Forward => "forward",
        }
    }
}

/// Everything the ladder and the fact insert need, derived once from raw
/// bytes (docs/specs/SLICE_009.md §4, §5, §7). `working_from`/
/// `working_to_cc` are the WORKING view (inner if forwarded, direct
/// otherwise) — only the live ladder path (`gather_and_classify`) reads
/// them; the link path (`domain/capture/commands.rs`) uses everything
/// else and re-derives direction from the held row's own `direction_hint`
/// instead of re-running the ladder (spec §8: "not from re-matching
/// current state").
pub struct ParsedMetadata {
    pub via: Via,
    pub forward_style: Option<&'static str>,
    pub forward_depth: u8,
    /// Clamped to `min(message time, receipt time)` already (spec §4's
    /// future-Date clamp) — never re-clamped downstream.
    pub occurred_at: DateTime<Utc>,
    pub backdated: bool,
    pub message_id: Option<String>,
    pub thread_key: Option<String>,
    pub working_from: Option<String>,
    pub working_to_cc: Vec<String>,
}

/// mime-parse -> (unconditional) forward-resolve -> occurred_at/backdated/
/// via/message_id/thread_key. `None` only when `mime::parse` itself fails
/// (structurally unparseable bytes — mail-parser's own "nothing
/// email-shaped" gate, `domain/intake/email/mime.rs`).
///
/// Unlike intake's detect-first-then-resolve dance, capture has no
/// "format" to detect, so `forward::resolve` always runs once (spec §5:
/// "optional unwrap … fires only when the banner trigger holds" — the
/// trigger is `forward::resolve`'s OWN conservative gate, not a
/// preceding detect step).
///
/// message_id/thread_key (spec §5 dedup paragraph, load-bearing): for
/// `via = Cc`, message_id is the mail's own (outer) Message-ID; for
/// `via = Forward`, message_id is the OUTER mail's LAST References entry
/// (the forwarded ORIGINAL's id) — NEVER the forward's own Message-ID,
/// because that is what lets the same per-person UNIQUE constraint back
/// both re-forward dedup and forward-of-already-CC-captured dedup.
/// thread_key is uniform across both arms: the outer mail's FIRST
/// References entry, else its own Message-ID.
pub fn parse_metadata(raw: &[u8], received_at: DateTime<Utc>) -> Option<ParsedMetadata> {
    let outer = mime::parse(raw)?;
    let outer_message_id = outer.message_id.clone();
    let outer_references = outer.references.clone();

    let resolved = forward::resolve(outer);
    let (via, forward_depth) = match resolved.trust {
        SenderTrust::ForwardedClaim { depth } => (Via::Forward, depth),
        SenderTrust::Direct => (Via::Cc, 0),
    };
    let working = resolved.mail;

    // The future-Date clamp (spec §4, criterion 9): a forged or
    // timezone-misinterpreted message time can never suppress a Person
    // from Today for years, nor plant an uncleareable client_replied
    // nudge — bounded to the present by construction, both directions.
    let occurred_at_raw = working.date;
    let backdated = via == Via::Forward && occurred_at_raw.is_some();
    // Upper clamp: the Date header is sender-controlled; a future date
    // must never out-rank real work (spec §4). Lower floor (adversarial
    // L2): a garbled/forged ancient date (year 0001) is capped to
    // 2000-01-01 — no legitimate real-estate correspondence predates it,
    // and an unfloored value renders a degenerate timeline/Today row.
    let floor = chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
        .expect("static floor parses")
        .with_timezone(&chrono::Utc);
    let occurred_at = occurred_at_raw
        .unwrap_or(received_at)
        .min(received_at)
        .max(floor);

    let (message_id, thread_key) = match via {
        Via::Cc => (
            outer_message_id.clone(),
            outer_references.first().cloned().or(outer_message_id),
        ),
        Via::Forward => (
            outer_references.last().cloned(),
            outer_references.first().cloned().or(outer_message_id),
        ),
    };

    let mut working_to_cc = working.to_addrs;
    working_to_cc.extend(working.cc_addrs);

    Some(ParsedMetadata {
        via,
        forward_style: resolved.style,
        forward_depth,
        occurred_at,
        backdated,
        message_id,
        thread_key,
        working_from: working.from_addr,
        working_to_cc,
    })
}

/// The gathered ladder inputs (docs/specs/SLICE_009.md §5): one
/// `is_active_member_email`/`contact::identify` round trip per candidate
/// address (bounded by the mime-layer's ~25-recipient cap; v1 accepts the
/// per-address query cost — capture never contends the intake advisory
/// lock, spec §5, so there is no cross-tenant lock-budget concern to
/// bound it further). `capture_address` is the presented recipient
/// address (lowercased) — excluded from recipient matching (it is not a
/// correspondent).
pub async fn gather_and_classify(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
    capture_address: &str,
    meta: &ParsedMetadata,
) -> Result<super::ladder::LadderOutcome, sqlx::Error> {
    let from_is_active_member = match &meta.working_from {
        Some(from) => queries::is_active_member_email(tx, organization_id, from).await?,
        None => false,
    };

    let from_matched_person = if from_is_active_member {
        None
    } else {
        match meta
            .working_from
            .as_deref()
            .and_then(contact::normalize_email)
        {
            Some(normalized) => contact::identify(tx, organization_id, Some(normalized), None)
                .await?
                .map(|m| m.person_id),
            None => None,
        }
    };

    let recipients: Vec<&str> = meta
        .working_to_cc
        .iter()
        .map(String::as_str)
        .filter(|a| *a != capture_address)
        .collect();

    let mut recipient_matched_persons: Vec<PersonId> = Vec::new();
    let mut first_non_member_recipient: Option<&str> = None;
    for addr in &recipients {
        let is_member = queries::is_active_member_email(tx, organization_id, addr).await?;
        if !is_member && first_non_member_recipient.is_none() {
            first_non_member_recipient = Some(addr);
        }
        if let Some(normalized) = contact::normalize_email(addr) {
            if let Some(m) = contact::identify(tx, organization_id, Some(normalized), None).await? {
                if !recipient_matched_persons.contains(&m.person_id) {
                    recipient_matched_persons.push(m.person_id);
                }
            }
        }
    }

    Ok(super::ladder::classify(
        meta.working_from.as_deref(),
        from_is_active_member,
        from_matched_person,
        &recipient_matched_persons,
        first_non_member_recipient,
    ))
}

/// Inserts one `correspondence_captured` row (deduped via the per-person
/// Message-ID partial UNIQUE — `ON CONFLICT … DO NOTHING`) and, for a
/// NEWLY created outbound row, the auto-`contact_attempted` (D-042.4):
/// System actor on behalf of the agent, `causation_id` = the
/// correspondence fact id, `occurred_at` = message time (the same clamped
/// value). Returns `None` when the insert deduped (nothing else happens —
/// no duplicate attempt, no publish for that person). Shared by the live
/// pipeline (`domain/capture/receive.rs`) and the link command
/// (`domain/capture/commands.rs`) so both write EXACTLY the same shape
/// (spec §8: "a link-created OUTBOUND row writes the auto-attempt exactly
/// as the live pipeline does").
#[allow(clippy::too_many_arguments)]
pub async fn insert_fact_and_maybe_attempt(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    person_id: PersonId,
    direction: Direction,
    meta: &ParsedMetadata,
    correspondence_raw_id: crate::ids::CorrespondenceRawId,
    correlation_id: CorrelationId,
) -> Result<Option<Uuid>, sqlx::Error> {
    let envelope = FactEnvelope::for_system(
        organization_id,
        Origin::Webhook,
        meta.occurred_at,
        correlation_id,
        Some(agent_user_id),
    );
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();

    let row = sqlx::query!(
        r#"INSERT INTO correspondence_captured
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, agent_user_id, direction, message_id, thread_key, via,
             correspondence_raw_id, backdated)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
           ON CONFLICT (organization_id, person_id, message_id) WHERE message_id IS NOT NULL
           DO NOTHING
           RETURNING id"#,
        organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        person_id.0,
        agent_user_id.0,
        direction.as_str(),
        meta.message_id,
        meta.thread_key,
        meta.via.as_str(),
        correspondence_raw_id.0,
        meta.backdated,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    if direction == Direction::Outbound {
        let attempt_envelope = envelope.with_causation(row.id);
        facts::insert_contact_attempted(
            tx,
            &attempt_envelope,
            ContactAttemptedFact {
                person_id,
                channel: ContactChannel::Email,
                outcome: ContactOutcome::Sent,
                corrects_id: None,
                recorded_at: None,
            },
        )
        .await?;
    }

    Ok(Some(row.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, hour, 0, 0).unwrap()
    }

    const CC_MAIL: &[u8] = b"From: Agent Person <agent@ourfirm.com>\r\n\
To: save-abcdefghijkl@leads.elysianfeld.com, Client One <client@example.com>\r\n\
Subject: Re: showing\r\n\
Date: Thu, 27 Aug 2026 12:00:00 +0000\r\n\
Message-ID: <cc-1@ourfirm.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
See you Thursday.\r\n";

    #[test]
    fn cc_path_uses_the_header_date_and_is_never_backdated() {
        let meta = parse_metadata(CC_MAIL, ts(23)).expect("parses");
        assert_eq!(meta.via, Via::Cc);
        assert!(!meta.backdated);
        assert_eq!(meta.message_id.as_deref(), Some("cc-1@ourfirm.com"));
        assert_eq!(meta.working_from.as_deref(), Some("agent@ourfirm.com"));
        assert!(meta
            .working_to_cc
            .contains(&"client@example.com".to_string()));
    }

    #[test]
    fn cc_path_falls_back_to_receipt_time_when_the_header_date_is_absent() {
        let no_date: &[u8] =
            b"From: agent@ourfirm.com\r\nTo: client@example.com\r\nSubject: x\r\n\r\nbody\r\n";
        let received_at = ts(10);
        let meta = parse_metadata(no_date, received_at).expect("parses");
        assert_eq!(meta.occurred_at, received_at);
        assert!(!meta.backdated);
    }

    #[test]
    fn future_header_date_is_clamped_to_receipt_time_on_the_cc_path() {
        let future: &[u8] = b"From: agent@ourfirm.com\r\nTo: client@example.com\r\nSubject: x\r\nDate: Thu, 27 Aug 2099 12:00:00 +0000\r\n\r\nbody\r\n";
        let received_at = ts(10);
        let meta = parse_metadata(future, received_at).expect("parses");
        assert_eq!(meta.occurred_at, received_at, "clamped to receipt time");
        assert!(!meta.backdated, "clamping never sets backdated");
    }

    // --- The lower floor (spec §4/§9 adversarial L2) --------------------

    #[test]
    fn header_date_below_the_2000_floor_is_clamped_to_the_floor_on_the_cc_path() {
        let ancient: &[u8] = b"From: agent@ourfirm.com\r\nTo: client@example.com\r\nSubject: x\r\nDate: Mon, 01 Jan 1990 00:00:00 +0000\r\n\r\nbody\r\n";
        let received_at = ts(10);
        let meta = parse_metadata(ancient, received_at).expect("parses");
        assert_eq!(
            meta.occurred_at,
            Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
            "floored to 2000-01-01, not the forged 1990 date"
        );
        assert!(!meta.backdated, "clamping never sets backdated");
    }

    #[test]
    fn header_date_in_2019_passes_through_the_floor_unchanged_on_the_cc_path() {
        let legit: &[u8] = b"From: agent@ourfirm.com\r\nTo: client@example.com\r\nSubject: x\r\nDate: Tue, 15 Jan 2019 08:30:00 +0000\r\n\r\nbody\r\n";
        let received_at = ts(10);
        let meta = parse_metadata(legit, received_at).expect("parses");
        assert_eq!(
            meta.occurred_at,
            Utc.with_ymd_and_hms(2019, 1, 15, 8, 30, 0).unwrap(),
            "a legitimate historical date is neither floored nor upper-clamped"
        );
    }

    fn forward_mail(date_line: &str) -> Vec<u8> {
        format!(
            "From: Agent Person <agent@ourfirm.com>\r\n\
To: save-abcdefghijkl@leads.elysianfeld.com\r\n\
Subject: Fwd: old thread\r\n\
Message-ID: <fwd-outer@ourfirm.com>\r\n\
References: <thread-root@example.com> <original-1@example.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
---------- Forwarded message ---------\r\n\
From: Client One <client@example.com>\r\n\
{date_line}\
Subject: old thread\r\n\
To: agent@ourfirm.com\r\n\
\r\n\
Old message body.\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn forward_path_uses_the_inner_date_and_is_backdated() {
        let raw = forward_mail("Date: Mon, Aug 3, 2026 at 9:00 AM\r\n");
        let received_at = ts(23);
        let meta = parse_metadata(&raw, received_at).expect("parses");
        assert_eq!(meta.via, Via::Forward);
        assert!(meta.backdated);
        assert_eq!(
            meta.occurred_at,
            Utc.with_ymd_and_hms(2026, 8, 3, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn forward_path_falls_back_to_receipt_time_and_not_backdated_when_inner_date_unparseable() {
        let raw = forward_mail("Date: not a real date\r\n");
        let received_at = ts(23);
        let meta = parse_metadata(&raw, received_at).expect("parses");
        assert_eq!(meta.via, Via::Forward);
        assert!(!meta.backdated);
        assert_eq!(meta.occurred_at, received_at);
    }

    #[test]
    fn forward_path_derives_message_id_from_the_last_outer_reference_and_thread_key_from_the_first()
    {
        let raw = forward_mail("Date: Mon, Aug 3, 2026 at 9:00 AM\r\n");
        let meta = parse_metadata(&raw, ts(23)).expect("parses");
        assert_eq!(
            meta.message_id.as_deref(),
            Some("original-1@example.com"),
            "the FORWARDED ORIGINAL's id (last References entry) — never the forward's own Message-ID"
        );
        assert_eq!(
            meta.thread_key.as_deref(),
            Some("thread-root@example.com"),
            "thread_key takes the FIRST References entry"
        );
    }

    #[test]
    fn forward_path_message_id_is_none_when_the_outer_mail_carries_no_references() {
        let raw = b"From: Agent Person <agent@ourfirm.com>\r\n\
To: save-abcdefghijkl@leads.elysianfeld.com\r\n\
Subject: Fwd: old thread\r\n\
Message-ID: <fwd-outer-2@ourfirm.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
---------- Forwarded message ---------\r\n\
From: Client One <client@example.com>\r\n\
Subject: old thread\r\n\
\r\n\
Old message body.\r\n";
        let meta = parse_metadata(raw, ts(23)).expect("parses");
        assert_eq!(meta.via, Via::Forward);
        assert_eq!(
            meta.message_id, None,
            "no References -> no dedup key; falls back to raw-byte dedup only (accepted)"
        );
        assert_eq!(
            meta.thread_key.as_deref(),
            Some("fwd-outer-2@ourfirm.com"),
            "thread_key falls back to the outer mail's own Message-ID"
        );
    }

    #[test]
    fn unparseable_bytes_yield_none() {
        assert!(parse_metadata(b"", ts(0)).is_none());
        assert!(parse_metadata(b"\x00\x01\x02", ts(0)).is_none());
    }
}

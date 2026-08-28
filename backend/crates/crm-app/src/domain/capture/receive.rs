//! The capture Phase A/B entry point (docs/specs/SLICE_009.md §5, §9):
//! resolve the presented token (§3's digest lookup), seal + store raw
//! (Phase A, its own transaction — `store::insert_pending`), then the
//! pipeline (Phase B, one transaction — `pipeline`/`ladder`). Wired into
//! `domain/intake/receive.rs`'s dispatch as an internal routing
//! extension: the frozen `/inbound/email` HTTP envelope is untouched
//! (spec §3).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::capture::ladder::LadderOutcome;
use crate::domain::capture::token::CaptureToken;
use crate::domain::capture::{address, pipeline, store};
use crate::domain::intake::receive::ReceiveInboundEmailError;
use crate::domain::raw_payload::crypto;
use crate::ids::{CorrelationId, CorrespondenceRawId, OrganizationId, PersonId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

/// The outcome that matters to the frozen HTTP envelope: EVERY non-
/// rejected outcome (captured, held, duplicate, unparseable) is 200
/// `{"status":"accepted"}` — the finer vocabulary (spec §9) lives only in
/// this function's own `capture.inbound_email` span, never in the
/// response (mirrors 007d's "the envelope reveals nothing about the
/// parse outcome").
pub enum CaptureEmailOutcome {
    Captured,
    /// Redelivery of an already-processed raw — nothing reprocessed, no
    /// publish.
    Duplicate,
    /// Unknown, wrong, or deactivated-member token — nothing stored
    /// (criterion 5).
    Rejected,
}

/// Held-queue flood cap per (organization, agent) — adversarial M2. The
/// list endpoint shows at most 200; the cap keeps un-listable rows (and
/// their plaintext counterparty addresses) from accumulating forever
/// under spam. Generous vs the display window so a busy-but-legitimate
/// backlog is never silently dropped at the margin.
const HELD_QUEUE_CAP: i64 = 500;

/// `IntakeMailConfig`'s `.domain` is the only field `CaptureToken` needs
/// (capture has one grammar, independent of the Organization's configured
/// `IntakeAddressScheme` — see `token.rs`), so this takes the concrete
/// type directly rather than importing crm-api's `AppState` shape.
pub async fn receive_captured_email(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    mail_cfg: &crate::config::IntakeMailConfig,
    presented: CaptureToken,
    raw: &[u8],
    received_at: DateTime<Utc>,
) -> Result<CaptureEmailOutcome, ReceiveInboundEmailError> {
    let mut conn = pool.acquire().await?;
    let resolved = address::resolve(&mut conn, &presented).await?;
    drop(conn);

    let Some(resolved) = resolved else {
        return Ok(CaptureEmailOutcome::Rejected);
    };

    let capture_address = presented.render(mail_cfg);

    let candidate_id = CorrespondenceRawId::new(Uuid::new_v4());
    let content_hmac = crypto::content_hmac(key, raw);
    let sealed = crypto::seal_correspondence(key, resolved.organization_id, candidate_id, raw)
        .map_err(|_| ReceiveInboundEmailError::Crypto)?;
    let byte_len = raw.len() as i32;

    let stored_id = store::insert_pending(
        pool,
        candidate_id,
        resolved.organization_id,
        received_at,
        &sealed.nonce,
        &sealed.ciphertext,
        &content_hmac,
        byte_len,
    )
    .await?;

    process_and_record(
        pool,
        key,
        publisher,
        resolved.organization_id,
        resolved.agent_user_id,
        capture_address,
        stored_id,
        received_at,
        byte_len,
    )
    .await
}

/// The three shapes Phase B can settle into (kept distinct so the
/// instrumented wrapper below can never conflate "already processed" with
/// "unparseable" — both would otherwise collapse to the same `Ok(None)`).
enum PhaseBResult {
    Duplicate,
    Unparseable,
    Processed {
        outcome: LadderOutcome,
        forwarded: bool,
        forward_style: Option<&'static str>,
        forward_depth: u8,
        correlation_id: CorrelationId,
        newly_created: Vec<PersonId>,
    },
}

/// Phase B, instrumented (docs/specs/SLICE_009.md §9): a domain-layer span
/// (mirrors `domain/commands/log_contact_attempt.rs`'s
/// `#[tracing::instrument]` + `Span::current().record` pattern, rather
/// than the route-handler-only instrumentation `routes/inbound_email.rs`
/// uses for intake — capture's span is nested inside that handler's own
/// `intake.inbound_email` span when reached via `/inbound/email`, which
/// is exactly what "internal routing extension" (spec §3) implies: the
/// frozen span's declared fields are untouched, and capture's own span
/// carries capture's own vocabulary). NEVER subjects, addresses,
/// message-ids, or tokens — only ids, statics, and counts.
#[tracing::instrument(
    name = "capture.inbound_email",
    skip_all,
    fields(
        organization_id = %organization_id,
        correspondence_raw_id = %correspondence_raw_id,
        byte_len = byte_len,
        outcome = tracing::field::Empty,
        direction = tracing::field::Empty,
        matched = tracing::field::Empty,
        forwarded = tracing::field::Empty,
        forward_style = tracing::field::Empty,
        forward_depth = tracing::field::Empty,
    )
)]
#[allow(clippy::too_many_arguments)]
async fn process_and_record(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    capture_address: String,
    correspondence_raw_id: CorrespondenceRawId,
    received_at: DateTime<Utc>,
    byte_len: i32,
) -> Result<CaptureEmailOutcome, ReceiveInboundEmailError> {
    let span = tracing::Span::current();
    let result = process_attempt(
        pool,
        key,
        organization_id,
        agent_user_id,
        &capture_address,
        correspondence_raw_id,
        received_at,
    )
    .await;

    match result {
        Ok(PhaseBResult::Duplicate) => {
            span.record("outcome", "capture_duplicate");
            Ok(CaptureEmailOutcome::Duplicate)
        }
        Ok(PhaseBResult::Unparseable) => {
            span.record("outcome", "capture_unparseable");
            span.record("matched", false);
            Ok(CaptureEmailOutcome::Captured)
        }
        Ok(PhaseBResult::Processed {
            outcome,
            forwarded,
            forward_style,
            forward_depth,
            correlation_id,
            newly_created,
        }) => {
            span.record("forwarded", forwarded);
            if let Some(style) = forward_style {
                span.record("forward_style", style);
            }
            if forwarded {
                span.record("forward_depth", forward_depth);
            }
            match &outcome {
                LadderOutcome::Matched { direction, .. } => {
                    span.record("outcome", "captured");
                    span.record("direction", direction.as_str());
                    span.record("matched", true);
                }
                LadderOutcome::Held { direction_hint, .. } => {
                    span.record("outcome", "capture_unmatched");
                    span.record("direction", direction_hint.as_str());
                    span.record("matched", false);
                }
            }
            // No held-queue realtime (spec §8): `newly_created` is empty
            // for a Held outcome by construction (see `process_attempt`).
            for person_id in newly_created {
                let event = RealtimeEvent::person_changed(
                    organization_id,
                    received_at,
                    correlation_id,
                    person_id,
                    PersonChange::CorrespondenceCaptured,
                );
                publisher
                    .publish_after_commit(Publication::for_event(event))
                    .await;
            }
            Ok(CaptureEmailOutcome::Captured)
        }
        Err(ReceiveInboundEmailError::Crypto) => {
            span.record("outcome", "crypto");
            Err(ReceiveInboundEmailError::Crypto)
        }
        Err(ReceiveInboundEmailError::Database(err)) => {
            span.record("outcome", "database");
            Err(ReceiveInboundEmailError::Database(err))
        }
        Err(other) => {
            span.record("outcome", "internal");
            Err(other)
        }
    }
}

/// The actual Phase B work: lock, decrypt, parse, ladder, insert, mark
/// processed — one transaction. No queue exists for capture to route an
/// unparseable message to (unlike intake's Unresolved) — marked
/// processed, nothing else created (stated assumption; D-042.6 already
/// forecloses a read surface over `correspondence_raw` regardless).
async fn process_attempt(
    pool: &PgPool,
    key: &RawPayloadKey,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    capture_address: &str,
    correspondence_raw_id: CorrespondenceRawId,
    received_at: DateTime<Utc>,
) -> Result<PhaseBResult, ReceiveInboundEmailError> {
    let mut tx = pool.begin().await?;

    let locked = store::lock_for_processing(&mut tx, correspondence_raw_id, organization_id)
        .await?
        .ok_or(ReceiveInboundEmailError::Internal)?;
    if locked.processed {
        return Ok(PhaseBResult::Duplicate);
    }

    let raw = crypto::open_correspondence(
        key,
        organization_id,
        correspondence_raw_id,
        &locked.nonce,
        &locked.ciphertext,
    )
    .map_err(|_| ReceiveInboundEmailError::Crypto)?;

    let Some(meta) = pipeline::parse_metadata(&raw, received_at) else {
        store::mark_processed(&mut tx, correspondence_raw_id, organization_id).await?;
        tx.commit().await?;
        return Ok(PhaseBResult::Unparseable);
    };

    let ladder_outcome =
        pipeline::gather_and_classify(&mut tx, organization_id, capture_address, &meta).await?;

    let correlation_id = CorrelationId::new(Uuid::new_v4());
    let mut newly_created = Vec::new();

    match &ladder_outcome {
        LadderOutcome::Matched { direction, persons } => {
            for person_id in persons {
                let inserted = pipeline::insert_fact_and_maybe_attempt(
                    &mut tx,
                    organization_id,
                    agent_user_id,
                    *person_id,
                    *direction,
                    &meta,
                    correspondence_raw_id,
                    correlation_id,
                )
                .await?;
                if inserted.is_some() {
                    newly_created.push(*person_id);
                }
            }
        }
        LadderOutcome::Held {
            direction_hint,
            counterparty,
        } => {
            // Flood guard (adversarial M2): the held queue retains
            // plaintext third-party addresses and the list surfaces only
            // the newest 200 — an unbounded INSERT under spam would park
            // PII beyond the agent's reach forever (D-015 §4). At the
            // cap, the mail is still stored (encrypted raw, processed)
            // but no held row is created; the span records the overflow.
            let held_count = store::count_held(&mut tx, organization_id, agent_user_id).await?;
            if held_count >= HELD_QUEUE_CAP {
                tracing::Span::current().record("outcome", "capture_held_overflow");
            } else {
                store::insert_held(
                    &mut tx,
                    store::HeldMessageInsert {
                        organization_id,
                        agent_user_id,
                        correspondence_raw_id,
                        // Bounded (adversarial L4): the longest legal
                        // SMTP address is 320 bytes; anything longer is
                        // attacker garbage and truncation only ever
                        // affects garbage.
                        counterparty_email: counterparty
                            .clone()
                            .map(|c| crate::domain::inquiry::parse::truncate_to_bytes(&c, 320)),
                        direction_hint: *direction_hint,
                        captured_at: received_at,
                    },
                )
                .await?;
            }
        }
    }

    store::mark_processed(&mut tx, correspondence_raw_id, organization_id).await?;
    tx.commit().await?;

    Ok(PhaseBResult::Processed {
        forwarded: meta.via == pipeline::Via::Forward,
        forward_style: meta.forward_style,
        forward_depth: meta.forward_depth,
        outcome: ladder_outcome,
        correlation_id,
        newly_created,
    })
}

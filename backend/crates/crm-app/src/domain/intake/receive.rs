//! Inbound email intake (SLICE_007b Phase A + SLICE_007d Phase B):
//! receives raw RFC 822 bytes, stores them encrypted, then attempts the
//! pinned-format parse and completes intake as the System actor
//! (`Origin::Webhook`) through the same `complete_intake` Phase B the
//! `/api/inquiries` path uses. Anything that fails a parse gate lands
//! Unresolved with the raw mail preserved; the HTTP envelope reveals
//! nothing about the parse outcome (docs/specs/SLICE_007d.md §4d/§5).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::commands::receive_inquiry::{
    complete_intake, CompleteIntake, ReceiveInquiryOutcome,
};
use crate::domain::commands::CommandError;
use crate::domain::envelope::Origin;
use crate::domain::inquiry::parse::UnresolvedReason;
use crate::domain::intake::email::{detect, format, mime};
use crate::domain::intake::{IntakeActor, IntakeAddress};
use crate::domain::raw_payload::{crypto, store};
use crate::realtime::{Publication, Publisher, RealtimeEvent};

/// Presented to `constant_time_eq` on an unknown slug, so that branch does
/// the same 8-byte compare as a known slug with the wrong token. 8 bytes:
/// the length `IntakeAddress::parse_recipient` guarantees for every
/// presented token.
const DUMMY_TOKEN: &[u8; 8] = b"00000000";

pub enum InboundEmailOutcome {
    /// A pinned format matched and intake completed: Person, Inquiry,
    /// facts, routing (docs/specs/SLICE_007d.md §4e).
    Completed {
        person_id: Uuid,
        inquiry_id: Uuid,
        raw_payload_id: Uuid,
    },
    /// Stored, but a parse gate failed — the row is terminal
    /// `unresolved` with `reason` (docs/specs/SLICE_007d.md §4e).
    Unresolved {
        raw_payload_id: Uuid,
        reason: UnresolvedReason,
    },
    /// The advisory-lock budget was exhausted: the row stays `pending`
    /// (queue-visible; a redelivery or 007e's Try-again completes it) and
    /// the response is still 200 accepted — `/inbound/email` can never
    /// return `intake_busy` (docs/specs/SLICE_007d.md §4f).
    DeferredPending { raw_payload_id: Uuid },
    /// Byte-identical redelivery of an already-terminal row: nothing
    /// reprocessed, nothing published.
    Duplicate,
    /// Wrong token: stored nowhere.
    Rejected,
}

#[derive(Debug)]
pub enum ReceiveInboundEmailError {
    OrgNotFound,
    InvalidRecipient,
    Crypto,
    /// A non-transport Phase-B failure (e.g. an Organization with no
    /// stages configured): 500 to the caller, the committed `pending` row
    /// remains per SLICE_002's crash-window rule.
    Internal,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ReceiveInboundEmailError {
    fn from(err: sqlx::Error) -> Self {
        ReceiveInboundEmailError::Database(err)
    }
}

/// Phase A (unchanged from SLICE_007b): parse recipient → resolve org by
/// slug+token → seal + insert. Phase B (SLICE_007d): attempt the
/// pinned-format parse and complete intake as the System actor. The
/// per-Organization advisory lock is taken by `complete_intake` only on
/// the success path — parse-gate failures never contend for it.
pub async fn receive_inbound_email(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    mail_cfg: &crate::config::IntakeMailConfig,
    recipient: &str,
    raw: &[u8],
) -> Result<InboundEmailOutcome, ReceiveInboundEmailError> {
    let received_at = Utc::now();

    // Parse recipient (syntax validation only; reveals nothing secret).
    let intake_addr = IntakeAddress::parse_recipient(recipient, mail_cfg)
        .ok_or(ReceiveInboundEmailError::InvalidRecipient)?;

    // Resolve organization by slug + tenant-only authentication.
    let mut conn = pool.acquire().await?;
    let lookup = organization_by_intake_slug(&mut conn, &intake_addr.slug).await?;
    drop(conn);

    let (org_id, stored_token) = match lookup {
        Some(row) => row,
        None => {
            // Unknown slug: still run the constant-time compare against a
            // fixed dummy token, so this path does the same work as the
            // wrong-token path below (never a shortcut an attacker could
            // distinguish by timing).
            let _ = constant_time_eq(intake_addr.token.as_bytes(), DUMMY_TOKEN);
            return Err(ReceiveInboundEmailError::OrgNotFound);
        }
    };

    // Constant-time token compare (tenant credential, never logged).
    if !constant_time_eq(intake_addr.token.as_bytes(), stored_token.as_bytes()) {
        return Ok(InboundEmailOutcome::Rejected);
    }

    // Phase A: seal + insert pending (unchanged from SLICE_007b).
    let candidate_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(key, raw);
    let sealed = crypto::seal(key, org_id, candidate_id, raw)
        .map_err(|_| ReceiveInboundEmailError::Crypto)?;

    let byte_len = raw.len() as i32;
    let stored_id = store::insert_pending(
        pool,
        candidate_id,
        org_id,
        "email",
        "rfc822_v1",
        "webhook",
        received_at,
        &sealed.nonce,
        &sealed.ciphertext,
        &content_hmac,
        byte_len,
    )
    .await?;

    // Phase B (docs/specs/SLICE_007d.md §4d): the shared completion path,
    // with the email parse closure — MIME → format detection → field
    // extraction → normalization. The correlation id is fresh per
    // delivery (there is no CommandContext to inherit one from).
    let correlation_id = Uuid::new_v4();
    let actor = IntakeActor::System {
        organization_id: org_id,
        origin: Origin::Webhook,
        correlation_id,
    };
    let result = complete_intake(
        pool,
        key,
        publisher,
        &actor,
        CompleteIntake {
            raw_payload_id: stored_id,
            content_hmac: &content_hmac,
            received_at,
            assign_to_user_id: None,
        },
        |bytes| {
            let mail = mime::parse(bytes).ok_or(UnresolvedReason::EmailUnparsed)?;
            let email_format = detect(&mail).ok_or(UnresolvedReason::EmailUnrecognizedFormat)?;
            // The static format name is the one format-derived value
            // observability may record (docs/specs/SLICE_007d.md §8); the
            // route's span declares the field, and recording on a span
            // without it is a no-op for the `/api/inquiries` caller's
            // span (which never runs this closure anyway).
            tracing::Span::current().record("format", email_format.name());
            format::to_parsed_lead(email_format.extract(&mail))
        },
    )
    .await;

    match result {
        Ok(ReceiveInquiryOutcome::Resolved {
            inquiry_id,
            person_id,
            duplicate: false,
            ..
        }) => Ok(InboundEmailOutcome::Completed {
            person_id,
            inquiry_id,
            raw_payload_id: stored_id,
        }),
        Ok(ReceiveInquiryOutcome::Unresolved {
            raw_payload_id,
            reason,
            duplicate: false,
        }) => Ok(InboundEmailOutcome::Unresolved {
            raw_payload_id,
            reason,
        }),
        // A terminal row from an earlier delivery (resolved or
        // unresolved, including 007b-era `email_unparsed` rows):
        // idempotent success, nothing reprocessed, no publish
        // (docs/specs/SLICE_007d.md §4d).
        Ok(
            ReceiveInquiryOutcome::Resolved {
                duplicate: true, ..
            }
            | ReceiveInquiryOutcome::Unresolved {
                duplicate: true, ..
            },
        ) => Ok(InboundEmailOutcome::Duplicate),
        // Advisory-lock budget exhausted: row stays `pending`, 200
        // accepted, one ids-only invalidation so the queue shows the row
        // (docs/specs/SLICE_007d.md §4f). Known accepted limitation: when
        // a later redelivery completes this row, only `person_changed`
        // fires — connected queue viewers keep the stale row until
        // refetch (SLICE_003's refetch-recovery convention).
        Err(CommandError::IntakeBusy) => {
            let event = RealtimeEvent::intake_unresolved_changed(
                org_id,
                received_at,
                correlation_id,
                stored_id,
            );
            publisher
                .publish_after_commit(Publication::for_event(event))
                .await;
            Ok(InboundEmailOutcome::DeferredPending {
                raw_payload_id: stored_id,
            })
        }
        Err(CommandError::Crypto) => Err(ReceiveInboundEmailError::Crypto),
        Err(CommandError::Database(err)) => Err(ReceiveInboundEmailError::Database(err)),
        // InvalidAssignee is unreachable (assign_to_user_id is None);
        // NoStagesConfigured/Corrupt are real internal states — the
        // committed `pending` row remains per the crash-window rule.
        Err(_) => Err(ReceiveInboundEmailError::Internal),
    }
}

/// Look up an Organization by its intake_slug, returning (id, intake_token).
/// Returns None if slug is unknown. The token must then be constant-time
/// compared in Rust (never in the WHERE clause).
async fn organization_by_intake_slug(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    slug: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, intake_token FROM organization WHERE intake_slug = $1",
    )
    .bind(slug)
    .fetch_optional(&mut **conn)
    .await
}

/// Constant-time comparison (never use == on secrets). The presented bytes
/// are assumed to be an 8-byte token; the stored bytes should also be 8
/// bytes. Returns false on length mismatch (the known attack vector).
fn constant_time_eq(presented: &[u8], stored: &[u8]) -> bool {
    if presented.len() != stored.len() {
        return false;
    }
    let mut result = 0u8;
    for (a, b) in presented.iter().zip(stored.iter()) {
        result |= a ^ b;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_identical_and_rejects_length_mismatch_or_wrong_bytes() {
        assert!(constant_time_eq(b"k7f3q2wd", b"k7f3q2wd"));
        assert!(!constant_time_eq(b"k7f3q2wd", b"k7f3q2we"));
        assert!(!constant_time_eq(b"short", b"k7f3q2wd"));
        assert!(!constant_time_eq(b"", DUMMY_TOKEN));
        // Single-bit-flip near-misses: every position must be checked, not
        // just short-circuited on the first differing byte.
        for i in 0..DUMMY_TOKEN.len() {
            let mut near_miss = *DUMMY_TOKEN;
            near_miss[i] ^= 0x01;
            assert!(!constant_time_eq(&near_miss, DUMMY_TOKEN), "byte {i}");
        }
    }

    #[test]
    fn dummy_token_is_exactly_eight_bytes() {
        // The length IntakeAddress::parse_recipient guarantees for every
        // presented token (address.rs's TOKEN_LEN), so the unknown-slug
        // path's dummy compare never hits the length-mismatch fast path.
        assert_eq!(DUMMY_TOKEN.len(), 8);
    }
}

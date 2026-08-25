//! Phase-A-only inbound email intake (SLICE_007b): receives encrypted raw
//! RFC 822 bytes, stores them, marks Unresolved with no parsing, publishes
//! the event. No CommandContext, no facts, no actor. Returns an idempotent
//! Stored outcome on both first delivery and byte-identical re-delivery
//! (same response, no second publish).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::intake::IntakeAddress;
use crate::domain::raw_payload::{crypto, store};
use crate::realtime::{Publication, Publisher, RealtimeEvent};

/// Presented to `constant_time_eq` on an unknown slug, so that branch does
/// the same 8-byte compare as a known slug with the wrong token. 8 bytes:
/// the length `IntakeAddress::parse_recipient` guarantees for every
/// presented token.
const DUMMY_TOKEN: &[u8; 8] = b"00000000";

pub enum InboundEmailOutcome {
    Stored { raw_payload_id: Uuid },
    Duplicate,
    Rejected,
}

#[derive(Debug)]
pub enum ReceiveInboundEmailError {
    OrgNotFound,
    InvalidRecipient,
    Crypto,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ReceiveInboundEmailError {
    fn from(err: sqlx::Error) -> Self {
        ReceiveInboundEmailError::Database(err)
    }
}

/// Phase A only: parse recipient → resolve org by slug+token →
/// seal + insert → mark unresolved → publish. Never locks, never takes the
/// intake advisory lock (Phase B's concern only), never writes facts.
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

    // Phase A: seal + insert pending + mark unresolved + publish (new approach).
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

    // Mark unresolved only if the row is still pending (fresh delivery).
    let marked = mark_unresolved_if_pending(pool, stored_id, org_id, "email_unparsed").await?;

    if marked {
        // Fresh delivery: publish the event.
        let event = RealtimeEvent::intake_unresolved_changed(
            org_id,
            received_at,
            Uuid::new_v4(),
            stored_id,
        );
        let publication = Publication::for_event(event);
        publisher.publish_after_commit(publication).await;
        Ok(InboundEmailOutcome::Stored {
            raw_payload_id: stored_id,
        })
    } else {
        // Duplicate or already-terminal: same response, no publish.
        Ok(InboundEmailOutcome::Duplicate)
    }
}

/// Atomically mark a pending raw_payload as unresolved, returning true if
/// the row was actually pending (fresh delivery / stuck-pending rescue);
/// false if it was already terminal (duplicate after the mark).
async fn mark_unresolved_if_pending(
    pool: &PgPool,
    id: Uuid,
    organization_id: Uuid,
    reason: &str,
) -> Result<bool, ReceiveInboundEmailError> {
    let result = sqlx::query(
        "UPDATE raw_payload SET resolution = 'unresolved', resolved_at = now(), unresolved_reason = $3 WHERE id = $1 AND organization_id = $2 AND resolution = 'pending' RETURNING id"
    )
    .bind(id)
    .bind(organization_id)
    .bind(reason)
    .fetch_optional(pool)
    .await?;

    Ok(result.is_some())
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

//! Two-phase `raw_payload` persistence (docs/specs/SLICE_002.md §3) and the
//! unresolved-queue read model.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub struct LockedRawPayload {
    pub id: Uuid,
    pub source: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub resolution: String,
    pub unresolved_reason: Option<String>,
    pub inquiry_id: Option<Uuid>,
}

pub struct UnresolvedItem {
    pub id: Uuid,
    pub source: String,
    pub received_at: DateTime<Utc>,
    pub resolution: String,
    pub unresolved_reason: Option<String>,
    pub byte_len: i32,
}

pub struct ResolvedLookup {
    pub inquiry_id: Uuid,
    pub person_id: Uuid,
    pub person_created: bool,
    pub strategy: String,
    pub assigned_user_id: Option<Uuid>,
}

/// Phase A (docs/specs/SLICE_002.md §3): stores the encrypted payload
/// before parsing, in its own transaction committed before Phase B runs.
/// `ON CONFLICT DO NOTHING` on the idempotency key means a byte-identical
/// retry discards its own freshly-encrypted copy; the returned id is
/// always the *stored* row's id (spec §14a), which is what the AEAD
/// associated data was actually computed against.
#[allow(clippy::too_many_arguments)]
pub async fn insert_pending(
    pool: &PgPool,
    id: Uuid,
    organization_id: Uuid,
    source: &str,
    payload_format: &str,
    origin: &str,
    received_at: DateTime<Utc>,
    nonce: &[u8],
    ciphertext: &[u8],
    content_hmac: &[u8],
    byte_len: i32,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending')
           ON CONFLICT (organization_id, source, content_hmac) DO NOTHING"#,
        id,
        organization_id,
        source,
        payload_format,
        origin,
        received_at,
        nonce,
        ciphertext,
        content_hmac,
        byte_len,
    )
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query!(
        r#"SELECT id FROM raw_payload WHERE organization_id = $1 AND source = $2 AND content_hmac = $3"#,
        organization_id,
        source,
        content_hmac,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(row.id)
}

/// Phase B row lock (docs/specs/SLICE_002.md §3): `SELECT … FOR UPDATE`
/// scoped to the Organization, so a concurrent duplicate delivery blocks
/// until the first finishes.
pub async fn lock_for_processing(
    tx: &mut PgConnection,
    id: Uuid,
    organization_id: Uuid,
) -> Result<Option<LockedRawPayload>, sqlx::Error> {
    sqlx::query_as!(
        LockedRawPayload,
        r#"SELECT id, source, nonce, ciphertext, resolution, unresolved_reason, inquiry_id
           FROM raw_payload WHERE id = $1 AND organization_id = $2 FOR UPDATE"#,
        id,
        organization_id,
    )
    .fetch_optional(tx)
    .await
}

pub async fn mark_resolved(
    tx: &mut PgConnection,
    id: Uuid,
    organization_id: Uuid,
    inquiry_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE raw_payload SET resolution = 'resolved', resolved_at = now(), inquiry_id = $3
           WHERE id = $1 AND organization_id = $2"#,
        id,
        organization_id,
        inquiry_id,
    )
    .execute(tx)
    .await?;
    Ok(())
}

pub async fn mark_unresolved(
    tx: &mut PgConnection,
    id: Uuid,
    organization_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE raw_payload SET resolution = 'unresolved', resolved_at = now(), unresolved_reason = $3
           WHERE id = $1 AND organization_id = $2"#,
        id,
        organization_id,
        reason,
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// Reconstructs the `Resolved` outcome for a duplicate delivery whose row
/// was already `resolved` on this call. `assigned_user_id` here is the
/// Person's *current* assignee, not a historical snapshot
/// (docs/specs/SLICE_002.md §5).
pub async fn resolved_outcome_for_inquiry(
    tx: &mut PgConnection,
    organization_id: Uuid,
    inquiry_id: Uuid,
) -> Result<ResolvedLookup, sqlx::Error> {
    sqlx::query_as!(
        ResolvedLookup,
        r#"SELECT i.id as inquiry_id, i.person_id, ir.person_created, rd.strategy,
                  p.assigned_user_id
           FROM inquiry i
           JOIN inquiry_received ir ON ir.inquiry_id = i.id
           JOIN routing_decision rd ON rd.inquiry_id = i.id
           JOIN person p ON p.id = i.person_id
           WHERE i.id = $1 AND i.organization_id = $2"#,
        inquiry_id,
        organization_id,
    )
    .fetch_one(tx)
    .await
}

/// Unresolved-queue metadata (`GET /api/intake/unresolved`; spec §5):
/// visible to every Organization member, id/source/received_at/
/// resolution/reason/byte_len only — the queue never decrypts. Fetches one
/// row past the 500 cap to compute `truncated`. Lists `pending` and
/// `unresolved` only — `discarded` rows left the queue in SLICE_007e
/// (declared SLICE_002 §5 amendment).
pub async fn list_unresolved(
    conn: &mut PgConnection,
    organization_id: Uuid,
) -> Result<(Vec<UnresolvedItem>, bool), sqlx::Error> {
    let mut rows = sqlx::query_as!(
        UnresolvedItem,
        r#"SELECT id, source, received_at, resolution, unresolved_reason, byte_len
           FROM raw_payload
           WHERE organization_id = $1 AND resolution IN ('pending', 'unresolved')
           ORDER BY received_at DESC
           LIMIT 501"#,
        organization_id,
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((rows, truncated))
}

/// The workbench detail read (docs/specs/SLICE_007e.md §4): the full
/// org-scoped row, visible only while `pending` or `unresolved` —
/// `resolved`/`discarded`/unknown/cross-org are all the same `None`
/// (byte-identical 404 upstream). The first reader of `payload_format`.
pub struct DetailRawPayload {
    pub id: Uuid,
    pub source: String,
    pub payload_format: String,
    pub received_at: DateTime<Utc>,
    pub resolution: String,
    pub unresolved_reason: Option<String>,
    pub byte_len: i32,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub async fn unresolved_row_for_detail(
    conn: &mut PgConnection,
    id: Uuid,
    organization_id: Uuid,
) -> Result<Option<DetailRawPayload>, sqlx::Error> {
    sqlx::query_as!(
        DetailRawPayload,
        r#"SELECT id, source, payload_format, received_at, resolution,
                  unresolved_reason, byte_len, nonce, ciphertext
           FROM raw_payload
           WHERE id = $1 AND organization_id = $2
             AND resolution IN ('pending', 'unresolved')"#,
        id,
        organization_id,
    )
    .fetch_optional(conn)
    .await
}

//! `correspondence_raw` and `capture_message` persistence
//! (docs/specs/SLICE_009.md §4 items 2, 4). Two-phase, mirroring
//! `raw_payload::store`'s Phase A/B split: `insert_pending` commits alone
//! (Phase A — crash window rescued by MTA redelivery, no admin queue
//! exists here so redelivery is the ONLY rescue); `lock_for_processing` +
//! `mark_processed` bracket Phase B's single transaction.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

use crate::domain::capture::ladder::Direction;
use crate::ids::{CaptureMessageId, CorrespondenceRawId, OrganizationId, UserId};

// --- correspondence_raw ------------------------------------------------

pub struct LockedCorrespondenceRaw {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub processed: bool,
}

/// Phase A: seal + insert, own transaction, `ON CONFLICT DO NOTHING` on
/// the `(organization_id, content_hmac)` idempotency key — a
/// byte-identical redelivery discards its own freshly-encrypted copy and
/// the returned id is always the STORED row's (the one the AEAD
/// associated data was actually computed against), exactly
/// `raw_payload::store::insert_pending`'s contract.
#[allow(clippy::too_many_arguments)]
pub async fn insert_pending(
    pool: &PgPool,
    id: CorrespondenceRawId,
    organization_id: OrganizationId,
    received_at: DateTime<Utc>,
    nonce: &[u8],
    ciphertext: &[u8],
    content_hmac: &[u8],
    byte_len: i32,
) -> Result<CorrespondenceRawId, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"INSERT INTO correspondence_raw
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
           VALUES ($1, $2, $3, $4, $5, $6, $7, false)
           ON CONFLICT (organization_id, content_hmac) DO NOTHING"#,
        id.0,
        organization_id.0,
        received_at,
        nonce,
        ciphertext,
        content_hmac,
        byte_len,
    )
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query!(
        r#"SELECT id FROM correspondence_raw WHERE organization_id = $1 AND content_hmac = $2"#,
        organization_id.0,
        content_hmac,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(CorrespondenceRawId::new(row.id))
}

/// Phase B row lock: `SELECT … FOR UPDATE`, Organization-scoped, so a
/// concurrent duplicate delivery blocks until the first finishes
/// (`raw_payload::store::lock_for_processing`'s exact pattern).
pub async fn lock_for_processing(
    tx: &mut PgConnection,
    id: CorrespondenceRawId,
    organization_id: OrganizationId,
) -> Result<Option<LockedCorrespondenceRaw>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT nonce, ciphertext, processed
           FROM correspondence_raw WHERE id = $1 AND organization_id = $2 FOR UPDATE"#,
        id.0,
        organization_id.0,
    )
    .fetch_optional(tx)
    .await?;
    Ok(row.map(|r| LockedCorrespondenceRaw {
        nonce: r.nonce,
        ciphertext: r.ciphertext,
        processed: r.processed,
    }))
}

/// The same row-lock read, used by the link endpoint to re-derive a held
/// row's metadata (occurred_at/via/message_id/thread_key) from its
/// original stored bytes (docs/specs/SLICE_009.md §8) — no `FOR UPDATE`
/// (link's own transaction locks `capture_message`, and `correspondence_raw`
/// is never mutated again once `processed`).
pub async fn read_for_link(
    conn: &mut PgConnection,
    id: CorrespondenceRawId,
    organization_id: OrganizationId,
) -> Result<Option<(DateTime<Utc>, Vec<u8>, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT received_at, nonce, ciphertext
           FROM correspondence_raw WHERE id = $1 AND organization_id = $2"#,
        id.0,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.received_at, r.nonce, r.ciphertext)))
}

pub async fn mark_processed(
    tx: &mut PgConnection,
    id: CorrespondenceRawId,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE correspondence_raw SET processed = true WHERE id = $1 AND organization_id = $2"#,
        id.0,
        organization_id.0,
    )
    .execute(tx)
    .await?;
    Ok(())
}

// --- capture_message (held queue) ---------------------------------------

pub struct HeldMessageInsert {
    pub organization_id: OrganizationId,
    pub agent_user_id: UserId,
    pub correspondence_raw_id: CorrespondenceRawId,
    pub counterparty_email: Option<String>,
    pub direction_hint: Direction,
    pub captured_at: DateTime<Utc>,
}

pub async fn insert_held(
    tx: &mut PgConnection,
    fields: HeldMessageInsert,
) -> Result<CaptureMessageId, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO capture_message
            (organization_id, agent_user_id, correspondence_raw_id, counterparty_email,
             direction_hint, captured_at, status)
           VALUES ($1, $2, $3, $4, $5, $6, 'held')
           RETURNING id"#,
        fields.organization_id.0,
        fields.agent_user_id.0,
        fields.correspondence_raw_id.0,
        fields.counterparty_email,
        fields.direction_hint.as_str(),
        fields.captured_at,
    )
    .fetch_one(tx)
    .await?;
    Ok(CaptureMessageId::new(row.id))
}

pub struct UnmatchedItem {
    pub id: CaptureMessageId,
    pub counterparty_email: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub direction_hint: Option<Direction>,
    /// Always `Held` — the query below filters on it — but carried
    /// through anyway since the response shape names it explicitly (spec
    /// §8): "id, counterparty_email, captured_at, direction_hint, status".
    pub status: HeldStatus,
}

fn decode_direction_hint(raw: Option<String>) -> Result<Option<Direction>, sqlx::Error> {
    match raw.as_deref() {
        None => Ok(None),
        Some("inbound") => Ok(Some(Direction::Inbound)),
        Some("outbound") => Ok(Some(Direction::Outbound)),
        Some(other) => Err(sqlx::Error::Decode(
            format!("capture_message.direction_hint: unknown value {other:?}").into(),
        )),
    }
}

/// The agent's held list (docs/specs/SLICE_009.md §8): attributed-agent
/// only (the caller supplies `agent_user_id` from the SESSION, never a
/// client-chosen value — enforced at the route layer, D-042.3), `status
/// = 'held'` only, capped at 200 + a `truncated` flag (one extra row
/// fetched to detect it, `today::queries`'s cap pattern).
pub async fn list_unmatched(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    agent_user_id: UserId,
) -> Result<(Vec<UnmatchedItem>, bool), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, counterparty_email, captured_at, direction_hint
           FROM capture_message
           WHERE organization_id = $1 AND agent_user_id = $2 AND status = 'held'
           ORDER BY captured_at DESC, id DESC
           LIMIT 201"#,
        organization_id.0,
        agent_user_id.0,
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 200;
    let items = rows
        .into_iter()
        .take(200)
        .map(|r| {
            Ok(UnmatchedItem {
                id: CaptureMessageId::new(r.id),
                counterparty_email: r.counterparty_email,
                captured_at: r.captured_at,
                direction_hint: decode_direction_hint(r.direction_hint)?,
                status: HeldStatus::Held,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok((items, truncated))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldStatus {
    Held,
    Linked,
    Dismissed,
}

impl HeldStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            HeldStatus::Held => "held",
            HeldStatus::Linked => "linked",
            HeldStatus::Dismissed => "dismissed",
        }
    }

    fn decode(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "held" => Ok(HeldStatus::Held),
            "linked" => Ok(HeldStatus::Linked),
            "dismissed" => Ok(HeldStatus::Dismissed),
            other => Err(sqlx::Error::Decode(
                format!("capture_message.status: unknown value {other:?}").into(),
            )),
        }
    }
}

pub struct HeldRow {
    pub correspondence_raw_id: CorrespondenceRawId,
    pub counterparty_email: Option<String>,
    pub direction_hint: Option<Direction>,
    pub status: HeldStatus,
}

/// Locks a held-queue row scoped to BOTH the Organization and the
/// attributed agent — a row belonging to a different agent (or a
/// different Organization) is indistinguishable from a nonexistent one at
/// this query (404 upstream, D-042.3: attributed-agent only, admins
/// included).
pub async fn lock_for_transition(
    tx: &mut PgConnection,
    id: CaptureMessageId,
    organization_id: OrganizationId,
    agent_user_id: UserId,
) -> Result<Option<HeldRow>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT correspondence_raw_id, counterparty_email, direction_hint, status
           FROM capture_message
           WHERE id = $1 AND organization_id = $2 AND agent_user_id = $3
           FOR UPDATE"#,
        id.0,
        organization_id.0,
        agent_user_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    row.map(|r| {
        Ok(HeldRow {
            correspondence_raw_id: CorrespondenceRawId::new(r.correspondence_raw_id),
            counterparty_email: r.counterparty_email,
            direction_hint: decode_direction_hint(r.direction_hint)?,
            status: HeldStatus::decode(&r.status)?,
        })
    })
    .transpose()
}

/// Both terminal transitions NULL `counterparty_email` in the same
/// statement (D-015 §4; reviewer finding — a no-DELETE table must not
/// retain third-party PII forever past either terminal state).
pub async fn mark_linked(
    tx: &mut PgConnection,
    id: CaptureMessageId,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE capture_message SET status = 'linked', counterparty_email = NULL
           WHERE id = $1 AND organization_id = $2"#,
        id.0,
        organization_id.0,
    )
    .execute(tx)
    .await?;
    Ok(())
}

pub async fn mark_dismissed(
    tx: &mut PgConnection,
    id: CaptureMessageId,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE capture_message SET status = 'dismissed', counterparty_email = NULL
           WHERE id = $1 AND organization_id = $2"#,
        id.0,
        organization_id.0,
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// The flood-guard count (adversarial M2): live held rows for one
/// (organization, agent) pair. Runs inside the capture transaction.
pub async fn count_held(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
    agent_user_id: UserId,
) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM capture_message
           WHERE organization_id = $1 AND agent_user_id = $2 AND status = 'held'"#,
        organization_id.0,
        agent_user_id.0,
    )
    .fetch_one(tx)
    .await?;
    Ok(count)
}

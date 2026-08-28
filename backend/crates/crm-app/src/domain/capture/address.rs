//! `capture_address` persistence (docs/specs/SLICE_009.md §3, §4 item 1):
//! mint-if-absent (new membership and reactivation both call this — a
//! reactivated member gets their EXISTING address back, since the
//! `(organization_id, user_id)` UNIQUE makes an unconditional mint an
//! error), the receive-path lookup, self-service rotation (mirrors
//! `domain/intake/rotate.rs::rotate_intake_token` exactly, adapted for a
//! per-agent, globally-unique-by-digest row instead of a single
//! per-Organization column), and the plaintext re-display read.

use chrono::Utc;
use sqlx::PgConnection;
use sqlx::PgPool;

use crate::domain::capture::token::{mint_capture_token, token_lookup_digest, CaptureToken};
use crate::domain::envelope::{CommandContext, FactEnvelope};
use crate::ids::{OrganizationId, UserId};

/// The presented token's length, fixed 12 bytes — every real caller
/// (`CaptureToken::parse_recipient`, `mint_capture_token`) already
/// guarantees this, so the dummy-compare branch below never hits a
/// length-mismatch fast path either.
const DUMMY_TOKEN: &[u8; 12] = b"000000000000";

pub struct ResolvedCaptureAddress {
    pub organization_id: OrganizationId,
    pub agent_user_id: UserId,
}

/// The receive-path lookup (docs/specs/SLICE_009.md §3): indexed SELECT
/// by `token_lookup` (the digest — never the token's own bytes touch the
/// B-tree, see `token.rs`'s module doc), joined against an ACTIVE
/// membership so a deactivated agent's token stops resolving without
/// deleting or mutating the row (criterion 8: reactivation restores the
/// SAME address because the row was never touched). Miss (`None`) or
/// found-but-verify-fails are BOTH `Ok(None)`, byte-identical to the
/// caller — no oracle distinguishing "no such token" from "token belongs
/// to a deactivated member" from "digest collided but the full token
/// didn't verify". A dummy constant-time compare on the miss branch keeps
/// the total per-call work (one indexed SELECT + one constant-time
/// compare) uniform across every outcome, mirroring
/// `domain/intake/receive.rs`'s own dummy-token pattern.
pub async fn resolve(
    conn: &mut PgConnection,
    presented: &CaptureToken,
) -> Result<Option<ResolvedCaptureAddress>, sqlx::Error> {
    let digest = token_lookup_digest(presented);
    let row = sqlx::query!(
        r#"SELECT ca.organization_id, ca.user_id, ca.token
           FROM capture_address ca
           JOIN organization_membership m
             ON m.organization_id = ca.organization_id AND m.user_id = ca.user_id
           WHERE ca.token_lookup = $1 AND m.status = 'active'"#,
        digest,
    )
    .fetch_optional(conn)
    .await?;

    let Some(row) = row else {
        let _ = presented.verify(DUMMY_TOKEN);
        return Ok(None);
    };

    let stored = CaptureToken::new(row.token);
    if !stored.verify(presented.reveal().as_bytes()) {
        return Ok(None);
    }

    Ok(Some(ResolvedCaptureAddress {
        organization_id: OrganizationId::new(row.organization_id),
        agent_user_id: UserId::new(row.user_id),
    }))
}

/// Mints a capture address for `(organization_id, user_id)` ONLY IF none
/// exists yet (docs/specs/SLICE_009.md §4 item 1). Called from
/// `AcceptInvitation` (a brand-new membership — always absent, so this
/// always mints) and `SetMemberStatus`'s reactivation branch (usually
/// already present from backfill/original activation — a no-op restoring
/// continuity; absent only for a membership that predates this slice's
/// backfill somehow, handled rather than assumed). Retries on the
/// (astronomically unlikely) global `token_lookup` collision; the
/// `(organization_id, user_id)` conflict path is the common, intentional
/// no-op — the two are distinguished by which constraint actually fired,
/// not guessed.
pub async fn mint_capture_address_if_absent(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), sqlx::Error> {
    loop {
        let token = mint_capture_token();
        let digest = token_lookup_digest(&token);
        let result = sqlx::query!(
            r#"INSERT INTO capture_address (organization_id, user_id, token, token_lookup)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (organization_id, user_id) DO NOTHING"#,
            organization_id.0,
            user_id.0,
            token.reveal(),
            digest,
        )
        .execute(&mut *tx)
        .await;
        match result {
            Ok(_) => return Ok(()),
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => continue,
            Err(err) => return Err(err),
        }
    }
}

/// The plaintext re-display read (`GET /api/capture/address`) — kept for
/// re-display exactly like `organization.intake_token`.
pub async fn current_token(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<Option<CaptureToken>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        r#"SELECT token FROM capture_address WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(CaptureToken::new))
}

#[derive(Debug)]
pub enum RotateError {
    /// No `capture_address` row for this (organization, user) — should be
    /// unreachable for an active member given backfill + mint-if-absent
    /// on every activation path, handled rather than assumed.
    NotFound,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RotateError {
    fn from(err: sqlx::Error) -> Self {
        RotateError::Database(err)
    }
}

impl RotateError {
    pub fn kind(&self) -> &'static str {
        match self {
            RotateError::NotFound => "not_found",
            RotateError::Database(_) => "database",
        }
    }
}

/// Self-service rotation (docs/specs/SLICE_009.md §3, §8): immediate
/// invalidation, one transaction (mint → UPDATE → audit fact), mirroring
/// `domain/intake/rotate.rs::rotate_intake_token`. Two collision guards,
/// for two different reasons: `new != old` (so "rotated" always actually
/// changes something observable) via `CaptureToken::verify`, and a retry
/// on the DB's global `token_lookup` UNIQUE (capture tokens, unlike
/// intake's single per-Organization column, are looked up directly by
/// digest — see `token.rs`'s module doc — so a genuine cross-agent
/// collision, astronomically unlikely, is a real possibility this
/// function must not surface as a 500).
pub async fn rotate_capture_token(
    pool: &PgPool,
    ctx: &CommandContext,
) -> Result<CaptureToken, RotateError> {
    let mut tx = pool.begin().await?;

    let old_token: Option<String> = sqlx::query_scalar!(
        r#"SELECT token FROM capture_address WHERE organization_id = $1 AND user_id = $2 FOR UPDATE"#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let old = CaptureToken::new(old_token.ok_or(RotateError::NotFound)?);

    let new_token = loop {
        let mut candidate = mint_capture_token();
        while candidate.verify(old.reveal().as_bytes()) {
            candidate = mint_capture_token();
        }
        let digest = token_lookup_digest(&candidate);
        let result = sqlx::query!(
            r#"UPDATE capture_address SET token = $3, token_lookup = $4
               WHERE organization_id = $1 AND user_id = $2"#,
            ctx.organization_id.0,
            ctx.actor_user_id.0,
            candidate.reveal(),
            digest,
        )
        .execute(&mut *tx)
        .await;
        match result {
            Ok(_) => break candidate,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => continue,
            Err(err) => return Err(err.into()),
        }
    };

    let envelope = FactEnvelope::for_command(ctx, Utc::now());
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    sqlx::query!(
        r#"INSERT INTO capture_token_rotated
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id,
             origin, occurred_at, correlation_id, causation_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(new_token)
}

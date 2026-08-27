//! Intake-token rotation (docs/specs/SLICE_007g.md §5): break-glass for
//! a leaked/spammed address. Single-token, immediate invalidation — the
//! old address stops working the instant this commits (mail to it is
//! 200-rejected at the endpoint, silently; the confirm dialog says so).
//! One transaction: mint → UPDATE → audit fact. No realtime event
//! (nothing queue-visible changes).

use chrono::Utc;
use sqlx::PgPool;

use crate::domain::admin::validation::mint_intake_token;
use crate::domain::envelope::{CommandContext, FactEnvelope};

#[derive(Debug)]
pub enum RotateError {
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
            RotateError::Database(_) => "database",
        }
    }
}

/// Mints a fresh token (re-minting on the 2^-40 collision with the old
/// one so "new ≠ old" can never flake), updates the Organization, and
/// writes the append-only audit fact — one transaction. Returns the new
/// token so the route can render the response without a post-commit
/// re-read (which could fail AFTER the rotation happened and lie to the
/// admin — adversarial L1).
pub async fn rotate_intake_token(
    pool: &PgPool,
    ctx: &CommandContext,
) -> Result<String, RotateError> {
    let mut tx = pool.begin().await?;

    let old: Option<String> = sqlx::query_scalar!(
        r#"SELECT intake_token FROM organization WHERE id = $1 FOR UPDATE"#,
        ctx.organization_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let old = old.ok_or(RotateError::Database(sqlx::Error::RowNotFound))?;

    let mut new_token = mint_intake_token();
    while new_token == old {
        new_token = mint_intake_token();
    }

    sqlx::query!(
        r#"UPDATE organization SET intake_token = $2 WHERE id = $1"#,
        ctx.organization_id.0,
        new_token,
    )
    .execute(&mut *tx)
    .await?;

    let envelope = FactEnvelope::for_command(ctx, Utc::now());
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    sqlx::query!(
        r#"INSERT INTO intake_token_rotated
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

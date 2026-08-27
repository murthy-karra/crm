//! `RevokeInvitation` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, InvitationStatus};
use crate::domain::admin::AdminActor;
use crate::domain::envelope::{Actor, FactEnvelope};
use crate::domain::facts::{self, InvitationResolvedFact};
use crate::ids::{CorrelationId, InvitationId, OrganizationId};

pub struct RevokeInvitation {
    pub organization_id: OrganizationId,
    /// Stays bare `Uuid`, not `InvitationId` (hardening chunk N4): this
    /// command's callers are `routes/platform.rs` and
    /// `routes/organization.rs`, both outside this lane's ownership
    /// boundary (their path extractors — `TwoUuidPathIds`/`UuidPathId` —
    /// are shared, untyped, and explicitly deferred by the ladder doc's
    /// "Recorded residuals"). Wrapped explicitly, once, at the top of
    /// `revoke_invitation_attempt` below.
    pub invitation_id: Uuid,
}

/// `SELECT … FOR UPDATE` the invitation, scoped to the Organization; if
/// pending or expired → set `revoked / revoked`, fact
/// `invitation_resolved {revoked}`; if already accepted → `InvitationUsed`;
/// if already revoked → idempotent success; unknown or other-Organization
/// id → `NotFound` (docs/specs/SLICE_004.md §4). Actor paths: org-admin,
/// platform.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %cmd.organization_id,
        actor_id = %actor.actor_user_id,
        invitation_id = %cmd.invitation_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn revoke_invitation(
    pool: &PgPool,
    actor: AdminActor,
    cmd: RevokeInvitation,
) -> Result<(), AdminCommandError> {
    let result = revoke_invitation_attempt(pool, actor, cmd).await;
    match &result {
        Ok(()) => {
            tracing::Span::current().record("outcome", "revoked");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "revoke_invitation failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn revoke_invitation_attempt(
    pool: &PgPool,
    actor: AdminActor,
    cmd: RevokeInvitation,
) -> Result<(), AdminCommandError> {
    // The one crossing from the command's bare-`Uuid` boundary field into
    // the typed invitation-query/fact layer this lane owns — visible and
    // explicit, not an implicit `From`/`Into` (hardening chunk N4).
    let invitation_id = InvitationId::new(cmd.invitation_id);
    let mut tx = pool.begin().await?;

    let invitation = queries::lock_invitation_in_org(&mut tx, cmd.organization_id, invitation_id)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    let now = Utc::now();
    match invitation.status(now) {
        InvitationStatus::Accepted => return Err(AdminCommandError::InvitationUsed),
        InvitationStatus::Revoked => {
            // Idempotent: revoking an already-revoked invitation succeeds
            // with no additional fact.
            tx.commit().await?;
            return Ok(());
        }
        InvitationStatus::Pending | InvitationStatus::Expired => {}
    }

    queries::revoke_invitation_row(&mut tx, invitation_id).await?;

    let envelope = FactEnvelope {
        organization_id: cmd.organization_id,
        actor: Actor::User(actor.actor_user_id),
        on_behalf_of_user_id: None,
        origin: actor.origin,
        occurred_at: now,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
        causation_id: None,
    };
    facts::insert_invitation_resolved(
        &mut tx,
        &envelope,
        InvitationResolvedFact {
            invitation_id,
            outcome: "revoked",
        },
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

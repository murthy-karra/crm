//! `RevokeInvitation` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, InvitationStatus};
use crate::domain::admin::AdminActor;
use crate::domain::envelope::{ActorKind, FactEnvelope};
use crate::domain::facts::{self, InvitationResolvedFact};
use crate::ids::OrganizationId;

pub struct RevokeInvitation {
    pub organization_id: OrganizationId,
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
    let mut tx = pool.begin().await?;

    let invitation =
        queries::lock_invitation_in_org(&mut tx, cmd.organization_id, cmd.invitation_id)
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

    queries::revoke_invitation_row(&mut tx, cmd.invitation_id).await?;

    let envelope = FactEnvelope {
        organization_id: cmd.organization_id,
        actor_kind: ActorKind::User,
        actor_user_id: Some(actor.actor_user_id),
        on_behalf_of_user_id: None,
        origin: actor.origin,
        occurred_at: now,
        correlation_id: Uuid::new_v4(),
        causation_id: None,
    };
    facts::insert_invitation_resolved(
        &mut tx,
        &envelope,
        InvitationResolvedFact {
            invitation_id: cmd.invitation_id,
            outcome: "revoked",
        },
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

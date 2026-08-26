//! `SetMemberStatus` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, MemberView};
use crate::domain::admin::{AdminActor, MembershipStatus, Role};
use crate::domain::envelope::{ActorKind, FactEnvelope};
use crate::domain::facts::{self, MembershipChangedFact};
use crate::ids::OrganizationId;
use crate::realtime::Publisher;

pub struct SetMemberStatus {
    pub organization_id: OrganizationId,
    pub user_id: Uuid,
    pub status: MembershipStatus,
}

/// Same advisory lock as `ChangeMemberRole`; `NotFound` as above; no-op if
/// unchanged. Deactivating the last active admin → `LastAdmin`. On
/// `inactive`: revokes every session this member holds for this
/// Organization inside the same transaction, then — after commit —
/// disconnects their realtime connection (best-effort; `warn` on failure).
/// Fact `membership_changed {from_status, to_status, reason:
/// deactivate|reactivate}` (docs/specs/SLICE_004.md §4, §6). Actor path:
/// org-admin only.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %cmd.organization_id,
        actor_id = %actor.actor_user_id,
        target_user_id = %cmd.user_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn set_member_status(
    pool: &PgPool,
    publisher: &Publisher,
    actor: AdminActor,
    cmd: SetMemberStatus,
) -> Result<MemberView, AdminCommandError> {
    let result = set_member_status_attempt(pool, publisher, actor, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "changed");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "set_member_status failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn set_member_status_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    actor: AdminActor,
    cmd: SetMemberStatus,
) -> Result<MemberView, AdminCommandError> {
    let mut tx = pool.begin().await?;

    queries::acquire_admin_lock(&mut tx, cmd.organization_id).await?;

    let current = queries::member_role_status(&mut tx, cmd.organization_id, cmd.user_id)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    let mut deactivated = false;

    if current.status != cmd.status {
        if current.role == Role::Admin
            && current.status == MembershipStatus::Active
            && cmd.status == MembershipStatus::Inactive
        {
            let remaining =
                queries::count_active_admins_excluding(&mut tx, cmd.organization_id, cmd.user_id)
                    .await?;
            if remaining == 0 {
                return Err(AdminCommandError::LastAdmin);
            }
        }

        queries::update_membership_status(&mut tx, cmd.organization_id, cmd.user_id, cmd.status)
            .await?;

        let reason = if cmd.status == MembershipStatus::Inactive {
            deactivated = true;
            "deactivate"
        } else {
            "reactivate"
        };

        if deactivated {
            queries::revoke_sessions_for_member(&mut tx, cmd.organization_id, cmd.user_id).await?;
        }

        let envelope = FactEnvelope {
            organization_id: cmd.organization_id,
            actor_kind: ActorKind::User,
            actor_user_id: Some(actor.actor_user_id),
            on_behalf_of_user_id: None,
            origin: actor.origin,
            occurred_at: Utc::now(),
            correlation_id: Uuid::new_v4(),
            causation_id: None,
        };
        facts::insert_membership_changed(
            &mut tx,
            &envelope,
            MembershipChangedFact {
                user_id: cmd.user_id,
                from_role: Some(current.role.as_str()),
                to_role: current.role.as_str(),
                from_status: Some(current.status.as_str()),
                to_status: cmd.status.as_str(),
                reason,
            },
        )
        .await?;
    }

    let member = queries::member_view(&mut tx, cmd.organization_id, cmd.user_id)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    tx.commit().await?;

    if deactivated {
        publisher.disconnect_user(cmd.user_id).await;
    }

    Ok(member)
}

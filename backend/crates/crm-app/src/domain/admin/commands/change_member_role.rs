//! `ChangeMemberRole` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, MemberView};
use crate::domain::admin::{AdminActor, MembershipStatus, Role};
use crate::domain::envelope::{Actor, FactEnvelope};
use crate::domain::facts::{self, MembershipChangedFact};
use crate::ids::{OrganizationId, UserId};

pub struct ChangeMemberRole {
    pub organization_id: OrganizationId,
    pub user_id: UserId,
    pub role: Role,
}

/// Under the per-Organization `admin:` advisory lock: `NotFound` if the
/// target is not a member of this Organization; no-op success (no fact) if
/// already that role. Promoting an inactive member is allowed and does not
/// count toward the last-active-admin invariant. Demoting the last active
/// admin → `LastAdmin`. Fact `membership_changed {from_role, to_role,
/// reason: promote|demote}` (docs/specs/SLICE_004.md §4). Actor paths:
/// org-admin (any direction), platform (promote only — enforced by the
/// router, not here), CLI.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %cmd.organization_id,
        actor_id = %actor.actor_user_id,
        target_user_id = %cmd.user_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn change_member_role(
    pool: &PgPool,
    actor: AdminActor,
    cmd: ChangeMemberRole,
) -> Result<MemberView, AdminCommandError> {
    let result = change_member_role_attempt(pool, actor, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "changed");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "change_member_role failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn change_member_role_attempt(
    pool: &PgPool,
    actor: AdminActor,
    cmd: ChangeMemberRole,
) -> Result<MemberView, AdminCommandError> {
    let mut tx = pool.begin().await?;

    queries::acquire_admin_lock(&mut tx, cmd.organization_id).await?;

    let current = queries::member_role_status(&mut tx, cmd.organization_id, cmd.user_id)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    if current.role != cmd.role {
        // Only demoting an active admin can ever trip the invariant:
        // promotion never reduces the active-admin count, and an inactive
        // admin was never counted in the first place
        // (docs/specs/SLICE_004.md §4).
        if current.role == Role::Admin
            && current.status == MembershipStatus::Active
            && cmd.role == Role::Member
        {
            let remaining =
                queries::count_active_admins_excluding(&mut tx, cmd.organization_id, cmd.user_id)
                    .await?;
            if remaining == 0 {
                return Err(AdminCommandError::LastAdmin);
            }
        }

        queries::update_membership_role(&mut tx, cmd.organization_id, cmd.user_id, cmd.role)
            .await?;

        let reason = if cmd.role == Role::Admin {
            "promote"
        } else {
            "demote"
        };
        let envelope = FactEnvelope {
            organization_id: cmd.organization_id,
            actor: Actor::User(actor.actor_user_id),
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
                to_role: cmd.role.as_str(),
                from_status: Some(current.status.as_str()),
                to_status: current.status.as_str(),
                reason,
            },
        )
        .await?;
    }

    let member = queries::member_view(&mut tx, cmd.organization_id, cmd.user_id)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    tx.commit().await?;
    Ok(member)
}

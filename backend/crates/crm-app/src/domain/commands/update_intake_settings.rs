//! Existing routing-setting semantics inside the shared guarded command layer.
use super::CommandError;
use crate::{
    domain::{admin::queries, envelope::CommandContext, intake::IntakeRoutingMode},
    ids::UserId,
};
use sqlx::PgPool;

pub struct UpdateIntakeSettings {
    pub mode: IntakeRoutingMode,
    pub assignee: Option<UserId>,
}
pub async fn update_intake_settings(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateIntakeSettings,
) -> Result<(), CommandError> {
    let mut tx = crate::auth::workspace::begin(pool, ctx.organization_id).await?;
    let current_admin=sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' AND role='admin' FOR SHARE").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).fetch_optional(&mut *tx).await?.is_some();
    if !current_admin {
        return Err(CommandError::Forbidden);
    }
    let valid = match (cmd.mode, cmd.assignee) {
        (IntakeRoutingMode::DefaultAssignee, None) => false,
        (_, None) => true,
        (mode, Some(user)) => {
            queries::is_active_member(&mut tx, ctx.organization_id, user).await?
                || (!matches!(mode, IntakeRoutingMode::DefaultAssignee)
                    && queries::intake_default_assignee_user_id(&mut tx, ctx.organization_id)
                        .await?
                        == Some(user))
        }
    };
    if !valid {
        return Err(CommandError::InvalidAssignee);
    }
    queries::update_intake_routing_settings(&mut tx, ctx.organization_id, cmd.mode, cmd.assignee)
        .await?;
    tx.commit().await?;
    Ok(())
}

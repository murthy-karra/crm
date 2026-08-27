//! `AcceptInvitation` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::password;
use crate::domain::admin::commands::token;
use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, InvitationStatus};
use crate::domain::admin::{validation, MembershipStatus, Role};
use crate::domain::envelope::{CommandContext, FactEnvelope, Origin};
use crate::domain::facts::{self, InvitationResolvedFact, MembershipChangedFact};
use crate::ids::{CorrelationId, OrganizationId, UserId};

pub struct AcceptInvitation {
    pub token: String,
    pub display_name: String,
    pub password: String,
    /// `WebSession` from the public route (docs/specs/SLICE_004.md §4).
    pub origin: Origin,
}

pub struct AcceptInvitationOutcome {
    pub user_id: UserId,
    pub email: String,
    pub display_name: String,
    pub organization_id: OrganizationId,
    pub organization_name: String,
    pub role: Role,
}

/// Hash token; read the invitation without a lock and validate state first
/// (`NotFound` if absent or revoked, `InvitationExpired` if expired,
/// `InvitationUsed` if accepted; `InvitationNotAcceptable` if an
/// `app_user` with that email exists — generic) and validate display name
/// and password; only then Argon2id under `spawn_blocking` (so a dead
/// token never costs a hash); then open the transaction, re-check every
/// condition under `FOR UPDATE`. Insert `app_user`, `local_credential`,
/// `organization_membership {role, status: active}`; facts
/// `invitation_resolved {accepted}` and `membership_changed {to_role,
/// to_status: active, reason: invitation}` with `actor_user_id` = the new
/// user. `CommandContext::from_auth` does not apply — the context is built
/// after the `app_user` insert, inside the transaction
/// (docs/specs/SLICE_004.md §4). Actor path: the public accept route.
#[tracing::instrument(skip_all, fields(outcome = tracing::field::Empty))]
pub async fn accept_invitation(
    pool: &PgPool,
    cmd: AcceptInvitation,
) -> Result<AcceptInvitationOutcome, AdminCommandError> {
    let result = accept_invitation_attempt(pool, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "accepted");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "accept_invitation failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

/// Maps an `InvitationRow`'s derived status to the corresponding error,
/// `None` for `Pending` (docs/specs/SLICE_004.md §4, §9).
fn status_error(status: InvitationStatus) -> Option<AdminCommandError> {
    match status {
        InvitationStatus::Pending => None,
        InvitationStatus::Expired => Some(AdminCommandError::InvitationExpired),
        InvitationStatus::Accepted => Some(AdminCommandError::InvitationUsed),
        // A revoked invitation is indistinguishable from an unknown one —
        // both are 404, never disclosing that a token once existed.
        InvitationStatus::Revoked => Some(AdminCommandError::NotFound),
    }
}

async fn accept_invitation_attempt(
    pool: &PgPool,
    cmd: AcceptInvitation,
) -> Result<AcceptInvitationOutcome, AdminCommandError> {
    if !token::is_valid_format(&cmd.token) {
        return Err(AdminCommandError::NotFound);
    }
    let token_hash = token::hash(&cmd.token);

    // --- Pre-transaction validation: cheapest and most-likely-to-fail
    // checks first, so a dead or malformed token never reaches Argon2id
    // (docs/specs/SLICE_004.md §4). ---------------------------------------
    let mut conn = pool.acquire().await?;
    let preview = queries::find_invitation_by_token_hash(&mut conn, &token_hash)
        .await?
        .ok_or(AdminCommandError::NotFound)?;

    let now = Utc::now();
    if let Some(err) = status_error(preview.status(now)) {
        return Err(err);
    }

    if queries::app_user_id_by_email(&mut conn, &preview.email)
        .await?
        .is_some()
    {
        return Err(AdminCommandError::InvitationNotAcceptable);
    }
    drop(conn);

    let display_name = validation::validate_display_name(&cmd.display_name)?;
    validation::validate_password(&cmd.password)?;

    let candidate = cmd.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || password::hash_password(&candidate))
        .await
        .map_err(|_| AdminCommandError::Crypto)?
        .map_err(|_| AdminCommandError::Crypto)?;

    // --- Transaction: re-check every condition under FOR UPDATE
    // (docs/specs/SLICE_004.md §4, §9 accept-race handling). --------------
    let mut tx = pool.begin().await?;

    let invitation = queries::lock_invitation_by_token_hash(&mut tx, &token_hash)
        .await?
        .ok_or(AdminCommandError::NotFound)?;
    let now = Utc::now();
    if let Some(err) = status_error(invitation.status(now)) {
        return Err(err);
    }

    let user_id = match queries::insert_app_user(&mut tx, &invitation.email, &display_name).await {
        Ok(id) => id,
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            // Race: the same email accepted a different Organization's
            // invitation concurrently (docs/specs/SLICE_004.md §9).
            return Err(AdminCommandError::InvitationNotAcceptable);
        }
        Err(err) => return Err(err.into()),
    };

    queries::insert_local_credential(&mut tx, user_id, &password_hash).await?;
    queries::insert_membership(
        &mut tx,
        invitation.organization_id,
        user_id,
        invitation.role,
        MembershipStatus::Active,
    )
    .await?;
    queries::mark_invitation_accepted(&mut tx, invitation.id, user_id).await?;

    let ctx = CommandContext {
        organization_id: invitation.organization_id,
        actor_user_id: user_id,
        origin: cmd.origin,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let envelope = FactEnvelope::for_command(&ctx, now);

    facts::insert_invitation_resolved(
        &mut tx,
        &envelope,
        InvitationResolvedFact {
            invitation_id: invitation.id,
            outcome: "accepted",
        },
    )
    .await?;
    facts::insert_membership_changed(
        &mut tx,
        &envelope,
        MembershipChangedFact {
            user_id,
            from_role: None,
            to_role: invitation.role.as_str(),
            from_status: None,
            to_status: MembershipStatus::Active.as_str(),
            reason: "invitation",
        },
    )
    .await?;

    tx.commit().await?;

    Ok(AcceptInvitationOutcome {
        user_id,
        email: invitation.email.clone(),
        display_name,
        organization_id: invitation.organization_id,
        organization_name: invitation.organization_name.clone(),
        role: invitation.role,
    })
}

//! `IssueInvitation` (docs/specs/SLICE_004.md §4).

use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::token;
use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries::{self, InvitationStatus, InvitationView, InvitedByRef};
use crate::domain::admin::validation;
use crate::domain::admin::{AdminActor, Role};
use crate::domain::envelope::{ActorKind, FactEnvelope};
use crate::domain::facts::{self, InvitationIssuedFact, InvitationResolvedFact};

pub struct IssueInvitation {
    pub organization_id: Uuid,
    pub email: String,
    pub role: Role,
}

pub struct IssueInvitationOutcome {
    pub invitation: InvitationView,
    /// The raw token — returned once, here, and never again
    /// (docs/specs/SLICE_004.md §2).
    pub token: String,
    pub accept_path: String,
}

/// Normalize email (trim + lowercase; syntactic check). In one transaction:
/// if an open invitation exists for `(org, email)`, supersede it
/// (`revoked / superseded` + fact); insert the new row with
/// `expires_at = now() + invitation_ttl`; write `invitation_issued`.
/// Returns the raw token once. `AlreadyMember` if the email belongs to a
/// current member (any status) of the target Organization; existence
/// elsewhere is never disclosed (docs/specs/SLICE_004.md §4). A single
/// retry absorbs the partial-unique-index race between two concurrent
/// issues for the same `(org, email)`; a second failure is a database
/// error (503).
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %cmd.organization_id,
        actor_id = %actor.actor_user_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn issue_invitation(
    pool: &PgPool,
    actor: AdminActor,
    actor_display_name: &str,
    invitation_ttl: Duration,
    cmd: IssueInvitation,
) -> Result<IssueInvitationOutcome, AdminCommandError> {
    let email = match validation::normalize_email(&cmd.email) {
        Ok(email) => email,
        Err(err) => {
            tracing::Span::current().record("outcome", err.kind());
            return Err(err);
        }
    };

    let mut last_err = None;
    for attempt in 0..2 {
        match issue_invitation_attempt(
            pool,
            actor,
            actor_display_name,
            invitation_ttl,
            cmd.organization_id,
            &email,
            cmd.role,
        )
        .await
        {
            Ok(outcome) => {
                tracing::Span::current().record("outcome", "issued");
                return Ok(outcome);
            }
            Err(AdminCommandError::Database(sqlx::Error::Database(db_err)))
                if db_err.is_unique_violation() && attempt == 0 =>
            {
                // Partial-unique-index race (docs/specs/SLICE_004.md §9):
                // another concurrent issue for the same (org, email) won.
                // Retry the whole supersede-and-insert transaction once.
                last_err = Some(AdminCommandError::Database(sqlx::Error::Database(db_err)));
                continue;
            }
            Err(err) => {
                tracing::warn!(error_kind = err.kind(), "issue_invitation failed");
                tracing::Span::current().record("outcome", err.kind());
                return Err(err);
            }
        }
    }

    let err = last_err.expect("loop only exits via return or after recording last_err");
    tracing::warn!(
        error_kind = err.kind(),
        "issue_invitation failed after retry"
    );
    tracing::Span::current().record("outcome", err.kind());
    Err(err)
}

async fn issue_invitation_attempt(
    pool: &PgPool,
    actor: AdminActor,
    actor_display_name: &str,
    invitation_ttl: Duration,
    organization_id: Uuid,
    email: &str,
    role: Role,
) -> Result<IssueInvitationOutcome, AdminCommandError> {
    let mut tx = pool.begin().await?;

    if queries::is_member_by_email(&mut tx, organization_id, email).await? {
        return Err(AdminCommandError::AlreadyMember);
    }

    let now = Utc::now();
    let superseded = queries::find_open_invitation(&mut tx, organization_id, email).await?;
    if let Some(old_id) = superseded {
        queries::supersede_invitation(&mut tx, old_id).await?;
        let envelope = FactEnvelope {
            organization_id,
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
                invitation_id: old_id,
                outcome: "superseded",
            },
        )
        .await?;
    }

    let raw_token = token::generate();
    let token_hash = token::hash(&raw_token);
    let expires_at = now
        + chrono::Duration::from_std(invitation_ttl)
            .expect("invitation_ttl fits in chrono::Duration");

    let (invitation_id, created_at) = queries::insert_invitation(
        &mut tx,
        queries::NewInvitation {
            organization_id,
            email,
            role,
            token_hash: &token_hash,
            invited_by_user_id: actor.actor_user_id,
            expires_at,
        },
    )
    .await?;

    let envelope = FactEnvelope {
        organization_id,
        actor_kind: ActorKind::User,
        actor_user_id: Some(actor.actor_user_id),
        on_behalf_of_user_id: None,
        origin: actor.origin,
        occurred_at: now,
        correlation_id: Uuid::new_v4(),
        causation_id: None,
    };
    facts::insert_invitation_issued(
        &mut tx,
        &envelope,
        InvitationIssuedFact {
            invitation_id,
            role: role.as_str(),
            superseded_invitation_id: superseded,
        },
    )
    .await?;

    tx.commit().await?;

    Ok(IssueInvitationOutcome {
        invitation: InvitationView {
            id: invitation_id,
            email: email.to_string(),
            role,
            status: InvitationStatus::Pending,
            expires_at,
            created_at,
            invited_by: InvitedByRef {
                id: actor.actor_user_id,
                display_name: actor_display_name.to_string(),
            },
        },
        accept_path: format!("/invite/{raw_token}"),
        token: raw_token,
    })
}

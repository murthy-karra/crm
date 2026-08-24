//! `CreateOrganization` (docs/specs/SLICE_004.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries;
use crate::domain::admin::validation;
use crate::domain::admin::AdminActor;
use crate::domain::envelope::{ActorKind, FactEnvelope};
use crate::domain::facts;
use crate::domain::stage;

pub struct CreateOrganization {
    pub name: String,
}

/// Trim; 1–120 chars; insert `organization` (`status = 'active'` by column
/// default); `stage::seed_defaults`; fact `organization_created`. Name
/// collision (case-insensitive, `organization_name_lower_idx`) →
/// `OrganizationNameTaken` (docs/specs/SLICE_004.md §4). Actor paths:
/// platform, CLI.
#[tracing::instrument(
    skip_all,
    fields(
        actor_id = %actor.actor_user_id,
        intake_slug = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn create_organization(
    pool: &PgPool,
    actor: AdminActor,
    cmd: CreateOrganization,
) -> Result<queries::OrganizationRef, AdminCommandError> {
    let result = create_organization_attempt(pool, actor, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "created");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "create_organization failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    };
    result
}

async fn create_organization_attempt(
    pool: &PgPool,
    actor: AdminActor,
    cmd: CreateOrganization,
) -> Result<queries::OrganizationRef, AdminCommandError> {
    let name = validation::validate_organization_name(&cmd.name)?;

    let mut tx = pool.begin().await?;

    // Intake address (docs/specs/SLICE_007a.md §4): first free slug among
    // the candidates, chosen by a pre-select so a collision never aborts
    // the transaction; the unique index remains the last-resort guard.
    let candidates = validation::intake_slug_candidates(&name);
    let taken = queries::taken_intake_slugs(&mut tx, &candidates).await?;
    let intake_slug = candidates
        .into_iter()
        .find(|c| !taken.contains(c))
        // Nine collisions on one lossy slug is not a request-shape error;
        // surface it as the 503-class failure the route/CLI already map.
        .ok_or_else(|| {
            AdminCommandError::Database(sqlx::Error::Protocol(
                "intake slug candidates exhausted".to_string(),
            ))
        })?;
    let intake_token = validation::mint_intake_token();
    tracing::Span::current().record("intake_slug", intake_slug.as_str());

    let organization_id =
        match queries::insert_organization(&mut tx, &name, &intake_slug, &intake_token).await {
            Ok(id) => id,
            Err(sqlx::Error::Database(db_err))
                if db_err.is_unique_violation()
                    && db_err.constraint() == Some("organization_name_lower_idx") =>
            {
                return Err(AdminCommandError::OrganizationNameTaken);
            }
            // A lost race on the slug index (or any other violation) is a
            // plain database error, never misreported as a name clash.
            Err(err) => return Err(err.into()),
        };

    stage::seed_defaults(&mut tx, organization_id).await?;

    let envelope = FactEnvelope {
        organization_id,
        actor_kind: ActorKind::User,
        actor_user_id: Some(actor.actor_user_id),
        on_behalf_of_user_id: None,
        origin: actor.origin,
        occurred_at: Utc::now(),
        correlation_id: Uuid::new_v4(),
        causation_id: None,
    };
    facts::insert_organization_created(&mut tx, &envelope).await?;

    tx.commit().await?;

    Ok(queries::OrganizationRef {
        id: organization_id,
        name,
    })
}

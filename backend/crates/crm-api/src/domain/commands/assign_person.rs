//! `AssignPerson` (docs/specs/SLICE_002.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::commands::CommandError;
use crate::domain::envelope::{CommandContext, FactEnvelope};
use crate::domain::facts::{self, AssignmentChangedFact};
use crate::domain::person::model::PersonSummary;
use crate::domain::person::queries as person_queries;

pub struct AssignPerson {
    pub person_id: Uuid,
    /// `None` means unassign.
    pub assigned_user_id: Option<Uuid>,
}

/// One transaction; loads the Person through the scope (else
/// `PersonNotFound`), validates the target belongs to the Organization,
/// no-op success without a fact if unchanged, else `UPDATE person` + one
/// `assignment_changed` fact with `reason = 'manual'`
/// (docs/specs/SLICE_002.md §4). Returns `(summary, changed)`.
///
/// Public entry point wrapping `assign_person_attempt`: logs *why* a
/// failure occurred (by stable `CommandError::kind()` tag, never the
/// error's `Display`/`Debug` text — docs/specs/SLICE_002.md §8), which a
/// bare `?`-propagated error would otherwise leave with no server-side
/// signal beyond the HTTP status code.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn assign_person(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: AssignPerson,
) -> Result<(PersonSummary, bool), CommandError> {
    let result = assign_person_attempt(pool, ctx, cmd).await;
    match &result {
        Ok((_, changed)) => {
            tracing::Span::current()
                .record("outcome", if *changed { "changed" } else { "unchanged" });
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "assign_person failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn assign_person_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: AssignPerson,
) -> Result<(PersonSummary, bool), CommandError> {
    let mut tx = pool.begin().await?;

    let person = person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    if let Some(target) = cmd.assigned_user_id {
        let is_member =
            person_queries::is_organization_member(&mut tx, ctx.organization_id, target).await?;
        if !is_member {
            return Err(CommandError::InvalidAssignee);
        }
    }

    let changed = person.assigned_user_id != cmd.assigned_user_id;

    if changed {
        person_queries::update_assignment(
            &mut tx,
            cmd.person_id,
            ctx.organization_id,
            cmd.assigned_user_id,
        )
        .await?;

        let envelope = FactEnvelope::for_command(ctx, Utc::now());
        facts::insert_assignment_changed(
            &mut tx,
            &envelope,
            AssignmentChangedFact {
                person_id: cmd.person_id,
                from_user_id: person.assigned_user_id,
                to_user_id: cmd.assigned_user_id,
                reason: "manual",
            },
        )
        .await?;
    }

    let summary = person_queries::summary_by_id(&mut tx, ctx.organization_id, cmd.person_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    tx.commit().await?;
    Ok((summary, changed))
}

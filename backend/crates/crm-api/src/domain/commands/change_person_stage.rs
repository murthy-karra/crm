//! `ChangePersonStage` (docs/specs/SLICE_002.md §4).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::commands::CommandError;
use crate::domain::envelope::{CommandContext, FactEnvelope};
use crate::domain::facts::{self, StageChangedFact};
use crate::domain::person::model::PersonSummary;
use crate::domain::person::queries as person_queries;
use crate::domain::stage;
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

pub struct ChangePersonStage {
    pub person_id: Uuid,
    pub stage_id: Uuid,
}

/// One transaction; loads the Person through the scope (else
/// `PersonNotFound`), validates the target stage belongs to the
/// Organization (else `InvalidStage`), no-op success without a fact if
/// unchanged, else `UPDATE person` + one `stage_changed` fact with
/// `reason = 'manual'` (docs/specs/SLICE_002.md §4). Returns
/// `(summary, changed)`.
///
/// Public entry point wrapping `change_person_stage_attempt`: logs *why* a
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
pub async fn change_person_stage(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ChangePersonStage,
) -> Result<(PersonSummary, bool), CommandError> {
    let result = change_person_stage_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok((_, changed)) => {
            tracing::Span::current()
                .record("outcome", if *changed { "changed" } else { "unchanged" });
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "change_person_stage failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn change_person_stage_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ChangePersonStage,
) -> Result<(PersonSummary, bool), CommandError> {
    let mut tx = pool.begin().await?;

    let person = person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    let stage_valid = stage::exists(&mut tx, cmd.stage_id, ctx.organization_id).await?;
    if !stage_valid {
        return Err(CommandError::InvalidStage);
    }

    let changed = person.stage_id != cmd.stage_id;
    let occurred_at = Utc::now();

    if changed {
        person_queries::update_stage(&mut tx, cmd.person_id, ctx.organization_id, cmd.stage_id)
            .await?;

        let envelope = FactEnvelope::for_command(ctx, occurred_at);
        facts::insert_stage_changed(
            &mut tx,
            &envelope,
            StageChangedFact {
                person_id: cmd.person_id,
                from_stage_id: Some(person.stage_id),
                to_stage_id: cmd.stage_id,
                reason: "manual",
            },
        )
        .await?;
    }

    let summary = person_queries::summary_by_id(&mut tx, ctx.organization_id, cmd.person_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    tx.commit().await?;

    // An event only when changed (docs/specs/SLICE_003.md §4).
    if changed {
        let event = RealtimeEvent::person_changed(
            ctx.organization_id,
            occurred_at,
            ctx.correlation_id,
            cmd.person_id,
            PersonChange::StageChanged,
        );
        publisher
            .publish_after_commit(Publication::for_event(event))
            .await;
    }

    Ok((summary, changed))
}

//! `LogContactAttempt` (docs/specs/SLICE_003.md §4, D-022). Not idempotent
//! by design (a double submit is two facts; the dialog disables its button
//! while pending, and a client attempt id is `LATER` per the spec).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::commands::CommandError;
use crate::domain::envelope::{CommandContext, FactEnvelope};
use crate::domain::facts::{self, ContactAttemptedFact};
use crate::domain::person::model::PersonSummary;
use crate::domain::person::queries as person_queries;
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

/// `"call" | "text" | "email" | "other"` (docs/specs/SLICE_003.md §2, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactChannel {
    Call,
    Text,
    Email,
    Other,
}

impl ContactChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            ContactChannel::Call => "call",
            ContactChannel::Text => "text",
            ContactChannel::Email => "email",
            ContactChannel::Other => "other",
        }
    }
}

/// `"reached" | "no_answer" | "left_message" | "sent"`
/// (docs/specs/SLICE_003.md §2, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactOutcome {
    Reached,
    NoAnswer,
    LeftMessage,
    Sent,
}

impl ContactOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            ContactOutcome::Reached => "reached",
            ContactOutcome::NoAnswer => "no_answer",
            ContactOutcome::LeftMessage => "left_message",
            ContactOutcome::Sent => "sent",
        }
    }
}

pub struct LogContactAttempt {
    pub person_id: Uuid,
    pub channel: ContactChannel,
    pub outcome: ContactOutcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactAttemptRef {
    pub id: Uuid,
    pub channel: ContactChannel,
    pub outcome: ContactOutcome,
    pub occurred_at: DateTime<Utc>,
}

/// One transaction: locks the Person through the Organization (else
/// `PersonNotFound`), inserts one `contact_attempted` fact with
/// `occurred_at = Utc::now()` and an envelope from `CommandContext`
/// (`actor_kind = 'user'`, `origin = 'web_session'`, fresh
/// `correlation_id`), commits, then publishes
/// `person.changed{contact_attempted}` (docs/specs/SLICE_003.md §4).
/// Allowed for any member on any Person of the Organization (no-roles
/// model, §7) — the caller only needs a valid `CommandContext`, not
/// assignment.
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
pub async fn log_contact_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: LogContactAttempt,
) -> Result<(PersonSummary, ContactAttemptRef), CommandError> {
    let result = log_contact_attempt_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "logged");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "log_contact_attempt failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn log_contact_attempt_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: LogContactAttempt,
) -> Result<(PersonSummary, ContactAttemptRef), CommandError> {
    let mut tx = pool.begin().await?;

    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    let occurred_at = Utc::now();
    let envelope = FactEnvelope::for_command(ctx, occurred_at);

    let fact_id = facts::insert_contact_attempted(
        &mut tx,
        &envelope,
        ContactAttemptedFact {
            person_id: cmd.person_id,
            channel: cmd.channel.as_str(),
            outcome: cmd.outcome.as_str(),
        },
    )
    .await?;

    let summary = person_queries::summary_by_id(&mut tx, ctx.organization_id, cmd.person_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;

    tx.commit().await?;

    let event = RealtimeEvent::person_changed(
        ctx.organization_id,
        occurred_at,
        ctx.correlation_id,
        cmd.person_id,
        PersonChange::ContactAttempted,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;

    Ok((
        summary,
        ContactAttemptRef {
            id: fact_id,
            channel: cmd.channel,
            outcome: cmd.outcome,
            occurred_at,
        },
    ))
}

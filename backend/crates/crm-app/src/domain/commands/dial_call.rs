//! `dial_call` (docs/specs/SLICE_006.md §3): caller only; the guarded
//! `UPDATE … SET dial_requested_at` (0 rows → `InvalidCallState`); then
//! spawns the dial task. Returns 202 with the call still `placing` — the
//! task owns `placing → ringing`.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::domain::commands::CallError;
use crate::domain::envelope::CommandContext;
use crate::domain::telephony::dial_task::{DialTask, DialTaskOutcome};
use crate::domain::telephony::queries::{self as call_queries, CallView};
use crate::realtime::Publisher;
use crate::telephony::Telephony;

/// The spawned task's handle is returned so a test can await the dial's
/// result; the route drops it (the task is detached by construction).
#[tracing::instrument(
    name = "call.dial",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = tracing::field::Empty,
        call_id = %call_id,
        person_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
        sip_status_class = tracing::field::Empty,
    )
)]
pub async fn dial_call(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    call_id: Uuid,
) -> Result<(CallView, JoinHandle<DialTaskOutcome>), CallError> {
    let result = dial_call_attempt(pool, publisher, telephony, ctx, call_id).await;
    match &result {
        Ok(_) => tracing::Span::current().record("outcome", "dial_requested"),
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "dial_call failed");
            tracing::Span::current().record("outcome", err.kind())
        }
    };
    result
}

async fn dial_call_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    call_id: Uuid,
) -> Result<(CallView, JoinHandle<DialTaskOutcome>), CallError> {
    let mut conn = pool.acquire().await?;
    let call = call_queries::call_by_id(&mut conn, ctx.organization_id, call_id)
        .await?
        .ok_or(CallError::CallNotFound)?;
    if call.caller_user_id != ctx.actor_user_id {
        return Err(CallError::Forbidden);
    }
    let span = tracing::Span::current();
    span.record(
        "correlation_id",
        tracing::field::display(call.correlation_id),
    );
    span.record("person_id", tracing::field::display(call.person_id));

    if !call_queries::mark_dial_requested(&mut conn, ctx.organization_id, call_id, Utc::now())
        .await?
    {
        return Err(CallError::InvalidCallState);
    }

    let view = call_queries::call_view_by_id(&mut conn, ctx.organization_id, call_id)
        .await?
        .ok_or(CallError::Corrupt)?;
    drop(conn);

    let handle = DialTask {
        pool: pool.clone(),
        publisher: publisher.clone(),
        telephony: Arc::clone(telephony),
        organization_id: ctx.organization_id,
        call_id,
        person_id: call.person_id,
        contact_method_id: call.contact_method_id,
        caller_user_id: call.caller_user_id,
    }
    .spawn();

    Ok((view, handle))
}

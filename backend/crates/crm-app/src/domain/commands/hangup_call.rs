//! `hangup_call` (docs/specs/SLICE_006.md §3): caller only; idempotent.
//! **Settle first** (row lock): `answered → ended{agent_hangup}`,
//! `placing|ringing → failed{cancelled}`, terminal → no-op; **then**
//! `provider.hangup` best-effort — including on an already-terminal call,
//! so a client retry after a 503 still cleans the room up. Settling before
//! deleting the room keeps a concurrent dial-task `ProviderError` from
//! winning and changing the recorded outcome.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;

use crate::domain::commands::CallError;
use crate::domain::envelope::CommandContext;
use crate::domain::telephony::queries::{self as call_queries, CallView};
use crate::domain::telephony::settle::transition_tag;
use crate::domain::telephony::{settle, Signal};
use crate::ids::CallId;
use crate::realtime::Publisher;
use crate::telephony::Telephony;

#[tracing::instrument(
    name = "call.hangup",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = tracing::field::Empty,
        call_id = %call_id,
        person_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn hangup_call(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    call_id: CallId,
) -> Result<CallView, CallError> {
    let result = hangup_call_attempt(pool, publisher, telephony, ctx, call_id).await;
    if let Err(err) = &result {
        tracing::warn!(error_kind = err.kind(), "hangup_call failed");
        tracing::Span::current().record("outcome", err.kind());
    }
    result
}

async fn hangup_call_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    call_id: CallId,
) -> Result<CallView, CallError> {
    let mut conn = pool.acquire().await?;
    let call = call_queries::call_by_id(&mut conn, ctx.organization_id, call_id)
        .await?
        .ok_or(CallError::CallNotFound)?;
    drop(conn);
    if call.caller_user_id != ctx.actor_user_id {
        return Err(CallError::Forbidden);
    }
    let span = tracing::Span::current();
    span.record(
        "correlation_id",
        tracing::field::display(call.correlation_id),
    );
    span.record("person_id", tracing::field::display(call.person_id));

    let outcome = settle(
        pool,
        publisher,
        ctx.organization_id,
        call_id,
        &Signal::AgentHangup,
        Utc::now(),
    )
    .await?
    .ok_or(CallError::CallNotFound)?;
    span.record("outcome", transition_tag(&outcome.transition));

    // Best-effort, even when the call was already terminal.
    if let Err(err) = telephony.provider.hangup(&call.provider_room).await {
        tracing::warn!(error_kind = err.kind(), "room delete failed");
    }

    let mut conn = pool.acquire().await?;
    call_queries::call_view_by_id(&mut conn, ctx.organization_id, call_id)
        .await?
        .ok_or(CallError::Corrupt)
}

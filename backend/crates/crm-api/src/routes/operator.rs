//! `POST /api/operator/turns` (docs/specs/SLICE_005.md §5, §7, §8, §9).
//! Stateless, non-streaming, bounded: one user message in, one reply out.
//! The turn runs in a spawned task holding both concurrency guards, so a
//! client disconnect neither leaks a slot nor skips the ledger row.

use std::time::Instant;

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::Instrument;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::operator::{record_turn, SqlxToolBackend, TurnRecord};
use crate::state::AppState;
use crm_operator::{
    HistoryMessage, HistoryRole, OperatorContext, ScreenContext, ScreenRoute, ToolCallRecord,
    TurnInput, TurnOutcome, WirePersonCard,
};

/// Generous: 14,000 chars of `\uXXXX`-escaped JSON is ~170 KB.
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_HISTORY_ITEMS: usize = 6;
const MAX_HISTORY_ITEM_CHARS: usize = 2000;
const MAX_HISTORY_TOTAL_CHARS: usize = 6000;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/operator/turns",
        post(post_turn).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnRequest {
    message: String,
    #[serde(default)]
    history: Vec<HistoryItem>,
    #[serde(default)]
    context: Option<ContextItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryItem {
    role: HistoryRole,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextItem {
    route: ScreenRoute,
    #[serde(default)]
    person_id: Option<Uuid>,
}

#[derive(Serialize)]
struct WireReferences {
    people: Vec<WirePersonCard>,
}

#[derive(Serialize)]
struct TurnResponse {
    turn_id: Uuid,
    reply: String,
    references: WireReferences,
    tool_calls: Vec<ToolCallRecord>,
    outcome: TurnOutcome,
}

/// §5 validation: `message` 1–2000 chars after trim; `history` ≤ 6 items,
/// each ≤ 2000 chars, ≤ 6000 total. Unknown fields anywhere are already
/// rejected by `deny_unknown_fields` (the Slice 001 probe style).
fn validate(req: TurnRequest) -> Result<TurnInput, ApiError> {
    let message = req.message.trim().to_string();
    let message_chars = message.chars().count();
    if message_chars == 0 || message_chars > MAX_MESSAGE_CHARS {
        return Err(ApiError::MalformedRequest);
    }
    if req.history.len() > MAX_HISTORY_ITEMS {
        return Err(ApiError::MalformedRequest);
    }
    let mut total = 0usize;
    for item in &req.history {
        let chars = item.content.chars().count();
        if chars > MAX_HISTORY_ITEM_CHARS {
            return Err(ApiError::MalformedRequest);
        }
        total += chars;
    }
    if total > MAX_HISTORY_TOTAL_CHARS {
        return Err(ApiError::MalformedRequest);
    }
    let screen = match req.context {
        Some(ctx) => ScreenContext {
            route: ctx.route,
            person_id: ctx.person_id,
        },
        None => ScreenContext::other(),
    };
    Ok(TurnInput {
        message,
        history: req
            .history
            .into_iter()
            .map(|h| HistoryMessage {
                role: h.role,
                content: h.content,
            })
            .collect(),
        screen,
    })
}

async fn post_turn(
    State(state): State<AppState>,
    auth: AuthContext,
    body: Result<Json<TurnRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    // `AuthContext` is resolved once per request; a membership deactivated
    // mid-turn still runs read-only tools for at most `turn_timeout`. Its
    // next request is 401 like every other tenant route.
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let input = validate(req)?;

    let runtime = state.operator.clone().ok_or(ApiError::OperatorDisabled)?;
    let pool = state.db.clone().ok_or(ApiError::Unavailable)?;

    // Fail fast — never queue (§7). Rejections are a span event only; no
    // ledger row (§2 PII rule: they never became a turn).
    let slot = runtime.try_acquire(auth.actor_user_id).map_err(|_| {
        tracing::info!(
            organization_id = %auth.active_organization_id,
            actor_id = %auth.actor_user_id,
            "operator turn rejected: busy"
        );
        ApiError::OperatorBusy
    })?;

    let ctx = OperatorContext {
        actor_user_id: auth.actor_user_id,
        organization_id: auth.active_organization_id,
        actor_display_name: auth.actor_display_name.clone(),
        turn_id: Uuid::new_v4(),
        now: Utc::now(),
    };
    let context_route = input.screen.route;

    let span = tracing::info_span!(
        "operator.turn",
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.turn_id,
        provider = runtime.service.provider().name(),
        model = runtime.service.provider().model(),
        outcome = tracing::field::Empty,
        model_call_count = tracing::field::Empty,
        tool_call_count = tracing::field::Empty,
        prompt_tokens = tracing::field::Empty,
        completion_tokens = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    let task = tokio::spawn(
        async move {
            // Both guards live exactly as long as this task (§7).
            let _slot = slot;
            let started = Instant::now();
            let backend = SqlxToolBackend::new(pool.clone());
            let output = runtime.service.run_turn(&ctx, &backend, input).await;
            let completed_at = Utc::now();

            let span = tracing::Span::current();
            span.record("outcome", output.outcome.as_str());
            span.record("model_call_count", output.model_call_count);
            span.record("tool_call_count", output.tool_calls.len());
            if let Some(n) = output.usage.prompt_tokens {
                span.record("prompt_tokens", n);
            }
            if let Some(n) = output.usage.completion_tokens {
                span.record("completion_tokens", n);
            }
            span.record("latency_ms", started.elapsed().as_millis() as u64);

            let provider = runtime.service.provider();
            if let Err(err) = record_turn(
                &pool,
                TurnRecord {
                    ctx: &ctx,
                    output: &output,
                    provider: provider.name(),
                    model: provider.model(),
                    completed_at,
                    context_route,
                },
            )
            .await
            {
                // Observability, not truth (§9): the reply still returns.
                tracing::error!(
                    turn_id = %ctx.turn_id,
                    error = %err,
                    "operator ledger insert failed"
                );
            }

            (ctx.turn_id, output)
        }
        .instrument(span),
    );

    let (turn_id, output) = task.await.map_err(|_| ApiError::InternalError)?;

    if !output.outcome.is_reply() {
        return Err(ApiError::OperatorUnavailable);
    }

    let response = TurnResponse {
        turn_id,
        reply: output.reply.unwrap_or_default(),
        references: WireReferences {
            people: output
                .references
                .people
                .iter()
                .map(|card| card.to_wire())
                .collect(),
        },
        tool_calls: output.tool_calls,
        outcome: output.outcome,
    };
    Ok(Json(response).into_response())
}

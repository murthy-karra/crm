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
use crate::domain::commands::{self, StartCall};
use crate::domain::envelope::CommandContext;
use crate::error::ApiError;
use crate::operator::{record_turn, SqlxToolBackend, TurnRecord};
use crate::state::AppState;
use crm_operator::{
    HistoryMessage, HistoryRole, OperatorContext, ProposalView, ScreenContext, ScreenRoute,
    ToolCallRecord, TurnInput, TurnOutcome, WirePersonCard,
};

/// Generous: 14,000 chars of `\uXXXX`-escaped JSON is ~170 KB.
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_HISTORY_ITEMS: usize = 6;
const MAX_HISTORY_ITEM_CHARS: usize = 2000;
const MAX_HISTORY_TOTAL_CHARS: usize = 6000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/operator/turns",
            post(post_turn).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
        )
        .route(
            "/api/operator/proposals/{id}/confirm",
            post(confirm_proposal),
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

/// `proposal` on the wire (docs/specs/SLICE_006b.md §4): the drawer's
/// card renders from this object only, never from model prose.
#[derive(Serialize)]
struct WireProposal {
    id: Uuid,
    kind: &'static str,
    person: WirePersonCard,
    phone: String,
    contact_method_id: Uuid,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl WireProposal {
    fn from_view(view: &ProposalView) -> Self {
        Self {
            id: view.proposal_id,
            kind: "start_call",
            person: view.person.to_wire(),
            phone: view.phone.as_str().to_string(),
            contact_method_id: view.contact_method_id,
            expires_at: view.expires_at,
        }
    }
}

#[derive(Serialize)]
struct TurnResponse {
    turn_id: Uuid,
    reply: String,
    references: WireReferences,
    tool_calls: Vec<ToolCallRecord>,
    proposal: Option<WireProposal>,
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
        // crm-operator keeps a bare `Uuid` at the tool seam (D-028 §5
        // crate fence) — convert explicitly at this boundary (hardening
        // chunks N1/N2).
        actor_user_id: auth.actor_user_id.as_uuid(),
        organization_id: auth.active_organization_id.as_uuid(),
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
            let backend = SqlxToolBackend::new(pool.clone(), runtime.proposal_ttl());
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
        proposal: output.proposal.as_ref().map(WireProposal::from_view),
        outcome: output.outcome,
    };
    Ok(Json(response).into_response())
}

/// `POST /api/operator/proposals/{id}/confirm` (docs/specs/SLICE_006b.md
/// §4): the human click that executes a proposed call. Deterministic and
/// model-free — needs no operator runtime, takes no turn semaphore, works
/// with the provider down. Claim-then-execute: the claim serializes
/// double-confirms on the row; a claimed row is consumed forever (a crash
/// before finalize leaves `claimed`, which reads as consumed).
#[tracing::instrument(
    name = "operator.proposal_confirm",
    skip_all,
    fields(
        proposal_id = %proposal_id,
        organization_id = %auth.active_organization_id,
        actor_id = %auth.actor_user_id,
        turn_id = tracing::field::Empty,
        call_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn confirm_proposal(
    State(state): State<AppState>,
    axum::extract::Path(proposal_id): axum::extract::Path<Uuid>,
    auth: AuthContext,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let span = tracing::Span::current();

    // 1. Claim (single-use gate). Scoped to (org, actor): a foreign or
    //    other-user proposal is indistinguishable from a nonexistent one.
    let claimed = sqlx::query!(
        r#"UPDATE operator_proposal
           SET status = 'claimed'
           WHERE id = $1 AND organization_id = $2 AND actor_user_id = $3
             AND status = 'proposed' AND expires_at > now()
           RETURNING person_id, contact_method_id, turn_id"#,
        proposal_id,
        auth.active_organization_id.0,
        auth.actor_user_id.0,
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let Some(row) = claimed else {
        // 2. Distinguish 404 / consumed / expired with one scoped read.
        //    Consumed beats expired: any row no longer `proposed` was used.
        let probe = sqlx::query!(
            r#"SELECT status, call_id FROM operator_proposal
               WHERE id = $1 AND organization_id = $2 AND actor_user_id = $3"#,
            proposal_id,
            auth.active_organization_id.0,
            auth.actor_user_id.0,
        )
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Unavailable)?;
        return match probe {
            None => {
                span.record("outcome", "not_found");
                Err(ApiError::NotFound)
            }
            Some(p) if p.status != "proposed" => {
                span.record("outcome", "proposal_consumed");
                Err(ApiError::ProposalConsumed { call_id: p.call_id })
            }
            Some(_) => {
                span.record("outcome", "proposal_expired");
                Err(ApiError::ProposalExpired)
            }
        };
    };
    span.record("turn_id", tracing::field::display(row.turn_id));

    // 3. Execute the exact command the Call button uses, as the session
    //    user, with the turn id as the correlation id (SLICE_006b §3).
    let telephony = match state.telephony.as_ref() {
        Some(t) => t,
        None => {
            finalize_failed(pool, proposal_id, "telephony_disabled", None).await;
            span.record("outcome", "telephony_disabled");
            return Err(ApiError::TelephonyDisabled);
        }
    };
    let ctx = CommandContext::for_operator(&auth, row.turn_id);
    match commands::start_call(
        pool,
        &state.publisher,
        telephony,
        &ctx,
        StartCall {
            person_id: row.person_id,
            contact_method_id: row.contact_method_id,
        },
    )
    .await
    {
        Ok((call, grant)) => {
            span.record("call_id", tracing::field::display(call.id));
            span.record("outcome", "confirmed");
            // Finalize; the call exists either way — a failure here only
            // costs the receipt row's final state, never the call.
            let finalized = sqlx::query!(
                r#"UPDATE operator_proposal
                   SET status = 'confirmed', call_id = $2, confirmed_at = now()
                   WHERE id = $1 AND status = 'claimed'"#,
                proposal_id,
                call.id,
            )
            .execute(pool)
            .await;
            if let Err(err) = finalized {
                tracing::error!(error = %err, "proposal finalize failed after start_call");
            }
            Ok((
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "call": call,
                    "join": {
                        "url": grant.url,
                        "token": grant.token.into_string(),
                        "room": grant.room,
                    },
                })),
            )
                .into_response())
        }
        Err(err) => {
            let kind = err.kind();
            span.record("outcome", kind);
            // SLICE_006b §2: when the command created a call row that
            // settled failed (e.g. telephony_unavailable), the receipt
            // keeps it. The row is found through the correlation chain —
            // this execution used `correlation_id = turn_id`, and a
            // turn has at most one proposal, so at most one call matches.
            let failed_call_id = sqlx::query_scalar!(
                r#"SELECT id FROM call
                   WHERE organization_id = $1 AND correlation_id = $2"#,
                auth.active_organization_id.0,
                row.turn_id,
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
            finalize_failed(pool, proposal_id, kind, failed_call_id).await;
            Err(ApiError::from(err))
        }
    }
}

/// Best-effort `failed` finalization; the command's error is the truth.
async fn finalize_failed(
    pool: &sqlx::PgPool,
    proposal_id: Uuid,
    kind: &str,
    call_id: Option<Uuid>,
) {
    let result = sqlx::query!(
        r#"UPDATE operator_proposal
           SET status = 'failed', failure_code = $2, call_id = $3
           WHERE id = $1 AND status = 'claimed'"#,
        proposal_id,
        kind,
        call_id,
    )
    .execute(pool)
    .await;
    if let Err(err) = result {
        tracing::error!(error = %err, "proposal failed-finalize failed");
    }
}

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
use crate::domain::task::{self, CreateTask, TaskKind};
use crate::error::ApiError;
use crate::ids::{CallId, ContactMethodId, PersonId, ProposalId, TaskId, TurnId, UserId};
use crate::operator::{record_turn, SqlxToolBackend, TurnRecord};
use crate::state::AppState;
use crm_operator::{
    HistoryMessage, HistoryRole, OperatorContext, ScreenContext, ScreenRoute, TaskReceiptView,
    ToolCallRecord, TurnInput, TurnOutcome, TurnProposal, WirePersonCard,
};

/// docs/specs/SLICE_018.md §3: the client's `-new Date().getTimezoneOffset()`
/// sign-flipped, bounds matching the D-054 §3 client rule (14h either way).
const UTC_OFFSET_MIN: i32 = -840;
const UTC_OFFSET_MAX: i32 = 840;

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
    /// docs/specs/SLICE_018.md §3, §5: additive optional; absent or null =
    /// unknown. Client-supplied, harmless data (AGENTS §5.2 stays
    /// satisfied: it never becomes trusted identity or authorization
    /// context, only the `create_task` due-instant composition's time
    /// zone and the prompt's local-time line).
    #[serde(default)]
    utc_offset_minutes: Option<i32>,
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
    person_id: Option<PersonId>,
}

#[derive(Serialize)]
struct WireReferences {
    people: Vec<WirePersonCard>,
}

/// `assignee` on a `create_task` proposal card (docs/specs/SLICE_018.md
/// §5).
#[derive(Serialize)]
struct WireMemberRef {
    id: UserId,
    display_name: String,
}

/// `proposal` on the wire (docs/specs/SLICE_006b.md §4; docs/specs/
/// SLICE_018.md §5): a `kind`-discriminated union. The drawer's card
/// renders from this object only, never from model prose.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireProposal {
    StartCall {
        // Typed even though `ProposalView` hands over bare `Uuid`s (the
        // D-028 fence): the two wraps in `from_turn_proposal` are the one
        // place a transposition could slip through serialize-only code
        // (reviewer N4 MINOR-1). Wire-identical via serde transparency.
        id: ProposalId,
        person: WirePersonCard,
        phone: String,
        contact_method_id: ContactMethodId,
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    CreateTask {
        id: ProposalId,
        person: WirePersonCard,
        title: String,
        task_kind: String,
        due_at: Option<chrono::DateTime<chrono::Utc>>,
        assignee: WireMemberRef,
        expires_at: chrono::DateTime<chrono::Utc>,
    },
}

impl WireProposal {
    fn from_turn_proposal(proposal: &TurnProposal) -> Self {
        match proposal {
            TurnProposal::StartCall(view) => WireProposal::StartCall {
                id: ProposalId::new(view.proposal_id),
                person: view.person.to_wire(),
                phone: view.phone.as_str().to_string(),
                contact_method_id: ContactMethodId::new(view.contact_method_id),
                expires_at: view.expires_at,
            },
            TurnProposal::CreateTask(view) => WireProposal::CreateTask {
                id: ProposalId::new(view.proposal_id),
                person: view.person.to_wire(),
                title: view.title.as_str().to_string(),
                task_kind: view.kind.clone(),
                due_at: view.due_at,
                assignee: WireMemberRef {
                    id: UserId::new(view.assignee.id),
                    display_name: view.assignee.display_name.clone(),
                },
                expires_at: view.expires_at,
            },
        }
    }
}

/// `receipt` on the wire (docs/specs/SLICE_018.md §5): present only on 200
/// outcomes. Deliberately narrower than `TaskReceiptView` — no
/// `completed_by_display_name` (not part of the frozen wire shape).
#[derive(Serialize)]
struct WireReceipt {
    kind: &'static str,
    task_id: TaskId,
    person: WirePersonCard,
    title: String,
    task_kind: String,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl WireReceipt {
    fn from_view(view: &TaskReceiptView) -> Self {
        Self {
            kind: "complete_task",
            task_id: TaskId::new(view.task_id),
            person: view.person.to_wire(),
            title: view.title.as_str().to_string(),
            task_kind: view.kind.clone(),
            due_at: view.due_at,
            completed_at: view.completed_at,
        }
    }
}

#[derive(Serialize)]
struct TurnResponse {
    turn_id: TurnId,
    reply: String,
    references: WireReferences,
    tool_calls: Vec<ToolCallRecord>,
    proposal: Option<WireProposal>,
    receipt: Option<WireReceipt>,
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
    // docs/specs/SLICE_018.md §3, §5: −840..=840; absent/null already
    // decoded to `None` above. A non-integer (a float, a string) never
    // reaches here — it fails `Json<TurnRequest>` deserialization first,
    // the existing `JsonRejection -> MalformedRequest` path in `post_turn`.
    if let Some(offset) = req.utc_offset_minutes {
        if !(UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(&offset) {
            return Err(ApiError::MalformedRequest);
        }
    }
    let screen = match req.context {
        Some(ctx) => ScreenContext {
            route: ctx.route,
            // crm-operator keeps a bare `Uuid` at the tool seam (D-028 §5
            // crate fence) — convert explicitly at this boundary
            // (hardening chunk N3, mirroring N1/N2's org/user seam
            // conversions elsewhere in this file).
            person_id: ctx.person_id.map(PersonId::as_uuid),
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
        utc_offset_minutes: req.utc_offset_minutes,
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
    // docs/specs/SLICE_018.md §2: `SqlxToolBackend` gains a `Publisher`
    // because every task command takes one.
    let publisher = state.publisher.clone();

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
            // Both guards live exactly as long as this task (§7). `auth`
            // moves in here too (docs/specs/SLICE_013.md §2): the
            // backend's `run_saved_list` needs it for `list_saved_lists`/
            // `saved_list_detail`, and nothing above this point needs
            // `auth` again — every earlier use (the busy-log, `ctx`,
            // `slot`) already read what it needed by copy or clone.
            let _slot = slot;
            let started = Instant::now();
            let backend =
                SqlxToolBackend::new(pool.clone(), runtime.proposal_ttl(), auth, publisher);
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
        // crm-operator's `OperatorContext.turn_id` keeps a bare `Uuid` at
        // the tool seam (D-028 §5 crate fence); wrap once here, at the
        // wire boundary (hardening chunk N4, mirroring the org/user/
        // person seam conversions elsewhere in this file).
        turn_id: TurnId::new(turn_id),
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
        proposal: output
            .proposal
            .as_ref()
            .map(WireProposal::from_turn_proposal),
        receipt: output.receipt.as_ref().map(WireReceipt::from_view),
        outcome: output.outcome,
    };
    Ok(Json(response).into_response())
}

/// `POST /api/operator/proposals/{id}/confirm` (docs/specs/SLICE_006b.md
/// §4; docs/specs/SLICE_018.md §5): the human click that executes a
/// proposed call or task. Deterministic and model-free — needs no
/// operator runtime, takes no turn semaphore, works with the provider
/// down. Claim-then-execute: the claim serializes double-confirms on the
/// row; a claimed row is consumed forever (a crash before finalize leaves
/// `claimed`, which reads as consumed). The claim's `tool` column decides
/// which branch runs below — `start_call` needs telephony, `create_task`
/// needs neither telephony nor the operator runtime.
#[tracing::instrument(
    name = "operator.proposal_confirm",
    skip_all,
    fields(
        proposal_id = %proposal_id,
        organization_id = %auth.active_organization_id,
        actor_id = %auth.actor_user_id,
        turn_id = tracing::field::Empty,
        call_id = tracing::field::Empty,
        task_id = tracing::field::Empty,
        tool = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn confirm_proposal(
    State(state): State<AppState>,
    axum::extract::Path(proposal_id): axum::extract::Path<ProposalId>,
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
           RETURNING tool, person_id, contact_method_id, task_id, turn_id"#,
        proposal_id.0,
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
            r#"SELECT status, call_id, task_id FROM operator_proposal
               WHERE id = $1 AND organization_id = $2 AND actor_user_id = $3"#,
            proposal_id.0,
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
                Err(ApiError::ProposalConsumed {
                    call_id: p.call_id,
                    task_id: p.task_id,
                })
            }
            Some(_) => {
                span.record("outcome", "proposal_expired");
                Err(ApiError::ProposalExpired)
            }
        };
    };
    // `operator_proposal.turn_id` is a genuine Operator turn id (hardening
    // chunk N4) — wrap once here, right after the claim, and use the
    // typed value for every use below (the span field, the correlation
    // conversion inside `CommandContext::for_operator`, and the
    // correlation-chain lookup on the error path).
    let turn_id = TurnId::new(row.turn_id);
    span.record("turn_id", tracing::field::display(turn_id));
    span.record("tool", row.tool.as_str());

    match row.tool.as_str() {
        "start_call" => {
            confirm_start_call(
                &state,
                pool,
                &auth,
                &span,
                proposal_id,
                turn_id,
                row.person_id,
                row.contact_method_id,
            )
            .await
        }
        "create_task" => {
            confirm_create_task(&state, pool, &auth, &span, proposal_id, turn_id).await
        }
        // Unreachable by the `operator_proposal_tool_check` CHECK (docs/
        // specs/SLICE_018.md §4); fail closed rather than panic, the
        // `start_call` branch's `contact_method_id` assertion precedent.
        _ => {
            finalize_failed(pool, proposal_id, "corrupt", None).await;
            span.record("outcome", "corrupt");
            Err(ApiError::Unavailable)
        }
    }
}

/// The `start_call` confirm branch (docs/specs/SLICE_006b.md §4): executes
/// the exact command the Call button uses, as the session user, with the
/// turn id as the correlation id.
#[allow(clippy::too_many_arguments)]
async fn confirm_start_call(
    state: &AppState,
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    span: &tracing::Span,
    proposal_id: ProposalId,
    turn_id: TurnId,
    person_id: uuid::Uuid,
    contact_method_id: Option<uuid::Uuid>,
) -> Result<Response, ApiError> {
    let Some(contact_method_id) = contact_method_id else {
        // Unreachable by the `operator_proposal_contact_method_id_check`
        // CHECK (docs/specs/SLICE_018.md §4: `contact_method_id` is
        // non-null iff `tool = 'start_call'`); fail closed to 503 rather
        // than panic — one assertion.
        finalize_failed(pool, proposal_id, "corrupt", None).await;
        span.record("outcome", "corrupt");
        return Err(ApiError::Unavailable);
    };
    let telephony = match state.telephony.as_ref() {
        Some(t) => t,
        None => {
            finalize_failed(pool, proposal_id, "telephony_disabled", None).await;
            span.record("outcome", "telephony_disabled");
            return Err(ApiError::TelephonyDisabled);
        }
    };
    let ctx = CommandContext::for_operator(auth, turn_id);
    match commands::start_call(
        pool,
        &state.publisher,
        telephony,
        &ctx,
        StartCall {
            person_id: PersonId::new(person_id),
            contact_method_id: ContactMethodId::new(contact_method_id),
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
                proposal_id.0,
                call.id.0,
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
            let failed_call_id: Option<CallId> = sqlx::query_scalar!(
                r#"SELECT id FROM call
                   WHERE organization_id = $1 AND correlation_id = $2"#,
                auth.active_organization_id.0,
                turn_id.as_uuid(),
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(CallId::new);
            finalize_failed(pool, proposal_id, kind, failed_call_id).await;
            Err(ApiError::from(err))
        }
    }
}

/// The `create_task` confirm branch (docs/specs/SLICE_018.md §5): needs no
/// telephony and no operator runtime — reads the sidecar row, calls
/// `crm_app::domain::task::create_task` with `CommandContext::for_operator`,
/// finalizes `confirmed` + `task_id`, and answers 201 `{"task": Task}`,
/// byte-identical to `POST /api/people/{id}/tasks`.
async fn confirm_create_task(
    state: &AppState,
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    span: &tracing::Span,
    proposal_id: ProposalId,
    turn_id: TurnId,
) -> Result<Response, ApiError> {
    // A Person deleted between propose and confirm cascades the sidecar
    // away (docs/specs/SLICE_018.md §5): the scoped claim still succeeded,
    // but the sidecar read finds no row — finalize `failed` with
    // `not_found`, answer 404.
    let sidecar = sqlx::query!(
        r#"SELECT person_id, title, kind, due_at, assignee_user_id
           FROM operator_task_proposal WHERE proposal_id = $1"#,
        proposal_id.0,
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let Some(sidecar) = sidecar else {
        finalize_failed(pool, proposal_id, "not_found", None).await;
        span.record("outcome", "not_found");
        return Err(ApiError::NotFound);
    };

    let Some(kind) = TaskKind::from_db_str(&sidecar.kind) else {
        // Unreachable: the sidecar's own CHECK admits only the closed
        // enum's strings. Fail closed to 503 rather than panic.
        finalize_failed(pool, proposal_id, "corrupt", None).await;
        span.record("outcome", "corrupt");
        return Err(ApiError::Unavailable);
    };

    let ctx = CommandContext::for_operator(auth, turn_id);
    match task::create_task(
        pool,
        &state.publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(sidecar.person_id),
            title: sidecar.title,
            kind,
            due_at: sidecar.due_at,
            assignee_user_id: Some(UserId::new(sidecar.assignee_user_id)),
        },
    )
    .await
    {
        Ok(created) => {
            span.record("task_id", tracing::field::display(created.id));
            span.record("outcome", "confirmed");
            let finalized = sqlx::query!(
                r#"UPDATE operator_proposal
                   SET status = 'confirmed', task_id = $2, confirmed_at = now()
                   WHERE id = $1 AND status = 'claimed'"#,
                proposal_id.0,
                created.id.0,
            )
            .execute(pool)
            .await;
            if let Err(err) = finalized {
                tracing::error!(error = %err, "proposal finalize failed after create_task");
            }
            Ok((
                axum::http::StatusCode::CREATED,
                Json(serde_json::json!({ "task": created })),
            )
                .into_response())
        }
        Err(err) => {
            let kind = err.kind();
            span.record("outcome", kind);
            // Unlike `start_call`, `create_task` never leaves a partial
            // row on failure (it is one transaction) — no correlation-chain
            // lookup, `call_id`/`task_id` both stay NULL on the failed row.
            finalize_failed(pool, proposal_id, kind, None).await;
            Err(ApiError::from(err))
        }
    }
}

/// Best-effort `failed` finalization; the command's error is the truth.
async fn finalize_failed(
    pool: &sqlx::PgPool,
    proposal_id: ProposalId,
    kind: &str,
    call_id: Option<CallId>,
) {
    let result = sqlx::query!(
        r#"UPDATE operator_proposal
           SET status = 'failed', failure_code = $2, call_id = $3
           WHERE id = $1 AND status = 'claimed'"#,
        proposal_id.0,
        kind,
        call_id.map(|id| id.0),
    )
    .execute(pool)
    .await;
    if let Err(err) = result {
        tracing::error!(error = %err, "proposal failed-finalize failed");
    }
}

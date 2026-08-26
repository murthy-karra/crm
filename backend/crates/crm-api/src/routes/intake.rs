use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::extractors::OrgAdminContext;
use crate::auth::AuthContext;
use crate::domain::commands::{self, ReceiveInquiry, ReceiveInquiryOutcome};
use crate::domain::envelope::CommandContext;
use crate::domain::inquiry::parse::Source;
use crate::domain::intake::workbench::{self, DiscardOutcome, UnresolvedContent};
use crate::domain::intake::IntakeActor;
use crate::domain::raw_payload::store;
use crate::error::ApiError;
use crate::state::AppState;

const MAX_INTAKE_BODY_BYTES: usize = 256 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/inquiries",
            post(receive_inquiry_handler).layer(DefaultBodyLimit::max(MAX_INTAKE_BODY_BYTES)),
        )
        .route("/api/intake/unresolved", get(list_unresolved))
        .route("/api/intake/unresolved/{id}", get(unresolved_detail))
        .route("/api/intake/unresolved/{id}/retry", post(retry_unresolved))
        .route(
            "/api/intake/unresolved/{id}/discard",
            post(discard_unresolved),
        )
}

/// A `{id}` path segment parsed as a UUID, rejecting straight to `400
/// malformed_request` — the `routes/people.rs` `PersonId` pattern
/// (declared before the auth extractor so a non-UUID id is a 400
/// independent of auth state, testable service-free).
struct RawPayloadId(Uuid);

impl FromRequestParts<AppState> for RawPayloadId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(RawPayloadId(id))
    }
}

#[derive(Deserialize)]
struct ReceiveInquiryRequest {
    source: String,
    payload: serde_json::Value,
    #[serde(default)]
    assign_to_user_id: Option<Uuid>,
}

/// `POST /api/inquiries` — the simulated dev ingress and the manual-entry
/// form are the same route (docs/specs/SLICE_002.md §5). Any JSON-extraction
/// failure — including exceeding the body limit above — maps to 400
/// `malformed_request`, mirroring Slice 001's login handler.
async fn receive_inquiry_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    body: Result<Json<ReceiveInquiryRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let source = Source::parse(&req.source).ok_or(ApiError::MalformedRequest)?;
    let payload_bytes = serde_json::to_vec(&req.payload).map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let actor = IntakeActor::User(CommandContext::from_auth(&auth));

    let cmd = ReceiveInquiry {
        source,
        payload: payload_bytes,
        assign_to_user_id: req.assign_to_user_id,
        received_at: Utc::now(),
    };

    let outcome =
        commands::receive_inquiry(pool, &state.raw_payload_key, &state.publisher, &actor, cmd)
            .await?;

    let (status, body) = match outcome {
        ReceiveInquiryOutcome::Resolved {
            inquiry_id,
            person_id,
            person_created,
            routing_strategy,
            assigned_user_id,
            duplicate,
        } => (
            duplicate_status(duplicate),
            json!({
                "status": "resolved",
                "inquiry_id": inquiry_id,
                "person_id": person_id,
                "person_created": person_created,
                "routing_strategy": routing_strategy.as_str(),
                "assigned_user_id": assigned_user_id,
                "duplicate": duplicate,
            }),
        ),
        ReceiveInquiryOutcome::Unresolved {
            raw_payload_id,
            reason,
            duplicate,
        } => (
            duplicate_status(duplicate),
            json!({
                "status": "unresolved",
                "raw_payload_id": raw_payload_id,
                "reason": reason.as_str(),
                "duplicate": duplicate,
            }),
        ),
    };

    Ok((status, Json(body)).into_response())
}

/// 201 on any first delivery (resolved or unresolved — unresolved is a
/// committed, terminal outcome this slice), 200 when the payload was
/// already stored and processed (docs/specs/SLICE_002.md §5).
fn duplicate_status(duplicate: bool) -> StatusCode {
    if duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    }
}

/// Metadata-only queue, visible to every Organization member — never
/// decrypts (docs/specs/SLICE_002.md §3, §5). `auth.active_organization_id`
/// only: this is Organization data, not Person visibility.
async fn list_unresolved(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let (items, truncated) = store::list_unresolved(&mut conn, auth.active_organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    let items: Vec<_> = items
        .into_iter()
        .map(|item| {
            json!({
                "id": item.id,
                "source": item.source,
                "received_at": item.received_at,
                "resolution": item.resolution.as_str(),
                "reason": item.unresolved_reason,
                "byte_len": item.byte_len,
            })
        })
        .collect();

    Ok(Json(json!({ "items": items, "truncated": truncated })))
}

/// `GET /api/intake/unresolved/{id}` — admin-only decrypt-on-demand
/// detail (docs/specs/SLICE_007e.md §5, D-037). Content exists only in
/// this per-row response — never in the list, never in spans/logs.
#[tracing::instrument(
    name = "intake.unresolved_detail",
    skip_all,
    fields(
        organization_id = tracing::field::Empty,
        actor_id = tracing::field::Empty,
        raw_payload_id = %id.0,
        payload_format = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn unresolved_detail(
    id: RawPayloadId,
    State(state): State<AppState>,
    admin: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let span = tracing::Span::current();
    span.record(
        "organization_id",
        tracing::field::display(admin.auth.active_organization_id),
    );
    span.record(
        "actor_id",
        tracing::field::display(admin.auth.actor_user_id),
    );

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&admin.auth);

    let detail = workbench::unresolved_detail(pool, &state.raw_payload_key, &ctx, id.0)
        .await
        .map_err(|err| {
            span.record("outcome", err.kind());
            ApiError::from(err)
        })?;
    span.record("payload_format", detail.payload_format.as_str());
    span.record("outcome", "shown");

    let content = match detail.content {
        UnresolvedContent::Email {
            subject,
            from_display,
            from_addr,
            date,
            text,
            truncated,
        } => json!({
            "kind": "email",
            "subject": subject,
            "from_display": from_display,
            "from_addr": from_addr,
            "date": date,
            "text": text,
            "truncated": truncated,
        }),
        UnresolvedContent::Text { text, truncated } => json!({
            "kind": "text",
            "text": text,
            "truncated": truncated,
        }),
    };

    Ok(Json(json!({
        "id": detail.id,
        "source": detail.source,
        "payload_format": detail.payload_format,
        "received_at": detail.received_at,
        "resolution": detail.resolution.as_str(),
        "reason": detail.unresolved_reason,
        "byte_len": detail.byte_len,
        "content": content,
    })))
}

/// `POST /api/intake/unresolved/{id}/retry` (docs/specs/SLICE_007e.md
/// §4/§5): reset + the shared Phase B under the System actor; the
/// SLICE_002 §5 response vocabulary reused.
#[tracing::instrument(
    name = "intake.retry",
    skip_all,
    fields(
        organization_id = tracing::field::Empty,
        actor_id = tracing::field::Empty,
        raw_payload_id = %id.0,
        payload_format = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn retry_unresolved(
    id: RawPayloadId,
    State(state): State<AppState>,
    admin: OrgAdminContext,
) -> Result<Response, ApiError> {
    let span = tracing::Span::current();
    span.record(
        "organization_id",
        tracing::field::display(admin.auth.active_organization_id),
    );
    span.record(
        "actor_id",
        tracing::field::display(admin.auth.actor_user_id),
    );

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&admin.auth);

    let outcome =
        workbench::retry_intake(pool, &state.raw_payload_key, &state.publisher, &ctx, id.0)
            .await
            .map_err(|err| {
                span.record(
                    "outcome",
                    match &err {
                        workbench::WorkbenchError::Command(commands::CommandError::IntakeBusy) => {
                            "retried_busy"
                        }
                        other => other.kind(),
                    },
                );
                ApiError::from(err)
            })?;

    let body = match outcome {
        ReceiveInquiryOutcome::Resolved {
            inquiry_id,
            person_id,
            person_created,
            routing_strategy,
            assigned_user_id,
            duplicate,
        } => {
            span.record("outcome", "retried_resolved");
            json!({
                "status": "resolved",
                "inquiry_id": inquiry_id,
                "person_id": person_id,
                "person_created": person_created,
                "routing_strategy": routing_strategy.as_str(),
                "assigned_user_id": assigned_user_id,
                "duplicate": duplicate,
            })
        }
        ReceiveInquiryOutcome::Unresolved {
            raw_payload_id,
            reason,
            duplicate,
        } => {
            span.record("outcome", "retried_unresolved");
            json!({
                "status": "unresolved",
                "raw_payload_id": raw_payload_id,
                "reason": reason.as_str(),
                "duplicate": duplicate,
            })
        }
    };
    Ok((StatusCode::OK, Json(body)).into_response())
}

/// `POST /api/intake/unresolved/{id}/discard` (docs/specs/SLICE_007e.md
/// §4/§5): explicit, attributed, idempotent; not deletion.
#[tracing::instrument(
    name = "intake.discard",
    skip_all,
    fields(
        organization_id = tracing::field::Empty,
        actor_id = tracing::field::Empty,
        raw_payload_id = %id.0,
        outcome = tracing::field::Empty,
    )
)]
async fn discard_unresolved(
    id: RawPayloadId,
    State(state): State<AppState>,
    admin: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let span = tracing::Span::current();
    span.record(
        "organization_id",
        tracing::field::display(admin.auth.active_organization_id),
    );
    span.record(
        "actor_id",
        tracing::field::display(admin.auth.actor_user_id),
    );

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&admin.auth);

    let outcome = workbench::discard_raw_payload(pool, &state.publisher, &ctx, id.0)
        .await
        .map_err(|err| {
            span.record("outcome", err.kind());
            ApiError::from(err)
        })?;
    span.record(
        "outcome",
        match outcome {
            DiscardOutcome::Discarded => "discarded",
            DiscardOutcome::AlreadyDiscarded => "already_discarded",
        },
    );

    Ok(Json(json!({ "status": "discarded" })))
}

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::commands::{self, ReceiveInquiry, ReceiveInquiryOutcome};
use crate::domain::envelope::CommandContext;
use crate::domain::inquiry::parse::Source;
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
    let ctx = CommandContext::from_auth(&auth);

    let cmd = ReceiveInquiry {
        source,
        payload: payload_bytes,
        assign_to_user_id: req.assign_to_user_id,
        received_at: Utc::now(),
    };

    let outcome =
        commands::receive_inquiry(pool, &state.raw_payload_key, &state.publisher, &ctx, cmd)
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
                "resolution": item.resolution,
                "reason": item.unresolved_reason,
                "byte_len": item.byte_len,
            })
        })
        .collect();

    Ok(Json(json!({ "items": items, "truncated": truncated })))
}

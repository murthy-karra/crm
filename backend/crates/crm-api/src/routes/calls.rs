//! The four call routes (docs/specs/SLICE_006.md §5): `start`, `dial`,
//! `hangup` need telephony enabled (503 `telephony_disabled` otherwise);
//! `GET /api/calls/{id}` is a read and works with telephony disabled.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::commands::{self, StartCall};
use crate::domain::envelope::CommandContext;
use crate::domain::telephony::queries as call_queries;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people/{id}/calls", post(start_call))
        .route("/api/calls/{id}/dial", post(dial_call))
        .route("/api/calls/{id}/hangup", post(hangup_call))
        .route("/api/calls/{id}", get(get_call))
}

/// A `{id}` path segment parsed as a UUID, rejecting straight to 400
/// `malformed_request` ahead of authentication — the `routes/people.rs`
/// `PersonId` pattern.
struct PathId(Uuid);

impl FromRequestParts<AppState> for PathId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PathId(id))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartCallRequest {
    contact_method_id: Uuid,
}

/// `POST /api/people/{id}/calls` → 201 `{"call", "join": {url, token,
/// room}}`. The token is serialized here, once, and nowhere else.
async fn start_call(
    State(state): State<AppState>,
    PathId(person_id): PathId,
    auth: AuthContext,
    body: Result<Json<StartCallRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let telephony = state
        .telephony
        .as_ref()
        .ok_or(ApiError::TelephonyDisabled)?;
    let ctx = CommandContext::from_auth(&auth);

    let (call, grant) = commands::start_call(
        pool,
        &state.publisher,
        telephony,
        &ctx,
        StartCall {
            person_id,
            contact_method_id: req.contact_method_id,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "call": call,
            "join": {
                "url": grant.url,
                "token": grant.token.into_string(),
                "room": grant.room,
            },
        })),
    ))
}

/// `POST /api/calls/{id}/dial` → 202 `{"call"}` (still `placing`).
async fn dial_call(
    State(state): State<AppState>,
    PathId(call_id): PathId,
    auth: AuthContext,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let telephony = state
        .telephony
        .as_ref()
        .ok_or(ApiError::TelephonyDisabled)?;
    let ctx = CommandContext::from_auth(&auth);

    let (call, _task) =
        commands::dial_call(pool, &state.publisher, telephony, &ctx, call_id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "call": call }))))
}

/// `POST /api/calls/{id}/hangup` → 200 `{"call"}`, idempotent.
async fn hangup_call(
    State(state): State<AppState>,
    PathId(call_id): PathId,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let telephony = state
        .telephony
        .as_ref()
        .ok_or(ApiError::TelephonyDisabled)?;
    let ctx = CommandContext::from_auth(&auth);

    let call = commands::hangup_call(pool, &state.publisher, telephony, &ctx, call_id).await?;
    Ok(Json(json!({ "call": call })))
}

/// `GET /api/calls/{id}` → 200 `{"call"}` for any member; 404 foreign.
async fn get_call(
    State(state): State<AppState>,
    PathId(call_id): PathId,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let call = call_queries::call_view_by_id(&mut conn, auth.active_organization_id, call_id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(json!({ "call": call })))
}

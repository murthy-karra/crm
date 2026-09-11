//! Additive D-063 snapshot API. Source data is never a trusted command input.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{snapshot as s, snapshot_preview as p},
    },
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
fn response(status: StatusCode, value: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
fn path<T>(value: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    value
        .map(|Path(v)| v)
        .map_err(|_| ApiError::MalformedRequest)
}
fn query(value: Result<Query<s::PageQuery>, QueryRejection>) -> Result<s::PageQuery, ApiError> {
    value
        .map(|Query(v)| v)
        .map_err(|_| ApiError::MalformedRequest)
}
fn body<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    value
        .map(|Json(v)| v)
        .map_err(|_| ApiError::MalformedRequest)
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/migrations/fub/snapshots", get(list).post(propose))
        .route("/api/migrations/fub/snapshots/{id}", get(detail))
        .route("/api/migrations/fub/snapshots/{id}/confirm", post(confirm))
        .route("/api/migrations/fub/snapshots/{id}/retry", post(retry))
        .route("/api/migrations/fub/snapshots/{id}/cancel", post(cancel))
        .route("/api/migrations/fub/snapshots/{id}/budget", post(budget))
        .route(
            "/api/migrations/fub/snapshots/{id}/previews",
            post(generate),
        )
        .route(
            "/api/migrations/fub/snapshots/{id}/previews/{preview}",
            get(preview),
        )
        .route(
            "/api/migrations/fub/snapshots/{id}/previews/{preview}/retry",
            post(preview_retry),
        )
        .route(
            "/api/migrations/fub/snapshots/{id}/previews/{preview}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/snapshots/{id}/previews/{preview}/overlap-groups",
            get(groups),
        )
        .route(
            "/api/migrations/fub/snapshots/{id}/previews/{preview}/overlap-groups/{group}/members",
            get(members),
        )
        .layer(DefaultBodyLimit::max(8192))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
async fn list(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    q: Result<Query<s::PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let value = s::list(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        query(q)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn propose(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    b: Result<Json<s::ProposeCoreSnapshot>, JsonRejection>,
) -> Result<Response, ApiError> {
    let value = s::propose(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        body(b)?,
    )
    .await?;
    Ok(response(StatusCode::CREATED, value))
}
async fn detail(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let value = s::detail(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn confirm(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<s::SnapshotRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let value = s::source_action(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
        body(b)?,
        s::SourceAction::Confirm,
    )
    .await?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn retry(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<s::SnapshotRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let value = s::source_action(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
        body(b)?,
        s::SourceAction::Retry,
    )
    .await?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn cancel(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
    raw: axum::body::Bytes,
) -> Result<Response, ApiError> {
    if !raw.is_empty() {
        serde_json::from_slice::<Empty>(&raw).map_err(|_| ApiError::MalformedRequest)?;
    }
    let value = s::cancel(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn budget(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<s::IncreaseCoreSnapshotBudget>, JsonRejection>,
) -> Result<Response, ApiError> {
    let value = s::increase_budget(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
        body(b)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn generate(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<s::SnapshotRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let value = p::generate(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        path(id)?,
        body(b)?,
    )
    .await?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn preview(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (run, id) = path(ids)?;
    let value = p::detail(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &CommandContext::from_auth(&admin.auth),
        run,
        id,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn preview_retry(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
    b: Result<Json<s::SnapshotRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let (run, id) = path(ids)?;
    let value = p::retry(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &state.snapshot_policy,
        &CommandContext::from_auth(&admin.auth),
        run,
        id,
        body(b)?,
    )
    .await?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn records(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<s::PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (run, id) = path(ids)?;
    let value = p::records(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &CommandContext::from_auth(&admin.auth),
        run,
        id,
        query(q)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn groups(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<s::PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (run, id) = path(ids)?;
    let value = p::groups(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &CommandContext::from_auth(&admin.auth),
        run,
        id,
        query(q)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}
async fn members(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    ids: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
    q: Result<Query<s::PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (run, id, group) = path(ids)?;
    let value = p::members(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &state.raw_payload_key,
        &CommandContext::from_auth(&admin.auth),
        run,
        id,
        group,
        query(q)?,
    )
    .await?;
    Ok(response(StatusCode::OK, value))
}

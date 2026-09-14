//! D-082 bounded admitted-People metadata transport.  Registration and the
//! workspace prefix guard are coordinated because they are shared route files.
use crate::{
    auth::OrgAdminContext,
    domain::{envelope::CommandContext, migration::admitted_metadata as i},
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Json, Path, Query, State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde_json::Value;
use uuid::Uuid;
fn response(status: StatusCode, value: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
fn body<T>(v: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::PayloadTooLarge
        } else {
            ApiError::MalformedRequest
        }
    })
}
fn path<T>(v: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(v: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/admitted-metadata-imports",
            get(list).post(prepare),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}",
            get(detail),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/plans",
            axum::routing::post(apply_mappings),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/confirm",
            axum::routing::post(confirm),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/retry",
            axum::routing::post(retry),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/cancel",
            axum::routing::post(cancel),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/mappings",
            get(mappings),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/issues",
            get(issues),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
}
async fn apply_mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::MappingPatch>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::apply_mappings(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await?,
    ))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::Confirm>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::confirm(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await?,
    ))
}
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::Request>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::retry(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::Request>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::cancel(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await?,
    ))
}
async fn mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::mappings(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await?,
    ))
}
async fn records(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::records(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await?,
    ))
}
async fn issues(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::issues(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
        )
        .await?,
    ))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<i::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::list(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            query(q)?,
        )
        .await?,
    ))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::get(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await?,
    ))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<i::Prepare>, JsonRejection>,
) -> Result<Response, ApiError> {
    // Shared release readiness is intentionally checked before any root exists:
    // absent claims are not authoritative until the coordinator's handover commits.
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let release = s
        .current_import_release()
        .await
        .ok_or(ApiError::Unavailable)?;
    release
        .require_admitted_metadata(&mut conn)
        .await
        .map_err(crm_app::domain::migration::MigrationError::Database)?;
    drop(conn);
    Ok(response(
        StatusCode::CREATED,
        i::prepare(
            pool,
            &s.raw_payload_key,
            &release,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
        )
        .await?,
    ))
}

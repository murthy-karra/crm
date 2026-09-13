//! D-076 current-admin People refresh API. Values remain in bounded encrypted plan rows.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{people_refresh as h, MigrationError},
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
use serde_json::Value;
use uuid::Uuid;
fn body<T>(v: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn path<T>(v: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(v: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn error(e: MigrationError) -> ApiError {
    match e {
        MigrationError::SourceNotEligible | MigrationError::SourceAccountMismatch => {
            ApiError::ImportError("import_conflict")
        }
        e => e.into(),
    }
}
fn response(status: StatusCode, v: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/people-refreshes",
            get(list).post(prepare),
        )
        .route("/api/migrations/fub/people-refreshes/{id}", get(detail))
        .route(
            "/api/migrations/fub/people-refreshes/{id}/items",
            get(items),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/items/{item}",
            get(item),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/items/{item}/contacts",
            get(contacts),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/plans",
            post(repreview),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/retry",
            post(retry),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/results",
            get(results),
        )
        .route(
            "/api/migrations/fub/people-refreshes/{id}/items/{item}/fields/{side}/{field}",
            get(field),
        )
        .layer(DefaultBodyLimit::max(8192))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::PreparePeopleRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::CREATED,
        h::prepare(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
            release.as_deref(),
        )
        .await
        .map_err(error)?,
    ))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::list(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::detail(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn items(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::items(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn item(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (id, item) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::item(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            item,
        )
        .await
        .map_err(error)?,
    ))
}
async fn contacts(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, item) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::contacts(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            item,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn repreview(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::RepreviewPeopleRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        h::repreview(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::ConfirmPeopleRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::confirm(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            release.as_deref(),
        )
        .await
        .map_err(error)?,
    ))
}
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::LifecyclePeopleRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::retry(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            release.as_deref(),
        )
        .await
        .map_err(error)?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::LifecyclePeopleRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::cancel(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::results(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}

async fn field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, String, String)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}

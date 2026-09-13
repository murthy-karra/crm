//! Additive current-admin report routes, including no-store errors.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{core_change_reports as h, MigrationError},
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
use serde_json::{json, Value};
use uuid::Uuid;
fn error(e: MigrationError) -> ApiError {
    match e {
        MigrationError::SourceNotEligible
        | MigrationError::SourceAccountMismatch
        | MigrationError::ReleaseNotReady => ApiError::ImportError("import_conflict"),
        MigrationError::Crypto => ApiError::Unavailable,
        e => e.into(),
    }
}
fn body<T>(v: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn path<T>(v: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(v: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn response(status: StatusCode, value: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/core-change-reports",
            get(list).post(prepare),
        )
        .route("/api/migrations/fub/core-change-reports/{id}", get(detail))
        .route(
            "/api/migrations/fub/core-change-reports/{id}/rows",
            get(rows),
        )
        .route(
            "/api/migrations/fub/core-change-reports/{id}/rows/{row_id}",
            get(row_detail),
        )
        .route(
            "/api/migrations/fub/core-change-reports/{id}/resume",
            post(resume),
        )
        .route(
            "/api/migrations/fub/core-change-reports/{id}/cancel",
            post(cancel),
        )
        .layer(DefaultBodyLimit::max(8192))
        .layer(axum::middleware::map_response(
            |mut r: Response| async move {
                r.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                );
                r
            },
        ))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::PrepareCoreChangeReport>, JsonRejection>,
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
async fn resume(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::ReportRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::resume(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
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
    b: Result<Json<h::ReportRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::cancel(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn ready(s: &AppState, v: &mut Value) {
    if !s
        .current_import_release()
        .await
        .is_some_and(|r| r.core_change_ready())
    {
        v["actions"]["resume"] = json!(false);
    }
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let mut v = h::detail(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        path(p)?,
    )
    .await
    .map_err(error)?;
    ready(&s, &mut v).await;
    Ok(response(StatusCode::OK, v))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::ReportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut v = h::list(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        query(q)?,
    )
    .await
    .map_err(error)?;
    if let Some(items) = v["reports"].as_array_mut() {
        for item in items {
            ready(&s, item).await;
        }
    }
    Ok(response(StatusCode::OK, v))
}
async fn rows(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::ReportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::rows(
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
async fn row_detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (id, row) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::row_detail(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            row,
        )
        .await
        .map_err(error)?,
    ))
}

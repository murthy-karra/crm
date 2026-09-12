//! Additive admin-only metadata review, with no source content in HTTP inputs.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{history_capture as h, MigrationError},
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
        MigrationError::SourceNotEligible | MigrationError::SourceAccountMismatch => {
            ApiError::ImportError("import_conflict")
        }
        MigrationError::Crypto => ApiError::Unavailable,
        v => v.into(),
    }
}
fn response(status: StatusCode, value: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
fn path<T>(v: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(v: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn body<T>(v: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/history-captures",
            get(list).post(propose),
        )
        .route("/api/migrations/fub/history-captures/{id}", get(detail))
        .route(
            "/api/migrations/fub/history-captures/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/history-captures/{id}/retry",
            post(retry),
        )
        .route(
            "/api/migrations/fub/history-captures/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/history-captures/{id}/budget",
            post(budget),
        )
        .route(
            "/api/migrations/fub/history-captures/{id}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/history-captures/{id}/records/{record}",
            get(record),
        )
        .layer(DefaultBodyLimit::max(8192))
        .layer(axum::middleware::map_response(
            |mut r: Response| async move {
                r.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("no-store"),
                );
                r
            },
        ))
}
fn readiness(v: &mut Value, ready: bool) {
    v["release_ready"] = json!(ready);
    if !ready {
        v["actions"]["confirm"] = json!(false);
        v["actions"]["retry"] = json!(false);
    }
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::HistoryPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut v = h::list(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &s.snapshot_policy,
        &CommandContext::from_auth(&a.auth),
        query(q)?,
    )
    .await
    .map_err(error)?;
    let ready = s
        .current_import_release()
        .await
        .is_some_and(|v| v.history_capture_ready());
    if let Some(items) = v["captures"].as_array_mut() {
        for item in items {
            readiness(item, ready)
        }
    }
    Ok(response(StatusCode::OK, v))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let mut v = h::detail(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.snapshot_policy,
        &CommandContext::from_auth(&a.auth),
        path(p)?,
    )
    .await
    .map_err(error)?;
    readiness(
        &mut v,
        s.current_import_release()
            .await
            .is_some_and(|v| v.history_capture_ready()),
    );
    Ok(response(StatusCode::OK, v))
}
async fn propose(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::ProposeFubHistoryCapture>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::CREATED,
        h::propose(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
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
    b: Result<Json<h::ConfirmFubHistoryCapture>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::confirm(
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
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::HistoryRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::retry(
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
    b: Result<Json<h::HistoryRequest>, JsonRejection>,
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
async fn budget(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::IncreaseFubHistoryCaptureBudget>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::increase_budget(
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
async fn records(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::HistoryPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::records(
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
async fn record(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (run, id) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::record_detail(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            run,
            id,
        )
        .await
        .map_err(error)?,
    ))
}

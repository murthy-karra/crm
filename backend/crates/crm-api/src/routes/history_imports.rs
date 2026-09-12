//! Current-admin retained import commands. All metadata routes are no-store.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{history_import as h, MigrationError},
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
fn read_error(e: MigrationError) -> ApiError {
    match e {
        MigrationError::StorageLimit => ApiError::Unavailable,
        e => error(e),
    }
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
fn response(status: StatusCode, value: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/history-imports",
            get(list).post(prepare),
        )
        .route("/api/migrations/fub/history-imports/{id}", get(detail))
        .route(
            "/api/migrations/fub/history-imports/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/history-imports/{id}/resume",
            post(resume),
        )
        .route(
            "/api/migrations/fub/history-imports/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/history-imports/{id}/budget",
            post(budget),
        )
        .route(
            "/api/migrations/fub/history-imports/{id}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/history-imports/{id}/results",
            get(results),
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
fn readiness(v: &mut Value, ready: bool) {
    v["release_ready"] = json!(ready);
    if !ready {
        v["actions"]["confirm"] = json!(false);
        v["actions"]["resume"] = json!(false);
    }
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::ImportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut v = h::list(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &s.snapshot_policy,
        &CommandContext::from_auth(&a.auth),
        query(q)?,
    )
    .await
    .map_err(read_error)?;
    let ready = s
        .current_import_release()
        .await
        .is_some_and(|r| r.history_timeline_ready());
    if let Some(items) = v["imports"].as_array_mut() {
        for item in items {
            readiness(item, ready);
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
    .map_err(read_error)?;
    readiness(
        &mut v,
        s.current_import_release()
            .await
            .is_some_and(|r| r.history_timeline_ready()),
    );
    Ok(response(StatusCode::OK, v))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::PrepareFubHistoryImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::CREATED,
        h::prepare(
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
    b: Result<Json<h::ConfirmFubHistoryImport>, JsonRejection>,
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
async fn resume(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::ResumeFubHistoryImport>, JsonRejection>,
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
    b: Result<Json<h::CancelFubHistoryImport>, JsonRejection>,
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
    b: Result<Json<h::IncreaseFubHistoryImportBudget>, JsonRejection>,
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
    q: Result<Query<h::ImportPage>, QueryRejection>,
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
        .map_err(read_error)?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::ImportPage>, QueryRejection>,
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
        .map_err(read_error)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_admission_and_unavailable_reads_have_distinct_statuses() {
        assert_eq!(
            error(MigrationError::ReleaseNotReady)
                .into_response()
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            error(MigrationError::StorageLimit).into_response().status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            read_error(MigrationError::StorageLimit)
                .into_response()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            read_error(MigrationError::Crypto).into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

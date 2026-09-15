//! Slice 010d3 admitted-history HTTP boundary. Responses are always no-store.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{admitted_history as h, MigrationError},
    },
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
    routing::{get, post},
    Router,
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
fn map(e: MigrationError) -> ApiError {
    match e {
        MigrationError::SourceNotEligible => ApiError::ImportError("import_conflict"),
        MigrationError::Crypto => ApiError::Unavailable,
        x => x.into(),
    }
}
fn out(status: StatusCode, v: Value) -> Response {
    if serde_json::to_vec(&v).map_or(true, |bytes| bytes.len() > 512 * 1024) {
        return ApiError::Unavailable.into_response();
    }
    (status, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/admitted-history-imports",
            get(list).post(prepare),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}",
            get(detail),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/resume",
            post(resume),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/budget",
            post(budget),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/remainder",
            post(remainder).get(remainder_view),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/plans/{plan}/manifests",
            get(records),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/plans/{plan}/results",
            get(results),
        )
        .route(
            "/api/migrations/fub/admitted-history-imports/{id}/plans/{plan}/issues",
            get(issues),
        )
        .layer(DefaultBodyLimit::max(8192))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                let path = request.uri().path();
                let paged = request.method() == axum::http::Method::GET
                    && [
                        "/admitted-history-imports",
                        "/manifests",
                        "/results",
                        "/issues",
                    ]
                    .iter()
                    .any(|suffix| path.ends_with(suffix));
                if !paged && request.uri().query().is_some() {
                    return ApiError::MalformedRequest.into_response();
                }
                next.run(request).await
            },
        ))
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
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::OK,
        h::list(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            query(q)?,
        )
        .await
        .map_err(map)?,
    ))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::OK,
        h::get(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await
        .map_err(map)?,
    ))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::Prepare>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::CREATED,
        h::prepare(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
        )
        .await
        .map_err(map)?,
    ))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::Confirm>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::ACCEPTED,
        h::confirm(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            s.current_import_release().await.as_deref(),
        )
        .await
        .map_err(map)?,
    ))
}
async fn resume(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::Action>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::ACCEPTED,
        h::action(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            false,
            s.current_import_release().await.as_deref(),
        )
        .await
        .map_err(map)?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::Action>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::OK,
        h::action(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            true,
            s.current_import_release().await.as_deref(),
        )
        .await
        .map_err(map)?,
    ))
}
async fn budget(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::Budget>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
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
        .map_err(map)?,
    ))
}
async fn remainder(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<h::Remainder>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::ACCEPTED,
        h::create_remainder(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            s.current_import_release().await.as_deref(),
        )
        .await
        .map_err(map)?,
    ))
}
async fn records(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(out(
        StatusCode::OK,
        h::records(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await
        .map_err(map)?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(out(
        StatusCode::OK,
        h::results(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await
        .map_err(map)?,
    ))
}

async fn issues(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(out(
        StatusCode::OK,
        h::issues(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await
        .map_err(map)?,
    ))
}

async fn remainder_view(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(out(
        StatusCode::OK,
        h::remainder_view(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await
        .map_err(map)?,
    ))
}

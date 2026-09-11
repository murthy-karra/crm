//! D-065 retained People import. No endpoint accepts source fields or authority.
use crate::{
    auth::OrgAdminContext,
    domain::{envelope::CommandContext, migration::imports as i},
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
    v.map(|v| v.0).map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::PayloadTooLarge
        } else {
            ApiError::MalformedRequest
        }
    })
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/migrations/fub/imports", get(list).post(propose))
        .route("/api/migrations/fub/imports/{id}", get(detail))
        .route("/api/migrations/fub/imports/{id}/plans", post(replan))
        .route(
            "/api/migrations/fub/imports/{id}/plans/{plan}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/imports/{id}/plans/{plan}/mappings",
            get(mappings),
        )
        .route(
            "/api/migrations/fub/imports/{id}/plans/{plan}/mappings/{mapping}/fields/{field}",
            get(mapping_field),
        )
        .route(
            "/api/migrations/fub/imports/{id}/plans/{plan}/records/{record}/fields/{field}",
            get(field),
        )
        .route("/api/migrations/fub/imports/{id}/confirm", post(confirm))
        .route("/api/migrations/fub/imports/{id}/retry", post(retry))
        .route("/api/migrations/fub/imports/{id}/cancel", post(cancel))
        .route("/api/migrations/fub/imports/{id}/results", get(results))
        .route("/api/people/{id}/import-provenance", get(provenance))
        .route(
            "/api/people/{id}/import-provenance/fields/{field}",
            get(provenance_field),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<i::ImportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut value = i::list(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        query(q)?,
        &s.snapshot_policy,
    )
    .await?;
    let ready = s.current_import_release().await.is_some();
    if let Some(items) = value["imports"].as_array_mut() {
        for item in items {
            item["release_ready"] = serde_json::json!(ready);
            if !ready {
                item["actions"]["confirm"] = serde_json::json!(false)
            }
        }
    }
    Ok(response(StatusCode::OK, value))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let mut v = i::detail(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        path(p)?,
        &s.snapshot_policy,
    )
    .await?;
    let ready = s.current_import_release().await.is_some();
    v["release_ready"] = serde_json::json!(ready);
    if !ready {
        v["actions"]["confirm"] = serde_json::json!(false)
    }
    Ok(response(StatusCode::OK, v))
}
async fn propose(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<i::PlanPeopleImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::CREATED,
        i::propose(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
            &s.snapshot_policy,
        )
        .await?,
    ))
}
async fn replan(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ReplanPeopleImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::replan(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            &s.snapshot_policy,
        )
        .await?,
    ))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ConfirmPeopleImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    let id = path(p)?;
    let cmd = body(b)?;
    if let Some(receipt) = i::replay_confirmation(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        id,
        &cmd,
    )
    .await?
    {
        return Ok(response(StatusCode::ACCEPTED, receipt));
    }
    let release = s
        .current_import_release()
        .await
        .ok_or(ApiError::Unavailable)?;
    Ok(response(
        StatusCode::ACCEPTED,
        i::confirm(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            cmd,
            release.as_ref(),
            &s.snapshot_policy,
        )
        .await?,
    ))
}
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ImportRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::action(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            true,
            &s.snapshot_policy,
        )
        .await?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ImportRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::action(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            false,
            &s.snapshot_policy,
        )
        .await?,
    ))
}
async fn records(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::ImportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::records(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await?,
    ))
}
async fn mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::ImportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::mappings(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            query(q)?,
        )
        .await?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::ImportPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::results(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await?,
    ))
}
async fn field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan, record, field) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            record,
            &field,
            query(q)?,
        )
        .await?,
    ))
}
async fn mapping_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan, mapping, field) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::mapping_field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            plan,
            mapping,
            &field,
            query(q)?,
        )
        .await?,
    ))
}
async fn provenance(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::provenance(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            None,
        )
        .await?,
    ))
}
async fn provenance_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, field) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::provenance(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            Some((&field, query(q)?)),
        )
        .await?,
    ))
}

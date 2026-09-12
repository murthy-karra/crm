//! Current-admin retained activity HTTP boundary; source payloads are never request inputs.
use crate::{
    auth::OrgAdminContext,
    domain::{envelope::CommandContext, migration::activity as i},
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
use serde_json::{json, Value};
use uuid::Uuid;
fn activity_error(error: crate::domain::migration::MigrationError) -> ApiError {
    use crate::domain::migration::MigrationError;
    match error {
        MigrationError::SourceNotEligible => ApiError::ImportError("import_conflict"),
        MigrationError::InvalidImportChoice => ApiError::MalformedRequest,
        MigrationError::Crypto => ApiError::Unavailable,
        other => other.into(),
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
        .route(
            "/api/migrations/fub/activity-imports/{id}/records/{record}/observations",
            get(observations),
        )
        .route(
            "/api/migrations/fub/activity-imports",
            get(list).post(prepare),
        )
        .route("/api/migrations/fub/activity-imports/{id}", get(detail))
        .route(
            "/api/migrations/fub/activity-imports/{id}/plans",
            post(replan),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/retry",
            post(retry),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/records",
            get(records),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/mappings",
            get(mappings),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/results",
            get(results),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/targets",
            get(targets),
        )
        .route(
            "/api/migrations/fub/activity-imports/{id}/{endpoint}/{row}/fields/{field}",
            get(field),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("no-store"),
                );
                response
            },
        ))
}
async fn ready(s: &AppState) -> bool {
    s.current_import_release()
        .await
        .is_some_and(|v| v.activity_ready())
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut v = i::list(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &CommandContext::from_auth(&a.auth),
        query(q)?,
        &s.snapshot_policy,
    )
    .await
    .map_err(activity_error)?;
    let ready = ready(&s).await;
    if let Some(items) = v["imports"].as_array_mut() {
        for item in items {
            item["release_ready"] = json!(ready);
            if !ready {
                item["actions"]["confirm"] = json!(false)
            }
        }
    }
    Ok(response(StatusCode::OK, v))
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
    .await
    .map_err(activity_error)?;
    let ready = ready(&s).await;
    v["release_ready"] = json!(ready);
    if !ready {
        v["actions"]["confirm"] = json!(false)
    }
    Ok(response(StatusCode::OK, v))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<i::PrepareActivityImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::CREATED,
        i::prepare(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
            &s.snapshot_policy,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn replan(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::PlanActivityImport>, JsonRejection>,
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
        .await
        .map_err(activity_error)?,
    ))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ConfirmActivityImport>, JsonRejection>,
) -> Result<Response, ApiError> {
    let id = path(p)?;
    let cmd = body(b)?;
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    if let Some(v) = i::replay_confirmation(pool, &s.raw_payload_key, &ctx, id, &cmd)
        .await
        .map_err(activity_error)?
    {
        return Ok(response(StatusCode::ACCEPTED, v));
    }
    let release = s
        .current_import_release()
        .await
        .ok_or(ApiError::Unavailable)?;
    Ok(response(
        StatusCode::ACCEPTED,
        i::confirm(
            pool,
            &s.raw_payload_key,
            &ctx,
            id,
            cmd,
            &release,
            &s.snapshot_policy,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ActivityAction>, JsonRejection>,
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
        .await
        .map_err(activity_error)?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::ActivityAction>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::ACCEPTED,
        i::action(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?,
            false,
            &s.snapshot_policy,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn records(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::records(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::mappings(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
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
        .await
        .map_err(activity_error)?,
    ))
}
async fn targets(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::targets(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(activity_error)?,
    ))
}
async fn field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, String, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, endpoint, row, field) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            &endpoint,
            row,
            &field,
            query(q)?,
        )
        .await
        .map_err(activity_error)?,
    ))
}

async fn observations(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::ActivityPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, record) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::observations(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            record,
            query(q)?,
        )
        .await
        .map_err(activity_error)?,
    ))
}

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
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/targets",get(targets))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/mappings/{mapping}/aliases",get(aliases))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/mappings/{mapping}/fields/{field}",get(mapping_field))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/plans/{plan}/records/{record}/fields/{field}",get(record_field))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/results",get(results))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/results/{result}/fields/{field}",get(result_field))
        .route("/api/migrations/fub/admitted-metadata-imports/{id}/remainder",get(remainder).post(create_remainder))
        .route("/api/people/{person}/admitted-metadata-import-provenance",get(provenance))
        .route("/api/people/{person}/admitted-metadata-import-provenance/{result}/fields/{field}",get(provenance_field))
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
        i::with_policy(
            &s.snapshot_policy,
            i::apply_mappings(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                body(b)?,
            ),
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
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        i::with_policy(
            &s.snapshot_policy,
            i::confirm(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                release.as_deref(),
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                body(b)?,
            ),
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
        i::with_policy(
            &s.snapshot_policy,
            i::retry(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                body(b)?,
            ),
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
        i::with_policy(
            &s.snapshot_policy,
            i::cancel(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                body(b)?,
            ),
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
        i::with_policy(
            &s.snapshot_policy,
            i::mappings(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                id,
                plan,
                query(q)?,
            ),
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
        i::with_policy(
            &s.snapshot_policy,
            i::records(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                id,
                plan,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn issues(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::issues(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                id,
                plan,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn readiness(s: &AppState, value: &mut Value) {
    let ready = s
        .current_import_release()
        .await
        .is_some_and(|r| r.admitted_metadata_ready());
    value["release_ready"] = serde_json::json!(ready);
    if !ready {
        value["actions"]["confirm"] = serde_json::json!(false);
        value["actions"]["remainder"] = serde_json::json!(false);
    }
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<i::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let mut value = i::with_policy(
        &s.snapshot_policy,
        i::list(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            query(q)?,
        ),
    )
    .await?;
    if let Some(items) = value["imports"].as_array_mut() {
        for item in items {
            readiness(&s, item).await;
        }
    }
    Ok(response(StatusCode::OK, value))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let mut value = i::with_policy(
        &s.snapshot_policy,
        i::get(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        ),
    )
    .await?;
    readiness(&s, &mut value).await;
    Ok(response(StatusCode::OK, value))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<i::Prepare>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let cmd = body(b)?;
    let ctx = CommandContext::from_auth(&a.auth);
    if let Some(value) = i::with_policy(
        &s.snapshot_policy,
        i::replay_prepare(pool, &s.raw_payload_key, &ctx, &cmd),
    )
    .await?
    {
        return Ok(response(StatusCode::CREATED, value));
    }
    let release = s
        .current_import_release()
        .await
        .ok_or(ApiError::Unavailable)?;
    Ok(response(
        StatusCode::CREATED,
        i::with_policy(
            &s.snapshot_policy,
            i::prepare(
                pool,
                &s.raw_payload_key,
                &release,
                &CommandContext::from_auth(&a.auth),
                cmd,
            ),
        )
        .await?,
    ))
}

async fn targets(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::targets(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                id,
                plan,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn aliases(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, plan, mapping) = path(p)?;
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::aliases(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                id,
                plan,
                mapping,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::results(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn provenance(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<i::PlanPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::provenance(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                query(q)?,
            ),
        )
        .await?,
    ))
}
async fn remainder(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::get(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &CommandContext::from_auth(&a.auth),
                path(p)?,
            ),
        )
        .await?["remainder"]
            .clone(),
    ))
}
async fn mapping_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (root, plan, id, field) = path(p)?;
    field_response(
        s,
        a,
        i::FieldOwner::Mapping { root, plan, id },
        field,
        query(q)?,
    )
    .await
}
async fn record_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (root, plan, id, field) = path(p)?;
    field_response(
        s,
        a,
        i::FieldOwner::Record { root, plan, id },
        field,
        query(q)?,
    )
    .await
}
async fn result_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (root, id, field) = path(p)?;
    field_response(s, a, i::FieldOwner::Result { root, id }, field, query(q)?).await
}
async fn provenance_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<i::FieldQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (person, id, field) = path(p)?;
    field_response(
        s,
        a,
        i::FieldOwner::Provenance { person, id },
        field,
        query(q)?,
    )
    .await
}
async fn field_response(
    s: AppState,
    a: OrgAdminContext,
    owner: i::FieldOwner,
    field: String,
    q: i::FieldQuery,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        i::with_policy(
            &s.snapshot_policy,
            i::field(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                &CommandContext::from_auth(&a.auth),
                owner,
                &field,
                q,
            ),
        )
        .await?,
    ))
}

async fn create_remainder(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<i::Request>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        i::with_policy(
            &s.snapshot_policy,
            i::create_remainder(
                s.db.as_ref().ok_or(ApiError::Unavailable)?,
                &s.raw_payload_key,
                release.as_deref(),
                &CommandContext::from_auth(&a.auth),
                path(p)?,
                body(b)?,
            ),
        )
        .await?,
    ))
}

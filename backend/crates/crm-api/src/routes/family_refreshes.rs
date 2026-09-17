//! D-092 common retained-family review workflow. All writes use typed commands.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{family_refresh as f, MigrationError},
    },
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;
fn body<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    value.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn path<T>(value: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    value.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(value: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    value.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn error(value: MigrationError) -> ApiError {
    value.into()
}
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/family-refreshes",
            get(list).post(prepare),
        )
        .route("/api/migrations/fub/family-refreshes/{id}", get(detail))
        .route(
            "/api/migrations/fub/family-refreshes/{id}/families",
            get(families),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/mappings",
            get(mappings),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/mappings/{mapping}/targets",
            get(targets),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/items",
            get(items),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/items/{item}/fields",
            get(fields),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/items/{item}/fields/{field}",
            get(field),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/results",
            get(results),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/plans",
            post(plan).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/resume",
            post(resume),
        )
        .route(
            "/api/migrations/fub/family-refreshes/{id}/cancel",
            post(cancel),
        )
        .layer(DefaultBodyLimit::max(8192))
        .layer(middleware::map_response(no_store))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<f::queries::BundlePage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::queries::list(pool, &s.raw_payload_key, &ctx, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn items(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<f::item_queries::ItemPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::item_queries::items(pool, &s.raw_payload_key, &ctx, path(p)?, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<f::mapping_queries::MappingPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::mapping_queries::mappings(pool, &s.raw_payload_key, &ctx, path(p)?, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<f::result_queries::ResultPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::result_queries::results(pool, &s.raw_payload_key, &ctx, path(p)?, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::queries::detail(pool, &ctx, path(p)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn families(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok(Json(
        f::queries::families(pool, &ctx, path(p)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<f::commands::PrepareFamilyRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let release = s.current_import_release().await;
    Ok((
        StatusCode::CREATED,
        Json(
            f::commands::prepare_with_readiness(
                pool,
                &s.raw_payload_key,
                &s.snapshot_policy,
                release.as_deref(),
                &ctx,
                body(b)?,
            )
            .await
            .map_err(error)?,
        ),
    )
        .into_response())
}
async fn plan(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<f::plan_commands::PlanFamilyRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok((
        StatusCode::CREATED,
        Json(
            f::plan_commands::plan(
                pool,
                &s.raw_payload_key,
                &s.snapshot_policy,
                &ctx,
                path(p)?,
                body(b)?,
            )
            .await
            .map_err(error)?,
        ),
    )
        .into_response())
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<f::confirmation::ConfirmFamilyRefresh>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let release = s.current_import_release().await;
    Ok((
        StatusCode::OK,
        Json(
            f::confirmation::confirm_with_readiness(
                pool,
                &s.raw_payload_key,
                release.as_deref(),
                &ctx,
                path(p)?,
                body(b)?,
            )
            .await
            .map_err(error)?,
        ),
    )
        .into_response())
}
async fn resume(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<f::lifecycle::FamilyControl>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let release = s.current_import_release().await;
    Ok((
        StatusCode::OK,
        Json(
            f::resume::resume_with_readiness(
                pool,
                &s.raw_payload_key,
                &s.snapshot_policy,
                release.as_deref(),
                &ctx,
                path(p)?,
                body(b)?,
            )
            .await
            .map_err(error)?,
        ),
    )
        .into_response())
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<f::lifecycle::FamilyControl>, JsonRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    Ok((
        StatusCode::OK,
        Json(
            f::lifecycle::cancel(pool, &s.raw_payload_key, &ctx, path(p)?, body(b)?)
                .await
                .map_err(error)?,
        ),
    )
        .into_response())
}

async fn fields(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<f::field_queries::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let (bundle, item) = path(p)?;
    Ok(Json(
        f::field_queries::fields(pool, &s.raw_payload_key, &ctx, bundle, item, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<f::field_queries::FragmentQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let (bundle, item, field) = path(p)?;
    Ok(Json(
        f::field_queries::fragment(
            pool,
            &s.raw_payload_key,
            &ctx,
            bundle,
            item,
            &field,
            query(q)?,
        )
        .await
        .map_err(error)?,
    )
    .into_response())
}

async fn targets(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<f::mapping_queries::TargetPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let (bundle, mapping) = path(p)?;
    Ok(Json(
        f::mapping_queries::targets(pool, &s.raw_payload_key, &ctx, bundle, mapping, query(q)?)
            .await
            .map_err(error)?,
    )
    .into_response())
}

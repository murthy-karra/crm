//! Current-admin, metadata-only v2 Person review routes.
use crate::{
    auth::OrgAdminContext,
    domain::migration::history_review::{self, PageQuery, ReviewError, TimelineQuery},
    error::ApiError,
    ids::PersonId,
    state::AppState,
};
use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::{header, HeaderValue},
    middleware,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use uuid::Uuid;
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people/{id}/migration-review/v2", get(core))
        .route(
            "/api/people/{id}/migration-review/inquiries",
            get(inquiries),
        )
        .route("/api/people/{id}/migration-review/timeline", get(timeline))
        .route(
            "/api/people/{id}/migration-review/timeline/{kind}/{entry_id}",
            get(entry),
        )
        .layer(middleware::map_response(no_store))
}
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn error(e: ReviewError) -> ApiError {
    match e {
        ReviewError::Forbidden => ApiError::Forbidden,
        ReviewError::NotFound => ApiError::NotFound,
        ReviewError::Malformed => ApiError::MalformedRequest,
        ReviewError::RefreshRequired => ApiError::ImportError("history_refresh_required"),
        ReviewError::Unavailable => ApiError::Unavailable,
        ReviewError::Database(e) => ApiError::database(e),
    }
}
async fn core(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        history_review::core(pool, &auth, PersonId::new(person))
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn inquiries(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        history_review::inquiries(
            pool,
            &state.raw_payload_key,
            &auth,
            PersonId::new(person),
            &query,
        )
        .await
        .map_err(error)?,
    )
    .into_response())
}
async fn timeline(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<TimelineQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        history_review::timeline(
            pool,
            &state.raw_payload_key,
            &auth,
            PersonId::new(person),
            &query,
        )
        .await
        .map_err(error)?,
    )
    .into_response())
}
async fn entry(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<(Uuid, String, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((person, kind, id)) = path.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        history_review::entry(
            pool,
            &state.raw_payload_key,
            &auth,
            PersonId::new(person),
            &kind,
            id,
        )
        .await
        .map_err(error)?,
    )
    .into_response())
}

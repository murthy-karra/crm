//! Admin-only native review representation; separate from the complete legacy DTO.
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
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::OrgAdminContext,
    domain::migration::activity_review::{self, PageQuery, ReviewError, TaskState},
    error::ApiError,
    ids::PersonId,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people/{id}/migration-review", get(detail))
        .route("/api/people/{id}/migration-review/notes", get(notes))
        .route(
            "/api/people/{id}/migration-review/notes/{note_id}",
            get(note),
        )
        .route("/api/people/{id}/migration-review/tasks", get(tasks))
        .layer(middleware::map_response(no_store))
}
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn error(value: ReviewError) -> ApiError {
    match value {
        ReviewError::Forbidden => ApiError::Forbidden,
        ReviewError::NotFound => ApiError::NotFound,
        ReviewError::Malformed => ApiError::MalformedRequest,
        ReviewError::RefreshRequired => ApiError::ImportError("activity_refresh_required"),
        ReviewError::Database(e) => ApiError::database(e),
        ReviewError::Unavailable => ApiError::Unavailable,
    }
}
async fn detail(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        activity_review::detail(pool, &auth, PersonId::new(person))
            .await
            .map_err(error)?,
    )
    .into_response())
}
async fn notes(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        activity_review::notes(
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
async fn note(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((person, note)) = path.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(Json(
        activity_review::note(pool, &auth, PersonId::new(person), note)
            .await
            .map_err(error)?,
    )
    .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TasksQuery {
    state: TaskState,
    limit: Option<usize>,
    cursor: Option<String>,
}
async fn tasks(
    State(state): State<AppState>,
    OrgAdminContext { auth }: OrgAdminContext,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<TasksQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(person) = path.map_err(|_| ApiError::MalformedRequest)?;
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let page = PageQuery {
        limit: query.limit,
        cursor: query.cursor,
    };
    Ok(Json(
        activity_review::tasks(
            pool,
            &state.raw_payload_key,
            &auth,
            PersonId::new(person),
            query.state,
            &page,
        )
        .await
        .map_err(error)?,
    )
    .into_response())
}

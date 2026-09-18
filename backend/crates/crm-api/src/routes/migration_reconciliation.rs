//! Read-only reconciliation summary for one retained original import.
use crate::{
    auth::OrgAdminContext,
    domain::{envelope::CommandContext, migration::reconciliation},
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{rejection::PathRejection, Path, Request, State},
    http::{header, HeaderValue, Method},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use uuid::Uuid;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

fn path<T>(value: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    value.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
pub async fn cache_policy(request: Request, next: Next) -> Response {
    let scoped = request.method() == Method::GET
        && request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/imports/")
        && request.uri().path().ends_with("/reconciliation");
    let mut response = next.run(request).await;
    if scoped {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}
pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/migrations/fub/imports/{id}/reconciliation",
        get(detail),
    )
}
async fn detail(
    State(state): State<AppState>,
    auth: OrgAdminContext,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    let summary = reconciliation::summary(
        state.db.as_ref().ok_or(ApiError::Unavailable)?,
        &CommandContext::from_auth(&auth.auth),
        path(id)?,
    )
    .await?;
    if serde_json::to_vec(&summary)
        .map_err(|_| ApiError::InternalError)?
        .len()
        > MAX_RESPONSE_BYTES
    {
        return Err(ApiError::PayloadTooLarge);
    }
    Ok(Json(summary).into_response())
}

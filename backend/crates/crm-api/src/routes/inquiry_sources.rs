use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::auth::AuthContext;
use crate::domain::inquiry::queries as inquiry_queries;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/inquiry-sources", get(list_inquiry_sources))
}

/// `GET /api/inquiry-sources` (docs/specs/SLICE_011a.md §5b): feeds the
/// FilterBar's Source picker. `auth.active_organization_id` only — org
/// data, not Person visibility (the `GET /api/stages` pattern). Any
/// authenticated member.
async fn list_inquiry_sources(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let (sources, truncated) =
        inquiry_queries::distinct_sources(&mut conn, auth.active_organization_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "sources": sources, "truncated": truncated })))
}

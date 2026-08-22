use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::auth::AuthContext;
use crate::domain::stage;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/stages", get(list_stages))
}

/// `auth.active_organization_id` only — stages are Organization data, not
/// Person visibility (docs/specs/SLICE_002.md §4).
async fn list_stages(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let stages = stage::list(&mut conn, auth.active_organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "stages": stages })))
}

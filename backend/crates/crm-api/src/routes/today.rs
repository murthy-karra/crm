//! `GET /api/today` (docs/specs/SLICE_003.md §5).

use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::Utc;

use crate::auth::AuthContext;
use crate::domain::person::PersonVisibilityScope;
use crate::domain::today;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/today", get(get_today))
}

/// No parameters: the viewer is always `AuthContext.actor_user_id`, never
/// client input (docs/specs/SLICE_003.md §5, §7).
async fn get_today(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<today::TodayList>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let scope = PersonVisibilityScope::from_auth(&auth);

    let list = today::query(&mut conn, &scope, auth.actor_user_id, Utc::now())
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(list))
}

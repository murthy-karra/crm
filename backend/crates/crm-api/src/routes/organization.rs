use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/organization/members", get(members))
}

#[derive(Serialize)]
struct MemberPayload {
    user_id: Uuid,
    display_name: String,
    email: String,
    joined_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct MembersResponse {
    members: Vec<MemberPayload>,
}

/// `auth.active_organization_id` is the only Organization selector here —
/// it comes solely from the server-verified session (AGENTS.md §4.2), never
/// from a client-supplied query string, header, or body field.
async fn members(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<MembersResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let rows = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
        "SELECT u.id, u.display_name, u.email, m.created_at
         FROM organization_membership m
         JOIN app_user u ON u.id = m.user_id
         WHERE m.organization_id = $1
         ORDER BY m.created_at, u.id",
    )
    .bind(auth.active_organization_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let members = rows
        .into_iter()
        .map(|(user_id, display_name, email, joined_at)| MemberPayload {
            user_id,
            display_name,
            email,
            joined_at,
        })
        .collect();

    Ok(Json(MembersResponse { members }))
}

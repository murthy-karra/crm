//! `POST /api/realtime/token` (docs/specs/SLICE_003.md §5, §6).

use axum::extract::State;
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use serde_json::json;

use crate::auth::AuthContext;
use crate::realtime::token;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/realtime/token", post(mint_token))
}

/// Minting is entirely local (HMAC) and never contacts Centrifugo. 401
/// (including a revoked membership — `AuthContext` re-verifies membership
/// on every request, `docs/specs/SLICE_003.md` §7) and 503 on database
/// failure are produced by the `AuthContext` extractor itself, exactly as
/// every other authenticated endpoint (docs/specs/SLICE_003.md §5). The
/// token itself is never logged (docs/specs/SLICE_003.md §8).
#[tracing::instrument(
    skip_all,
    fields(actor_id = %auth.actor_user_id, organization_id = %auth.active_organization_id)
)]
async fn mint_token(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, crate::error::ApiError> {
    let pool = state
        .db
        .as_ref()
        .ok_or(crate::error::ApiError::Unavailable)?;
    let mut tx = crate::auth::workspace::begin(pool, auth.active_organization_id)
        .await
        .map_err(crate::error::ApiError::database)?;
    crate::auth::workspace::read_check(
        &mut tx,
        auth.active_organization_id,
        auth.actor_user_id,
        true,
    )
    .await
    .map_err(crate::error::ApiError::database)?;
    let jwt = token::mint(
        &state.realtime_token_secret,
        auth.actor_user_id,
        auth.active_organization_id,
        Utc::now(),
        state.realtime_token_ttl,
    );
    tx.rollback()
        .await
        .map_err(crate::error::ApiError::database)?;
    Ok(Json(json!({ "token": jwt })))
}

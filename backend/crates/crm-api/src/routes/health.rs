use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/internal/ready", get(ready))
}

/// Liveness: no dependencies, always 200.
async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// Readiness: 200 only if a configured database answers `SELECT 1` within
/// the bound; 503 if no database is configured, unreachable, or slow.
/// Deliberately outside `/api` so the Vite proxy and tunnel never expose it.
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let Some(pool) = state.db.as_ref() else {
        return not_ready();
    };

    // sqlx's own pool `acquire_timeout` only bounds obtaining a connection;
    // once a connection is acquired, query execution has no sqlx-level
    // timeout. This wrapper is the only thing bounding a database that is
    // reachable but hung (lock contention, stuck failover) — do not remove
    // it as "redundant" with the pool's acquire_timeout.
    let check = sqlx::query("SELECT 1").execute(pool);
    match tokio::time::timeout(state.database_connect_timeout, check).await {
        Ok(Ok(_)) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "readiness check: database query failed");
            not_ready()
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = state.database_connect_timeout.as_millis(),
                "readiness check: timed out"
            );
            not_ready()
        }
    }
}

fn not_ready() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "status": "not_ready" })),
    )
}

//! `GET /api/today` (docs/specs/SLICE_003.md §5).

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::response::Json;
use axum::routing::{delete, get, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
#[cfg(feature = "test-support")]
use std::time::Instant;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::envelope::CommandContext;
use crate::domain::person::PersonVisibilityScope;
use crate::domain::today::{self, DisableTodayWorkSource, EnableTodayWorkSource};
use crate::error::ApiError;
use crate::ids::SavedListId;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/today", get(get_today))
        .merge(source_control_router_inner())
}

/// Source controls are shared with the test-only frozen-Today router used by
/// the Phase B parity harness. The harness substitutes only `GET /api/today`;
/// it must still establish each viewer's source mode through the ordinary
/// authenticated PUT/DELETE/GET contract before a measured wave.
#[cfg(feature = "test-support")]
pub fn source_control_router() -> Router<AppState> {
    source_control_router_inner()
}

fn source_control_router_inner() -> Router<AppState> {
    Router::new()
        .route("/api/today/sources", get(get_sources))
        .route(
            "/api/today/sources/{id}",
            put(enable_source).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route("/api/today/sources/{id}", delete(disable_source))
}

/// Test-only variant used by the isolated Phase B parity harness. The clock
/// is captured by the server-side router at construction, never selected by
/// a client, and `query_owned_at` still executes the ordinary snapshot-clock
/// statement before substituting it for Today evaluation boundaries.
#[cfg(feature = "test-support")]
pub fn router_with_test_clock(now: chrono::DateTime<chrono::Utc>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/today",
            get(move |state: State<AppState>, auth: AuthContext| {
                let now = now;
                async move { get_today_at(state, auth, now).await }
            }),
        )
        .merge(source_control_router_inner())
}

/// Match saved-list's path precedence: a malformed UUID is a bare 400 before
/// session lookup, while a syntactically valid invisible ID stays a 404.
struct SourceListIdPath(SavedListId);

impl FromRequestParts<AppState> for SourceListIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(Self(SavedListId::new(id)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnableSourceRequest {
    expected_list_revision: i64,
}

/// No parameters: the viewer is always `AuthContext.actor_user_id`, never
/// client input (docs/specs/SLICE_003.md §5, §7).
async fn get_today(
    state: State<AppState>,
    auth: AuthContext,
) -> Result<Json<today::TodayList>, ApiError> {
    get_today_with_clock(state, auth, None).await
}

#[cfg(feature = "test-support")]
async fn get_today_at(
    state: State<AppState>,
    auth: AuthContext,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Json<today::TodayList>, ApiError> {
    get_today_with_clock(state, auth, Some(now)).await
}

async fn get_today_with_clock(
    State(state): State<AppState>,
    auth: AuthContext,
    #[cfg(feature = "test-support")] now: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg(not(feature = "test-support"))] _now: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Json<today::TodayList>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    #[cfg(feature = "test-support")]
    let acquire_started = Instant::now();
    let conn = match pool.acquire().await {
        Ok(conn) => {
            #[cfg(feature = "test-support")]
            crate::domain::today::test_support::record_feed_pool_acquisition(
                crate::domain::today::test_support::PoolAcquisitionOutcome::Acquired,
                acquire_started.elapsed(),
            );
            conn
        }
        Err(_error) => {
            #[cfg(feature = "test-support")]
            crate::domain::today::test_support::record_feed_pool_acquisition(
                if matches!(&_error, sqlx::Error::PoolTimedOut) {
                    crate::domain::today::test_support::PoolAcquisitionOutcome::TimedOut
                } else {
                    crate::domain::today::test_support::PoolAcquisitionOutcome::Failed
                },
                acquire_started.elapsed(),
            );
            return Err(ApiError::Unavailable);
        }
    };
    let scope = PersonVisibilityScope::from_auth(&auth);

    #[cfg(feature = "test-support")]
    let list = match now {
        Some(now) => today::query_owned_at(conn, &scope, auth.actor_user_id, now).await,
        None => today::query_owned(conn, &scope, auth.actor_user_id, chrono::Utc::now()).await,
    }
    .map_err(ApiError::database)?;
    #[cfg(not(feature = "test-support"))]
    let list = today::query_owned(conn, &scope, auth.actor_user_id, chrono::Utc::now())
        .await
        .map_err(ApiError::database)?;

    Ok(Json(list))
}

async fn get_sources(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(ApiError::database)?;
    let sources = today::list_today_work_sources(&mut conn, &auth).await?;
    Ok(Json(
        json!({ "limit": today::TODAY_SOURCE_LIMIT, "sources": sources }),
    ))
}

async fn enable_source(
    State(state): State<AppState>,
    SourceListIdPath(list_id): SourceListIdPath,
    auth: AuthContext,
    body: Result<Json<EnableSourceRequest>, JsonRejection>,
) -> Result<Json<today::TodaySourceChange>, ApiError> {
    let Json(body) = body.map_err(|_| ApiError::MalformedRequest)?;
    // Validate before resource lookup so malformed request bodies are never
    // distinguishable by resource visibility.
    crate::domain::saved_list::validate_expected_revision(body.expected_list_revision)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let result = today::enable_today_work_source(
        pool,
        &CommandContext::from_auth(&auth),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision: body.expected_list_revision,
        },
    )
    .await?;
    Ok(Json(result))
}

async fn disable_source(
    State(state): State<AppState>,
    SourceListIdPath(list_id): SourceListIdPath,
    auth: AuthContext,
) -> Result<Json<today::TodaySourceChange>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let result = today::disable_today_work_source(
        pool,
        &CommandContext::from_auth(&auth),
        DisableTodayWorkSource { list_id },
    )
    .await?;
    Ok(Json(result))
}

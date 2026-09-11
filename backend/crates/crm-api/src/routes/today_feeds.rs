//! System-feed HTTP surface (docs/specs/SLICE_011d.md §6). The command/read
//! module owns all persistence authorization; this adapter owns only
//! trusted extractor order, strict wire decoding, and the stable JSON
//! envelopes. `OrgAdminContext` runs before body parsing on every admin
//! route, matching every other admin surface (SLICE_004 §5); the command
//! itself re-checks `Role::Admin` inside its locked transaction.

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::response::Json;
use axum::routing::{get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{AuthContext, OrgAdminContext};
use crate::domain::envelope::CommandContext;
use crate::domain::person::filter::FilterDefinition;
use crate::domain::today::system_feeds::commands::{
    self, PreviewTodaySystemFeed, RevertTodaySystemFeed, SetTodaySystemFeedEnabled,
    UpdateTodaySystemFeed,
};
use crate::domain::today::system_feeds::queries;
use crate::domain::today::system_feeds::FeedKey;
use crate::error::ApiError;
use crate::ids::UserId;
use crate::state::AppState;

const MAX_TODAY_FEEDS_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/organization/today-feeds", get(list_today_feeds_admin))
        .route(
            "/api/organization/today-feeds/{feed_key}",
            put(update_today_feed).layer(DefaultBodyLimit::max(MAX_TODAY_FEEDS_BODY_BYTES)),
        )
        .route(
            "/api/organization/today-feeds/{feed_key}/revert",
            post(revert_today_feed).layer(DefaultBodyLimit::max(MAX_TODAY_FEEDS_BODY_BYTES)),
        )
        .route(
            "/api/organization/today-feeds/{feed_key}/enabled",
            put(set_today_feed_enabled).layer(DefaultBodyLimit::max(MAX_TODAY_FEEDS_BODY_BYTES)),
        )
        .route(
            "/api/organization/today-feeds/{feed_key}/preview",
            post(preview_today_feed).layer(DefaultBodyLimit::max(MAX_TODAY_FEEDS_BODY_BYTES)),
        )
        .route("/api/today/feeds", get(list_today_feeds_member))
}

/// Body parsing precedes the feed-key lookup (spec §6 precedence: 400
/// before 404), so this decodes a plain string rather than failing closed
/// on an unrecognized token itself — the caller maps it to `FeedKey` (404
/// if unknown) only after any body has already been validated.
fn decode_feed_key(raw: &str) -> Result<FeedKey, ApiError> {
    FeedKey::decode(raw).ok_or(ApiError::NotFound)
}

/// Body-size failures (axum's `BytesRejection`, wrapped inside
/// `JsonRejection` since `Json<T>` buffers through `Bytes` first under
/// `DefaultBodyLimit`) must map to `ApiError::PayloadTooLarge` (413), the
/// SAME mapping every other oversize-body route in this codebase uses
/// (`routes/inbound_email.rs`) — not the blanket 400 a bare `map_err`
/// would give every JsonRejection variant alike.
fn map_body_rejection(rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::BytesRejection(_) => ApiError::PayloadTooLarge,
        _ => ApiError::MalformedRequest,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateTodayFeedRequest {
    expected_revision: i64,
    filter: FilterDefinition,
    #[serde(default)]
    fresh_within_hours: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevertTodayFeedRequest {
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetTodayFeedEnabledRequest {
    expected_revision: i64,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewTodayFeedRequest {
    filter: FilterDefinition,
    #[serde(default)]
    fresh_within_hours: Option<i32>,
    #[serde(default)]
    subject_user_id: Option<uuid::Uuid>,
}

async fn list_today_feeds_admin(
    State(state): State<AppState>,
    admin: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(ApiError::database)?;
    let feeds = queries::admin_feed_view(&mut conn, admin.auth.active_organization_id).await?;
    Ok(Json(json!({ "feeds": feeds })))
}

async fn list_today_feeds_member(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(ApiError::database)?;
    let feeds = queries::member_feed_view(&mut conn, auth.active_organization_id)
        .await
        .map_err(ApiError::database)?;
    Ok(Json(json!({ "feeds": feeds })))
}

async fn update_today_feed(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    Path(raw_feed_key): Path<String>,
    body: Result<Json<UpdateTodayFeedRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(map_body_rejection)?;
    let feed_key = decode_feed_key(&raw_feed_key)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = commands::update_today_system_feed(
        pool,
        &CommandContext::from_auth(&admin.auth),
        UpdateTodaySystemFeed {
            feed_key,
            expected_revision: req.expected_revision,
            filter: req.filter,
            fresh_within_hours: req.fresh_within_hours,
        },
    )
    .await?;
    Ok(Json(
        json!({ "feed": outcome.feed, "changed": outcome.changed }),
    ))
}

async fn revert_today_feed(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    Path(raw_feed_key): Path<String>,
    body: Result<Json<RevertTodayFeedRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(map_body_rejection)?;
    let feed_key = decode_feed_key(&raw_feed_key)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = commands::revert_today_system_feed(
        pool,
        &CommandContext::from_auth(&admin.auth),
        RevertTodaySystemFeed {
            feed_key,
            expected_revision: req.expected_revision,
        },
    )
    .await?;
    Ok(Json(
        json!({ "feed": outcome.feed, "changed": outcome.changed }),
    ))
}

async fn set_today_feed_enabled(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    Path(raw_feed_key): Path<String>,
    body: Result<Json<SetTodayFeedEnabledRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(map_body_rejection)?;
    let feed_key = decode_feed_key(&raw_feed_key)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = commands::set_today_system_feed_enabled(
        pool,
        &CommandContext::from_auth(&admin.auth),
        SetTodaySystemFeedEnabled {
            feed_key,
            expected_revision: req.expected_revision,
            enabled: req.enabled,
        },
    )
    .await?;
    Ok(Json(
        json!({ "feed": outcome.feed, "changed": outcome.changed }),
    ))
}

async fn preview_today_feed(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    Path(raw_feed_key): Path<String>,
    body: Result<Json<PreviewTodayFeedRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(map_body_rejection)?;
    let feed_key = decode_feed_key(&raw_feed_key)?;
    // Defaults to the admin themself (spec §4).
    let subject = req
        .subject_user_id
        .map(UserId::new)
        .unwrap_or(admin.auth.actor_user_id);

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = commands::preview_today_system_feed(
        pool,
        &CommandContext::from_auth(&admin.auth),
        PreviewTodaySystemFeed {
            feed_key,
            filter: req.filter,
            fresh_within_hours: req.fresh_within_hours,
            subject,
        },
    )
    .await?;
    Ok(Json(json!({
        "subject": { "id": outcome.subject, "display_name": outcome.subject_display_name },
        "items": outcome.items,
        "truncated": outcome.truncated,
        "description": outcome.description,
    })))
}

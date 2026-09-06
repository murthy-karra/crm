//! Saved-list HTTP surface (Slice 011b). The command/read module owns all
//! persistence authorization; this adapter owns only trusted extractor order,
//! strict wire decoding, and the stable JSON envelopes.

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::envelope::CommandContext;
use crate::domain::person::filter::FilterDefinition;
use crate::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListScope, UpdateSavedList,
};
use crate::error::ApiError;
use crate::ids::SavedListId;
use crate::state::AppState;

const MAX_SAVED_LIST_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/saved-lists", get(list_saved_lists))
        .route(
            "/api/saved-lists",
            post(create_saved_list).layer(DefaultBodyLimit::max(MAX_SAVED_LIST_BODY_BYTES)),
        )
        .route("/api/saved-lists/{id}", get(get_saved_list))
        .route(
            "/api/saved-lists/{id}",
            put(update_saved_list).layer(DefaultBodyLimit::max(MAX_SAVED_LIST_BODY_BYTES)),
        )
        .route(
            "/api/saved-lists/{id}",
            delete(delete_saved_list).layer(DefaultBodyLimit::max(MAX_SAVED_LIST_BODY_BYTES)),
        )
        .route("/api/saved-lists/{id}/count", get(count_saved_list))
}

/// A typed path wrapper keeps malformed IDs ahead of authentication, matching
/// the established Person/call/intake route precedent. The application id is
/// foreign to this crate, so this local wrapper is required by Rust's orphan
/// rules.
struct SavedListIdPath(SavedListId);

impl FromRequestParts<AppState> for SavedListIdPath {
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

/// UUID's default parser accepts several textual spellings. Saved-list body
/// retry keys intentionally accept only the canonical lower-case form so the
/// persisted/retried request identity is wire-stable.
fn deserialize_canonical_uuid<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Uuid, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let bytes = raw.as_bytes();
    let canonical_shape = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        });
    if !canonical_shape {
        return Err(D::Error::custom(
            "UUID must be canonical lowercase-hyphenated",
        ));
    }
    Uuid::parse_str(&raw).map_err(D::Error::custom)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSavedListRequest {
    #[serde(deserialize_with = "deserialize_canonical_uuid")]
    request_id: Uuid,
    scope: SavedListScope,
    name: String,
    filter: FilterDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSavedListRequest {
    expected_revision: i64,
    name: String,
    filter: FilterDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSavedListRequest {
    expected_revision: i64,
}

/// Query parsing deliberately stays permissive of unrelated keys, as the
/// existing People GET surface does. A scalar serde field rejects a duplicate
/// `revision`, while this parser additionally pins the stable decimal wire
/// form instead of accepting `+1`, leading zeroes, or a JSON-style exponent.
#[derive(Deserialize)]
struct CountSavedListQuery {
    revision: Option<String>,
}

fn parse_canonical_revision(raw: Option<String>) -> Result<i64, ApiError> {
    let raw = raw.ok_or(ApiError::MalformedRequest)?;
    if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::MalformedRequest);
    }
    let revision = raw.parse::<i64>().map_err(|_| ApiError::MalformedRequest)?;
    saved_list::validate_expected_revision(revision).map_err(ApiError::from)?;
    Ok(revision)
}

async fn list_saved_lists(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let lists = saved_list::list_saved_lists(&mut conn, &auth).await?;
    Ok(Json(json!({ "lists": lists })))
}

async fn get_saved_list(
    State(state): State<AppState>,
    SavedListIdPath(list_id): SavedListIdPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let detail = saved_list::saved_list_detail(&mut conn, &auth, list_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(json!({
        "list": detail.list,
        "filter": detail.filter,
        "description": detail.description,
        "filter_error": detail.filter_error.map(|error| error.as_str()),
    })))
}

/// Auth is deliberately extracted before the body result wrapper: malformed
/// JSON or a structurally bad filter never gets parsed for an unauthenticated
/// request. The command repeats pure validation for non-HTTP callers.
async fn create_saved_list(
    State(state): State<AppState>,
    auth: AuthContext,
    body: Result<Json<CreateSavedListRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    // Retain the specified 401 -> 400 -> scope authorization order. The
    // command performs the same checks inside its membership-locked tx.
    saved_list::normalize_and_validate_input(&req.name, &req.filter)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = saved_list::create_saved_list(
        pool,
        &CommandContext::from_auth(&auth),
        CreateSavedList {
            request_id: req.request_id,
            scope: req.scope,
            name: req.name,
            filter: req.filter,
        },
    )
    .await?;
    let status = if outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(json!({ "list": outcome.list, "created": outcome.created })),
    ))
}

async fn update_saved_list(
    State(state): State<AppState>,
    SavedListIdPath(list_id): SavedListIdPath,
    auth: AuthContext,
    body: Result<Json<UpdateSavedListRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    saved_list::validate_expected_revision(req.expected_revision)?;
    saved_list::normalize_and_validate_input(&req.name, &req.filter)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = saved_list::update_saved_list(
        pool,
        &CommandContext::from_auth(&auth),
        UpdateSavedList {
            list_id,
            expected_revision: req.expected_revision,
            name: req.name,
            filter: req.filter,
        },
    )
    .await?;
    Ok(Json(
        json!({ "list": outcome.list, "changed": outcome.changed }),
    ))
}

async fn delete_saved_list(
    State(state): State<AppState>,
    SavedListIdPath(list_id): SavedListIdPath,
    auth: AuthContext,
    body: Result<Json<DeleteSavedListRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    saved_list::validate_expected_revision(req.expected_revision)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = saved_list::delete_saved_list(
        pool,
        &CommandContext::from_auth(&auth),
        DeleteSavedList {
            list_id,
            expected_revision: req.expected_revision,
        },
    )
    .await?;
    Ok(Json(json!({ "deleted": outcome.deleted })))
}

async fn count_saved_list(
    State(state): State<AppState>,
    SavedListIdPath(list_id): SavedListIdPath,
    auth: AuthContext,
    query: Result<Query<CountSavedListQuery>, QueryRejection>,
) -> Result<Json<saved_list::SavedListCount>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    let revision = parse_canonical_revision(query.revision)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let count = saved_list::count_saved_list_matches(&mut conn, &auth, list_id, revision)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(count))
}

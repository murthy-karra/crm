//! Tag HTTP surface (Slice 011e, e1). The command/read module owns rule-1
//! authorization (D-051), quota, and idempotency; this adapter owns only
//! trusted extractor order, strict wire decoding, and the stable JSON
//! envelopes — the `saved_lists.rs` adapter's shape.

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::envelope::CommandContext;
use crate::domain::tag::{self, CreateTag, DeleteTag, RenameTag, Tag};
use crate::error::ApiError;
use crate::ids::TagId;
use crate::state::AppState;

const MAX_TAG_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tags", get(list_tags))
        .route(
            "/api/tags",
            post(create_tag).layer(DefaultBodyLimit::max(MAX_TAG_BODY_BYTES)),
        )
        .route(
            "/api/tags/{id}",
            put(rename_tag).layer(DefaultBodyLimit::max(MAX_TAG_BODY_BYTES)),
        )
        .route("/api/tags/{id}", delete(delete_tag))
}

/// A typed path wrapper keeps a malformed id ahead of authentication,
/// matching the established Person/saved-list precedent
/// (docs/specs/SLICE_011e.md §5 error precedence).
struct TagIdPath(TagId);

impl FromRequestParts<AppState> for TagIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(Self(TagId::new(id)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTagRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameTagRequest {
    name: String,
}

/// `GET /api/tags` (docs/specs/SLICE_011e.md §5): the full index, ordered
/// `lower(name), id`, each row's `can_manage` computed for THIS viewer
/// (admin: all true; member: true only for their own unused tags) — a
/// display hint; every write command re-decides the same rule under the
/// tag row's lock.
async fn list_tags(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(ApiError::database)?;
    let rows = tag::list_for_organization(&mut conn, auth.active_organization_id).await?;
    let tags: Vec<Tag> = rows
        .into_iter()
        .map(|row| Tag {
            can_manage: tag::can_manage(
                auth.role,
                auth.actor_user_id,
                row.created_by_user_id,
                row.person_count,
            ),
            id: row.id,
            name: row.name,
            person_count: row.person_count,
        })
        .collect();
    Ok(Json(json!({ "tags": tags })))
}

async fn create_tag(
    State(state): State<AppState>,
    auth: AuthContext,
    body: Result<Json<CreateTagRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    // Retains the 401 -> 400 -> 409 order; the command repeats pure
    // validation for non-HTTP callers.
    tag::normalize_and_validate_name(&req.name)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = tag::create_tag(
        pool,
        &CommandContext::from_auth(&auth),
        CreateTag { name: req.name },
    )
    .await?;
    let status = if outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(json!({ "tag": outcome.tag, "created": outcome.created })),
    ))
}

async fn rename_tag(
    State(state): State<AppState>,
    TagIdPath(tag_id): TagIdPath,
    auth: AuthContext,
    body: Result<Json<RenameTagRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    tag::normalize_and_validate_name(&req.name)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = tag::rename_tag(
        pool,
        &CommandContext::from_auth(&auth),
        RenameTag {
            tag_id,
            name: req.name,
        },
    )
    .await?;
    Ok(Json(
        json!({ "tag": outcome.tag, "changed": outcome.changed }),
    ))
}

async fn delete_tag(
    State(state): State<AppState>,
    TagIdPath(tag_id): TagIdPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = tag::delete_tag(
        pool,
        &CommandContext::from_auth(&auth),
        DeleteTag { tag_id },
    )
    .await?;
    Ok(Json(json!({
        "deleted": outcome.deleted,
        "removed_from_people": outcome.removed_from_people,
    })))
}

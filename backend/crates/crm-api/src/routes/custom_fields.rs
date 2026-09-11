//! Custom-field definition/option HTTP surface (Slice 019a). The
//! command/read module owns admin-only authorization (D-058 §2), quota,
//! and idempotency; this adapter owns only trusted extractor order,
//! strict wire decoding, and the stable JSON envelopes — the
//! `tags.rs`/`tasks.rs` adapter shape. Handlers order extractors `Path,
//! OrgAdminContext, Json` (docs/specs/SLICE_019.md §4): malformed path
//! uuid 400 → 401 → 403 (admin writes only) → 400 body → 404 → 409 → 422
//! → 503.

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::{AuthContext, OrgAdminContext};
use crate::domain::custom_field::{
    self, AddCustomFieldOption, CreateCustomField, FieldType, ReorderCustomFields,
    UpdateCustomField, UpdateCustomFieldOption,
};
use crate::domain::envelope::CommandContext;
use crate::error::ApiError;
use crate::ids::{CustomFieldId, CustomFieldOptionId};
use crate::state::AppState;

const MAX_CUSTOM_FIELD_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/custom-fields", get(list_custom_fields))
        .route(
            "/api/custom-fields",
            post(create_custom_field).layer(DefaultBodyLimit::max(MAX_CUSTOM_FIELD_BODY_BYTES)),
        )
        // A static segment beside `{field_id}`: axum's router gives the
        // literal route priority, so `order` never reaches the uuid path
        // extractor below (docs/specs/SLICE_019.md §4, pinned by a test).
        .route(
            "/api/custom-fields/order",
            put(reorder_custom_fields).layer(DefaultBodyLimit::max(MAX_CUSTOM_FIELD_BODY_BYTES)),
        )
        .route(
            "/api/custom-fields/{field_id}",
            put(update_custom_field).layer(DefaultBodyLimit::max(MAX_CUSTOM_FIELD_BODY_BYTES)),
        )
        .route(
            "/api/custom-fields/{field_id}/options",
            post(add_custom_field_option)
                .layer(DefaultBodyLimit::max(MAX_CUSTOM_FIELD_BODY_BYTES)),
        )
        .route(
            "/api/custom-fields/{field_id}/options/{option_id}",
            put(update_custom_field_option)
                .layer(DefaultBodyLimit::max(MAX_CUSTOM_FIELD_BODY_BYTES)),
        )
}

/// A `{field_id}` path segment, matching the established `PersonIdPath`/
/// `TagIdPath` precedent: a malformed id is 400 independent of auth
/// state, tested service-free.
struct CustomFieldIdPath(CustomFieldId);

impl FromRequestParts<AppState> for CustomFieldIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(Self(CustomFieldId::new(id)))
    }
}

/// The `{field_id}/options/{option_id}` pair (docs/specs/SLICE_019.md
/// §4), the `PersonTagIdsPath` pattern: either id being a non-UUID is a
/// 400 independent of auth state.
struct CustomFieldOptionIdsPath(CustomFieldId, CustomFieldOptionId);

impl FromRequestParts<AppState> for CustomFieldOptionIdsPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path((field_id, option_id)) = Path::<(Uuid, Uuid)>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(Self(
            CustomFieldId::new(field_id),
            CustomFieldOptionId::new(option_id),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCustomFieldRequest {
    label: String,
    field_type: FieldType,
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReorderCustomFieldsRequest {
    field_ids: Vec<CustomFieldId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCustomFieldRequest {
    label: String,
    archived: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddCustomFieldOptionRequest {
    label: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCustomFieldOptionRequest {
    label: String,
    archived: bool,
}

/// `GET /api/custom-fields` (docs/specs/SLICE_019.md §4): any active
/// member; live fields first ordered `position, id`, then archived
/// ordered `archived_at DESC, id`.
async fn list_custom_fields(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let fields = custom_field::list_definitions(&mut conn, auth.active_organization_id).await?;
    Ok(Json(json!({ "fields": fields })))
}

/// `POST /api/custom-fields` (docs/specs/SLICE_019.md §4): admin only at
/// the extractor; the command re-checks under the `custom_fields:<org>`
/// lock.
async fn create_custom_field(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    body: Result<Json<CreateCustomFieldRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = custom_field::create_custom_field(
        pool,
        &CommandContext::from_auth(&admin.auth),
        CreateCustomField {
            label: req.label,
            field_type: req.field_type,
            options: req.options,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "field": outcome.field }))))
}

/// `PUT /api/custom-fields/order` (docs/specs/SLICE_019.md §4): admin
/// only; the full live order, no more, no fewer.
async fn reorder_custom_fields(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    body: Result<Json<ReorderCustomFieldsRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = custom_field::reorder_custom_fields(
        pool,
        &CommandContext::from_auth(&admin.auth),
        ReorderCustomFields {
            field_ids: req.field_ids,
        },
    )
    .await?;

    Ok(Json(json!({ "fields": outcome.fields })))
}

/// `PUT /api/custom-fields/{field_id}` (docs/specs/SLICE_019.md §4):
/// admin only; full-replace rename/archive/restore.
async fn update_custom_field(
    State(state): State<AppState>,
    CustomFieldIdPath(field_id): CustomFieldIdPath,
    admin: OrgAdminContext,
    body: Result<Json<UpdateCustomFieldRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = custom_field::update_custom_field(
        pool,
        &CommandContext::from_auth(&admin.auth),
        UpdateCustomField {
            field_id,
            label: req.label,
            archived: req.archived,
        },
    )
    .await?;

    Ok(Json(json!({
        "field": outcome.field,
        "changed": outcome.changed,
    })))
}

/// `POST /api/custom-fields/{field_id}/options` (docs/specs/SLICE_019.md
/// §4): admin only; choice fields only, permitted on an archived field.
async fn add_custom_field_option(
    State(state): State<AppState>,
    CustomFieldIdPath(field_id): CustomFieldIdPath,
    admin: OrgAdminContext,
    body: Result<Json<AddCustomFieldOptionRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = custom_field::add_custom_field_option(
        pool,
        &CommandContext::from_auth(&admin.auth),
        AddCustomFieldOption {
            field_id,
            label: req.label,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "field": outcome.field }))))
}

/// `PUT /api/custom-fields/{field_id}/options/{option_id}` (docs/specs/
/// SLICE_019.md §4): admin only; full-replace rename/archive/restore.
async fn update_custom_field_option(
    State(state): State<AppState>,
    CustomFieldOptionIdsPath(field_id, option_id): CustomFieldOptionIdsPath,
    admin: OrgAdminContext,
    body: Result<Json<UpdateCustomFieldOptionRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = custom_field::update_custom_field_option(
        pool,
        &CommandContext::from_auth(&admin.auth),
        UpdateCustomFieldOption {
            field_id,
            option_id,
            label: req.label,
            archived: req.archived,
        },
    )
    .await?;

    Ok(Json(json!({
        "field": outcome.field,
        "changed": outcome.changed,
    })))
}

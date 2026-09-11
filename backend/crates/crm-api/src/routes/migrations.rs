//! Admin-only Slice 010a FUB assessment API. Credentials only exist in the
//! request body long enough to validate and seal them; no response contains
//! the key, its suffix, ciphertext, or source response bytes.
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::OrgAdminContext;
use crate::domain::envelope::CommandContext;
use crate::domain::migration::commands;
use crate::error::ApiError;
use crate::state::AppState;

const MAX_CREDENTIAL_BODY_BYTES: usize = 16 * 1024;
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(super::migration_snapshots::router())
        .route("/api/migrations/fub/", get(summary))
        .route(
            "/api/migrations/fub/connections",
            post(create_connection).layer(DefaultBodyLimit::max(MAX_CREDENTIAL_BODY_BYTES)),
        )
        .route(
            "/api/migrations/fub/connections/{id}/credential",
            put(replace_credential).layer(DefaultBodyLimit::max(MAX_CREDENTIAL_BODY_BYTES)),
        )
        .route("/api/migrations/fub/connections/{id}", delete(disconnect))
        .route("/api/migrations/fub/assessments", post(start_assessment))
        .route("/api/migrations/fub/assessments/{id}", get(get_assessment))
        .route(
            "/api/migrations/fub/assessments/{id}/retry",
            post(retry_assessment),
        )
        .route(
            "/api/migrations/fub/assessments/{id}/cancel",
            post(cancel_assessment),
        )
}
struct UuidPath(Uuid);
impl FromRequestParts<AppState> for UuidPath {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(Self(id))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialRequest {
    request_id: Uuid,
    api_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceCredentialRequest {
    request_id: Uuid,
    api_key: String,
    expected_revision: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    request_id: Uuid,
    connection_id: Uuid,
    expected_revision: i32,
}

fn no_store<T: serde::Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
async fn summary(
    State(state): State<AppState>,
    admin: OrgAdminContext,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(no_store(
        commands::fub_summary(
            pool,
            &state.raw_payload_key,
            &CommandContext::from_auth(&admin.auth),
        )
        .await?,
    ))
}
async fn create_connection(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    body: Result<Json<CredentialRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let connection = commands::connect_fub(
        pool,
        &state.raw_payload_key,
        state.migration_reader.as_ref(),
        &CommandContext::from_auth(&admin.auth),
        commands::ConnectFub {
            request_id: req.request_id,
            api_key: req.api_key,
        },
    )
    .await?
    .value;
    Ok((
        StatusCode::CREATED,
        no_store(json!({"connection":connection,"request_id":req.request_id})),
    )
        .into_response())
}
async fn replace_credential(
    State(state): State<AppState>,
    UuidPath(id): UuidPath,
    admin: OrgAdminContext,
    body: Result<Json<ReplaceCredentialRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let connection = commands::replace_fub_credential(
        pool,
        &state.raw_payload_key,
        state.migration_reader.as_ref(),
        &CommandContext::from_auth(&admin.auth),
        commands::ReplaceFubCredential {
            request_id: req.request_id,
            connection_id: id,
            expected_revision: req.expected_revision,
            api_key: req.api_key,
        },
    )
    .await?
    .value;
    Ok(no_store(
        json!({"connection":connection,"request_id":req.request_id}),
    ))
}
async fn disconnect(
    State(state): State<AppState>,
    UuidPath(id): UuidPath,
    admin: OrgAdminContext,
) -> Result<StatusCode, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    commands::disconnect_fub(
        pool,
        &CommandContext::from_auth(&admin.auth),
        commands::DisconnectFub { connection_id: id },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn start_assessment(
    State(state): State<AppState>,
    admin: OrgAdminContext,
    body: Result<Json<StartRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let assessment = commands::start_fub_assessment(
        pool,
        &state.raw_payload_key,
        &CommandContext::from_auth(&admin.auth),
        commands::StartFubAssessment {
            request_id: req.request_id,
            connection_id: req.connection_id,
            expected_revision: req.expected_revision,
        },
    )
    .await?
    .value;
    Ok((
        StatusCode::ACCEPTED,
        no_store(json!({"assessment":assessment,"request_id":req.request_id})),
    )
        .into_response())
}
async fn get_assessment(
    State(state): State<AppState>,
    UuidPath(id): UuidPath,
    admin: OrgAdminContext,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(no_store(
        json!({"assessment":commands::get_fub_assessment(pool,&state.raw_payload_key,&CommandContext::from_auth(&admin.auth),id).await?}),
    ))
}
async fn retry_assessment(
    State(state): State<AppState>,
    UuidPath(id): UuidPath,
    admin: OrgAdminContext,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok((StatusCode::ACCEPTED,no_store(json!({"assessment":commands::retry_fub_assessment(pool,&state.raw_payload_key,&CommandContext::from_auth(&admin.auth),commands::RetryFubAssessment{assessment_id:id}).await?}))).into_response())
}
async fn cancel_assessment(
    State(state): State<AppState>,
    UuidPath(id): UuidPath,
    admin: OrgAdminContext,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    Ok(no_store(
        json!({"assessment":commands::cancel_fub_assessment(pool,&state.raw_payload_key,&CommandContext::from_auth(&admin.auth),commands::CancelFubAssessment{assessment_id:id}).await?}),
    ))
}

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::commands::{
    self, AssignPerson, ChangePersonStage, ContactChannel, ContactOutcome, LogContactAttempt,
};
use crate::domain::envelope::CommandContext;
use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::person::queries as person_queries;
use crate::domain::person::PersonVisibilityScope;
use crate::error::ApiError;
use crate::ids::UserId;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people", get(list_people))
        .route("/api/people/{id}", get(get_person))
        .route("/api/people/{id}/assignment", post(set_assignment))
        .route("/api/people/{id}/stage", post(set_stage))
        .route("/api/people/{id}/contact-attempts", post(log_contact))
}

/// A `{id}` path segment parsed as a UUID, rejecting straight to `400
/// malformed_request` (docs/specs/SLICE_002.md §5) rather than axum's
/// default `PathRejection` body. Used as a *bare* (non-`Result`-wrapped)
/// extractor and listed before `AuthContext` in every handler below: axum
/// evaluates extractors in declaration order and stops at the first one
/// that returns `Err`, so — unlike `Result<Path<Uuid>, _>`, whose own
/// extraction always "succeeds" and only defers the error into the
/// handler body — this genuinely short-circuits ahead of authentication,
/// making a non-UUID id a 400 independent of auth state and, since it
/// never touches `state.db`, testable service-free.
struct PersonId(Uuid);

impl FromRequestParts<AppState> for PersonId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonId(id))
    }
}

async fn list_people(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let scope = PersonVisibilityScope::from_auth(&auth);

    let (people, truncated) = person_queries::list_summaries(&mut conn, &scope)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "people": people, "truncated": truncated })))
}

async fn get_person(
    State(state): State<AppState>,
    PersonId(person_id): PersonId,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let scope = PersonVisibilityScope::from_auth(&auth);
    let organization_id = scope.organization_id();

    let person = person_queries::summary_by_id(&mut conn, organization_id, person_id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::NotFound)?;

    let contact_methods =
        person_queries::contact_methods_for_person(&mut conn, organization_id, person_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;

    let inquiries = inquiry_queries::list_for_person(&mut conn, organization_id, person_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    let history = person_queries::history_for_person(&mut conn, organization_id, person_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({
        "person": person,
        "contact_methods": contact_methods,
        "inquiries": inquiries,
        "history": history,
    })))
}

#[derive(Deserialize)]
struct AssignmentRequest {
    // `None` (both a JSON `null` and an omitted key) means unassign.
    #[serde(default)]
    assigned_user_id: Option<UserId>,
}

async fn set_assignment(
    State(state): State<AppState>,
    PersonId(person_id): PersonId,
    auth: AuthContext,
    body: Result<Json<AssignmentRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&auth);

    let (summary, changed) = commands::assign_person(
        pool,
        &state.publisher,
        &ctx,
        AssignPerson {
            person_id,
            assigned_user_id: req.assigned_user_id,
        },
    )
    .await?;

    Ok(Json(json!({ "person": summary, "changed": changed })))
}

#[derive(Deserialize)]
struct StageRequest {
    stage_id: Uuid,
}

async fn set_stage(
    State(state): State<AppState>,
    PersonId(person_id): PersonId,
    auth: AuthContext,
    body: Result<Json<StageRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&auth);

    let (summary, changed) = commands::change_person_stage(
        pool,
        &state.publisher,
        &ctx,
        ChangePersonStage {
            person_id,
            stage_id: req.stage_id,
        },
    )
    .await?;

    Ok(Json(json!({ "person": summary, "changed": changed })))
}

#[derive(Deserialize)]
struct ContactAttemptRequest {
    channel: ContactChannel,
    outcome: ContactOutcome,
}

/// `POST /api/people/{id}/contact-attempts` (docs/specs/SLICE_003.md §5).
/// An invalid `channel`/`outcome` value or non-JSON body is a serde
/// rejection, not a new `ApiError` variant — it maps to the existing 400
/// `malformed_request` exactly like every other bad body in this file.
async fn log_contact(
    State(state): State<AppState>,
    PersonId(person_id): PersonId,
    auth: AuthContext,
    body: Result<Json<ContactAttemptRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&auth);

    let (summary, contact_attempt) = commands::log_contact_attempt(
        pool,
        &state.publisher,
        &ctx,
        LogContactAttempt {
            person_id,
            channel: req.channel,
            outcome: req.outcome,
        },
    )
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({ "person": summary, "contact_attempt": contact_attempt })),
    ))
}

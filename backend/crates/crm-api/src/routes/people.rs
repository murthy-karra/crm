use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequestParts, Path, Query, State};
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
use crate::domain::person::filter::FilterDefinition;
use crate::domain::person::queries as person_queries;
use crate::domain::person::PersonVisibilityScope;
use crate::error::ApiError;
use crate::ids::{PersonId, StageId, UserId};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people", get(list_people))
        .route("/api/people/{id}", get(get_person))
        .route("/api/people/{id}/assignment", post(set_assignment))
        .route("/api/people/{id}/stage", post(set_stage))
        .route("/api/people/{id}/contact-attempts", post(log_contact))
}

/// A `{id}` path segment parsed as a UUID and typed as `PersonId`
/// (hardening chunk N3), rejecting straight to `400 malformed_request`
/// (docs/specs/SLICE_002.md §5) rather than axum's default `PathRejection`
/// body. Used as a *bare* (non-`Result`-wrapped) extractor and listed
/// before `AuthContext` in every handler below: axum evaluates extractors
/// in declaration order and stops at the first one that returns `Err`, so
/// — unlike `Result<Path<Uuid>, _>`, whose own extraction always
/// "succeeds" and only defers the error into the handler body — this
/// genuinely short-circuits ahead of authentication, making a non-UUID id
/// a 400 independent of auth state and, since it never touches
/// `state.db`, testable service-free. Named `PersonIdPath` (not
/// `PersonId`) to avoid shadowing `crm_app::ids::PersonId`, which the
/// domain layer now uses for the same value; the two can't be the same
/// type here because implementing `FromRequestParts` (axum's trait) for a
/// foreign type (crm-app's `PersonId`) would violate the orphan rule.
struct PersonIdPath(PersonId);

impl FromRequestParts<AppState> for PersonIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonIdPath(PersonId::new(id)))
    }
}

/// `?filter=` (docs/specs/SLICE_011a.md §5a): the query struct has no
/// `deny_unknown_fields` — unrelated query params (`?foo=bar`) are ignored
/// exactly as today, per the spec's stated divergence from the filter
/// JSON's own strict decode discipline. `filter: Option<String>` distinguishes
/// a truly ABSENT param (`None`, legacy path) from a PRESENT-but-empty one
/// (`Some("")`, a 400 — empty string is not JSON).
#[derive(Deserialize)]
struct ListPeopleQuery {
    #[serde(default)]
    filter: Option<String>,
}

/// `auth: AuthContext` is listed BEFORE the query extractor so a garbage or
/// unauthenticated-session request gets 401 before any filter parsing runs
/// (docs/specs/SLICE_011a.md §5a: "401 first" — the substantial-parse
/// counterpart to `PersonIdPath`'s pre-auth 400 above, a deliberate,
/// stated divergence from that precedent). `Result<Query<_>, _>` mirrors
/// the `body: Result<Json<_>, JsonRejection>` pattern used by the mutation
/// handlers below: extraction itself never short-circuits, so this always
/// runs after `auth`.
async fn list_people(
    State(state): State<AppState>,
    auth: AuthContext,
    query: Result<Query<ListPeopleQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let scope = PersonVisibilityScope::from_auth(&auth);

    let (people, truncated) = match query.filter {
        // Absent `filter` -> the untouched legacy path: same query, same
        // `.sqlx` entry, same shape, same order, same cap math (§5a).
        None => person_queries::list_summaries(&mut conn, &scope)
            .await
            .map_err(|_| ApiError::Unavailable)?,
        Some(raw) => {
            // Present-but-empty `?filter=` (or `?filter`) is a 400: empty
            // string is not JSON, and only a truly absent param is the
            // legacy path (§5a).
            if raw.is_empty() {
                return Err(ApiError::MalformedRequest);
            }
            // axum's `Query` extractor already percent-decodes both keys
            // and values (`serde_urlencoded`), so `raw` is the plain JSON
            // text at this point — no separate percent-decode step needed.
            let filter: FilterDefinition =
                serde_json::from_str(&raw).map_err(|_| ApiError::MalformedRequest)?;
            // Error order (§5a): 401 (already past, via `auth` above) ->
            // 400 structural -> 422 org-scoped -> 200/503.
            filter.validate()?;
            let organization_id = scope.organization_id();
            filter
                .validate_references(&mut conn, organization_id)
                .await?;

            // Observability (§7): `filter_kinds` (static vocabulary,
            // comma-joined) + `filter_clause_count` on the request span.
            // NO clause values, ids, sources, or day counts.
            let span = tracing::Span::current();
            span.record("filter_kinds", filter.kinds_field());
            span.record("filter_clause_count", filter.clauses.len());

            // `me` resolves server-side to the caller AFTER validation,
            // appended to the bound user array — never a wire value
            // reaching SQL as a token (§4c).
            let params = filter.to_query_params(auth.actor_user_id);
            person_queries::filtered_summaries(&mut conn, &scope, &params)
                .await
                .map_err(|_| ApiError::Unavailable)?
        }
    };

    Ok(Json(json!({ "people": people, "truncated": truncated })))
}

async fn get_person(
    State(state): State<AppState>,
    PersonIdPath(person_id): PersonIdPath,
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
    PersonIdPath(person_id): PersonIdPath,
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
    stage_id: StageId,
}

async fn set_stage(
    State(state): State<AppState>,
    PersonIdPath(person_id): PersonIdPath,
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
    PersonIdPath(person_id): PersonIdPath,
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

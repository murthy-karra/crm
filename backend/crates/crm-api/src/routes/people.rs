use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::admin::Role;
use crate::domain::commands::{
    self, AssignPerson, ChangePersonStage, ContactChannel, ContactOutcome, LogContactAttempt,
};
use crate::domain::envelope::CommandContext;
use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::person::filter::{FilterDefinition, PersonFilterParams};
use crate::domain::person::queries as person_queries;
use crate::domain::person::sort::PersonSort;
use crate::domain::person::PersonVisibilityScope;
use crate::domain::tag::{self, AddPersonTag, RemovePersonTag};
use crate::domain::task;
use crate::error::ApiError;
use crate::ids::{PersonId, StageId, TagId, UserId};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people", get(list_people))
        .route("/api/people/{id}", get(get_person))
        .route("/api/people/{id}/assignment", post(set_assignment))
        .route("/api/people/{id}/stage", post(set_stage))
        .route("/api/people/{id}/contact-attempts", post(log_contact))
        .route("/api/people/{id}/tags/{tag_id}", put(add_person_tag))
        .route("/api/people/{id}/tags/{tag_id}", delete(remove_person_tag))
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

/// The `{id}/tags/{tag_id}` pair (docs/specs/SLICE_011e.md §5), same
/// pre-authentication 400 precedent as `PersonIdPath` above: either id
/// being a non-UUID is a 400 independent of auth state.
struct PersonTagIdsPath(PersonId, TagId);

impl FromRequestParts<AppState> for PersonTagIdsPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path((person_id, tag_id)) = Path::<(Uuid, Uuid)>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonTagIdsPath(
            PersonId::new(person_id),
            TagId::new(tag_id),
        ))
    }
}

/// `?filter=` (docs/specs/SLICE_011a.md §5a) and `?sort=`
/// (docs/specs/SLICE_011b_SORT.md §6): the query struct has no
/// `deny_unknown_fields` — unrelated query params (`?foo=bar`) are ignored
/// exactly as today, per the spec's stated divergence from the filter
/// JSON's own strict decode discipline. `filter: Option<String>` distinguishes
/// a truly ABSENT param (`None`, legacy path) from a PRESENT-but-empty one
/// (`Some("")`, a 400 — empty string is not JSON). `sort: Option<PersonSort>`
/// decodes through `PersonSort`'s own `TryFrom<String>`-backed `Deserialize`
/// directly in this typed extractor, so a malformed, empty, or repeated
/// `sort` fails EXTRACTION itself — a 400 before any pool acquisition (spec
/// §6) — exactly like axum's existing scalar-field handling already gives
/// `filter` for a repeated param.
#[derive(Deserialize)]
struct ListPeopleQuery {
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    sort: Option<PersonSort>,
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

    // Observability (docs/specs/SLICE_011b_SORT.md §6): the static sort
    // token, beside `filter_kinds` — NEVER a clause value. Recorded for any
    // successfully-decoded `sort`, including an explicit `created.desc`
    // (which still normalizes to the legacy query path below).
    if let Some(sort) = query.sort {
        tracing::Span::current().record("sort", sort.token());
    }
    // `None` here means "no sort" OR "sort=created.desc" — both take the
    // byte-identical legacy path (spec §4, §6).
    let sort = query.sort.and_then(PersonSort::normalized);

    let (people, truncated) = match (query.filter, sort) {
        // Absent `filter`, no (or default) `sort` -> the untouched legacy
        // path: same query, same `.sqlx` entry, same shape, same order,
        // same cap math (§5a).
        (None, None) => person_queries::list_summaries(&mut conn, &scope)
            .await
            .map_err(|_| ApiError::Unavailable)?,
        // A sort with no ad-hoc filter still runs a sorted statement, with
        // all-NULL clause parameters — pinned equal to the full list (§4).
        (None, Some(sort)) => {
            let params = PersonFilterParams::default();
            person_queries::filtered_summaries_sorted(&mut conn, &scope, &params, sort)
                .await
                .map_err(|_| ApiError::Unavailable)?
        }
        (Some(raw), sort) => {
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
            match sort {
                None => person_queries::filtered_summaries(&mut conn, &scope, &params)
                    .await
                    .map_err(|_| ApiError::Unavailable)?,
                Some(sort) => {
                    person_queries::filtered_summaries_sorted(&mut conn, &scope, &params, sort)
                        .await
                        .map_err(|_| ApiError::Unavailable)?
                }
            }
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

    let mut history = person_queries::history_for_person(&mut conn, organization_id, person_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    // The `note` and `task_completed` history kinds' `can_manage`
    // (docs/specs/SLICE_015.md §5, docs/specs/SLICE_016.md §4, the
    // tags-route pattern): the domain query always emits `false` (it has
    // no viewer context); this route knows the viewer's role and id, so
    // it overwrites the field — admin, or the viewer is the row's own
    // author (note) / assignee or creator (task_completed). An unmatched
    // imported actor (`actor: null` for a note; `assignee`/`created_by`
    // both `null` for a task) is manageable by an admin only.
    for entry in history.iter_mut() {
        if entry.kind == "note" {
            let can_manage = match &entry.actor {
                Some(actor) => auth.role == Role::Admin || actor.id == auth.actor_user_id,
                None => auth.role == Role::Admin,
            };
            entry.detail["can_manage"] = serde_json::json!(can_manage);
        } else if entry.kind == "task_completed" {
            let assignee_id = entry.detail["assignee"]["id"].as_str();
            let created_by_id = entry.detail["created_by"]["id"].as_str();
            let viewer_id = auth.actor_user_id.to_string();
            let can_manage = auth.role == Role::Admin
                || assignee_id == Some(viewer_id.as_str())
                || created_by_id == Some(viewer_id.as_str());
            entry.detail["can_manage"] = serde_json::json!(can_manage);
        }
    }

    let tags = tag::list_for_person(&mut conn, organization_id, person_id).await?;

    let tasks = task::open_for_person(&mut conn, organization_id, person_id).await?;
    // The Person detail's `tasks[]` `can_manage` (docs/specs/SLICE_016.md
    // §4): same viewer-relative overwrite as the history kind above — the
    // domain read has no viewer context and always returns `false`.
    let tasks: Vec<serde_json::Value> = tasks
        .into_iter()
        .map(|t| {
            let can_manage = auth.role == Role::Admin
                || t.assignee
                    .as_ref()
                    .is_some_and(|a| a.id == auth.actor_user_id)
                || t.created_by
                    .as_ref()
                    .is_some_and(|c| c.id == auth.actor_user_id);
            let mut value = serde_json::to_value(&t).unwrap_or(serde_json::Value::Null);
            value["can_manage"] = serde_json::json!(can_manage);
            value
        })
        .collect();

    Ok(Json(json!({
        "person": person,
        "contact_methods": contact_methods,
        "inquiries": inquiries,
        "history": history,
        "tags": tags,
        "tasks": tasks,
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

/// `PUT /api/people/{person_id}/tags/{tag_id}` (docs/specs/SLICE_011e.md
/// §5): target-state idempotent apply — 200 whether or not the tag was
/// already applied, `changed` distinguishes the two.
async fn add_person_tag(
    State(state): State<AppState>,
    PersonTagIdsPath(person_id, tag_id): PersonTagIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&auth);

    let outcome = tag::add_person_tag(
        pool,
        &state.publisher,
        &ctx,
        AddPersonTag { person_id, tag_id },
    )
    .await?;

    Ok(Json(
        json!({ "tags": outcome.tags, "changed": outcome.changed }),
    ))
}

/// `DELETE /api/people/{person_id}/tags/{tag_id}` (docs/specs/SLICE_011e.md
/// §5): target-state idempotent remove.
async fn remove_person_tag(
    State(state): State<AppState>,
    PersonTagIdsPath(person_id, tag_id): PersonTagIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&auth);

    let outcome = tag::remove_person_tag(
        pool,
        &state.publisher,
        &ctx,
        RemovePersonTag { person_id, tag_id },
    )
    .await?;

    Ok(Json(
        json!({ "tags": outcome.tags, "changed": outcome.changed }),
    ))
}

//! Task HTTP surface (Slice 016a). The command/read module owns rule-1
//! authorization (docs/specs/SLICE_016.md §1 rule 1, §3), title
//! validation, and the tombstone semantics; this adapter owns only
//! trusted extractor order, strict wire decoding, and the stable JSON
//! envelopes — the `notes.rs`/`tags.rs` adapter's shape. No task title is
//! ever logged, spanned, or put in an error envelope from this file
//! (AGENTS.md §9, docs/specs/SLICE_016.md §1 rule 7): every handler below
//! passes the title straight through to the command and never formats or
//! records it itself.

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::envelope::CommandContext;
use crate::domain::task::{
    self, CompleteTask, CreateTask, DeleteTask, ReopenTask, SnoozeTask, TaskKind, UpdateTask,
};
use crate::error::ApiError;
use crate::ids::{PersonId, TaskId, UserId};
use crate::state::AppState;

const MAX_TASK_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", get(list_my_tasks))
        .route(
            "/api/people/{person_id}/tasks",
            post(create_task).layer(DefaultBodyLimit::max(MAX_TASK_BODY_BYTES)),
        )
        .route(
            "/api/people/{person_id}/tasks/{task_id}",
            put(update_task).layer(DefaultBodyLimit::max(MAX_TASK_BODY_BYTES)),
        )
        .route(
            "/api/people/{person_id}/tasks/{task_id}",
            delete(delete_task),
        )
        .route(
            "/api/people/{person_id}/tasks/{task_id}/complete",
            post(complete_task),
        )
        .route(
            "/api/people/{person_id}/tasks/{task_id}/reopen",
            post(reopen_task),
        )
        .route(
            "/api/people/{person_id}/tasks/{task_id}/snooze",
            post(snooze_task).layer(DefaultBodyLimit::max(MAX_TASK_BODY_BYTES)),
        )
}

/// A `{person_id}` path segment, matching the established Person-route
/// precedent (docs/specs/SLICE_002.md §5 error precedence): a malformed id
/// is 400 independent of auth state, tested service-free.
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

/// The `{person_id}/tasks/{task_id}` pair (docs/specs/SLICE_016.md §4),
/// the `PersonNoteIdsPath`/`PersonTagIdsPath` pattern: either id being a
/// non-UUID is a 400 independent of auth state.
struct PersonTaskIdsPath(PersonId, TaskId);

impl FromRequestParts<AppState> for PersonTaskIdsPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path((person_id, task_id)) = Path::<(Uuid, Uuid)>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonTaskIdsPath(
            PersonId::new(person_id),
            TaskId::new(task_id),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTaskRequest {
    title: String,
    #[serde(default)]
    kind: TaskKind,
    #[serde(default)]
    due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    assignee_user_id: Option<UserId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateTaskRequest {
    title: String,
    kind: TaskKind,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: UserId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnoozeTaskRequest {
    due_at: DateTime<Utc>,
}

/// `GET /api/tasks?scope=mine` (docs/specs/SLICE_016.md §4, 016b): the
/// only accepted query shape is exactly `scope=mine` — no default, no
/// other value, no extra key. `deny_unknown_fields` rejects an extra key;
/// `scope: String` (not `Option`) makes a MISSING `scope` a deserialize
/// failure (`QueryRejection` — no query string, or a query string that
/// never sets this key, both fail to populate a required field); an
/// EMPTY or UNKNOWN value still deserializes (any string is valid UTF-8
/// query text) but is rejected explicitly below. Every one of "missing,
/// empty, unknown, extra" therefore fails closed to 400.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListTasksQuery {
    scope: String,
}

/// `POST /api/people/{person_id}/tasks` (docs/specs/SLICE_016.md §4): any
/// active member.
async fn create_task(
    State(state): State<AppState>,
    PersonIdPath(person_id): PersonIdPath,
    auth: AuthContext,
    body: Result<Json<CreateTaskRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let task = task::create_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        CreateTask {
            person_id,
            title: req.title,
            kind: req.kind,
            due_at: req.due_at,
            assignee_user_id: req.assignee_user_id,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "task": task }))))
}

/// `PUT /api/people/{person_id}/tasks/{task_id}` (docs/specs/SLICE_016.md
/// §4): member route; rule 1 is decided inside the command under the task
/// row's lock, not at the route.
async fn update_task(
    State(state): State<AppState>,
    PersonTaskIdsPath(person_id, task_id): PersonTaskIdsPath,
    auth: AuthContext,
    body: Result<Json<UpdateTaskRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = task::update_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        UpdateTask {
            person_id,
            task_id,
            title: req.title,
            kind: req.kind,
            due_at: req.due_at,
            assignee_user_id: req.assignee_user_id,
        },
    )
    .await?;

    Ok(Json(
        json!({ "task": outcome.task, "changed": outcome.changed }),
    ))
}

/// `POST …/tasks/{task_id}/complete` (docs/specs/SLICE_016.md §4).
async fn complete_task(
    State(state): State<AppState>,
    PersonTaskIdsPath(person_id, task_id): PersonTaskIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = task::complete_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        CompleteTask { person_id, task_id },
    )
    .await?;

    Ok(Json(
        json!({ "task": outcome.task, "changed": outcome.changed }),
    ))
}

/// `POST …/tasks/{task_id}/reopen` (docs/specs/SLICE_016.md §4).
async fn reopen_task(
    State(state): State<AppState>,
    PersonTaskIdsPath(person_id, task_id): PersonTaskIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = task::reopen_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        ReopenTask { person_id, task_id },
    )
    .await?;

    Ok(Json(
        json!({ "task": outcome.task, "changed": outcome.changed }),
    ))
}

/// `POST …/tasks/{task_id}/snooze` (docs/specs/SLICE_016.md §4): its own
/// route (never a full `UpdateTask`) so the Today panel (016b) can always
/// snooze from a possibly-stale row.
async fn snooze_task(
    State(state): State<AppState>,
    PersonTaskIdsPath(person_id, task_id): PersonTaskIdsPath,
    auth: AuthContext,
    body: Result<Json<SnoozeTaskRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = task::snooze_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        SnoozeTask {
            person_id,
            task_id,
            due_at: req.due_at,
        },
    )
    .await?;

    Ok(Json(
        json!({ "task": outcome.task, "changed": outcome.changed }),
    ))
}

/// `DELETE …/tasks/{task_id}` (docs/specs/SLICE_016.md §4): member route;
/// rule 1 is decided inside the command. A repeat delete, or delete of a
/// tombstone, is 404 — identical to a nonexistent id.
async fn delete_task(
    State(state): State<AppState>,
    PersonTaskIdsPath(person_id, task_id): PersonTaskIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = task::delete_task(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        DeleteTask { person_id, task_id },
    )
    .await?;

    Ok(Json(json!({ "deleted": outcome.deleted })))
}

/// `GET /api/tasks?scope=mine` (docs/specs/SLICE_016.md §4, 016b): the
/// viewer's open, dated tasks with `due_at <= generated_at + 24h`,
/// ordered `due_at, id`; fetch 201, return 200 with `truncated` set
/// exactly by the 201st row. The viewer is always
/// `auth.actor_user_id`/`auth.active_organization_id` — never client
/// input (the `GET /api/today` precedent). `auth` runs before the query
/// extractor, so a platform-only session is 401 before a query-shape 400
/// (the `list_people` precedent).
async fn list_my_tasks(
    State(state): State<AppState>,
    auth: AuthContext,
    query: Result<Query<ListTasksQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::MalformedRequest)?;
    if query.scope != "mine" {
        return Err(ApiError::MalformedRequest);
    }

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let generated_at = Utc::now();
    let mut tasks = task::open_for_assignee(
        &mut conn,
        auth.active_organization_id,
        auth.actor_user_id,
        generated_at,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    let truncated = tasks.len() > 200;
    tasks.truncate(200);

    Ok(Json(json!({
        "tasks": tasks,
        "generated_at": generated_at,
        "truncated": truncated,
    })))
}

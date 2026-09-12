//! Frozen cd3b010 operational Person-detail comparison, test-support only.
//! The body between markers is byte-exact; see the retained source manifest.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static ENTRY_HITS: AtomicUsize = AtomicUsize::new(0);

mod person_queries {
    pub use crate::domain::person::queries::perf_cd3b010_history::run as history_for_person;
    pub use crate::domain::person::queries::{contact_methods_for_person, summary_by_id};
}

mod task {
    pub use crate::domain::task::perf_cd3b010_open::run as open_for_person;
}

// BEGIN FROZEN CD3B010
async fn get_person(
    State(state): State<AppState>,
    PersonIdPath(person_id): PersonIdPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(ApiError::database)?;
    let scope = PersonVisibilityScope::from_auth(&auth);
    let organization_id = scope.organization_id();

    let person = person_queries::summary_by_id(&mut conn, organization_id, person_id)
        .await
        .map_err(ApiError::database)?
        .ok_or(ApiError::NotFound)?;

    let contact_methods =
        person_queries::contact_methods_for_person(&mut conn, organization_id, person_id)
            .await
            .map_err(ApiError::database)?;

    let inquiries = inquiry_queries::list_for_person(&mut conn, organization_id, person_id)
        .await
        .map_err(ApiError::database)?;

    let mut history = person_queries::history_for_person(&mut conn, organization_id, person_id)
        .await
        .map_err(ApiError::database)?;
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

    // `custom_fields` (docs/specs/SLICE_019.md §4): set values on LIVE
    // fields only, in field position order — assembled here beside
    // `tags`/`tasks`, no change to `crm-app/src/domain/person/` (spec's
    // stated ownership boundary).
    let custom_fields =
        custom_field::values_for_person(&mut conn, organization_id, person_id).await?;

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
        "custom_fields": custom_fields,
    })))
}
// END FROZEN CD3B010

async fn counted_get_person(
    state: State<AppState>,
    person: PersonIdPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    ENTRY_HITS.fetch_add(1, Ordering::SeqCst);
    get_person(state, person, auth).await
}

/// Explicit builder-only substitution; never selected by request data.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/people/{id}", get(counted_get_person))
}

pub fn entry_hits() -> [usize; 3] {
    [
        ENTRY_HITS.load(Ordering::SeqCst),
        crate::domain::person::queries::perf_cd3b010_history::entry_hits(),
        crate::domain::task::perf_cd3b010_open::entry_hits(),
    ]
}

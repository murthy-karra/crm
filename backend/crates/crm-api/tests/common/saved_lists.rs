//! Shared Slice 011b saved-list fixture helpers (item 3 of the LATER
//! batch, docs/tasks/LATER_BATCH_2026-09-08.md). Used by `db_saved_lists.rs`
//! and `db_saved_lists_sort.rs` — moved here because both need them. No
//! test bodies changed; this is exactly the code that used to live at the
//! top of `db_saved_lists.rs`. Run only via ./scripts/check-db.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::auth::AuthContext;
use crm_api::domain::admin::Role;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::person::sort::PersonSort;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};

pub const PW: &str = "correct horse battery staple";

pub fn empty_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: Vec::new(),
    }
}

pub fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

pub fn auth_context(organization_id: Uuid, actor_user_id: Uuid, role: Role) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(actor_user_id),
        actor_email: "fixture@example.test".to_string(),
        actor_display_name: "Fixture".to_string(),
        active_organization_id: OrganizationId::new(organization_id),
        active_organization_name: "Fixture Organization".to_string(),
        role,
    }
}

pub async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    request_id: Uuid,
    scope: SavedListScope,
    name: &str,
    filter: FilterDefinition,
) -> saved_list::CreateSavedListOutcome {
    create_list_with_sort(
        app_pool,
        organization_id,
        actor_user_id,
        request_id,
        scope,
        name,
        filter,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_list_with_sort(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    request_id: Uuid,
    scope: SavedListScope,
    name: &str,
    filter: FilterDefinition,
    sort: Option<PersonSort>,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id,
            scope,
            name: name.to_string(),
            filter,
            sort,
        },
    )
    .await
    .unwrap()
}

pub async fn delete_json(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    body: Value,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// The privacy contract intentionally makes hidden, foreign, and missing
/// resources indistinguishable. Keep this byte-level rather than only a
/// JSON-value assertion: callers must not be able to distinguish those paths
/// from an envelope formatting difference either.
pub async fn assert_status_body(
    response: axum::response::Response,
    expected_status: StatusCode,
    expected_body: &[u8],
    label: &str,
) {
    assert_eq!(response.status(), expected_status, "{label}");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), expected_body, "{label}");
}

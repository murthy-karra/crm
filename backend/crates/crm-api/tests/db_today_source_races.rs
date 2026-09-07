//! Synchronized command coverage for Today work sources (Slice 011c AC2/3).
//!
//! These tests deliberately exercise the same typed commands used by the
//! routes. Direct SQL is only used to observe the final persisted state after
//! a serialized mutation race.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use crm_api::auth::AuthContext;
use crm_api::domain::admin::Role;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListError, SavedListScope, UpdateSavedList,
};
use crm_api::domain::today::{
    self, DisableTodayWorkSource, EnableTodayWorkSource, TodaySource, TodaySourceChange,
};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, UserId};
use crm_api::realtime::Publisher;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Barrier;
use tower::ServiceExt;
use uuid::Uuid;

const PW: &str = "correct horse battery staple";

fn empty_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: Vec::new(),
    }
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn auth_context(organization_id: Uuid, actor_user_id: Uuid) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(actor_user_id),
        actor_email: "fixture@example.test".to_string(),
        actor_display_name: "Fixture".to_string(),
        active_organization_id: OrganizationId::new(organization_id),
        active_organization_name: "Fixture Organization".to_string(),
        role: Role::Member,
    }
}

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    name: &str,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: name.to_string(),
            filter: empty_filter(),
        },
    )
    .await
    .unwrap()
}

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: SavedListId,
    expected_list_revision: i64,
) -> Result<TodaySourceChange, SavedListError> {
    today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
}

async fn disable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: SavedListId,
) -> Result<TodaySourceChange, SavedListError> {
    today::disable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        DisableTodayWorkSource { list_id },
    )
    .await
}

async fn list_sources(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
) -> Vec<TodaySource> {
    let mut conn = app_pool.acquire().await.unwrap();
    today::list_today_work_sources(&mut conn, &auth_context(organization_id, actor_user_id))
        .await
        .unwrap()
}

fn assert_change(change: TodaySourceChange, enabled: bool, changed: bool) {
    assert_eq!(change.enabled, enabled);
    assert_eq!(change.changed, changed);
}

async fn delete_json_with_cookie(
    router: &Router,
    uri: &str,
    cookie: &str,
    body: Value,
) -> Response {
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

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected recording publisher");
    };
    recorded.lock().await.clone()
}

/// The revision check precedes the idempotent and capacity branches. A retry
/// with the current revision is a no-op even at five sources; an old revision
/// conflicts even though the target is already enabled.
#[sqlx::test]
#[ignore]
async fn today_source_enable_checks_revision_before_idempotence_and_cap(migrator_pool: PgPool) {
    let (organization_id, owner_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source duplicate at cap",
        "owner@today-source-duplicate-cap.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let target = create_list(&app_pool, organization_id, owner_id, "Target source").await;
    assert_change(
        enable(
            &app_pool,
            organization_id,
            owner_id,
            target.list.id,
            target.list.revision,
        )
        .await
        .unwrap(),
        true,
        true,
    );
    for n in 1..=4 {
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            &format!("Other source {n}"),
        )
        .await;
        assert_change(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap(),
            true,
            true,
        );
    }
    assert_eq!(
        list_sources(&app_pool, organization_id, owner_id)
            .await
            .len(),
        5
    );
    assert_change(
        enable(
            &app_pool,
            organization_id,
            owner_id,
            target.list.id,
            target.list.revision,
        )
        .await
        .unwrap(),
        true,
        false,
    );

    let updated = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: target.list.id,
            expected_revision: target.list.revision,
            name: "Target source renamed".to_string(),
            filter: empty_filter(),
        },
    )
    .await
    .unwrap();
    assert!(updated.changed);
    assert_eq!(updated.list.revision, target.list.revision + 1);

    assert!(matches!(
        enable(
            &app_pool,
            organization_id,
            owner_id,
            target.list.id,
            target.list.revision,
        )
        .await,
        Err(SavedListError::Conflict)
    ));
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "owner@today-source-duplicate-cap.test", PW).await;
    let stale = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/today/sources/{}", target.list.id),
        &cookie,
        json!({ "expected_list_revision": target.list.revision }),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(stale).await,
        json!({ "error": "saved_list_conflict" })
    );
    assert_eq!(
        list_sources(&app_pool, organization_id, owner_id)
            .await
            .len(),
        5,
        "neither rejected retry changes the configured target state"
    );
}

/// Opposing target-state commands are released together but serialize through
/// the saved-list Organization lock. Their `changed` receipts identify the
/// legal serial order and must agree with the persisted final source state.
#[sqlx::test]
#[ignore]
async fn today_source_opposing_enable_disable_race_has_a_serialized_final_state(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source opposing commands",
        "owner@today-source-opposing.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let list = create_list(&app_pool, organization_id, owner_id, "Race target").await;
    let barrier = Arc::new(Barrier::new(3));

    let enable_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            enable(
                &pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
        })
    };
    let disable_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            disable(&pool, organization_id, owner_id, list.list.id).await
        })
    };
    barrier.wait().await;
    let enabled = enable_task.await.unwrap().unwrap();
    let disabled = disable_task.await.unwrap().unwrap();

    assert_change(enabled, true, true);
    assert!(!disabled.enabled);
    let configured = list_sources(&app_pool, organization_id, owner_id).await;
    assert_eq!(
        configured
            .iter()
            .any(|source| source.list_id == list.list.id),
        !disabled.changed,
        "a changed disable ran after enable; an unchanged disable ran before it"
    );
}

/// An enable and deletion at the same revision can complete in either lock
/// order, but deletion always wins the final durable state: no raw preference
/// and no list discoverable to the actor remain afterwards.
#[sqlx::test]
#[ignore]
async fn today_source_enable_delete_race_leaves_no_preference_or_discoverable_configuration(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source enable delete race",
        "owner@today-source-enable-delete.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let list = create_list(&app_pool, organization_id, owner_id, "Delete race target").await;
    let barrier = Arc::new(Barrier::new(3));

    let enable_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            enable(
                &pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
        })
    };
    let delete_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            saved_list::delete_saved_list(
                &pool,
                &command_context(organization_id, owner_id),
                DeleteSavedList {
                    list_id: list.list.id,
                    expected_revision: list.list.revision,
                },
            )
            .await
        })
    };
    barrier.wait().await;
    let enable_result = enable_task.await.unwrap();
    let delete_result = delete_task.await.unwrap();

    assert!(matches!(delete_result, Ok(outcome) if outcome.deleted));
    assert!(matches!(
        enable_result,
        Ok(TodaySourceChange {
            enabled: true,
            changed: true,
        }) | Err(SavedListError::NotFound)
    ));
    assert!(
        list_sources(&app_pool, organization_id, owner_id)
            .await
            .is_empty(),
        "a tombstoned list is never a discoverable Today source"
    );
    let preference_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2 AND list_id = $3",
    )
    .bind(organization_id)
    .bind(owner_id)
    .bind(list.list.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(preference_count, 0);
    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM saved_list WHERE id = $1")
            .bind(list.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(deleted_at.is_some());
}

/// Source target-state and saved-list configuration are private local state:
/// their authenticated HTTP mutations must not publish an Organization event.
#[sqlx::test]
#[ignore]
async fn today_source_http_mutations_and_list_delete_publish_no_organization_event(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source quiet mutations",
        "owner@today-source-quiet.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let list = create_list(&app_pool, organization_id, owner_id, "Quiet source").await;
    let publisher = Publisher::recording();
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = crate::common::login_cookie(&router, "owner@today-source-quiet.test", PW).await;
    let source_uri = format!("/api/today/sources/{}", list.list.id);

    let enabled = crate::common::put_json_with_cookie(
        &router,
        &source_uri,
        &cookie,
        json!({ "expected_list_revision": list.list.revision }),
    )
    .await;
    assert_eq!(enabled.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(enabled).await,
        json!({ "enabled": true, "changed": true })
    );
    let disabled = crate::common::delete_with_cookie(&router, &source_uri, &cookie).await;
    assert_eq!(disabled.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(disabled).await,
        json!({ "enabled": false, "changed": true })
    );
    let deleted = delete_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{}", list.list.id),
        &cookie,
        json!({ "expected_revision": list.list.revision }),
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(deleted).await,
        json!({ "deleted": true })
    );
    assert!(
        recorded(&publisher).await.is_empty(),
        "source preferences and list definitions must not publish Organization events"
    );
}

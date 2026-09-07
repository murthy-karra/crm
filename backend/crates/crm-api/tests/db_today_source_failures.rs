//! Slice 011c AC8 controlled PostgreSQL failure paths.
//!
//! These tests use only app-role pools and database-local faults. They do not
//! install a process hook: a denied grant and real relation lock exercise the
//! production query, source savepoints, and owned-connection cleanup.

use std::time::Duration;

use chrono::Utc;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope, UpdateSavedList};
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayReason, TodaySourceIssueError, TodaySourcesStatus,
};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

async fn connect_as_one_app(migrator_pool: &PgPool) -> PgPool {
    let options = migrator_pool
        .connect_options()
        .as_ref()
        .clone()
        .username("crm_app")
        .password(&crate::common::app_password());
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("single-connection crm_app pool")
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_builtin_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    viewer_id: Uuid,
) -> Uuid {
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(viewer_id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'failure-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now())
    .execute(pool)
    .await
    .unwrap();
    person_id
}

fn stage_filter(stage_id: Uuid) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::Stage(StageClause {
            stage_ids: vec![StageId::new(stage_id)],
        })],
    }
}

fn explicit_assignee_filter(user_id: Uuid) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::User(UserId::new(user_id))],
        })],
    }
}

fn empty_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: Vec::new(),
    }
}

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    name: &str,
    filter: FilterDefinition,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, viewer_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: name.to_owned(),
            filter,
            sort: None,
        },
    )
    .await
    .unwrap()
}

async fn update_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    list_id: SavedListId,
    expected_revision: i64,
    name: &str,
    filter: FilterDefinition,
) -> saved_list::UpdateSavedListOutcome {
    saved_list::update_saved_list(
        app_pool,
        &command_context(organization_id, viewer_id),
        UpdateSavedList {
            list_id,
            expected_revision,
            name: name.to_owned(),
            filter,
            sort: None,
        },
    )
    .await
    .unwrap()
}

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    list_id: SavedListId,
    expected_list_revision: i64,
) {
    let change = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, viewer_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
    .unwrap();
    assert!(change.enabled);
}

fn scope(organization_id: Uuid) -> PersonVisibilityScope {
    PersonVisibilityScope::Organization(OrganizationId::new(organization_id))
}

fn item_for(items: &[TodayItem], person_id: Uuid) -> &TodayItem {
    items
        .iter()
        .find(|item| item.person.id.as_uuid() == person_id)
        .expect("captured built-in item")
}

fn has_list_reason(item: &TodayItem, list_id: SavedListId) -> bool {
    item.reasons.iter().any(
        |reason| matches!(reason, TodayReason::ListMember { list_id: actual, .. } if *actual == list_id),
    )
}

async fn backend_pid(connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
        .await
        .unwrap()
}

/// There is no timing sleep here. This waits until PostgreSQL reports the
/// owned query as waiting for the control transaction's membership-table lock.
async fn wait_for_membership_lock_wait(
    observer: &mut sqlx::PgConnection,
    query_pid: i32,
    control_pid: i32,
    membership_relation: i64,
) {
    let observed = tokio::time::timeout(Duration::from_millis(350), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM pg_locks waiting
                       WHERE waiting.pid = $1
                         AND waiting.relation::bigint = $3
                         AND waiting.mode = 'AccessShareLock'
                         AND NOT waiting.granted
                   )
                   AND $2 = ANY(pg_blocking_pids($1))"#,
            )
            .bind(query_pid)
            .bind(control_pid)
            .bind(membership_relation)
            .fetch_one(&mut *observer)
            .await
            .unwrap();
            if waiting {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        observed.is_ok(),
        "the explicit AssignedTo reference read never waited on the held membership lock"
    );
}

/// AC8 metadata outage: a denied source-preference read leaves captured
/// built-ins available and no fabricated per-source issue, then succeeds on a
/// clean owned connection after the grant is restored.
#[sqlx::test]
#[ignore]
async fn today_source_metadata_permission_failure_is_unavailable_and_owned_pool_recovers(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source metadata failure",
        "today-source-metadata-failure@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let builtin_id = insert_builtin_person(&app_pool, organization_id, stage_id, viewer_id).await;
    let source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Metadata source",
        stage_filter(stage_id),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        source.list.id,
        source.list.revision,
    )
    .await;
    drop(app_pool);

    sqlx::query("REVOKE SELECT ON TABLE today_work_source FROM crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let unavailable = today::query_owned(
        one_app_pool.acquire().await.unwrap(),
        &scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(
        unavailable.sources.status,
        TodaySourcesStatus::Unavailable
    ));
    assert!(
        unavailable.sources.issues.is_empty(),
        "metadata failure cannot pretend one configured source failed"
    );
    assert!(
        item_for(&unavailable.items, builtin_id)
            .reasons
            .iter()
            .all(|reason| !matches!(reason, TodayReason::ListMember { .. })),
        "a failed metadata read cannot fabricate source membership"
    );

    sqlx::query("GRANT SELECT ON TABLE today_work_source TO crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
    let clean = today::query_owned(
        one_app_pool.acquire().await.unwrap(),
        &scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(clean.sources.status, TodaySourcesStatus::Complete));
    assert!(clean.sources.issues.is_empty());
    assert!(has_list_reason(
        item_for(&clean.items, builtin_id),
        source.list.id
    ));
}

/// AC8 source failure: one source completes before PostgreSQL blocks the next
/// concrete AssignedTo reference. The timeout must preserve the captured
/// built-in and completed annotation while keeping its owned connection clean.
#[sqlx::test]
#[ignore]
async fn today_source_membership_lock_timeout_keeps_completed_source_and_reuses_owned_connection(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source membership lock",
        "today-source-membership-lock@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let builtin_id = insert_builtin_person(&app_pool, organization_id, stage_id, viewer_id).await;

    // Assign filters through the real update command after sorting generated
    // UUIDs, so the successful stage filter deterministically precedes the
    // explicit-user filter that will block during reference validation.
    let first = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Ordered source one",
        empty_filter(),
    )
    .await;
    let second = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Ordered source two",
        empty_filter(),
    )
    .await;
    let (completed_seed, blocked_seed) = if first.list.id.as_uuid() < second.list.id.as_uuid() {
        (first, second)
    } else {
        (second, first)
    };
    let completed = update_list(
        &app_pool,
        organization_id,
        viewer_id,
        completed_seed.list.id,
        completed_seed.list.revision,
        "Completed stage source",
        stage_filter(stage_id),
    )
    .await;
    let blocked = update_list(
        &app_pool,
        organization_id,
        viewer_id,
        blocked_seed.list.id,
        blocked_seed.list.revision,
        "Blocked explicit assignee source",
        explicit_assignee_filter(viewer_id),
    )
    .await;
    assert!(
        completed.list.id.as_uuid() < blocked.list.id.as_uuid(),
        "the successful source must be evaluated before the blocked source"
    );
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        completed.list.id,
        completed.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        blocked.list.id,
        blocked.list.revision,
    )
    .await;
    drop(app_pool);

    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut owned_connection = one_app_pool.acquire().await.unwrap();
    let owned_pid = backend_pid(&mut owned_connection).await;
    let mut observer = migrator_pool.acquire().await.unwrap();
    let membership_relation: i64 =
        sqlx::query_scalar("SELECT 'organization_membership'::regclass::oid::bigint")
            .fetch_one(&mut *observer)
            .await
            .unwrap();
    let mut control = migrator_pool.acquire().await.unwrap();
    let control_pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *control)
        .await
        .unwrap();
    sqlx::query("BEGIN").execute(&mut *control).await.unwrap();
    sqlx::query("LOCK TABLE organization_membership IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *control)
        .await
        .unwrap();

    let blocked_scope = scope(organization_id);
    let blocked_query = tokio::spawn(async move {
        today::query_owned(
            owned_connection,
            &blocked_scope,
            UserId::new(viewer_id),
            Utc::now(),
        )
        .await
    });
    wait_for_membership_lock_wait(&mut observer, owned_pid, control_pid, membership_relation).await;
    let partial = blocked_query.await.unwrap().unwrap();
    assert!(matches!(
        partial.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(partial.sources.issues.len(), 1);
    assert_eq!(partial.sources.issues[0].list_id, blocked.list.id);
    assert!(matches!(
        partial.sources.issues[0].error,
        TodaySourceIssueError::Unavailable
    ));
    assert!(
        has_list_reason(item_for(&partial.items, builtin_id), completed.list.id),
        "the source completed before the blocked reference must survive"
    );
    assert!(
        !has_list_reason(item_for(&partial.items, builtin_id), blocked.list.id),
        "the uncertain source must not contribute membership"
    );

    sqlx::query("ROLLBACK")
        .execute(&mut *control)
        .await
        .unwrap();
    drop(control);
    let mut reusable_connection = one_app_pool.acquire().await.unwrap();
    assert_eq!(
        backend_pid(&mut reusable_connection).await,
        owned_pid,
        "max-one owned pool should reuse the recovered connection"
    );
    let clean = today::query_owned(
        reusable_connection,
        &scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(clean.sources.status, TodaySourcesStatus::Complete));
    assert!(clean.sources.issues.is_empty());
    let recovered_item = item_for(&clean.items, builtin_id);
    assert!(has_list_reason(recovered_item, completed.list.id));
    assert!(has_list_reason(recovered_item, blocked.list.id));
}

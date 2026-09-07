//! Slice 011d correction (c): the call feed uses the SAME connection-
//! recovery mechanism as a list source (docs/specs/SLICE_011d.md §5 step 4;
//! 011c AC8) — actual SQL cancellation via a tight `statement_timeout`, a
//! savepoint rollback under the shared 100 ms grace, and (only if that
//! recovery itself fails) an unrecoverable signal that skips all list-
//! source work and detaches the connection instead of pooling it. Style
//! per `db_today_source_hooks.rs`: task-local `test_support` hooks around a
//! real owned app connection, deterministic checkpoints instead of timing
//! races.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::response::IntoResponse;
use chrono::{Duration as ChronoDuration, Utc};
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::system_feeds::commands::{self, PreviewTodaySystemFeed};
use crm_api::domain::today::system_feeds::error::TodayFeedError;
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayReason, TodaySourcesStatus,
};
use crm_api::error::ApiError;
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};
use crm_app::domain::today::test_support::{
    scope as with_today_hooks, HookFuture, TodayQueryHook, TodayQueryHooks, TodayQueryPhase,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool};
use tokio::sync::oneshot;
use uuid::Uuid;

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn visibility_scope(organization_id: Uuid) -> PersonVisibilityScope {
    PersonVisibilityScope::Organization(OrganizationId::new(organization_id))
}

/// An admin fixture (preview is admin-only) — mirrors
/// `db_today_system_feed_commands.rs`'s helper of the same shape.
async fn create_org_with_admin(pool: &PgPool, org_name: &str, email: &str) -> (Uuid, Uuid) {
    let org_id = crate::common::create_org(pool, org_name).await;
    crate::common::seed_stages(pool, org_id).await;
    let user_id =
        crate::common::create_user(pool, email, "Admin", "correct horse battery staple").await;
    crate::common::add_membership_with(
        pool,
        org_id,
        user_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    (org_id, user_id)
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

async fn backend_pid(connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
        .await
        .unwrap()
}

async fn stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry(pool: &PgPool, organization_id: Uuid, person_id: Uuid) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'call-failure-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now() - ChronoDuration::days(3))
    .execute(pool)
    .await
    .unwrap();
}

fn stage_filter(stage_id: Uuid) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::Stage(StageClause {
            stage_ids: vec![StageId::new(stage_id)],
        })],
    }
}

async fn create_and_enable_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    filter: FilterDefinition,
) -> SavedListId {
    let list = saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, viewer_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: "Call failure list".to_string(),
            filter,
            sort: None,
        },
    )
    .await
    .unwrap();
    let change = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, viewer_id),
        EnableTodayWorkSource {
            list_id: list.list.id,
            expected_list_revision: list.list.revision,
        },
    )
    .await
    .unwrap();
    assert!(change.enabled);
    list.list.id
}

fn item_for(items: &[TodayItem], person_id: Uuid) -> &TodayItem {
    items
        .iter()
        .find(|item| item.person.id.as_uuid() == person_id)
        .expect("captured Today item")
}

fn has_list_reason(item: &TodayItem, list_id: SavedListId) -> bool {
    item.reasons.iter().any(
        |reason| matches!(reason, TodayReason::ListMember { list_id: actual, .. } if *actual == list_id),
    )
}

/// Injects a SQL failure right after the call feed's own `SAVEPOINT` —
/// before `call_membership` runs — simulating that statement failing.
struct FailCallFeedAfterSavepoint;

impl TodayQueryHook for FailCallFeedAfterSavepoint {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == TodayQueryPhase::CallFeedAfterSavepoint && source_id.is_none() {
            return Box::pin(async move {
                sqlx::query("SELECT 1 / 0")
                    .execute(connection)
                    .await
                    .map(|_| ())
            });
        }
        Box::pin(async { Ok(()) })
    }
}

/// Injects a SQL failure right after `call_membership` completes — before
/// `call_only` runs — simulating that statement failing while membership
/// demonstrably succeeded.
struct FailCallFeedAfterMembership;

impl TodayQueryHook for FailCallFeedAfterMembership {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == TodayQueryPhase::CallFeedAfterMembership && source_id.is_none() {
            return Box::pin(async move {
                sqlx::query("SELECT 1 / 0")
                    .execute(connection)
                    .await
                    .map(|_| ())
            });
        }
        Box::pin(async { Ok(()) })
    }
}

/// First injects the call feed's SQL failure, then holds the actual
/// rollback checkpoint forever. Production's 100 ms recovery budget, not
/// this test, releases it.
struct ExhaustCallFeedRecoveryBudget {
    recovery_started: Mutex<Option<oneshot::Sender<(Instant, Instant)>>>,
}

impl TodayQueryHook for ExhaustCallFeedRecoveryBudget {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id.is_some() {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            TodayQueryPhase::CallFeedAfterSavepoint => Box::pin(async move {
                sqlx::query("SELECT 1 / 0")
                    .execute(connection)
                    .await
                    .map(|_| ())
            }),
            TodayQueryPhase::RecoveryAfterRollback => {
                let recovery_started = self.recovery_started.lock().unwrap().take();
                Box::pin(async move {
                    if let Some(recovery_started) = recovery_started {
                        recovery_started
                            .send((
                                Instant::now(),
                                deadline.expect("call feed recovery passes its shared deadline"),
                            ))
                            .expect("test observes call feed rollback cleanup");
                    }
                    std::future::pending::<Result<(), sqlx::Error>>().await
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

/// Statement failure in `call_membership` (before it ever queries `call`):
/// partial status, the call feed's issue, intact person-state items, and
/// list sources still evaluated on the recovered connection.
#[sqlx::test]
#[ignore]
async fn call_membership_failure_is_partial_with_intact_person_state_and_list_sources(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Call feed membership failure",
        "call-membership-failure@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let list_id =
        create_and_enable_list(&app_pool, organization_id, viewer_id, stage_filter(stage)).await;

    let hooks = TodayQueryHooks::new(Arc::new(FailCallFeedAfterSavepoint));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let partial = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(organization_id),
            UserId::new(viewer_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert!(matches!(
        partial.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(partial.sources.system_feed_issues.len(), 1);
    assert_eq!(
        partial.sources.system_feed_issues[0].feed_key,
        "call_outcome_needed"
    );
    assert_eq!(
        partial.sources.system_feed_issues[0].error,
        crm_api::domain::today::SystemFeedIssueError::Unavailable
    );
    assert!(!partial.sources.system_feed_issues[0].fallback);
    assert!(partial.sources.issues.is_empty());

    let item = item_for(&partial.items, builtin_person);
    assert!(
        item.reasons
            .iter()
            .any(|r| matches!(r, TodayReason::NoContactAttempt { .. })),
        "the person-state item survives the call feed's failure intact"
    );
    assert!(
        has_list_reason(item, list_id),
        "list sources still evaluate on the recovered connection"
    );

    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    // A recovered failure reuses the SAME connection (contrast with the
    // unrecoverable test below), so a fresh acquire from the max-one pool
    // must still succeed promptly.
    let _ = backend_pid(&mut replacement_connection).await;
}

/// Statement failure in `call_only` (after `call_membership` already
/// succeeded): the SAME guarantees, and specifically proves membership's
/// results were not partially merged before the later failure.
#[sqlx::test]
#[ignore]
async fn call_only_failure_is_partial_with_intact_person_state_and_list_sources(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Call feed call-only failure",
        "call-only-failure@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let list_id =
        create_and_enable_list(&app_pool, organization_id, viewer_id, stage_filter(stage)).await;

    let hooks = TodayQueryHooks::new(Arc::new(FailCallFeedAfterMembership));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let partial = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(organization_id),
            UserId::new(viewer_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert!(matches!(
        partial.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(partial.sources.system_feed_issues.len(), 1);
    assert_eq!(
        partial.sources.system_feed_issues[0].feed_key,
        "call_outcome_needed"
    );
    assert_eq!(
        partial.sources.system_feed_issues[0].error,
        crm_api::domain::today::SystemFeedIssueError::Unavailable
    );

    let item = item_for(&partial.items, builtin_person);
    assert!(
        item.reasons
            .iter()
            .any(|r| matches!(r, TodayReason::NoContactAttempt { .. })),
        "the person-state item survives intact"
    );
    assert!(
        !item
            .reasons
            .iter()
            .any(|r| matches!(r, TodayReason::CallOutcomeNeeded { .. })),
        "call_membership's own success must never be half-applied when call_only later fails"
    );
    assert!(has_list_reason(item, list_id));
}

/// The call feed's own savepoint rollback cannot complete inside the
/// shared 100 ms grace: the connection is unhealthy, the whole Today
/// response reports unavailable with the call feed's issue still present
/// (never a fabricated partial source result), and the owned connection is
/// discarded rather than pooled.
#[sqlx::test]
#[ignore]
async fn call_feed_unrecoverable_failure_marks_the_response_unavailable_and_discards_the_connection(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Call feed unrecoverable",
        "call-feed-unrecoverable@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let _list_id =
        create_and_enable_list(&app_pool, organization_id, viewer_id, stage_filter(stage)).await;

    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut owned_connection = one_app_pool.acquire().await.unwrap();
    let owned_pid = backend_pid(&mut owned_connection).await;
    let (recovery_started_tx, recovery_started_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(ExhaustCallFeedRecoveryBudget {
        recovery_started: Mutex::new(Some(recovery_started_tx)),
    }));
    let query = tokio::spawn(async move {
        with_today_hooks(
            hooks,
            today::query_owned_at(
                owned_connection,
                &visibility_scope(organization_id),
                UserId::new(viewer_id),
                Utc::now(),
            ),
        )
        .await
    });
    let (recovery_checkpoint, recovery_deadline) = recovery_started_rx
        .await
        .expect("rollback checkpoint reached after the call feed's failure");
    let unavailable = tokio::time::timeout(Duration::from_secs(1), query)
        .await
        .expect("the 100 ms cleanup budget bounds the query")
        .unwrap()
        .unwrap();
    let completed_at = Instant::now();
    let remaining_at_checkpoint = recovery_deadline.saturating_duration_since(recovery_checkpoint);
    assert!(
        (Duration::from_millis(95)..=Duration::from_millis(100)).contains(&remaining_at_checkpoint),
        "RecoveryAfterRollback must receive the real 100 ms cleanup deadline"
    );
    assert!(
        completed_at + Duration::from_millis(5) >= recovery_deadline,
        "the pending recovery checkpoint must be stopped only at the cleanup deadline"
    );

    assert!(matches!(
        unavailable.sources.status,
        TodaySourcesStatus::Unavailable
    ));
    assert!(
        unavailable.sources.issues.is_empty(),
        "list sources were never reached — enumeration itself never ran"
    );
    assert_eq!(unavailable.sources.system_feed_issues.len(), 1);
    assert_eq!(
        unavailable.sources.system_feed_issues[0].feed_key,
        "call_outcome_needed"
    );
    assert!(
        item_for(&unavailable.items, builtin_person)
            .reasons
            .iter()
            .any(|r| matches!(r, TodayReason::NoContactAttempt { .. })),
        "the person-state work captured before the call feed ran is still returned"
    );

    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    assert_ne!(
        backend_pid(&mut replacement_connection).await,
        owned_pid,
        "an unrecoverable call-feed failure must dispose the old owned connection"
    );
}

// --- Coverage gap (2): preview's real statement timeout ---------------------

/// Injects a genuinely slow query (`pg_sleep`) at the checkpoint preview
/// reaches immediately AFTER its real `statement_timeout` is configured —
/// so PostgreSQL itself cancels the sleep once the real 1,250 ms budget
/// elapses (error 57014), never a simulated/synthetic failure.
struct SleepPastPreviewTimeout;

impl TodayQueryHook for SleepPastPreviewTimeout {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        _source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == TodayQueryPhase::PreviewBeforeEvaluation {
            return Box::pin(async move {
                sqlx::query("SELECT pg_sleep(2)")
                    .execute(connection)
                    .await
                    .map(|_| ())
            });
        }
        Box::pin(async { Ok(()) })
    }
}

/// docs/specs/SLICE_011d.md §4: preview's 1,250 ms statement timeout is an
/// error, never a partial result — a real PostgreSQL cancellation
/// propagates as `TodayFeedError::Database`, which `ApiError::from` maps
/// to `Unavailable` (503), exactly like every other genuine database
/// failure in this domain (never a bespoke "preview timed out" code).
#[sqlx::test]
#[ignore]
async fn preview_statement_timeout_is_unavailable_and_never_partial(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d preview timeout",
        "admin@d011-preview-timeout.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id).await;
    let person = insert_person(&app_pool, organization_id, stage, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;

    let hooks = TodayQueryHooks::new(Arc::new(SleepPastPreviewTimeout));
    let started = Instant::now();
    let result = with_today_hooks(
        hooks,
        commands::preview_today_system_feed(
            &app_pool,
            &command_context(organization_id, admin_id),
            PreviewTodaySystemFeed {
                feed_key: FeedKey::UnansweredInquiry,
                filter: FilterDefinition {
                    version: 1,
                    clauses: vec![
                        Clause::AssignedTo(AssignedToClause {
                            assignees: vec![Assignee::Me],
                        }),
                        Clause::AwaitingResponse(BoolClause { value: true }),
                    ],
                },
                fresh_within_hours: Some(24),
                subject: UserId::new(admin_id),
            },
        ),
    )
    .await;
    let elapsed = started.elapsed();

    let err = result.expect_err(
        "a real statement-timeout cancellation must propagate as an error, never a partial result",
    );
    assert!(matches!(err, TodayFeedError::Database(_)));
    assert!(
        elapsed < Duration::from_millis(1900),
        "the real 1,250 ms statement_timeout must cancel the 2 s sleep, not wait it out: {elapsed:?}"
    );
    let response = ApiError::from(err).into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );

    // Nothing was persisted: the feed row is untouched by the cancelled
    // preview transaction.
    let (enabled, filter, revision): (bool, Option<String>, i64) = sqlx::query_as(
        "SELECT enabled, filter::text, revision FROM today_system_feed \
         WHERE organization_id = $1 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(organization_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    assert!(enabled);
    assert!(filter.is_none());
    assert_eq!(revision, 1);
}

//! Slice 011c transaction-setting and owned-query cancellation regressions.
//!
//! The checkpoints are task-local and each test owns its own channels. They
//! exercise the real app pool, Today query, and Operator service without a
//! process-wide fault switch or timing guesses about query progress.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use crm_api::auth::AuthContext;
use crm_api::domain::admin::Role;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{Clause, FilterDefinition, StageClause};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::{
    self, DisableTodayWorkSource, EnableTodayWorkSource, TodayItem, TodayReason,
    TodaySourceIssueError, TodaySourcesStatus,
};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};
use crm_api::operator::SqlxToolBackend;
use crm_app::domain::today::test_support::{
    scope as with_today_hooks, HookFuture, TodayQueryHook, TodayQueryHooks, TodayQueryPhase,
};
use crm_operator::{
    ChatResponse, Limits, OperatorContext, OperatorService, ScreenContext, ScriptedProvider,
    ScriptedStep, ToolCall, TurnInput, TurnOutcome,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool, Postgres};
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

/// `SqlxToolBackend::new`'s `AuthContext` (docs/specs/SLICE_013.md §2): only
/// `run_saved_list` reads it, which this file's one `SqlxToolBackend` call
/// does not exercise (it runs `get_today`) — a well-formed fixture is
/// enough.
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

fn visibility_scope(organization_id: Uuid) -> PersonVisibilityScope {
    PersonVisibilityScope::Organization(OrganizationId::new(organization_id))
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

async fn insert_source_only_person(pool: &PgPool, organization_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn stage_filter(stage_id: Uuid) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::Stage(StageClause {
            stage_ids: vec![StageId::new(stage_id)],
        })],
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

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    list_id: SavedListId,
    expected_list_revision: i64,
) {
    let changed = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, viewer_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
    .unwrap();
    assert!(changed.enabled);
}

async fn backend_pid(connection: &mut sqlx::pool::PoolConnection<Postgres>) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
        .await
        .unwrap()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSettings {
    jit: String,
    enable_mergejoin: String,
    statement_timeout: String,
    transaction_read_only: String,
    transaction_isolation: String,
}

async fn active_transaction_settings(connection: &mut PgConnection) -> SessionSettings {
    SessionSettings {
        jit: sqlx::query_scalar("SHOW jit")
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        enable_mergejoin: sqlx::query_scalar("SHOW enable_mergejoin")
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        statement_timeout: sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        transaction_read_only: sqlx::query_scalar("SHOW transaction_read_only")
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        transaction_isolation: sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
    }
}

async fn show_settings(connection: &mut sqlx::pool::PoolConnection<Postgres>) -> SessionSettings {
    SessionSettings {
        jit: sqlx::query_scalar("SHOW jit")
            .fetch_one(&mut **connection)
            .await
            .unwrap(),
        enable_mergejoin: sqlx::query_scalar("SHOW enable_mergejoin")
            .fetch_one(&mut **connection)
            .await
            .unwrap(),
        statement_timeout: sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut **connection)
            .await
            .unwrap(),
        transaction_read_only: sqlx::query_scalar("SHOW transaction_read_only")
            .fetch_one(&mut **connection)
            .await
            .unwrap(),
        transaction_isolation: sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut **connection)
            .await
            .unwrap(),
    }
}

/// Establish deliberately non-default session characteristics and release the
/// only pool connection. Every Today read below must restore this exact state
/// before that same socket is returned to the pool.
async fn configured_settings(pool: &PgPool) -> (i32, SessionSettings) {
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("SET jit = on")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("SET enable_mergejoin = on")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("SET statement_timeout = '431ms'")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query(
        "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ WRITE",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    let pid = backend_pid(&mut connection).await;
    let settings = show_settings(&mut connection).await;
    assert_eq!(
        settings.transaction_read_only, "off",
        "the pre-Today session baseline must be explicitly READ WRITE"
    );
    (pid, settings)
}

async fn assert_reused_settings(pool: &PgPool, expected_pid: i32, expected: &SessionSettings) {
    let mut connection = pool.acquire().await.unwrap();
    assert_eq!(
        backend_pid(&mut connection).await,
        expected_pid,
        "the healthy owned query must return its same socket"
    );
    assert_eq!(&show_settings(&mut connection).await, expected);
}

fn has_list_reason(item: &TodayItem, list_id: SavedListId) -> bool {
    item.reasons.iter().any(
        |reason| matches!(reason, TodayReason::ListMember { list_id: actual, .. } if *actual == list_id),
    )
}

/// Produces a recoverable source failure after membership while the source
/// savepoint is live. A completed source precedes the injected error, then the
/// real rollback/release path must leave the connection fit for reuse and
/// restore all transaction-local settings.
struct RecoverableMembershipFailure {
    successful_source_id: Uuid,
    failing_source_id: Uuid,
    successful_source_completed: AtomicBool,
    error_injected_after_successful_source: AtomicBool,
}

impl TodayQueryHook for RecoverableMembershipFailure {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        match phase {
            // This follows both source reads, so a set flag proves a real
            // enabled source completed before the next source is failed.
            TodayQueryPhase::BeforeSourceRelease
                if source_id == Some(self.successful_source_id) =>
            {
                self.successful_source_completed
                    .store(true, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }
            TodayQueryPhase::SourceAfterMembership if source_id == Some(self.failing_source_id) => {
                self.error_injected_after_successful_source.store(
                    self.successful_source_completed.load(Ordering::SeqCst),
                    Ordering::SeqCst,
                );
                Box::pin(async move {
                    sqlx::query("SELECT 1 / 0")
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

/// Reads transaction-local settings after the real source savepoint begins.
/// This observes the active Today transaction itself, rather than merely its
/// post-commit pooled connection state.
struct ActiveTodayTransactionSettings {
    source_id: Uuid,
    captured: Mutex<Option<SessionSettings>>,
}

impl TodayQueryHook for ActiveTodayTransactionSettings {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase != TodayQueryPhase::SourceAfterSavepoint || source_id != Some(self.source_id) {
            return Box::pin(async { Ok(()) });
        }
        let captured = &self.captured;
        Box::pin(async move {
            let settings = active_transaction_settings(connection).await;
            *captured.lock().unwrap() = Some(settings);
            Ok(())
        })
    }
}

/// Captures an Operator-owned Today connection at its real post-built-in
/// checkpoint and never releases it. The service's own turn timeout is the
/// cancellation mechanism under test.
struct OperatorAfterBuiltinsGate {
    arrived: Mutex<Option<oneshot::Sender<i32>>>,
}

impl TodayQueryHook for OperatorAfterBuiltinsGate {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        _source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase != TodayQueryPhase::AfterBuiltins {
            return Box::pin(async { Ok(()) });
        }
        let arrived = self.arrived.lock().unwrap().take();
        Box::pin(async move {
            if let Some(arrived) = arrived {
                let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(&mut *connection)
                    .await?;
                arrived
                    .send(pid)
                    .expect("test observes the blocked Operator connection");
            }
            std::future::pending::<Result<(), sqlx::Error>>().await
        })
    }
}

/// AC8/§8: `SET LOCAL jit`, source statement timeouts, and explicit read-only
/// repeatable-read transaction settings disappear at every healthy owned-query
/// boundary: normal, structurally malformed partial, and recovered SQL error.
#[sqlx::test]
#[ignore]
async fn today_owned_query_restores_pool_settings_after_normal_partial_and_recovered_source_error(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source settings",
        "today-source-settings@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let (pid, expected) = configured_settings(&one_app_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let normal_source_person =
        insert_source_only_person(&app_pool, organization_id, stage_id).await;
    let normal_source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Successful source setting probe",
        stage_filter(stage_id),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        normal_source.list.id,
        normal_source.list.revision,
    )
    .await;
    let active = Arc::new(ActiveTodayTransactionSettings {
        source_id: normal_source.list.id.as_uuid(),
        captured: Mutex::new(None),
    });
    let normal = with_today_hooks(
        TodayQueryHooks::new(active.clone()),
        today::query_owned(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(organization_id),
            UserId::new(viewer_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(
        normal.sources.status,
        TodaySourcesStatus::Complete
    ));
    assert!(
        has_list_reason(
            normal
                .items
                .iter()
                .find(|item| item.person.id.as_uuid() == normal_source_person)
                .expect("successful enabled source returns its source-only candidate"),
            normal_source.list.id,
        ),
        "the normal read evaluates and commits an enabled source before later failure coverage"
    );
    let active_settings = active
        .captured
        .lock()
        .unwrap()
        .clone()
        .expect("source checkpoint observed active transaction settings");
    assert_eq!(active_settings.jit, "off");
    assert_eq!(active_settings.enable_mergejoin, "off");
    assert_eq!(active_settings.transaction_read_only, "on");
    assert_eq!(active_settings.transaction_isolation, "repeatable read");
    assert_ne!(
        active_settings.statement_timeout,
        expected.statement_timeout
    );
    assert_reused_settings(&one_app_pool, pid, &expected).await;
    let disabled_normal = today::disable_today_work_source(
        &app_pool,
        &command_context(organization_id, viewer_id),
        DisableTodayWorkSource {
            list_id: normal_source.list.id,
        },
    )
    .await
    .unwrap();
    assert!(!disabled_normal.enabled);

    let malformed = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Malformed source",
        stage_filter(stage_id),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        malformed.list.id,
        malformed.list.revision,
    )
    .await;
    // A malformed stored definition is an integrity fixture only. Ordinary
    // source commands reject it; Today's partial-state behavior must still
    // return this connection cleanly.
    sqlx::query(
        "UPDATE saved_list SET filter = '{\"version\": 2, \"clauses\": []}'::jsonb WHERE id = $1",
    )
    .bind(malformed.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();
    let malformed_result = today::query_owned(
        one_app_pool.acquire().await.unwrap(),
        &visibility_scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(
        malformed_result.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(malformed_result.sources.issues.len(), 1);
    assert!(matches!(
        malformed_result.sources.issues[0].error,
        TodaySourceIssueError::UnsupportedFilter
    ));
    assert_reused_settings(&one_app_pool, pid, &expected).await;

    let disabled = today::disable_today_work_source(
        &app_pool,
        &command_context(organization_id, viewer_id),
        DisableTodayWorkSource {
            list_id: malformed.list.id,
        },
    )
    .await
    .unwrap();
    assert!(!disabled.enabled);
    let source_only_person = insert_source_only_person(&app_pool, organization_id, stage_id).await;
    let first = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Completed source before recovery",
        stage_filter(stage_id),
    )
    .await;
    let second = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Recoverable source",
        stage_filter(stage_id),
    )
    .await;
    let (successful, failing) = if first.list.id.as_uuid() < second.list.id.as_uuid() {
        (first, second)
    } else {
        (second, first)
    };
    assert!(
        successful.list.id.as_uuid() < failing.list.id.as_uuid(),
        "the completed source must be evaluated before the injected failure"
    );
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        successful.list.id,
        successful.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        failing.list.id,
        failing.list.revision,
    )
    .await;
    let failure = Arc::new(RecoverableMembershipFailure {
        successful_source_id: successful.list.id.as_uuid(),
        failing_source_id: failing.list.id.as_uuid(),
        successful_source_completed: AtomicBool::new(false),
        error_injected_after_successful_source: AtomicBool::new(false),
    });
    let partial = with_today_hooks(
        TodayQueryHooks::new(failure.clone()),
        today::query_owned(
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
    assert_eq!(partial.sources.issues.len(), 1);
    assert_eq!(partial.sources.issues[0].list_id, failing.list.id);
    assert!(matches!(
        partial.sources.issues[0].error,
        TodaySourceIssueError::Unavailable
    ));
    assert!(
        failure.successful_source_completed.load(Ordering::SeqCst),
        "the completed enabled source reached its post-read checkpoint"
    );
    assert!(
        failure
            .error_injected_after_successful_source
            .load(Ordering::SeqCst),
        "the injected source error must occur after a successful enabled source read"
    );
    let completed_item = partial
        .items
        .iter()
        .find(|item| item.person.id.as_uuid() == source_only_person)
        .expect("the completed source's source-only candidate survives the later failure");
    assert!(has_list_reason(completed_item, successful.list.id));
    assert!(
        !has_list_reason(completed_item, failing.list.id),
        "the source that failed after membership cannot contribute staged work"
    );
    assert_reused_settings(&one_app_pool, pid, &expected).await;
}

/// AC8: a real `OperatorService` turn enters the actual `SqlxToolBackend`
/// Today path, times out while the owned query is blocked at `AfterBuiltins`,
/// and must discard that max-one-pool connection before a normal Today read.
#[sqlx::test]
#[ignore]
async fn operator_turn_timeout_cancels_owned_today_query_and_replaces_its_connection(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source operator cancellation",
        "today-source-operator-cancellation@example.test",
        "Alice",
        "pw",
    )
    .await;
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut initial_connection = one_app_pool.acquire().await.unwrap();
    let initial_pid = backend_pid(&mut initial_connection).await;
    drop(initial_connection);

    let provider =
        ScriptedProvider::new(vec![ScriptedStep::Respond(ChatResponse::tool_calls(vec![
            ToolCall {
                id: "today-cancel".to_owned(),
                name: "get_today".to_owned(),
                arguments: "{\"limit\":1}".to_owned(),
            },
        ]))]);
    let service = OperatorService::new(
        Arc::new(provider.clone()),
        Limits {
            turn_timeout: Duration::from_secs(1),
            ..Limits::default()
        },
    );
    let backend = SqlxToolBackend::new(
        one_app_pool.clone(),
        Duration::from_secs(120),
        auth_context(organization_id, viewer_id),
    );
    let context = OperatorContext {
        actor_user_id: viewer_id,
        organization_id,
        actor_display_name: "Alice".to_owned(),
        turn_id: Uuid::new_v4(),
        now: Utc::now(),
    };
    let input = TurnInput {
        message: "Show my Today work".to_owned(),
        history: Vec::new(),
        screen: ScreenContext::other(),
    };
    let (arrived_tx, arrived_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(OperatorAfterBuiltinsGate {
        arrived: Mutex::new(Some(arrived_tx)),
    }));
    let turn = tokio::spawn(async move {
        with_today_hooks(hooks, service.run_turn(&context, &backend, input)).await
    });

    let query_pid = tokio::time::timeout(Duration::from_secs(2), arrived_rx)
        .await
        .expect("the real get_today tool reaches AfterBuiltins")
        .expect("the hook reports the app connection");
    assert_eq!(
        query_pid, initial_pid,
        "the max-one pool must lend the initially observed connection to Operator"
    );
    let output = tokio::time::timeout(Duration::from_secs(2), turn)
        .await
        .expect("Operator turn timeout bounds the blocked owned query")
        .expect("Operator task joins");
    assert_eq!(output.outcome, TurnOutcome::TurnTimeout);
    assert_eq!(
        provider.requests().len(),
        1,
        "the scripted get_today was issued"
    );

    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    let replacement_pid = backend_pid(&mut replacement_connection).await;
    assert_ne!(
        replacement_pid, initial_pid,
        "cancelling the owned Today query must dispose its socket"
    );
    let normal = today::query_owned(
        replacement_connection,
        &visibility_scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(
        normal.sources.status,
        TodaySourcesStatus::Complete
    ));
}

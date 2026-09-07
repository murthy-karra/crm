//! Slice 011c source-deadline regressions.
//!
//! Each test installs a task-local checkpoint hook around one real owned Today
//! query.  The hooks report only phase/deadline information to their own test;
//! they do not alter global process state or infer query progress from sleeps.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Utc};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{Clause, FilterDefinition, StageClause};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayReason, TodaySourceIssueError, TodaySourcesStatus,
};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};
use crm_app::domain::today::test_support::{
    scope as with_today_hooks, HookFuture, TodayQueryHook, TodayQueryHooks, TodayQueryPhase,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool, Postgres};
use tokio::sync::{mpsc, oneshot};
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
         VALUES ($1, $2, $3, 'today-deadline-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now() - ChronoDuration::days(3))
    .execute(pool)
    .await
    .unwrap();
    person_id
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

async fn backend_pid(connection: &mut sqlx::pool::PoolConnection<Postgres>) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
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

fn has_list_reason(item: &TodayItem, list_id: SavedListId) -> bool {
    item.reasons.iter().any(
        |reason| matches!(reason, TodayReason::ListMember { list_id: actual, .. } if *actual == list_id),
    )
}

fn item_for(items: &[TodayItem], person_id: Uuid) -> &TodayItem {
    items
        .iter()
        .find(|item| item.person.id.as_uuid() == person_id)
        .expect("captured built-in item")
}

type DeadlineSignal = (TodayQueryPhase, Instant, Instant);

/// Records the absolute source deadline at every required source checkpoint,
/// then leaves the source suspended before `RELEASE SAVEPOINT`. The query's
/// own 500 ms outer source timeout must cancel this hook and discard its
/// staged contribution.
struct WholeSourceDeadlineGate {
    source_id: Uuid,
    trace: mpsc::UnboundedSender<DeadlineSignal>,
    before_release: Mutex<Option<oneshot::Sender<()>>>,
}

impl TodayQueryHook for WholeSourceDeadlineGate {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        deadline: Option<Instant>,
        _connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id != Some(self.source_id) {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            TodayQueryPhase::SourceAfterSavepoint
            | TodayQueryPhase::SourceAfterMembership
            | TodayQueryPhase::BeforeSourceRelease => {
                let deadline = deadline.expect("source checkpoints carry one absolute deadline");
                self.trace
                    .send((phase, deadline, Instant::now()))
                    .expect("test receives source checkpoints");
                if phase == TodayQueryPhase::SourceAfterSavepoint {
                    // Spend a declared part of the same source allowance
                    // before later phases. The release gate below must still
                    // expire at this original deadline, never a renewed 500ms.
                    return Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        Ok(())
                    });
                }
                if phase == TodayQueryPhase::SourceAfterMembership {
                    return Box::pin(async { Ok(()) });
                }
                let before_release = self.before_release.lock().unwrap().take();
                Box::pin(async move {
                    if let Some(before_release) = before_release {
                        before_release
                            .send(())
                            .expect("test observes the controlled source gate");
                    }
                    std::future::pending::<Result<(), sqlx::Error>>().await
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

/// Injects one recoverable SQL error, consumes part of the recovery allowance,
/// and then holds the final commit past its original deadline. A structurally
/// malformed trailing source does not start another evaluation, so it must not
/// renew the 100 ms recovery allowance.
struct RecoveryCommitDeadlineTrace {
    failing_source_id: Uuid,
    trace: mpsc::UnboundedSender<DeadlineSignal>,
}

impl TodayQueryHook for RecoveryCommitDeadlineTrace {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == TodayQueryPhase::SourceAfterMembership
            && source_id == Some(self.failing_source_id)
        {
            return Box::pin(async move {
                sqlx::query("SELECT 1 / 0")
                    .execute(connection)
                    .await
                    .map(|_| ())
            });
        }
        if phase == TodayQueryPhase::RecoveryAfterRollback
            && source_id == Some(self.failing_source_id)
        {
            let deadline = deadline.expect("recovery and final commit carry a deadline");
            self.trace
                .send((phase, deadline, Instant::now()))
                .expect("test receives recovery checkpoint");
            return Box::pin(async move {
                let consume_until = deadline
                    .checked_sub(Duration::from_millis(40))
                    .expect("the recovery allowance exceeds the controlled remainder");
                tokio::time::sleep_until(tokio::time::Instant::from_std(consume_until)).await;
                Ok(())
            });
        }
        if phase == TodayQueryPhase::BeforeFinalCommit {
            let deadline = deadline.expect("recovery and final commit carry a deadline");
            self.trace
                .send((phase, deadline, Instant::now()))
                .expect("test receives final-commit checkpoint");
            return Box::pin(async move {
                // The outer commit timeout must cancel this hook at the same
                // original deadline; one extra millisecond removes a tie at
                // the timer boundary without inventing additional allowance.
                tokio::time::sleep_until(
                    tokio::time::Instant::from_std(deadline) + Duration::from_millis(1),
                )
                .await;
                Ok(())
            });
        }
        Box::pin(async { Ok(()) })
    }
}

/// AC8: one 500 ms source deadline covers savepoint setup, membership and
/// release. A source held before release expires as one unit and contributes
/// neither its built-in reason nor candidates to the partial response.
#[sqlx::test]
#[ignore]
async fn today_source_uses_one_absolute_deadline_through_savepoint_membership_and_release(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source whole deadline",
        "today-source-whole-deadline@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let builtin_person =
        insert_builtin_person(&app_pool, organization_id, stage_id, viewer_id).await;
    let source_only_person = insert_source_only_person(&app_pool, organization_id, stage_id).await;
    let source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Whole deadline source",
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

    let (trace_tx, mut trace_rx) = mpsc::unbounded_channel();
    let (before_release_tx, before_release_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(WholeSourceDeadlineGate {
        source_id: source.list.id.as_uuid(),
        trace: trace_tx,
        before_release: Mutex::new(Some(before_release_tx)),
    }));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let query_scope = visibility_scope(organization_id);
    let query = tokio::spawn(async move {
        with_today_hooks(
            hooks,
            today::query_owned(
                one_app_pool.acquire().await.unwrap(),
                &query_scope,
                UserId::new(viewer_id),
                Utc::now(),
            ),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), before_release_rx)
        .await
        .expect("source reaches BeforeSourceRelease")
        .expect("source gate remains live");
    let partial = tokio::time::timeout(Duration::from_secs(2), query)
        .await
        .expect("the whole-source timeout cancels the pending release hook")
        .expect("query task joins")
        .unwrap();
    let completed_at = Instant::now();

    let mut trace = Vec::new();
    while let Ok(signal) = trace_rx.try_recv() {
        trace.push(signal);
    }
    assert_eq!(
        trace.iter().map(|(phase, _, _)| *phase).collect::<Vec<_>>(),
        vec![
            TodayQueryPhase::SourceAfterSavepoint,
            TodayQueryPhase::SourceAfterMembership,
            TodayQueryPhase::BeforeSourceRelease,
        ]
    );
    let deadline = trace[0].1;
    let remaining_after_savepoint = deadline.saturating_duration_since(trace[0].2);
    assert!(
        (Duration::from_millis(400)..=Duration::from_millis(500))
            .contains(&remaining_after_savepoint),
        "the first source checkpoint must receive the fixed 500 ms source budget"
    );
    assert!(
        trace.iter().all(|(_, actual, _)| *actual == deadline),
        "one source evaluation cannot renew its absolute deadline between phases"
    );
    let remaining_before_release = deadline.saturating_duration_since(trace[2].2);
    assert!(
        (Duration::from_millis(20)..=Duration::from_millis(250))
            .contains(&remaining_before_release),
        "the early savepoint checkpoint must consume more than the permitted final disposition slack before release"
    );
    assert!(
        completed_at >= deadline
            && completed_at.duration_since(deadline) <= Duration::from_millis(200),
        "the pending release must be bounded by the original absolute deadline plus recovery disposition"
    );
    assert!(matches!(
        partial.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(partial.sources.issues.len(), 1);
    assert_eq!(partial.sources.issues[0].list_id, source.list.id);
    assert!(matches!(
        partial.sources.issues[0].error,
        TodaySourceIssueError::Unavailable
    ));
    assert!(
        !has_list_reason(item_for(&partial.items, builtin_person), source.list.id),
        "a source held before release must discard staged built-in membership"
    );
    assert!(
        partial
            .items
            .iter()
            .all(|item| item.person.id.as_uuid() != source_only_person),
        "a source held before release must discard its staged source-only candidate"
    );
}

/// AC8: a recovered source failure sets a single 100 ms cleanup deadline. A
/// malformed tail is a structural skip, so `BeforeFinalCommit` receives that
/// same deadline and an overrun disposes the owned connection.
#[sqlx::test]
#[ignore]
async fn today_source_recovery_deadline_survives_a_malformed_tail_until_final_commit(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today source recovery deadline",
        "today-source-recovery-deadline@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let mut sources = vec![
        create_list(
            &app_pool,
            organization_id,
            viewer_id,
            "Recovery deadline source one",
            stage_filter(stage_id),
        )
        .await,
        create_list(
            &app_pool,
            organization_id,
            viewer_id,
            "Recovery deadline source two",
            stage_filter(stage_id),
        )
        .await,
    ];
    sources.sort_by_key(|source| source.list.id.as_uuid());
    let failing = sources.remove(0);
    let malformed_tail = sources.remove(0);
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        failing.list.id,
        failing.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        malformed_tail.list.id,
        malformed_tail.list.revision,
    )
    .await;
    // This is an integrity fixture: it keeps the enabled preference but makes
    // the later UUID-ordered source a structural skip rather than a new source
    // evaluation (and therefore it must not reset recovery allowance).
    sqlx::query(
        "UPDATE saved_list SET filter = '{\"version\": 2, \"clauses\": []}'::jsonb WHERE id = $1",
    )
    .bind(malformed_tail.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();

    let (trace_tx, mut trace_rx) = mpsc::unbounded_channel();
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut owned_connection = one_app_pool.acquire().await.unwrap();
    let owned_pid = backend_pid(&mut owned_connection).await;
    let partial = tokio::time::timeout(
        Duration::from_secs(1),
        with_today_hooks(
            TodayQueryHooks::new(Arc::new(RecoveryCommitDeadlineTrace {
                failing_source_id: failing.list.id.as_uuid(),
                trace: trace_tx,
            })),
            today::query_owned(
                owned_connection,
                &visibility_scope(organization_id),
                UserId::new(viewer_id),
                Utc::now(),
            ),
        ),
    )
    .await
    .expect("the shared recovery deadline bounds the final commit hook")
    .unwrap();

    let mut trace = Vec::new();
    while let Ok(signal) = trace_rx.try_recv() {
        trace.push(signal);
    }
    assert_eq!(
        trace.iter().map(|(phase, _, _)| *phase).collect::<Vec<_>>(),
        vec![
            TodayQueryPhase::RecoveryAfterRollback,
            TodayQueryPhase::BeforeFinalCommit,
        ]
    );
    let (recovery_deadline, recovery_observed) = (trace[0].1, trace[0].2);
    let (commit_deadline, commit_observed) = (trace[1].1, trace[1].2);
    assert_eq!(
        commit_deadline, recovery_deadline,
        "a malformed tail cannot grant final commit a fresh recovery deadline"
    );
    let remaining_at_recovery_checkpoint =
        recovery_deadline.saturating_duration_since(recovery_observed);
    assert!(
        (Duration::from_millis(80)..=Duration::from_millis(100))
            .contains(&remaining_at_recovery_checkpoint),
        "the recovery checkpoint must carry the bounded 100 ms cleanup deadline"
    );
    assert!(
        recovery_deadline.saturating_duration_since(commit_observed) <= Duration::from_millis(50),
        "the rollback checkpoint must consume the original recovery allowance before commit"
    );
    assert!(matches!(
        partial.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(
        partial
            .sources
            .issues
            .iter()
            .map(|issue| (issue.list_id, issue.error))
            .collect::<Vec<_>>(),
        vec![
            (failing.list.id, TodaySourceIssueError::Unavailable),
            (
                malformed_tail.list.id,
                TodaySourceIssueError::UnsupportedFilter
            ),
        ],
        "the recovered failing source and malformed tail were both processed"
    );
    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    assert_ne!(
        backend_pid(&mut replacement_connection).await,
        owned_pid,
        "a final commit held through the shared recovery deadline must discard its socket"
    );
}

//! Slice 011c controlled checkpoint tests for the Today source read path.
//!
//! These use only task-local `test_support` hooks around a real owned app
//! connection.  Each barrier is reached by an explicit query checkpoint; no
//! test guesses when a transaction has reached a query stage from elapsed time.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Utc};
use crm_api::domain::commands::{self, ContactChannel, ContactOutcome, LogContactAttempt};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{Clause, FilterDefinition, StageClause};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListScope, UpdateSavedList,
};
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayReason, TodaySourceIssueError, TodaySourcesStatus,
};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, SavedListId, StageId, UserId};
use crm_api::realtime::Publisher;
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

async fn stage_id(pool: &PgPool, organization_id: Uuid, offset: i64) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id OFFSET $2 LIMIT 1",
    )
    .bind(organization_id)
    .bind(offset)
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
         VALUES ($1, $2, $3, 'today-hook-fixture', $4)",
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

async fn backend_pid(connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
        .await
        .unwrap()
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

fn list_reason_name(item: &TodayItem, list_id: SavedListId) -> Option<&str> {
    item.reasons.iter().find_map(|reason| match reason {
        TodayReason::ListMember {
            list_id: actual,
            name,
        } if *actual == list_id => Some(name.as_str()),
        _ => None,
    })
}

/// Gates the already-snapshotted query immediately after the built-in read.
/// The test owns both channels, so the gate cannot affect another query.
struct AfterBuiltinsGate {
    arrived: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

impl TodayQueryHook for AfterBuiltinsGate {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        _source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        _connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase != TodayQueryPhase::AfterBuiltins {
            return Box::pin(async { Ok(()) });
        }
        let arrived = self.arrived.lock().unwrap().take();
        let release = self.release.lock().unwrap().take();
        Box::pin(async move {
            if let Some(arrived) = arrived {
                arrived.send(()).expect("snapshot arrival receiver is live");
            }
            if let Some(release) = release {
                release.await.expect("test releases snapshot gate");
            }
            Ok(())
        })
    }
}

/// Produces a real PostgreSQL execution failure after B's membership result,
/// while the source savepoint is still active.
struct FailAfterMembership {
    source_id: Uuid,
}

impl TodayQueryHook for FailAfterMembership {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == TodayQueryPhase::SourceAfterMembership && source_id == Some(self.source_id) {
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

/// Pauses B after its real membership query. The test terminates this exact
/// backend from an independent connection while it is idle, observes it leave
/// PostgreSQL, then lets the hook issue its next real statement.
struct TerminateAfterMembership {
    source_id: Uuid,
    captured_pid: Mutex<Option<oneshot::Sender<i32>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

impl TodayQueryHook for TerminateAfterMembership {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase != TodayQueryPhase::SourceAfterMembership || source_id != Some(self.source_id) {
            return Box::pin(async { Ok(()) });
        }
        let captured_pid = self.captured_pid.lock().unwrap().take();
        let release = self.release.lock().unwrap().take();
        Box::pin(async move {
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *connection)
                .await?;
            if let Some(captured_pid) = captured_pid {
                captured_pid
                    .send(pid)
                    .expect("test observes the terminated backend");
            }
            if let Some(release) = release {
                release
                    .await
                    .expect("test releases the terminated backend hook");
            }
            sqlx::query("SELECT 1").execute(connection).await?;
            Ok(())
        })
    }
}

/// First injects B's SQL failure, then holds the actual rollback checkpoint
/// forever. Production's 100 ms recovery budget, not this test, releases it.
struct ExhaustRecoveryBudget {
    source_id: Uuid,
    recovery_started: Mutex<Option<oneshot::Sender<(Instant, Instant)>>>,
}

impl TodayQueryHook for ExhaustRecoveryBudget {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id != Some(self.source_id) {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            TodayQueryPhase::SourceAfterMembership => Box::pin(async move {
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
                                deadline.expect("source recovery passes its shared deadline"),
                            ))
                            .expect("test observes rollback cleanup");
                    }
                    std::future::pending::<Result<(), sqlx::Error>>().await
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

/// Builds three sources in their actual UUID evaluation order. Each matches the
/// built-in row, which lets the assertions prove exactly which completed source
/// annotations survived a later source failure.
async fn ordered_stage_sources(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    stage_id: Uuid,
) -> Vec<saved_list::CreateSavedListOutcome> {
    let mut sources = vec![
        create_list(
            app_pool,
            organization_id,
            viewer_id,
            "Hook source one",
            stage_filter(stage_id),
        )
        .await,
        create_list(
            app_pool,
            organization_id,
            viewer_id,
            "Hook source two",
            stage_filter(stage_id),
        )
        .await,
        create_list(
            app_pool,
            organization_id,
            viewer_id,
            "Hook source three",
            stage_filter(stage_id),
        )
        .await,
    ];
    sources.sort_by_key(|source| source.list.id.as_uuid());
    for source in &sources {
        enable(
            app_pool,
            organization_id,
            viewer_id,
            source.list.id,
            source.list.revision,
        )
        .await;
    }
    sources
}

/// AC8 snapshot: an edit and a contact fact committed after the built-in
/// checkpoint cannot mix their new values into the already-open source read;
/// the next query sees both mutations together.
#[sqlx::test]
#[ignore]
async fn today_source_after_builtins_gate_keeps_one_snapshot_across_list_and_contact_mutations(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today hook snapshot",
        "today-hook-snapshot@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let builtin_stage = stage_id(&app_pool, organization_id, 0).await;
    let source_stage = stage_id(&app_pool, organization_id, 1).await;
    let empty_stage = stage_id(&app_pool, organization_id, 2).await;
    let builtin_person =
        insert_person(&app_pool, organization_id, builtin_stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let source_only_person = insert_person(&app_pool, organization_id, source_stage, None).await;
    let source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Snapshot stage source",
        stage_filter(source_stage),
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
    // This source remains live after the gate, so the next snapshot can prove
    // source-side hydration sees the committed contact change. The third
    // source is deleted concurrently to exercise materialized source metadata.
    let surviving_source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Snapshot surviving source",
        stage_filter(source_stage),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        surviving_source.list.id,
        surviving_source.list.revision,
    )
    .await;
    let deleted_source = create_list(
        &app_pool,
        organization_id,
        viewer_id,
        "Snapshot deleted source",
        stage_filter(source_stage),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        viewer_id,
        deleted_source.list.id,
        deleted_source.list.revision,
    )
    .await;

    let (arrived_tx, arrived_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(AfterBuiltinsGate {
        arrived: Mutex::new(Some(arrived_tx)),
        release: Mutex::new(Some(release_rx)),
    }));
    let query_pool = connect_as_one_app(&migrator_pool).await;
    let query_scope = visibility_scope(organization_id);
    let owned_connection = query_pool.acquire().await.unwrap();
    let snapshot_query = tokio::spawn(async move {
        with_today_hooks(
            hooks,
            today::query_owned_at(
                owned_connection,
                &query_scope,
                UserId::new(viewer_id),
                Utc::now(),
            ),
        )
        .await
    });
    arrived_rx.await.expect("built-in snapshot checkpoint");

    let source_update = update_list(
        &app_pool,
        organization_id,
        viewer_id,
        source.list.id,
        source.list.revision,
        "Snapshot now-empty source",
        stage_filter(empty_stage),
    );
    let delete_context = command_context(organization_id, viewer_id);
    let source_delete = saved_list::delete_saved_list(
        &app_pool,
        &delete_context,
        DeleteSavedList {
            list_id: deleted_source.list.id,
            expected_revision: deleted_source.list.revision,
        },
    );
    let publisher = Publisher::recording();
    let contact_context = command_context(organization_id, viewer_id);
    let contact_attempt = commands::log_contact_attempt(
        &app_pool,
        &publisher,
        &contact_context,
        LogContactAttempt {
            person_id: PersonId::new(source_only_person),
            channel: ContactChannel::Call,
            outcome: ContactOutcome::NoAnswer,
        },
    );
    let (updated, deleted, contact) = tokio::join!(source_update, source_delete, contact_attempt);
    assert!(updated.changed);
    assert!(deleted.unwrap().deleted);
    contact.unwrap();
    release_tx.send(()).expect("release snapshot query");

    let frozen = snapshot_query.await.unwrap().unwrap();
    assert!(matches!(
        frozen.sources.status,
        TodaySourcesStatus::Complete
    ));
    let frozen_source = item_for(&frozen.items, source_only_person);
    assert!(
        has_list_reason(frozen_source, source.list.id),
        "the snapshot must retain the old list definition"
    );
    assert_eq!(
        list_reason_name(frozen_source, source.list.id),
        Some("Snapshot stage source"),
        "old membership cannot be labeled with the concurrent rename"
    );
    assert!(
        frozen
            .items
            .iter()
            .any(|item| item.person.id.as_uuid() == source_only_person),
        "the snapshot must retain the old list membership"
    );
    assert!(
        has_list_reason(frozen_source, deleted_source.list.id),
        "the snapshot must retain source metadata that was deleted after it began"
    );
    assert!(
        frozen_source.last_contact_attempt.is_none(),
        "later source hydration must retain the pre-contact state from the snapshot"
    );

    let next = today::query_owned_at(
        query_pool.acquire().await.unwrap(),
        &visibility_scope(organization_id),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap();
    let next_source = item_for(&next.items, source_only_person);
    assert!(
        has_list_reason(next_source, surviving_source.list.id),
        "the unchanged source keeps the person visible after the concurrent update"
    );
    assert!(
        !has_list_reason(next_source, source.list.id)
            && !has_list_reason(next_source, deleted_source.list.id),
        "the next read observes both the edited filter and deleted source"
    );
    assert!(
        next_source.last_contact_attempt.is_some(),
        "the next source hydration observes the committed contact change"
    );
}

/// AC8 source savepoint: a real SQL error in B must discard B only. A's
/// completed reason remains and C is still evaluated after recovery.
#[sqlx::test]
#[ignore]
async fn today_source_membership_checkpoint_sql_failure_discards_only_that_source(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today hook savepoint",
        "today-hook-savepoint@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id, 0).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let sources = ordered_stage_sources(&app_pool, organization_id, viewer_id, stage).await;
    let a = sources[0].list.id;
    let b = sources[1].list.id;
    let c = sources[2].list.id;

    let hooks = TodayQueryHooks::new(Arc::new(FailAfterMembership {
        source_id: b.as_uuid(),
    }));
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
    assert_eq!(partial.sources.issues.len(), 1);
    assert_eq!(partial.sources.issues[0].list_id, b);
    assert!(matches!(
        partial.sources.issues[0].error,
        TodaySourceIssueError::Unavailable
    ));
    let item = item_for(&partial.items, builtin_person);
    assert!(has_list_reason(item, a), "A completed before B failed");
    assert!(!has_list_reason(item, b), "B must roll back entirely");
    assert!(has_list_reason(item, c), "C follows successful B recovery");
}

/// AC8 unrecoverable source: terminating B's actual backend stops C, returns
/// the already captured built-in/A work, and forces the max-one owned pool to
/// establish a replacement socket.
#[sqlx::test]
#[ignore]
async fn today_source_backend_termination_marks_current_and_remaining_sources_unavailable(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today hook termination",
        "today-hook-termination@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id, 0).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let sources = ordered_stage_sources(&app_pool, organization_id, viewer_id, stage).await;
    let a = sources[0].list.id;
    let b = sources[1].list.id;
    let c = sources[2].list.id;

    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    // A separate crm_app connection can signal a backend owned by the same
    // app role without expanding production/test database privileges.
    let control_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut owned_connection = one_app_pool.acquire().await.unwrap();
    let owned_pid = backend_pid(&mut owned_connection).await;
    let (captured_pid_tx, captured_pid_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(TerminateAfterMembership {
        source_id: b.as_uuid(),
        captured_pid: Mutex::new(Some(captured_pid_tx)),
        release: Mutex::new(Some(release_rx)),
    }));
    let query_scope = visibility_scope(organization_id);
    let query = tokio::spawn(async move {
        with_today_hooks(
            hooks,
            today::query_owned_at(
                owned_connection,
                &query_scope,
                UserId::new(viewer_id),
                Utc::now(),
            ),
        )
        .await
    });
    let captured_pid = captured_pid_rx
        .await
        .expect("B checkpoint captured its backend");
    assert_eq!(
        captured_pid, owned_pid,
        "the hook must target the transferred connection"
    );
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(captured_pid)
        .fetch_one(&control_app_pool)
        .await
        .unwrap();
    assert!(terminated, "control connection must terminate B's backend");
    let mut absent = false;
    for _ in 0..100 {
        let still_present: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)")
                .bind(captured_pid)
                .fetch_one(&control_app_pool)
                .await
                .unwrap();
        if !still_present {
            absent = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        absent,
        "terminated backend must disappear before source recovery runs"
    );
    release_tx.send(()).expect("release terminated source hook");
    let terminated = query.await.unwrap().unwrap();
    assert!(matches!(
        terminated.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(
        terminated
            .sources
            .issues
            .iter()
            .map(|issue| issue.list_id)
            .collect::<Vec<_>>(),
        vec![b, c],
        "B and the unattempted C source are explicitly unavailable"
    );
    assert!(terminated
        .sources
        .issues
        .iter()
        .all(|issue| matches!(issue.error, TodaySourceIssueError::Unavailable)));
    assert!(
        has_list_reason(item_for(&terminated.items, builtin_person), a),
        "A's completed work survives B's unrecoverable failure"
    );
    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    assert_ne!(
        backend_pid(&mut replacement_connection).await,
        owned_pid,
        "unrecoverable source cleanup must dispose the old owned connection"
    );
}

/// AC8 cleanup budget: a rollback checkpoint that never completes consumes the
/// real 100 ms combined recovery allowance, stops C, and disposes the socket.
#[sqlx::test]
#[ignore]
async fn today_source_recovery_checkpoint_exhaustion_uses_cleanup_grace_and_discards_connection(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today hook recovery",
        "today-hook-recovery@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage = stage_id(&app_pool, organization_id, 0).await;
    let builtin_person = insert_person(&app_pool, organization_id, stage, Some(viewer_id)).await;
    insert_inquiry(&app_pool, organization_id, builtin_person).await;
    let sources = ordered_stage_sources(&app_pool, organization_id, viewer_id, stage).await;
    let a = sources[0].list.id;
    let b = sources[1].list.id;
    let c = sources[2].list.id;

    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let mut owned_connection = one_app_pool.acquire().await.unwrap();
    let owned_pid = backend_pid(&mut owned_connection).await;
    let (recovery_started_tx, recovery_started_rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(ExhaustRecoveryBudget {
        source_id: b.as_uuid(),
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
        .expect("rollback checkpoint reached after B failure");
    let exhausted = tokio::time::timeout(Duration::from_secs(1), query)
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
        exhausted.sources.status,
        TodaySourcesStatus::Partial
    ));
    assert_eq!(
        exhausted
            .sources
            .issues
            .iter()
            .map(|issue| issue.list_id)
            .collect::<Vec<_>>(),
        vec![b, c]
    );
    assert!(
        has_list_reason(item_for(&exhausted.items, builtin_person), a),
        "A's complete source reason remains in the partial response"
    );
    let mut replacement_connection = one_app_pool.acquire().await.unwrap();
    assert_ne!(
        backend_pid(&mut replacement_connection).await,
        owned_pid,
        "expired rollback cleanup cannot return a poisoned connection to the pool"
    );
}

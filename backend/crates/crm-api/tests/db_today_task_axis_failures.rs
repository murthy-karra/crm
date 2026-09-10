//! Slice 016b §12.12: the built-in task axis's all-or-nothing failure
//! discipline (docs/specs/SLICE_016.md §5) — the SAME connection-recovery
//! mechanism as the call feed and a list source: actual SQL cancellation
//! via a tight `statement_timeout`, a savepoint rollback under the shared
//! 100 ms grace, and (only if that recovery itself fails) an unrecoverable
//! signal that skips all list-source work and detaches the connection.
//! Style per `db_today_system_feed_call_failures.rs`: task-local
//! `test_support` hooks around a real owned app connection, deterministic
//! checkpoints instead of timing races. Run only via ./scripts/check-db.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::task::{self, CreateTask, TaskKind};
use crm_api::domain::today::{self, SystemFeedIssueError, TodaySourcesStatus};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, UserId};
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

struct Fixture {
    org_id: Uuid,
    admin_id: Uuid,
    person_id: Uuid,
    /// Round-1 review must-close item 2: a person-state candidate (a stale,
    /// unanswered inquiry — no fresh-inquiry window, so it surfaces via
    /// `NoContactAttempt`, not `NewInquiry`) wholly unrelated to the task
    /// axis, present in every phase-failure fixture so the "person-state
    /// reasons byte-identical to a no-task baseline" claim (docs/specs/
    /// SLICE_016.md §12 item 12) has something real to compare, rather
    /// than holding vacuously because no such item existed.
    person_state_person_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let (org_id, admin_id) = crate::common::today_system_feed::create_org_with_admin(
        pool,
        "Task Axis Failures Co",
        "task-axis-failures-admin@example.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(pool).await;
    let stage_id = crate::common::today_system_feed::first_stage_id(&app_pool, org_id).await;
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) \
         VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    task::create_task(
        &app_pool,
        &Publisher::Disabled,
        &command_context(org_id, admin_id),
        CreateTask {
            person_id: PersonId::new(person_id),
            title: "Fixture task".to_string(),
            kind: TaskKind::FollowUp,
            due_at: Some(Utc::now()),
            assignee_user_id: Some(UserId::new(admin_id)),
        },
    )
    .await
    .unwrap();
    let person_state_person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(admin_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'task-axis-failures-fixture', $4)",
    )
    .bind(org_id)
    .bind(person_state_person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now() - chrono::Duration::days(3))
    .execute(&app_pool)
    .await
    .unwrap();
    Fixture {
        org_id,
        admin_id,
        person_id,
        person_state_person_id,
    }
}

fn find_item(items: &[today::TodayItem], person_id: Uuid) -> Option<&today::TodayItem> {
    items.iter().find(|i| i.person.id.as_uuid() == person_id)
}

fn issue_present(list: &today::TodayList, feed_key: &str) -> bool {
    list.sources
        .system_feed_issues
        .iter()
        .any(|i| i.feed_key == feed_key)
}

// --- Injected failures at each of the three axis phases -------------------

struct FailAtPhase(TodayQueryPhase);

impl TodayQueryHook for FailAtPhase {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if phase == self.0 && source_id.is_none() {
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

async fn assert_injected_failure_is_all_or_nothing(migrator_pool: PgPool, phase: TodayQueryPhase) {
    let f = fixture(&migrator_pool).await;
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let now = Utc::now();

    // Round-1 review must-close item 2: an unhooked control run, same
    // connection pool, same fixed clock, BEFORE the failure is injected —
    // this is the "no-task baseline" the person-state item's reasons must
    // match byte-for-byte once the task axis is made to fail. The task
    // axis never touches this Person (a different Person, filtered out by
    // `task_org_assignee_due_open_idx`'s own predicates), so an injected
    // failure inside the axis must leave it completely unperturbed.
    let baseline = today::query_owned_at(
        one_app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();
    let baseline_person_state_item = find_item(&baseline.items, f.person_state_person_id)
        .expect("baseline: person-state item present")
        .clone();
    assert!(
        !baseline_person_state_item.reasons.is_empty(),
        "the baseline control must be non-vacuous: the person-state item needs a real reason \
         for the byte-identical comparison below to mean anything"
    );

    let hooks = TodayQueryHooks::new(Arc::new(FailAtPhase(phase)));
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            now,
        ),
    )
    .await
    .unwrap();

    assert!(matches!(list.sources.status, TodaySourcesStatus::Partial));
    let partial_person_state_item = find_item(&list.items, f.person_state_person_id)
        .expect("partial: person-state item still present after the injected axis failure");
    assert_eq!(
        partial_person_state_item.reasons, baseline_person_state_item.reasons,
        "person-state reasons must be byte-identical to the no-task baseline (§12 item 12)"
    );
    assert_eq!(
        partial_person_state_item.priority, baseline_person_state_item.priority,
        "priority must be byte-identical to the no-task baseline too"
    );
    assert!(issue_present(&list, "task_due"));
    let task_issue = list
        .sources
        .system_feed_issues
        .iter()
        .find(|i| i.feed_key == "task_due")
        .unwrap();
    assert_eq!(task_issue.error, SystemFeedIssueError::Unavailable);
    assert!(!task_issue.fallback);
    assert!(
        list.items
            .iter()
            .all(|item| !item.reasons.iter().any(|r| matches!(
                r,
                today::TodayReason::TaskOverdue { .. } | today::TodayReason::TaskDue { .. }
            ))),
        "no task_* reason on any item after an injected axis failure"
    );
    assert!(
        list.items
            .iter()
            .all(|item| item.person.id.as_uuid() != f.person_id),
        "no task-only item after an injected axis failure"
    );

    // The connection recovered and is reusable: a fresh, unhooked read
    // from the pool observes the SAME task, now unaffected.
    let follow_up = today::query_owned_at(
        one_app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();
    assert!(matches!(
        follow_up.sources.status,
        TodaySourcesStatus::Complete
    ));
    assert!(follow_up
        .items
        .iter()
        .any(|item| item.person.id.as_uuid() == f.person_id));
}

#[sqlx::test]
#[ignore]
async fn failure_after_savepoint_is_all_or_nothing_partial(migrator_pool: PgPool) {
    assert_injected_failure_is_all_or_nothing(
        migrator_pool,
        TodayQueryPhase::TaskAxisAfterSavepoint,
    )
    .await;
}

#[sqlx::test]
#[ignore]
async fn failure_after_membership_is_all_or_nothing_partial(migrator_pool: PgPool) {
    assert_injected_failure_is_all_or_nothing(
        migrator_pool,
        TodayQueryPhase::TaskAxisAfterMembership,
    )
    .await;
}

#[sqlx::test]
#[ignore]
async fn failure_before_release_is_all_or_nothing_partial(migrator_pool: PgPool) {
    assert_injected_failure_is_all_or_nothing(
        migrator_pool,
        TodayQueryPhase::BeforeTaskAxisRelease,
    )
    .await;
}

// --- Exhausted recovery budget: the whole response is unavailable --------

struct ExhaustTaskAxisRecoveryBudget {
    recovery_started: Mutex<Option<oneshot::Sender<Instant>>>,
}

impl TodayQueryHook for ExhaustTaskAxisRecoveryBudget {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id.is_some() {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            TodayQueryPhase::TaskAxisAfterSavepoint => Box::pin(async move {
                sqlx::query("SELECT 1 / 0")
                    .execute(connection)
                    .await
                    .map(|_| ())
            }),
            TodayQueryPhase::RecoveryAfterRollback => {
                let sender = self.recovery_started.lock().unwrap().take();
                Box::pin(async move {
                    if let Some(sender) = sender {
                        sender
                            .send(Instant::now())
                            .expect("test observes recovery start");
                    }
                    std::future::pending::<Result<(), sqlx::Error>>().await
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

#[sqlx::test]
#[ignore]
async fn exhausted_recovery_budget_marks_the_whole_response_unavailable(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (tx, rx) = oneshot::channel();
    let hooks = TodayQueryHooks::new(Arc::new(ExhaustTaskAxisRecoveryBudget {
        recovery_started: Mutex::new(Some(tx)),
    }));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    rx.await.expect("recovery attempt started");
    assert!(matches!(
        list.sources.status,
        TodaySourcesStatus::Unavailable
    ));
    assert!(list.sources.issues.is_empty());
    // LATER batch (2026-09-10) item 7c (016b LATER, curated): the
    // `task_due` system-feed issue token is pushed before the recovery
    // attempt even begins (`domain/today/mod.rs`'s axis failure branch),
    // so it must survive into the Unavailable response too -- not just
    // the (necessarily empty, on this path) item set the assertions above
    // already cover.
    assert!(issue_present(&list, "task_due"));
    let task_issue = list
        .sources
        .system_feed_issues
        .iter()
        .find(|i| i.feed_key == "task_due")
        .unwrap();
    assert_eq!(task_issue.error, SystemFeedIssueError::Unavailable);
    assert!(!task_issue.fallback);
}

// --- An unrecoverable call feed returns before the axis ever runs --------

struct FailCallFeedForever;

impl TodayQueryHook for FailCallFeedForever {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
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
                Box::pin(std::future::pending::<Result<(), sqlx::Error>>())
            }
            TodayQueryPhase::TaskAxisAfterSavepoint
            | TodayQueryPhase::TaskAxisAfterMembership
            | TodayQueryPhase::BeforeTaskAxisRelease => {
                // The axis must never run at all once the call feed is
                // unrecoverable: fail loudly if it does.
                Box::pin(async {
                    Err(sqlx::Error::Decode(
                        "task axis ran after an unrecoverable call feed".into(),
                    ))
                })
            }
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

#[sqlx::test]
#[ignore]
async fn unrecoverable_call_feed_returns_before_the_axis_runs(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let hooks = TodayQueryHooks::new(Arc::new(FailCallFeedForever));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert!(matches!(
        list.sources.status,
        TodaySourcesStatus::Unavailable
    ));
    assert!(
        !issue_present(&list, "task_due"),
        "the axis never ran, so it contributes no issue of its own"
    );
}

// --- Both the call feed and the axis fail: two issues, recovers ----------

struct FailBothCallFeedAndAxis;

impl TodayQueryHook for FailBothCallFeedAndAxis {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id.is_some() {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            TodayQueryPhase::CallFeedAfterSavepoint | TodayQueryPhase::TaskAxisAfterSavepoint => {
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

#[sqlx::test]
#[ignore]
async fn call_feed_and_axis_both_failing_yield_two_issues_and_recover(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let hooks = TodayQueryHooks::new(Arc::new(FailBothCallFeedAndAxis));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let started = Instant::now();
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert!(matches!(list.sources.status, TodaySourcesStatus::Partial));
    assert_eq!(list.sources.system_feed_issues.len(), 2);
    assert!(issue_present(&list, "call_outcome_needed"));
    assert!(issue_present(&list, "task_due"));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "both recoveries must complete within their seeded deadlines"
    );
}

// --- The axis completes within its OWN budget after a slow call feed -----

struct SlowCallFeedThenAxis;

impl TodayQueryHook for SlowCallFeedThenAxis {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        _connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id.is_some() {
            return Box::pin(async { Ok(()) });
        }
        match phase {
            // Within the call feed's own 500 ms budget, leaving little of
            // it — proving the axis does NOT inherit whatever remained.
            TodayQueryPhase::CallFeedAfterMembership => Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                Ok(())
            }),
            // Comfortably within the axis's OWN fresh budget (500 ms) even
            // though 400 + 300 = 700 ms would overrun a SHARED one.
            TodayQueryPhase::TaskAxisAfterSavepoint => Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(())
            }),
            _ => Box::pin(async { Ok(()) }),
        }
    }
}

#[sqlx::test]
#[ignore]
async fn axis_completes_within_its_own_budget_after_a_slow_call_feed(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let hooks = TodayQueryHooks::new(Arc::new(SlowCallFeedThenAxis));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert!(matches!(list.sources.status, TodaySourcesStatus::Complete));
    assert!(
        list.items.iter().any(|item| item.person.id.as_uuid() == f.person_id
            && item.reasons.iter().any(|r| matches!(
                r,
                today::TodayReason::TaskOverdue { .. } | today::TodayReason::TaskDue { .. }
            ))),
        "the axis must still succeed on its own fresh budget, unstarved by the call feed's own delay"
    );
}

// --- Round-1 review must-close item 3: SET LOCAL restoration -------------
//
// Mirrors `db_today_system_feed_call_failures.rs`'s own T8 precedent
// (`set_local_settings_restore_after_call_only_failure`): proven on the
// SAME one-connection pool the query ran on, since a fresh acquire from a
// max-one pool can only return that same underlying connection.

async fn show_pg_settings(pool: &PgPool) -> (String, String, String) {
    let jit: String = sqlx::query_scalar("SHOW jit")
        .fetch_one(pool)
        .await
        .unwrap();
    let enable_mergejoin: String = sqlx::query_scalar("SHOW enable_mergejoin")
        .fetch_one(pool)
        .await
        .unwrap();
    let statement_timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(pool)
        .await
        .unwrap();
    (jit, enable_mergejoin, statement_timeout)
}

fn assert_default_settings(label: &str, settings: (String, String, String)) {
    let (jit, enable_mergejoin, statement_timeout) = settings;
    assert_eq!(
        jit, "on",
        "{label}: jit must be restored to the session default"
    );
    assert_eq!(
        enable_mergejoin, "on",
        "{label}: enable_mergejoin must be restored to the session default"
    );
    assert_eq!(
        statement_timeout, "0",
        "{label}: statement_timeout must be restored to the session default (no timeout)"
    );
}

#[sqlx::test]
#[ignore]
async fn set_local_settings_restore_after_a_recovered_task_axis_failure(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let hooks = TodayQueryHooks::new(Arc::new(FailAtPhase(
        TodayQueryPhase::TaskAxisAfterMembership,
    )));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let _partial = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();

    assert_default_settings(
        "recovered task-axis failure",
        show_pg_settings(&one_app_pool).await,
    );
}

// --- Round-1 review must-close item 4: real SQL cancellation -------------
//
// Mirrors `db_today_system_feed_call_failures.rs`'s own preview-timeout
// coverage gap fix (`SleepPastPreviewTimeout` /
// `preview_statement_timeout_is_unavailable_and_never_partial`): a
// genuinely slow statement (`pg_sleep(2)`) at a task-axis checkpoint, so
// PostgreSQL's own real `statement_timeout` cancels it (error 57014) —
// never a simulated `SELECT 1 / 0` failure standing in for what a real
// timeout looks like.

struct SleepPastTaskAxisTimeout;

impl TodayQueryHook for SleepPastTaskAxisTimeout {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        _deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a> {
        if source_id.is_some() {
            return Box::pin(async { Ok(()) });
        }
        if phase == TodayQueryPhase::TaskAxisAfterSavepoint {
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

#[sqlx::test]
#[ignore]
async fn task_axis_real_statement_timeout_cancellation_is_partial_and_recovers(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let hooks = TodayQueryHooks::new(Arc::new(SleepPastTaskAxisTimeout));
    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let started = Instant::now();
    let list = with_today_hooks(
        hooks,
        today::query_owned_at(
            one_app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(f.admin_id),
            Utc::now(),
        ),
    )
    .await
    .unwrap();
    let elapsed = started.elapsed();

    // The axis's own SOURCE_BUDGET statement_timeout must cancel the 2 s
    // sleep well before it completes — a real PostgreSQL 57014, recovered
    // exactly like the synthetic-failure phase tests above.
    assert!(
        elapsed < Duration::from_millis(1900),
        "the axis's own real statement_timeout must cancel the 2 s sleep, not wait it out: {elapsed:?}"
    );
    assert!(matches!(list.sources.status, TodaySourcesStatus::Partial));
    assert!(issue_present(&list, "task_due"));
    assert!(
        list.items
            .iter()
            .all(|item| !item.reasons.iter().any(|r| matches!(
                r,
                today::TodayReason::TaskOverdue { .. } | today::TodayReason::TaskDue { .. }
            ))),
        "no task_* reason on any item after a real statement-timeout cancellation"
    );

    // The connection recovered and is reusable on the SAME max-one pool.
    let follow_up = today::query_owned_at(
        one_app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(matches!(
        follow_up.sources.status,
        TodaySourcesStatus::Complete
    ));
    assert!(follow_up
        .items
        .iter()
        .any(|item| item.person.id.as_uuid() == f.person_id));
}

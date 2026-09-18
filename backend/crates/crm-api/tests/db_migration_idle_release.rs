//! Scheduler regression: empty queues must not acquire another connection for
//! release validation; a pending job with failed evidence is not silently idle.
use crm_api::{
    auth::workspace::ReleaseReadiness, domain::migration::release_work::ReleaseWork,
    realtime::Publisher, state::AppState,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

async fn state(migrator: &PgPool) -> (AppState, Arc<AtomicUsize>) {
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let count = acquisitions.clone();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .before_acquire(move |_, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(true) })
        })
        .connect_with((*migrator.connect_options()).clone())
        .await
        .unwrap();
    let mut state =
        AppState::for_tests(pool, &crate::common::test_config(), Publisher::recording());
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    (state, acquisitions)
}

#[sqlx::test]
#[ignore]
async fn empty_migration_queues_do_not_load_release_evidence(migrator: PgPool) {
    let (state, acquisitions) = state(&migrator).await;
    for work in [
        ReleaseWork::CoreChange,
        ReleaseWork::PeopleRefresh,
        ReleaseWork::PeopleAdmission,
        ReleaseWork::AdmittedPeopleRefresh,
        ReleaseWork::HistoryCapture,
        ReleaseWork::HistoryImport,
        ReleaseWork::AdmittedHistory,
        ReleaseWork::FamilyRefresh,
    ] {
        acquisitions.store(0, Ordering::SeqCst);
        assert!(
            state
                .migration_release_for_work(work)
                .await
                .unwrap()
                .is_none(),
            "{work:?}"
        );
        assert_eq!(
            acquisitions.load(Ordering::SeqCst),
            1,
            "{work:?} must only probe its queue"
        );
    }
    state.db.as_ref().unwrap().close().await;
    assert!(state
        .migration_release_for_work(ReleaseWork::PeopleRefresh)
        .await
        .is_err());
}

#[sqlx::test]
#[ignore]
async fn queued_work_checks_release_and_keeps_unavailable_distinct_from_idle(migrator: PgPool) {
    let (mut state, acquisitions) = state(&migrator).await;
    let pool = state.db.as_ref().unwrap().clone();
    // A connection-local candidate table isolates scheduler states without
    // manufacturing authorized domain jobs or weakening persistent DB guards.
    sqlx::raw_sql("CREATE TEMP TABLE migration_people_refresh (id uuid, organization_id uuid, lease_token uuid, state text, created_at timestamptz); INSERT INTO migration_people_refresh VALUES(gen_random_uuid(),gen_random_uuid(),NULL,'queued',now())")
        .execute(&pool).await.unwrap();
    acquisitions.store(0, Ordering::SeqCst);
    let work = state
        .migration_release_for_work(ReleaseWork::PeopleRefresh)
        .await
        .unwrap()
        .unwrap();
    // The deliberately incomplete shadow table makes compatibility validation
    // fail. The worker must still receive unavailable readiness and use its
    // normal pause/recovery rules, rather than treating this as an empty queue.
    assert!(work.readiness.is_none());
    assert_eq!(acquisitions.load(Ordering::SeqCst), 2);
    state.import_release = None;
    assert!(state
        .migration_release_for_work(ReleaseWork::PeopleRefresh)
        .await
        .unwrap()
        .unwrap()
        .readiness
        .is_none());
    sqlx::query("UPDATE migration_people_refresh SET state='completed'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(state
        .migration_release_for_work(ReleaseWork::PeopleRefresh)
        .await
        .unwrap()
        .is_none());
    // New work arriving after an idle observation is discovered next turn.
    sqlx::query("UPDATE migration_people_refresh SET state='queued'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(state
        .migration_release_for_work(ReleaseWork::PeopleRefresh)
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test]
#[ignore]
async fn family_scheduler_keeps_ready_plan_revocation_cleanup_eligible(migrator: PgPool) {
    let (state, _) = state(&migrator).await;
    let pool = state.db.as_ref().unwrap();
    sqlx::raw_sql("CREATE TEMP TABLE migration_family_refresh_plan (id uuid, state text); INSERT INTO migration_family_refresh_plan VALUES(gen_random_uuid(),'ready')")
        .execute(pool).await.unwrap();
    // Ready plans cannot execute, but still require executor-revocation cleanup.
    assert!(ReleaseWork::FamilyRefresh.is_pending(pool).await.unwrap());
    for terminal in ["paused", "completed", "cancelled", "expired"] {
        sqlx::query("UPDATE migration_family_refresh_plan SET state=$1")
            .bind(terminal)
            .execute(pool)
            .await
            .unwrap();
        assert!(!ReleaseWork::FamilyRefresh.is_pending(pool).await.unwrap());
    }
}

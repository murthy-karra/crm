//! A7 acceptance evidence through actual commands and worker transactions.
//! Test-only triggers stop/fail a Person result after its native writes. The
//! sequence is intentionally nontransactional evidence that those writes were
//! visible inside the failing/blocked transaction; no production seam is added.
use crate::import_support::Fixture;
use crm_api::domain::migration::{metadata, metadata_worker, MigrationError};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::Barrier;
use uuid::Uuid;

const PERSON_BARRIER: i64 = 10_151_700_001;

fn confirmation(ready: &Value, request: Uuid) -> metadata::ConfirmMetadataImport {
    serde_json::from_value(json!({"request_id":request,"plan_id":ready["latest_plan"]["id"],"plan_revision":ready["latest_plan"]["revision"],"confirmation_digest":ready["latest_plan"]["confirmation_digest"],"workspace_revision":ready["workspace_revision"],"acknowledgments":{"held_count":ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap()
}
async fn detail(f: &Fixture, child: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, child, &f.policy)
        .await
        .unwrap()
}
async fn drain(f: &Fixture) {
    for _ in 0..100 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("bounded synthetic child did not settle");
}
async fn confirm(f: &Fixture, child: Uuid, ready: &Value) {
    metadata::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        confirmation(ready, Uuid::new_v4()),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
}
async fn action(f: &Fixture, child: Uuid, request: Uuid, retry: bool) -> Value {
    metadata::action(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        metadata::ImportRequest {
            request_id: request,
        },
        retry,
        &f.policy,
    )
    .await
    .unwrap()
}
async fn native(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('tags',(SELECT count(*) FROM tag WHERE organization_id=$1),'fields',(SELECT count(*) FROM custom_field WHERE organization_id=$1),'links',(SELECT count(*) FROM person_tag WHERE organization_id=$1),'values',(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn ledger(f: &Fixture, child: Uuid) -> Value {
    // Deliberately exclude only lifecycle state/lease timestamps. Failed work
    // must leave every native-independent byte, cursor, count and receipt here
    // unchanged, including the original cancellation reservation.
    sqlx::query_scalar("SELECT jsonb_build_object('child_retained',i.retained_bytes,'child_reserved',i.reserved_bytes,'phase',i.phase,'checkpoint',i.checkpoint_id,'counts',i.counts,'snapshot_retained',s.retained_bytes,'snapshot_reserved',s.reserved_bytes,'org_retained',o.retained_bytes,'org_reserved',o.reserved_bytes,'results',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM migration_metadata_result r WHERE r.import_id=i.id AND r.organization_id=i.organization_id),'identities',(SELECT COALESCE(jsonb_agg(to_jsonb(x) ORDER BY x.kind,x.source_key),'[]') FROM migration_metadata_identity x WHERE x.import_id=i.id AND x.organization_id=i.organization_id),'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY c.actor_user_id,c.action,c.request_id),'[]') FROM migration_metadata_receipt c WHERE c.import_id=i.id AND c.organization_id=i.organization_id)) FROM migration_metadata_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id JOIN migration_snapshot_storage o ON o.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
        .bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn reach_person_phase(f: &Fixture, child: Uuid) {
    for _ in 0..10 {
        let phase: String = sqlx::query_scalar(
            "SELECT phase FROM migration_metadata_import WHERE id=$1 AND organization_id=$2",
        )
        .bind(child)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        if phase == "people" {
            assert_eq!(
                native(f).await,
                json!({"tags":1,"fields":1,"links":0,"values":0})
            );
            let results: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2")
                .bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
            assert_eq!(results, 2, "catalog settled, Person unit not started");
            return;
        }
        assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap());
    }
    panic!("small fixture did not reach Person phase");
}
async fn install_person_probe(migrator: &PgPool, fail: bool) {
    sqlx::query("CREATE SEQUENCE metadata_acceptance_person_reached")
        .execute(migrator)
        .await
        .unwrap();
    sqlx::query("GRANT USAGE, SELECT ON SEQUENCE metadata_acceptance_person_reached TO crm_app")
        .execute(migrator)
        .await
        .unwrap();
    let finish = if fail {
        "RAISE EXCEPTION USING ERRCODE='P0199',MESSAGE='synthetic_person_result_failure';"
            .to_owned()
    } else {
        format!("PERFORM pg_advisory_xact_lock({PERSON_BARRIER}::bigint);")
    };
    let function = format!("CREATE FUNCTION metadata_acceptance_person_probe() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='people' THEN IF (SELECT count(*) FROM person_tag WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id)<>1 OR (SELECT count(*) FROM person_custom_field_value WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id)<>1 THEN RAISE EXCEPTION USING ERRCODE='P0198',MESSAGE='synthetic_native_operations_not_reached'; END IF; PERFORM nextval('metadata_acceptance_person_reached'); {finish} END IF; RETURN NEW; END $$");
    sqlx::query(&function).execute(migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER metadata_acceptance_person_probe BEFORE INSERT ON migration_metadata_result FOR EACH ROW EXECUTE FUNCTION metadata_acceptance_person_probe()")
        .execute(migrator).await.unwrap();
}
async fn probe_reached(migrator: &PgPool) -> bool {
    sqlx::query_scalar("SELECT is_called AND last_value=1 FROM metadata_acceptance_person_reached")
        .fetch_one(migrator)
        .await
        .unwrap()
}
async fn blocked_by(migrator: &PgPool, blocker: i32) -> Option<i32> {
    tokio::time::timeout(Duration::from_millis(700), async {
        loop {
            let pid: Option<i32> = sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND $1=ANY(pg_blocking_pids(pid)) ORDER BY pid LIMIT 1")
                .bind(blocker).fetch_optional(migrator).await.unwrap();
            if pid.is_some() { return pid; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.ok().flatten()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_concurrency_confirm_cancel_race_has_one_terminal_receipt_outcome(
    migrator: PgPool,
) {
    let (f, parent, child, ready) = crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let calls = f.reader.calls();
    let parent_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM migration_import i WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let confirm_request = Uuid::new_v4();
    let cancel_request = Uuid::new_v4();
    let start = Arc::new(Barrier::new(2));
    // Both real commands start in the same wave, on distinct pool transactions.
    // The Organization/child row locks select their serialization order.
    let (confirmed, cancelled) = tokio::join!(
        async {
            start.wait().await;
            metadata::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                child,
                confirmation(&ready, confirm_request),
                &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
                &f.policy,
            )
            .await
        },
        async {
            start.wait().await;
            metadata::action(
                &f.pool,
                &f.key,
                &f.ctx,
                child,
                metadata::ImportRequest {
                    request_id: cancel_request,
                },
                false,
                &f.policy,
            )
            .await
        }
    );
    let cancelled = cancelled.unwrap();
    assert_eq!(cancelled["import"]["state"], "cancelled");
    assert_eq!(cancelled["import"]["reserved_bytes"], "0");
    let confirmation_replay = metadata::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        confirmation(&ready, confirm_request),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await;
    match confirmed {
        Ok(receipt) => assert_eq!(confirmation_replay.unwrap(), receipt),
        Err(MigrationError::ImportConflict) => assert!(matches!(
            confirmation_replay,
            Err(MigrationError::ImportConflict)
        )),
        Err(_) => panic!("unexpected confirmation race error"),
    }
    assert_eq!(action(&f, child, cancel_request, false).await, cancelled);
    drain(&f).await;
    assert_eq!(detail(&f, child).await["state"], "cancelled");
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":0,"links":0,"values":0})
    );
    let row:Value=sqlx::query_scalar("SELECT jsonb_build_object('results',(SELECT count(*) FROM migration_metadata_result WHERE import_id=$1),'identities',(SELECT count(*) FROM migration_metadata_identity WHERE import_id=$1),'reservations',(SELECT count(*) FROM migration_metadata_reservation WHERE import_id=$1),'cancel_receipts',(SELECT count(*) FROM migration_metadata_receipt WHERE import_id=$1 AND action='cancel'))")
        .bind(child).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        row,
        json!({"results":0,"identities":0,"reservations":0,"cancel_receipts":1})
    );
    let parent_after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM migration_import i WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(parent_before, parent_after);
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_concurrency_cancel_waits_for_complete_inflight_person_unit(migrator: PgPool) {
    let (f, _, child, ready) = crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let calls = f.reader.calls();
    confirm(&f, child, &ready).await;
    reach_person_phase(&f, child).await;
    install_person_probe(&migrator, false).await;
    let before = ledger(&f, child).await;
    let mut barrier = migrator.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *barrier)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PERSON_BARRIER)
        .execute(&mut *barrier)
        .await
        .unwrap();
    let (pool, key, policy) = (f.pool.clone(), f.key.clone(), f.policy.clone());
    let worker = tokio::spawn(async move { metadata_worker::run_once(&pool, &key, &policy).await });
    let worker_pid = blocked_by(&migrator, holder).await;
    let Some(worker_pid) = worker_pid else {
        barrier.rollback().await.unwrap();
        let _ = worker.await;
        panic!("actual Person worker did not reach database barrier");
    };
    // The trigger proves both native writes happened inside the unit; external
    // readers still see no partial writes or result/accounting changes.
    assert!(probe_reached(&migrator).await);
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"links":0,"values":0})
    );
    assert_eq!(ledger(&f, child).await, before);
    let cancel_request = Uuid::new_v4();
    let (pool, key, ctx, policy) = (
        f.pool.clone(),
        f.key.clone(),
        f.ctx.clone(),
        f.policy.clone(),
    );
    let cancel = tokio::spawn(async move {
        metadata::action(
            &pool,
            &key,
            &ctx,
            child,
            metadata::ImportRequest {
                request_id: cancel_request,
            },
            false,
            &policy,
        )
        .await
    });
    let cancel_pid = blocked_by(&migrator, worker_pid).await;
    let cancel_was_pending = !cancel.is_finished();
    // Always release before asserting the second observation; test failure must
    // not leave the deliberate blocker alive. Production lock_timeout stays2s.
    barrier.rollback().await.unwrap();
    assert!(worker.await.unwrap().unwrap());
    let cancelled = cancel.await.unwrap().unwrap();
    assert!(
        cancel_pid.is_some(),
        "cancel must wait on the actual worker's transaction"
    );
    assert!(cancel_was_pending);
    assert_eq!(cancelled["import"]["state"], "cancelled");
    assert_eq!(cancelled["import"]["reserved_bytes"], "0");
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"links":1,"values":1})
    );
    let results: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2",
    )
    .bind(child)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        results, 3,
        "two catalogs and exactly one complete Person result"
    );
    assert_eq!(cancelled["import"]["counts"]["people"]["settled"], "1");
    let committed = ledger(&f, child).await;
    assert_eq!(action(&f, child, cancel_request, false).await, cancelled);
    drain(&f).await;
    assert_eq!(ledger(&f, child).await, committed);
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_concurrency_person_failure_rolls_back_all_cells_result_cursor_and_bytes(
    migrator: PgPool,
) {
    let (f, _, child, ready) = crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let calls = f.reader.calls();
    confirm(&f, child, &ready).await;
    reach_person_phase(&f, child).await;
    let before = ledger(&f, child).await;
    install_person_probe(&migrator, true).await;
    assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    assert!(
        probe_reached(&migrator).await,
        "both native operations preceded the injected failure"
    );
    assert_eq!(detail(&f, child).await["state"], "paused");
    assert_eq!(
        detail(&f, child).await["pause_reason"],
        "storage_unavailable"
    );
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"links":0,"values":0})
    );
    assert_eq!(
        ledger(&f, child).await,
        before,
        "failed Person unit leaves exact pre-unit ledger and catalog results"
    );
    drain(&f).await;
    assert_eq!(
        ledger(&f, child).await,
        before,
        "recovery requires explicit retry"
    );
    sqlx::query("DROP TRIGGER metadata_acceptance_person_probe ON migration_metadata_result")
        .execute(&migrator)
        .await
        .unwrap();
    action(&f, child, Uuid::new_v4(), true).await;
    drain(&f).await;
    assert_eq!(detail(&f, child).await["state"], "completed");
    assert_eq!(detail(&f, child).await["reserved_bytes"], "0");
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"links":1,"values":1})
    );
    let outcomes:Value=sqlx::query_scalar("SELECT jsonb_build_object('person_results',(SELECT count(*) FROM migration_metadata_result WHERE import_id=$1 AND kind='people'),'catalog_results',(SELECT count(*) FROM migration_metadata_result WHERE import_id=$1 AND kind<>'people'),'identities',(SELECT count(*) FROM migration_metadata_identity WHERE import_id=$1),'reservations',(SELECT count(*) FROM migration_metadata_reservation WHERE import_id=$1))").bind(child).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        outcomes,
        json!({"person_results":1,"catalog_results":2,"identities":2,"reservations":0})
    );
    let committed = ledger(&f, child).await;
    drain(&f).await;
    assert_eq!(ledger(&f, child).await, committed);
    assert_eq!(f.reader.calls(), calls);
}

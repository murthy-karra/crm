//! Actual command/worker race evidence on a bounded retained synthetic Book.
//! Migrator-only probes expire a lease after native writes or block settlement.
//! They do not replace worker transactions, issue source requests, or introduce
//! a production test seam. PostgreSQL blocker observations prove overlap.
use crate::{
    db_activity_source as source,
    import_support::{self, Fixture},
};
use crm_api::{
    auth::workspace,
    domain::migration::{
        activity::{self, ActivityAction},
        activity_worker, imports, metadata, metadata_worker,
        snapshot::SnapshotPolicy,
        snapshot_source::Stream,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::Barrier;
use uuid::Uuid;

const RESULT_BARRIER: i64 = 10_162_900_001;

async fn fixture(migrator: &PgPool, count: u64) -> (Fixture, Uuid) {
    let book = source::book();
    book.set_records(Stream::TasksOpen, (1..=count).map(source::task).collect());
    book.set_records(Stream::People, vec![json!({"id":101,"firstName":"Synthetic race Person","stage":"Lead","assignedUserId":3,"tags":["Race Tag"],"customRace":"synthetic value"})]);
    book.set_records(
        Stream::CustomFields,
        vec![json!({"id":31,"name":"customRace","label":"Race Field","type":"text"})],
    );
    let f = import_support::fixture_with_book(migrator, book).await;
    let parent = source::completed_parent(&f).await;
    (f, parent)
}
async fn queued(migrator: &PgPool, count: u64) -> (Fixture, Uuid, Uuid) {
    let (f, parent) = fixture(migrator, count).await;
    let (child, ready) = source::prepare(&f, parent).await;
    let ready = source::replan(&f, child, &ready, source::choices(&f, child).await, None).await;
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        source::confirmation(&ready),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    (f, parent, child)
}
fn action(revision: &str, request: Uuid) -> ActivityAction {
    ActivityAction {
        request_id: request,
        expected_revision: revision.into(),
    }
}
async fn native(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('tasks',(SELECT count(*) FROM task WHERE organization_id=$1),'tags',(SELECT count(*) FROM tag WHERE organization_id=$1),'fields',(SELECT count(*) FROM custom_field WHERE organization_id=$1),'links',(SELECT count(*) FROM person_tag WHERE organization_id=$1),'values',(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn ledger(f: &Fixture, child: Uuid) -> Value {
    // Excludes only lifecycle/claim timestamps: all committed unit state, native
    // values, result bytes, identities and cancellation capacity remain exact.
    sqlx::query_scalar("SELECT jsonb_build_object('retained',i.retained_bytes,'measured',i.measured_bytes,'reserved',i.reserved_bytes,'native_bytes',i.native_bytes,'checkpoint',i.checkpoint_id,'counts',i.counts,'revision',i.revision,'activity_revision',i.activity_revision,'snapshot_retained',s.retained_bytes,'snapshot_reserved',s.reserved_bytes,'org_retained',o.retained_bytes,'org_reserved',o.reserved_bytes,'tasks',(SELECT COALESCE(jsonb_agg(to_jsonb(n) ORDER BY n.id),'[]') FROM task n WHERE n.organization_id=i.organization_id),'results',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM migration_activity_result r WHERE r.import_id=i.id AND r.organization_id=i.organization_id),'identities',(SELECT COALESCE(jsonb_agg(to_jsonb(d) ORDER BY d.kind,d.source_id),'[]') FROM migration_activity_identity d WHERE d.import_id=i.id AND d.organization_id=i.organization_id),'reservations',(SELECT COALESCE(jsonb_agg(to_jsonb(q) ORDER BY q.token),'[]') FROM migration_activity_reservation q WHERE q.import_id=i.id AND q.organization_id=i.organization_id)) FROM migration_activity_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id JOIN migration_snapshot_storage o ON o.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
        .bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn install_probe(migrator: &PgPool, expire: bool) {
    sqlx::query("CREATE SEQUENCE activity_race_result_reached")
        .execute(migrator)
        .await
        .unwrap();
    sqlx::query("GRANT USAGE,SELECT ON SEQUENCE activity_race_result_reached TO crm_app")
        .execute(migrator)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE activity_race_claims(token UUID PRIMARY KEY)")
        .execute(migrator)
        .await
        .unwrap();
    sqlx::query("GRANT INSERT,SELECT ON activity_race_claims TO crm_app")
        .execute(migrator)
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION activity_race_claim_probe() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.lease_token IS NOT NULL AND NEW.lease_token IS DISTINCT FROM OLD.lease_token THEN INSERT INTO activity_race_claims VALUES(NEW.lease_token); END IF; RETURN NEW; END $$").execute(migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER activity_race_claim_probe AFTER UPDATE OF lease_token ON migration_activity_import FOR EACH ROW EXECUTE FUNCTION activity_race_claim_probe()").execute(migrator).await.unwrap();
    let finish = if expire {
        "UPDATE migration_activity_import SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=NEW.import_id AND organization_id=NEW.organization_id;".into()
    } else {
        format!("PERFORM pg_advisory_xact_lock({RESULT_BARRIER}::bigint);")
    };
    let sql = format!("CREATE FUNCTION activity_race_result_probe() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.disposition<>'applied' OR NOT EXISTS(SELECT 1 FROM task WHERE id=NEW.target_id AND organization_id=NEW.organization_id) OR NOT EXISTS(SELECT 1 FROM migration_activity_identity WHERE target_id=NEW.target_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id) OR (SELECT count(*) FROM migration_activity_reservation WHERE import_id=NEW.import_id AND organization_id=NEW.organization_id AND purpose='work')<>1 THEN RAISE EXCEPTION USING ERRCODE='P0198',MESSAGE='synthetic_activity_unit_not_reached'; END IF; PERFORM nextval('activity_race_result_reached'); {finish} RETURN NEW; END $$");
    sqlx::query(&sql).execute(migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER activity_race_result_probe BEFORE INSERT ON migration_activity_result FOR EACH ROW EXECUTE FUNCTION activity_race_result_probe()").execute(migrator).await.unwrap();
}
async fn reached(migrator: &PgPool) -> bool {
    sqlx::query_scalar("SELECT is_called AND last_value=1 FROM activity_race_result_reached")
        .fetch_one(migrator)
        .await
        .unwrap()
}
async fn blocker(migrator: &PgPool, holder: i32) -> Option<i32> {
    tokio::time::timeout(Duration::from_millis(700), async {
        loop {
            let pid: Option<i32> = sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND $1=ANY(pg_blocking_pids(pid)) ORDER BY pid LIMIT 1")
                .bind(holder).fetch_optional(migrator).await.unwrap();
            if pid.is_some() { return pid; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.ok().flatten()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_race_expired_inflight_unit_rolls_back_and_real_workers_reclaim_once(
    migrator: PgPool,
) {
    let (f, parent, child) = queued(&migrator, 3).await;
    let parent_before = source::parent_state(&f, parent).await;
    let calls = f.reader.calls();
    let before = ledger(&f, child).await;
    install_probe(&migrator, true).await;
    assert!(matches!(
        activity_worker::run_once(&f.pool, &f.key, &f.policy).await,
        Err(MigrationError::ImportBusy)
    ));
    assert!(
        reached(&migrator).await,
        "native row, identity and reservation preceded lease expiry"
    );
    assert_eq!(
        ledger(&f, child).await,
        before,
        "the expired unit rolls back every write and byte"
    );
    let old: Uuid = sqlx::query_scalar(
        "SELECT lease_token FROM migration_activity_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(child)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    // The failed execution transaction rolled back its injected expiry. Expire
    // the persisted, real claim as an adversarial clock fixture for takeover.
    sqlx::query("DROP TRIGGER activity_race_result_probe ON migration_activity_result")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_activity_import SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1 AND organization_id=$2").bind(child).bind(f.org).execute(&migrator).await.unwrap();
    let (a, b) = tokio::join!(
        activity_worker::run_once(&f.pool, &f.key, &f.policy),
        activity_worker::run_once(&f.pool, &f.key, &f.policy)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(a || b);
    source::drain(&f).await;
    assert_eq!(source::ready(&f, child).await["state"], "completed");
    let evidence: Value = sqlx::query_scalar("SELECT jsonb_build_object('tasks',(SELECT count(*) FROM task WHERE organization_id=$1),'results',(SELECT count(*) FROM migration_activity_result WHERE import_id=$2 AND organization_id=$1),'identities',(SELECT count(*) FROM migration_activity_identity WHERE import_id=$2 AND organization_id=$1),'reservations',(SELECT count(*) FROM migration_activity_reservation WHERE import_id=$2 AND organization_id=$1),'old_claims',(SELECT count(*) FROM activity_race_claims WHERE token=$3),'replacement_claims',(SELECT count(*) FROM activity_race_claims WHERE token<>$3))").bind(f.org).bind(child).bind(old).fetch_one(&f.pool).await.unwrap();
    assert_eq!(evidence["tasks"], 3);
    assert_eq!(evidence["results"], 3);
    assert_eq!(evidence["identities"], 3);
    assert_eq!(evidence["reservations"], 0);
    assert_eq!(evidence["old_claims"], 1);
    assert!(evidence["replacement_claims"].as_i64().unwrap() >= 1);
    let final_ledger = ledger(&f, child).await;
    assert_eq!(final_ledger["retained"], final_ledger["measured"]);
    source::drain(&f).await;
    assert_eq!(ledger(&f, child).await, final_ledger);
    assert_eq!(source::parent_state(&f, parent).await, parent_before);
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_race_cancel_waits_for_inflight_unit_then_fences_all_later_units(
    migrator: PgPool,
) {
    let (f, parent, child) = queued(&migrator, 3).await;
    let parent_before = source::parent_state(&f, parent).await;
    let calls = f.reader.calls();
    let ready = source::ready(&f, child).await;
    let before = ledger(&f, child).await;
    install_probe(&migrator, false).await;
    let mut gate = migrator.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *gate)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(RESULT_BARRIER)
        .execute(&mut *gate)
        .await
        .unwrap();
    let (pool, key, policy) = (f.pool.clone(), f.key.clone(), f.policy.clone());
    let worker = tokio::spawn(async move { activity_worker::run_once(&pool, &key, &policy).await });
    let Some(worker_pid) = blocker(&migrator, holder).await else {
        gate.rollback().await.unwrap();
        let _ = worker.await;
        panic!("real activity worker did not reach settlement barrier");
    };
    assert!(reached(&migrator).await);
    assert_eq!(
        ledger(&f, child).await,
        before,
        "external readers cannot see a partial unit"
    );
    let stale = action(ready["revision"].as_str().unwrap(), Uuid::new_v4());
    let (pool, key, ctx, policy) = (
        f.pool.clone(),
        f.key.clone(),
        f.ctx.clone(),
        f.policy.clone(),
    );
    let cancel = tokio::spawn(async move {
        activity::action(&pool, &key, &ctx, child, stale, false, &policy).await
    });
    let cancel_pid = blocker(&migrator, worker_pid).await;
    let pending = !cancel.is_finished();
    gate.rollback().await.unwrap();
    assert!(worker.await.unwrap().unwrap());
    let cancelled = cancel.await.unwrap();
    assert!(
        cancel_pid.is_some() && pending,
        "typed cancel waits on the actual worker transaction"
    );
    assert!(
        matches!(cancelled, Err(MigrationError::ImportConflict)),
        "settled unit advanced the reviewed revision"
    );
    let current = source::ready(&f, child).await;
    let request = Uuid::new_v4();
    let revision = current["revision"].as_str().unwrap();
    let receipt = activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        action(revision, request),
        false,
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(receipt["import"]["state"], "cancelled");
    assert_eq!(receipt["import"]["reserved_bytes"], "0");
    assert_eq!(native(&f).await["tasks"], 1);
    let committed = ledger(&f, child).await;
    assert_eq!(committed["results"].as_array().unwrap().len(), 1);
    assert_eq!(committed["identities"].as_array().unwrap().len(), 1);
    assert_eq!(
        activity::action(
            &f.pool,
            &f.key,
            &f.ctx,
            child,
            action(revision, request),
            false,
            &f.policy
        )
        .await
        .unwrap(),
        receipt
    );
    source::drain(&f).await;
    let (a, b) = tokio::join!(
        activity_worker::run_once(&f.pool, &f.key, &f.policy),
        activity_worker::run_once(&f.pool, &f.key, &f.policy)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(!a && !b);
    assert_eq!(ledger(&f, child).await, committed);
    assert_eq!(source::parent_state(&f, parent).await, parent_before);
    assert_eq!(f.reader.calls(), calls);
}

async fn totals(f: &Fixture) -> Value {
    let v:Value=sqlx::query_scalar("SELECT jsonb_build_object('run_retained',s.retained_bytes,'run_reserved',s.reserved_bytes,'org_retained',o.retained_bytes,'org_reserved',o.reserved_bytes,'activity_retained',(SELECT COALESCE(sum(retained_bytes),0) FROM migration_activity_import WHERE organization_id=$1),'metadata_retained',(SELECT COALESCE(sum(retained_bytes),0) FROM migration_metadata_import WHERE organization_id=$1),'activity_reserved',(SELECT COALESCE(sum(reserved_bytes),0) FROM migration_activity_import WHERE organization_id=$1),'metadata_reserved',(SELECT COALESCE(sum(reserved_bytes),0) FROM migration_metadata_import WHERE organization_id=$1),'activity_reservation_sum',(SELECT COALESCE(sum(byte_count),0) FROM migration_activity_reservation WHERE organization_id=$1),'metadata_reservation_sum',(SELECT COALESCE(sum(byte_count),0) FROM migration_metadata_reservation WHERE organization_id=$1),'work_tokens',(SELECT count(*) FROM migration_activity_reservation WHERE organization_id=$1 AND purpose='work')+(SELECT count(*) FROM migration_metadata_reservation WHERE organization_id=$1 AND purpose='work')) FROM migration_snapshot s JOIN migration_snapshot_storage o ON o.organization_id=s.organization_id WHERE s.organization_id=$1 AND s.id=$2").bind(f.org).bind(f.snapshot).fetch_one(&f.pool).await.unwrap();
    assert_eq!(v["activity_reserved"], v["activity_reservation_sum"]);
    assert_eq!(v["metadata_reserved"], v["metadata_reservation_sum"]);
    let child_reserved =
        v["activity_reserved"].as_i64().unwrap() + v["metadata_reserved"].as_i64().unwrap();
    assert_eq!(v["run_reserved"].as_i64(), Some(child_reserved));
    assert_eq!(v["org_reserved"].as_i64(), Some(child_reserved));
    assert_eq!(
        v["work_tokens"], 0,
        "completed transactions leave only cancellation reservations"
    );
    v
}
async fn both_drain(f: &Fixture) {
    for _ in 0..100 {
        let (a, b) = tokio::join!(
            activity_worker::run_once(&f.pool, &f.key, &f.policy),
            metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        totals(f).await;
        if !a && !b {
            return;
        }
    }
    panic!("bounded sibling workers did not settle");
}
async fn metadata_ready(f: &Fixture, child: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, child, &f.policy)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_race_sibling_commands_cannot_oversubscribe_run_or_org_and_settle_owned_bytes(
    migrator: PgPool,
) {
    // Both budgets get an independent real fixture. 96KiB can hold one complete
    // proposal and its 64KiB cancel reservation, but never two reservations.
    for limiting in ["run", "organization"] {
        let (f, parent) = fixture(&migrator, 1).await;
        let calls = f.reader.calls();
        let parent_before = source::parent_state(&f, parent).await;
        let initial = totals(&f).await;
        let mut lower: SnapshotPolicy = f.policy.clone();
        let base = initial[if limiting == "run" {
            "run_retained"
        } else {
            "org_retained"
        }]
        .as_i64()
        .unwrap();
        let ceiling = base + 96 * 1024;
        if limiting == "run" {
            lower.run_ceiling_bytes = ceiling;
        } else {
            lower.org_ceiling_bytes = ceiling;
        }
        const { assert!(96 * 1024 < 2 * imports::CANCEL_RESERVATION) };
        let activity_request = Uuid::new_v4();
        let metadata_request = Uuid::new_v4();
        let barrier = Arc::new(Barrier::new(2));
        let (a, m) = tokio::join!(
            async {
                barrier.wait().await;
                activity::prepare(
                    &f.pool,
                    &f.key,
                    &f.ctx,
                    activity::PrepareActivityImport {
                        request_id: activity_request,
                        parent_import_id: parent,
                    },
                    &lower,
                )
                .await
            },
            async {
                barrier.wait().await;
                metadata::propose(
                    &f.pool,
                    &f.key,
                    &f.ctx,
                    metadata::PlanMetadataImport {
                        request_id: metadata_request,
                        parent_import_id: parent,
                    },
                    &lower,
                )
                .await
            }
        );
        assert_eq!(
            usize::from(a.is_ok()) + usize::from(m.is_ok()),
            1,
            "only one child fits the actual shared cap"
        );
        assert!(a.is_ok() || matches!(a, Err(MigrationError::StorageLimit)));
        assert!(m.is_ok() || matches!(m, Err(MigrationError::StorageLimit)));
        let admitted = totals(&f).await;
        assert!(
            admitted[if limiting == "run" {
                "run_retained"
            } else {
                "org_retained"
            }]
            .as_i64()
            .unwrap()
                + admitted[if limiting == "run" {
                    "run_reserved"
                } else {
                    "org_reserved"
                }]
                .as_i64()
                .unwrap()
                <= ceiling
        );
        assert_eq!(admitted["run_reserved"], imports::CANCEL_RESERVATION);
        let children:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM migration_activity_import WHERE organization_id=$1)+(SELECT count(*) FROM migration_metadata_import WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(
            children, 1,
            "failed admission leaves no orphan child or receipt"
        );
        // Same request IDs replay the winner and retry the transaction that
        // rolled back, after restoring the fixture's ordinary policy ceiling.
        let a = activity::prepare(
            &f.pool,
            &f.key,
            &f.ctx,
            activity::PrepareActivityImport {
                request_id: activity_request,
                parent_import_id: parent,
            },
            &f.policy,
        )
        .await
        .unwrap();
        let m = metadata::propose(
            &f.pool,
            &f.key,
            &f.ctx,
            metadata::PlanMetadataImport {
                request_id: metadata_request,
                parent_import_id: parent,
            },
            &f.policy,
        )
        .await
        .unwrap();
        let child = Uuid::parse_str(a["import"]["id"].as_str().unwrap()).unwrap();
        let sibling = Uuid::parse_str(m["import"]["id"].as_str().unwrap()).unwrap();
        both_drain(&f).await;
        let ready = source::ready(&f, child).await;
        let ready = source::replan(&f, child, &ready, source::choices(&f, child).await, None).await;
        let metadata = metadata_ready(&f, sibling).await;
        let plan = Uuid::parse_str(metadata["latest_plan"]["id"].as_str().unwrap()).unwrap();
        let mappings:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 ORDER BY id").bind(plan).bind(f.org).fetch_all(&f.pool).await.unwrap();
        assert_eq!(mappings.len(), 2);
        metadata::replan(&f.pool,&f.key,&f.ctx,sibling,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":metadata["latest_plan"]["revision"],"mappings":mappings.iter().map(|id|json!({"mapping_id":id,"choice":{"kind":"create_matching"}})).collect::<Vec<_>>()})).unwrap(),&f.policy).await.unwrap();
        both_drain(&f).await;
        let metadata = metadata_ready(&f, sibling).await;
        activity::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            child,
            source::confirmation(&ready),
            &workspace::ReleaseReadiness::for_tests(),
            &f.policy,
        )
        .await
        .unwrap();
        metadata::confirm(&f.pool,&f.key,&f.ctx,sibling,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":metadata["latest_plan"]["id"],"plan_revision":metadata["latest_plan"]["revision"],"confirmation_digest":metadata["latest_plan"]["confirmation_digest"],"workspace_revision":metadata["workspace_revision"],"acknowledgments":{"held_count":metadata["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap(),&workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
        for _ in 0..10 {
            let phase: String = sqlx::query_scalar(
                "SELECT phase FROM migration_metadata_import WHERE id=$1 AND organization_id=$2",
            )
            .bind(sibling)
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
            if phase == "people" {
                break;
            }
            assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
                .await
                .unwrap());
        }
        assert_eq!(
            native(&f).await,
            json!({"tasks":0,"tags":1,"fields":1,"links":0,"values":0})
        );
        let before = totals(&f).await;
        let (a, m) = tokio::join!(
            activity_worker::run_once(&f.pool, &f.key, &f.policy),
            metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        );
        let (a, m) = (a.unwrap(), m.unwrap());
        assert!(a && m);
        let after = totals(&f).await;
        let child_delta = after["activity_retained"].as_i64().unwrap()
            - before["activity_retained"].as_i64().unwrap()
            + after["metadata_retained"].as_i64().unwrap()
            - before["metadata_retained"].as_i64().unwrap();
        assert!(child_delta > 0);
        assert_eq!(
            after["run_retained"].as_i64().unwrap() - before["run_retained"].as_i64().unwrap(),
            child_delta
        );
        assert_eq!(
            after["org_retained"].as_i64().unwrap() - before["org_retained"].as_i64().unwrap(),
            child_delta
        );
        assert_eq!(
            native(&f).await,
            json!({"tasks":1,"tags":1,"fields":1,"links":1,"values":1})
        );
        let activity_before:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM migration_activity_import i WHERE id=$1 AND organization_id=$2").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
        metadata::action(
            &f.pool,
            &f.key,
            &f.ctx,
            sibling,
            metadata::ImportRequest {
                request_id: Uuid::new_v4(),
            },
            false,
            &f.policy,
        )
        .await
        .unwrap();
        let activity_after:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM migration_activity_import i WHERE id=$1 AND organization_id=$2").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(
            activity_after, activity_before,
            "sibling cancellation never consumes activity-owned capacity"
        );
        let after_cancel = totals(&f).await;
        assert_eq!(after_cancel["metadata_reserved"], 0);
        assert_eq!(
            after_cancel["activity_reserved"],
            imports::CANCEL_RESERVATION
        );
        both_drain(&f).await;
        assert_eq!(source::ready(&f, child).await["state"], "completed");
        assert_eq!(metadata_ready(&f, sibling).await["state"], "cancelled");
        assert_eq!(totals(&f).await["org_reserved"], 0);
        assert_eq!(source::parent_state(&f, parent).await, parent_before);
        assert_eq!(f.reader.calls(), calls);
    }
}

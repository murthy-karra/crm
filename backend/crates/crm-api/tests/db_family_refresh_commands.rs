//! Real typed preparation admission. No refresh confirmation or native execution.
use crate::{
    db_activity_source, db_history_capture_support as capture,
    db_history_import_support as history, db_people_admission_execution as admission,
    import_support,
};
use crm_api::{
    domain::migration::{
        family_refresh::{
            commands::{self, PrepareFamilyRefresh},
            evidence::{Purpose, Scope},
            model::Family,
            preparation_worker::{self as worker, Progress},
        },
        MigrationError,
    },
    ids::{OrganizationId, UserId},
};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_combined_is_atomic_metered_and_replay_safe(pool: PgPool) {
    let (f, parent, _, book) = history::fixture(&pool).await;
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let (history, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, history).await;
    capture::drain(&f, &book).await;
    let request = Uuid::new_v4();
    let input = || PrepareFamilyRefresh {
        request_id: request,
        parent_import_id: parent,
        core_report_id: Some(report),
        history_capture_id: Some(history),
        families: vec![Family::History, Family::Activity, Family::Metadata],
    };
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &tiny, &f.ctx, input()).await,
        Err(MigrationError::StorageLimit)
    ));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_bundle WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "failed admission leaves no bundle or receipt");
    let prepared = commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, input())
        .await
        .unwrap();
    assert_eq!(prepared.families.len(), 3);
    assert_eq!(prepared.families[0].family, Family::Metadata);
    assert_eq!(prepared.state, "preparing");
    assert_eq!(prepared.revision, "1");
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: prepared.bundle_id,
        plan: prepared.families[0].plan_id,
        family: Family::Metadata,
        revision: 1,
    };
    let b = sqlx::query("SELECT * FROM migration_family_refresh_bundle WHERE id=$1")
        .bind(prepared.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let binding: serde_json::Value = scope
        .open(
            &f.key,
            prepared.bundle_id,
            Purpose::Binding,
            b.get("source_nonce"),
            b.get("source_ciphertext"),
        )
        .unwrap();
    assert_eq!(binding["history"]["capture_id"], json!(history));
    assert_eq!(binding["core_report_id"], json!(report));
    let rows=sqlx::query("SELECT measured_bytes,retained_bytes,reserved_bytes,phase FROM migration_family_refresh_plan WHERE bundle_id=$1").bind(prepared.bundle_id).fetch_all(&pool).await.unwrap();
    for row in rows {
        assert_eq!(
            row.get::<i64, _>("retained_bytes"),
            row.get::<i64, _>("measured_bytes")
        );
        assert_eq!(row.get::<i64, _>("reserved_bytes"), 8192);
        assert_eq!(row.get::<String, _>("phase"), "cohort");
    }
    assert_eq!(
        b.get::<i64, _>("shared_retained_bytes"),
        b.get::<i64, _>("shared_measured_bytes")
    );
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    let replay = commands::prepare(&f.pool, &f.key, &tiny, &f.ctx, input())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let mut changed = input();
    changed.families.swap(0, 1);
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, changed).await,
        Err(MigrationError::Conflict)
    ));
    let mut second = input();
    second.request_id = Uuid::new_v4();
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, second).await,
        Err(MigrationError::Conflict)
    ));
    let mut stranger = f.ctx.clone();
    stranger.actor_user_id = UserId::new(Uuid::new_v4());
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &stranger, input()).await,
        Err(MigrationError::Forbidden)
    ));
    stranger = f.ctx.clone();
    stranger.organization_id = OrganizationId::new(Uuid::new_v4());
    assert!(
        commands::prepare(&f.pool, &f.key, &f.policy, &stranger, input())
            .await
            .is_err()
    );
    let installed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_requirement WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        installed, 0,
        "preparation does not activate native refresh capability"
    );
    let first = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(first.plan, prepared.families[0].plan_id);
    assert!(worker::claim_next(&f.pool).await.unwrap().is_none());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(first.token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '100 milliseconds' WHERE id=$1")
        .bind(first.plan).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let successor = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(successor.plan, first.plan);
    assert_eq!(successor.epoch, first.epoch + 1);
    assert_ne!(successor.token, first.token);
    assert!(!worker::release(&f.pool, &first).await.unwrap());
    assert!(worker::release(&f.pool, &successor).await.unwrap());
    let mut idle = false;
    for _ in 0..100 {
        match worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() {
            Progress::Advanced => {}
            Progress::Idle => {
                idle = true;
                break;
            }
            Progress::Paused => panic!("valid retained sources should advance"),
        }
    }
    assert!(idle, "bounded preparation should exhaust runnable phases");
    let plans=sqlx::query("SELECT phase,lease_token,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE bundle_id=$1")
        .bind(prepared.bundle_id).fetch_all(&pool).await.unwrap();
    for plan in plans {
        assert_eq!(plan.get::<String, _>("phase"), "mappings");
        assert!(plan.get::<Option<Uuid>, _>("lease_token").is_none());
        assert_eq!(
            plan.get::<i64, _>("measured_bytes"),
            plan.get::<i64, _>("retained_bytes")
        );
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_history_only_uses_history_payer(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    assert_eq!(prepared.families.len(), 1);
    let row=sqlx::query("SELECT b.payer_plan_id,b.core_snapshot_id,p.history_capture_id,p.retained_bytes,p.reserved_bytes FROM migration_family_refresh_bundle b JOIN migration_family_refresh_plan p ON p.bundle_id=b.id WHERE b.id=$1").bind(prepared.bundle_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        row.get::<Uuid, _>("payer_plan_id"),
        prepared.families[0].plan_id
    );
    assert!(row.get::<Option<Uuid>, _>("core_snapshot_id").is_none());
    assert_eq!(row.get::<Uuid, _>("history_capture_id"), history);
    assert_eq!(row.get::<i64, _>("reserved_bytes"), 8192);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_rejects_invalid_selection_and_foreign_evidence(pool: PgPool) {
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let input = || PrepareFamilyRefresh {
        request_id: Uuid::new_v4(),
        parent_import_id: parent,
        core_report_id: Some(Uuid::new_v4()),
        history_capture_id: None,
        families: vec![Family::Activity],
    };
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, input()).await,
        Err(MigrationError::SourceNotEligible)
    ));
    let other = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let other_parent = db_activity_source::completed_parent(&other).await;
    let foreign_report = admission::report(
        &other,
        other_parent,
        vec![json!({"id":101,"firstName":"Foreign","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let mut foreign = input();
    foreign.core_report_id = Some(foreign_report);
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, foreign).await,
        Err(MigrationError::SourceNotEligible)
    ));
    for families in [
        vec![],
        vec![Family::Activity, Family::Activity],
        vec![Family::History],
    ] {
        let mut cmd = input();
        cmd.families = families;
        assert!(matches!(
            commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, cmd).await,
            Err(MigrationError::InvalidInput)
        ));
    }
    let mut json = serde_json::to_value(input()).unwrap();
    json["organization_id"] = json!(f.org);
    assert!(serde_json::from_value::<PrepareFamilyRefresh>(json).is_err());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_worker_pauses_without_losing_control_capacity(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &tiny).await.unwrap(),
        Progress::Paused
    );
    let row = sqlx::query("SELECT state,pause_reason,lease_token,measured_bytes,retained_bytes,reserved_bytes FROM migration_family_refresh_plan WHERE id=$1")
        .bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "paused");
    assert_eq!(row.get::<String, _>("pause_reason"), "storage_limit");
    assert!(row.get::<Option<Uuid>, _>("lease_token").is_none());
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes")
    );
    assert!(row.get::<i64, _>("reserved_bytes") > 8000);
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Idle
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_worker_authenticates_before_preparation(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let wrong_key = crm_api::config::RawPayloadKey::new([0x42; 32]);
    assert_eq!(
        worker::run_once(&f.pool, &wrong_key, &f.policy)
            .await
            .unwrap(),
        Progress::Paused
    );
    let row = sqlx::query("SELECT state,pause_reason,checkpoint,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1")
        .bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "paused");
    assert_eq!(
        row.get::<String, _>("pause_reason"),
        "retained_integrity_failed"
    );
    assert_eq!(row.get::<i64, _>("checkpoint"), 0);
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Idle
    );
}

//! H3 TRUST/CONTRACT: retained-source authority and durable reader/write fences.
//! Every successful root/anchor is produced by the real synthetic lifecycle.
use crate::{
    db_admitted_history as admitted, db_history_capture_support as capture,
    db_history_import_support as original, import_support::Fixture,
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_history as h, history_capture_source::Stream, MigrationError},
    ids::UserId,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().expect("UUID string")).unwrap()
}

fn database_message(error: &sqlx::Error) -> (String, String) {
    let database = error.as_database_error().expect("database error");
    (
        database.code().as_deref().unwrap_or_default().to_owned(),
        database.message().to_owned(),
    )
}

async fn shared_probe(
    f: &Fixture,
    original_stamp: bool,
    admitted_stamp: bool,
) -> Result<(), sqlx::Error> {
    let mut tx = f.pool.begin().await?;
    if original_stamp {
        sqlx::query("SELECT set_config('crm.history_reader','fub-history-timeline-v1',true)")
            .execute(&mut *tx)
            .await?;
    }
    if admitted_stamp {
        sqlx::query(
            "SELECT set_config('crm.admitted_history_reader','fub-admitted-history-v1',true)",
        )
        .execute(&mut *tx)
        .await?;
    }
    let result = sqlx::query("SELECT crm_workspace_shared($1)")
        .bind(f.org)
        .execute(&mut *tx)
        .await
        .map(|_| ());
    tx.rollback().await?;
    result
}

async fn install_original_anchor(f: &Fixture, parent: Uuid) -> Uuid {
    let book = capture::HistoryBook::new();
    book.set_records(
        Stream::Events,
        vec![json!({
            "id": 701,
            "personId": 101,
            "type": "Registration",
            "created": "2026-02-01T00:00:00Z"
        })],
    );
    let (capture_id, _) = capture::propose(f, parent).await;
    capture::confirm(f, capture_id).await;
    capture::drain(f, &book).await;
    assert_eq!(
        capture::ready(f, capture_id).await["state"],
        "completed_with_gaps"
    );
    let run = original::ready(f, parent, capture_id).await;
    original::confirm(f, run).await;
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=$1 AND parent_import_id=$2)"
    )
    .bind(f.org)
    .bind(parent)
    .fetch_one(&f.pool)
    .await
    .unwrap());
    run
}

async fn confirm_without_work(
    f: &Fixture,
    admission_id: Uuid,
    capture_id: Uuid,
) -> (Uuid, Uuid, Value) {
    let root = admitted::ready(f, admission_id, capture_id).await;
    let before = h::get(&f.pool, &f.ctx, root).await.unwrap();
    let plan = uuid(&before["latest_plan_id"]);
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            expected_revision: before["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let confirmed = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(confirmed["state"], "queued");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM fub_event_record_imported WHERE organization_id=$1 AND admitted_root_id=$2"
        )
        .bind(f.org)
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "confirmation installs the boundary before any worker fact"
    );
    (root, plan, confirmed)
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn independent_reader_stamps_and_cancelled_zero_write_root_remain_durable(migrator: PgPool) {
    let (f, admission_id, capture_id) = admitted::fixture(&migrator).await;
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_people_admission WHERE id=$1 AND organization_id=$2",
    )
    .bind(admission_id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let original_run = install_original_anchor(&f, parent).await;
    let claimed_lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_history_import_run SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '1 hour' WHERE id=$1 AND organization_id=$2")
        .bind(original_run)
        .bind(f.org)
        .bind(claimed_lease)
        .execute(&migrator)
        .await
        .unwrap();
    let (root, _, confirmed) = confirm_without_work(&f, admission_id, capture_id).await;

    let old_only = shared_probe(&f, true, false).await.unwrap_err();
    assert_eq!(
        database_message(&old_only),
        (
            "P010H".to_owned(),
            "admitted_history_reader_required".to_owned()
        )
    );
    let new_only = shared_probe(&f, false, true).await.unwrap_err();
    assert_eq!(
        database_message(&new_only),
        ("P010H".to_owned(), "history_reader_required".to_owned())
    );
    shared_probe(&f, true, true).await.unwrap();

    let before: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT r.retained_bytes,s.retained_bytes,(SELECT count(*) FROM migration_history_import_receipt x WHERE x.organization_id=r.organization_id AND x.owner_run_id=r.id),md5(to_jsonb(r)::text) FROM migration_history_import_run r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2",
    )
    .bind(original_run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let request_id = Uuid::new_v4();
    let digest = vec![0_u8; 32];
    let nonce = vec![0_u8; 24];
    let ciphertext = vec![1_u8];
    let mut old_unit = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.history_reader','fub-history-timeline-v1',true)")
        .execute(&mut *old_unit)
        .await
        .unwrap();
    let rejected = sqlx::query("INSERT INTO migration_history_import_receipt(organization_id,request_id,owner_run_id,actor_user_id,operation,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,'resume',$5,$6,$7)")
        .bind(f.org)
        .bind(request_id)
        .bind(original_run)
        .bind(f.actor)
        .bind(&digest)
        .bind(&nonce)
        .bind(&ciphertext)
        .execute(&mut *old_unit)
        .await
        .unwrap_err();
    assert_eq!(
        database_message(&rejected),
        (
            "P010H".to_owned(),
            "admitted_history_reader_required".to_owned()
        ),
        "an original unit claimed by old code is fenced on its actual write connection"
    );
    old_unit.rollback().await.unwrap();
    let after: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT r.retained_bytes,s.retained_bytes,(SELECT count(*) FROM migration_history_import_receipt x WHERE x.organization_id=r.organization_id AND x.owner_run_id=r.id),md5(to_jsonb(r)::text) FROM migration_history_import_run r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2",
    )
    .bind(original_run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        after, before,
        "the rejected old unit commits no receipt or ledger delta"
    );
    let mut compatible_unit = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.history_reader','fub-history-timeline-v1',true),set_config('crm.admitted_history_reader','fub-admitted-history-v1',true)")
        .execute(&mut *compatible_unit)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_history_import_receipt(organization_id,request_id,owner_run_id,actor_user_id,operation,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,'resume',$5,$6,$7)")
        .bind(f.org)
        .bind(request_id)
        .bind(original_run)
        .bind(f.actor)
        .bind(&digest)
        .bind(&nonce)
        .bind(&ciphertext)
        .execute(&mut *compatible_unit)
        .await
        .unwrap();
    compatible_unit.commit().await.unwrap();
    let compatible: (i64, i64, i64) = sqlx::query_as(
        "SELECT r.retained_bytes,s.retained_bytes,(SELECT count(*) FROM migration_history_import_receipt x WHERE x.organization_id=r.organization_id AND x.owner_run_id=r.id) FROM migration_history_import_run r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2",
    )
    .bind(original_run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(compatible.0, before.0 + 57);
    assert_eq!(compatible.1, before.1 + 57);
    assert_eq!(compatible.2, before.2 + 1);

    h::action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Action {
            request_id: Uuid::new_v4(),
            expected_revision: confirmed["revision"].as_str().unwrap().into(),
        },
        true,
        None,
    )
    .await
    .unwrap();
    let cancelled = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert!(!cancelled["confirmed_plan_id"].is_null());
    assert_eq!(
        database_message(&shared_probe(&f, true, false).await.unwrap_err()),
        (
            "P010H".to_owned(),
            "admitted_history_reader_required".to_owned()
        )
    );
    assert_eq!(
        database_message(&shared_probe(&f, false, true).await.unwrap_err()),
        ("P010H".to_owned(), "history_reader_required".to_owned())
    );
    shared_probe(&f, true, true).await.unwrap();
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn source_authority_and_root_plan_cursor_scope_fail_closed(migrator: PgPool) {
    let (f, admission_id, capture_id) = admitted::fixture(&migrator).await;
    let root = admitted::ready(&f, admission_id, capture_id).await;
    let ready = h::get(&f.pool, &f.ctx, root).await.unwrap();
    let plan = uuid(&ready["latest_plan_id"]);
    let first = h::records(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        plan,
        h::Page {
            limit: Some(1),
            ..h::Page::default()
        },
    )
    .await
    .unwrap();
    let cursor = first["next_cursor"]
        .as_str()
        .expect("three-family fixture has a next cursor")
        .to_owned();

    let (foreign, foreign_admission, foreign_capture) = admitted::fixture(&migrator).await;
    let foreign_root = admitted::ready(&foreign, foreign_admission, foreign_capture).await;
    let foreign_detail = h::get(&foreign.pool, &foreign.ctx, foreign_root)
        .await
        .unwrap();
    let foreign_plan = uuid(&foreign_detail["latest_plan_id"]);

    let roots_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_admitted_history_root WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    for (candidate_admission, candidate_capture) in [
        (admission_id, foreign_capture),
        (foreign_admission, capture_id),
        (foreign_admission, foreign_capture),
    ] {
        assert!(matches!(
            h::prepare(
                &f.pool,
                &f.key,
                &f.policy,
                &f.ctx,
                h::Prepare {
                    request_id: Uuid::new_v4(),
                    admission_id: candidate_admission,
                    history_capture_id: candidate_capture,
                },
            )
            .await,
            Err(MigrationError::NotFound | MigrationError::SourceNotEligible)
        ));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_history_root WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        roots_before,
        "foreign source combinations create no root"
    );

    let mut member = f.ctx.clone();
    member.actor_user_id = UserId::new(f.member);
    assert!(matches!(
        h::get(&f.pool, &member, root).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        h::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &member,
            h::Prepare {
                request_id: Uuid::new_v4(),
                admission_id,
                history_capture_id: capture_id,
            },
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        h::get(&f.pool, &f.ctx, foreign_root).await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        h::records(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            foreign_plan,
            h::Page::default(),
        )
        .await,
        Err(MigrationError::NotFound)
    ));

    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            expected_revision: ready["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert!(matches!(
        h::records(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            plan,
            h::Page {
                cursor: Some(cursor),
                limit: Some(1),
                ..h::Page::default()
            },
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    assert!(matches!(
        h::action(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            root,
            h::Action {
                request_id: Uuid::new_v4(),
                expected_revision: ready["revision"].as_str().unwrap().into(),
            },
            true,
            None,
        )
        .await,
        Err(MigrationError::ImportConflict)
    ));
}

#[sqlx::test]
#[ignore]
async fn admitted_history_readiness_requires_enabled_authority_and_erasure_triggers(pool: PgPool) {
    let inventory = include_str!("../../crm-app/src/auth/admitted_history_schema.sql");
    assert!(sqlx::query_scalar::<_, bool>(inventory)
        .fetch_one(&pool)
        .await
        .unwrap());
    for (table, trigger) in [
        (
            "migration_admitted_history_result",
            "admitted_history_result_insert",
        ),
        (
            "migration_admitted_history_manifest",
            "admitted_history_immutable",
        ),
        (
            "migration_history_import_identity",
            "admitted_history_owner_insert",
        ),
        (
            "migration_history_import_display",
            "zz_admitted_history_suppress",
        ),
        ("person", "admitted_history_person_erasure"),
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(&format!("ALTER TABLE {table} DISABLE TRIGGER {trigger}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            !sqlx::query_scalar::<_, bool>(inventory)
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            "missing {trigger}"
        );
        tx.rollback().await.unwrap();
    }
    assert!(sqlx::query_scalar::<_, bool>(inventory)
        .fetch_one(&pool)
        .await
        .unwrap());
}

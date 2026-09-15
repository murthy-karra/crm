//! Synthetic foundation regressions. Direct draft-row construction exercises
//! database defenses; it is not evidence of the unfinished refresh commands.
use crate::{db_activity_source, db_people_admission_execution, import_support};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn native_proof_covers_timestamps_and_capability_is_permanent(pool: PgPool) {
    let before = json!({"id":Uuid::new_v4(),"body":"synthetic", "created_at":"2026-09-01T00:00:00Z", "updated_at":"2026-09-02T00:00:00Z", "revision":1});
    let mut changed = before.clone();
    changed["updated_at"] = json!("2026-09-03T00:00:00Z");
    let same: bool = sqlx::query_scalar(
        "SELECT crm_family_refresh_native_digest($1)=crm_family_refresh_native_digest($2)",
    )
    .bind(&before)
    .bind(&changed)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!same, "a different timestamp is a different approved write");
    changed = before.clone();
    changed["revision"] = json!(2);
    let same: bool = sqlx::query_scalar(
        "SELECT crm_family_refresh_native_digest($1)=crm_family_refresh_native_digest($2)",
    )
    .bind(&before)
    .bind(&changed)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(same, "the database owns the separate revision comparison");
    let org = crate::common::create_org(&pool, "Synthetic refresh capability").await;
    sqlx::query("INSERT INTO migration_family_refresh_requirement(organization_id,capability) VALUES($1,'fub-family-refresh-v1')").bind(org).execute(&pool).await.unwrap();
    let failure = sqlx::query("SELECT crm_family_refresh_capability($1)")
        .bind(org)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        failure.as_database_error().unwrap().code().as_deref(),
        Some("P010G")
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT crm_family_refresh_capability($1)")
        .bind(org)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(sqlx::query(
        "DELETE FROM migration_family_refresh_requirement WHERE organization_id=$1"
    )
    .bind(org)
    .execute(&pool)
    .await
    .is_err());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn preparation_lease_and_cohort_are_bound_to_original_evidence(pool: PgPool) {
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let report = db_people_admission_execution::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic newer", "stage":"Lead","assignedUserId":3})],
    )
    .await;
    let bundle = Uuid::new_v4();
    let plan = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,core_report_id,core_snapshot_id,state,source_nonce,source_ciphertext) SELECT $1,r.organization_id,r.parent_import_id,r.parent_plan_id,r.source_account_id,$2,'fub-family-refresh-v1',r.id,r.newer_snapshot_id,'preparing',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex') FROM migration_core_change_report r WHERE r.id=$3 AND r.organization_id=$4")
        .bind(bundle).bind(f.actor).bind(report).bind(f.org).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,nonce,ciphertext) SELECT $1,id,organization_id,'activity',1,'preparing','cohort',core_snapshot_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2")
        .bind(plan).bind(bundle).execute(&pool).await.unwrap();
    let cohort = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id) SELECT $1,$2,r.organization_id,r.source_id,r.person_id,r.id,i.snapshot_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id WHERE r.import_id=$3 AND r.organization_id=$4 AND r.source_id='101'")
        .bind(cohort).bind(bundle).bind(parent).bind(f.org).execute(&pool).await.unwrap();
    let row = sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1")
        .bind(cohort)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id) VALUES($1,$2,$3,'999',$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(bundle).bind(f.org).bind(Uuid::new_v4()).bind(row.get::<Uuid,_>("original_result_id")).bind(row.get::<Uuid,_>("creation_snapshot_id")).execute(&pool).await.is_err(),"cannot borrow another source result");
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&pool).await.unwrap();
    assert!(sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(Uuid::new_v4()).execute(&pool).await.is_err(),"cannot steal an active lease");
    assert!(sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).execute(&pool).await.is_err(),"renewal needs the current token");
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='cancelled',lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(plan).execute(&pool).await.unwrap();
    assert!(
        sqlx::query("UPDATE migration_family_refresh_plan SET state='preparing' WHERE id=$1")
            .bind(plan)
            .execute(&pool)
            .await
            .is_err(),
        "cancelled plans cannot reactivate"
    );
}

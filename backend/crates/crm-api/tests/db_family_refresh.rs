//! Synthetic foundation regressions. Direct draft-row construction exercises
//! database defenses; it is not evidence of the unfinished refresh commands.
use crate::{db_activity_source, db_people_admission_execution, import_support};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn native_proof_covers_timestamps_and_capability_is_permanent(pool: PgPool) {
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&pool)
        .await
        .unwrap();
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
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (bundle, _, _) = draft(&pool, &f, parent).await;
    let org = f.org;
    let mut install = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *install)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_requirement(organization_id,capability,bundle_id) VALUES($1,'fub-family-refresh-v1',$2)").bind(org).bind(bundle).execute(&mut *install).await.unwrap();
    install.commit().await.unwrap();
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
    let (bundle, plan, cohort) = draft(&pool, &f, parent).await;
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

async fn draft(pool: &PgPool, f: &import_support::Fixture, parent: Uuid) -> (Uuid, Uuid, Uuid) {
    draft_cohort(pool, f, parent, true).await
}
async fn draft_cohort(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
    include: bool,
) -> (Uuid, Uuid, Uuid) {
    draft_family(pool, f, parent, include, "activity").await
}
async fn draft_family(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
    include: bool,
    family: &str,
) -> (Uuid, Uuid, Uuid) {
    let report = db_people_admission_execution::report(
        f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic newer", "stage":"Lead","assignedUserId":3})],
    )
    .await;
    let bundle = Uuid::new_v4();
    let plan = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,core_report_id,core_snapshot_id,state,source_nonce,source_ciphertext) SELECT $1,r.organization_id,r.parent_import_id,r.parent_plan_id,r.source_account_id,$2,'fub-family-refresh-v1',r.id,r.newer_snapshot_id,'preparing',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex') FROM migration_core_change_report r WHERE r.id=$3 AND r.organization_id=$4")
        .bind(bundle).bind(f.actor).bind(report).bind(f.org).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,nonce,ciphertext) SELECT $1,id,organization_id,$3,1,'preparing','cohort',core_snapshot_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2")
        .bind(plan).bind(bundle).bind(family).execute(pool).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET payer_plan_id=$2 WHERE id=$1")
        .bind(bundle)
        .bind(plan)
        .execute(pool)
        .await
        .unwrap();
    let cohort = Uuid::new_v4();
    if include {
        sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id) SELECT $1,$2,r.organization_id,r.source_id,r.person_id,r.id,i.snapshot_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id WHERE r.import_id=$3 AND r.organization_id=$4 AND r.source_id='101'")
        .bind(cohort).bind(bundle).bind(parent).bind(f.org).execute(pool).await.unwrap();
    }
    (bundle, plan, cohort)
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn shared_evidence_is_charged_once_and_reservations_roll_back(pool: PgPool) {
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (bundle, plan, _) = draft(&pool, &f, parent).await;
    let baseline=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    let source_budget=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot WHERE id=(SELECT source_snapshot_id FROM migration_family_refresh_plan WHERE id=$1)").bind(plan).fetch_one(&pool).await.unwrap();
    let amount = 16384_i64;
    let control = Uuid::new_v4();
    let denied: bool = sqlx::query_scalar(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,$5,'control',1,4294967296)",
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .bind(amount)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!denied);
    let reserved: bool = sqlx::query_scalar(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,$5,'control',2147483648,4294967296)",
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .bind(amount)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(reserved);
    let actual: i64 = sqlx::query_scalar("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(actual > 0);
    let totals=sqlx::query("SELECT p.measured_bytes,p.retained_bytes,b.shared_measured_bytes,b.shared_retained_bytes FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(
        totals.get::<i64, _>("measured_bytes"),
        totals.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        totals.get::<i64, _>("shared_measured_bytes"),
        totals.get::<i64, _>("shared_retained_bytes")
    );
    assert_eq!(
        actual,
        totals.get::<i64, _>("retained_bytes") + totals.get::<i64, _>("shared_retained_bytes")
    );
    let after=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(
        after.get::<i64, _>("retained_bytes"),
        baseline.get::<i64, _>("retained_bytes") + actual
    );
    assert_eq!(
        after.get::<i64, _>("reserved_bytes"),
        baseline.get::<i64, _>("reserved_bytes") + amount - actual
    );
    let second: i64 = sqlx::query_scalar("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,true)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        second, 0,
        "already settled evidence must not be charged twice"
    );
    let released: i64 = sqlx::query_scalar(
        "SELECT reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(released, baseline.get::<i64, _>("reserved_bytes"));
    assert!(
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,true)")
            .bind(f.org)
            .bind(bundle)
            .bind(plan)
            .bind(control)
            .execute(&pool)
            .await
            .is_err(),
        "cannot refund another time"
    );
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let unit = Uuid::new_v4();
    let reserved: bool = sqlx::query_scalar(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,$5,'unit',2147483648,4294967296)",
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(unit)
    .bind(amount)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(reserved);
    sqlx::query("INSERT INTO migration_family_refresh_mapping(id,bundle_id,plan_id,organization_id,kind,source_key_hmac,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,'note_author',decode(repeat('00',32),'hex'),'hold',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'))")
        .bind(Uuid::new_v4()).bind(bundle).bind(plan).bind(f.org).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query("SELECT 1/0").execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_reservation WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        released
    );
    let mut app_tx = f.pool.begin().await.unwrap();
    crm_api::auth::workspace::shared(&mut app_tx, crm_api::ids::OrganizationId::new(f.org))
        .await
        .unwrap();
    // The accounting probe must first satisfy the preparation fence. Timezone
    // is the sole mapping without a source row; this synthetic row rolls back.
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *app_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET phase='mappings' WHERE id=$1")
        .bind(plan)
        .execute(&mut *app_tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_mapping(id,bundle_id,plan_id,organization_id,kind,source_key_hmac,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,'timezone',decode(repeat('00',32),'hex'),'hold',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'))")
        .bind(Uuid::new_v4()).bind(bundle).bind(plan).bind(f.org).execute(&mut *app_tx).await.unwrap();
    assert!(
        app_tx.commit().await.is_err(),
        "application cannot commit evidence without settling its charge"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let orphan = Uuid::new_v4();
    let mut expired = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *expired)
        .await
        .unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,16384,'unit',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(orphan)
    .fetch_one(&mut *expired)
    .await
    .unwrap());
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '100 milliseconds' WHERE id=$1").bind(plan).execute(&mut *expired).await.unwrap();
    expired.commit().await.unwrap();
    sqlx::query("SELECT pg_sleep(0.15)")
        .execute(&pool)
        .await
        .unwrap();
    let replacement = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=2,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(replacement).execute(&pool).await.unwrap();
    let mut stale = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *stale)
        .await
        .unwrap();
    assert!(
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,1,false)")
            .bind(f.org)
            .bind(bundle)
            .bind(plan)
            .bind(orphan)
            .execute(&mut *stale)
            .await
            .is_err(),
        "old worker cannot settle after takeover"
    );
    stale.rollback().await.unwrap();
    let mut takeover = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(replacement.to_string())
        .execute(&mut *takeover)
        .await
        .unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT crm_family_refresh_reclaim($1,$2,$3,$4,2)")
            .bind(f.org)
            .bind(bundle)
            .bind(plan)
            .bind(orphan)
            .fetch_one(&mut *takeover)
            .await
            .unwrap()
    );
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT crm_family_refresh_reclaim($1,$2,$3,$4,2)")
            .bind(f.org)
            .bind(bundle)
            .bind(plan)
            .bind(orphan)
            .fetch_one(&mut *takeover)
            .await
            .unwrap(),
        "a retry cannot refund twice"
    );
    takeover.commit().await.unwrap();
    let source_after=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot WHERE id=(SELECT source_snapshot_id FROM migration_family_refresh_plan WHERE id=$1)").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(
        source_after.get::<i64, _>("retained_bytes"),
        source_budget.get::<i64, _>("retained_bytes") + actual
    );
    assert_eq!(
        source_after.get::<i64, _>("reserved_bytes"),
        source_budget.get::<i64, _>("reserved_bytes")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        released
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_corrections_preserve_identity_and_erase_current_date_bucket(pool: PgPool) {
    use crate::{db_history_capture_support as capture, db_history_import_support as history};
    let (f, parent, initial, book) = history::fixture(&pool).await;
    let import = history::ready(&f, parent, initial).await;
    history::confirm(&f, import).await;
    history::drain(&f).await;
    let (newer, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, newer).await;
    capture::drain(&f, &book).await;
    let (successor, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, successor).await;
    capture::drain(&f, &book).await;
    let mut tx = pool.begin().await.unwrap();
    for (name, value) in [
        ("test.family_org", f.org),
        ("test.family_actor", f.actor),
        ("test.family_parent", parent),
        ("test.family_capture", newer),
        ("test.family_successor_capture", successor),
    ] {
        sqlx::query("SELECT set_config($1,$2,true)")
            .bind(name)
            .bind(value.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::raw_sql(include_str!("fixtures/family_refresh_history.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn cohort_pages_are_atomic_metered_and_replay_safe(pool: PgPool) {
    use crm_api::{
        domain::migration::{
            family_refresh::cohort::{self, Claim, Progress},
            snapshot::SnapshotPolicy,
        },
        ids::OrganizationId,
    };
    let (f, parent, _) = crate::db_admitted_people_refresh_execution::fixture_with_admission(
        &pool,
        vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let (bundle, plan, _) = draft_cohort(&pool, &f, parent, false).await;
    let control = Uuid::new_v4();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(&pool)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .execute(&pool)
        .await
        .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1")
        .bind(plan).bind(token).execute(&pool).await.unwrap();
    let mut claim = Claim {
        organization: OrganizationId::new(f.org),
        bundle,
        plan,
        token,
        epoch: 1,
    };
    let snapshot = |pool: PgPool| async move {
        sqlx::query_scalar::<_,serde_json::Value>("SELECT jsonb_build_object('plan',to_jsonb(p),'bundle',to_jsonb(b),'storage',to_jsonb(s),'cohort',(SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY source_person_id),'[]') FROM migration_family_refresh_cohort c WHERE c.bundle_id=b.id),'reservations',(SELECT jsonb_agg(to_jsonb(r) ORDER BY token) FROM migration_family_refresh_reservation r WHERE r.bundle_id=b.id)) FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id JOIN migration_snapshot_storage s ON s.organization_id=p.organization_id WHERE p.id=$1")
            .bind(plan).fetch_one(&pool).await.unwrap()
    };
    let before = snapshot(pool.clone()).await;
    let rejected =
        sqlx::query("UPDATE migration_family_refresh_plan SET cohort_after='101' WHERE id=$1")
            .bind(plan)
            .execute(&f.pool)
            .await
            .unwrap_err();
    assert_eq!(
        rejected.as_database_error().unwrap().message(),
        "stale family cohort preparation claim"
    );
    let rejected = sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id) SELECT $1,$2,r.organization_id,r.source_id,r.person_id,r.id,i.snapshot_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id WHERE r.import_id=$3 AND r.organization_id=$4 AND r.source_id='101'")
        .bind(Uuid::new_v4()).bind(bundle).bind(parent).bind(f.org).execute(&f.pool).await.unwrap_err();
    assert_eq!(
        rejected.as_database_error().unwrap().message(),
        "stale family cohort preparation claim"
    );
    claim.token = Uuid::new_v4();
    assert!(cohort::freeze_page(&f.pool, &claim, &f.policy, 1)
        .await
        .is_err());
    claim.token = token;
    claim.organization = OrganizationId::new(Uuid::new_v4());
    assert!(cohort::freeze_page(&f.pool, &claim, &f.policy, 1)
        .await
        .is_err());
    claim.organization = OrganizationId::new(f.org);
    assert_eq!(
        cohort::freeze_page(
            &f.pool,
            &claim,
            &SnapshotPolicy {
                run_ceiling_bytes: 1,
                org_ceiling_bytes: 1
            },
            1
        )
        .await
        .unwrap(),
        Progress::Capacity
    );
    assert_eq!(snapshot(pool.clone()).await, before);
    // Failure after cohort insertion must roll back rows, cursor and charges.
    sqlx::raw_sql("CREATE FUNCTION test_cohort_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.cohort_after<>OLD.cohort_after THEN RAISE EXCEPTION 'synthetic checkpoint failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_cohort_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_cohort_fault()")
        .execute(&pool).await.unwrap();
    assert!(cohort::freeze_page(&f.pool, &claim, &f.policy, 1)
        .await
        .is_err());
    assert_eq!(snapshot(pool.clone()).await, before);
    sqlx::raw_sql("DROP TRIGGER test_cohort_fault ON migration_family_refresh_plan; DROP FUNCTION test_cohort_fault()")
        .execute(&pool).await.unwrap();
    let mut pages = 0;
    loop {
        match cohort::freeze_page(&f.pool, &claim, &f.policy, 1)
            .await
            .unwrap()
        {
            Progress::Advanced {
                examined,
                included,
                finished,
            } => {
                assert_eq!(examined, 1);
                assert_eq!(included, 1);
                pages += 1;
                if finished {
                    break;
                }
                assert!(pages < 10);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(pages, 2);
    let after = snapshot(pool.clone()).await;
    assert_eq!(
        after["plan"]["cohort_counts"],
        json!({"original":"1","admitted":"1"})
    );
    assert_eq!(after["plan"]["phase"], "capture");
    assert_eq!(
        after["plan"]["measured_bytes"],
        after["plan"]["retained_bytes"]
    );
    assert_eq!(
        after["bundle"]["shared_measured_bytes"],
        after["bundle"]["shared_retained_bytes"]
    );
    assert_eq!(
        cohort::freeze_page(&f.pool, &claim, &f.policy, 1)
            .await
            .unwrap(),
        Progress::Finished
    );
    assert_eq!(snapshot(pool.clone()).await, after);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn cohort_recovers_proven_people_and_respects_terminal_boundary(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::cohort::PAGE_SQL;
    let (f, parent, admission) = crate::db_people_recovery::recovered_fixture(&pool).await;
    let root = sqlx::query(
        "SELECT source_account_id,completed_at FROM migration_people_admission WHERE id=$1",
    )
    .bind(admission)
    .fetch_one(&pool)
    .await
    .unwrap();
    let completed: chrono::DateTime<chrono::Utc> = root.get("completed_at");
    let rows = sqlx::query(PAGE_SQL)
        .bind(f.org)
        .bind(root.get::<i64, _>("source_account_id"))
        .bind(parent)
        .bind("")
        .bind(completed)
        .bind(51_i64)
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("disposition"), "original");
    assert_eq!(rows[1].get::<String, _>("disposition"), "recovery");
    assert_eq!(rows[1].get::<Uuid, _>("admission_id"), admission);
    let cutoff = completed - chrono::Duration::microseconds(1);
    let rows = sqlx::query(PAGE_SQL)
        .bind(f.org)
        .bind(root.get::<i64, _>("source_account_id"))
        .bind(parent)
        .bind("")
        .bind(cutoff)
        .bind(51_i64)
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "identity exists before root finishes");
    assert_eq!(rows[1].get::<String, _>("disposition"), "unfinished");
    let rows = sqlx::query(PAGE_SQL)
        .bind(Uuid::new_v4())
        .bind(root.get::<i64, _>("source_account_id"))
        .bind(parent)
        .bind("")
        .bind(completed)
        .bind(51_i64)
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn core_index_authenticates_all_occurrences_and_rolls_back_failed_pages(pool: PgPool) {
    use crm_api::{
        config::RawPayloadKey,
        domain::migration::{
            family_refresh::{
                cohort::{self, Claim},
                core_source,
                evidence::{Purpose, Scope},
                model::Family,
            },
            snapshot_source::Stream,
        },
        ids::OrganizationId,
    };
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let mut tasks: Vec<_> = (0..100)
        .map(|i| db_activity_source::task(500 + i))
        .collect();
    let mut duplicate = db_activity_source::task(500);
    duplicate["personId"] = json!(999);
    duplicate["name"] = json!("Distinct retained occurrence outside frozen People");
    tasks.push(duplicate);
    tasks.push(db_activity_source::task(700));
    f.reader.set_records(Stream::TasksOpen, tasks);
    f.reader
        .set_records(Stream::Notes, vec![json!({"id":11,"personId":101})]);
    f.reader
        .set_raw(Stream::NoteDetail, 0, 404, b"{}".to_vec(), false);
    let (bundle, plan, _) = draft_cohort(&pool, &f, parent, false).await;
    let control = Uuid::new_v4();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(&pool)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .execute(&pool)
        .await
        .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&pool).await.unwrap();
    let claim = Claim {
        organization: OrganizationId::new(f.org),
        bundle,
        plan,
        token,
        epoch: 1,
    };
    assert!(matches!(
        cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
            .await
            .unwrap(),
        cohort::Progress::Advanced { finished: true, .. }
    ));
    let snapshot = sqlx::query_scalar::<_, Uuid>(
        "SELECT source_snapshot_id FROM migration_family_refresh_plan WHERE id=$1",
    )
    .bind(plan)
    .fetch_one(&pool)
    .await
    .unwrap();
    let state = |pool: PgPool| async move {
        sqlx::query_scalar::<_,serde_json::Value>("SELECT jsonb_build_object('plan',to_jsonb(p),'bundle',to_jsonb(b),'org',to_jsonb(s),'snapshot',to_jsonb(c),'pages',(SELECT count(*) FROM migration_family_refresh_core_page WHERE bundle_id=b.id),'sources',(SELECT count(*) FROM migration_family_refresh_source WHERE bundle_id=b.id)) FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id JOIN migration_snapshot_storage s ON s.organization_id=p.organization_id JOIN migration_snapshot c ON c.id=p.source_snapshot_id WHERE p.id=$1")
            .bind(plan).fetch_one(&pool).await.unwrap()
    };
    let before = state(pool.clone()).await;
    let calls = f.reader.calls();
    assert!(
        core_source::index_page(&f.pool, &RawPayloadKey::new([98; 32]), &claim, &f.policy)
            .await
            .is_err()
    );
    assert_eq!(state(pool.clone()).await, before);
    // Corrupt one immutable source record only in this synthetic migrator-owned
    // database, then restore it before asserting the failure.
    let source=sqlx::query("SELECT r.id,r.semantic_hmac FROM migration_snapshot_record r JOIN migration_snapshot_capture c ON c.id=r.capture_id WHERE r.snapshot_id=$1 AND c.stream='users' ORDER BY c.sequence,r.ordinal LIMIT 1").bind(snapshot).fetch_one(&pool).await.unwrap();
    sqlx::raw_sql("ALTER TABLE migration_snapshot_record DISABLE TRIGGER USER")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_snapshot_record SET semantic_hmac=decode(repeat('00',32),'hex') WHERE id=$1").bind(source.get::<Uuid,_>("id")).execute(&pool).await.unwrap();
    let failure = core_source::index_page(&f.pool, &f.key, &claim, &f.policy).await;
    sqlx::query("UPDATE migration_snapshot_record SET semantic_hmac=$2 WHERE id=$1")
        .bind(source.get::<Uuid, _>("id"))
        .bind(source.get::<Vec<u8>, _>("semantic_hmac"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE migration_snapshot_record ENABLE TRIGGER USER")
        .execute(&pool)
        .await
        .unwrap();
    assert!(failure.is_err());
    assert_eq!(state(pool.clone()).await, before);
    sqlx::raw_sql("CREATE FUNCTION test_capture_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.capture_checkpoint>OLD.capture_checkpoint THEN RAISE EXCEPTION 'synthetic capture checkpoint failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_capture_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_capture_fault()")
        .execute(&pool).await.unwrap();
    assert!(core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
        .await
        .is_err());
    assert_eq!(state(pool.clone()).await, before);
    sqlx::raw_sql("DROP TRIGGER test_capture_fault ON migration_family_refresh_plan; DROP FUNCTION test_capture_fault()").execute(&pool).await.unwrap();
    let mut pages = 0;
    loop {
        match core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
        {
            core_source::Progress::Indexed { .. } => {
                pages += 1;
                assert!(pages < 30);
            }
            core_source::Progress::Finished => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(pages >= 8);
    assert_eq!(
        f.reader.calls(),
        calls,
        "indexing must use retained evidence only"
    );
    let variants=sqlx::query("SELECT count(*) AS n,count(DISTINCT semantic_hmac) AS variants,count(DISTINCT source_person_id) AS people FROM migration_family_refresh_source WHERE bundle_id=$1 AND kind='task' AND source_id='500'").bind(bundle).fetch_one(&pool).await.unwrap();
    assert_eq!(variants.get::<i64, _>("n"), 2);
    assert_eq!(variants.get::<i64, _>("variants"), 2);
    assert_eq!(
        variants.get::<i64, _>("people"),
        2,
        "do not hide conflicting Person links by cohort filtering"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_core_page WHERE bundle_id=$1 AND stream='note_detail' AND reason='source_unavailable'").bind(bundle).fetch_one(&pool).await.unwrap(),1);
    use crm_api::domain::migration::family_refresh::{
        core_resolution::{self, Kind, Resolution},
        model::Hold,
    };
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Task, "500")
            .await
            .unwrap(),
        Resolution::Held(Hold::SourceConflict)
    ));
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Note, "11")
            .await
            .unwrap(),
        Resolution::Held(Hold::SourceUnavailable)
    ));
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Task, "999999")
            .await
            .unwrap(),
        Resolution::Held(Hold::SourceNotObserved)
    ));
    let chosen = match core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Task, "501")
        .await
        .unwrap()
    {
        Resolution::Ready(r) => r,
        _ => panic!("qualified task must resolve"),
    };
    assert_eq!(chosen.observations, 1);
    assert_eq!(chosen.source_person.as_deref(), Some("101"));
    assert!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Task, "0501")
            .await
            .is_err()
    );
    assert!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Person, "101")
            .await
            .is_err()
    );
    let cursor=sqlx::query("SELECT id,nonce,ciphertext FROM migration_family_refresh_core_page WHERE bundle_id=$1 AND stream='tasks_open' AND checkpoint=1").bind(bundle).fetch_one(&pool).await.unwrap();
    let scope = Scope {
        organization: claim.organization,
        bundle,
        plan,
        family: Family::Activity,
        revision: 1,
    };
    let payload: serde_json::Value = scope
        .open(
            &f.key,
            cursor.get("id"),
            Purpose::Binding,
            cursor.get("nonce"),
            cursor.get("ciphertext"),
        )
        .unwrap();
    assert_eq!(payload["cursor"]["offset"], 100);
    let after = state(pool.clone()).await;
    assert_eq!(after["plan"]["phase"], "mappings");
    assert_eq!(
        after["plan"]["measured_bytes"],
        after["plan"]["retained_bytes"]
    );
    assert_eq!(
        core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap(),
        core_source::Progress::Finished
    );
    assert_eq!(state(pool.clone()).await, after);
}

pub(super) async fn assert_activity_after_states(
    f: &import_support::Fixture,
    root: Uuid,
    admitted: bool,
) {
    use crm_api::{
        domain::migration::{
            crypto,
            family_refresh::activity_baseline::{self, AfterState, Binding},
        },
        ids::OrganizationId,
    };
    let prefix = if admitted {
        "migration_admitted_activity"
    } else {
        "migration_activity"
    };
    let rows=sqlx::query(&format!("SELECT r.*,a.snapshot_id,m.native_source_key FROM {prefix}_result r JOIN {prefix}_import a ON a.id=r.import_id AND a.organization_id=r.organization_id JOIN {prefix}_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.organization_id=r.organization_id WHERE r.import_id=$1 AND r.organization_id=$2 AND r.disposition='applied'"))
        .bind(root).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert!(!rows.is_empty());
    for r in rows {
        let plan: Uuid = r.get("plan_id");
        let bytes = crypto::open_snapshot(
            &f.key,
            OrganizationId::new(f.org),
            r.get("snapshot_id"),
            r.get("id"),
            &format!("activity-v1:{plan}:result"),
            r.get("nonce"),
            r.get("ciphertext"),
        )
        .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proof: AfterState = serde_json::from_value(result["after_state"].clone()).unwrap();
        let query = match r.get::<String, _>("kind").as_str() {
            "note" => "SELECT to_jsonb(n) FROM note n WHERE id=$1 AND organization_id=$2",
            "task" => "SELECT to_jsonb(t) FROM task t WHERE id=$1 AND organization_id=$2",
            _ => panic!("unexpected kind"),
        };
        let native: serde_json::Value = sqlx::query_scalar(query)
            .bind(r.get::<Uuid, _>("target_id"))
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let baseline = activity_baseline::verify(
            Some(&proof),
            Binding {
                organization: OrganizationId::new(f.org),
                person: r.get("person_id"),
                target: r.get("target_id"),
                result: r.get("id"),
                first_import: root,
                native_source_key: &r.get::<String, _>("native_source_key"),
            },
            Some(&native),
        )
        .unwrap();
        assert_eq!(baseline.native, native);
        assert!(baseline.owned);
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn admitted_activity_retains_exact_initial_after_state(pool: PgPool) {
    let (f, _, root, ready) = crate::db_admitted_activity::prepared(&pool, 1).await;
    crate::db_admitted_activity::confirm_ready(&f, root, &ready).await;
    crate::db_admitted_activity::drain(&f).await;
    assert_activity_after_states(&f, root, true).await;
    use crm_api::domain::migration::family_refresh::{
        activity_baseline::{self, Discovery},
        model::{Hold, Kind},
    };
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_admitted_activity_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut new_task = db_activity_source::task(22);
    new_task["personId"] = json!(104);
    f.reader.set_records(
        crm_api::domain::migration::snapshot_source::Stream::TasksOpen,
        vec![new_task],
    );
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let bundle = claim.bundle;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='104'").bind(bundle).fetch_one(&pool).await.unwrap();
    let owners = sqlx::query(
        "SELECT * FROM crm_family_refresh_owned_activity($1,$2) ORDER BY kind,source_id",
    )
    .bind(f.org)
    .bind(bundle)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(owners.len(), 2);
    assert!(owners
        .iter()
        .all(|r| r.get::<Uuid, _>("cohort_id") == cohort));
    assert_eq!(owners[0].get::<String, _>("kind"), "note");
    assert_eq!(owners[0].get::<String, _>("source_id"), "11");
    assert_eq!(owners[1].get::<String, _>("kind"), "task");
    assert_eq!(owners[1].get::<String, _>("source_id"), "21");
    for (kind, source) in [(Kind::Note, "11"), (Kind::Task, "21")] {
        assert!(matches!(
            activity_baseline::discover(&f.pool, &f.key, &claim, cohort, kind, source)
                .await
                .unwrap(),
            Discovery::Proven(_)
        ));
    }
    assert_new_candidate(&f, &claim, cohort, Kind::Task, "22").await;
    let wrong:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(bundle).fetch_one(&pool).await.unwrap();
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &claim, wrong, Kind::Note, "11")
            .await
            .unwrap(),
        Discovery::Held(Hold::IdentityMismatch)
    ));

    let row=sqlx::query("SELECT retained_bytes,measured_bytes,reserved_bytes FROM migration_admitted_activity_import WHERE id=$1").bind(root).fetch_one(&pool).await.unwrap();
    assert_eq!(
        row.get::<i64, _>("retained_bytes"),
        row.get::<i64, _>("measured_bytes")
    );
    assert_eq!(row.get::<i64, _>("reserved_bytes"), 0);
}

async fn prepared_indexed_refresh(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
) -> crm_api::domain::migration::family_refresh::cohort::Claim {
    prepared_family_refresh(pool, f, parent, "activity").await
}
pub(super) async fn prepared_family_refresh(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
    family: &str,
) -> crm_api::domain::migration::family_refresh::cohort::Claim {
    use crm_api::{
        domain::migration::family_refresh::{
            cohort::{self, Claim},
            core_source,
        },
        ids::OrganizationId,
    };
    let (bundle, plan, _) = draft_family(pool, f, parent, false, family).await;
    let control = Uuid::new_v4();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(pool)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .execute(pool)
        .await
        .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(pool).await.unwrap();
    let claim = Claim {
        organization: OrganizationId::new(f.org),
        bundle,
        plan,
        token,
        epoch: 1,
    };
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();
    for step in 0..30 {
        if core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == core_source::Progress::Finished
        {
            break;
        }
        assert!(step < 29);
    }
    claim
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn original_activity_discovers_only_its_proven_baseline(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_baseline::{self, Discovery},
            model::{Hold, Kind},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (child, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, child).await;
    let ready = db_activity_source::replan(&f, child, &ready, choices, None).await;
    db_activity_source::confirm(&f, child, &ready).await;
    assert_activity_after_states(&f, child, false).await;
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Discovery::Proven(_)
    ));
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "999")
            .await
            .unwrap(),
        Discovery::Held(Hold::BaselineUnproven)
    ));
    let interval=sqlx::query("SELECT newer.id,newer.started_at,older.completed_at FROM migration_snapshot newer JOIN migration_family_refresh_plan p ON p.source_snapshot_id=newer.id JOIN migration_activity_import i ON i.id=$2 JOIN migration_snapshot older ON older.id=i.snapshot_id WHERE p.id=$1").bind(claim.plan).bind(child).fetch_one(&pool).await.unwrap();
    let snapshot: Uuid = interval.get("id");
    sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
        .bind(snapshot)
        .bind(interval.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"))
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Discovery::Held(Hold::SourceNotNewer)
    ));
    sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
        .bind(snapshot)
        .bind(interval.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_activity_import SET completed_at=clock_timestamp()+interval '1 minute' WHERE id=$1").bind(child).execute(&pool).await.unwrap();
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Discovery::Held(Hold::FirstCoverageRequired)
    ));
}

/// Exercise proof production through both real first-import executors. The
/// existing callers also verify native fidelity, retry and exact byte ledgers.
pub(super) async fn assert_metadata_after_states(
    f: &import_support::Fixture,
    root: Uuid,
    admitted: bool,
) {
    use crm_api::domain::migration::{
        crypto,
        family_refresh::{
            metadata_baseline::{self, AfterState, Binding},
            metadata_delta::Snapshot,
            model::Hold,
        },
    };
    let prefix = if admitted {
        "migration_admitted_metadata"
    } else {
        "migration_metadata"
    };
    let rows = sqlx::query(&format!("SELECT r.*,i.snapshot_id FROM {prefix}_result r JOIN {prefix}_import i ON i.id=r.import_id AND i.organization_id=r.organization_id WHERE r.import_id=$1 AND r.organization_id=$2 AND r.kind='people'"))
        .bind(root).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert!(!rows.is_empty());
    for r in rows {
        let plan: Uuid = r.get("plan_id");
        let namespace = if admitted {
            "admitted-metadata-v1"
        } else {
            "metadata-v1"
        };
        let bytes = crypto::open_snapshot(
            &f.key,
            f.ctx.organization_id,
            r.get("snapshot_id"),
            r.get("id"),
            &format!("{namespace}:{plan}:result"),
            &r.get::<Vec<u8>, _>("nonce"),
            &r.get::<Vec<u8>, _>("ciphertext"),
        )
        .unwrap();
        let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if r.get::<String, _>("disposition") == "held" {
            assert!(
                data["after_state"].is_null(),
                "held units do not create a baseline"
            );
            continue;
        }
        let proof: AfterState = serde_json::from_value(data["after_state"].clone()).unwrap();
        let snapshot: Snapshot =
            serde_json::from_value(data["after_state"]["snapshot"].clone()).unwrap();
        let person: Uuid = r.get("person_id");
        let revision: i64 = sqlx::query_scalar(
            "SELECT metadata_revision FROM person WHERE id=$1 AND organization_id=$2",
        )
        .bind(person)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(snapshot.revision, revision);
        let tags: Vec<Uuid> = sqlx::query_scalar("SELECT tag_id FROM person_tag WHERE person_id=$1 AND organization_id=$2 ORDER BY tag_id").bind(person).bind(f.org).fetch_all(&f.pool).await.unwrap();
        assert_eq!(snapshot.state.tags, tags.into_iter().collect());
        let fields = sqlx::query("SELECT field_id,field_type,text_value,number_value::text AS number_value,date_value::text AS date_value,option_id FROM person_custom_field_value WHERE person_id=$1 AND organization_id=$2").bind(person).bind(f.org).fetch_all(&f.pool).await.unwrap();
        assert_eq!(snapshot.state.fields.len(), fields.len());
        for field in fields {
            let id: Uuid = field.get("field_id");
            let actual = serde_json::to_value(&snapshot.state.fields[&id]).unwrap();
            let expected = match field.get::<String, _>("field_type").as_str() {
                "text" => json!({"text":field.get::<String,_>("text_value")}),
                "number" => json!({"number":field.get::<String,_>("number_value")}),
                "date" => json!({"date":field.get::<String,_>("date_value")}),
                "choice" => json!({"option_id":field.get::<Uuid,_>("option_id")}),
                _ => panic!("unsupported native type"),
            };
            assert_eq!(actual, expected);
        }
        let binding = Binding {
            organization: f.ctx.organization_id,
            import: root,
            manifest: r.get("manifest_id"),
            person,
        };
        let (_, owned) = metadata_baseline::verify(Some(&proof), &binding, Some(&snapshot))
            .unwrap_or_else(|_| panic!("exact persisted after-state must verify"));
        let applied_tags = data["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| {
                op["kind"] == "tag_link"
                    && op[if admitted { "outcome" } else { "disposition" }] == "applied"
            })
            .count();
        let applied_fields = data["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| {
                op["kind"] == "value"
                    && op[if admitted { "outcome" } else { "disposition" }] == "applied"
            })
            .count();
        assert_eq!(owned.tags.len(), applied_tags);
        assert_eq!(owned.fields.len(), applied_fields);
        let mut reverted = snapshot.clone();
        reverted.revision += 2;
        assert!(matches!(
            metadata_baseline::verify(Some(&proof), &binding, Some(&reverted)),
            Err(Hold::LocalChange)
        ));
        let foreign = Binding {
            organization: crm_api::ids::OrganizationId::new(Uuid::new_v4()),
            ..binding
        };
        assert!(matches!(
            metadata_baseline::verify(Some(&proof), &foreign, Some(&snapshot)),
            Err(Hold::BaselineUnproven)
        ));
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn shared_source_resolves_across_families_without_copying_or_reencrypting(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::{
        cohort::Claim,
        core_resolution::{self, Kind, Resolution},
    };
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let original = prepared_indexed_refresh(&pool, &f, parent).await;
    let original_bytes: i64 = sqlx::query_scalar(
        "SELECT shared_retained_bytes FROM migration_family_refresh_bundle WHERE id=$1",
    )
    .bind(original.bundle)
    .fetch_one(&pool)
    .await
    .unwrap();
    let plan = Uuid::new_v4();
    let control = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,nonce,ciphertext) SELECT $1,id,organization_id,'metadata',1,'preparing','mappings',core_snapshot_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2").bind(plan).bind(original.bundle).execute(&pool).await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(original.bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(&pool)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(original.bundle)
        .bind(plan)
        .bind(control)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&pool).await.unwrap();
    let claim = Claim {
        organization: original.organization,
        bundle: original.bundle,
        plan,
        token,
        epoch: 1,
    };
    let source = match core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Person, "101")
        .await
        .unwrap()
    {
        Resolution::Ready(r) => r,
        Resolution::Held(h) => panic!("unexpected hold {h:?}"),
    };
    assert_eq!(source.source_person.as_deref(), Some("101"));
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT plan_id FROM migration_family_refresh_source WHERE id=$1"
        )
        .bind(source.row)
        .fetch_one(&pool)
        .await
        .unwrap(),
        original.plan
    );
    let c=sqlx::query("SELECT id,person_id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    // A review-only manifest may reference the original shared source from a
    // sibling family. The rollback keeps this synthetic manifest out of billing.
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(token.to_string()).execute(&mut *tx).await.unwrap();
    let manifest = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,disposition,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,1,'metadata',decode(repeat('01',32),'hex'),$7,$7,'already_current','{}',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'),4096)")
        .bind(manifest).bind(claim.bundle).bind(plan).bind(f.org).bind(c.get::<Uuid,_>("id")).bind(source.row).bind(c.get::<Uuid,_>("person_id")).execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_source WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT shared_retained_bytes FROM migration_family_refresh_bundle WHERE id=$1"
        )
        .bind(claim.bundle)
        .fetch_one(&pool)
        .await
        .unwrap(),
        original_bytes
    );
    // Family prerequisites are independent even when one immutable index is shared.
    let snapshot: Uuid = sqlx::query_scalar(
        "SELECT core_snapshot_id FROM migration_family_refresh_bundle WHERE id=$1",
    )
    .bind(claim.bundle)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND stream='tasks_open'").bind(snapshot).execute(&pool).await.unwrap();
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Person, "101")
            .await
            .unwrap(),
        Resolution::Ready(_)
    ));
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &original, Kind::Task, "21")
            .await
            .unwrap(),
        Resolution::Held(
            crm_api::domain::migration::family_refresh::model::Hold::SourceUnavailable
        )
    ));
    sqlx::query("UPDATE migration_snapshot_stream SET state='completed' WHERE snapshot_id=$1 AND stream='tasks_open'").bind(snapshot).execute(&pool).await.unwrap();
    sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND stream='custom_fields'").bind(snapshot).execute(&pool).await.unwrap();
    assert!(matches!(
        core_resolution::resolve(&f.pool, &f.key, &claim, Kind::Person, "101")
            .await
            .unwrap(),
        Resolution::Held(
            crm_api::domain::migration::family_refresh::model::Hold::SourceUnavailable
        )
    ));
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_index_preserves_pages_privacy_and_atomic_accounting(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::{
        core_source::Progress,
        evidence::{Purpose, Scope},
        history_index,
        model::Family,
    };
    let (f, _, capture) = crate::db_admitted_history::fixture_count(&pool, 101).await;
    let capture_bytes = sqlx::query(
        "SELECT retained_bytes,reserved_bytes FROM migration_history_capture_run WHERE id=$1",
    )
    .bind(capture)
    .fetch_one(&pool)
    .await
    .unwrap();
    let capture_before = capture_bytes.get::<i64, _>("retained_bytes");
    let reserved_before = capture_bytes.get::<i64, _>("reserved_bytes");
    let claim = draft_history_refresh(&pool, &f, capture).await;
    let bundle = claim.bundle;
    let plan = claim.plan;
    let original = sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE id=$1")
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap();
    let before = original.get::<i64, _>("retained_bytes");
    let row=sqlx::query("SELECT id,semantic_hmac FROM migration_history_observation WHERE run_id=$1 ORDER BY capture_sequence,ordinal LIMIT 1").bind(capture).fetch_one(&pool).await.unwrap();
    sqlx::raw_sql("ALTER TABLE migration_history_observation DISABLE TRIGGER USER")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_history_observation SET semantic_hmac=decode(repeat('00',32),'hex') WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&pool).await.unwrap();
    let failure = history_index::index_page(&f.pool, &f.key, &claim, &f.policy).await;
    sqlx::query("UPDATE migration_history_observation SET semantic_hmac=$2 WHERE id=$1")
        .bind(row.get::<Uuid, _>("id"))
        .bind(row.get::<Vec<u8>, _>("semantic_hmac"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE migration_history_observation ENABLE TRIGGER USER")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        failure.is_err(),
        "tampered observation must roll back the complete page"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_history_page WHERE bundle_id=$1"
        )
        .bind(bundle)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::raw_sql("CREATE FUNCTION test_history_checkpoint_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.capture_checkpoint>OLD.capture_checkpoint THEN RAISE EXCEPTION 'synthetic history checkpoint failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_history_checkpoint_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_history_checkpoint_fault()").execute(&pool).await.unwrap();
    assert!(
        history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_history_checkpoint_fault ON migration_family_refresh_plan; DROP FUNCTION test_history_checkpoint_fault()").execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT retained_bytes FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        before
    );
    let mut pages = 0;
    let mut records = 0;
    loop {
        match history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
        {
            Progress::Indexed {
                observations,
                qualified,
            } => {
                pages += 1;
                records += observations;
                assert_eq!(observations, qualified);
                assert!(pages < 10);
            }
            Progress::Finished => break,
            Progress::Capacity => panic!("unexpected capacity hold"),
        }
    }
    assert_eq!(pages, 4);
    assert_eq!(records, 103);
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle,
        plan,
        family: Family::History,
        revision: 1,
    };
    let rows=sqlx::query("SELECT * FROM migration_family_refresh_source WHERE plan_id=$1 ORDER BY capture_sequence,ordinal").bind(plan).fetch_all(&pool).await.unwrap();
    for r in rows {
        let data: serde_json::Value = scope
            .open(
                &f.key,
                r.get("id"),
                Purpose::Source,
                r.get("nonce"),
                r.get("ciphertext"),
            )
            .unwrap();
        assert!(
            !data.to_string().contains("SENTINEL"),
            "body and phone content stays outside metadata index"
        );
        assert_eq!(data["evidence"]["source_person"], "104");
        assert!(!data["observation"].is_null());
    }
    let after=sqlx::query("SELECT measured_bytes,retained_bytes,capture_checkpoint FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(
        after.get::<i64, _>("measured_bytes"),
        after.get::<i64, _>("retained_bytes")
    );
    assert!(after.get::<i64, _>("retained_bytes") > before);
    assert_eq!(
        history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap(),
        Progress::Finished
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT retained_bytes FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        after.get::<i64, _>("retained_bytes")
    );
    let charged=sqlx::query("SELECT h.retained_bytes,h.reserved_bytes,p.retained_bytes+b.shared_retained_bytes AS evidence,p.reserved_bytes AS reserved FROM migration_history_capture_run h JOIN migration_family_refresh_plan p ON p.history_capture_id=h.id AND p.organization_id=h.organization_id JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(
        charged.get::<i64, _>("retained_bytes") - capture_before,
        charged.get::<i64, _>("evidence")
    );
    assert_eq!(
        charged.get::<i64, _>("reserved_bytes") - reserved_before,
        charged.get::<i64, _>("reserved")
    );
    // The capture's remaining capacity is binding even while the family plan
    // and Organization have spare room. A rejection creates no reservation.
    let ceiling =
        charged.get::<i64, _>("retained_bytes") + charged.get::<i64, _>("reserved_bytes") + 8191;
    let denied = Uuid::new_v4();
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,8192,'control',$5,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(denied)
    .bind(ceiling)
    .fetch_one(&f.pool)
    .await
    .unwrap());
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM migration_family_refresh_reservation WHERE token=$1)"
    )
    .bind(denied)
    .fetch_one(&pool)
    .await
    .unwrap());
    let abandoned = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,8192,'unit',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(abandoned)
    .fetch_one(&mut *tx)
    .await
    .unwrap());
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '100 milliseconds' WHERE id=$1").bind(plan).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query("SELECT pg_sleep(0.15)")
        .execute(&pool)
        .await
        .unwrap();
    let successor = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_epoch=2,lease_token=$2,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(successor).execute(&pool).await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(successor.to_string()).execute(&mut *tx).await.unwrap();
    for expected in [true, false] {
        assert_eq!(
            sqlx::query_scalar::<_, bool>("SELECT crm_family_refresh_reclaim($1,$2,$3,$4,2)")
                .bind(f.org)
                .bind(bundle)
                .bind(plan)
                .bind(abandoned)
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            expected
        );
    }
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_bytes FROM migration_history_capture_run WHERE id=$1"
        )
        .bind(capture)
        .fetch_one(&pool)
        .await
        .unwrap(),
        charged.get::<i64, _>("reserved_bytes")
    );
    let mut wrong = claim;
    wrong.token = Uuid::new_v4();
    assert!(
        history_index::index_page(&f.pool, &f.key, &wrong, &f.policy)
            .await
            .is_err()
    );
}

pub(super) async fn draft_history_refresh(
    pool: &PgPool,
    f: &import_support::Fixture,
    capture: Uuid,
) -> crm_api::domain::migration::family_refresh::cohort::Claim {
    use crm_api::domain::migration::family_refresh::cohort::{self, Claim};
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let bundle = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let control = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,history_capture_id,state,source_nonce,source_ciphertext) SELECT $1,organization_id,parent_import_id,parent_plan_id,source_account_id,$2,'fub-family-refresh-v1',id,'preparing',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex') FROM migration_history_capture_run WHERE id=$3")
        .bind(bundle).bind(f.actor).bind(capture).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,history_capture_id,nonce,ciphertext) SELECT $1,id,organization_id,'history',1,'preparing','cohort',history_capture_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2").bind(plan).bind(bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET payer_plan_id=$2 WHERE id=$1")
        .bind(bundle)
        .bind(plan)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(&mut *tx)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let claim = Claim {
        organization: f.ctx.organization_id,
        bundle,
        plan,
        token,
        epoch: 1,
    };
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();
    claim
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_baseline_authenticates_original_owner_and_body_only_corrections(pool: PgPool) {
    use crate::{db_history_capture_support as capture, db_history_import_support as original};
    use crm_api::domain::migration::{
        family_refresh::{
            core_source::Progress,
            history_baseline::{self, Discovery},
            history_index,
            history_resolution::{self, Resolution},
            model::{self, HistoryChange, Hold, Kind},
        },
        history_capture_source::Stream,
    };
    let (f, parent, first, book) = original::fixture(&pool).await;
    let root = original::ready(&f, parent, first).await;
    original::confirm(&f, root).await;
    original::drain(&f).await;
    book.set_records(Stream::Events,vec![json!({"id":1,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"CHANGED_BODY_SENTINEL"}),json!({"id":50,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z"})]);
    let (newer, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, newer).await;
    capture::drain(&f, &book).await;
    let claim = draft_history_refresh(&pool, &f, newer).await;
    for i in 0..10 {
        if history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == Progress::Finished
        {
            break;
        }
        assert!(i < 9);
    }
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    for (kind, source, expected) in [
        (Kind::Event, "1", HistoryChange::Correction),
        (Kind::Call, "2", HistoryChange::AlreadyCurrent),
    ] {
        let selected = match history_resolution::resolve(&f.pool, &f.key, &claim, kind, source)
            .await
            .unwrap()
        {
            Resolution::Ready(r) => r,
            Resolution::Held(h) => panic!("unexpected hold {h:?}"),
        };
        let baseline = match history_baseline::discover(
            &f.pool,
            &f.key,
            &claim,
            cohort,
            kind,
            &selected.evidence.identity_hmac,
        )
        .await
        .unwrap()
        {
            Discovery::Proven(b) => b,
            Discovery::Held(h) => panic!("unexpected baseline hold {h:?}"),
        };
        assert_eq!(baseline.capture, first);
        assert_eq!(baseline.version, 1);
        assert!(baseline.result.is_none());
        assert_eq!(
            model::compare_history(
                Some((&baseline.semantic, baseline.person)),
                &selected.evidence.semantic_hmac,
                baseline.person,
                true,
                false,
                false
            ),
            expected
        );
        assert!(!selected
            .evidence
            .display
            .metadata()
            .to_string()
            .contains("SENTINEL"));
    }
    let selected = match history_resolution::resolve(&f.pool, &f.key, &claim, Kind::Event, "50")
        .await
        .unwrap()
    {
        Resolution::Ready(r) => r,
        _ => panic!("new source must resolve"),
    };
    assert!(
        matches!(
            history_baseline::discover(
                &f.pool,
                &f.key,
                &claim,
                cohort,
                Kind::Event,
                &selected.evidence.identity_hmac
            )
            .await
            .unwrap(),
            Discovery::Held(Hold::BaselineUnproven)
        ),
        "new identities never get a fabricated baseline"
    );
    assert_new_candidate(&f, &claim, cohort, Kind::Event, "50").await;
    assert_history_preparation(&pool, &f, &claim, cohort).await;
    assert!(history_baseline::discover(
        &f.pool,
        &f.key,
        &claim,
        Uuid::new_v4(),
        Kind::Event,
        &selected.evidence.identity_hmac
    )
    .await
    .is_err());
    assert!(matches!(
        history_resolution::resolve(&f.pool, &f.key, &claim, Kind::Event, "99999")
            .await
            .unwrap(),
        Resolution::Held(Hold::SourceNotObserved)
    ));
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_baseline_authenticates_admitted_owner(pool: PgPool) {
    use crate::{db_admitted_history as admitted, db_history_capture_support as capture};
    use crm_api::domain::migration::{
        family_refresh::{
            core_source::Progress,
            history_baseline::{self, Discovery},
            history_index,
            history_resolution::{self, Resolution},
            model::Kind,
        },
        history_capture_source::Stream,
    };
    let (f, admission, first) = admitted::fixture(&pool).await;
    let root = admitted::ready(&f, admission, first).await;
    admitted::confirm(&f, root).await;
    admitted::drain(&f).await;
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_history_capture_run WHERE id=$1",
    )
    .bind(first)
    .fetch_one(&pool)
    .await
    .unwrap();
    let book = capture::HistoryBook::new();
    book.set_records(Stream::Events, vec![json!({"id":82,"personId":104,"type":"Inquiry"}),json!({"id":81,"personId":104,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"NEW_PRIVATE_BODY"})]);
    let (newer, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, newer).await;
    capture::drain(&f, &book).await;
    let claim = draft_history_refresh(&pool, &f, newer).await;
    let anchor = sqlx::query("SELECT s.id,s.completed_at,h.started_at FROM migration_people_admission a JOIN migration_snapshot s ON s.id=a.confirmed_snapshot_id JOIN migration_history_capture_run h ON h.id=$2 WHERE a.id=$1").bind(admission).bind(newer).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE migration_snapshot SET completed_at=$2 WHERE id=$1")
        .bind(anchor.get::<Uuid, _>("id"))
        .bind(anchor.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"))
        .execute(&pool)
        .await
        .unwrap();
    for i in 0..10 {
        if history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == Progress::Finished
        {
            break;
        }
        assert!(i < 9);
    }
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='104' AND admission_id=$2").bind(claim.bundle).bind(admission).fetch_one(&pool).await.unwrap();
    let owned = sqlx::query("SELECT * FROM crm_family_refresh_next_owned_history($1,$2,NULL)")
        .bind(f.org)
        .bind(claim.bundle)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        owned.get::<Uuid, _>("cohort_id"),
        cohort,
        "admitted owners use their exact frozen cohort"
    );
    let owner=sqlx::query("SELECT family,person_id,identity_hmac FROM migration_history_import_identity WHERE id=$1 AND organization_id=$2").bind(owned.get::<Uuid,_>("identity_id")).bind(f.org).fetch_one(&pool).await.unwrap();
    let expected = match owner.get::<String, _>("family").as_str() {
        "events" => "event",
        "calls" => "call",
        "text_messages" => "text",
        other => panic!("unexpected history family {other}"),
    };
    assert_eq!(owned.get::<String, _>("kind"), expected);
    assert_eq!(
        owned.get::<Uuid, _>("person_id"),
        owner.get::<Uuid, _>("person_id")
    );
    assert_eq!(
        owned.get::<Vec<u8>, _>("identity_hmac"),
        owner.get::<Vec<u8>, _>("identity_hmac")
    );
    use crm_api::domain::migration::family_refresh::{model::Hold, new_identity};
    assert!(matches!(
        new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Event, "82")
            .await
            .unwrap(),
        new_identity::Discovery::Held(Hold::SourceNotNewer)
    ));
    let old_source = match history_resolution::resolve(&f.pool, &f.key, &claim, Kind::Event, "81")
        .await
        .unwrap()
    {
        Resolution::Ready(r) => r,
        _ => panic!("retained source must resolve"),
    };
    assert!(matches!(
        history_baseline::discover(
            &f.pool,
            &f.key,
            &claim,
            cohort,
            Kind::Event,
            &old_source.evidence.identity_hmac
        )
        .await
        .unwrap(),
        Discovery::Held(Hold::SourceNotNewer)
    ));
    let frozen_hold = assert_frozen_history_hold(&pool, &f, &claim, cohort).await;
    // Indexing succeeds independently of this cohort's late creation boundary.
    sqlx::query("UPDATE migration_snapshot SET completed_at=$2 WHERE id=$1")
        .bind(anchor.get::<Uuid, _>("id"))
        .bind(anchor.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"))
        .execute(&pool)
        .await
        .unwrap();
    assert_new_candidate(&f, &claim, cohort, Kind::Event, "82").await;
    use crm_api::domain::migration::family_refresh::history_plan::{self, Prepared};
    assert!(
        matches!(history_plan::prepare_unit(&f.pool,&f.key,&f.policy,&claim,cohort,Kind::Event,"82").await.unwrap(),Prepared::Unit(id) if id==frozen_hold),
        "newly eligible evidence does not rewrite this plan's immutable hold"
    );
    let held: String =
        sqlx::query_scalar("SELECT disposition FROM migration_family_refresh_manifest WHERE id=$1")
            .bind(frozen_hold)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(held, "held");

    let selected = match history_resolution::resolve(&f.pool, &f.key, &claim, Kind::Event, "81")
        .await
        .unwrap()
    {
        Resolution::Ready(r) => r,
        Resolution::Held(h) => panic!("unexpected source hold {h:?}"),
    };
    let baseline = match history_baseline::discover(
        &f.pool,
        &f.key,
        &claim,
        cohort,
        Kind::Event,
        &selected.evidence.identity_hmac,
    )
    .await
    .unwrap()
    {
        Discovery::Proven(b) => b,
        Discovery::Held(h) => panic!("unexpected admitted baseline hold {h:?}"),
    };
    assert_eq!(baseline.capture, first);
    assert_eq!(baseline.version, 1);
    assert_ne!(baseline.semantic, selected.evidence.semantic_hmac);
    assert!(!selected
        .evidence
        .display
        .metadata()
        .to_string()
        .contains("PRIVATE_BODY"));
    let owner: Uuid = sqlx::query_scalar(
        "SELECT admitted_root_id FROM migration_history_import_identity WHERE id=$1",
    )
    .bind(baseline.identity)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, root);
}

/// Migrator-only construction of successful refresh evidence; no native refresh
/// executor exists yet. The reader runs with the application role and real lease.
pub(super) async fn prior_native_fixture(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    proofs: Vec<crm_api::domain::migration::family_refresh::native_baseline::AfterState>,
) -> (
    crm_api::domain::migration::family_refresh::cohort::Claim,
    Uuid,
    Vec<Uuid>,
) {
    prior_native_fixture_with_source(pool, f, parent, claim, proofs, false).await
}

async fn prior_native_fixture_with_source(
    pool: &PgPool,
    f: &import_support::Fixture,
    parent: Uuid,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    mut proofs: Vec<crm_api::domain::migration::family_refresh::native_baseline::AfterState>,
    reuse_source: bool,
) -> (
    crm_api::domain::migration::family_refresh::cohort::Claim,
    Uuid,
    Vec<Uuid>,
) {
    use crm_api::domain::migration::family_refresh::{
        cohort::Claim,
        evidence::{Purpose, Scope},
        model::Family,
        native_baseline::{ResultData, State},
    };
    let report = if reuse_source {
        sqlx::query_scalar("SELECT core_report_id FROM migration_family_refresh_bundle WHERE id=$1")
            .bind(claim.bundle)
            .fetch_one(pool)
            .await
            .unwrap()
    } else {
        db_people_admission_execution::report(f, parent,
            vec![json!({"id":101,"firstName":"Synthetic successor","stage":"Lead","assignedUserId":3})]).await
    };
    let family: String =
        sqlx::query_scalar("SELECT family FROM migration_family_refresh_plan WHERE id=$1")
            .bind(claim.plan)
            .fetch_one(pool)
            .await
            .unwrap();
    let metadata = family == "metadata";
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
    let cohort: Uuid = sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&mut *tx).await.unwrap();
    let mut entries = Vec::new();
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: if metadata {
            Family::Metadata
        } else {
            Family::Activity
        },
        revision: 1,
    };
    for (position, proof) in proofs.iter_mut().enumerate() {
        let result = Uuid::new_v4();
        let (kind, revision) = match &mut proof.state {
            State::Metadata { snapshot, .. } => {
                snapshot.head = Some(result);
                ("metadata", snapshot.revision)
            }
            State::Note { native, .. } => ("note", native["revision"].as_i64().unwrap()),
            State::Task { native, .. } => ("task", native["revision"].as_i64().unwrap()),
        };
        let source_key = vec![position as u8 + 1; 32];
        sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,position,kind,source_key_hmac,person_id,target_id,disposition,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'already_current','{}',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'),8192)")
            .bind(proof.manifest).bind(claim.bundle).bind(claim.plan).bind(f.org).bind(cohort).bind(position as i64+1).bind(kind).bind(&source_key).bind(proof.person).bind(proof.target).execute(&mut *tx).await.unwrap();
        let sealed = scope
            .seal(
                &f.key,
                result,
                Purpose::Result,
                &ResultData {
                    after_state: Some(proof.clone()),
                },
            )
            .unwrap();
        entries.push((result, kind, revision, source_key, sealed));
    }
    sqlx::query("INSERT INTO migration_family_refresh_requirement(organization_id,capability,bundle_id) VALUES($1,'fub-family-refresh-v1',$2) ON CONFLICT DO NOTHING").bind(f.org).bind(claim.bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='running',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex') WHERE id=$1").bind(claim.bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='running',phase='apply',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex') WHERE id=$1").bind(claim.plan).execute(&mut *tx).await.unwrap();
    for (proof, (result, kind, revision, source_key, sealed)) in proofs.iter().zip(&entries) {
        sqlx::query("INSERT INTO migration_family_refresh_result(id,bundle_id,plan_id,organization_id,manifest_id,disposition,person_id,target_id,native_revision,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'already_current',$6,$7,$8,$9,$10)")
            .bind(result).bind(claim.bundle).bind(claim.plan).bind(f.org).bind(proof.manifest).bind(proof.person).bind(proof.target).bind(revision).bind(sealed.nonce.as_slice()).bind(&sealed.ciphertext).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO migration_family_refresh_head(organization_id,source_account_id,kind,source_key_hmac,person_id,target_id,result_id,version) SELECT organization_id,source_account_id,$2,$3,$4,$5,$6,1 FROM migration_family_refresh_bundle WHERE id=$1")
            .bind(claim.bundle).bind(kind).bind(source_key).bind(proof.person).bind(proof.target).bind(result).execute(&mut *tx).await.unwrap();
    }
    let terminal = if proofs.is_empty() {
        "cancelled"
    } else {
        "completed"
    };
    sqlx::query("UPDATE migration_family_refresh_plan SET state=$2,lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(claim.plan).bind(terminal).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET state=$2,updated_at=clock_timestamp() WHERE id=$1").bind(claim.bundle).bind(terminal).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT crm_family_refresh_settle(organization_id,bundle_id,plan_id,token,$2,true) FROM migration_family_refresh_reservation WHERE plan_id=$1 AND purpose='control'").bind(claim.plan).bind(claim.epoch).execute(&mut *tx).await.unwrap();
    let bundle = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let token = Uuid::new_v4();
    let successor_cohort = Uuid::new_v4();
    let control = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,core_report_id,core_snapshot_id,state,source_nonce,source_ciphertext) SELECT $1,r.organization_id,r.parent_import_id,r.parent_plan_id,r.source_account_id,$2,'fub-family-refresh-v1',r.id,r.newer_snapshot_id,'preparing',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex') FROM migration_core_change_report r WHERE r.id=$3 AND r.organization_id=$4")
        .bind(bundle).bind(f.actor).bind(report).bind(f.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,nonce,ciphertext) SELECT $1,id,organization_id,$3,1,'preparing','classify',core_snapshot_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2")
        .bind(plan).bind(bundle).bind(&family).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET payer_plan_id=$2 WHERE id=$1")
        .bind(bundle)
        .bind(plan)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,admission_id,admission_result_id,creation_snapshot_id) SELECT $1,$2,organization_id,source_person_id,person_id,original_result_id,admission_id,admission_result_id,creation_snapshot_id FROM migration_family_refresh_cohort WHERE id=$3")
        .bind(successor_cohort).bind(bundle).bind(cohort).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(plan).bind(token).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,16384,'control',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(bundle)
    .bind(plan)
    .bind(control)
    .fetch_one(&mut *tx)
    .await
    .unwrap());
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,1,false)")
        .bind(f.org)
        .bind(bundle)
        .bind(plan)
        .bind(control)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (
        Claim {
            organization: claim.organization,
            bundle,
            plan,
            token,
            epoch: 1,
        },
        successor_cohort,
        entries.iter().map(|v| v.0).collect(),
    )
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn prior_refresh_activity_baselines_are_authenticated_and_revision_fenced(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_baseline::{self, Discovery},
            model::{Hold, Kind},
            native_baseline::{AfterState, State},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
    let note = json!({"id":11,"personId":101,"createdById":3,"body":"Synthetic initial note","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null,"type":"Note"});
    book.set_records(Stream::Notes, vec![note.clone()]);
    book.set_raw(
        Stream::NoteDetail,
        0,
        200,
        serde_json::to_vec(&note).unwrap(),
        false,
    );
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (child, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, child).await;
    let ready = db_activity_source::replan(&f, child, &ready, choices, None).await;
    db_activity_source::confirm(&f, child, &ready).await;
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    // Construct a later native state to distinguish it from the first-import
    // result. The future executor must produce this evidence transactionally.
    sqlx::query("UPDATE note SET body='Synthetic refreshed note' WHERE organization_id=$1")
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE task SET title='Synthetic refreshed task' WHERE organization_id=$1")
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    let mut proofs = Vec::new();
    for (kind, table) in [(Kind::Note, "note"), (Kind::Task, "task")] {
        let rows=sqlx::query(&format!("SELECT to_jsonb(n) AS native,i.source_id FROM {table} n JOIN migration_activity_identity i ON i.target_id=n.id AND i.organization_id=n.organization_id AND i.kind=$2 WHERE n.organization_id=$1"))
            .bind(f.org).bind(table).fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        let native: serde_json::Value = row.get("native");
        proofs.push(AfterState {
            version: 1,
            manifest: Uuid::new_v4(),
            source_id: row.get("source_id"),
            person: Uuid::parse_str(native["person_id"].as_str().unwrap()).unwrap(),
            target: Uuid::parse_str(native["id"].as_str().unwrap()).unwrap(),
            state: if kind == Kind::Note {
                State::Note {
                    native,
                    owned: true,
                }
            } else {
                State::Task {
                    native,
                    owned: true,
                }
            },
        });
    }
    let (next, cohort, results) =
        prior_native_fixture(&pool, &f, parent, &claim, proofs.clone()).await;
    for (index, kind) in [Kind::Note, Kind::Task].into_iter().enumerate() {
        for _ in 0..2 {
            let b = match activity_baseline::discover(
                &f.pool,
                &f.key,
                &next,
                cohort,
                kind,
                &proofs[index].source_id,
            )
            .await
            .unwrap()
            {
                Discovery::Proven(b) => b,
                Discovery::Held(h) => panic!("{h:?}"),
            };
            assert_eq!(b.revision, 2);
            assert_eq!(b.result_id, results[index]);
            assert_eq!(b.head_id, Some(results[index]));
        }
    }
    assert!(
        activity_baseline::discover(&f.pool, &f.key, &next, Uuid::new_v4(), Kind::Task, "21")
            .await
            .is_err()
    );
    let mut wrong = crm_api::domain::migration::family_refresh::cohort::Claim { ..next };
    wrong.token = Uuid::new_v4();
    assert!(
        activity_baseline::discover(&f.pool, &f.key, &wrong, cohort, Kind::Task, "21")
            .await
            .is_err()
    );
    wrong = crm_api::domain::migration::family_refresh::cohort::Claim { ..next };
    wrong.organization = crm_api::ids::OrganizationId::new(Uuid::new_v4());
    assert!(
        activity_baseline::discover(&f.pool, &f.key, &wrong, cohort, Kind::Task, "21")
            .await
            .is_err()
    );
    // Native migrator mutation exercises the database revision trigger, including ABA.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE task SET title=title||' local' WHERE id=$1")
        .bind(proofs[1].target)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE task SET title=left(title,length(title)-6) WHERE id=$1")
        .bind(proofs[1].target)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        activity_baseline::discover(&f.pool, &f.key, &next, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Discovery::Held(Hold::LocalChange)
    ));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_result WHERE bundle_id=$1",
    )
    .bind(next.bundle)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "read-only discovery/replay must not advance a baseline"
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn prior_refresh_unowned_head_cannot_fall_back_to_first_import(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_baseline::{self, Discovery},
            model::{Hold, Kind},
            native_baseline::{AfterState, State},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (child, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, child).await;
    let ready = db_activity_source::replan(&f, child, &ready, choices, None).await;
    db_activity_source::confirm(&f, child, &ready).await;
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    let baseline =
        match activity_baseline::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap()
        {
            Discovery::Proven(b) => b,
            Discovery::Held(h) => panic!("{h:?}"),
        };
    let proof = AfterState {
        version: 1,
        manifest: Uuid::new_v4(),
        source_id: "21".into(),
        person: baseline.person_id,
        target: baseline.target_id,
        state: State::Task {
            native: baseline.native,
            owned: false,
        },
    };
    let (next, cohort, _) = prior_native_fixture(&pool, &f, parent, &claim, vec![proof]).await;
    assert!(
        matches!(
            activity_baseline::discover(&f.pool, &f.key, &next, cohort, Kind::Task, "21")
                .await
                .unwrap(),
            Discovery::Held(Hold::BaselineUnproven)
        ),
        "equality and a usable older result cannot confer ownership after an unproven newer head"
    );
    assert!(
        matches!(
            activity_baseline::discover(
                &f.pool,
                &crm_api::config::RawPayloadKey::new([82; 32]),
                &next,
                cohort,
                Kind::Task,
                "21"
            )
            .await,
            Err(crm_api::domain::migration::MigrationError::Crypto)
        ),
        "corrupted or foreign result evidence must not fall back either"
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn accepted_scan_survives_zero_write_cancel_without_advancing_a_baseline(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_baseline::{self, Discovery},
            model::{Hold, Kind},
        },
        snapshot_source::Stream,
    };
    for reuse in [false, true] {
        let book = db_activity_source::book();
        book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
        let f = import_support::fixture_with_book(&pool, book).await;
        let parent = db_activity_source::completed_parent(&f).await;
        let (child, ready) = db_activity_source::prepare(&f, parent).await;
        let choices = db_activity_source::choices(&f, child).await;
        let ready = db_activity_source::replan(&f, child, &ready, choices, None).await;
        db_activity_source::confirm(&f, child, &ready).await;
        let prior = prepared_indexed_refresh(&pool, &f, parent).await;
        let (next, cohort, results) =
            prior_native_fixture_with_source(&pool, &f, parent, &prior, vec![], reuse).await;
        assert!(results.is_empty());
        let heads: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=$1",
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(heads, 0, "a cancelled scan is not an applied baseline");
        let previous=sqlx::query("SELECT p.source_snapshot_id,s.completed_at,b.state FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id JOIN migration_snapshot s ON s.id=p.source_snapshot_id WHERE p.id=$1").bind(prior.plan).fetch_one(&pool).await.unwrap();
        assert_eq!(previous.get::<String, _>("state"), "cancelled");
        let found = activity_baseline::discover(&f.pool, &f.key, &next, cohort, Kind::Task, "21")
            .await
            .unwrap();
        if reuse {
            assert!(
                matches!(found, Discovery::Held(Hold::SourceNotNewer)),
                "the same accepted capture is reserved for exact unfinished remainders"
            );
            continue;
        }
        let baseline = match found {
            Discovery::Proven(b) => b,
            Discovery::Held(h) => panic!("{h:?}"),
        };
        assert!(
            baseline.head_id.is_none(),
            "the unchanged first-import result remains the baseline"
        );
        let selected=sqlx::query("SELECT s.id,s.started_at FROM migration_snapshot s JOIN migration_family_refresh_plan p ON p.source_snapshot_id=s.id WHERE p.id=$1").bind(next.plan).fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
            .bind(selected.get::<Uuid, _>("id"))
            .bind(previous.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            matches!(
                activity_baseline::discover(&f.pool, &f.key, &next, cohort, Kind::Task, "21")
                    .await
                    .unwrap(),
                Discovery::Held(Hold::SourceNotNewer)
            ),
            "ordering uses the accepted scan, not only the older applied result"
        );
        sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
            .bind(selected.get::<Uuid, _>("id"))
            .bind(selected.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            activity_baseline::discover(&f.pool, &f.key, &next, cohort, Kind::Task, "21")
                .await
                .unwrap(),
            Discovery::Proven(_)
        ));
    }
}

async fn assert_new_candidate(
    f: &import_support::Fixture,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    cohort: Uuid,
    kind: crm_api::domain::migration::family_refresh::model::Kind,
    source: &str,
) {
    use crm_api::domain::migration::family_refresh::new_identity::{self, Discovery};
    for _ in 0..2 {
        match new_identity::discover(&f.pool, &f.key, claim, cohort, kind, source)
            .await
            .unwrap()
        {
            Discovery::New(c) => {
                let expected: Uuid = sqlx::query_scalar(
                    "SELECT person_id FROM migration_family_refresh_cohort WHERE id=$1",
                )
                .bind(cohort)
                .fetch_one(&f.pool)
                .await
                .unwrap();
                assert_eq!(c.person, expected);
                assert_eq!(c.kind, kind);
                assert!(!c.source.is_nil());
            }
            Discovery::Held(h) => panic!("new identity held: {h:?}"),
        }
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_new_activity_requires_exact_first_coverage(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            model::{Hold, Kind},
            new_identity::{self, Discovery},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (child, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, child).await;
    let ready = db_activity_source::replan(&f, child, &ready, choices, None).await;
    db_activity_source::confirm(&f, child, &ready).await;
    f.reader.set_records(
        Stream::TasksOpen,
        vec![db_activity_source::task(21), db_activity_source::task(22)],
    );
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let cohort: Uuid = sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    assert_new_candidate(&f, &claim, cohort, Kind::Task, "22").await;
    assert!(matches!(
        new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Discovery::Held(Hold::BaselineUnproven)
    ));
    assert!(matches!(
        new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "999")
            .await
            .unwrap(),
        Discovery::Held(Hold::SourceNotObserved)
    ));
    let end: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM migration_activity_import WHERE id=$1")
            .bind(child)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE migration_activity_import SET completed_at=clock_timestamp()+interval '1 minute' WHERE id=$1").bind(child).execute(&pool).await.unwrap();
    assert!(matches!(
        new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "22")
            .await
            .unwrap(),
        Discovery::Held(Hold::FirstCoverageRequired)
    ));
    sqlx::query("UPDATE migration_activity_import SET completed_at=$2 WHERE id=$1")
        .bind(child)
        .bind(end)
        .execute(&pool)
        .await
        .unwrap();
    assert_new_candidate(&f, &claim, cohort, Kind::Task, "22").await;
    let new_identities: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1 AND source_id='22'").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(
        new_identities, 0,
        "read-only discovery never consumes an identity"
    );
    // A native row without registry ownership cannot be acquired by equality.
    // Keep both legacy and account-scoped keys, including native tombstones.
    let person: Uuid =
        sqlx::query_scalar("SELECT person_id FROM migration_family_refresh_cohort WHERE id=$1")
            .bind(cohort)
            .fetch_one(&pool)
            .await
            .unwrap();
    for key in [
        "22".to_owned(),
        format!(
            "v1:{}:22",
            sqlx::query_scalar::<_, i64>(
                "SELECT source_account_id FROM migration_family_refresh_bundle WHERE id=$1"
            )
            .bind(claim.bundle)
            .fetch_one(&pool)
            .await
            .unwrap()
        ),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,created_by_user_id,assignee_user_id,origin,correlation_id,source,source_external_id) VALUES($1,$2,$3,'Local task','call',$4,$4,'web_session',$5,'fub',$6)")
            .bind(id).bind(f.org).bind(person).bind(f.actor).bind(Uuid::new_v4()).bind(key).execute(&pool).await.unwrap();
        assert!(matches!(
            new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "22")
                .await
                .unwrap(),
            Discovery::Held(Hold::BaselineUnproven)
        ));
        sqlx::query("UPDATE task SET title='',deleted_at=clock_timestamp(),deleted_by_user_id=$2 WHERE id=$1").bind(id).bind(f.actor).execute(&pool).await.unwrap();
        assert!(matches!(
            new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "22")
                .await
                .unwrap(),
            Discovery::Held(Hold::BaselineUnproven)
        ));
        sqlx::query("DELETE FROM task WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_new_activity_without_first_coverage_is_held(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            model::{Hold, Kind},
            new_identity::{self, Discovery},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(22)]);
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let cohort: Uuid = sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    assert!(matches!(
        new_identity::discover(&f.pool, &f.key, &claim, cohort, Kind::Task, "22")
            .await
            .unwrap(),
        Discovery::Held(Hold::FirstCoverageRequired)
    ));
    assert!(
        new_identity::discover(&f.pool, &f.key, &claim, Uuid::new_v4(), Kind::Task, "22")
            .await
            .is_err()
    );
    let mut foreign = claim;
    foreign.organization = crm_api::ids::OrganizationId::new(Uuid::new_v4());
    assert!(
        new_identity::discover(&f.pool, &f.key, &foreign, cohort, Kind::Task, "22")
            .await
            .is_err()
    );
}

async fn assert_history_preparation(
    pool: &PgPool,
    f: &import_support::Fixture,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    cohort: Uuid,
) {
    use crm_api::domain::migration::family_refresh::{
        evidence::{Purpose, Scope},
        history_plan::{self, Prepared, Proposal},
        model::{Counts, Family, Kind},
    };
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: 1,
    };
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert!(matches!(
        history_plan::prepare_unit(&f.pool, &f.key, &tiny, claim, cohort, Kind::Event, "1")
            .await
            .unwrap(),
        Prepared::Capacity
    ));
    let before: serde_json::Value = sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'measured',measured_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION test_history_plan_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.position>OLD.position THEN RAISE EXCEPTION 'synthetic history plan fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_history_plan_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_history_plan_fault()")
        .execute(pool).await.unwrap();
    assert!(history_plan::prepare_unit(
        &f.pool,
        &f.key,
        &f.policy,
        claim,
        cohort,
        Kind::Event,
        "1"
    )
    .await
    .is_err());
    sqlx::raw_sql("DROP TRIGGER test_history_plan_fault ON migration_family_refresh_plan; DROP FUNCTION test_history_plan_fault()")
        .execute(pool).await.unwrap();
    let after: serde_json::Value = sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'measured',measured_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    assert_eq!(
        after, before,
        "failed unit rolls back both progress and accounting"
    );
    let manifests: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1",
    )
    .bind(claim.plan)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(manifests, 0);
    let mut original_cursor = None;
    for (kind, source, expected) in [
        (Kind::Event, "1", "correction"),
        (Kind::Call, "2", "already_current"),
        (Kind::Event, "50", "insert"),
    ] {
        let id = match history_plan::prepare_unit(
            &f.pool, &f.key, &f.policy, claim, cohort, kind, source,
        )
        .await
        .unwrap()
        {
            Prepared::Unit(id) => id,
            _ => panic!("expected persisted history unit"),
        };
        let row = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("disposition"), expected);
        let p: Proposal = scope
            .open(
                &f.key,
                id,
                Purpose::Manifest,
                row.get("nonce"),
                row.get("ciphertext"),
            )
            .unwrap();
        assert_eq!(p.source_id, source);
        assert_eq!(p.target, row.get::<Uuid, _>("target_id"));
        assert_eq!(p.baseline.is_none(), expected == "insert");
        assert_eq!(p.version_id.is_some(), expected == "correction");
        assert!(!p.source.display.metadata().to_string().contains("SENTINEL"));
        let bytes: i64 = sqlx::query_scalar(
            "SELECT retained_bytes FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(claim.plan)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(
            matches!(history_plan::prepare_unit(&f.pool,&f.key,&f.policy,claim,cohort,kind,source).await.unwrap(),Prepared::Unit(replay) if replay==id)
        );
        let after: i64 = sqlx::query_scalar(
            "SELECT retained_bytes FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(claim.plan)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(after, bytes, "replay does not charge or allocate again");
        if source == "2" {
            use crm_api::domain::migration::family_refresh::item_queries;
            original_cursor = item_queries::items(
                &f.pool,
                &f.key,
                &f.ctx,
                claim.bundle,
                item_query(claim.plan, None),
            )
            .await
            .unwrap()
            .next_cursor;
            assert!(original_cursor.is_some());
        }
    }
    assert_item_pages(pool, f, claim, cohort, original_cursor.unwrap()).await;
    let row = sqlx::query("SELECT counts,position,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    let counts: Counts = serde_json::from_value(row.get("counts")).unwrap();
    assert_eq!(row.get::<i64, _>("position"), 3);
    assert_eq!(counts.units, 3);
    assert_eq!(counts.inserts, 1);
    assert_eq!(counts.already_current, 1);
    assert_eq!(counts.history_corrections, 1);
    assert!(counts.reconciles());
    assert_eq!(
        row.get::<i64, _>("retained_bytes"),
        row.get::<i64, _>("measured_bytes")
    );
    let native: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_family_refresh_result WHERE plan_id=$1")
            .bind(claim.plan)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(native, 0);
}

fn item_query(
    plan: Uuid,
    cursor: Option<String>,
) -> crm_api::domain::migration::family_refresh::item_queries::ItemPage {
    use crm_api::domain::migration::family_refresh::{item_queries::ItemPage, model::Family};
    ItemPage {
        family: Family::History,
        plan_id: plan,
        cohort_id: None,
        outcome: None,
        limit: Some(1),
        cursor,
    }
}
async fn assert_item_pages(
    pool: &PgPool,
    f: &import_support::Fixture,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    cohort: Uuid,
    original_cursor: String,
) {
    use crm_api::domain::migration::{
        family_refresh::item_queries::{self, Outcome},
        MigrationError,
    };
    let older = item_queries::items(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        item_query(claim.plan, Some(original_cursor)),
    )
    .await
    .unwrap();
    assert_eq!(older.items.len(), 1);
    assert_eq!(older.items[0].position, "2");
    assert!(
        older.next_cursor.is_none(),
        "cursor upper bound excludes later inserts"
    );
    let first = item_queries::items(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        item_query(claim.plan, None),
    )
    .await
    .unwrap();
    assert_eq!(first.items[0].position, "1");
    let cursor = first.next_cursor.unwrap();
    let second = item_queries::items(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        item_query(claim.plan, Some(cursor.clone())),
    )
    .await
    .unwrap();
    assert_eq!(second.items[0].position, "2");
    let third = item_queries::items(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        item_query(claim.plan, second.next_cursor),
    )
    .await
    .unwrap();
    assert_eq!(third.items[0].position, "3");
    assert!(third.next_cursor.is_none());
    let json = serde_json::to_string(&third).unwrap();
    assert!(json.len() < 4096 && !json.contains("SENTINEL") && !json.contains("ciphertext"));
    for scenario in 0..5 {
        let mut q = item_query(claim.plan, Some(cursor.clone()));
        match scenario {
            0 => q.limit = Some(2),
            1 => q.cohort_id = Some(cohort),
            2 => q.outcome = Some(Outcome::Insert),
            3 => q.cursor = Some(format!("{}x", cursor)),
            _ => q.limit = Some(51),
        }
        assert!(matches!(
            item_queries::items(&f.pool, &f.key, &f.ctx, claim.bundle, q).await,
            Err(MigrationError::InvalidInput)
        ));
    }
    let mut q = item_query(claim.plan, None);
    q.outcome = Some(Outcome::Insert);
    q.cohort_id = Some(cohort);
    let filtered = item_queries::items(&f.pool, &f.key, &f.ctx, claim.bundle, q)
        .await
        .unwrap();
    assert_eq!(filtered.items[0].position, "3");
    assert!(filtered.next_cursor.is_none());
    let mut other = f.ctx.clone();
    other.actor_user_id = crm_api::ids::UserId::new(f.member);
    assert!(matches!(
        item_queries::items(
            &f.pool,
            &f.key,
            &other,
            claim.bundle,
            item_query(claim.plan, None)
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(pool)
    .await
    .unwrap();
    assert!(
        matches!(
            item_queries::items(
                &f.pool,
                &f.key,
                &other,
                claim.bundle,
                item_query(claim.plan, Some(cursor.clone()))
            )
            .await,
            Err(MigrationError::InvalidInput)
        ),
        "another authorized actor cannot reuse this cursor"
    );
    assert!(item_queries::items(
        &f.pool,
        &f.key,
        &other,
        claim.bundle,
        item_query(claim.plan, None)
    )
    .await
    .is_ok());
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(pool)
    .await
    .unwrap();
    other.organization_id = crm_api::ids::OrganizationId::new(Uuid::new_v4());
    assert!(item_queries::items(
        &f.pool,
        &f.key,
        &other,
        claim.bundle,
        item_query(claim.plan, None)
    )
    .await
    .is_err());
    let mut q = item_query(claim.plan, None);
    q.cohort_id = Some(Uuid::new_v4());
    assert!(matches!(
        item_queries::items(&f.pool, &f.key, &f.ctx, claim.bundle, q).await,
        Err(MigrationError::NotFound)
    ));
    let foreign_org = crate::common::create_org(pool, "Foreign family item reader").await;
    crate::common::add_membership_with(
        pool,
        foreign_org,
        f.actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let mut foreign = f.ctx.clone();
    foreign.organization_id = crm_api::ids::OrganizationId::new(foreign_org);
    assert!(matches!(
        item_queries::items(
            &f.pool,
            &f.key,
            &foreign,
            claim.bundle,
            item_query(claim.plan, None)
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        matches!(
            item_queries::items(
                &f.pool,
                &f.key,
                &f.ctx,
                claim.bundle,
                item_query(claim.plan, Some(cursor.clone()))
            )
            .await,
            Err(MigrationError::InvalidInput)
        ),
        "workspace revision invalidates the item cursor"
    );
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision-1 WHERE id=$1")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET revision=revision+1 WHERE id=$1")
        .bind(claim.bundle)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        matches!(
            item_queries::items(
                &f.pool,
                &f.key,
                &f.ctx,
                claim.bundle,
                item_query(claim.plan, Some(cursor))
            )
            .await,
            Err(MigrationError::InvalidInput)
        ),
        "revision change invalidates prior cursor"
    );
}

async fn assert_frozen_history_hold(
    pool: &PgPool,
    f: &import_support::Fixture,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
    cohort: Uuid,
) -> Uuid {
    use crm_api::domain::migration::family_refresh::{
        evidence::{Purpose, Scope},
        history_hold::HeldProposal,
        history_plan::{self, Prepared},
        model::{Counts, Family, Hold, Kind},
    };
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert!(matches!(
        history_plan::prepare_unit(&f.pool, &f.key, &tiny, claim, cohort, Kind::Event, "82")
            .await
            .unwrap(),
        Prepared::Capacity
    ));
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION test_history_hold_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.position>OLD.position THEN RAISE EXCEPTION 'synthetic history hold fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_history_hold_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_history_hold_fault()")
        .execute(pool).await.unwrap();
    assert!(history_plan::prepare_unit(
        &f.pool,
        &f.key,
        &f.policy,
        claim,
        cohort,
        Kind::Event,
        "82"
    )
    .await
    .is_err());
    sqlx::raw_sql("DROP TRIGGER test_history_hold_fault ON migration_family_refresh_plan; DROP FUNCTION test_history_hold_fault()").execute(pool).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    assert_eq!(
        before, after,
        "failed hold rolls back its counts and charge"
    );
    let id = match history_plan::prepare_unit(
        &f.pool,
        &f.key,
        &f.policy,
        claim,
        cohort,
        Kind::Event,
        "82",
    )
    .await
    .unwrap()
    {
        Prepared::Unit(id) => id,
        _ => panic!("qualified held identity must persist"),
    };
    let r = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(r.get::<String, _>("disposition"), "held");
    assert_eq!(r.get::<String, _>("reason"), "source_not_newer");
    assert!(r.get::<Option<Uuid>, _>("target_id").is_none());
    assert!(r.get::<Option<Uuid>, _>("expected_head_id").is_none());
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: 1,
    };
    let proposal: HeldProposal = scope
        .open(
            &f.key,
            id,
            Purpose::Manifest,
            r.get("nonce"),
            r.get("ciphertext"),
        )
        .unwrap();
    assert_eq!(proposal.reason, Hold::SourceNotNewer);
    assert_eq!(proposal.source_id, "82");
    let counts: Counts = serde_json::from_value(r.get("counts")).unwrap();
    assert_eq!(counts.units, 1);
    assert_eq!(counts.held, 1);
    assert!(counts.reconciles());
    let row=sqlx::query("SELECT position,counts,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(pool).await.unwrap();
    assert_eq!(row.get::<i64, _>("position"), 1);
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        row.get::<serde_json::Value, _>("counts"),
        r.get::<serde_json::Value, _>("counts")
    );
    let units: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1",
    )
    .bind(claim.plan)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(units, 1);
    id
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_source_walk_is_atomic_bounded_and_counts_each_identity_once(pool: PgPool) {
    use crate::{db_history_capture_support as capture, db_history_import_support as original};
    use crm_api::domain::migration::{
        family_refresh::{
            core_source,
            evidence::{Purpose, Scope},
            history_index,
            history_walk::{self, Progress},
            model::{Counts, Family},
        },
        history_capture_source::Stream,
    };
    let (f, parent, first, book) = original::fixture(&pool).await;
    let root = original::ready(&f, parent, first).await;
    original::confirm(&f, root).await;
    original::drain(&f).await;
    book.set_records(Stream::Events,vec![
        json!({"id":1,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"WALK_CHANGED_PRIVATE"}),
        json!({"id":50,"personId":101,"type":"Inquiry"}),
        json!({"id":60,"personId":999,"type":"Inquiry"}),
        json!({"id":61}),
    ]);
    let (capture, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture).await;
    capture::drain(&f, &book).await;
    use crm_api::domain::migration::family_refresh::{cohort, commands, preparation_worker};
    commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        commands::PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(capture),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let claim = preparation_worker::claim_next(&f.pool)
        .await
        .unwrap()
        .unwrap();
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();

    for i in 0..10 {
        if history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == core_source::Progress::Finished
        {
            break;
        }
        assert!(i < 9);
    }
    // A retained equivalent occurrence must be visited, but cannot allocate a
    // second unit. Synthetic insertion uses migrator; the app cannot alter index.
    let row=sqlx::query("SELECT * FROM migration_family_refresh_source WHERE plan_id=$1 AND kind='event' AND source_id='1'").bind(claim.plan).fetch_one(&pool).await.unwrap();
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: 1,
    };
    let data: serde_json::Value = scope
        .open(
            &f.key,
            row.get("id"),
            Purpose::Source,
            row.get("nonce"),
            row.get("ciphertext"),
        )
        .unwrap();
    let copy = Uuid::new_v4();
    let sealed = scope.seal(&f.key, copy, Purpose::Source, &data).unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(claim.token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET phase='capture' WHERE id=$1")
        .bind(claim.plan)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,source_person_id,identity_hmac,semantic_hmac,qualified,reason,nonce,ciphertext,history_page_id) SELECT $2,bundle_id,plan_id,organization_id,capture_id,capture_sequence,ordinal+1000,representation,kind,source_id,source_person_id,identity_hmac,semantic_hmac,qualified,reason,$3,$4,history_page_id FROM migration_family_refresh_source WHERE id=$1")
        .bind(row.get::<Uuid,_>("id")).bind(copy).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET phase='mappings' WHERE id=$1")
        .bind(claim.plan)
        .execute(&mut *tx)
        .await
        .unwrap();
    // Settle the synthetic extra evidence with the same metering machinery.
    let reservation = Uuid::new_v4();
    let ok: bool =
        sqlx::query_scalar("SELECT crm_family_refresh_reserve($1,$2,$3,$4,$5,65536,'unit',$6,$7)")
            .bind(f.org)
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(reservation)
            .bind(claim.epoch)
            .bind(f.policy.run_ceiling_bytes)
            .bind(f.policy.org_ceiling_bytes)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(ok);
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(f.org)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for (order, expected) in [
        ("ASC", "outcome missing"),
        ("DESC", "skipped an occurrence"),
    ] {
        let endpoint:Uuid=sqlx::query_scalar(&format!("SELECT id FROM migration_family_refresh_source WHERE plan_id=$1 ORDER BY id {order} LIMIT 1"))
            .bind(claim.plan).fetch_one(&pool).await.unwrap();
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)")
            .bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
        let rejected =
            sqlx::query("UPDATE migration_family_refresh_plan SET checkpoint_id=$2 WHERE id=$1")
                .bind(claim.plan)
                .bind(endpoint)
                .execute(&mut *tx)
                .await
                .unwrap_err();
        assert!(
            rejected.to_string().contains(expected),
            "unexpected checkpoint rejection: {rejected}"
        );
        tx.rollback().await.unwrap();
    }
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'checkpoint',checkpoint_id,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert_eq!(
        history_walk::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        Progress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_walk_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id THEN RAISE EXCEPTION 'synthetic walk fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_walk_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_walk_fault()").execute(&pool).await.unwrap();
    assert!(history_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
        .await
        .is_err());
    sqlx::raw_sql("DROP TRIGGER test_walk_fault ON migration_family_refresh_plan; DROP FUNCTION test_walk_fault()").execute(&pool).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'checkpoint',checkpoint_id,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let mut steps = 0;
    loop {
        match history_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
        {
            Progress::Advanced => steps += 1,
            Progress::Finished => break,
            Progress::Capacity => panic!("unexpected capacity"),
        };
        assert!(steps < 20);
    }
    assert_eq!(
        steps, 7,
        "one step for every occurrence, including the repeat"
    );
    let p = sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE id=$1")
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap();
    let counts: Counts = serde_json::from_value(p.get("counts")).unwrap();
    assert!(counts.reconciles());
    assert_eq!(counts.units, 6);
    assert_eq!(counts.inserts, 1);
    assert_eq!(counts.excluded, 1);
    assert!(counts.held >= 1);
    assert_eq!(p.get::<i64, _>("position"), 6);
    assert!(p.get::<bool, _>("source_walk_complete"));
    assert_eq!(p.get::<String, _>("state"), "preparing");
    assert!(p.get::<Option<Vec<u8>>, _>("digest").is_none());
    assert_eq!(
        p.get::<i64, _>("measured_bytes"),
        p.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        history_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        Progress::Finished
    );
    let counts_after: serde_json::Value =
        sqlx::query_scalar("SELECT counts FROM migration_family_refresh_plan WHERE id=$1")
            .bind(claim.plan)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counts_after, p.get::<serde_json::Value, _>("counts"));
    let native: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_family_refresh_result WHERE plan_id=$1")
            .bind(claim.plan)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(native, 0);
    // Replacement plan revisions reuse the original bundle index and AEAD scope.
    let replacement = Uuid::new_v4();
    let next_token = Uuid::new_v4();
    let binding: serde_json::Value = scope
        .open(
            &f.key,
            claim.plan,
            Purpose::Binding,
            p.get("nonce"),
            p.get("ciphertext"),
        )
        .unwrap();
    let next_scope = Scope {
        plan: replacement,
        revision: 2,
        ..scope
    };
    let binding = next_scope
        .seal(&f.key, replacement, Purpose::Binding, &binding)
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='superseded',lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(claim.plan).execute(&mut *tx).await.unwrap();
    let old_control:Uuid=sqlx::query_scalar("SELECT token FROM migration_family_refresh_reservation WHERE plan_id=$1 AND purpose='control'").bind(claim.plan).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(f.org)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(old_control)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,history_capture_id,predecessor_plan_id,nonce,ciphertext) VALUES($1,$2,$3,'history',2,'preparing','mappings',$4,$5,$6,$7)")
        .bind(replacement).bind(claim.bundle).bind(f.org).bind(capture).bind(claim.plan).bind(binding.nonce.as_slice()).bind(binding.ciphertext).execute(&mut *tx).await.unwrap();
    let amount: i64 = sqlx::query_scalar(
        "SELECT measured_bytes+8192 FROM migration_family_refresh_plan WHERE id=$1",
    )
    .bind(replacement)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let control = Uuid::new_v4();
    let reserved: bool =
        sqlx::query_scalar("SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,$5,'control',$6,$7)")
            .bind(f.org)
            .bind(claim.bundle)
            .bind(replacement)
            .bind(control)
            .bind(amount)
            .bind(f.policy.run_ceiling_bytes)
            .bind(f.policy.org_ceiling_bytes)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(reserved);
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
        .bind(f.org)
        .bind(claim.bundle)
        .bind(replacement)
        .bind(control)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$2,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(replacement).bind(next_token).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let successor = crm_api::domain::migration::family_refresh::cohort::Claim {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: replacement,
        token: next_token,
        epoch: 1,
    };
    let mut visited = 0;
    loop {
        match history_walk::run_once(&f.pool, &f.key, &f.policy, &successor)
            .await
            .unwrap()
        {
            Progress::Advanced => visited += 1,
            Progress::Finished => break,
            _ => panic!("unexpected replacement capacity"),
        };
        assert!(visited < 20);
    }
    assert_eq!(visited, 7);
    let before_owned:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(replacement).fetch_one(&pool).await.unwrap();
    use crm_api::domain::migration::family_refresh::history_missing;
    for step in 0..10 {
        if history_missing::run_once(&f.pool, &f.key, &f.policy, &successor)
            .await
            .unwrap()
            == Progress::Finished
        {
            break;
        }
        assert!(step < 9);
    }
    let after_owned:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(replacement).fetch_one(&pool).await.unwrap();
    assert_eq!(
        before_owned, after_owned,
        "already observed identities advance the owned cursor without new units or charges"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_source WHERE plan_id=$1"
        )
        .bind(replacement)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(replacement)
        .fetch_one(&pool)
        .await
        .unwrap(),
        6
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn history_missing_walk_holds_absence_without_deleting_or_advancing_baselines(pool: PgPool) {
    use crate::{db_history_capture_support as capture, db_history_import_support as original};
    use crm_api::domain::migration::{
        family_refresh::{
            cohort, commands, core_source,
            evidence::{Purpose, Scope},
            history_index,
            history_missing::{self, MissingProposal},
            history_walk::{self, Progress},
            model::{Counts, Family, Hold},
            preparation_worker,
        },
        history_capture_source::Stream,
    };
    let (f, parent, first, book) = original::fixture(&pool).await;
    let root = original::ready(&f, parent, first).await;
    original::confirm(&f, root).await;
    original::drain(&f).await;
    let native_before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
        book.set_records(stream, vec![]);
    }
    let (capture, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture).await;
    capture::drain(&f, &book).await;
    commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        commands::PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(capture),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let claim = preparation_worker::claim_next(&f.pool)
        .await
        .unwrap()
        .unwrap();
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();
    for step in 0..10 {
        if history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == core_source::Progress::Finished
        {
            break;
        }
        assert!(step < 9);
    }
    assert!(
        history_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .is_err(),
        "source reconciliation must finish first"
    );
    assert_eq!(
        history_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        Progress::Finished
    );
    let owners=sqlx::query("SELECT id FROM migration_history_import_identity WHERE organization_id=$1 AND fact_id IS NOT NULL ORDER BY id").bind(f.org).fetch_all(&pool).await.unwrap();
    assert!(owners.len() >= 2);
    let first_owner: Uuid = owners[0].get("id");
    let last_owner: Uuid = owners.last().unwrap().get("id");
    assert!(
        sqlx::query("SELECT * FROM crm_family_refresh_next_owned_history($1,$2,NULL)")
            .bind(Uuid::new_v4())
            .bind(claim.bundle)
            .fetch_optional(&f.pool)
            .await
            .unwrap()
            .is_none(),
        "foreign Organization cannot enumerate owners"
    );
    for (next, complete, expected) in [
        (Some(first_owner), false, "outcome missing"),
        (Some(last_owner), false, "skipped an identity"),
        (None, true, "incomplete"),
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
        let error=sqlx::query("UPDATE migration_family_refresh_plan SET owned_after=$2,owned_walk_complete=$3 WHERE id=$1").bind(claim.plan).bind(next).bind(complete).execute(&mut *tx).await.unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        tx.rollback().await.unwrap();
    }
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'cursor',owned_after,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert_eq!(
        history_missing::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        Progress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_missing_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.owned_after IS DISTINCT FROM OLD.owned_after THEN RAISE EXCEPTION 'synthetic missing fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_missing_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_missing_fault()").execute(&pool).await.unwrap();
    assert!(
        history_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_missing_fault ON migration_family_refresh_plan; DROP FUNCTION test_missing_fault()").execute(&pool).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'cursor',owned_after,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "failure rolls back manifest, counts, cursor and settlement"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let mut advanced = 0;
    loop {
        match history_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
        {
            Progress::Advanced => advanced += 1,
            Progress::Finished => break,
            Progress::Capacity => panic!("unexpected capacity"),
        }
        assert!(advanced <= owners.len());
    }
    assert_eq!(advanced, owners.len());
    let p = sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE id=$1")
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap();
    let counts: Counts = serde_json::from_value(p.get("counts")).unwrap();
    assert_eq!(counts.units, owners.len() as u64);
    assert_eq!(counts.held, counts.units);
    assert!(counts.reconciles());
    assert!(p.get::<bool, _>("owned_walk_complete"));
    assert!(p.get::<Option<Vec<u8>>, _>("digest").is_none());
    assert_eq!(p.get::<String, _>("state"), "preparing");
    assert_eq!(
        p.get::<i64, _>("retained_bytes"),
        p.get::<i64, _>("measured_bytes")
    );
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: 1,
    };
    let manifests = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE plan_id=$1")
        .bind(claim.plan)
        .fetch_all(&pool)
        .await
        .unwrap();
    for m in manifests {
        assert_eq!(m.get::<String, _>("reason"), "source_not_observed");
        assert!(m.get::<Option<Uuid>, _>("source_row_id").is_none());
        assert!(m.get::<Option<Uuid>, _>("target_id").is_none());
        assert!(m.get::<Option<Uuid>, _>("expected_head_id").is_none());
        let proposal: MissingProposal = scope
            .open(
                &f.key,
                m.get("id"),
                Purpose::Manifest,
                m.get("nonce"),
                m.get("ciphertext"),
            )
            .unwrap();
        assert!(proposal.source_not_observed);
        assert_eq!(proposal.reason, Hold::SourceNotObserved);
        assert_eq!(proposal.baseline.unwrap().identity, proposal.identity);
    }
    assert_eq!(
        history_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        Progress::Finished
    );
    let native_after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(
        native_before, native_after,
        "native identity/fact ownership remains unchanged"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_history_head WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    preparation_worker::release(&f.pool, &claim).await.unwrap();
    assert!(preparation_worker::claim_next(&f.pool)
        .await
        .unwrap()
        .is_none());
}

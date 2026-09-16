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
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,nonce,ciphertext) SELECT $1,id,organization_id,'activity',1,'preparing','cohort',core_snapshot_id,source_nonce,source_ciphertext FROM migration_family_refresh_bundle WHERE id=$2")
        .bind(plan).bind(bundle).execute(pool).await.unwrap();
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
    sqlx::query("INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,source_person_id,semantic_hmac,qualified,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,1,0,'synthetic','task','1','101',decode(repeat('00',32),'hex'),true,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'))")
        .bind(Uuid::new_v4()).bind(bundle).bind(plan).bind(f.org).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query("SELECT 1/0").execute(&mut *tx).await.is_err());
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
    sqlx::query("INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,semantic_hmac,qualified,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,1,0,'synthetic','task','1',decode(repeat('00',32),'hex'),true,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'))")
        .bind(Uuid::new_v4()).bind(bundle).bind(plan).bind(f.org).bind(Uuid::new_v4()).execute(&mut *app_tx).await.unwrap();
    assert!(
        app_tx.commit().await.is_err(),
        "application cannot commit evidence without settling its charge"
    );
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

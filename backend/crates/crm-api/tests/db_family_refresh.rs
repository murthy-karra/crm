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
    sqlx::query("INSERT INTO migration_family_refresh_mapping(id,bundle_id,plan_id,organization_id,kind,source_key_hmac,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,'note_author',decode(repeat('00',32),'hex'),'hold',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'))")
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
    let claim = prepared_indexed_refresh(&pool, &f, parent).await;
    let bundle = claim.bundle;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='104'").bind(bundle).fetch_one(&pool).await.unwrap();
    for (kind, source) in [(Kind::Note, "11"), (Kind::Task, "21")] {
        assert!(matches!(
            activity_baseline::discover(&f.pool, &f.key, &claim, cohort, kind, source)
                .await
                .unwrap(),
            Discovery::Proven(_)
        ));
    }
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
    use crm_api::{
        domain::migration::family_refresh::{
            cohort::{self, Claim},
            core_source,
        },
        ids::OrganizationId,
    };
    let (bundle, plan, _) = draft_cohort(pool, f, parent, false).await;
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
}

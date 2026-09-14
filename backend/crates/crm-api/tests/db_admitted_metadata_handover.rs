//! TRUST/BOUNDARY: populated original metadata is handed over atomically;
//! pre-handover legacy permits cannot survive the committed registry boundary.
use crate::{db_metadata_import_gate, db_people_admission_execution, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_metadata as admitted, metadata, metadata_worker, people_admission,
        people_admission_worker, MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn fixture(migrator: &PgPool) -> (Fixture, Uuid, Value, admitted::Prepare) {
    let (f, parent, original, ready) = db_metadata_import_gate::fixture(migrator, false).await;
    let admission = db_people_admission_execution::ready(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Handover","stage":"Lead","assignedUserId":3,"tags":["Atomic Tag"],"customAtomic":"later value"})],
    )
    .await;
    let d = people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
        .await
        .unwrap();
    let p = &d["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        admission,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: Uuid::parse_str(p["id"].as_str().unwrap()).unwrap(),
            plan_revision: p["revision"].as_str().unwrap().parse().unwrap(),
            plan_digest: p["digest"].as_str().unwrap().into(),
            eligible_count: p["counts"]["eligible"].as_str().unwrap().parse().unwrap(),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    for _ in 0..30 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
            .await
            .unwrap()["state"]
            == "completed"
        {
            break;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    assert_eq!(
        people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
            .await
            .unwrap()["state"],
        "completed"
    );
    let source_report_id =
        sqlx::query_scalar("SELECT report_id FROM migration_people_admission WHERE id=$1")
            .bind(admission)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    metadata::confirm(&f.pool,&f.key,&f.ctx,original,serde_json::from_value(json!({
        "request_id":Uuid::new_v4(),"plan_id":ready["latest_plan"]["id"],
        "plan_revision":ready["latest_plan"]["revision"],"confirmation_digest":ready["latest_plan"]["confirmation_digest"],
        "workspace_revision":ready["workspace_revision"],
        "acknowledgments":{"held_count":ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}
    })).unwrap(),&ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    (
        f,
        original,
        ready,
        admitted::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id,
        },
    )
}
async fn finish_original(f: &Fixture, original: Uuid) {
    for _ in 0..30 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
    }
    let state: String =
        sqlx::query_scalar("SELECT state FROM migration_metadata_import WHERE id=$1")
            .bind(original)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "completed");
}
async fn ledgers(f: &Fixture, original: Uuid) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT i.retained_bytes,i.reserved_bytes,s.retained_bytes,s.reserved_bytes,l.retained_bytes,l.reserved_bytes FROM migration_metadata_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id JOIN migration_snapshot_storage l ON l.organization_id=i.organization_id WHERE i.id=$1")
        .bind(original).fetch_one(&f.pool).await.unwrap()
}
async fn original_evidence(f: &Fixture, original: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('identities',(SELECT jsonb_agg(to_jsonb(x) ORDER BY kind,source_key) FROM migration_metadata_identity x WHERE import_id=$1),'results',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM migration_metadata_result x WHERE import_id=$1),'plans',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM migration_metadata_plan x WHERE import_id=$1))")
        .bind(original).fetch_one(&f.pool).await.unwrap()
}
async fn prepare(f: &Fixture, command: &admitted::Prepare) -> Result<Value, MigrationError> {
    admitted::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        admitted::Prepare {
            request_id: command.request_id,
            admission_id: command.admission_id,
            source_report_id: command.source_report_id,
        },
    )
    .await
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn populated_metadata_handover_rolls_back_partial_claims_then_charges_once(migrator: PgPool) {
    let (f, original, _, command) = fixture(&migrator).await;
    finish_original(&f, original).await;
    let evidence = original_evidence(&f, original).await;
    assert_eq!(evidence["identities"].as_array().unwrap().len(), 2);
    let before = ledgers(&f, original).await;
    let source_calls = f.reader.calls();
    // Ordered backfill inserts/charges the field first. Failing the later tag
    // proves rollback of already-written claims AND all owner ledgers.
    sqlx::raw_sql("CREATE FUNCTION test_fail_metadata_handover() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='tag' THEN RAISE EXCEPTION USING ERRCODE='P0901',MESSAGE='injected_late_handover_failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_fail_metadata_handover BEFORE INSERT ON migration_metadata_catalog_claim FOR EACH ROW EXECUTE FUNCTION test_fail_metadata_handover();")
        .execute(&migrator).await.unwrap();
    let error = prepare(&f, &command).await.unwrap_err();
    match error {
        MigrationError::Database(error) => {
            let database = error.as_database_error().expect("injected database fault");
            assert_eq!(database.code().as_deref(), Some("P0901"));
            assert_eq!(database.message(), "injected_late_handover_failure");
        }
        error => panic!("unexpected failure: {error}"),
    }
    assert_eq!(ledgers(&f, original).await, before);
    let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM migration_metadata_catalog_claim),(SELECT count(*) FROM migration_metadata_catalog_readiness),(SELECT count(*) FROM migration_admitted_metadata_import),(SELECT count(*) FROM migration_admitted_metadata_receipt)")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
    assert_eq!(original_evidence(&f, original).await, evidence);
    sqlx::raw_sql("DROP TRIGGER test_fail_metadata_handover ON migration_metadata_catalog_claim; DROP FUNCTION test_fail_metadata_handover();")
        .execute(&migrator).await.unwrap();
    let receipt = prepare(&f, &command).await.unwrap();
    let root = Uuid::parse_str(receipt["import"]["id"].as_str().unwrap()).unwrap();
    let claim_bytes: i64 = sqlx::query_scalar("SELECT sum(octet_length(source_key)+octet_length(evidence_nonce)+octet_length(evidence_ciphertext))::bigint FROM migration_metadata_catalog_claim WHERE original_import_id=$1")
        .bind(original).fetch_one(&f.pool).await.unwrap();
    assert!(claim_bytes > 0);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_metadata_catalog_claim WHERE original_import_id=$1",
    )
    .bind(original)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    let child: (i64, i64) = sqlx::query_as(
        "SELECT retained_bytes,reserved_bytes FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let after = ledgers(&f, original).await;
    assert_eq!(after.0 - before.0, claim_bytes);
    assert_eq!(after.2 - before.2, claim_bytes);
    assert_eq!(after.4 - before.4, claim_bytes + child.0);
    assert_eq!((after.1, after.3), (before.1, before.3));
    assert_eq!(after.5 - before.5, child.1);
    let registry: Value = sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(c) ORDER BY kind,source_key) FROM migration_metadata_catalog_claim c")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(prepare(&f, &command).await.unwrap(), receipt);
    assert_eq!(ledgers(&f, original).await, after);
    assert_eq!(original_evidence(&f, original).await, evidence);
    assert_eq!(sqlx::query_scalar::<_,Value>("SELECT jsonb_agg(to_jsonb(c) ORDER BY kind,source_key) FROM migration_metadata_catalog_claim c").fetch_one(&f.pool).await.unwrap(),registry);
    assert_eq!(f.reader.calls(), source_calls);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT state='ready' FROM migration_metadata_catalog_readiness WHERE organization_id=$1"
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn active_legacy_metadata_lease_blocks_handover_and_old_permit_fails_afterward(
    migrator: PgPool,
) {
    let (f, original, ready, command) = fixture(&migrator).await;
    let plan = Uuid::parse_str(ready["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let row = sqlx::query(
        "SELECT id,target_id FROM migration_metadata_mapping WHERE plan_id=$1 AND kind='tag'",
    )
    .bind(plan)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let unit: Uuid = row.get("id");
    let target: Uuid = row.get("target_id");
    let token = Uuid::new_v4();
    // Migrator-only fixture models a lease acquired by the previous executable.
    sqlx::query("UPDATE migration_metadata_import SET state='running',lease_token=$2,lease_expires_at=clock_timestamp()+interval '1 minute' WHERE id=$1")
        .bind(original).bind(token).execute(&migrator).await.unwrap();
    let mut old = f.pool.acquire().await.unwrap();
    sqlx::query("SELECT set_config('crm.metadata_token',$1,false),set_config('crm.metadata_unit',$2,false),set_config('crm.metadata_claim_v1','',false)")
        .bind(token.to_string()).bind(unit.to_string()).execute(&mut *old).await.unwrap();
    let target_row = json!({"id":target,"organization_id":f.org,"created_by_user_id":f.actor,"name":"Atomic Tag"});
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT crm_metadata_insert_allowed($1,'tag',$2)")
            .bind(f.org)
            .bind(&target_row)
            .fetch_one(&mut *old)
            .await
            .unwrap()
    );
    assert!(matches!(
        prepare(&f, &command).await,
        Err(MigrationError::ImportBusy)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM migration_metadata_catalog_readiness")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    // Actual original worker recovery completes all retained units before the
    // successful barrier. Its committed identities are included in catch-up.
    sqlx::query("UPDATE migration_metadata_import SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(original).execute(&migrator).await.unwrap();
    finish_original(&f, original).await;
    prepare(&f, &command).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_metadata_catalog_claim WHERE original_import_id=$1"
        )
        .bind(original)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        2
    );
    // Restore an otherwise valid old lease in this disposable database to
    // isolate the new proof requirement from ordinary expiry/state rejection.
    // The existing old session still carries only its pre-handover GUCs.
    sqlx::query("UPDATE migration_metadata_import SET state='running',lease_token=$2,lease_expires_at=clock_timestamp()+interval '1 minute' WHERE id=$1")
        .bind(original).bind(token).execute(&migrator).await.unwrap();
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT crm_metadata_insert_allowed($1,'tag',$2)")
            .bind(f.org)
            .bind(&target_row)
            .fetch_one(&mut *old)
            .await
            .unwrap()
    );
    let error = sqlx::query(
        "INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,'Atomic Tag')",
    )
    .bind(target)
    .bind(f.org)
    .bind(f.actor)
    .execute(&mut *old)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("P010C")
    );
    sqlx::raw_sql("RESET crm.metadata_token; RESET crm.metadata_unit; RESET crm.metadata_claim_v1")
        .execute(&mut *old)
        .await
        .unwrap();
}

//! D-082 preparation accepts only an actual terminal admission cohort and
//! freezes the selected report's retained People evidence and local baseline.
use crate::db_admitted_people_refresh_execution::{fixture_with_admission, report};
use crm_api::{auth::workspace::ReleaseReadiness, domain::migration::admitted_metadata};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_preparation_freezes_terminal_people_cohort(migrator: PgPool) {
    let people = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"Person","stage":"Lead","assignedUserId":3,"tags":["Past Client"]}),
    ];
    let (fixture, parent, admission) = fixture_with_admission(&migrator, people.clone()).await;
    let selected = report(&fixture, parent, people).await;
    let value = admitted_metadata::prepare(
        &fixture.pool,
        &fixture.key,
        &ReleaseReadiness::for_tests(),
        &fixture.ctx,
        admitted_metadata::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id: selected,
        },
    )
    .await
    .unwrap();
    let root = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND disposition='eligible'").bind(root).bind(plan).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_source WHERE import_id=$1 AND family='people'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND kind='tag' AND disposition='held'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert!(sqlx::query_scalar::<_,i64>("SELECT (octet_length(baseline_nonce)+octet_length(baseline_ciphertext))::bigint FROM migration_admitted_metadata_manifest WHERE import_id=$1").bind(root).fetch_one(&fixture.pool).await.unwrap()>24);
}

use crate::import_support::Fixture;
use crm_api::domain::migration::{admitted_metadata_worker, snapshot_source::Stream};
use serde_json::Value;

async fn prepared_typed(migrator: &PgPool) -> (Fixture, Uuid, Uuid, Uuid) {
    let people = vec![
        json!({"id":104,"firstName":"Typed","lastName":"Admission","stage":"Lead","assignedUserId":3,"tags":["Past Client"],"customText":"Exact retained text","customNumber":123456.125,"customDate":"2024-02-29","customChoice":"North"}),
    ];
    let (f, parent, admission) = fixture_with_admission(migrator, people.clone()).await;
    f.reader.set_records(Stream::CustomFields,vec![
        json!({"id":21,"name":"customText","label":"Text","type":"text","isRecurring":false}),
        json!({"id":22,"name":"customNumber","label":"Number","type":"number","isRecurring":false}),
        json!({"id":23,"name":"customDate","label":"Date","type":"date","isRecurring":false}),
        json!({"id":24,"name":"customChoice","label":"Choice","type":"dropdown","isRecurring":false,"choices":["North","South"]}),
    ]);
    let selected = report(&f, parent, people).await;
    let v = admitted_metadata::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        admitted_metadata::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id: selected,
        },
    )
    .await
    .unwrap();
    let root = Uuid::parse_str(v["import"]["id"].as_str().unwrap()).unwrap();
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let person: Uuid = sqlx::query_scalar(
        "SELECT person_id FROM migration_admitted_metadata_manifest WHERE import_id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(sqlx::query_scalar::<_,bool>("SELECT i.snapshot_id=r.newer_snapshot_id AND i.capture_sequence=r.newer_sequence FROM migration_admitted_metadata_import i JOIN migration_core_change_report r ON r.id=i.source_report_id WHERE i.id=$1").bind(root).fetch_one(&f.pool).await.unwrap());
    (f, root, plan, person)
}
async fn approve_all(f: &Fixture, root: Uuid, plan: Uuid) {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM migration_admitted_metadata_mapping WHERE import_id=$1 ORDER BY kind,id",
    )
    .bind(root)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            mappings: ids
                .into_iter()
                .map(|id| admitted_metadata::MappingChoice {
                    id,
                    disposition: "create_matching".into(),
                    target_id: None,
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    admitted_metadata::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            plan_revision: 1,
        },
    )
    .await
    .unwrap();
}
async fn finish(f: &Fixture, root: Uuid) {
    for _ in 0..30 {
        let state: String =
            sqlx::query_scalar("SELECT state FROM migration_admitted_metadata_import WHERE id=$1")
                .bind(root)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        if state == "completed" {
            return;
        }
        assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
            .await
            .unwrap());
    }
    panic!("admitted worker failed to complete bounded fixture")
}
async fn ledger(f: &Fixture, root: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('root_retained',i.retained_bytes,'root_reserved',i.reserved_bytes,'snapshot_retained',s.retained_bytes,'snapshot_reserved',s.reserved_bytes,'org_retained',l.retained_bytes,'org_reserved',l.reserved_bytes,'reservations',(SELECT count(*) FROM migration_admitted_metadata_reservation WHERE import_id=i.id),'results',(SELECT count(*) FROM migration_admitted_metadata_result WHERE import_id=i.id),'claims',(SELECT count(*) FROM migration_metadata_catalog_claim WHERE admitted_import_id=i.id),'checkpoint',i.checkpoint_id) FROM migration_admitted_metadata_import i JOIN migration_snapshot s ON s.id=i.snapshot_id JOIN migration_snapshot_storage l ON l.organization_id=i.organization_id WHERE i.id=$1").bind(root).fetch_one(&f.pool).await.unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_typed_units_preserve_types_and_settle_together(migrator: PgPool) {
    let (f, root, plan, person) = prepared_typed(&migrator).await;
    let original:Value=sqlx::query_scalar("SELECT jsonb_build_object('person',to_jsonb(p),'identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY source_id) FROM migration_import_identity i WHERE organization_id=p.organization_id)) FROM person p WHERE id=$1").bind(person).fetch_one(&f.pool).await.unwrap();
    let before = ledger(&f, root).await;
    approve_all(&f, root, plan).await;
    let calls = f.reader.calls();
    finish(&f, root).await;
    assert_eq!(
        f.reader.calls(),
        calls,
        "execution uses retained evidence only"
    );
    let values:Value=sqlx::query_scalar("SELECT jsonb_object_agg(f.external_key,jsonb_build_object('type',v.field_type,'text',v.text_value,'number',v.number_value::text,'date',v.date_value::text,'choice',o.label,'origin',v.origin)) FROM person_custom_field_value v JOIN custom_field f ON f.id=v.field_id LEFT JOIN custom_field_option o ON o.id=v.option_id WHERE v.person_id=$1").bind(person).fetch_one(&f.pool).await.unwrap();
    assert_eq!(values["customText"]["text"], "Exact retained text");
    assert_eq!(values["customNumber"]["number"], "123456.1250");
    assert_eq!(values["customDate"]["date"], "2024-02-29");
    assert_eq!(values["customChoice"]["choice"], "North");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_tag WHERE person_id=$1")
            .bind(person)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let after = ledger(&f, root).await;
    assert_eq!(after["root_reserved"], 0);
    assert_eq!(after["reservations"], 0);
    assert_eq!(after["results"], 8); // four fields, two choices, one tag, one Person
    let child_delta =
        after["root_retained"].as_i64().unwrap() - before["root_retained"].as_i64().unwrap();
    assert_eq!(
        after["snapshot_retained"].as_i64().unwrap()
            - before["snapshot_retained"].as_i64().unwrap(),
        child_delta
    );
    assert_eq!(
        after["org_retained"].as_i64().unwrap() - before["org_retained"].as_i64().unwrap(),
        child_delta
    );
    let current:Value=sqlx::query_scalar("SELECT jsonb_build_object('person',to_jsonb(p),'identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY source_id) FROM migration_import_identity i WHERE organization_id=p.organization_id)) FROM person p WHERE id=$1").bind(person).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        current, original,
        "metadata never changes Person core/details revision or source identity"
    );
    assert!(!admitted_metadata_worker::run_once(&f.pool, &f.key)
        .await
        .unwrap());
    assert_eq!(
        ledger(&f, root).await,
        after,
        "completed work cannot be adopted or double-charged"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_result_failure_rolls_back_native_claim_checkpoint_and_bytes(
    migrator: PgPool,
) {
    let (f, root, plan, _) = prepared_typed(&migrator).await;
    approve_all(&f, root, plan).await;
    let before = ledger(&f, root).await;
    sqlx::raw_sql("CREATE FUNCTION admitted_result_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic unit result failure'; END $$; CREATE TRIGGER admitted_result_fault BEFORE INSERT ON migration_admitted_metadata_result FOR EACH ROW EXECUTE FUNCTION admitted_result_fault();").execute(&migrator).await.unwrap();
    assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
        .await
        .is_err());
    assert_eq!(
        ledger(&f, root).await,
        before,
        "claim/native/result/checkpoint/accounting must all roll back"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM custom_field WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_admitted_metadata_import WHERE id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "paused"
    );
    sqlx::query("DROP TRIGGER admitted_result_fault ON migration_admitted_metadata_result")
        .execute(&migrator)
        .await
        .unwrap();
    assert!(
        !admitted_metadata_worker::run_once(&f.pool, &f.key)
            .await
            .unwrap(),
        "repair never implicitly retries"
    );
    admitted_metadata::retry(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::Request {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    finish(&f, root).await;
}

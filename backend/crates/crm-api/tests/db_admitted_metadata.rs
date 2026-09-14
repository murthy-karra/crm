//! D-082 preparation accepts only an actual terminal admission cohort and
//! freezes the selected report's retained People evidence and local baseline.
use crate::db_admitted_people_refresh_execution::{fixture_with_admission, report};
use crm_api::{auth::workspace::ReleaseReadiness, domain::migration::admitted_metadata};
use serde_json::json;
use sqlx::{PgPool, Row};
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
    drain_preparation(&fixture, root).await;
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND disposition='eligible'").bind(root).bind(plan).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_source WHERE import_id=$1 AND family='people' AND source_id='104'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND kind='tag' AND disposition='held'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert!(sqlx::query_scalar::<_,i64>("SELECT (octet_length(baseline_nonce)+octet_length(baseline_ciphertext))::bigint FROM migration_admitted_metadata_manifest WHERE import_id=$1").bind(root).fetch_one(&fixture.pool).await.unwrap()>24);
}

use crate::import_support::Fixture;
use crm_api::domain::migration::{admitted_metadata_worker, snapshot_source::Stream};
use serde_json::Value;

async fn drain_preparation(f: &Fixture, root: Uuid) {
    for _ in 0..1000 {
        let ready:bool=sqlx::query_scalar("SELECT p.state='ready' FROM migration_admitted_metadata_plan p JOIN migration_admitted_metadata_import i ON i.latest_plan_id=p.id WHERE i.id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
        if ready {
            return;
        }
        assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
            .await
            .unwrap());
    }
    panic!("preparation did not finish");
}
async fn prepared_typed(migrator: &PgPool) -> (Fixture, Uuid, Uuid, Uuid) {
    prepared_typed_native(migrator, false).await
}
async fn prepared_typed_native(migrator: &PgPool, native: bool) -> (Fixture, Uuid, Uuid, Uuid) {
    let people = vec![
        json!({"id":104,"firstName":"Typed","lastName":"Admission","stage":"Lead","assignedUserId":3,"tags":["Past Client"],"customText":"Exact retained text","customNumber":123456.125,"customDate":"2024-02-29","customChoice":"North"}),
    ];
    let (f, parent, admission) = fixture_with_admission(migrator, people.clone()).await;
    if native {
        let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND disposition='settled'").bind(admission).fetch_one(&f.pool).await.unwrap();
        for (label, ty, position) in [
            ("Text", "text", 0),
            ("Number", "number", 1),
            ("Date", "date", 2),
        ] {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(f.org).bind(label).bind(ty).bind(position).bind(f.actor).execute(migrator).await.unwrap();
            if ty != "date" {
                sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,number_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,$4,$5,$6,$7,'web_session',$8)").bind(f.org).bind(person).bind(id).bind(ty).bind(if ty=="text"{Some("Exact retained text")}else{None}).bind(if ty=="number"{Some(999i64)}else{None}).bind(f.actor).bind(Uuid::new_v4()).execute(migrator).await.unwrap();
            }
        }
        let tag = Uuid::new_v4();
        sqlx::query("INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,'Past Client')").bind(tag).bind(f.org).bind(f.actor).execute(migrator).await.unwrap();
        sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) VALUES($1,$2,$3,$4)").bind(f.org).bind(person).bind(tag).bind(f.actor).execute(migrator).await.unwrap();
    }

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
    drain_preparation(&f, root).await;
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
async fn approve_all(f: &Fixture, root: Uuid, _plan: Uuid) -> Uuid {
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
            expected_plan_revision: "1".into(),
            source_report_id: None,
            mappings: ids
                .into_iter()
                .map(|id| crm_api::domain::migration::metadata::MappingPatch {
                    mapping_id: id,
                    choice: crm_api::domain::migration::metadata::Choice::CreateMatching,
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    drain_preparation(&f, root).await;
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    admitted_metadata::confirm(
        &f.pool,
        &f.key,
        Some(&ReleaseReadiness::for_tests()),
        &f.ctx,
        root,
        admitted_metadata::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            plan_revision: "2".into(),
            confirmation_digest: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["latest_plan"]["confirmation_digest"]
                .as_str()
                .unwrap()
                .to_owned(),
            workspace_revision: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["workspace_revision"]
                .as_str()
                .unwrap()
                .to_owned(),
            acknowledgments: crm_api::domain::migration::metadata::Acknowledgments {
                held_count: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                    ["latest_plan"]["counts"]["held_count"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                review_only: true,
                remaining_data: true,
            },
        },
    )
    .await
    .unwrap();
    plan
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
    let _plan = approve_all(&f, root, plan).await;
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
    assert_eq!(sqlx::query_scalar::<_,Value>("SELECT jsonb_agg(label ORDER BY position) FROM custom_field_option WHERE organization_id=$1").bind(f.org).fetch_one(&f.pool).await.unwrap(),json!(["North","South"]));
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
    let _plan = approve_all(&f, root, plan).await;
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
    finish_catalog(&f, root).await;
    let before_person = ledger(&f, root).await;
    sqlx::query("CREATE TRIGGER admitted_result_fault BEFORE INSERT ON migration_admitted_metadata_result FOR EACH ROW EXECUTE FUNCTION admitted_result_fault()").execute(&migrator).await.unwrap();
    assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
        .await
        .is_err());
    assert_eq!(
        ledger(&f, root).await,
        before_person,
        "all Person cells and their single result settle together"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_tag WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER admitted_result_fault ON migration_admitted_metadata_result")
        .execute(&migrator)
        .await
        .unwrap();
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

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_frozen_native_baselines_hold_changes_without_adoption(migrator: PgPool) {
    let (f, root, _plan, person) = prepared_typed_native(&migrator, true).await;
    let maps=sqlx::query("SELECT m.id,CASE WHEN m.kind='tag' THEN (SELECT id FROM tag WHERE organization_id=m.organization_id AND name='Past Client') WHEN m.source_id='21' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Text') WHEN m.source_id='22' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Number') WHEN m.source_id='23' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Date') END AS target FROM migration_admitted_metadata_mapping m WHERE m.import_id=$1 ORDER BY m.kind,m.id").bind(root).fetch_all(&f.pool).await.unwrap();
    admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            expected_plan_revision: "1".into(),
            source_report_id: None,
            mappings: maps
                .iter()
                .map(|m| {
                    let target = m.get::<Option<Uuid>, _>("target");
                    crm_api::domain::migration::metadata::MappingPatch {
                        mapping_id: m.get("id"),
                        choice: target
                            .map(|target_id| {
                                crm_api::domain::migration::metadata::Choice::MapExisting {
                                    target_id,
                                }
                            })
                            .unwrap_or(
                                crm_api::domain::migration::metadata::Choice::CreateMatching,
                            ),
                    }
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    drain_preparation(&f, root).await;
    // Edits after preview must be held even when incoming content is now equal.
    sqlx::query("DELETE FROM person_tag WHERE organization_id=$1 AND person_id=$2")
        .bind(f.org)
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,date_value,updated_by_user_id,origin,correlation_id) SELECT $1,$2,id,'date','2024-02-29',$3,'web_session',$4 FROM custom_field WHERE organization_id=$1 AND label='Date'").bind(f.org).bind(person).bind(f.actor).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
    drain_preparation(&f, root).await;
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    admitted_metadata::confirm(
        &f.pool,
        &f.key,
        Some(&ReleaseReadiness::for_tests()),
        &f.ctx,
        root,
        admitted_metadata::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            plan_revision: "2".into(),
            confirmation_digest: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["latest_plan"]["confirmation_digest"]
                .as_str()
                .unwrap()
                .to_owned(),
            workspace_revision: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["workspace_revision"]
                .as_str()
                .unwrap()
                .to_owned(),
            acknowledgments: crm_api::domain::migration::metadata::Acknowledgments {
                held_count: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                    ["latest_plan"]["counts"]["held_count"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                review_only: true,
                remaining_data: true,
            },
        },
    )
    .await
    .unwrap();
    finish(&f, root).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_tag WHERE person_id=$1")
            .bind(person)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0,
        "removed link is never resurrected"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_custom_field_value WHERE person_id=$1 AND origin='web_session'").bind(person).fetch_one(&f.pool).await.unwrap(),3,"equal cells never gain migration ownership");
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT number_value::text FROM person_custom_field_value WHERE person_id=$1 AND field_type='number'").bind(person).fetch_one(&f.pool).await.unwrap(),"999.0000");
    let row=sqlx::query("SELECT r.*,i.snapshot_id FROM migration_admitted_metadata_result r JOIN migration_admitted_metadata_import i ON i.id=r.import_id WHERE r.import_id=$1 AND r.kind='people'").bind(root).fetch_one(&f.pool).await.unwrap();
    let bytes = crm_api::domain::migration::crypto::open_snapshot(
        &f.key,
        f.ctx.organization_id,
        row.get("snapshot_id"),
        row.get("id"),
        &format!("admitted-metadata-v1:{plan}:result"),
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )
    .unwrap();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let ops = result["operations"].as_array().unwrap();
    assert_eq!(ops.iter().filter(|x| x["outcome"] == "held").count(), 3);
    assert_eq!(
        ops.iter()
            .filter(|x| x["outcome"] == "already_present")
            .count(),
        1
    );
    assert_eq!(
        ops.iter().filter(|x| x["outcome"] == "applied").count(),
        1,
        "independent choice remains eligible"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_readiness_requires_atomic_unit_schema(migrator: PgPool) {
    let mut tx = migrator.begin().await.unwrap();
    let ready = ReleaseReadiness::for_tests();
    ready.require_admitted_metadata(&mut tx).await.unwrap();
    sqlx::query("DROP INDEX migration_admitted_metadata_result_unit")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(ready.require_admitted_metadata(&mut tx).await.is_err());
    tx.rollback().await.unwrap();
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_private_permit_binds_table_claim_person_org_and_lease(migrator: PgPool) {
    let (f, root, plan, person) = prepared_typed(&migrator).await;
    let plan = approve_all(&f, root, plan).await;
    finish_catalog(&f, root).await;
    let row=sqlx::query("SELECT x.manifest_id,x.target_id,encode(m.source_key,'hex') AS source_key FROM migration_admitted_metadata_operation x JOIN migration_admitted_metadata_mapping m ON m.id=x.mapping_id WHERE x.import_id=$1 AND x.plan_id=(SELECT confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=$1) AND x.kind='tag_link'").bind(root).fetch_one(&f.pool).await.unwrap();
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='running',lease_token=$2,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(root).bind(lease).execute(&migrator).await.unwrap();
    let target: Uuid = row.get("target_id");
    let proof = json!({"lease":lease,"root_id":root,"plan_id":plan,"unit_id":row.get::<Uuid,_>("manifest_id"),"target_id":target,"claim_kind":"tag","claim_key":row.get::<String,_>("source_key"),"native_table":"person_tag"});
    let native = json!({"organization_id":f.org,"person_id":person,"tag_id":target});
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT crm_admitted_metadata_insert_allowed($1,$2,$3)")
            .bind(f.org)
            .bind(proof.to_string())
            .bind(&native)
            .fetch_one(&f.pool)
            .await
            .unwrap()
    );
    for name in ["lease", "unit_id", "root_id", "plan_id", "target_id"] {
        let mut bad = proof.clone();
        bad[name] = json!(Uuid::new_v4());
        assert!(
            !sqlx::query_scalar::<_, bool>("SELECT crm_admitted_metadata_insert_allowed($1,$2,$3)")
                .bind(f.org)
                .bind(bad.to_string())
                .bind(&native)
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            "permit accepts wrong {name}"
        );
    }
    for (name, value) in [
        ("native_table", "person"),
        ("claim_kind", "field"),
        ("claim_key", "00"),
    ] {
        let mut bad = proof.clone();
        bad[name] = json!(value);
        assert!(!sqlx::query_scalar::<_, bool>(
            "SELECT crm_admitted_metadata_insert_allowed($1,$2,$3)"
        )
        .bind(f.org)
        .bind(bad.to_string())
        .bind(&native)
        .fetch_one(&f.pool)
        .await
        .unwrap());
    }
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT crm_admitted_metadata_insert_allowed($1,$2,$3)"
    )
    .bind(Uuid::new_v4())
    .bind(proof.to_string())
    .bind(&native)
    .fetch_one(&f.pool)
    .await
    .unwrap());
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_metadata_permit',$1,true)")
        .bind(proof.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,'Forbidden forged catalog')").bind(Uuid::new_v4()).bind(f.org).bind(f.actor).execute(&mut *tx).await.is_err(),"Person-unit permit cannot insert another catalog row");
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='cancelled',lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(root).execute(&migrator).await.unwrap();
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT crm_admitted_metadata_insert_allowed($1,$2,$3)")
            .bind(f.org)
            .bind(proof.to_string())
            .bind(&native)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "cancel fences an otherwise exact proof"
    );
}

async fn finish_catalog(f: &Fixture, root: Uuid) {
    for _ in 0..10 {
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM migration_admitted_metadata_result WHERE import_id=$1 AND kind<>'people'").bind(root).fetch_one(&f.pool).await.unwrap();
        if n == 7 {
            return;
        }
        assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
            .await
            .unwrap());
    }
    panic!("catalog did not settle")
}

#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_hot_units_seek_past_settled_people_at_d050(migrator: PgPool) {
    let (f, root, plan, person) = prepared_typed(&migrator).await;
    let plan = approve_all(&f, root, plan).await;
    finish(&f, root).await;
    let mut c = migrator.acquire().await.unwrap();
    // Cardinality-only retained/native rows. Never execute these copied encrypted
    // envelopes: their AAD intentionally belongs to the original real fixture.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&mut *c)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE admitted_scale AS SELECT n,gen_random_uuid() AS person,gen_random_uuid() AS manifest,gen_random_uuid() AS result FROM generate_series(1,$1::bigint) n").bind(25_000-count).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT person,$1,'Synthetic','Metadata '||n,$2,$3 FROM admitted_scale").bind(f.org).bind(f.lead_stage).bind(f.actor).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_metadata_manifest(id,import_id,plan_id,organization_id,admission_result_id,person_id,source_person_id,disposition,baseline_nonce,baseline_ciphertext,item_byte_bound,settled_at) SELECT x.manifest,m.import_id,m.plan_id,m.organization_id,m.admission_result_id,x.person,(1000000+x.n)::text,'settled',m.baseline_nonce,m.baseline_ciphertext,m.item_byte_bound,clock_timestamp() FROM admitted_scale x CROSS JOIN migration_admitted_metadata_manifest m WHERE m.import_id=$1 AND m.plan_id=(SELECT confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=$1) AND m.person_id=$2").bind(root).bind(person).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,mapping_id,source_key,target_id,disposition,nonce,ciphertext) SELECT gen_random_uuid(),x.manifest,o.import_id,o.plan_id,o.organization_id,o.kind,o.mapping_id,o.source_key,o.target_id,o.disposition,o.nonce,o.ciphertext FROM admitted_scale x CROSS JOIN migration_admitted_metadata_operation o WHERE o.import_id=$1 AND o.manifest_id=(SELECT id FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=(SELECT confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=$1) AND person_id=$2)").bind(root).bind(person).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_metadata_result(id,import_id,plan_id,unit_id,manifest_id,organization_id,kind,disposition,person_id,nonce,ciphertext) SELECT x.result,r.import_id,r.plan_id,x.manifest,x.manifest,r.organization_id,'people',r.disposition,x.person,r.nonce,r.ciphertext FROM admitted_scale x CROSS JOIN migration_admitted_metadata_result r WHERE r.import_id=$1 AND r.kind='people'").bind(root).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,number_value,date_value,option_id,updated_by_user_id,origin,correlation_id) SELECT v.organization_id,x.person,v.field_id,v.field_type,v.text_value,v.number_value,v.date_value,v.option_id,v.updated_by_user_id,v.origin,v.correlation_id FROM admitted_scale x CROSS JOIN person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=$2").bind(f.org).bind(person).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) SELECT t.organization_id,x.person,t.tag_id,t.added_by_user_id FROM admitted_scale x CROSS JOIN person_tag t WHERE t.organization_id=$1 AND t.person_id=$2").bind(f.org).bind(person).execute(&mut *c).await.unwrap();
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_membership WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&mut *c)
            .await
            .unwrap();
    sqlx::query("CREATE TEMP TABLE admitted_members AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,$1::bigint) n").bind(50-members).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'am-perf-'||n||'@synthetic.test','Synthetic metadata member' FROM admitted_members").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM admitted_members").bind(f.org).execute(&mut *c).await.unwrap();
    for table in [
        "person",
        "person_tag",
        "person_custom_field_value",
        "migration_admitted_metadata_manifest",
        "migration_admitted_metadata_operation",
        "migration_admitted_metadata_result",
        "migration_admitted_metadata_mapping",
        "migration_metadata_catalog_claim",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *c)
            .await
            .unwrap();
    }
    let last:Uuid=sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_manifest WHERE import_id=$1 ORDER BY id DESC LIMIT 1").bind(root).fetch_one(&mut *c).await.unwrap();
    let after:Uuid=sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_manifest WHERE import_id=$1 ORDER BY id DESC OFFSET 1 LIMIT 1").bind(root).fetch_one(&mut *c).await.unwrap();
    sqlx::query(
        "UPDATE migration_admitted_metadata_manifest SET disposition='eligible' WHERE id=$1",
    )
    .bind(last)
    .execute(&mut *c)
    .await
    .unwrap();
    fn statement(prefix: &str) -> String {
        let source = include_str!("../../crm-app/src/domain/migration/admitted_metadata_worker.rs");
        let start = source
            .find(&format!("\"{prefix}"))
            .expect("production hot statement");
        serde_json::Deserializer::from_str(&source[start..])
            .into_iter::<String>()
            .next()
            .unwrap()
            .unwrap()
    }
    fn indexed(plan: &Value, table: &str) -> bool {
        match plan {
            Value::Object(o) => {
                o.get("Index Name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.starts_with(table))
                    || o.values().any(|v| indexed(v, table))
            }
            Value::Array(a) => a.iter().any(|v| indexed(v, table)),
            _ => false,
        }
    }
    macro_rules! explain {($name:expr,$prefix:expr,$table:expr $(,$bind:expr)* $(,)?)=>{{
        let sql=statement($prefix);let query=format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let result:Value=sqlx::query_scalar(&query)$(.bind($bind))*.fetch_one(&mut *c).await.unwrap();
        assert!(indexed(&result,$table),"{} must use tenant/unit index: {}",$name,result);
        println!("CRM_010F3_HOT_QUERY {}",json!({"name":$name,"sql":sql,"plan":result}));
    }};}
    explain!(
        "late_person_keyset",
        "SELECT m.* FROM migration_admitted_metadata_manifest m WHERE m.import_id=$1",
        "migration_admitted_metadata_manifest",
        root,
        plan,
        f.org,
        after
    );
    explain!(
        "person_operations",
        "SELECT * FROM migration_admitted_metadata_operation WHERE manifest_id=$1",
        "migration_admitted_metadata_operation",
        last,
        root,
        plan,
        f.org
    );
    explain!(
        "settled_catalog_receipt",
        "SELECT disposition FROM migration_admitted_metadata_result WHERE import_id=$1",
        "migration_admitted_metadata_result",
        root,
        f.org,
        last
    );
    let field: Uuid = sqlx::query_scalar(
        "SELECT field_id FROM person_custom_field_value WHERE person_id=$1 LIMIT 1",
    )
    .bind(person)
    .fetch_one(&mut *c)
    .await
    .unwrap();
    explain!("native_value_baseline","SELECT to_jsonb(v)-ARRAY['created_at','updated_at'] FROM person_custom_field_value v WHERE organization_id=$1","person_custom_field_value",f.org,person,field);
    explain!(
        "native_tag_baseline",
        "SELECT tag_id FROM person_tag WHERE person_id=$1",
        "person_tag",
        person,
        f.org
    );
    // Catalog cardinality is deliberately bounded, while Person/result cardinality
    // is realistic. Require indexed receipt probes even for its seven-row catalog.
    let catalog_sql =
        statement("SELECT m.* FROM migration_admitted_metadata_mapping m WHERE m.import_id=$1");
    let p: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {catalog_sql}"
    ))
    .bind(root)
    .bind(plan)
    .bind(f.org)
    .bind("field")
    .bind(Uuid::nil())
    .fetch_one(&mut *c)
    .await
    .unwrap();
    assert!(indexed(&p, "migration_admitted_metadata_result"));
    println!(
        "CRM_010F3_HOT_QUERY {}",
        json!({"name":"catalog_keyset_and_exact_result_probe","sql":catalog_sql,"plan":p})
    );
    let logical:i64=sqlx::query_scalar("SELECT (SELECT sum(octet_length(source_person_id)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)) FROM migration_admitted_metadata_manifest WHERE import_id=$1)+(SELECT sum(octet_length(source_key)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_operation WHERE import_id=$1)+(SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_result WHERE import_id=$1)").bind(root).fetch_one(&mut *c).await.unwrap();
    let physical:i64=sqlx::query_scalar("SELECT pg_total_relation_size('migration_admitted_metadata_manifest')+pg_total_relation_size('migration_admitted_metadata_operation')+pg_total_relation_size('migration_admitted_metadata_result')").fetch_one(&mut *c).await.unwrap();
    println!(
        "CRM_010F3_STORAGE {}",
        json!({"people":25000,"members":50,"classification":"inert_cardinality_seed_not_billable_execution","copied_manifest_operation_result_logical_bytes":logical,"physical_relations_bytes":physical,"fields_per_person":4,"tags_per_person":1})
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_reuses_original_claims_after_ready_handover(migrator: PgPool) {
    use crm_api::domain::migration::{
        metadata, metadata_worker, people_admission, people_admission_worker,
    };
    let (f, parent, original, original_ready) =
        crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let admission=crate::db_people_admission_execution::ready(&f,parent,vec![json!({"id":104,"firstName":"Admitted","lastName":"Shared","stage":"Lead","assignedUserId":3,"tags":["Atomic Tag"],"customAtomic":"admitted value"})]).await;
    let ready = people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
        .await
        .unwrap();
    let p = &ready["plan"];
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
    let source_report: Uuid =
        sqlx::query_scalar("SELECT report_id FROM migration_people_admission WHERE id=$1")
            .bind(admission)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let prepared = admitted_metadata::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        admitted_metadata::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id: source_report,
        },
    )
    .await
    .unwrap();
    let root = Uuid::parse_str(prepared["import"]["id"].as_str().unwrap()).unwrap();
    drain_preparation(&f, root).await;
    // Readiness is active before the original worker obtains its lease. It must
    // atomically create a shared claim instead of bypassing the new registry.
    metadata::confirm(&f.pool,&f.key,&f.ctx,original,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":original_ready["latest_plan"]["id"],"plan_revision":original_ready["latest_plan"]["revision"],"confirmation_digest":original_ready["latest_plan"]["confirmation_digest"],"workspace_revision":original_ready["workspace_revision"],"acknowledgments":{"held_count":original_ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap(),&ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    for _ in 0..30 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
    }
    let original_detail = metadata::detail(&f.pool, &f.key, &f.ctx, original, &f.policy)
        .await
        .unwrap();
    assert_eq!(
        original_detail["state"], "completed",
        "pause={}",
        original_detail["pause_reason"]
    );
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
    let evidence:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(x) ORDER BY kind,source_key) FROM migration_metadata_identity x WHERE import_id=$1").bind(original).fetch_one(&f.pool).await.unwrap();
    let maps=sqlx::query("SELECT m.id,c.target_id FROM migration_admitted_metadata_mapping m JOIN migration_metadata_catalog_claim c ON c.organization_id=m.organization_id AND c.kind=m.kind AND c.source_key=m.source_key WHERE m.import_id=$1 ORDER BY m.kind,m.id").bind(root).fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        maps.len(),
        2,
        "original/admitted namespaces use exact matching keys"
    );
    let _plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            expected_plan_revision: "1".into(),
            source_report_id: None,
            mappings: maps
                .iter()
                .map(|m| crm_api::domain::migration::metadata::MappingPatch {
                    mapping_id: m.get("id"),
                    choice: crm_api::domain::migration::metadata::Choice::MapExisting {
                        target_id: m.get("target_id"),
                    },
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    drain_preparation(&f, root).await;
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    admitted_metadata::confirm(
        &f.pool,
        &f.key,
        Some(&ReleaseReadiness::for_tests()),
        &f.ctx,
        root,
        admitted_metadata::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            plan_revision: "2".into(),
            confirmation_digest: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["latest_plan"]["confirmation_digest"]
                .as_str()
                .unwrap()
                .to_owned(),
            workspace_revision: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                ["workspace_revision"]
                .as_str()
                .unwrap()
                .to_owned(),
            acknowledgments: crm_api::domain::migration::metadata::Acknowledgments {
                held_count: admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()
                    ["latest_plan"]["counts"]["held_count"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                review_only: true,
                remaining_data: true,
            },
        },
    )
    .await
    .unwrap();
    finish(&f, root).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_metadata_catalog_claim WHERE admitted_import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "reuse does not copy/adopt ownership"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1 AND text_value='admitted value'").bind(f.org).fetch_one(&f.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,Value>("SELECT jsonb_agg(to_jsonb(x) ORDER BY kind,source_key) FROM migration_metadata_identity x WHERE import_id=$1").bind(original).fetch_one(&f.pool).await.unwrap(),evidence);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_preparation_retains_all_observations_and_rolls_back_failed_steps(
    migrator: PgPool,
) {
    let person = json!({"id":104,"firstName":"Source","lastName":"Observations","stage":"Lead","assignedUserId":3,"tags":["First spelling"]});
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![person.clone()]).await;
    let mut repeated = vec![person; 17];
    repeated[16]["tags"] = json!(["A later conflicting observation"]);
    let selected = report(&f, parent, repeated).await;
    let prepared = admitted_metadata::prepare(
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
    let root = Uuid::parse_str(prepared["import"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_metadata_manifest WHERE import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "HTTP preparation commits only bounded inputs"
    );
    sqlx::raw_sql("CREATE FUNCTION admitted_source_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic preparation failure'; END $$; CREATE TRIGGER admitted_source_fault BEFORE INSERT ON migration_admitted_metadata_observation FOR EACH ROW EXECUTE FUNCTION admitted_source_fault()").execute(&migrator).await.unwrap();
    let mut failed = false;
    for _ in 0..10 {
        let before = ledger(&f, root).await;
        let checkpoint: Value = sqlx::query_scalar(
            "SELECT to_jsonb(p) FROM migration_admitted_metadata_plan p WHERE import_id=$1",
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        if admitted_metadata_worker::run_once(&f.pool, &f.key)
            .await
            .is_err()
        {
            failed = true;
            assert_eq!(
                ledger(&f, root).await,
                before,
                "source bytes and reservation settle atomically with checkpoint"
            );
            assert_eq!(
                sqlx::query_scalar::<_, Value>(
                    "SELECT to_jsonb(p) FROM migration_admitted_metadata_plan p WHERE import_id=$1"
                )
                .bind(root)
                .fetch_one(&f.pool)
                .await
                .unwrap(),
                checkpoint
            );
            break;
        }
    }
    assert!(
        failed,
        "the first nonempty retained source step must hit the fault"
    );
    sqlx::query("DROP TRIGGER admitted_source_fault ON migration_admitted_metadata_observation")
        .execute(&migrator)
        .await
        .unwrap();
    assert!(!admitted_metadata_worker::run_once(&f.pool, &f.key)
        .await
        .unwrap());
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
    let before_source = ledger(&f, root).await;
    assert!(admitted_metadata_worker::run_once(&f.pool, &f.key)
        .await
        .unwrap());
    let source_bytes:i64=sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(source_id)+octet_length(semantic_hmac)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_source WHERE import_id=$1),0)+COALESCE((SELECT sum(octet_length(semantic_hmac)) FROM migration_admitted_metadata_observation WHERE import_id=$1),0)::bigint").bind(root).fetch_one(&f.pool).await.unwrap();
    let after_source = ledger(&f, root).await;
    for field in ["root_retained", "snapshot_retained", "org_retained"] {
        assert_eq!(
            after_source[field].as_i64().unwrap() - before_source[field].as_i64().unwrap(),
            source_bytes,
            "exact preparation inventory for {field}"
        );
    }
    drain_preparation(&f, root).await;
    let source=sqlx::query("SELECT id,observations,conflict,qualified FROM migration_admitted_metadata_source WHERE import_id=$1 AND family='people' AND source_id='104'").bind(root).fetch_one(&f.pool).await.unwrap();
    assert_eq!(source.get::<i64, _>("observations"), 17);
    assert!(source.get::<bool, _>("conflict"));
    assert!(source.get::<bool, _>("qualified"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_metadata_observation WHERE source_row_id=$1"
        )
        .bind(source.get::<Uuid, _>("id"))
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        17
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT disposition FROM migration_admitted_metadata_manifest WHERE import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "held",
        "17th observation cannot be lost behind sampled report evidence"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT preparation_bytes FROM migration_admitted_metadata_plan WHERE import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(ledger(&f, root).await["root_reserved"], 65536);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_preparation_preserves_missing_native_cohort_target(migrator: PgPool) {
    let person = json!({"id":104,"firstName":"Missing","lastName":"Target","stage":"Lead","assignedUserId":3,"tags":["Past Client"]});
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![person.clone()]).await;
    let selected = report(&f, parent, vec![person]).await;
    let expected:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND disposition='settled'").bind(admission).fetch_one(&f.pool).await.unwrap();
    // Simulate a missing native row in this disposable database. Upstream
    // provenance FKs normally prevent physical deletion; retain the immutable
    // evidence verbatim and remove only these test constraints to inject the
    // corruption the metadata planner must hold. No production bypass changes.
    sqlx::raw_sql("ALTER TABLE person_admission_provenance DROP CONSTRAINT person_admission_provenance_person_id_organization_id_fkey; ALTER TABLE person_admitted DROP CONSTRAINT person_admitted_person_id_organization_id_fkey").execute(&migrator).await.unwrap();
    sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
        .bind(expected)
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    let prepared = admitted_metadata::prepare(
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
    let root = Uuid::parse_str(prepared["import"]["id"].as_str().unwrap()).unwrap();
    drain_preparation(&f, root).await;
    let header = admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap();
    let plan = Uuid::parse_str(header["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let issues = admitted_metadata::issues(&f.pool, &f.key, &f.ctx, root, plan, Default::default())
        .await
        .unwrap();
    assert!(!issues["items"].as_array().unwrap().is_empty());
    let manifest=sqlx::query("SELECT expected_person_id,person_id,disposition FROM migration_admitted_metadata_manifest WHERE import_id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        manifest.get::<Option<Uuid>, _>("expected_person_id"),
        Some(expected)
    );
    assert_eq!(manifest.get::<Option<Uuid>, _>("person_id"), None);
    assert_eq!(manifest.get::<String, _>("disposition"), "held");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person WHERE id=$1")
            .bind(expected)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_preparation_hot_steps_at_d050(migrator: PgPool) {
    let (f, root, plan, person) = prepared_typed(&migrator).await;
    let rootrow=sqlx::query("SELECT snapshot_id,admission_id,source_account_id FROM migration_admitted_metadata_import WHERE id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
    let snapshot: Uuid = rootrow.get("snapshot_id");
    let admission: Uuid = rootrow.get("admission_id");
    let account: i64 = rootrow.get("source_account_id");
    let mut c = migrator.acquire().await.unwrap();
    let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&mut *c)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE staged_scale AS SELECT n,gen_random_uuid() AS person,gen_random_uuid() AS source,gen_random_uuid() AS record,gen_random_uuid() AS item,gen_random_uuid() AS result FROM generate_series(1,$1::bigint) n").bind(25000-existing).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT person,$1,'Synthetic','Staged metadata '||n,$2,$3 FROM staged_scale").bind(f.org).bind(f.lead_stage).bind(f.actor).execute(&mut *c).await.unwrap();
    // Cardinality-only copies retain valid relational scopes; encrypted envelopes
    // deliberately keep their original AAD and are never used by a worker.
    sqlx::query("CREATE TEMP TABLE staged_captures AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,250) n").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture SELECT (jsonb_populate_record(NULL::migration_snapshot_capture,to_jsonb(cap)||jsonb_build_object('id',x.id,'sequence',10000+x.n,'checkpoint',10000+x.n))).* FROM staged_captures x CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='people' ORDER BY sequence LIMIT 1) cap").bind(snapshot).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record SELECT (jsonb_populate_record(NULL::migration_snapshot_record,to_jsonb(r)||jsonb_build_object('id',x.record,'capture_id',cap.id,'capture_sequence',10000+cap.n,'ordinal',((x.n-1)%100)::integer,'source_id',(1000000+x.n)::text))).* FROM staged_scale x JOIN staged_captures cap ON cap.n=(x.n-1)/100+1 CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='people' AND source_id='104' LIMIT 1) r").bind(snapshot).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_metadata_source SELECT (jsonb_populate_record(NULL::migration_admitted_metadata_source,to_jsonb(s)||jsonb_build_object('id',x.source,'source_id',(1000000+x.n)::text,'capture_id',cap.id,'capture_sequence',10000+cap.n,'ordinal',((x.n-1)%100)::integer))).* FROM staged_scale x JOIN staged_captures cap ON cap.n=(x.n-1)/100+1 CROSS JOIN migration_admitted_metadata_source s WHERE s.plan_id=$1 AND s.family='people' AND s.source_id='104'").bind(plan).execute(&mut *c).await.unwrap();
    sqlx::query("ALTER TABLE migration_people_admission_item DISABLE TRIGGER migration_people_admission_item_building").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_people_admission_item SELECT (jsonb_populate_record(NULL::migration_people_admission_item,to_jsonb(i)||jsonb_build_object('id',x.item,'source_id',(1000000+x.n)::text,'source_key','people:'||(1000000+x.n)::text,'prospective_person_id',x.person,'settled_result_id',x.result))).* FROM staged_scale x CROSS JOIN LATERAL(SELECT * FROM migration_people_admission_item WHERE admission_id=$1 AND source_id='104') i").bind(admission).execute(&mut *c).await.unwrap();
    sqlx::query("ALTER TABLE migration_people_admission_item ENABLE TRIGGER migration_people_admission_item_building").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_people_admission_result SELECT (jsonb_populate_record(NULL::migration_people_admission_result,to_jsonb(r)||jsonb_build_object('id',x.result,'item_id',x.item,'person_id',x.person,'source_id',(1000000+x.n)::text))).* FROM staged_scale x CROSS JOIN LATERAL(SELECT * FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='104') r").bind(admission).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_import_identity SELECT (jsonb_populate_record(NULL::migration_import_identity,to_jsonb(i)||jsonb_build_object('target_id',x.person,'source_id',(1000000+x.n)::text,'admission_item_id',x.item,'admission_result_id',x.result))).* FROM staged_scale x CROSS JOIN LATERAL(SELECT * FROM migration_import_identity WHERE admission_id=$1 AND target_id=$2 AND family='people') i").bind(admission).bind(person).execute(&mut *c).await.unwrap();
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_membership WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&mut *c)
            .await
            .unwrap();
    sqlx::query("CREATE TEMP TABLE staged_members AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,$1::bigint) n").bind(50-members).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'staged-perf-'||n||'@synthetic.test','Synthetic preparation member' FROM staged_members").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM staged_members").bind(f.org).execute(&mut *c).await.unwrap();
    for table in [
        "person",
        "organization_membership",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_admitted_metadata_source",
        "migration_people_admission_item",
        "migration_people_admission_result",
        "migration_import_identity",
        "migration_admitted_metadata_mapping",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *c)
            .await
            .unwrap();
    }
    fn statement(prefix: &str) -> String {
        let source =
            include_str!("../../crm-app/src/domain/migration/admitted_metadata/preparation.rs");
        let start = source
            .find(&format!("\"{prefix}"))
            .expect("production preparation statement");
        serde_json::Deserializer::from_str(&source[start..])
            .into_iter::<String>()
            .next()
            .unwrap()
            .unwrap()
    }
    fn index(plan: &Value, name: &str) -> bool {
        match plan {
            Value::Object(o) => {
                o.get("Index Name")
                    .and_then(Value::as_str)
                    .is_some_and(|v| v.contains(name))
                    || o.values().any(|v| index(v, name))
            }
            Value::Array(v) => v.iter().any(|v| index(v, name)),
            _ => false,
        }
    }
    if std::env::var("CRM_ADMITTED_PREPARATION_PLAN").as_deref() == Ok("choices") {
        sqlx::query("INSERT INTO migration_admitted_metadata_mapping SELECT (jsonb_populate_record(NULL::migration_admitted_metadata_mapping,to_jsonb(m)||jsonb_build_object('id',x.item,'source_id',(1000000+x.n)::text,'source_key',decode(md5(x.n::text)||md5(x.n::text),'hex'),'target_id',x.person))).* FROM staged_scale x CROSS JOIN LATERAL(SELECT * FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND kind='field' LIMIT 1) m").bind(plan).execute(&mut *c).await.unwrap();
        sqlx::query("INSERT INTO migration_admitted_metadata_observation(id,import_id,plan_id,organization_id,source_row_id,snapshot_record_id,semantic_hmac,qualified) SELECT gen_random_uuid(),$1,$2,$3,x.source,x.record,s.semantic_hmac,true FROM staged_scale x JOIN migration_admitted_metadata_source s ON s.id=x.source").bind(root).bind(plan).bind(f.org).execute(&mut *c).await.unwrap();
        sqlx::query("ANALYZE migration_admitted_metadata_mapping")
            .execute(&mut *c)
            .await
            .unwrap();
        let after:Uuid=sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND kind='field' ORDER BY id DESC OFFSET 1 LIMIT 1").bind(plan).fetch_one(&mut *c).await.unwrap();
        let q=format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",statement("SELECT * FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND (kind,id)"));
        let p: Value = sqlx::query_scalar(&q)
            .bind(plan)
            .bind(f.org)
            .bind("field")
            .bind(after)
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_CHOICE_KEYSET={p}");
        assert!(index(&p, "migration_admitted_metadata_mapping_prepare"));
        let target=sqlx::query("SELECT id,source_key,target_id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND kind='field' AND target_id IS NOT NULL ORDER BY id DESC LIMIT 1").bind(plan).fetch_one(&mut *c).await.unwrap();
        let q=format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",statement("SELECT disposition,target_id FROM migration_admitted_metadata_mapping WHERE plan_id=$1"));
        let p: Value = sqlx::query_scalar(&q)
            .bind(plan)
            .bind(f.org)
            .bind("field")
            .bind(target.get::<Vec<u8>, _>("source_key"))
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_CHOICE_INHERIT={p}");
        assert!(index(&p, "migration_admitted_metadata"));
        let q = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            statement(
                "SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping WHERE plan_id=$1"
            )
        );
        let p: Value = sqlx::query_scalar(&q)
            .bind(plan)
            .bind(f.org)
            .bind("field")
            .bind(target.get::<Uuid, _>("target_id"))
            .bind(Uuid::nil())
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_CHOICE_DESTINATION={p}");
        assert!(index(&p, "migration_admitted_metadata_selected_target"));
        let inventory:Value=sqlx::query_scalar("SELECT jsonb_build_object('source_rows',(SELECT count(*) FROM migration_admitted_metadata_source WHERE plan_id=$1),'observation_rows',(SELECT count(*) FROM migration_admitted_metadata_observation WHERE plan_id=$1),'source_logical_bytes',(SELECT sum(octet_length(source_id)+octet_length(semantic_hmac)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_source WHERE plan_id=$1),'observation_logical_bytes',(SELECT sum(octet_length(semantic_hmac)) FROM migration_admitted_metadata_observation WHERE plan_id=$1),'physical_relation_bytes',pg_total_relation_size('migration_admitted_metadata_source')+pg_total_relation_size('migration_admitted_metadata_observation'),'classification','inert_cardinality_copies_not_billable_execution')").bind(plan).fetch_one(&mut *c).await.unwrap();
        println!("PREP_STORAGE={inventory}");
        return;
    }
    let only_cohort = std::env::var("CRM_ADMITTED_PREPARATION_PLAN").as_deref() == Ok("cohort");
    if !only_cohort {
        let query = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            statement("SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1")
        );
        let p: Value = sqlx::query_scalar(&query)
            .bind(snapshot)
            .bind(f.org)
            .bind(10250i64)
            .bind(10249i64)
            .bind(99i32)
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_CAPTURE={p}");
        assert!(index(&p, "migration_snapshot_capture"));
        let cap: Uuid =
            sqlx::query_scalar("SELECT id FROM staged_captures ORDER BY n DESC LIMIT 1")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        let query = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            statement("SELECT * FROM migration_snapshot_record WHERE capture_id=$1")
        );
        let p: Value = sqlx::query_scalar(&query)
            .bind(cap)
            .bind(snapshot)
            .bind(f.org)
            .bind(96i32)
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_RECORD={p}");
        assert!(index(&p, "migration_snapshot_record"));
        let query = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            statement("SELECT s.* FROM migration_admitted_metadata_source s WHERE s.plan_id=$1")
        );
        let p: Value = sqlx::query_scalar(&query)
            .bind(plan)
            .bind(f.org)
            .bind(10250i64)
            .bind(95i32)
            .bind(Uuid::nil())
            .bind(admission)
            .bind(account)
            .fetch_one(&mut *c)
            .await
            .unwrap();
        println!("PREP_TAG={p}");
        assert!(index(&p, "migration_admitted_metadata_source_order"));
    }
    let after:Uuid=sqlx::query_scalar("SELECT id FROM migration_people_admission_result WHERE admission_id=$1 ORDER BY id DESC OFFSET 1 LIMIT 1").bind(admission).fetch_one(&mut *c).await.unwrap();
    let query = format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        statement("SELECT ar.id,ar.item_id,ar.person_id,ar.source_id,p.id AS live_person_id")
    );
    let p: Value = sqlx::query_scalar(&query)
        .bind(f.org)
        .bind(admission)
        .bind(after)
        .bind(account)
        .fetch_one(&mut *c)
        .await
        .unwrap();
    println!("PREP_COHORT={p}");
    assert!(index(&p, "migration_admitted_metadata_admission_work"));
    assert!(
        exact_identity_lookup(&p),
        "the per-unit identity check must not hash the complete identity namespace"
    );
    if !only_cohort {
        for prefix in [
            "SELECT id,source_id FROM migration_admitted_metadata_source WHERE plan_id=$1",
            "SELECT * FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field'",
        ] {
            let query = format!(
                "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
                statement(prefix)
            );
            let p: Value = sqlx::query_scalar(&query)
                .bind(plan)
                .bind(f.org)
                .bind(Uuid::nil())
                .fetch_one(&mut *c)
                .await
                .unwrap();
            println!("PREP_FIELD={p}");
        }
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_typed_reads_require_current_tenant_admin(migrator: PgPool) {
    use crm_api::{
        domain::migration::MigrationError,
        ids::{OrganizationId, UserId},
    };
    let (f, root, plan, _) = prepared_typed(&migrator).await;
    let mut member = f.ctx.clone();
    member.actor_user_id = UserId::new(f.member);
    assert!(matches!(
        admitted_metadata::get(&f.pool, &member, root).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        admitted_metadata::list(&f.pool, &f.key, &member, admitted_metadata::Page::default()).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        admitted_metadata::mappings(
            &f.pool,
            &f.key,
            &member,
            root,
            plan,
            admitted_metadata::PlanPage::default()
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        admitted_metadata::records(
            &f.pool,
            &f.key,
            &member,
            root,
            plan,
            admitted_metadata::PlanPage::default()
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        admitted_metadata::issues(&f.pool, &f.key, &member, root, plan, Default::default()).await,
        Err(MigrationError::Forbidden)
    ));
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO organization(id,name,intake_slug,intake_token) VALUES($1,'Synthetic isolation','am-'||replace($1::text,'-',''),'testonly')")
        .bind(other)
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) VALUES($1,$2,'admin','active')").bind(other).bind(f.actor).execute(&migrator).await.unwrap();
    let mut outsider = f.ctx.clone();
    outsider.organization_id = OrganizationId::new(other);
    assert!(matches!(
        admitted_metadata::get(&f.pool, &outsider, root).await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        admitted_metadata::mappings(
            &f.pool,
            &f.key,
            &outsider,
            root,
            plan,
            admitted_metadata::PlanPage::default()
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        admitted_metadata::records(
            &f.pool,
            &f.key,
            &outsider,
            root,
            plan,
            admitted_metadata::PlanPage::default()
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        admitted_metadata::issues(&f.pool, &f.key, &outsider, root, plan, Default::default()).await,
        Err(MigrationError::NotFound)
    ));
}

#[cfg(feature = "perf-harness")]
fn exact_identity_lookup(p: &Value) -> bool {
    match p {
        Value::Object(o) => {
            if o.get("Relation Name").and_then(Value::as_str) == Some("migration_import_identity") {
                return o.get("Node Type").and_then(Value::as_str) == Some("Index Scan")
                    && o.get("Index Name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n.starts_with("migration_import_identity"))
                    && o.get("Actual Rows")
                        .and_then(Value::as_f64)
                        .is_some_and(|n| n <= 1.0);
            }
            o.values().any(exact_identity_lookup)
        }
        Value::Array(a) => a.iter().any(exact_identity_lookup),
        _ => false,
    }
}
#[cfg(feature = "perf-harness")]
#[test]
fn admitted_metadata_recorded_cohort_plan_uses_a_bounded_identity_lookup() {
    // Replay the actual db4 plan to correct the overly specific pkey assertion.
    // This runs no new EXPLAIN and preserves D-050's measurement budget.
    let plan: Value =
        serde_json::from_str(include_str!("fixtures/admitted_metadata_cohort_plan.json")).unwrap();
    assert!(exact_identity_lookup(&plan));
    assert_eq!(plan[0]["Plan"]["Actual Rows"], 1.0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_replans_preserve_evidence_inherit_choices_and_replay_across_sources(
    migrator: PgPool,
) {
    use crm_api::domain::migration::metadata::{Choice, MappingPatch};
    let (f, root, first, _) = prepared_typed(&migrator).await;
    let mapping:Uuid=sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND kind='field' ORDER BY id LIMIT 1").bind(first).fetch_one(&f.pool).await.unwrap();
    let frozen:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM migration_admitted_metadata_mapping m WHERE plan_id=$1").bind(first).fetch_one(&f.pool).await.unwrap();
    let request = Uuid::new_v4();
    let response = admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: request,
            expected_plan_revision: "1".into(),
            source_report_id: None,
            mappings: vec![MappingPatch {
                mapping_id: mapping,
                choice: Choice::CreateMatching,
            }],
        },
    )
    .await
    .unwrap();
    let second =
        Uuid::parse_str(response["import"]["latest_plan"]["id"].as_str().unwrap()).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_metadata_manifest WHERE plan_id=$1"
        )
        .bind(second)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "HTTP stages no full cohort"
    );
    assert!(
        admitted_metadata::apply_mappings(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            admitted_metadata::MappingPatch {
                request_id: Uuid::new_v4(),
                expected_plan_revision: "2".into(),
                source_report_id: None,
                mappings: vec![]
            }
        )
        .await
        .is_err(),
        "only one building plan"
    );
    drain_preparation(&f, root).await;
    assert_eq!(sqlx::query_scalar::<_,Value>("SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM migration_admitted_metadata_mapping m WHERE plan_id=$1").bind(first).fetch_one(&f.pool).await.unwrap(),frozen,"old choices/baselines/evidence are immutable");
    let inherited = admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            expected_plan_revision: "2".into(),
            source_report_id: None,
            mappings: vec![],
        },
    )
    .await
    .unwrap();
    let third =
        Uuid::parse_str(inherited["import"]["latest_plan"]["id"].as_str().unwrap()).unwrap();
    drain_preparation(&f, root).await;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND disposition='create_matching'").bind(third).fetch_one(&f.pool).await.unwrap(),1);
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let selected=report(&f,parent,vec![json!({"id":104,"firstName":"New retained source","stage":"Lead","assignedUserId":3,"tags":["New tag"]})]).await;
    let changed = admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            expected_plan_revision: "3".into(),
            source_report_id: Some(selected),
            mappings: vec![],
        },
    )
    .await
    .unwrap();
    let fourth = Uuid::parse_str(changed["import"]["latest_plan"]["id"].as_str().unwrap()).unwrap();
    drain_preparation(&f, root).await;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND disposition<>'held'").bind(fourth).fetch_one(&f.pool).await.unwrap(),0,"a different source invalidates all dependent choices");
    assert!(sqlx::query_scalar::<_,bool>("SELECT a.snapshot_id<>b.snapshot_id FROM migration_admitted_metadata_plan a,migration_admitted_metadata_plan b WHERE a.id=$1 AND b.id=$2").bind(first).bind(fourth).fetch_one(&f.pool).await.unwrap());
    let replay = admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: request,
            expected_plan_revision: "1".into(),
            source_report_id: None,
            mappings: vec![MappingPatch {
                mapping_id: mapping,
                choice: Choice::CreateMatching,
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        replay, response,
        "receipt opens with its original snapshot even after source replacement"
    );
    assert!(
        admitted_metadata::apply_mappings(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            admitted_metadata::MappingPatch {
                request_id: request,
                expected_plan_revision: "1".into(),
                source_report_id: None,
                mappings: vec![]
            }
        )
        .await
        .is_err(),
        "altered reuse conflicts"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_cancel_spends_protected_capacity_and_replays_at_full_quota(
    migrator: PgPool,
) {
    let (f, root, _, _) = prepared_typed(&migrator).await;
    let before = ledger(&f, root).await;
    assert_eq!(before["root_reserved"], 65536);
    sqlx::query("UPDATE migration_snapshot_storage SET byte_limit=retained_bytes+reserved_bytes WHERE organization_id=$1").bind(f.org).execute(&migrator).await.unwrap();
    let request = Uuid::new_v4();
    let receipt = admitted_metadata::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::Request {
            request_id: request,
        },
    )
    .await
    .unwrap();
    let after = ledger(&f, root).await;
    assert_eq!(after["root_reserved"], 0);
    assert_eq!(after["reservations"], 0);
    let actual:i64=sqlx::query_scalar("SELECT (octet_length(digest)+octet_length(nonce)+octet_length(ciphertext))::bigint FROM migration_admitted_metadata_receipt WHERE import_id=$1 AND action='cancel'").bind(root).fetch_one(&f.pool).await.unwrap();
    for name in ["root_retained", "snapshot_retained", "org_retained"] {
        assert_eq!(
            after[name].as_i64().unwrap() - before[name].as_i64().unwrap(),
            actual
        );
    }
    assert_eq!(
        admitted_metadata::cancel(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            admitted_metadata::Request {
                request_id: request
            }
        )
        .await
        .unwrap(),
        receipt
    );
    assert_eq!(ledger(&f, root).await, after);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_prepare_replays_after_cancel_and_allows_fresh_unconfirmed_root(
    migrator: PgPool,
) {
    let (f, root, _, _) = prepared_typed(&migrator).await;
    let r = sqlx::query(
        "SELECT admission_id,source_report_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let request = Uuid::new_v4();
    let make = || admitted_metadata::Prepare {
        request_id: request,
        admission_id: r.get("admission_id"),
        source_report_id: r.get("source_report_id"),
    };
    let receipt = admitted_metadata::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        make(),
    )
    .await
    .unwrap();
    let before = ledger(&f, root).await;
    assert_eq!(
        admitted_metadata::prepare(
            &f.pool,
            &f.key,
            &ReleaseReadiness::for_tests(),
            &f.ctx,
            make()
        )
        .await
        .unwrap(),
        receipt
    );
    assert_eq!(ledger(&f, root).await, before);
    let mut mismatch = make();
    mismatch.source_report_id = Uuid::new_v4();
    assert!(matches!(
        admitted_metadata::prepare(
            &f.pool,
            &f.key,
            &ReleaseReadiness::for_tests(),
            &f.ctx,
            mismatch
        )
        .await,
        Err(crm_api::domain::migration::MigrationError::Conflict)
    ));
    admitted_metadata::cancel(
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
    assert_eq!(
        admitted_metadata::replay_prepare(&f.pool, &f.key, &f.ctx, &make())
            .await
            .unwrap()
            .unwrap(),
        receipt
    );
    let mut next = make();
    next.request_id = Uuid::new_v4();
    let new = admitted_metadata::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        next,
    )
    .await
    .unwrap();
    let new_root = Uuid::parse_str(new["import"]["id"].as_str().unwrap()).unwrap();
    assert_ne!(new_root, root);
    drain_preparation(&f, new_root).await;
    assert_eq!(
        admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap()["state"],
        "cancelled"
    );
    assert_eq!(
        admitted_metadata::replay_prepare(&f.pool, &f.key, &f.ctx, &make())
            .await
            .unwrap()
            .unwrap(),
        receipt
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_readers_bind_pages_and_lossless_segments_to_frozen_evidence(
    migrator: PgPool,
) {
    use crm_api::domain::migration::MigrationError;
    let (f, root, plan, person) = prepared_typed(&migrator).await;
    let q = || admitted_metadata::PlanPage {
        limit: Some(1),
        ..Default::default()
    };
    let first = admitted_metadata::mappings(&f.pool, &f.key, &f.ctx, root, plan, q())
        .await
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let token = first["next_cursor"].as_str().unwrap().to_owned();
    let mut next = q();
    next.cursor = Some(token.clone());
    let second = admitted_metadata::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        plan,
        admitted_metadata::PlanPage {
            cursor: Some(token.clone()),
            ..q()
        },
    )
    .await
    .unwrap();
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    assert!(matches!(
        admitted_metadata::records(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            plan,
            admitted_metadata::PlanPage {
                cursor: Some(token.clone()),
                ..q()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    next.kind = Some("field".into());
    assert!(matches!(
        admitted_metadata::mappings(&f.pool, &f.key, &f.ctx, root, plan, next).await,
        Err(MigrationError::InvalidInput)
    ));
    let record =
        admitted_metadata::records(&f.pool, &f.key, &f.ctx, root, plan, Default::default())
            .await
            .unwrap();
    let record = Uuid::parse_str(record["items"][0]["id"].as_str().unwrap()).unwrap();
    let mut parts = String::new();
    let mut cursor = None;
    for _ in 0..500 {
        let part = admitted_metadata::field(
            &f.pool,
            &f.key,
            &f.ctx,
            admitted_metadata::FieldOwner::Record {
                root,
                plan,
                id: record,
            },
            "source.all",
            admitted_metadata::FieldQuery {
                cursor,
                limit: Some(13),
            },
        )
        .await
        .unwrap();
        assert_eq!(part["offset_bytes"], parts.len().to_string());
        parts.push_str(part["text"].as_str().unwrap());
        cursor = part["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(part["full_utf8_bytes"], parts.len().to_string());
            break;
        }
    }
    assert!(cursor.is_none());
    assert!(parts.contains("Exact retained text"));
    let confirmed = approve_all(&f, root, plan).await;
    let mut stale = q();
    stale.cursor = Some(token);
    assert!(matches!(
        admitted_metadata::mappings(&f.pool, &f.key, &f.ctx, root, plan, stale).await,
        Err(MigrationError::InvalidInput)
    ));
    finish(&f, root).await;
    let provenance =
        admitted_metadata::provenance(&f.pool, &f.key, &f.ctx, person, Default::default())
            .await
            .unwrap();
    assert_eq!(provenance["items"].as_array().unwrap().len(), 1);
    assert_eq!(provenance["items"][0]["plan_id"], confirmed.to_string());
    assert_eq!(
        provenance["items"][0]["admitted_metadata_import_id"],
        root.to_string()
    );
    let detail = admitted_metadata::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(detail["counts"]["people"]["settled"], "1");
    assert_eq!(detail["counts"]["values"]["pending"], "0");
    assert_eq!(detail["counts"]["values"]["applied"], "4");
    assert_eq!(detail["cohort_counts"]["remaining_people"], "0");
}

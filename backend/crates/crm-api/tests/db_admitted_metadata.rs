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
    let (f, root, plan, person) = prepared_typed_native(&migrator, true).await;
    let maps=sqlx::query("SELECT m.id,CASE WHEN m.kind='tag' THEN (SELECT id FROM tag WHERE organization_id=m.organization_id AND name='Past Client') WHEN m.source_id='21' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Text') WHEN m.source_id='22' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Number') WHEN m.source_id='23' THEN (SELECT id FROM custom_field WHERE organization_id=m.organization_id AND label='Date') END AS target FROM migration_admitted_metadata_mapping m WHERE m.import_id=$1 ORDER BY m.kind,m.id").bind(root).fetch_all(&f.pool).await.unwrap();
    admitted_metadata::apply_mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_metadata::MappingPatch {
            request_id: Uuid::new_v4(),
            mappings: maps
                .iter()
                .map(|m| {
                    let target = m.get::<Option<Uuid>, _>("target");
                    admitted_metadata::MappingChoice {
                        id: m.get("id"),
                        disposition: if target.is_some() {
                            "map_existing"
                        } else {
                            "create_matching"
                        }
                        .into(),
                        target_id: target,
                    }
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    // Edits after preview must be held even when incoming content is now equal.
    sqlx::query("DELETE FROM person_tag WHERE organization_id=$1 AND person_id=$2")
        .bind(f.org)
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,date_value,updated_by_user_id,origin,correlation_id) SELECT $1,$2,id,'date','2024-02-29',$3,'web_session',$4 FROM custom_field WHERE organization_id=$1 AND label='Date'").bind(f.org).bind(person).bind(f.actor).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
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
    approve_all(&f, root, plan).await;
    finish_catalog(&f, root).await;
    let row=sqlx::query("SELECT x.manifest_id,x.target_id,encode(m.source_key,'hex') AS source_key FROM migration_admitted_metadata_operation x JOIN migration_admitted_metadata_mapping m ON m.id=x.mapping_id WHERE x.import_id=$1 AND x.kind='tag_link'").bind(root).fetch_one(&f.pool).await.unwrap();
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
    approve_all(&f, root, plan).await;
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
    sqlx::query("INSERT INTO migration_admitted_metadata_manifest(id,import_id,plan_id,organization_id,admission_result_id,person_id,source_person_id,disposition,baseline_nonce,baseline_ciphertext,item_byte_bound,settled_at) SELECT x.manifest,m.import_id,m.plan_id,m.organization_id,m.admission_result_id,x.person,(1000000+x.n)::text,'settled',m.baseline_nonce,m.baseline_ciphertext,m.item_byte_bound,clock_timestamp() FROM admitted_scale x CROSS JOIN migration_admitted_metadata_manifest m WHERE m.import_id=$1 AND m.person_id=$2").bind(root).bind(person).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,mapping_id,source_key,target_id,disposition,nonce,ciphertext) SELECT gen_random_uuid(),x.manifest,o.import_id,o.plan_id,o.organization_id,o.kind,o.mapping_id,o.source_key,o.target_id,o.disposition,o.nonce,o.ciphertext FROM admitted_scale x CROSS JOIN migration_admitted_metadata_operation o WHERE o.import_id=$1 AND o.manifest_id=(SELECT id FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND person_id=$2)").bind(root).bind(person).execute(&mut *c).await.unwrap();
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

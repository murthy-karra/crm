//! Explicit synthetic browser harness, excluded from normal check-db runs.
//! Production router/commands and admitted-metadata worker; synthetic fixture reader,
//! test-only keys/readiness and bounded worker scheduling are explicit harness
//! dependencies, not production startup or operator-owned release evidence.
//! Run only with --features perf-harness and the exact ignored test name.
//! This binds isolated API3104 before touching the guarded crm_010f3_qa database.
//! No live FUB reader, provider configuration, shared tenant or demo app is used.
#![cfg(feature = "perf-harness")]
use crate::{common, db_admitted_people_refresh_execution as execution, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_metadata_worker, snapshot_source::Stream},
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::{io::Write, os::unix::fs::OpenOptionsExt, str::FromStr, sync::Arc};
use uuid::Uuid;

#[tokio::test]
#[ignore = "explicit synthetic API3104 browser harness; long-lived until stopped"]
async fn serve_admitted_metadata_ui_fixture() {
    assert_eq!(
        std::env::var("CRM_ADMITTED_METADATA_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(options.get_database(), Some("crm_010f3_qa"));
    assert_eq!(options.get_username(), "crm_migrator");
    assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3104")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();
    if std::env::var("CRM_ADMITTED_METADATA_UI_SEED").as_deref() == Ok("create-synthetic-fixture") {
        let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM organization)")
            .fetch_one(&migrator)
            .await
            .unwrap();
        assert!(
            empty,
            "Synthetic fixture creation requires this empty isolated database"
        );
        // Enough settled People and catalog definitions to exercise both bounded
        // display pages. All data is fabricated and captured through the real
        // retained-source/import/admission path before the browser starts.
        let people: Vec<Value> = (106..162)
            .map(|id| {
                json!({
                    "id":id,"firstName":"Metadata cohort","lastName":id.to_string(),
                    "stage":"Lead","assignedUserId":3,
                    "emails":[{"value":format!("metadata-{id}@synthetic.test")}]
                })
            })
            .collect();
        let (f, parent, admission) =
            execution::fixture_with_admission(&migrator, people.clone()).await;
        let mut fields = vec![
            json!({"id":21,"name":"customText","label":"Text","type":"text","isRecurring":false}),
            json!({"id":22,"name":"customNumber","label":"Number","type":"number","isRecurring":false}),
            json!({"id":23,"name":"customDate","label":"Date","type":"date","isRecurring":false}),
            json!({"id":24,"name":"customChoice","label":"Choice","type":"dropdown","choices":["North","South"]}),
        ];
        fields.extend((25..77).map(|id|json!({"id":id,"name":format!("customUnused{id}"),"label":format!("Unused source field {id}"),"type":"text"})));
        f.reader.set_records(Stream::CustomFields, fields);
        let incoming: Vec<Value> = people
            .into_iter()
            .map(|mut person| {
                person["tags"] = json!(["Shared cohort tag"]);
                person["customText"] = json!("Exact retained text");
                person["customNumber"] = json!(123456.125);
                person["customDate"] = json!("2024-02-29");
                person["customChoice"] = json!("North");
                if person["id"] == 108 {
                    person["customText"] = json!("Long retained 日本語 evidence. ".repeat(400));
                }
                if person["id"] == 109 {
                    person["customDate"] = Value::Null;
                }
                person
            })
            .collect();
        let report = execution::report(&f, parent, incoming).await;
        let person: Uuid = sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='107' AND disposition='settled'")
            .bind(admission).fetch_one(&f.pool).await.unwrap();
        // Explicit synthetic native comparison fixtures: this SQL is not an
        // application write route and cannot weaken the migration-review hold.
        for (label, kind, position) in [("Text", "text", 0), ("Number", "number", 1)] {
            let field = Uuid::new_v4();
            sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(field).bind(f.org).bind(label).bind(kind).bind(position).bind(f.actor).execute(&migrator).await.unwrap();
            sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,number_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,$4,$5,$6,$7,'web_session',$8)")
                .bind(f.org).bind(person).bind(field).bind(kind).bind(if kind=="text" {Some("Exact retained text")} else {None})
                .bind(if kind=="number" {Some(999i64)} else {None}).bind(f.actor).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
        }
        let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
            .bind(f.actor)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let inventory = json!({"synthetic_only":true,"organization_id":f.org,"actor_id":f.actor,"email":email,"password":"synthetic import fixture password","parent_import_id":parent,"admission_id":admission,"report_id":report,"baseline_snapshot_id":f.snapshot,"source_reader_calls_before_metadata":f.reader.calls()});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open("/private/tmp/crm-mobile005-010f3/integration/metadata-ui/ui-fixture.json")
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&inventory).unwrap())
            .unwrap();
        f.pool.close().await;
    }
    let pool = common::connect_as_app(&migrator).await;
    let mut config = common::test_config();
    config.cors_allowed_origin = Some("http://127.0.0.1:5174".into());
    let retained_only_reader = Arc::new(import_support::Book::new(vec![]));
    let mut state = AppState::for_tests(pool.clone(), &config, Publisher::recording())
        .with_migration_reader(retained_only_reader.clone());
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    // Root can grant bounded execution units after a real UI confirmation.
    // Preparation runs normally; no direct state mutation or fixture endpoint
    // substitutes for the typed cancel/remainder commands.
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        // A tested-binary restart must not grant the previous execution budget
        // again or destroy the partial-cancellation observation boundary.
        let mut execution_units = std::fs::read(
            "/private/tmp/crm-mobile005-010f3/integration/metadata-ui/worker-stats.json",
        )
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value["execution_units"].as_u64())
        .unwrap_or(0);
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            interval.tick().await;
            for _ in 0..32 {
                let pending: Option<String> = sqlx::query_scalar("SELECT state FROM migration_admitted_metadata_import i WHERE state IN ('queued','running') OR state='proposed' AND EXISTS(SELECT 1 FROM migration_admitted_metadata_plan p WHERE p.id=i.latest_plan_id AND p.state='building') ORDER BY created_at,id LIMIT 1")
                    .fetch_optional(worker_state.db.as_ref().unwrap()).await.unwrap();
                let executing = pending
                    .as_deref()
                    .is_some_and(|s| s == "queued" || s == "running");
                let budget = std::fs::read(
                    "/private/tmp/crm-mobile005-010f3/integration/metadata-ui/worker-units.json",
                )
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value["execution_units"].as_u64())
                .unwrap_or(0);
                if executing && execution_units >= budget {
                    break;
                }
                match admitted_metadata_worker::run_once(
                    worker_state.db.as_ref().unwrap(),
                    &worker_state.raw_payload_key,
                )
                .await
                {
                    Ok(true) => {
                        if executing {
                            execution_units += 1;
                        }
                        let stats = json!({"execution_units":execution_units,"source_reader_calls":retained_only_reader.calls()});
                        std::fs::write(
                            "/private/tmp/crm-mobile005-010f3/integration/metadata-ui/worker-stats.json",
                            serde_json::to_vec(&stats).unwrap(),
                        )
                        .unwrap();
                    }
                    Ok(false) => break,
                    Err(error) => {
                        let _ = error;
                        eprintln!("Synthetic admitted metadata worker returned an error; inspect retained state");
                        break;
                    }
                }
            }
        }
    });
    eprintln!("Synthetic admitted metadata API listening on 127.0.0.1:3104; database crm_010f3_qa");
    axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    worker.abort();
    pool.close().await;
    migrator.close().await;
}

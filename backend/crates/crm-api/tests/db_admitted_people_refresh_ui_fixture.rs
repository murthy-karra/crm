//! Explicit synthetic browser harness, excluded from normal check-db runs.
//! Production router/commands and admitted-refresh worker; synthetic fixture reader,
//! test-only keys/readiness and bounded worker scheduling are explicit harness
//! dependencies, not production startup or operator-owned release evidence.
//! Run only with --features perf-harness and the exact ignored test name.
//! This binds isolated API3103 before touching the guarded crm_010e4_qa database.
//! No live FUB reader, provider configuration, shared tenant or demo app is used.
#![cfg(feature = "perf-harness")]
use crate::{common, db_admitted_people_refresh_execution as execution, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_people_refresh_worker,
    },
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::{io::Write, os::unix::fs::OpenOptionsExt, str::FromStr, sync::Arc};

#[tokio::test]
#[ignore = "explicit synthetic API3103 browser harness; long-lived until stopped"]
async fn serve_admitted_people_refresh_ui_fixture() {
    assert_eq!(
        std::env::var("CRM_ADMITTED_REFRESH_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(options.get_database(), Some("crm_010e4_qa"));
    assert_eq!(options.get_username(), "crm_migrator");
    assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3103")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();
    if std::env::var("CRM_ADMITTED_REFRESH_UI_SEED").as_deref() == Ok("create-synthetic-fixture") {
        let people = vec![
            json!({"id":106,"firstName":"Later admitted","lastName":"Contact review","stage":"Lead","assignedUserId":3,"emails":[{"value":"shared@synthetic.test"}]}),
            json!({"id":107,"firstName":"Distinct shared contact","stage":"Lead","assignedUserId":3,"emails":[{"value":"shared@synthetic.test"}]}),
            json!({"id":108,"firstName":"Local hold baseline","stage":"Lead","assignedUserId":3}),
        ];
        let (f, parent, admission) = execution::fixture_with_admission(&migrator, people).await;
        let contacts: Vec<Value> = (1..=57).map(|n| json!({"value":format!("review.{n}@synthetic.test"),"isPrimary":n==57})).collect();
        let report = execution::report(&f, parent, vec![
            json!({"id":106,"firstName":"Updated later Person","lastName":"Contact review","stage":"Lead","assignedUserId":3,"emails":contacts}),
            json!({"id":107,"firstName":"Second updated Person","stage":"Lead","assignedUserId":3,"emails":[{"value":"shared@synthetic.test"}]}),
            json!({"id":108,"firstName":"Source proposed name","stage":"Lead","assignedUserId":3}),
        ]).await;
        // A synthetic concurrent local edit, performed after genuine admission.
        // This must hold the entire Person in preview and execution.
        sqlx::query("UPDATE person SET first_name='Retained local edit' WHERE id=(SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='108' AND disposition='settled')")
            .bind(admission).execute(&migrator).await.unwrap();
        let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
            .bind(f.actor)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let inventory = json!({"synthetic_only":true,"organization_id":f.org,"actor_id":f.actor,"email":email,"password":"synthetic import fixture password","parent_import_id":parent,"admission_id":admission,"report_id":report,"baseline_snapshot_id":f.snapshot,"source_reader_calls_before_refresh":f.reader.calls()});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open("/private/tmp/crm-mobile004-010e4/migration/ui-fixture.json")
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&inventory).unwrap())
            .unwrap();
        f.pool.close().await;
    }
    let pool = common::connect_as_app(&migrator).await;
    let mut config = common::test_config();
    config.cors_allowed_origin = Some("http://127.0.0.1:5174".into());
    let mut state = AppState::for_tests(pool.clone(), &config, Publisher::recording())
        .with_migration_reader(Arc::new(import_support::Book::new(vec![])));
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    // Root can grant bounded execution units after a real UI confirmation.
    // Preparation runs normally; no direct state mutation or fixture endpoint
    // substitutes for the typed cancel/remainder commands.
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        let mut execution_units = 0_u64;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            interval.tick().await;
            for _ in 0..32 {
                let pending: Option<String> = sqlx::query_scalar("SELECT state FROM migration_admitted_people_refresh WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1")
                    .fetch_optional(worker_state.db.as_ref().unwrap()).await.unwrap();
                let executing = pending
                    .as_deref()
                    .is_some_and(|s| s == "queued" || s == "running");
                let budget = std::fs::read("/private/tmp/crm-mobile004-010e4/migration/worker-units.json")
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .and_then(|value| value["execution_units"].as_u64())
                    .unwrap_or(0);
                if executing && execution_units >= budget {
                    break;
                }
                match admitted_people_refresh_worker::run_once(
                    worker_state.db.as_ref().unwrap(),
                    &worker_state.raw_payload_key,
                    &worker_state.snapshot_policy,
                    worker_state.import_release.as_deref(),
                )
                .await
                {
                    Ok(true) => {
                        if executing {
                            execution_units += 1;
                        }
                        let stats = json!({"execution_units":execution_units});
                        std::fs::write(
                            "/private/tmp/crm-mobile004-010e4/migration/worker-stats.json",
                            serde_json::to_vec(&stats).unwrap(),
                        )
                        .unwrap();
                    }
                    Ok(false) => break,
                    Err(error) => {
                        eprintln!("Synthetic admitted refresh worker paused by error: {error:?}");
                        break;
                    }
                }
            }
        }
    });
    eprintln!("Synthetic admitted refresh API listening on 127.0.0.1:3103; database crm_010e4_qa");
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

//! Opt-in retained synthetic browser fixture for Slice 010f4.
#![cfg(feature = "perf-harness")]
use crate::{common, db_admitted_people_refresh_execution as execution, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_activity_worker, snapshot_source::Stream},
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::{io::Write, os::unix::fs::OpenOptionsExt, str::FromStr, sync::Arc};
#[tokio::test]
#[ignore = "explicit synthetic API3107/browser5177 fixture"]
async fn serve_admitted_activity_ui_fixture() {
    let ui_directory = std::path::PathBuf::from(
        std::env::var("CRM_ADMITTED_ACTIVITY_UI_DIR")
            .expect("absolute owned UI evidence directory"),
    );
    assert!(ui_directory.is_absolute());
    assert_eq!(
        std::env::var("CRM_ADMITTED_ACTIVITY_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let opt = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(opt.get_database(), Some("crm_010f4_qa"));
    assert_eq!(opt.get_username(), "crm_migrator");
    assert!(matches!(opt.get_host(), "127.0.0.1" | "localhost"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3107")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(opt).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();
    let seed = std::env::var("CRM_ADMITTED_ACTIVITY_UI_SEED").unwrap_or_default();
    if matches!(
        seed.as_str(),
        "create-synthetic-fixture" | "create-synthetic-fixture-v2"
    ) {
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM organization")
                .fetch_one(&migrator)
                .await
                .unwrap(),
            if seed.ends_with("-v2") { 1 } else { 0 },
            "preserve retained fixtures; seed only the explicitly owned next attempt"
        );
        let people:Vec<Value>=(104..164).map(|id|json!({"id":id,"firstName":"Activity","lastName":id.to_string(),"stage":"Lead","assignedUserId":3})).collect();
        let (f, parent, admission) =
            execution::fixture_with_admission(&migrator, people.clone()).await;
        f.reader.set_records(
            Stream::Users,
            vec![json!({"id":3,"name":"Synthetic author","timezone":"America/Los_Angeles"})],
        );
        f.reader.set_records(Stream::Notes,people.iter().take(1).map(|p|json!({"id":p["id"],"personId":p["id"],"body":"Retained QA note","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false})).collect());
        f.reader.set_records(Stream::TasksOpen,people.iter().map(|p|json!({"id":1000+p["id"].as_i64().unwrap(),"personId":p["id"],"name":"Call synthetic Person","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z"})).collect());
        f.reader.set_records(Stream::TasksCompleted, vec![]);
        f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":104,"personId":104,"body":"Frozen synthetic detail","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false,"type":"Note"})).unwrap(),false);
        let report = execution::report(&f, parent, people).await;
        std::fs::create_dir_all(&ui_directory).unwrap();
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(ui_directory.join("ui-fixture.json"))
            .unwrap();
        let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
            .bind(f.actor)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        out.write_all(&serde_json::to_vec_pretty(&json!({"synthetic_only":true,
            "organization_id":f.org,"actor_id":f.actor,"email":email,
            "password":"synthetic import fixture password",
            "parent_import_id":parent,"admission_id":admission,"report_id":report,
            "api":"http://127.0.0.1:3107","web":"http://127.0.0.1:5177",
            "worker_control":ui_directory.join("worker-units.json"),
            "journey":"desktop then 390px: prepare qualified six streams, map all four roles, confirm, cancel, continue never-settled remainder, reconcile"})).unwrap()).unwrap();
        f.pool.close().await;
    }
    let pool = common::connect_as_app(&migrator).await;
    let mut cfg = common::test_config();
    cfg.cors_allowed_origin = Some("http://127.0.0.1:5177".into());
    let retained_only_reader = Arc::new(import_support::Book::new(vec![]));
    let mut state = AppState::for_tests(pool.clone(), &cfg, Publisher::recording())
        .with_migration_reader(retained_only_reader.clone());
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    std::fs::create_dir_all(&ui_directory).unwrap();
    let control = ui_directory.join("worker-units.json");
    if !control.exists() {
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&control)
            .unwrap()
            .write_all(b"{\"allowed_units\":0}\n")
            .unwrap();
    }
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        let stats = ui_directory.join("worker-stats.json");
        let read = |path: &std::path::Path| {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null)
        };
        let mut completed = read(&stats)["completed_units"].as_u64().unwrap_or(0);
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            tick.tick().await;
            let allowed = read(&control)["allowed_units"].as_u64().unwrap_or(0);
            if completed >= allowed {
                continue;
            }
            let result = admitted_activity_worker::run_once(
                worker_state.db.as_ref().unwrap(),
                &worker_state.raw_payload_key,
                &worker_state.snapshot_policy,
            )
            .await;
            let failed = result.is_err();
            match result {
                Ok(true) => completed += 1,
                // Consume the unused allowance when idle; a paused review must
                // not keep polling PostgreSQL outside the coordinator slot.
                Ok(false) => completed = allowed,
                Err(_) => completed = allowed,
            }
            let value = json!({"completed_units":completed,"last_unit_failed":failed,
                "source_reader_calls":retained_only_reader.calls()});
            let temporary = stats.with_extension("new");
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .unwrap();
            file.write_all(&serde_json::to_vec(&value).unwrap())
                .unwrap();
            file.sync_all().unwrap();
            std::fs::rename(temporary, &stats).unwrap();
        }
    });
    eprintln!("010f4 synthetic API http://127.0.0.1:3107; start Web at http://127.0.0.1:5177; execute desktop and 390px prepare/map/confirm/cancel/remainder journey.");
    axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    worker.abort();
    let _ = worker.await;
    pool.close().await;
    migrator.close().await;
}

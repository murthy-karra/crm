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
    if std::env::var("CRM_ADMITTED_ACTIVITY_UI_SEED").as_deref() == Ok("create-synthetic-fixture") {
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT NOT EXISTS(SELECT 1 FROM organization)")
                .fetch_one(&migrator)
                .await
                .unwrap()
        );
        let people:Vec<Value>=(104..164).map(|id|json!({"id":id,"firstName":"Activity","lastName":id.to_string(),"stage":"Lead","assignedUserId":3})).collect();
        let (f, parent, admission) =
            execution::fixture_with_admission(&migrator, people.clone()).await;
        f.reader.set_records(
            Stream::Users,
            vec![json!({"id":3,"name":"Synthetic author","timezone":"America/Los_Angeles"})],
        );
        f.reader.set_records(Stream::Notes,people.iter().map(|p|json!({"id":p["id"],"personId":p["id"],"body":"Retained QA note","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false})).collect());
        f.reader.set_records(Stream::TasksOpen,people.iter().map(|p|json!({"id":1000+p["id"].as_i64().unwrap(),"personId":p["id"],"name":"Call synthetic Person","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z"})).collect());
        f.reader.set_records(Stream::TasksCompleted, vec![]);
        f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":104,"personId":104,"body":"Frozen synthetic detail","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false,"type":"Note"})).unwrap(),false);
        let report = execution::report(&f, parent, people).await;
        std::fs::create_dir_all(
            "/private/tmp/crm-mobile006-010f4-thyhauvv/integration/activity-ui",
        )
        .unwrap();
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(
                "/private/tmp/crm-mobile006-010f4-thyhauvv/integration/activity-ui/ui-fixture.json",
            )
            .unwrap();
        out.write_all(&serde_json::to_vec_pretty(&json!({"synthetic_only":true,"parent_import_id":parent,"admission_id":admission,"report_id":report,"api":"http://127.0.0.1:3107","web":"http://127.0.0.1:5177","journey":"desktop then 390px: prepare qualified six streams, map all four roles, confirm, cancel, continue never-settled remainder, reconcile"})).unwrap()).unwrap();
        f.pool.close().await;
    }
    let pool = common::connect_as_app(&migrator).await;
    let mut cfg = common::test_config();
    cfg.cors_allowed_origin = Some("http://127.0.0.1:5177".into());
    let mut state = AppState::for_tests(pool.clone(), &cfg, Publisher::recording())
        .with_migration_reader(Arc::new(import_support::Book::new(vec![])));
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    eprintln!("010f4 synthetic API http://127.0.0.1:3107; start Web at http://127.0.0.1:5177; execute desktop and 390px prepare/map/confirm/cancel/remainder journey.");
    axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    pool.close().await;
    migrator.close().await;
}

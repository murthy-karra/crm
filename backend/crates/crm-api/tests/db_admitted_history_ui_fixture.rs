//! Opt-in synthetic browser harness for Slice 010d3.
//!
//! This test owns only the isolated 010d3 UI database and loopback API. It
//! creates the same retained admission/capture evidence used by the DB
//! acceptance tests, then leaves preparation and bounded apply under a small
//! file-controlled worker gate for a real browser journey.
#![cfg(feature = "perf-harness")]

use crate::{common, db_admitted_history, db_history_capture_support, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::{
        envelope::{CommandContext, Origin},
        migration::{
            admitted_history_worker, history_capture, history_capture_source::Stream,
            history_capture_worker,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool, Row};
use std::{io::Write, os::unix::fs::OpenOptionsExt, str::FromStr, sync::Arc};
use uuid::Uuid;

async fn create_held_capture(migrator: &PgPool, org: Uuid, actor: Uuid, admission: Uuid) -> Uuid {
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_people_admission WHERE id=$1 AND organization_id=$2",
    )
    .bind(admission)
    .bind(org)
    .fetch_one(migrator)
    .await
    .unwrap();
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(org)
            .fetch_one(migrator)
            .await
            .unwrap();
    let config = common::test_config();
    let policy = crm_api::domain::migration::snapshot::SnapshotPolicy::default();
    let ctx = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(actor),
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let proposed = history_capture::propose(
        migrator,
        &config.raw_payload_key,
        &policy,
        &ctx,
        history_capture::ProposeFubHistoryCapture {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            connection_id: connection.get("id"),
            expected_revision: connection.get::<i32, _>("revision").to_string(),
        },
    )
    .await
    .unwrap();
    let capture_id: Uuid = proposed["capture_id"].as_str().unwrap().parse().unwrap();
    let before = history_capture::detail(migrator, &policy, &ctx, capture_id)
        .await
        .unwrap();
    history_capture::confirm(
        migrator,
        &config.raw_payload_key,
        &policy,
        &ctx,
        capture_id,
        history_capture::ConfirmFubHistoryCapture {
            request_id: Uuid::new_v4(),
            expected_run_revision: before["revision"].as_str().unwrap().into(),
            acknowledgements: history_capture::Acknowledgements {
                api_visible_account_scope: true,
                coverage_gaps: true,
                retained_not_imported: true,
                source_user_evidence_revision: before["source_user_evidence_revision"]
                    .as_str()
                    .unwrap()
                    .into(),
                source_user_difference: before["source_user_difference"].as_bool().unwrap(),
            },
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let book = db_history_capture_support::HistoryBook::new();
    book.set_records(
        Stream::Events,
        vec![
            json!({"id":901,"personId":104,"participants":[{"type":"person","personId":105}],"type":"Inquiry","created":"2026-01-04T00:00:00Z","description":"H3_HELD_AMBIGUOUS"}),
            json!({"id":902,"personId":104,"type":"Inquiry","created":"2026-01-05T00:00:00Z","description":"H3_VALID_EVENT"}),
        ],
    );
    book.set_records(
        Stream::Calls,
        vec![json!({"id":903,"personId":104,"userId":3,"created":"2026-01-06T00:00:00Z","duration":1.5})],
    );
    book.set_records(
        Stream::TextMessages,
        vec![json!({"id":904,"personId":104,"created":"unknown","sent":"2026-01-07T00:00:00Z","message":"H3_VALID_TEXT"})],
    );
    for _ in 0..200 {
        if !history_capture_worker::run_once(
            migrator,
            &config.raw_payload_key,
            book.as_ref(),
            &policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    let finished = history_capture::detail(migrator, &policy, &ctx, capture_id)
        .await
        .unwrap();
    assert_eq!(finished["state"], "completed_with_gaps");
    capture_id
}

#[tokio::test]
#[ignore = "explicit synthetic API3108/browser5178 fixture"]
async fn serve_admitted_history_ui_fixture() {
    let ui_directory = std::path::PathBuf::from(
        std::env::var("CRM_ADMITTED_HISTORY_UI_DIR").expect("absolute owned UI evidence directory"),
    );
    assert!(ui_directory.is_absolute());
    assert_eq!(
        std::env::var("CRM_ADMITTED_HISTORY_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(options.get_database(), Some("crm_010d3_ui_20260915"));
    assert_eq!(options.get_username(), "crm_migrator");
    assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3108")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();

    if std::env::var("CRM_ADMITTED_HISTORY_UI_SEED").as_deref() == Ok("create-synthetic-fixture") {
        let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM organization)")
            .fetch_one(&migrator)
            .await
            .unwrap();
        assert!(
            empty,
            "fixture creation requires an empty isolated database"
        );
        let (fixture, admission, capture) = db_admitted_history::fixture_count(&migrator, 75).await;
        std::fs::create_dir_all(&ui_directory).unwrap();
        let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
            .bind(fixture.actor)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(ui_directory.join("ui-fixture.json"))
            .unwrap();
        output
            .write_all(
                &serde_json::to_vec_pretty(&json!({
                    "synthetic_only": true,
                    "organization_id": fixture.org,
                    "actor_id": fixture.actor,
                    "email": email,
                    "password": "synthetic import fixture password",
                    "admission_id": admission,
                    "history_capture_id": capture,
                    "api": "http://127.0.0.1:3108",
                    "web": "http://127.0.0.1:5178",
                    "worker_control": ui_directory.join("worker-units.json"),
                    "worker_stats": ui_directory.join("worker-stats.json"),
                    "journey": "desktop and 390px: prepare, review coverage, confirm, apply 50, cancel, create remainder, finish"
                }))
                .unwrap(),
            )
            .unwrap();
        fixture.pool.close().await;
    }

    let pool = common::connect_as_app(&migrator).await;
    if std::env::var("CRM_ADMITTED_HISTORY_UI_HELD").as_deref() == Ok("create-held-capture") {
        let fixture: Value =
            serde_json::from_slice(&std::fs::read(ui_directory.join("ui-fixture.json")).unwrap())
                .unwrap();
        let capture_id = create_held_capture(
            &migrator,
            fixture["organization_id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            fixture["actor_id"].as_str().unwrap().parse().unwrap(),
            fixture["admission_id"].as_str().unwrap().parse().unwrap(),
        )
        .await;
        std::fs::write(
            ui_directory.join("held-capture.json"),
            serde_json::to_vec_pretty(&json!({
                "capture_id": capture_id,
                "held_source_id": "901",
                "held_reason": "ambiguous_relationship"
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let mut config = common::test_config();
    config.cors_allowed_origin = Some("http://127.0.0.1:5178".into());
    let retained_only_reader = Arc::new(import_support::Book::new(vec![]));
    let mut state = AppState::for_tests(pool.clone(), &config, Publisher::recording())
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
            .write_all(b"{\"allowed_units\":0,\"release_ready\":true}\n")
            .unwrap();
    }
    let stats = ui_directory.join("worker-stats.json");
    let worker_state = state.clone();
    let worker_control = control.clone();
    let worker_stats = stats.clone();
    let worker = tokio::spawn(async move {
        let read = |path: &std::path::Path| {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null)
        };
        let mut applied_units = read(&worker_stats)["applied_units"].as_u64().unwrap_or(0);
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            tick.tick().await;
            let pending = sqlx::query("SELECT state,phase FROM migration_admitted_history_root WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1")
                .fetch_optional(worker_state.db.as_ref().unwrap())
                .await
                .unwrap();
            let Some(pending) = pending else { continue };
            let applying = pending.get::<String, _>("phase") == "applying";
            let control = read(&worker_control);
            let allowed = control["allowed_units"].as_u64().unwrap_or(0);
            if applying && applied_units >= allowed {
                continue;
            }
            let release_ready = control["release_ready"].as_bool().unwrap_or(true);
            let result = admitted_history_worker::run_once(
                worker_state.db.as_ref().unwrap(),
                &worker_state.raw_payload_key,
                &worker_state.snapshot_policy,
                if release_ready {
                    worker_state.import_release.as_deref()
                } else {
                    None
                },
            )
            .await;
            let failed = result.is_err();
            if applying && matches!(result, Ok(true)) {
                applied_units += 1;
            }
            let value = json!({
                "applied_units": applied_units,
                "last_unit_failed": failed,
                "source_reader_calls": retained_only_reader.calls()
            });
            let temporary = worker_stats.with_extension("new");
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
            std::fs::rename(&temporary, &worker_stats).unwrap();
        }
    });
    eprintln!("010d3 synthetic API http://127.0.0.1:3108; start Web at http://127.0.0.1:5178; execute desktop and 390px history review journey.");
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

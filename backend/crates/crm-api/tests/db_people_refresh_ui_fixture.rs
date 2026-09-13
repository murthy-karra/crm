//! Explicit synthetic browser harness, excluded from normal check-db runs.
//! Run only with --features perf-harness and the exact ignored test name.
//! This binds isolated API3103 before touching the guarded crm_010e2_qa database.
//! No live FUB reader, provider configuration, shared tenant or demo app is used.
#![cfg(feature = "perf-harness")]
use crate::{common, db_activity_source, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, core_change_worker, people_refresh_worker,
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_source::Stream,
    },
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool, Row};
use std::{io::Write, os::unix::fs::OpenOptionsExt, str::FromStr, sync::Arc};
use uuid::Uuid;
fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}

#[tokio::test]
#[ignore = "explicit synthetic API3103 browser harness; long-lived until stopped"]
async fn serve_people_refresh_ui_fixture() {
    assert_eq!(
        std::env::var("CRM_PEOPLE_REFRESH_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(options.get_database(), Some("crm_010e2_qa"));
    assert_eq!(options.get_username(), "crm_migrator");
    assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3103")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();
    if std::env::var("CRM_PEOPLE_REFRESH_UI_SEED").as_deref() == Ok("create-synthetic-fixture") {
        let people = vec![
            json!({"id":101,"firstName":"Synthetic Preview","lastName":"Clear me","stage":"Lead","assignedUserId":3,"assignedPondId":null,"emails":[{"value":"first@synthetic.test"},{"value":"remove@synthetic.test"}],"phones":[{"value":"4155550101"}]}),
            json!({"id":102,"firstName":"Synthetic Local Conflict","stage":"Lead","phones":[{"value":"4155550102"}]}),
            json!({"id":103,"firstName":"Synthetic Original Hold","stage":"Lead","isTrash":true}),
            json!({"id":104,"firstName":"Synthetic Not Seen Again","stage":"Lead","phones":[{"value":"4155550104"}]}),
            json!({"id":105,"firstName":"Synthetic Already Current","stage":"Lead","phones":[{"value":"4155550105"}]}),
        ];
        let f = import_support::fixture(&migrator, people).await;
        let parent = db_activity_source::completed_parent(&f).await;
        sqlx::query("UPDATE person SET first_name='Synthetic protected local edit' WHERE organization_id=$1 AND id=(SELECT person_id FROM migration_import_result WHERE organization_id=$1 AND import_id=$2 AND source_id='102')")
            .bind(f.org).bind(parent).execute(&migrator).await.unwrap();
        f.reader.set_records(Stream::People, vec![
            json!({"id":101,"firstName":"Synthetic Refreshed","lastName":null,"stage":"Lead","assignedUserId":null,"assignedPondId":null,"emails":[{"value":"first@synthetic.test"}],"phones":[{"value":"4155550199"},{"value":"4155550101"}]}),
            json!({"id":102,"firstName":"Synthetic Source Edit","stage":"Lead","phones":[{"value":"4155550102"}]}),
            json!({"id":103,"firstName":"Synthetic Previously Held","stage":"Lead","phones":[{"value":"4155550103"}]}),
            json!({"id":105,"firstName":"Synthetic Already Current","stage":"Lead","phones":[{"value":"4155550105"}]}),
            json!({"id":106,"firstName":"Synthetic New Source Only","stage":"Lead","phones":[{"value":"4155550106"}]}),
        ]);
        let connection =
            sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
                .bind(f.org)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        let proposed = snapshot::propose(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            snapshot::ProposeCoreSnapshot {
                request_id: Uuid::new_v4(),
                connection_id: connection.get("id"),
                expected_revision: connection.get("revision"),
            },
        )
        .await
        .unwrap();
        let newer = uuid(&proposed["snapshot"]["id"]);
        snapshot::source_action(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            newer,
            SnapshotRequest {
                request_id: Uuid::new_v4(),
            },
            SourceAction::Confirm,
        )
        .await
        .unwrap();
        import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
        let report = core_change_reports::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            core_change_reports::PrepareCoreChangeReport {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                newer_snapshot_id: newer,
            },
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap();
        let report = uuid(&report["report_id"]);
        for _ in 0..1000 {
            if !core_change_worker::run_once(
                &f.pool,
                &f.key,
                &f.policy,
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
            .unwrap()
            {
                break;
            }
        }
        assert_eq!(
            core_change_reports::detail(&f.pool, &f.key, &f.ctx, report)
                .await
                .unwrap()["state"],
            "completed"
        );
        let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
            .bind(f.actor)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let inventory = json!({"synthetic_only":true,"organization_id":f.org,"actor_id":f.actor,"email":email,"password":"synthetic import fixture password","parent_import_id":parent,"report_id":report,"baseline_snapshot_id":f.snapshot,"newer_snapshot_id":newer,"source_reader_calls_before_refresh":f.reader.calls()});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open("/private/tmp/crm-010e2-qa/ui-fixture.json")
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
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            interval.tick().await;
            for _ in 0..32 {
                match people_refresh_worker::run_once(
                    worker_state.db.as_ref().unwrap(),
                    &worker_state.raw_payload_key,
                    &worker_state.snapshot_policy,
                    worker_state.import_release.as_deref(),
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        eprintln!("Synthetic refresh worker paused by error: {error:?}");
                        break;
                    }
                }
            }
        }
    });
    eprintln!("Synthetic People refresh API listening on 127.0.0.1:3103; database crm_010e2_qa");
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

//! 010d2: actual read boundaries, capability lifetime and old-artifact denial.
use crate::{common, db_history_capture_support as capture, import_support::Fixture};
use axum::http::StatusCode;
use crm_api::{
    auth::workspace,
    domain::migration::{history_capture_source::Stream, history_import_worker},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn prepared(migrator: &PgPool) -> (Fixture, Uuid, Uuid, Value) {
    prepared_book(migrator, 1).await
}

async fn prepared_book(migrator: &PgPool, events: i64) -> (Fixture, Uuid, Uuid, Value) {
    let (f, parent, book) = capture::fixture(migrator).await;
    book.set_records(
        Stream::Events,
        (1..=events)
            .map(|id| {
                json!({
                    "id":id,"personId":101,"userId":3,"type":"Registration",
                    "created":"2026-01-02T03:04:05Z","message":"SYNTHETIC_UNREADABLE_BODY"
                })
            })
            .collect(),
    );
    book.set_records(Stream::Calls, if events > 1 { vec![json!({"id":501,"personId":101,"created":"2026-01-03T00:00:00Z","duration":12.5,"isIncoming":true,"outcome":"Source label","body":"SYNTHETIC_UNREADABLE_BODY"})] } else { vec![] });
    book.set_records(Stream::TextMessages, if events > 1 { vec![json!({"id":601,"personId":101,"created":"unknown","sent":"2026-01-04T00:00:00Z","status":"Source status","message":"SYNTHETIC_UNREADABLE_BODY"})] } else { vec![] });
    let (capture_id, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture_id).await;
    capture::drain(&f, &book).await;
    let history = capture::ready(&f, capture_id).await;
    assert_eq!(history["state"], "completed_with_gaps");
    let revision: i64 =
        sqlx::query_scalar("SELECT workspace_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let response = common::post_json_with_cookie(&f.app, "/api/migrations/fub/history-imports", &f.cookie, json!({
        "request_id":Uuid::new_v4(),"parent_import_id":parent,"capture_id":capture_id,
        "expected_capture_revision":history["revision"],
        "expected_workspace_revision":revision.to_string(),"expected_policy_revision":f.policy.revision()
    })).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt = common::body_json(response).await;
    let id = Uuid::parse_str(receipt["import_id"].as_str().unwrap()).unwrap();
    let release = workspace::ReleaseReadiness::for_tests();
    let detail = loop {
        let response = common::get_with_cookie(
            &f.app,
            &format!("/api/migrations/fub/history-imports/{id}"),
            &f.cookie,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let detail = common::body_json(response).await;
        if detail["state"] == "ready" {
            break detail;
        }
        assert_eq!(detail["state"], "preparing", "{detail}");
        assert!(
            history_import_worker::run_once(&f.pool, &f.key, &f.policy, Some(&release))
                .await
                .unwrap()
        );
    };
    let person: Uuid = sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE organization_id=$1 AND import_id=$2 AND source_id='101' AND person_id IS NOT NULL LIMIT 1")
        .bind(f.org).bind(parent).fetch_one(&f.pool).await.unwrap();
    (f, person, id, detail)
}

async fn confirm(f: &Fixture, id: Uuid, detail: &Value) {
    let response = common::post_json_with_cookie(&f.app, &format!("/api/migrations/fub/history-imports/{id}/confirm"), &f.cookie, json!({
        "request_id":Uuid::new_v4(),"plan_id":detail["plan_id"],
        "expected_revision":detail["revision"],"expected_plan_revision":detail["plan_revision"],
        "expected_workspace_revision":detail["workspace_revision"],"expected_policy_revision":detail["policy_revision"],
        "acknowledgements":{"external_facts":true,"date_uncertainty":true,"coverage_and_holds":true,"review_only":true}
    })).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[sqlx::test]
#[ignore]
async fn timeline_anchor_fences_both_complete_readers_before_any_fact(migrator: PgPool) {
    let (f, person, id, detail) = prepared(&migrator).await;
    let paths = [
        format!("/api/people/{person}"),
        format!("/api/people/{person}/migration-review"),
    ];
    let mut before = Vec::new();
    for path in &paths {
        let response = common::get_with_cookie(&f.app, path, &f.cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        before.push(common::body_json(response).await);
    }

    // Both handlers call history_complete_read before summary_by_id touches
    // inquiry. Block that exact later relation, with no production test hook.
    // HTTP middleware and each inner reader retain shared workspace permits.
    let mut barrier = migrator.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *barrier)
        .await
        .unwrap();
    sqlx::query("LOCK TABLE inquiry IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *barrier)
        .await
        .unwrap();
    let readers: Vec<_> = paths
        .iter()
        .map(|path| {
            let app = f.app.clone();
            let cookie = f.cookie.clone();
            let path = path.clone();
            tokio::spawn(async move { common::get_with_cookie(&app, &path, &cookie).await })
        })
        .collect();
    let reader_pids = tokio::time::timeout(std::time::Duration::from_millis(600), async {
        loop {
            let pids:Vec<i32>=sqlx::query_scalar("SELECT DISTINCT waiting.pid FROM pg_locks waiting JOIN pg_locks held ON held.pid=waiting.pid AND held.database=waiting.database WHERE waiting.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND waiting.locktype='relation' AND waiting.relation='inquiry'::regclass AND waiting.mode='AccessShareLock' AND NOT waiting.granted AND held.locktype='advisory' AND held.mode='ShareLock' AND held.granted AND $1=ANY(pg_blocking_pids(waiting.pid)) ORDER BY waiting.pid LIMIT 2")
                .bind(holder).fetch_all(&migrator).await.unwrap();
            if pids.len()==2 {return pids;}
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    }).await.unwrap_or_default();
    let app = f.app.clone();
    let cookie = f.cookie.clone();
    let command = json!({
        "request_id":Uuid::new_v4(),"plan_id":detail["plan_id"],
        "expected_revision":detail["revision"],"expected_plan_revision":detail["plan_revision"],
        "expected_workspace_revision":detail["workspace_revision"],"expected_policy_revision":detail["policy_revision"],
        "acknowledgements":{"external_facts":true,"date_uncertainty":true,"coverage_and_holds":true,"review_only":true}
    });
    let confirmation = tokio::spawn(async move {
        common::post_json_with_cookie(
            &app,
            &format!("/api/migrations/fub/history-imports/{id}/confirm"),
            &cookie,
            command,
        )
        .await
    });
    // Observe the actual exclusive advisory request blocked by both readers;
    // task.is_finished or a sleep would not establish database lock ordering.
    let confirmation_waits = if reader_pids.len() == 2 {
        tokio::time::timeout(std::time::Duration::from_millis(600), async {
            loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l WHERE l.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND l.locktype='advisory' AND l.mode='ExclusiveLock' AND NOT l.granted AND $1::int[] <@ pg_blocking_pids(l.pid))")
                    .bind(&reader_pids).fetch_one(&migrator).await.unwrap();
                if blocked {return true;}
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        }).await.unwrap_or(false)
    } else {
        false
    };
    let unanchored: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=$1)",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    // Always release the deliberate blocker before asserting observations.
    // Both observation deadlines together stay below production's 2s lock timeout.
    barrier.rollback().await.unwrap();
    let mut responses = Vec::new();
    for reader in readers {
        responses.push(reader.await.unwrap());
    }
    let confirmed = confirmation.await.unwrap();
    assert_eq!(
        reader_pids.len(),
        2,
        "both old readers passed their guard before waiting on inquiry"
    );
    assert!(
        confirmation_waits,
        "first Confirm must wait on both complete-reader permits"
    );
    assert!(
        unanchored,
        "no anchor may commit while a complete response is loading"
    );
    for (response, before) in responses.into_iter().zip(before) {
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            common::body_json(response).await,
            before,
            "raced complete read retains its exact pre-anchor response"
        );
    }
    assert_eq!(confirmed.status(), StatusCode::ACCEPTED);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fub_event_record_imported WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "confirmation establishes the fence before a worker writes"
    );
    sqlx::query("REVOKE SELECT ON inquiry FROM crm_app")
        .execute(&migrator)
        .await
        .unwrap();
    for path in [
        format!("/api/people/{person}"),
        format!("/api/people/{person}/migration-review"),
    ] {
        let response = common::get_with_cookie(&f.app, &path, &f.cookie).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            common::body_json(response).await,
            json!({"error":"history_review_required"})
        );
    }
    sqlx::query("GRANT SELECT ON inquiry TO crm_app")
        .execute(&migrator)
        .await
        .unwrap();
    let path = format!("/api/people/{person}/migration-review/v2");
    assert_eq!(
        common::get_with_cookie(&f.app, &path, &f.cookie)
            .await
            .status(),
        StatusCode::OK
    );
    for path in [
        path,
        format!("/api/people/{person}/migration-review/timeline"),
        format!("/api/migrations/fub/history-imports/{id}"),
    ] {
        for (cookie, expected) in [
            (&f.member_cookie, StatusCode::FORBIDDEN),
            (&String::new(), StatusCode::UNAUTHORIZED),
        ] {
            let response = common::get_with_cookie(&f.app, &path, cookie).await;
            assert_eq!(response.status(), expected);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert!(!common::body_json(response)
                .await
                .to_string()
                .contains("SYNTHETIC_UNREADABLE_BODY"));
        }
    }
}

#[sqlx::test]
#[ignore]
async fn timeline_compiled_read_capability_is_actual_connection_and_transaction_local(
    migrator: PgPool,
) {
    let (f, _, id, detail) = prepared(&migrator).await;
    confirm(&f, id, &detail).await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::app_url_for(&migrator))
        .await
        .unwrap();
    for _ in 0..2 {
        let mut legacy = pool.begin().await.unwrap();
        let error = sqlx::query("SELECT crm_workspace_read($1,$2,false)")
            .bind(f.org)
            .bind(f.actor)
            .execute(&mut *legacy)
            .await
            .unwrap_err();
        assert!(workspace::is_history_review_error(&error));
        legacy.rollback().await.unwrap();
        let mut current = pool.begin().await.unwrap();
        workspace::read_check(
            &mut current,
            f.ctx.organization_id,
            f.ctx.actor_user_id,
            false,
        )
        .await
        .unwrap();
        let capability: String =
            sqlx::query_scalar("SELECT current_setting('crm.history_reader',true)")
                .fetch_one(&mut *current)
                .await
                .unwrap();
        assert_eq!(capability, workspace::HISTORY_TIMELINE_CAPABILITY);
        current.commit().await.unwrap();
    }
    pool.close().await;
}

/// Opt-in real API for the single desktop/mobile walkthrough. SQLx owns the
/// disposable DB. The private control grants real worker units or stops the
/// fixture; all user mutations still pass through the application router.
#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn timeline_browser_fixture(migrator: PgPool) {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, time::Duration};
    let folder = std::path::PathBuf::from(
        std::env::var("CRM_010D2_BROWSER_DIR")
            .expect("explicit owned private browser fixture directory required"),
    );
    assert!(folder.is_absolute() && folder.is_dir());
    let (f, person, id, _) = prepared_book(&migrator, 110).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:13012")
        .await
        .unwrap();
    let app = f.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(folder.join("fixture.json"))
        .unwrap();
    output
        .write_all(
            &serde_json::to_vec(&json!({"api":"http://127.0.0.1:13012",
        "email":email,"password":"synthetic import fixture password","person_id":person,
        "import_id":id,"organization_id":f.org}))
            .unwrap(),
        )
        .unwrap();
    drop(output);
    let release = workspace::ReleaseReadiness::for_tests();
    let mut units = 0_u64;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "browser fixture exceeded its bounded lifetime"
        );
        let control: Value =
            serde_json::from_slice(&std::fs::read(folder.join("control.json")).unwrap()).unwrap();
        if control["stop"] == true {
            break;
        }
        if units < control["units"].as_u64().unwrap_or(0) {
            history_import_worker::run_once(
                &f.pool,
                &f.key,
                &f.policy,
                (control["release_ready"] != false).then_some(&release),
            )
            .await
            .unwrap();
            units += 1;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    server.abort();
    let _ = server.await;
}

#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn timeline_actual_capture_only_api_artifact_fails_closed(migrator: PgPool) {
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };
    let executable = std::env::var("CRM_010D2_OLD_API")
        .expect("supply the verified deployed 010d1 API artifact");
    let (f, person, id, detail) = prepared(&migrator).await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let cwd = std::env::temp_dir().join(format!("crm-010d2-old-api-{}", Uuid::new_v4()));
    std::fs::create_dir(&cwd).unwrap();
    struct Process(std::process::Child, std::path::PathBuf);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_dir(&self.1);
        }
    }
    let mut process = Process(
        Command::new(executable)
            .env_clear()
            .current_dir(&cwd)
            .env("DATABASE_URL", common::app_url_for(&migrator))
            .env("CRM_API_BIND_ADDR", address.to_string())
            .env("CRM_SESSION_SECRET", "a".repeat(32))
            .env("CRM_RAW_PAYLOAD_KEY", common::TEST_RAW_PAYLOAD_KEY_HEX)
            .env(
                "CENTRIFUGO_HTTP_API_KEY",
                common::TEST_CENTRIFUGO_HTTP_API_KEY,
            )
            .env(
                "CENTRIFUGO_TOKEN_HMAC_SECRET",
                common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET,
            )
            .env("CRM_CENTRIFUGO_API_URL", "http://127.0.0.1:1/api")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        cwd,
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let base = format!("http://{address}");
    let mut listening = false;
    for _ in 0..100 {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "old artifact exited before serving"
        );
        if client
            .get(format!("{base}/api/me"))
            .header("cookie", &f.cookie)
            .send()
            .await
            .is_ok()
        {
            listening = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(listening, "old API did not start within ten seconds");
    for suffix in ["", "/migration-review"] {
        let response = client
            .get(format!("{base}/api/people/{person}{suffix}"))
            .header("cookie", &f.cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
    confirm(&f, id, &detail).await;
    for cancel in [false, true] {
        if cancel {
            let path = format!("/api/migrations/fub/history-imports/{id}");
            let current =
                common::body_json(common::get_with_cookie(&f.app, &path, &f.cookie).await).await;
            let cancelled = common::post_json_with_cookie(
                &f.app,
                &format!("{path}/cancel"),
                &f.cookie,
                json!({"request_id":Uuid::new_v4(),"expected_revision":current["revision"]}),
            )
            .await;
            assert_eq!(cancelled.status(), StatusCode::OK);
        }
        for suffix in ["", "/migration-review"] {
            let response = client
                .get(format!("{base}/api/people/{person}{suffix}"))
                .header("cookie", &f.cookie)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(
                response.json::<Value>().await.unwrap(),
                json!({"error":"unavailable"})
            );
        }
    }
}

//! Required A11 verification: real database and current test-artifact evidence.
//! Reports and source replies are synthetic; no provider requests or expiry sleeps.
use std::{
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};

use chrono::{Duration, Utc};
use crm_api::{
    auth::workspace::{self, ReleaseReadiness},
    config::RawPayloadKey,
    domain::migration::{history_capture as h, history_capture_worker, MigrationError},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{db_history_capture_support as hs, import_support::Fixture};

struct PrivateReport(PathBuf);
impl PrivateReport {
    fn new(value: &Value) -> Self {
        let directory =
            std::env::temp_dir().join(format!("crm-010d1-release-test-{}", Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let report = Self(directory.join("report.json"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&report.0)
            .unwrap();
        file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
        report
    }
}
impl Drop for PrivateReport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(directory) = self.0.parent() {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

async fn evidence(pool: &PgPool) -> Value {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    let now = Utc::now();
    json!({
        "confirmation_ready": true,
        "metadata_confirmation_ready": true,
        "activity_confirmation_ready": true,
        "history_capture_confirmation_ready": true,
        "database_name": database,
        "checked_at": now - Duration::seconds(1),
        "evidence_expires_at": now + Duration::minutes(4),
        "candidates": [{
            "sha256": workspace::artifact_fingerprint().await.unwrap(),
            "gate_version": workspace::GATE_VERSION,
            "role": "api",
            "capabilities": ["fub-metadata-import-v1", "fub-activity-import-v1",
                             "fub-history-capture-v1"]
        }]
    })
}

async fn load(pool: &PgPool, value: &Value) -> Result<ReleaseReadiness, sqlx::Error> {
    let report = PrivateReport::new(value);
    ReleaseReadiness::load_report(pool, &report.0).await
}

fn invalid_reports(base: &Value) -> Vec<(&'static str, Value, bool)> {
    let mut cases = Vec::new();
    for name in [
        "missing history capability",
        "history flag false",
        "CLI role",
        "forged artifact hash",
        "wrong gate version",
        "stale checked time",
        "expired deadline",
        "wrong database",
        "capability belongs to a different artifact",
    ] {
        let mut value = base.clone();
        let loads = match name {
            "missing history capability" => {
                value["candidates"][0]["capabilities"] =
                    json!(["fub-metadata-import-v1", "fub-activity-import-v1"]);
                true
            }
            "history flag false" => {
                value["history_capture_confirmation_ready"] = json!(false);
                true
            }
            "CLI role" => {
                value["candidates"][0]["role"] = json!("cli");
                true
            }
            "forged artifact hash" => {
                value["candidates"][0]["sha256"] = json!("0".repeat(64));
                false
            }
            "wrong gate version" => {
                value["candidates"][0]["gate_version"] = json!("synthetic-unsupported");
                false
            }
            "stale checked time" => {
                value["checked_at"] = json!(Utc::now() - Duration::minutes(6));
                false
            }
            "expired deadline" => {
                value["evidence_expires_at"] = json!(Utc::now() - Duration::minutes(1));
                false
            }
            "wrong database" => {
                value["database_name"] = json!("synthetic_other_database");
                false
            }
            "capability belongs to a different artifact" => {
                let mut other = value["candidates"][0].clone();
                other["sha256"] = json!("0".repeat(64));
                value["candidates"][0]["capabilities"] = json!([]);
                value["candidates"].as_array_mut().unwrap().push(other);
                true
            }
            _ => unreachable!(),
        };
        cases.push((name, value, loads));
    }
    cases
}

async fn confirm(f: &Fixture, id: Uuid, release: Option<&ReleaseReadiness>) {
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        hs::confirmation(&hs::ready(f, id).await),
        release,
    )
    .await
    .unwrap();
}

async fn resume(f: &Fixture, id: Uuid, release: Option<&ReleaseReadiness>) {
    h::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        hs::action(&hs::ready(f, id).await),
        release,
    )
    .await
    .unwrap();
}

async fn receipt_count(f: &Fixture, id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_receipt WHERE run_id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore]
async fn history_release_real_report_gates_confirmation_resume_and_worker_recovery(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let base = evidence(&f.pool).await;
    let valid = load(&f.pool, &base).await.unwrap();
    assert!(valid.history_capture_ready());
    valid
        .require_history_capture(&mut f.pool.acquire().await.unwrap())
        .await
        .unwrap();
    let (id, proposed) = hs::propose(&f, parent).await;
    let mut rejected = vec![("missing report", None)];
    for (name, value, should_load) in invalid_reports(&base) {
        let result = load(&f.pool, &value).await;
        assert_eq!(
            result.is_ok(),
            should_load,
            "{name}: report-loading boundary"
        );
        let release = result.ok();
        assert!(
            !release
                .as_ref()
                .is_some_and(ReleaseReadiness::history_capture_ready),
            "{name}"
        );
        rejected.push((name, release));
    }
    for (name, release) in &rejected {
        let result = h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            hs::confirmation(&proposed),
            release.as_ref(),
        )
        .await;
        assert!(
            matches!(result, Err(MigrationError::Conflict)),
            "{name}: confirm"
        );
        assert_eq!(
            hs::ready(&f, id).await,
            proposed,
            "{name}: no durable confirmation"
        );
        assert_eq!(book.count(), 0);
    }
    confirm(&f, id, Some(&valid)).await;
    assert!(history_capture_worker::run_once(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        Some(&valid)
    )
    .await
    .unwrap());
    assert_eq!(
        book.count(),
        1,
        "positive control reached the identity reader"
    );

    for (name, release) in &rejected {
        // A real committed identity page exists. An independent worker session
        // establishes its DB-clock startup boundary without altering any
        // retained page, checkpoint, lease or credential.
        let session = history_capture_worker::WorkerSession::default();
        let before = hs::ready(&f, id).await;
        assert_eq!(before["state"], "queued");
        let calls = book.count();
        assert!(
            !history_capture_worker::run_once_with_session(
                &f.pool,
                &f.key,
                book.as_ref(),
                &f.policy,
                release.as_ref(),
                &session,
            )
            .await
            .unwrap(),
            "{name}: worker recovery"
        );
        let paused = hs::ready(&f, id).await;
        assert_eq!(paused["state"], "paused", "{name}");
        assert_eq!(paused["pause_reason"], "release_not_ready", "{name}");
        assert_eq!(paused["capture_sequence"], before["capture_sequence"]);
        assert_eq!(book.count(), calls, "{name}: no source request");
        let result = h::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            hs::action(&paused),
            release.as_ref(),
        )
        .await;
        assert!(
            matches!(result, Err(MigrationError::Conflict)),
            "{name}: resume"
        );
        assert_eq!(
            hs::ready(&f, id).await,
            paused,
            "{name}: no resume mutation"
        );
        resume(&f, id, Some(&valid)).await;
        assert!(history_capture_worker::run_once(
            &f.pool,
            &f.key,
            book.as_ref(),
            &f.policy,
            Some(&valid)
        )
        .await
        .unwrap());
        assert_eq!(
            book.count(),
            calls + 1,
            "{name}: valid recovery rechecks identity"
        );
    }
    // Required HC-R2-01 regression: current compatibility is checked again at
    // explicit Resume, even when the report itself is fresh and correct.
    let session = history_capture_worker::WorkerSession::default();
    assert!(!history_capture_worker::run_once_with_session(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        None,
        &session
    )
    .await
    .unwrap());
    let supported = hs::ready(&f, id).await;
    assert_eq!(supported["state"], "paused");
    for field in ["profile_version", "parser_version", "schema_version"] {
        sqlx::query(&format!("UPDATE migration_history_capture_run SET {field}=$3 WHERE id=$1 AND organization_id=$2"))
            .bind(id).bind(f.org).bind("synthetic-unsupported").execute(&migrator).await.unwrap();
        let before = hs::ready(&f, id).await;
        let receipts = receipt_count(&f, id).await;
        let calls = book.count();
        assert_eq!(before["actions"]["retry"], false, "{field}");
        let result = h::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            hs::action(&before),
            Some(&valid),
        )
        .await;
        assert!(
            matches!(result, Err(MigrationError::Conflict)),
            "{field}: unsupported Resume"
        );
        assert_eq!(
            hs::ready(&f, id).await,
            before,
            "{field}: no run/ledger change"
        );
        assert_eq!(receipt_count(&f, id).await, receipts);
        assert_eq!(book.count(), calls);
        sqlx::query(&format!("UPDATE migration_history_capture_run SET {field}=$3 WHERE id=$1 AND organization_id=$2"))
            .bind(id).bind(f.org).bind(supported[field].as_str().unwrap()).execute(&migrator).await.unwrap();
    }
    assert_eq!(hs::ready(&f, id).await, supported);
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        hs::action(&hs::ready(&f, id).await),
    )
    .await
    .unwrap();
}

#[sqlx::test]
#[ignore]
async fn history_release_cancelled_confirmation_remains_a_durable_startup_requirement(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let release = load(&f.pool, &evidence(&f.pool).await).await.unwrap();
    let (proposed, _) = hs::propose(&f, parent).await;
    let original: (String, String) = sqlx::query_as("SELECT profile_version,parser_version FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2")
        .bind(proposed).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let supported = hs::ready(&f, proposed).await;
    for field in ["profile_version", "parser_version", "schema_version"] {
        sqlx::query(&format!("UPDATE migration_history_capture_run SET {field}=$3 WHERE id=$1 AND organization_id=$2"))
            .bind(proposed).bind(f.org).bind("synthetic-unsupported").execute(&migrator).await.unwrap();
        workspace::startup_compatible(&mut f.pool.acquire().await.unwrap())
            .await
            .unwrap();
        let before = hs::ready(&f, proposed).await;
        let receipts = receipt_count(&f, proposed).await;
        assert_eq!(before["actions"]["confirm"], false, "{field}");
        let result = h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            proposed,
            hs::confirmation(&before),
            Some(&release),
        )
        .await;
        assert!(
            matches!(result, Err(MigrationError::Conflict)),
            "{field}: unsupported confirmation"
        );
        assert_eq!(
            hs::ready(&f, proposed).await,
            before,
            "{field}: no run/ledger change"
        );
        assert_eq!(receipt_count(&f, proposed).await, receipts);
        assert_eq!(book.count(), 0);
        let requirements: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_history_capture_run WHERE organization_id=$1 AND confirmed_at IS NOT NULL")
            .bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(
            requirements, 0,
            "{field}: proposed-only data has no durable requirement"
        );
        assert!(before["confirmed_at"].is_null());
        sqlx::query(&format!("UPDATE migration_history_capture_run SET {field}=$3 WHERE id=$1 AND organization_id=$2"))
            .bind(proposed).bind(f.org).bind(supported[field].as_str().unwrap()).execute(&migrator).await.unwrap();
    }
    let (bound, _) = hs::propose(&f, parent).await;
    confirm(&f, bound, Some(&release)).await;
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        bound,
        hs::action(&hs::ready(&f, bound).await),
    )
    .await
    .unwrap();
    let cancelled = hs::ready(&f, bound).await;
    assert_eq!(cancelled["state"], "cancelled");
    assert!(!cancelled["confirmed_at"].is_null());
    assert_eq!(cancelled["capture_sequence"], "0");
    assert_eq!(cancelled["reserved_bytes"], "0");
    assert_eq!(book.count(), 0);
    for (label, profile, parser) in [
        (
            "unsupported profile",
            "synthetic-unsupported",
            original.1.as_str(),
        ),
        (
            "unsupported parser",
            original.0.as_str(),
            "synthetic-unsupported",
        ),
    ] {
        // Deliberate migrator-only compatibility fixtures; no production command
        // can replace the profile of a confirmed capture.
        sqlx::query("UPDATE migration_history_capture_run SET profile_version=$3,parser_version=$4 WHERE id=$1 AND organization_id=$2")
            .bind(proposed).bind(f.org).bind(profile).bind(parser).execute(&migrator).await.unwrap();
        workspace::startup_compatible(&mut f.pool.acquire().await.unwrap())
            .await
            .unwrap();
        let requirements: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_history_capture_run WHERE organization_id=$1 AND confirmed_at IS NOT NULL")
            .bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(
            requirements, 1,
            "{label}: proposal does not create a requirement"
        );
        sqlx::query("UPDATE migration_history_capture_run SET profile_version=$3,parser_version=$4 WHERE id=$1 AND organization_id=$2")
            .bind(bound).bind(f.org).bind(profile).bind(parser).execute(&migrator).await.unwrap();
        let result = workspace::startup_compatible(&mut f.pool.acquire().await.unwrap()).await;
        assert!(
            matches!(result, Err(sqlx::Error::Protocol(message)) if message == "history capture artifact incompatible"),
            "{label}: cancelled binding still blocks startup"
        );
        sqlx::query("UPDATE migration_history_capture_run SET profile_version=$3,parser_version=$4 WHERE organization_id=$1 AND id=ANY($2)")
            .bind(f.org).bind(vec![proposed, bound]).bind(&original.0).bind(&original.1).execute(&migrator).await.unwrap();
        workspace::startup_compatible(&mut f.pool.acquire().await.unwrap())
            .await
            .unwrap();
    }
    assert_eq!(hs::ready(&f, bound).await, cancelled);
    assert!(hs::ready(&f, proposed).await["confirmed_at"].is_null());
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        proposed,
        hs::action(&hs::ready(&f, proposed).await),
    )
    .await
    .unwrap();
}

async fn connection_fingerprint(f: &Fixture) -> String {
    sqlx::query_scalar(
        "SELECT md5(to_jsonb(c)::text) FROM migration_connection c WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore]
async fn history_release_wrong_key_pauses_intact_capture_before_any_source_request(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let release = load(&f.pool, &evidence(&f.pool).await).await.unwrap();
    let (id, _) = hs::propose(&f, parent).await;
    confirm(&f, id, Some(&release)).await;
    let connection = connection_fingerprint(&f).await;
    let before = hs::ready(&f, id).await;
    let mut wrong_bytes = *f.key.as_bytes();
    wrong_bytes[0] ^= 1;
    let wrong_key = RawPayloadKey::new(wrong_bytes);
    assert!(!history_capture_worker::run_once(
        &f.pool,
        &wrong_key,
        book.as_ref(),
        &f.policy,
        Some(&release)
    )
    .await
    .unwrap());
    let paused = hs::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "retained_integrity_failed");
    assert_eq!(paused["capture_sequence"], before["capture_sequence"]);
    assert_eq!(paused["retained_bytes"], before["retained_bytes"]);
    assert_eq!(book.count(), 0);
    assert!(book.requests.lock().unwrap().is_empty());
    assert_eq!(connection_fingerprint(&f).await, connection);
    let captures: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(captures, 0);
    // The correct key resumes the same intact attempt through real commands.
    resume(&f, id, Some(&release)).await;
    for _ in 0..6 {
        if !history_capture_worker::run_once(
            &f.pool,
            &f.key,
            book.as_ref(),
            &f.policy,
            Some(&release),
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    assert_eq!(hs::ready(&f, id).await["state"], "completed_with_gaps");
    assert_eq!(book.count(), 4, "identity and three empty collection pages");
    assert_eq!(book.requests.lock().unwrap()[0], "identity");
    assert_eq!(connection_fingerprint(&f).await, connection);
}

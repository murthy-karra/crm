//! Synthetic-only Slice 010a command, worker, crypto and HTTP integration evidence.
use axum::http::StatusCode;
use crm_api::{
    config::RawPayloadKey,
    domain::{
        envelope::{CommandContext, Origin},
        migration::{
            commands::*,
            crypto,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            worker, MigrationError,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
    realtime::Publisher,
    state::AppState,
};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;
use uuid::Uuid;
const PW: &str = "correct horse battery staple";
#[derive(Default)]
struct Fake {
    calls: AtomicUsize,
    mode: AtomicUsize,
    block: AtomicUsize,
    entered: Notify,
    release: Notify,
}
impl Fake {
    async fn step(&self) -> Result<(), ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.block.swap(0, Ordering::SeqCst) == 1 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(ReaderError::Unavailable),
            2 => Err(ReaderError::MalformedResponse.with_capture(Capture {
                status: 200,
                body: b"synthetic malformed private".to_vec(),
                truncated: false,
                source_version: None,
            })),
            3 => Err(ReaderError::RateLimited(Some(17))),
            4 => Err(ReaderError::InvalidCredential),
            _ => Ok(()),
        }
    }
}
#[async_trait::async_trait]
impl FubReader for Fake {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        self.step().await?;
        let account = if self.mode.load(Ordering::SeqCst) == 5 {
            99
        } else {
            17
        };
        let raw=format!(r#"{{"account":{{"id":{account},"domain":"synthetic.example"}},"user":{{"id":3,"name":"Synthetic Agent"}}}}"#).into_bytes();
        Ok((
            Identity {
                account_id: account,
                account_domain: Some("synthetic.example".into()),
                user_id: Some(3),
                display_name: None,
            },
            raw,
        ))
    }
    async fn probe(&self, _: &str, probe: Probe) -> Result<ProbeResult, ReaderError> {
        self.step().await?;
        if probe == Probe::Stages {
            return Err(ReaderError::AccessDenied);
        }
        let (total, count) = match probe {
            Probe::PeopleExcludingTrash => (Some("4".into()), 1),
            Probe::PeopleIncludingTrash => (None, 1),
            Probe::Users => (Some("0".into()), 0),
            _ => (Some("1".into()), 1),
        };
        Ok(ProbeResult {
            status: 200,
            body: format!("synthetic {:?} response", probe).into_bytes(),
            reported_total: total,
            retrieved_count: count,
            continuation: false,
            source_version: None,
        })
    }
}
struct Fixture {
    pool: PgPool,
    ctx: CommandContext,
    key: RawPayloadKey,
    reader: Arc<Fake>,
}
async fn fixture(migrator: &PgPool) -> Fixture {
    let (org, user) = crate::common::create_org_with_stages_and_member(
        migrator,
        "Synthetic Migration",
        "migration@synthetic.test",
        "Migration",
        PW,
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(org)
    .bind(user)
    .execute(migrator)
    .await
    .unwrap();
    Fixture {
        pool: crate::common::connect_as_app(migrator).await,
        ctx: CommandContext {
            organization_id: OrganizationId::new(org),
            actor_user_id: UserId::new(user),
            origin: Origin::WebSession,
            correlation_id: CorrelationId::new(Uuid::new_v4()),
        },
        key: RawPayloadKey::new([0x11; 32]),
        reader: Arc::new(Fake::default()),
    }
}
async fn connect(f: &Fixture) -> crm_api::domain::migration::ConnectionView {
    connect_fub(
        &f.pool,
        &f.key,
        f.reader.as_ref(),
        &f.ctx,
        ConnectFub {
            request_id: Uuid::new_v4(),
            api_key: "synthetic-api-secret".into(),
        },
    )
    .await
    .unwrap()
    .value
}
async fn start(
    f: &Fixture,
    c: &crm_api::domain::migration::ConnectionView,
) -> crm_api::domain::migration::AssessmentView {
    start_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        StartFubAssessment {
            request_id: Uuid::new_v4(),
            connection_id: c.id,
            expected_revision: c.revision,
        },
    )
    .await
    .unwrap()
    .value
}
async fn report(f: &Fixture, id: Uuid) -> crm_api::domain::migration::AssessmentView {
    get_fub_assessment(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap()
}
async fn due(f: &Fixture) {
    sqlx::query("UPDATE migration_assessment SET next_attempt_at=now()-interval '1 second',lease_expires_at=CASE WHEN lease_token IS NOT NULL THEN now()-interval '1 second' ELSE NULL END WHERE organization_id=$1").bind(f.ctx.organization_id.0).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE migration_assessment_check SET next_attempt_at=now()-interval '1 second' WHERE organization_id=$1").bind(f.ctx.organization_id.0).execute(&f.pool).await.unwrap();
}
async fn evidence_count(f: &Fixture) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM migration_assessment_evidence WHERE organization_id=$1",
    )
    .bind(f.ctx.organization_id.0)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore]
async fn receipts_concurrent_replay_conflict_and_immutable_after_progress(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let id = Uuid::new_v4();
    let submit = || {
        connect_fub(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ConnectFub {
                request_id: id,
                api_key: "synthetic-api-secret".into(),
            },
        )
    };
    let (a, b) = tokio::join!(submit(), submit());
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(
        serde_json::to_value(&a.value).unwrap(),
        serde_json::to_value(&b.value).unwrap()
    );
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
    assert!(a.replayed != b.replayed);
    assert!(matches!(
        connect_fub(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ConnectFub {
                request_id: id,
                api_key: "different".into()
            }
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let request = Uuid::new_v4();
    let start_req = || {
        start_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            StartFubAssessment {
                request_id: request,
                connection_id: a.value.id,
                expected_revision: 1,
            },
        )
    };
    let (x, y) = tokio::join!(start_req(), start_req());
    let assessment = x.unwrap().value;
    assert_eq!(assessment.id, y.unwrap().value.id);
    assert!(matches!(
        start_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            StartFubAssessment {
                request_id: Uuid::new_v4(),
                connection_id: a.value.id,
                expected_revision: 1
            }
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    cancel_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        CancelFubAssessment {
            assessment_id: assessment.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(start_req().await.unwrap().value.state, "queued");
    let request = Uuid::new_v4();
    let replace = || {
        replace_fub_credential(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ReplaceFubCredential {
                request_id: request,
                connection_id: a.value.id,
                expected_revision: 1,
                api_key: "new-synthetic-key".into(),
            },
        )
    };
    let (x, y) = tokio::join!(replace(), replace());
    assert_eq!(x.unwrap().value.revision, 2);
    assert_eq!(y.unwrap().value.revision, 2);
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 2);
    disconnect_fub(
        &f.pool,
        &f.ctx,
        DisconnectFub {
            connection_id: a.value.id,
        },
    )
    .await
    .unwrap();
    disconnect_fub(
        &f.pool,
        &f.ctx,
        DisconnectFub {
            connection_id: a.value.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(replace().await.unwrap().value.revision, 2);
    assert_eq!(submit().await.unwrap().value.revision, 1);
    assert_eq!(
        fub_summary(&f.pool, &f.key, &f.ctx)
            .await
            .unwrap()
            .connection
            .unwrap()
            .revision,
        3
    );
    let responses: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT response FROM migration_request_receipt")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert!(
        responses
            .iter()
            .all(|r| !r.to_string().contains("synthetic")),
        "receipts encrypt identity-bearing projections too"
    );
}
#[sqlx::test]
#[ignore]
async fn domain_authority_and_cross_tenant_locators_are_enforced(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    let (org, user) = crate::common::create_org_with_stages_and_member(
        &migrator,
        "Other",
        "other-mig@test.example",
        "Other",
        PW,
    )
    .await;
    sqlx::query("UPDATE organization_membership SET role='admin' WHERE organization_id=$1")
        .bind(org)
        .execute(&migrator)
        .await
        .unwrap();
    let other = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(user),
        ..f.ctx.clone()
    };
    assert!(matches!(
        get_fub_assessment(&f.pool, &f.key, &other, a.id).await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        disconnect_fub(
            &f.pool,
            &other,
            DisconnectFub {
                connection_id: c.id
            }
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        start_fub_assessment(
            &f.pool,
            &f.key,
            &other,
            StartFubAssessment {
                request_id: Uuid::new_v4(),
                connection_id: c.id,
                expected_revision: 1
            }
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    sqlx::query("UPDATE organization_membership SET role='member' WHERE organization_id=$1")
        .bind(f.ctx.organization_id.0)
        .execute(&migrator)
        .await
        .unwrap();
    assert!(matches!(
        fub_summary(&f.pool, &f.key, &f.ctx).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        connect_fub(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ConnectFub {
                request_id: Uuid::new_v4(),
                api_key: "key".into()
            }
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        replace_fub_credential(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ReplaceFubCredential {
                request_id: Uuid::new_v4(),
                connection_id: c.id,
                expected_revision: 1,
                api_key: "key".into()
            }
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        disconnect_fub(
            &f.pool,
            &f.ctx,
            DisconnectFub {
                connection_id: c.id
            }
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        retry_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            RetryFubAssessment {
                assessment_id: a.id
            }
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        cancel_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            CancelFubAssessment {
                assessment_id: a.id
            }
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
}
#[sqlx::test]
#[ignore]
async fn six_checks_report_honest_counts_and_preserve_last_report_without_business_writes(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    let before: Vec<(String, i64)> = business_counts(&f.pool).await;
    for _ in 0..6 {
        assert!(worker::run_once(&f.pool, &f.key, f.reader.as_ref())
            .await
            .unwrap());
    }
    assert!(!worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap());
    let a = report(&f, a.id).await;
    assert_eq!(a.state, "completed");
    assert_eq!(a.checks.len(), 19);
    assert_eq!(a.source_access_scope, "unknown");
    assert_eq!(a.source_display_name.as_deref(), Some("synthetic.example"));
    for c in &a.checks {
        assert!(!c.reason_codes.is_empty());
        assert!(!c.next_action.is_empty());
        match c.check_key.as_str() {
            "people_excluding_trash" => {
                assert_eq!(c.reported_total.as_deref(), Some("4"));
                assert_eq!(c.retrieved_count, "1");
                assert_eq!(c.coverage, "partial")
            }
            "people_including_trash" => {
                assert_eq!(c.reported_total, None);
                assert_eq!(c.retrieved_count, "1");
                assert_eq!(c.coverage, "partial")
            }
            "users" => {
                assert_eq!(c.reported_total.as_deref(), Some("0"));
                assert_eq!(c.coverage, "complete_for_query")
            }
            "stages" => {
                assert_eq!(c.reported_total, None);
                assert_eq!(c.coverage, "unavailable")
            }
            "identity" | "custom_fields" => {}
            _ => assert_eq!(c.coverage, "not_checked"),
        }
    }
    assert_eq!(before, business_counts(&f.pool).await);
    let new = start(&f, &c).await;
    let summary = fub_summary(&f.pool, &f.key, &f.ctx).await.unwrap();
    assert_eq!(summary.active_assessment.unwrap().id, new.id);
    assert_eq!(summary.latest_report.unwrap().id, a.id);
    assert_eq!(evidence_count(&f).await, 5);
}
async fn business_counts(pool: &PgPool) -> Vec<(String, i64)> {
    let mut result = vec![];
    for table in [
        "person",
        "inquiry",
        "raw_payload",
        "stage",
        "app_user",
        "organization_membership",
    ] {
        let count = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        result.push((table.into(), count))
    }
    result
}
#[sqlx::test]
#[ignore]
async fn in_flight_revocation_and_cancel_fence_all_evidence(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    f.reader.block.store(1, Ordering::SeqCst);
    let worker = worker::run_once(&f.pool, &f.key, f.reader.as_ref());
    let action = async {
        f.reader.entered.notified().await;
        sqlx::query(
            "UPDATE organization_membership SET status='inactive' WHERE organization_id=$1",
        )
        .bind(f.ctx.organization_id.0)
        .execute(&migrator)
        .await
        .unwrap();
        f.reader.release.notify_one();
    };
    let (result, _) = tokio::join!(worker, action);
    result.unwrap();
    assert_eq!(evidence_count(&f).await, 0);
    sqlx::query("UPDATE organization_membership SET status='active' WHERE organization_id=$1")
        .bind(f.ctx.organization_id.0)
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        report(&f, a.id).await.pause_reason.as_deref(),
        Some("initiator_not_authorized")
    );
    retry_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        RetryFubAssessment {
            assessment_id: a.id,
        },
    )
    .await
    .unwrap();
    worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap();
    assert_eq!(evidence_count(&f).await, 1);
    f.reader.block.store(1, Ordering::SeqCst);
    let worker = worker::run_once(&f.pool, &f.key, f.reader.as_ref());
    let action = async {
        f.reader.entered.notified().await;
        cancel_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            CancelFubAssessment {
                assessment_id: a.id,
            },
        )
        .await
        .unwrap();
        f.reader.release.notify_one();
    };
    let (result, _) = tokio::join!(worker, action);
    result.unwrap();
    assert_eq!(report(&f, a.id).await.state, "cancelled");
    assert_eq!(evidence_count(&f).await, 1);
    assert!(!worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap());
}
#[sqlx::test]
#[ignore]
async fn retry_budget_due_time_explicit_cycle_and_completed_checks_skip(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap();
    f.reader.mode.store(3, Ordering::SeqCst);
    for i in 1..=3 {
        worker::run_once(&f.pool, &f.key, f.reader.as_ref())
            .await
            .unwrap();
        let r = report(&f, a.id).await;
        assert_eq!(r.state, if i == 3 { "paused" } else { "waiting_retry" });
        if i < 3 {
            let next: chrono::DateTime<chrono::Utc> =
                sqlx::query_scalar("SELECT next_attempt_at FROM migration_assessment WHERE id=$1")
                    .bind(a.id)
                    .fetch_one(&f.pool)
                    .await
                    .unwrap();
            assert!(next > chrono::Utc::now() + chrono::Duration::seconds(14));
            assert!(!worker::run_once(&f.pool, &f.key, f.reader.as_ref())
                .await
                .unwrap());
        }
        due(&f).await;
    }
    assert_eq!(
        report(&f, a.id).await.pause_reason.as_deref(),
        Some("retry_exhausted")
    );
    let before = f.reader.calls.load(Ordering::SeqCst);
    assert!(!worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap());
    assert_eq!(before, f.reader.calls.load(Ordering::SeqCst));
    retry_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        RetryFubAssessment {
            assessment_id: a.id,
        },
    )
    .await
    .unwrap();
    retry_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        RetryFubAssessment {
            assessment_id: a.id,
        },
    )
    .await
    .unwrap();
    f.reader.mode.store(0, Ordering::SeqCst);
    for _ in 0..5 {
        worker::run_once(&f.pool, &f.key, f.reader.as_ref())
            .await
            .unwrap();
    }
    let r = report(&f, a.id).await;
    assert_eq!(r.state, "completed");
    assert_eq!(
        r.checks
            .iter()
            .find(|c| c.check_key == "identity")
            .unwrap()
            .attempts,
        1
    );
    assert_eq!(
        r.checks
            .iter()
            .find(|c| c.check_key == "custom_fields")
            .unwrap()
            .attempts,
        4
    );
}
#[sqlx::test]
#[ignore]
async fn malformed_bytes_encrypted_and_paused_disconnect_reconnect_retains_identity(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    f.reader.mode.store(2, Ordering::SeqCst);
    worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap();
    let r = report(&f, a.id).await;
    assert_eq!(r.state, "paused");
    assert_eq!(r.pause_reason.as_deref(), Some("malformed_response"));
    let e = sqlx::query("SELECT * FROM migration_assessment_evidence WHERE assessment_id=$1")
        .bind(a.id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(e.get::<String, _>("classification"), "malformed_response");
    assert_eq!(
        crypto::open_evidence(
            &f.key,
            f.ctx.organization_id,
            e.get("id"),
            e.get("nonce"),
            e.get("ciphertext")
        )
        .unwrap(),
        b"synthetic malformed private"
    );
    assert!(!String::from_utf8_lossy(e.get::<&[u8], _>("ciphertext")).contains("synthetic"));
    disconnect_fub(
        &f.pool,
        &f.ctx,
        DisconnectFub {
            connection_id: c.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(report(&f, a.id).await.state, "cancelled");
    assert!(matches!(
        retry_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            RetryFubAssessment {
                assessment_id: a.id
            }
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let connection = fub_summary(&f.pool, &f.key, &f.ctx)
        .await
        .unwrap()
        .connection
        .unwrap();
    assert_eq!(connection.source_display_name, c.source_display_name);
    assert_eq!(connection.revision, 2);
    f.reader.mode.store(5, Ordering::SeqCst);
    assert!(matches!(
        replace_fub_credential(
            &f.pool,
            &f.key,
            f.reader.as_ref(),
            &f.ctx,
            ReplaceFubCredential {
                request_id: Uuid::new_v4(),
                connection_id: c.id,
                expected_revision: 2,
                api_key: "new-key".into()
            }
        )
        .await,
        Err(MigrationError::SourceAccountMismatch)
    ));
    f.reader.mode.store(0, Ordering::SeqCst);
    let new = replace_fub_credential(
        &f.pool,
        &f.key,
        f.reader.as_ref(),
        &f.ctx,
        ReplaceFubCredential {
            request_id: Uuid::new_v4(),
            connection_id: c.id,
            expected_revision: 2,
            api_key: "new-key".into(),
        },
    )
    .await
    .unwrap()
    .value;
    assert_eq!(new.revision, 3);
    assert_eq!(
        report(&f, a.id).await.source_display_name,
        c.source_display_name
    );
}
#[sqlx::test]
#[ignore]
async fn expired_lease_reclaims_and_db_failure_does_not_commit_partial_result(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    f.reader.block.store(1, Ordering::SeqCst);
    let work = worker::run_once(&f.pool, &f.key, f.reader.as_ref());
    let action = async {
        f.reader.entered.notified().await;
        due(&f).await;
        f.reader.release.notify_one();
    };
    let (result, _) = tokio::join!(work, action);
    result.unwrap();
    assert_eq!(evidence_count(&f).await, 0);
    worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap();
    assert_eq!(evidence_count(&f).await, 1);
    assert_eq!(
        report(&f, a.id)
            .await
            .checks
            .iter()
            .find(|v| v.check_key == "identity")
            .unwrap()
            .attempts,
        2
    );
    sqlx::query("ALTER TABLE migration_assessment_evidence ADD CONSTRAINT synthetic_commit_failure CHECK (classification <> 'success') NOT VALID").execute(&migrator).await.unwrap();
    assert!(worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .is_err());
    assert_eq!(evidence_count(&f).await, 1);
    assert_eq!(
        report(&f, a.id)
            .await
            .checks
            .iter()
            .find(|v| v.check_key == "custom_fields")
            .unwrap()
            .state,
        "running"
    );
    sqlx::query(
        "ALTER TABLE migration_assessment_evidence DROP CONSTRAINT synthetic_commit_failure",
    )
    .execute(&migrator)
    .await
    .unwrap();
    due(&f).await;
    worker::run_once(&f.pool, &f.key, f.reader.as_ref())
        .await
        .unwrap();
    assert_eq!(evidence_count(&f).await, 2);
}
#[sqlx::test]
#[ignore]
async fn http_admin_body_precedence_contract_and_no_store(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let config = crate::common::test_config();
    let state = AppState::for_tests(f.pool.clone(), &config, Publisher::recording())
        .with_migration_reader(f.reader.clone());
    let app = crm_api::build_app(state);
    let unauth = crate::common::get_with_cookie(&app, "/api/migrations/fub/", "").await;
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let cookie = crate::common::login_cookie(&app, "migration@synthetic.test", PW).await;
    let unknown = crate::common::post_json_with_cookie(
        &app,
        "/api/migrations/fub/connections",
        &cookie,
        json!({"request_id":Uuid::new_v4(),"api_key":"key","role":"admin"}),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    let request = Uuid::new_v4();
    let body = json!({"request_id":request,"api_key":"synthetic-http-secret"});
    let response = crate::common::post_json_with_cookie(
        &app,
        "/api/migrations/fub/connections",
        &cookie,
        body.clone(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value = crate::common::body_json(response).await;
    assert!(!value.to_string().contains("secret"));
    let again = crate::common::post_json_with_cookie(
        &app,
        "/api/migrations/fub/connections",
        &cookie,
        body,
    )
    .await;
    assert_eq!(again.status(), StatusCode::CREATED);
    assert_eq!(crate::common::body_json(again).await, value);
    let summary = crate::common::body_json(
        crate::common::get_with_cookie(&app, "/api/migrations/fub/", &cookie).await,
    )
    .await;
    assert!(summary.get("active_assessment").is_some());
    assert!(summary.get("latest_report").is_some());
    let malformed =
        crate::common::get_with_cookie(&app, "/api/migrations/fub/assessments/not-a-uuid", "")
            .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    sqlx::query("UPDATE organization_membership SET role='member' WHERE organization_id=$1")
        .bind(f.ctx.organization_id.0)
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        crate::common::get_with_cookie(&app, "/api/migrations/fub/", &cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
#[ignore]
async fn concurrent_distinct_starts_and_in_flight_disconnect_are_atomic(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let submit = || {
        start_fub_assessment(
            &f.pool,
            &f.key,
            &f.ctx,
            StartFubAssessment {
                request_id: Uuid::new_v4(),
                connection_id: c.id,
                expected_revision: 1,
            },
        )
    };
    let (x, y) = tokio::join!(submit(), submit());
    let a = match (x, y) {
        (Ok(a), Err(MigrationError::Conflict)) | (Err(MigrationError::Conflict), Ok(a)) => a.value,
        _ => panic!("exactly one distinct start may succeed"),
    };
    f.reader.block.store(1, Ordering::SeqCst);
    let work = worker::run_once(&f.pool, &f.key, f.reader.as_ref());
    let action = async {
        f.reader.entered.notified().await;
        disconnect_fub(
            &f.pool,
            &f.ctx,
            DisconnectFub {
                connection_id: c.id,
            },
        )
        .await
        .unwrap();
        f.reader.release.notify_one();
    };
    let (result, _) = tokio::join!(work, action);
    result.unwrap();
    assert_eq!(report(&f, a.id).await.state, "cancelled");
    assert_eq!(evidence_count(&f).await, 0);
    let row = sqlx::query(
        "SELECT credential_nonce,credential_ciphertext FROM migration_connection WHERE id=$1",
    )
    .bind(c.id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(row.get::<Option<Vec<u8>>, _>("credential_nonce").is_none());
    assert!(row
        .get::<Option<Vec<u8>>, _>("credential_ciphertext")
        .is_none());
}
#[sqlx::test]
#[ignore]
async fn tracing_contains_safe_context_and_outcomes_without_credentials_or_raw_content(
    migrator: PgPool,
) {
    use tracing::instrument::WithSubscriber;
    #[derive(Clone, Default)]
    struct Writer(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Writer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }
    let f = fixture(&migrator).await;
    let writer = Writer::default();
    let buffer = writer.0.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .finish();
    let id = async {
        let c = connect(&f).await;
        let a = start(&f, &c).await;
        worker::run_once(&f.pool, &f.key, f.reader.as_ref())
            .await
            .unwrap();
        f.reader.mode.store(2, Ordering::SeqCst);
        worker::run_once(&f.pool, &f.key, f.reader.as_ref())
            .await
            .unwrap();
        a.id
    }
    .with_subscriber(subscriber)
    .await;
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("migration_check"));
    assert!(captured.contains("malformed_response"));
    assert!(captured.contains("success"));
    assert!(captured.contains(&id.to_string()));
    assert!(captured.contains(&f.ctx.organization_id.0.to_string()));
    for sentinel in [
        "synthetic-api-secret",
        "synthetic malformed private",
        "synthetic.example",
        "Synthetic Agent",
    ] {
        assert!(
            !captured.contains(sentinel),
            "private sentinel must be absent"
        )
    }
}
#[sqlx::test]
#[ignore]
async fn migration_claim_and_report_plans_use_bounded_indexes(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let c = connect(&f).await;
    let a = start(&f, &c).await;
    // Synthetic history only, to make the selective lookup/claim plan meaningful.
    sqlx::query("INSERT INTO migration_assessment(id,organization_id,connection_id,connection_revision,profile_version,initiated_by_user_id,state,source_account_id,identity_nonce,identity_ciphertext,created_at,completed_at) SELECT gen_random_uuid(),organization_id,connection_id,connection_revision,profile_version,initiated_by_user_id,'completed',source_account_id,identity_nonce,identity_ciphertext,now()-i*interval '1 hour',now()-i*interval '1 hour' FROM migration_assessment CROSS JOIN generate_series(1,2000) i WHERE id=$1").bind(a.id).execute(&migrator).await.unwrap();
    sqlx::query("ANALYZE migration_assessment")
        .execute(&migrator)
        .await
        .unwrap();
    for (label,query) in [
        ("latest_report","EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM migration_assessment WHERE organization_id=$1 AND state='completed' ORDER BY created_at DESC,id DESC LIMIT 1"),
        ("active_report","EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry','paused') ORDER BY created_at DESC,id DESC LIMIT 1"),
        ("latest_assessment","EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM migration_assessment WHERE organization_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1")
    ] {let rows:Vec<String>=sqlx::query_scalar(query).bind(f.ctx.organization_id.0).fetch_all(&f.pool).await.unwrap();let plan=rows.join("\n");assert!(plan.contains("Index"),"{label}: {plan}");println!("{label}\n{plan}");}
    let rows:Vec<String>=sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS) SELECT a.id,a.organization_id,a.connection_id,a.connection_revision,a.initiated_by_user_id FROM migration_assessment a WHERE ((a.state IN ('queued','waiting_retry') AND (a.next_attempt_at IS NULL OR a.next_attempt_at<=now())) OR (a.state='running' AND a.lease_expires_at<=now())) ORDER BY a.created_at FOR UPDATE SKIP LOCKED LIMIT 1").fetch_all(&f.pool).await.unwrap();
    let plan = rows.join("\n");
    assert!(
        plan.contains("migration_assessment_due_claim_idx"),
        "{plan}"
    );
    println!("claim\n{plan}");
}

//! D-065 background-work boundary proofs through actual API/domain paths.
//!
//! `force_review` deliberately constructs otherwise-illegal held states with
//! existing work. These are negative cleanup/defense fixtures, not evidence
//! that legal confirmation accepts populated Organizations or active work.
//! Legal entry/admission/lease races are covered in db_import_gate.rs.
//! All providers, credentials, messages and calls here are synthetic.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use crm_api::{
    auth::workspace,
    config::Config,
    domain::{
        admin::{MembershipStatus, Role},
        commands::{self, CallError, CallOutcomeCorrection, CorrectCallOutcome},
        envelope::{CommandContext, Origin},
        intake::extraction::{
            worker::{self as extraction_worker, ExtractionReport},
            ExtractionInput, ExtractorError, ExtractorReply, LeadExtractor,
        },
    },
    ids::{CallId, CorrelationId, OrganizationId, UserId},
    operator::OperatorRuntime,
    realtime::Publisher,
    state::AppState,
    telephony::{DialOutcome, RecordedCall, Telephony},
};
use crm_operator::{ChatRequest, ChatResponse, InferenceProvider, Limits, ProviderError};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tower::ServiceExt;
use uuid::Uuid;

const INBOUND_SECRET: &str = "synthetic-workspace-inbound-secret-32bytes";
const LEAD_EML: &[u8] = include_bytes!("fixtures/email/unrecognized_lead.eml");

async fn force_review(pool: &PgPool, org: Uuid) {
    // Migrator-only negative fixture. Product code has no activation/reset or
    // bypass equivalent, and legal confirmation must reject these workspaces.
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(org).execute(pool).await.expect("negative held workspace fixture");
}

async fn admin_fixture(pool: &PgPool, suffix: &str) -> (Uuid, Uuid, String) {
    let org = crate::common::create_org(pool, &format!("Synthetic workspace {suffix}")).await;
    crate::common::seed_stages(pool, org).await;
    let email = format!("admin-{suffix}@workspace.invalid");
    let actor = crate::common::create_user(pool, &email, "Synthetic Admin", "pw").await;
    crate::common::add_membership_with(pool, org, actor, Role::Admin, MembershipStatus::Active)
        .await;
    (org, actor, email)
}

fn message() -> Value {
    json!({"message":"Read my synthetic workspace","history":[],"context":{"route":"other"}})
}

async fn assert_error(response: Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(crate::common::body_json(response).await["error"], code);
}

struct ControlledInference {
    calls: AtomicUsize,
    active: AtomicUsize,
    entered: Semaphore,
    dropped: Semaphore,
}
impl ControlledInference {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            entered: Semaphore::new(0),
            dropped: Semaphore::new(0),
        })
    }
}
struct InferenceFlight<'a>(&'a ControlledInference);
impl Drop for InferenceFlight<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.dropped.add_permits(1);
    }
}
#[async_trait]
impl InferenceProvider for ControlledInference {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        drop(request);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        let _flight = InferenceFlight(self);
        self.entered.add_permits(1);
        // Only cancellation can finish this synthetic provider invocation.
        // No remote request or retained prompt exists in this fixture.
        std::future::pending::<()>().await;
        unreachable!("pending synthetic inference cannot return")
    }
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn model(&self) -> &str {
        "workspace-deadline-fixture"
    }
}

async fn operator_router(
    pool: &PgPool,
    provider: Arc<ControlledInference>,
    turn_timeout: Duration,
) -> (Router, Arc<OperatorRuntime>) {
    let state = AppState::for_tests(
        crate::common::connect_as_app(pool).await,
        &crate::common::test_config(),
        Publisher::recording(),
    )
    .with_operator(OperatorRuntime::with_provider(
        provider,
        Limits {
            turn_timeout,
            ..Limits::default()
        },
        1,
    ));
    let runtime = state.operator.as_ref().expect("synthetic operator").clone();
    (crm_api::build_app(state), runtime)
}

async fn admission_count(pool: &PgPool, org: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM workspace_operation_admission WHERE organization_id=$1",
    )
    .bind(org)
    .fetch_one(pool)
    .await
    .expect("admission count")
}

async fn terminal_operator(pool: &PgPool, org: Uuid) -> (String, i32, i32) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let row = sqlx::query_as::<_, (String, i32, i32)>(
                "SELECT outcome,model_call_count,tool_call_count FROM operator_turn WHERE organization_id=$1 ORDER BY recorded_at DESC LIMIT 1",
            ).bind(org).fetch_optional(pool).await.expect("terminal operator ledger");
            if let Some(row) = row {
                if admission_count(pool, org).await == 0 {
                    return row;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("operator must terminate and release its admission")
}

#[sqlx::test]
#[ignore]
async fn workspace_operator_disconnect_keeps_admission_until_deadline_and_blocks_held_inference(
    migrator_pool: PgPool,
) {
    let (org, actor, email) = admin_fixture(&migrator_pool, "operator-lifetime").await;
    let provider = ControlledInference::new();
    let (router, runtime) =
        operator_router(&migrator_pool, provider.clone(), Duration::from_secs(2)).await;
    let cookie = crate::common::login_cookie(&router, &email, "pw").await;
    let in_flight_router = router.clone();
    let in_flight_cookie = cookie.clone();
    let request = tokio::spawn(async move {
        crate::common::post_json_with_cookie(
            &in_flight_router,
            "/api/operator/turns",
            &in_flight_cookie,
            message(),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), provider.entered.acquire())
        .await
        .expect("provider entered")
        .unwrap()
        .forget();
    let (kind, deadline): (String, DateTime<Utc>) = sqlx::query_as(
        "SELECT kind,deadline FROM workspace_operation_admission WHERE organization_id=$1 AND actor_user_id=$2",
    ).bind(org).bind(actor).fetch_one(&migrator_pool).await.expect("admission exists before inference");
    assert_eq!(kind, "operator");
    assert!(deadline > Utc::now());
    assert_eq!(provider.active.load(Ordering::SeqCst), 1);
    assert!(runtime.try_acquire(UserId::new(actor)).is_err());

    // No workspace transaction may span provider inference. The short
    // exclusive probe acquires while the durable admission remains present.
    let mut probe = migrator_pool.begin().await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        workspace::exclusive(&mut probe, OrganizationId::new(org)),
    )
    .await
    .expect("inference must not hold a workspace transaction")
    .unwrap();
    probe.rollback().await.unwrap();
    assert_eq!(admission_count(&migrator_pool, org).await, 1);
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(
        admission_count(&migrator_pool, org).await,
        1,
        "client disconnect does not abandon work admission"
    );

    // Deliberately illegal entry fixture: terminal audit and release still
    // need to work if a stale/background task observes a held workspace.
    force_review(&migrator_pool, org).await;
    tokio::time::timeout(Duration::from_secs(4), provider.dropped.acquire())
        .await
        .expect("absolute deadline drops the provider future")
        .unwrap()
        .forget();
    assert_eq!(provider.active.load(Ordering::SeqCst), 0);
    assert_eq!(
        terminal_operator(&migrator_pool, org).await,
        ("turn_timeout".into(), 1, 0)
    );
    let released = runtime
        .try_acquire(UserId::new(actor))
        .expect("concurrency slot released with turn");
    drop(released);
    assert_error(
        crate::common::post_json_with_cookie(&router, "/api/operator/turns", &cookie, message())
            .await,
        StatusCode::CONFLICT,
        "workspace_in_migration_review",
    )
    .await;
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "held admin starts no inference"
    );
    assert_eq!(admission_count(&migrator_pool, org).await, 0);
    let turns: i64 =
        sqlx::query_scalar("SELECT count(*) FROM operator_turn WHERE organization_id=$1")
            .bind(org)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(turns, 1, "held rejection creates no extra terminal turn");
}

#[sqlx::test]
#[ignore]
async fn workspace_operator_admission_wait_consumes_absolute_deadline_before_provider_start(
    migrator_pool: PgPool,
) {
    let (org, _, email) = admin_fixture(&migrator_pool, "operator-scheduling").await;
    let provider = ControlledInference::new();
    let (router, _) =
        operator_router(&migrator_pool, provider.clone(), Duration::from_millis(200)).await;
    let cookie = crate::common::login_cookie(&router, &email, "pw").await;
    let mut insert_gate = migrator_pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE workspace_operation_admission IN SHARE MODE")
        .execute(&mut *insert_gate)
        .await
        .unwrap();
    let request = tokio::spawn(async move {
        crate::common::post_json_with_cookie(&router, "/api/operator/turns", &cookie, message())
            .await
    });
    // Observe the actual INSERT waiting, rather than guessing when the HTTP
    // handler started. Its absolute deadline is established before this point.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='workspace_operation_admission'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND mode='RowExclusiveLock' AND NOT granted)",
            ).fetch_one(&migrator_pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("actual admission INSERT reaches controlled lock");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    tokio::time::sleep(Duration::from_millis(300)).await;
    insert_gate.rollback().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), request)
        .await
        .expect("expired scheduled turn terminates")
        .unwrap();
    assert_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "operator_unavailable",
    )
    .await;
    assert_eq!(
        terminal_operator(&migrator_pool, org).await,
        ("turn_timeout".into(), 0, 0)
    );
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        0,
        "expired admission cannot start fresh inference"
    );
}

fn inbound_config() -> Config {
    Config::from_source(|name| match name {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(crate::common::TEST_RAW_PAYLOAD_KEY_HEX.into()),
        "CENTRIFUGO_HTTP_API_KEY" => Some(crate::common::TEST_CENTRIFUGO_HTTP_API_KEY.into()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => {
            Some(crate::common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.into())
        }
        "CRM_INBOUND_EMAIL_SECRET" => Some(INBOUND_SECRET.into()),
        _ => None,
    })
    .unwrap()
}
async fn inbound(router: &Router, recipient: &str, raw: &[u8]) -> Response {
    // The established email relay contract uses constant-time bearer auth,
    // not per-message signatures. Exercise that actual endpoint unchanged.
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inbound/email")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {INBOUND_SECRET}"))
                .body(Body::from(
                    json!({"recipient":recipient,"raw":STANDARD.encode(raw)}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn intake_recipient(pool: &PgPool, org: Uuid) -> String {
    let (slug, token): (String, String) =
        sqlx::query_as("SELECT intake_slug,intake_token FROM organization WHERE id=$1")
            .bind(org)
            .fetch_one(pool)
            .await
            .unwrap();
    format!("leads-{token}@{slug}.elysianfeld.com")
}
async fn ingress_counts(pool: &PgPool, org: Uuid) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM raw_payload WHERE organization_id=$1),(SELECT count(*) FROM correspondence_raw WHERE organization_id=$1),(SELECT count(*) FROM correspondence_captured WHERE organization_id=$1),(SELECT count(*) FROM capture_message WHERE organization_id=$1),(SELECT count(*) FROM intake_extraction WHERE organization_id=$1)")
        .bind(org).fetch_one(pool).await.unwrap()
}
#[derive(Default)]
struct CountingExtractor(AtomicUsize);
#[async_trait]
impl LeadExtractor for CountingExtractor {
    async fn extract(&self, _input: &ExtractionInput) -> Result<ExtractorReply, ExtractorError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ExtractorReply {
            content: json!({"is_lead":false,"confidence":0.95}).to_string(),
            prompt_tokens: None,
            completion_tokens: None,
        })
    }
    fn provider(&self) -> &'static str {
        "fake"
    }
    fn model(&self) -> &str {
        "synthetic-held-skip"
    }
}

#[sqlx::test]
#[ignore]
async fn workspace_inbound_retries_before_raw_storage_and_extraction_skips_held_without_attempts(
    migrator_pool: PgPool,
) {
    let (held, _, email) = admin_fixture(&migrator_pool, "held-ingress").await;
    let (operational, _, _) = admin_fixture(&migrator_pool, "operational-ingress").await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let config = inbound_config();
    let publisher = Publisher::recording();
    let router = crm_api::build_app(AppState::for_tests(
        app_pool.clone(),
        &config,
        publisher.clone(),
    ));
    let cookie = crate::common::login_cookie(&router, &email, "pw").await;
    let capture_response =
        crate::common::get_with_cookie(&router, "/api/capture/address", &cookie).await;
    assert_eq!(capture_response.status(), StatusCode::OK);
    let capture_recipient = crate::common::body_json(capture_response).await["address"]
        .as_str()
        .unwrap()
        .to_owned();
    let held_recipient = intake_recipient(&migrator_pool, held).await;
    let operational_recipient = intake_recipient(&migrator_pool, operational).await;
    for recipient in [&held_recipient, &operational_recipient] {
        let response = inbound(&router, recipient, LEAD_EML).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            crate::common::body_json(response).await,
            json!({"status":"accepted"})
        );
    }
    let raw_id: Uuid = sqlx::query_scalar("SELECT id FROM raw_payload WHERE organization_id=$1")
        .bind(held)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    let before_row: (String, Option<String>, i32, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT resolution,unresolved_reason,extraction_attempts,extraction_next_attempt_at FROM raw_payload WHERE id=$1",
    ).bind(raw_id).fetch_one(&migrator_pool).await.unwrap();
    assert_eq!(before_row.0, "unresolved");
    assert_eq!(before_row.1.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(before_row.2, 0);
    let capture_raw = format!("From: Synthetic Admin <{email}>\r\nTo: Synthetic Lead <lead@workspace.invalid>\r\nMessage-ID: <workspace-capture-before@synthetic.invalid>\r\nSubject: Synthetic capture\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nSynthetic body only.\r\n");
    let captured = inbound(&router, &capture_recipient, capture_raw.as_bytes()).await;
    assert_eq!(captured.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(captured).await,
        json!({"status":"accepted"})
    );
    let before = ingress_counts(&migrator_pool, held).await;
    assert_eq!(before.0, 1);
    assert_eq!(
        before.1, 1,
        "operational capture really persists raw evidence"
    );
    force_review(&migrator_pool, held).await;
    let novel_raw = [LEAD_EML, b"\r\nNew synthetic deferred content."].concat();
    let novel_capture = capture_raw.replace("workspace-capture-before", "workspace-capture-held");
    for (recipient, raw) in [
        (&held_recipient, novel_raw.as_slice()),
        (&capture_recipient, novel_capture.as_bytes()),
    ] {
        let response = inbound(&router, recipient, raw).await;
        assert_eq!(response.headers().get("retry-after").unwrap(), "5");
        assert_error(
            response,
            StatusCode::SERVICE_UNAVAILABLE,
            "workspace_in_migration_review",
        )
        .await;
    }
    assert_eq!(
        ingress_counts(&migrator_pool, held).await,
        before,
        "held relay requests store no raw/fact/queue rows"
    );
    let extractor = CountingExtractor::default();
    let first = tokio::time::timeout(
        Duration::from_secs(3),
        extraction_worker::run_once(&app_pool, &config.raw_payload_key, &publisher, &extractor),
    )
    .await
    .expect("bounded extraction pass")
    .unwrap();
    assert_eq!(
        first,
        ExtractionReport {
            claimed: 1,
            not_a_lead: 1,
            ..ExtractionReport::default()
        }
    );
    assert_eq!(
        extractor.0.load(Ordering::SeqCst),
        1,
        "only operational Organization reaches extractor"
    );
    for _ in 0..2 {
        let report = tokio::time::timeout(
            Duration::from_secs(2),
            extraction_worker::run_once(&app_pool, &config.raw_payload_key, &publisher, &extractor),
        )
        .await
        .expect("held-only pass must terminate without hot loop")
        .unwrap();
        assert_eq!(report, ExtractionReport::default());
    }
    let after_row: (String, Option<String>, i32, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT resolution,unresolved_reason,extraction_attempts,extraction_next_attempt_at FROM raw_payload WHERE id=$1",
    ).bind(raw_id).fetch_one(&migrator_pool).await.unwrap();
    assert_eq!(
        after_row, before_row,
        "held work keeps attempts, lease and disposition unchanged"
    );
    assert_eq!(ingress_counts(&migrator_pool, held).await, before);
    assert_eq!(extractor.0.load(Ordering::SeqCst), 1);
}

fn command_context(org: Uuid, actor: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(actor),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}
fn assert_call_review<T>(result: Result<T, CallError>) {
    match result {
        Err(CallError::Database(error)) => assert!(workspace::is_review_error(&error)),
        Err(other) => panic!("expected workspace denial, got {}", other.kind()),
        Ok(_) => panic!("held ordinary call command unexpectedly succeeded"),
    }
}
async fn signed_room_finished(router: &Router, telephony: &Telephony, call: Uuid) -> Response {
    let body = json!({"event":"room_finished","id":Uuid::new_v4(),"room":{"name":Telephony::room_for(call),"sid":"RM_synthetic"}}).to_string().into_bytes();
    let token = telephony.webhook.sign_for_tests(&body, Utc::now(), 300);
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/livekit")
                .header("content-type", "application/json")
                .header("authorization", token)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[sqlx::test]
#[ignore]
async fn workspace_held_calls_allow_signed_terminal_and_owned_hangup_but_not_dial_or_correction(
    migrator_pool: PgPool,
) {
    use crate::common::calls;
    let f = calls::fixture(&migrator_pool).await;
    let (person, phone, _) = calls::create_person_with_phone(
        &f.router,
        &f.alice,
        "held-call@synthetic.invalid",
        Some(f.alice_id),
    )
    .await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (answered, _) = calls::start_with_agent_present(&f, person, phone).await;
    assert_eq!(
        calls::dial(&f.router, &f.alice, answered).await.status(),
        StatusCode::ACCEPTED
    );
    calls::wait_for_status(&f.router, &f.alice, answered, "answered").await;
    let (placing, _) =
        calls::start_as_with_agent_present(&f, &f.carol, f.carol_id, person, phone).await;
    force_review(&migrator_pool, f.org_id).await;
    let dials_before = f
        .provider
        .calls()
        .iter()
        .filter(|call| matches!(call, RecordedCall::Dial { .. }))
        .count();
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    assert_call_review(
        commands::dial_call(
            &app_pool,
            &f.publisher,
            &f.telephony,
            &command_context(f.org_id, f.carol_id),
            CallId::new(placing),
        )
        .await,
    );
    assert_error(
        calls::dial(&f.router, &f.carol, placing).await,
        StatusCode::CONFLICT,
        "workspace_in_migration_review",
    )
    .await;
    assert_eq!(
        f.provider
            .calls()
            .iter()
            .filter(|call| matches!(call, RecordedCall::Dial { .. }))
            .count(),
        dials_before
    );
    assert_eq!(calls::call_row(&migrator_pool, placing).await.0, "placing");

    assert_eq!(
        signed_room_finished(&f.router, &f.telephony, answered)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(calls::call_row(&migrator_pool, answered).await.0, "ended");
    assert_eq!(
        calls::call_row(&migrator_pool, answered).await.2.as_deref(),
        Some("remote_hangup")
    );
    assert_call_review(
        commands::correct_call_outcome(
            &app_pool,
            &f.publisher,
            &command_context(f.org_id, f.alice_id),
            CorrectCallOutcome {
                call_id: CallId::new(answered),
                outcome: CallOutcomeCorrection::Reached,
            },
        )
        .await,
    );
    assert_error(
        calls::correct(&f.router, &f.alice, answered, "reached").await,
        StatusCode::CONFLICT,
        "workspace_in_migration_review",
    )
    .await;
    assert_eq!(
        calls::hangup(&f.router, &f.alice, placing).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        calls::hangup(&f.router, &f.bob, placing).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        calls::hangup(&f.router, &f.carol, placing).await.status(),
        StatusCode::OK
    );
    assert_eq!(calls::call_row(&migrator_pool, placing).await.0, "failed");
    assert_eq!(
        calls::call_row(&migrator_pool, placing).await.1.as_deref(),
        Some("cancelled")
    );
    assert_eq!(
        calls::hangup(&f.router, &f.alice, answered).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        signed_room_finished(&f.router, &f.telephony, answered)
            .await
            .status(),
        StatusCode::OK
    );
    let (completed, corrections): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM call_completed WHERE call_id=$1),(SELECT count(*) FROM contact_attempted WHERE causation_id=$1 AND corrects_id IS NOT NULL)",
    ).bind(answered).fetch_one(&migrator_pool).await.unwrap();
    assert_eq!(
        completed, 1,
        "repeated terminal cleanup does not duplicate facts"
    );
    assert_eq!(corrections, 0, "held correction cannot append an outcome");
}

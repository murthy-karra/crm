//! DB-backed tests for Slice 007f (docs/specs/SLICE_007f.md §10): the
//! extraction worker driven directly via `run_once` with a scripted fake
//! extractor — no network, no Groq. Run only via ./scripts/check-db.
mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::intake::extraction::worker::{run_once, ExtractionReport};
use crm_api::domain::intake::extraction::{
    ExtractionInput, ExtractorError, ExtractorReply, LeadExtractor,
};
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const LEAD_EML: &[u8] = include_bytes!("fixtures/email/unrecognized_lead.eml");
const SPAM_EML: &[u8] = include_bytes!("fixtures/email/spam.eml");

const PW: &str = "pw";
const TEST_INBOUND_EMAIL_SECRET: &str = "test-inbound-email-secret-value-32b";

/// A reply the fixture's contacts genuinely contain.
fn lead_reply() -> String {
    json!({
        "is_lead": true, "confidence": 0.92,
        "first_name": "Priya", "last_name": "Natarajan",
        "email": "priya.natarajan@example.com",
        "phone": "(555) 555-0183",
        "message": "Relocating in October; weekend showing?"
    })
    .to_string()
}

type Script = Result<String, ExtractorError>;

/// (subject, sender_domain, text) as captured by the fake extractor.
type SeenInput = (Option<String>, Option<String>, String);

/// Scripted `LeadExtractor`: pops one reply per call; panics when the
/// script runs dry (a test bug). Optional gate: block inside `extract`
/// until the test releases it (the mid-extraction interleavings).
struct FakeExtractor {
    script: Mutex<VecDeque<Script>>,
    entered: tokio::sync::Semaphore,
    proceed: tokio::sync::Semaphore,
    gated: bool,
    /// Every input seen — the worker seam pin (SLICE_007h1 §3: the
    /// model must see the same view pinned matching saw, i.e. the INNER
    /// message of a recognized forward).
    seen: Mutex<Vec<SeenInput>>,
}

impl FakeExtractor {
    fn scripted(replies: Vec<Script>) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(replies.into()),
            entered: tokio::sync::Semaphore::new(0),
            proceed: tokio::sync::Semaphore::new(0),
            gated: false,
            seen: Mutex::new(Vec::new()),
        })
    }
    fn gated(replies: Vec<Script>) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(replies.into()),
            entered: tokio::sync::Semaphore::new(0),
            proceed: tokio::sync::Semaphore::new(0),
            gated: true,
            seen: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl LeadExtractor for FakeExtractor {
    async fn extract(&self, input: &ExtractionInput) -> Result<ExtractorReply, ExtractorError> {
        self.seen.lock().unwrap().push((
            input.subject.clone(),
            input.sender_domain.clone(),
            input.text.clone(),
        ));
        if self.gated {
            self.entered.add_permits(1);
            let _ = self.proceed.acquire().await.expect("gate open");
        }
        let next = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake extractor script exhausted");
        next.map(|content| ExtractorReply {
            content,
            prompt_tokens: Some(120),
            completion_tokens: Some(40),
        })
    }
    fn provider(&self) -> &'static str {
        "fake"
    }
    fn model(&self) -> &str {
        "fake-model"
    }
}

fn test_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => Some(common::TEST_CENTRIFUGO_HTTP_API_KEY.to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => {
            Some(common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.to_string())
        }
        "CRM_INBOUND_EMAIL_SECRET" => Some(TEST_INBOUND_EMAIL_SECRET.to_string()),
        _ => None,
    })
    .unwrap()
}

async fn build_router(migrator_pool: &PgPool, publisher: Publisher) -> Router {
    let app_pool = common::connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, publisher);
    crm_api::build_app(state)
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected Publisher::Recording");
    };
    recorded.lock().await.clone()
}

async fn intake_row(pool: &PgPool, org_id: Uuid) -> (String, String) {
    sqlx::query_as("SELECT intake_slug, intake_token FROM organization WHERE id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn post_inbound_email(
    router: &Router,
    recipient: &str,
    raw: &[u8],
) -> axum::response::Response {
    let body = json!({ "recipient": recipient, "raw": STANDARD.encode(raw) }).to_string();
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inbound/email")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    format!("Bearer {TEST_INBOUND_EMAIL_SECRET}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Org with stages, alice admin, bob active member as default assignee.
async fn org_with_default(migrator_pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    use crm_api::domain::admin::{MembershipStatus, Role};
    let org_id = common::create_org(migrator_pool, name).await;
    common::seed_stages(migrator_pool, org_id).await;
    let slug: String = name.to_lowercase().replace(' ', "");
    let alice =
        common::create_user(migrator_pool, &format!("alice@{slug}.test"), "Alice", PW).await;
    let bob = common::create_user(migrator_pool, &format!("bob@{slug}.test"), "Bob", PW).await;
    common::add_membership_with(
        migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership(migrator_pool, org_id, bob).await;
    admin_queries::update_intake_default_assignee(
        &mut migrator_pool.acquire().await.unwrap(),
        org_id,
        Some(bob),
    )
    .await
    .unwrap();
    (org_id, bob)
}

/// Delivers `raw` (lands `email_unrecognized_format`) and returns the row
/// id, eligible immediately.
async fn deliver_eligible(router: &Router, pool: &PgPool, org_id: Uuid, raw: &[u8]) -> Uuid {
    let (slug, token) = intake_row(pool, org_id).await;
    let resp = post_inbound_email(
        router,
        &format!("leads-{token}@{slug}.elysianfeld.com"),
        raw,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM raw_payload WHERE organization_id = $1 ORDER BY received_at DESC LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let (reason,): (Option<String>,) =
        sqlx::query_as("SELECT unresolved_reason FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    id
}

/// Clears the lease/backoff so the row is claimable now (test clock
/// control; crm_migrator).
async fn make_due(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE raw_payload SET extraction_next_attempt_at = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn run_pass(
    migrator_pool: &PgPool,
    publisher: &Publisher,
    fake: &FakeExtractor,
) -> ExtractionReport {
    let app_pool = common::connect_as_app(migrator_pool).await;
    let key = test_config().raw_payload_key;
    run_once(&app_pool, &key, publisher, fake).await.unwrap()
}

async fn row_state(pool: &PgPool, id: Uuid) -> (String, Option<String>, i32) {
    sqlx::query_as(
        "SELECT resolution, unresolved_reason, extraction_attempts FROM raw_payload WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn ledger_outcomes(pool: &PgPool, id: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT outcome FROM intake_extraction WHERE raw_payload_id = $1 ORDER BY seq",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Criteria 1 (ledger discipline) + 2 (end-to-end resolution).
#[sqlx::test]
#[ignore]
async fn valid_lead_reply_resolves_end_to_end(migrator_pool: PgPool) {
    let (org_id, bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;
    let events_before = recorded(&publisher).await.len();

    let fake = FakeExtractor::scripted(vec![Ok(lead_reply())]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.claimed, 1);
    assert_eq!(report.resolved, 1);

    let (resolution, _, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "resolved");

    // The Person, normalized, on bob's Today via D-035.
    let (first, last, assigned): (Option<String>, Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT first_name, last_name, assigned_user_id FROM person WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(first.as_deref(), Some("Priya"));
    assert_eq!(last.as_deref(), Some("Natarajan"));
    assert_eq!(assigned, Some(bob));
    let mut methods: Vec<(String, String)> = sqlx::query_as(
        "SELECT kind, normalized_value FROM contact_method WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    methods.sort();
    assert_eq!(
        methods,
        vec![
            ("email".into(), "priya.natarajan@example.com".into()),
            ("phone".into(), "+15555550183".into()),
        ]
    );
    // Inquiry source is fixed 'email'; facts are System/webhook.
    let (source,): (String,) =
        sqlx::query_as("SELECT source FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(source, "email");
    let (actor_kind, actor, on_behalf, origin): (String, Option<Uuid>, Option<Uuid>, String) =
        sqlx::query_as(
            "SELECT actor_kind, actor_user_id, on_behalf_of_user_id, origin FROM inquiry_received WHERE organization_id = $1",
        )
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(actor_kind, "system");
    assert_eq!(actor, None);
    assert_eq!(on_behalf, None);
    assert_eq!(origin, "webhook");

    // Exactly one person_changed for the completion.
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), events_before + 1);
    assert_eq!(events.last().unwrap().1["type"], "person.changed");

    // Ledger: one 'extracted' row with confidence, tokens, and the same
    // correlation id as the facts.
    let (outcome, confidence, prompt_tokens, correlation): (String, Option<f32>, Option<i32>, Uuid) =
        sqlx::query_as(
            "SELECT outcome, confidence, prompt_tokens, correlation_id FROM intake_extraction WHERE raw_payload_id = $1",
        )
        .bind(id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(outcome, "extracted");
    assert!((confidence.unwrap() - 0.92).abs() < 1e-6);
    assert_eq!(prompt_tokens, Some(120));
    let (fact_correlation,): (Uuid,) =
        sqlx::query_as("SELECT correlation_id FROM inquiry_received WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(correlation, fact_correlation, "ledger chains to the facts");

    // Criterion 1: the ledger is append-only. As crm_app the grant
    // denies first; as crm_migrator the trigger is the backstop — both
    // directions pinned.
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let err = sqlx::query(
        "UPDATE intake_extraction SET outcome = 'not_a_lead' WHERE raw_payload_id = $1",
    )
    .bind(id)
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    let err = sqlx::query("DELETE FROM intake_extraction WHERE raw_payload_id = $1")
        .bind(id)
        .execute(&app_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    let err = sqlx::query(
        "UPDATE intake_extraction SET outcome = 'not_a_lead' WHERE raw_payload_id = $1",
    )
    .bind(id)
    .execute(&migrator_pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
    let err = sqlx::query("TRUNCATE intake_extraction")
        .execute(&migrator_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
}

/// Criterion 3: confident spam → terminal not_a_lead + one publish; the
/// row stays discardable (007e).
#[sqlx::test]
#[ignore]
async fn confident_spam_lands_not_a_lead(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, SPAM_EML).await;
    let events_before = recorded(&publisher).await.len();

    let fake = FakeExtractor::scripted(vec![Ok(
        json!({"is_lead": false, "confidence": 0.95}).to_string()
    )]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.not_a_lead, 1);

    let (resolution, reason, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("not_a_lead"));
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), events_before + 1);
    assert_eq!(
        events.last().unwrap().1["type"],
        "intake.unresolved_changed"
    );
    // No Person anywhere.
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // Terminal rows are never claimed again.
    let fake2 = FakeExtractor::scripted(vec![]);
    let report = run_pass(&migrator_pool, &publisher, &fake2).await;
    assert_eq!(report.claimed, 0);

    // Still discardable via 007e.
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/intake/unresolved/{id}/discard"))
                .header("cookie", &alice_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Criteria 4, 5, 6: hallucination and low confidence back off; the third
/// quality failure is terminal; a fourth pass never claims it.
#[sqlx::test]
#[ignore]
async fn quality_failures_back_off_then_terminal(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;

    // 1: hallucinated email; 2: low confidence; 3: schema-invalid.
    let fake =
        FakeExtractor::scripted(vec![
        Ok(json!({"is_lead": true, "confidence": 0.9, "email": "invented@example.com"})
            .to_string()),
        Ok(json!({"is_lead": true, "confidence": 0.69, "email": "priya.natarajan@example.com"})
            .to_string()),
        Ok("not json".to_string()),
    ]);

    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.retryable, 1);
    let (_, reason, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(attempts, 1);
    let (next,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT extraction_next_attempt_at FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(next.unwrap() > chrono::Utc::now(), "backoff set");

    make_due(&migrator_pool, id).await;
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.retryable, 1);
    let (_, _, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(attempts, 2);

    let events_before = recorded(&publisher).await.len();
    make_due(&migrator_pool, id).await;
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.failed_terminal, 1);
    let (resolution, reason, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_extraction_failed"));
    assert_eq!(attempts, 3);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), events_before + 1, "one terminal publish");

    // Granular causes live in the ledger; a fourth pass claims nothing.
    assert_eq!(
        ledger_outcomes(&migrator_pool, id).await,
        vec!["hallucinated_contact", "low_confidence", "schema_invalid"]
    );
    make_due(&migrator_pool, id).await;
    let fake4 = FakeExtractor::scripted(vec![]);
    let report = run_pass(&migrator_pool, &publisher, &fake4).await;
    assert_eq!(report.claimed, 0);
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "no Person from any failed attempt");
}

/// Criterion 7: transport failures never count, never go terminal, and
/// the row resolves when the provider recovers — a lead is never lost.
#[sqlx::test]
#[ignore]
async fn transport_failures_wait_forever_then_recover(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;

    let fake = FakeExtractor::scripted(vec![
        Err(ExtractorError::Unavailable),
        Err(ExtractorError::Timeout),
        Err(ExtractorError::RateLimited),
        Ok(lead_reply()),
    ]);
    for pass in 0..3 {
        let report = run_pass(&migrator_pool, &publisher, &fake).await;
        assert_eq!(report.retryable, 1, "pass {pass}");
        let (resolution, reason, attempts) = row_state(&migrator_pool, id).await;
        assert_eq!(resolution, "unresolved");
        assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
        assert_eq!(attempts, 0, "transport never counts");
        make_due(&migrator_pool, id).await;
    }
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.resolved, 1, "recovered");
    assert_eq!(
        ledger_outcomes(&migrator_pool, id).await,
        vec![
            "provider_unavailable",
            "provider_timeout",
            "rate_limited",
            "extracted"
        ]
    );
}

/// Criterion 8: IntakeBusy inside complete_intake → the row is un-reset
/// (never strands pending) and resolves on a later pass.
#[sqlx::test]
#[ignore]
async fn intake_busy_unresets_and_recovers(migrator_pool: PgPool) {
    use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;

    let hold_duration = ADVISORY_LOCK_BUDGET + Duration::from_secs(4);
    let external_pool = migrator_pool.clone();
    let org_text = org_id.to_string();
    let hold = tokio::spawn(async move {
        let mut tx = external_pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0))")
            .bind(&org_text)
            .execute(&mut *tx)
            .await
            .unwrap();
        tokio::time::sleep(hold_duration).await;
        tx.rollback().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let fake = FakeExtractor::scripted(vec![Ok(lead_reply()), Ok(lead_reply())]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.retryable, 1);
    let (resolution, reason, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "unresolved", "un-reset, not stranded pending");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(attempts, 0, "intake_busy never counts");
    assert!(ledger_outcomes(&migrator_pool, id)
        .await
        .contains(&"intake_busy".to_string()));

    hold.await.unwrap();
    make_due(&migrator_pool, id).await;
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.resolved, 1);
}

/// Criterion 9: a row discarded mid-extraction → superseded; no Person;
/// resolution stays discarded.
#[sqlx::test]
#[ignore]
async fn discard_mid_extraction_supersedes(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;

    let fake = FakeExtractor::gated(vec![Ok(lead_reply())]);
    let pass = {
        let migrator = migrator_pool.clone();
        let publisher = publisher.clone();
        let fake = fake.clone();
        tokio::spawn(async move { run_pass(&migrator, &publisher, &fake).await })
    };
    // Wait until the worker is inside extract(), then discard the row
    // (as crm_app, the columns discard writes).
    let _ = fake.entered.acquire().await.unwrap();
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let alice: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE email = 'alice@acmerealty.test'")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE raw_payload SET resolution = 'discarded', discarded_by_user_id = $2, discarded_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(alice)
    .execute(&app_pool)
    .await
    .unwrap();
    fake.proceed.add_permits(1);

    let report = pass.await.unwrap();
    assert_eq!(report.superseded, 1);
    let (resolution, _, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "discarded");
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        ledger_outcomes(&migrator_pool, id).await,
        vec!["superseded"]
    );
}

/// Criterion 10: two concurrent passes claim disjoint rows (SKIP LOCKED).
#[sqlx::test]
#[ignore]
async fn concurrent_passes_claim_disjoint_rows(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let _a = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;
    let _b = deliver_eligible(&router, &migrator_pool, org_id, SPAM_EML).await;

    // Worker 1 blocks inside its first extract while worker 2 runs a full
    // pass — SKIP LOCKED must route worker 2 to the other row.
    let gated = FakeExtractor::gated(vec![Ok(lead_reply()), Ok(lead_reply())]);
    let pass1 = {
        let migrator = migrator_pool.clone();
        let publisher = publisher.clone();
        let fake = gated.clone();
        tokio::spawn(async move { run_pass(&migrator, &publisher, &fake).await })
    };
    let _ = gated.entered.acquire().await.unwrap();

    let fake2 = FakeExtractor::scripted(vec![Ok(
        json!({"is_lead": false, "confidence": 0.95}).to_string()
    )]);
    let report2 = run_pass(&migrator_pool, &publisher, &fake2).await;
    assert_eq!(report2.claimed, 1, "the second worker got the other row");
    assert_eq!(report2.not_a_lead, 1);

    gated.proceed.add_permits(2);
    let report1 = pass1.await.unwrap();
    assert!(report1.resolved >= 1);
    let (people,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(people, 1, "one lead, one spam");
}

/// Criterion 11: nothing else is ever claimed.
#[sqlx::test]
#[ignore]
async fn only_eligible_rows_are_claimed(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;

    // A generic_v1 unresolved row (no contact method).
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = common::post_json_with_cookie(
        &router,
        "/api/inquiries",
        &alice_cookie,
        json!({ "source": "website", "payload": { "first_name": "NoContact" } }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    // An email row with a future next_attempt (leased).
    let leased = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;
    sqlx::query(
        "UPDATE raw_payload SET extraction_next_attempt_at = now() + interval '1 hour' WHERE id = $1",
    )
    .bind(leased)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let fake = FakeExtractor::scripted(vec![]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.claimed, 0);
}

/// Criterion 12: a 007e Try-again on a terminal extraction row re-arms
/// the counters and the deterministic parse lands it back eligible.
#[sqlx::test]
#[ignore]
async fn workbench_retry_rearms_extraction(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;

    // Drive it terminal with three schema failures.
    let fake = FakeExtractor::scripted(vec![Ok("x".into()), Ok("x".into()), Ok("x".into())]);
    for _ in 0..3 {
        make_due(&migrator_pool, id).await;
        run_pass(&migrator_pool, &publisher, &fake).await;
    }
    let (_, reason, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(reason.as_deref(), Some("email_extraction_failed"));
    assert_eq!(attempts, 3);

    // Admin Try-again re-runs the deterministic parse (unknown format →
    // email_unrecognized_format) and re-arms extraction.
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/intake/unresolved/{id}/retry"))
                .header("cookie", &alice_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["reason"], "email_unrecognized_format");

    let (_, reason, attempts) = row_state(&migrator_pool, id).await;
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(attempts, 0, "re-armed");
    let (next,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT extraction_next_attempt_at FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(next, None);

    // And the worker can now resolve it.
    let fake2 = FakeExtractor::scripted(vec![Ok(lead_reply())]);
    let report = run_pass(&migrator_pool, &publisher, &fake2).await;
    assert_eq!(report.resolved, 1);
}

/// Criterion 13: tenant isolation — each org's row resolves into its own
/// org only; ledgers carry the right org.
#[sqlx::test]
#[ignore]
async fn extraction_is_tenant_isolated(migrator_pool: PgPool) {
    let (org_a, _) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (org_b, _) = org_with_default(&migrator_pool, "Best Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id_a = deliver_eligible(&router, &migrator_pool, org_a, LEAD_EML).await;
    let id_b = deliver_eligible(&router, &migrator_pool, org_b, LEAD_EML).await;

    let fake = FakeExtractor::scripted(vec![Ok(lead_reply()), Ok(lead_reply())]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.resolved, 2);

    for org in [org_a, org_b] {
        let (people,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
                .bind(org)
                .fetch_one(&migrator_pool)
                .await
                .unwrap();
        assert_eq!(people, 1, "{org}");
    }
    for (id, org) in [(id_a, org_a), (id_b, org_b)] {
        let (ledger_org,): (Uuid,) = sqlx::query_as(
            "SELECT organization_id FROM intake_extraction WHERE raw_payload_id = $1",
        )
        .bind(id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(ledger_org, org);
    }
}

/// Criterion 15: a duplicate /api/inquiries replay of a not_a_lead row
/// decodes faithfully.
#[sqlx::test]
#[ignore]
async fn duplicate_replay_of_not_a_lead_decodes_faithfully(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, SPAM_EML).await;
    let fake = FakeExtractor::scripted(vec![Ok(
        json!({"is_lead": false, "confidence": 0.95}).to_string()
    )]);
    run_pass(&migrator_pool, &publisher, &fake).await;
    let (_, reason, _) = row_state(&migrator_pool, id).await;
    assert_eq!(reason.as_deref(), Some("not_a_lead"));

    // Byte-identical redelivery of the (terminal) row: accepted, no
    // reprocessing.
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let resp = post_inbound_email(
        &router,
        &format!("leads-{token}@{slug}.elysianfeld.com"),
        SPAM_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );
    let (_, reason, _) = row_state(&migrator_pool, id).await;
    assert_eq!(reason.as_deref(), Some("not_a_lead"), "not reprocessed");
}

/// Criterion 17: a full worker pass leaks no content into spans or logs.
#[sqlx::test]
#[ignore]
async fn extraction_pass_leaks_no_content(migrator_pool: PgPool) {
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    // Unique org name (global-subscriber caveat — see the other capture
    // tests).
    let (org_id, _bob) = org_with_default(&migrator_pool, "Extraction Capture Org").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let _id = deliver_eligible(&router, &migrator_pool, org_id, LEAD_EML).await;
    let _failed = deliver_eligible(&router, &migrator_pool, org_id, SPAM_EML).await;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    // One success and one quality failure in the same captured pass —
    // the failure paths are the leak-prone ones (adversarial M1). The
    // failing reply itself carries content that must not surface.
    let fake = FakeExtractor::scripted(vec![
        Ok(lead_reply()),
        Ok(
            json!({"is_lead": true, "confidence": 0.9, "email": "SNEAKY.invented@example.com"})
                .to_string(),
        ),
    ]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.resolved, 1);
    assert_eq!(report.retryable, 1, "the hallucinated attempt backed off");

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("intake.extract"), "span present");
    assert!(
        !captured.contains("SNEAKY"),
        "failed-reply content must never surface"
    );
    for leak in [
        "Priya",
        "Natarajan",
        "priya.natarajan@example.com",
        "555-0183",
        "Shoreline",
        "new inquiry on Eospia",
        "elocating in October",
    ] {
        assert!(!captured.contains(leak), "leaked: {leak}");
    }
}

/// Adversarial H1: a deterministically failing row (corrupted ciphertext
/// — an internal_error BEFORE the model is ever called) is BOUNDED: three
/// counted attempts, then terminal — never an infinite retry loop, and
/// never a paid model call (the empty script panics if the extractor is
/// ever invoked).
#[sqlx::test]
#[ignore]
async fn deterministic_internal_errors_are_bounded_not_infinite(migrator_pool: PgPool) {
    use crm_api::domain::raw_payload::crypto;
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    // Sealed against a DIFFERENT id — decrypt fails forever.
    let row_id = Uuid::new_v4();
    let wrong_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, LEAD_EML);
    let sealed = crypto::seal(&key, org_id, wrong_id, LEAD_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution,
             resolved_at, unresolved_reason)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6,
                   'unresolved', now(), 'email_unrecognized_format')"#,
    )
    .bind(row_id)
    .bind(org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(LEAD_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    // An empty script: any extractor invocation panics the test.
    let fake = FakeExtractor::scripted(vec![]);
    for pass in 0..3 {
        let report = run_pass(&migrator_pool, &publisher, &fake).await;
        assert_eq!(report.claimed, 1, "pass {pass}");
        make_due(&migrator_pool, row_id).await;
    }
    let (resolution, reason, attempts) = row_state(&migrator_pool, row_id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_extraction_failed"));
    assert_eq!(attempts, 3);
    assert_eq!(
        ledger_outcomes(&migrator_pool, row_id).await,
        vec!["internal_error", "internal_error", "internal_error"]
    );
    // The fourth pass never claims it — the loop is bounded.
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.claimed, 0);
}

/// Eligibility hard negatives (criterion 11's remaining shapes): an
/// email_unparsed row and a discarded row are never claimed.
#[sqlx::test]
#[ignore]
async fn unparsed_and_discarded_rows_are_never_claimed(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;

    // email_unparsed (garbage bytes).
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let resp = post_inbound_email(
        &router,
        &format!("leads-{token}@{slug}.elysianfeld.com"),
        b"hello world this is not an email",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // A discarded unrecognized-format row.
    let discarded = deliver_eligible(&router, &migrator_pool, org_id, SPAM_EML).await;
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/intake/unresolved/{discarded}/discard"))
                .header("cookie", &alice_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let fake = FakeExtractor::scripted(vec![]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.claimed, 0);
}

// ---------------------------------------------------------------------
// Slice 007h1 (docs/specs/SLICE_007h1.md §3/§5/§6): the worker half of
// the shared unwrap seam, pinned executably — deleting the worker's
// `forward::resolve` call would fail these (the model would see the
// outer Fwd: view instead of the inner message).

const GMAIL_FWD_UNKNOWN_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_unknown_lead.eml");

/// A reply the forwarded fixture's INNER contacts genuinely contain.
fn inner_lead_reply() -> String {
    json!({
        "is_lead": true, "confidence": 0.9,
        "first_name": "Tom", "last_name": "Okafor",
        "email": null,
        "phone": "(628) 555-0942",
        "message": "Relocating; Saturday tour?"
    })
    .to_string()
}

/// Criteria 2 + 8 (worker half): a forwarded unknown-format lead is
/// extracted FROM THE INNER VIEW — inner subject (no "Fwd:"), inner
/// claimed sender domain, inner text with the banner gone — and
/// completes end-to-end from the model's reply.
#[sqlx::test]
#[ignore]
async fn forwarded_unknown_lead_extracts_from_the_inner_view(migrator_pool: PgPool) {
    let (org_id, _bob) = org_with_default(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver_eligible(&router, &migrator_pool, org_id, GMAIL_FWD_UNKNOWN_EML).await;
    make_due(&migrator_pool, id).await;

    let fake = FakeExtractor::scripted(vec![Ok(inner_lead_reply())]);
    let report = run_pass(&migrator_pool, &publisher, &fake).await;
    assert_eq!(report.resolved, 1);

    let seen = fake.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (subject, sender_domain, text) = &seen[0];
    assert_eq!(
        subject.as_deref(),
        Some("Condo tour this weekend?"),
        "inner subject, not the outer Fwd: one"
    );
    assert_eq!(
        sender_domain.as_deref(),
        Some("example.com"),
        "the inner claimed domain, not gmail.com"
    );
    assert!(text.starts_with("Hello - my partner"));
    assert!(
        !text.contains(&["Forwarded", "message"].join(" ")),
        "banner decoration must not reach the model"
    );

    let (resolution, _, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "resolved");
    let (first_name, last_name): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT first_name, last_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(first_name.as_deref(), Some("Tom"));
    assert_eq!(last_name.as_deref(), Some("Okafor"));
}

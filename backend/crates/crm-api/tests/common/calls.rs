//! Shared Slice 006 telephony fixture harness (item 3 of the LATER batch,
//! recorded in docs/specs/SLICE_011d_VERIFICATION.md's residuals: split the
//! largest test files). Used by `db_calls.rs`, `db_calls_corrections.rs`
//! and `db_calls_outcome_today.rs` — one `Fixture` (Acme + Best realty, a
//! scripted telephony runtime, logged-in cookies) and its call-lifecycle
//! helpers, moved here because all three files need them. No test bodies
//! changed; this is exactly the code that used to live at the top of
//! `db_calls.rs`. Run only via ./scripts/check-db.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use crm_api::telephony::{DialOutcome, ScriptedProvider, SipFailure, Telephony, TelephonyLimits};

pub const PW: &str = "pw";
pub const API_KEY: &str = "APIkey-test";
pub const API_SECRET: &[u8] = b"test-livekit-secret-never-logged";

/// Intake identifies People by normalized phone, so every fixture Person
/// gets its own number: `(555) 555-01NN` → `+1555555 01NN`. The
/// log-capture test asserts the digits never reach the output.
static NEXT_PHONE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// `(as entered, last seven digits)`.
pub fn next_phone() -> (String, String) {
    let n = 100 + NEXT_PHONE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    (format!("(555) 555-{n:04}"), format!("555{n:04}"))
}

pub fn limits() -> TelephonyLimits {
    TelephonyLimits {
        ring_timeout: Duration::from_secs(10),
        max_call: Duration::from_secs(60),
        join_ttl: Duration::from_secs(300),
        agent_join_timeout: Duration::from_millis(400),
        presence_poll_interval: Duration::from_millis(10),
    }
}

pub struct Fixture {
    pub org_id: Uuid,
    pub alice_id: Uuid,
    pub carol_id: Uuid,
    pub other_org_id: Uuid,
    pub provider: Arc<ScriptedProvider>,
    pub telephony: Arc<Telephony>,
    pub publisher: Publisher,
    pub router: Router,
    pub alice: String,
    pub carol: String,
    pub bob: String,
}

pub async fn build_router_with_telephony(
    migrator_pool: &PgPool,
    publisher: Publisher,
    telephony: Option<Arc<Telephony>>,
) -> Router {
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = crate::common::test_config();
    let mut state = AppState::for_tests(app_pool, &config, publisher);
    if let Some(telephony) = telephony {
        state = state.with_telephony(telephony);
    }
    crm_api::build_app(state)
}

/// Acme (alice, carol) and Best (bob); a scripted telephony runtime with
/// short dial-task timeouts; logged-in cookies for all three.
pub async fn fixture(migrator_pool: &PgPool) -> Fixture {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    let carol_id = crate::common::create_user(migrator_pool, "carol@acme.test", "Carol", PW).await;
    crate::common::add_membership(migrator_pool, org_id, carol_id).await;
    let (other_org_id, _bob_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        PW,
    )
    .await;

    let provider = Arc::new(ScriptedProvider::new());
    let telephony = Arc::new(Telephony::with_provider(
        provider.clone(),
        "scripted",
        API_KEY,
        API_SECRET,
        limits(),
    ));
    let publisher = Publisher::recording();
    let router =
        build_router_with_telephony(migrator_pool, publisher.clone(), Some(telephony.clone()))
            .await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let carol = crate::common::login_cookie(&router, "carol@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob@best.test", PW).await;
    Fixture {
        org_id,
        alice_id,
        carol_id,
        other_org_id,
        provider,
        telephony,
        publisher,
        router,
        alice,
        carol,
        bob,
    }
}

/// A Person with its own phone (and an email), via intake, assigned to
/// `assignee`. Returns `(person_id, phone_contact_method_id,
/// email_contact_method_id)`; see `create_person_with_phone_digits` for
/// the number.
pub async fn create_person_with_phone(
    router: &Router,
    cookie: &str,
    email: &str,
    assignee: Option<Uuid>,
) -> (Uuid, Uuid, Uuid) {
    let (ids, _) = create_person_with_phone_digits(router, cookie, email, assignee).await;
    ids
}

pub async fn create_person_with_phone_digits(
    router: &Router,
    cookie: &str,
    email: &str,
    assignee: Option<Uuid>,
) -> ((Uuid, Uuid, Uuid), String) {
    let (phone, digits) = next_phone();
    let resp = crate::common::post_inquiry(
        router,
        cookie,
        "zillow",
        json!({ "email": email, "phone": phone, "message": "hi" }),
        assignee,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let person_id: Uuid = crate::common::body_json(resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(router, &format!("/api/people/{person_id}"), cookie).await,
    )
    .await;
    let methods = detail["contact_methods"].as_array().unwrap();
    let id_of = |kind: &str| -> Uuid {
        methods.iter().find(|m| m["kind"] == kind).unwrap()["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    ((person_id, id_of("phone"), id_of("email")), digits)
}

pub async fn start(
    router: &Router,
    cookie: &str,
    person_id: Uuid,
    cm: Uuid,
) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        &format!("/api/people/{person_id}/calls"),
        cookie,
        json!({ "contact_method_id": cm }),
    )
    .await
}

pub async fn post_empty(router: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn dial(router: &Router, cookie: &str, call_id: Uuid) -> axum::response::Response {
    post_empty(router, &format!("/api/calls/{call_id}/dial"), cookie).await
}

pub async fn hangup(router: &Router, cookie: &str, call_id: Uuid) -> axum::response::Response {
    post_empty(router, &format!("/api/calls/{call_id}/hangup"), cookie).await
}

pub async fn get_call(router: &Router, cookie: &str, call_id: Uuid) -> Value {
    let resp =
        crate::common::get_with_cookie(router, &format!("/api/calls/{call_id}"), cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
    crate::common::body_json(resp).await["call"].clone()
}

/// Polls `GET /api/calls/{id}` until `status`, or panics after 5 s.
pub async fn wait_for_status(router: &Router, cookie: &str, call_id: Uuid, status: &str) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let call = get_call(router, cookie, call_id).await;
        if call["status"] == status {
            return call;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "call {call_id} never reached {status}: {call}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Starts a call and marks the agent present so the dial task proceeds;
/// returns `(call_id, start body)`.
pub async fn start_with_agent_present(f: &Fixture, person_id: Uuid, cm: Uuid) -> (Uuid, Value) {
    start_as_with_agent_present(f, &f.alice, f.alice_id, person_id, cm).await
}

/// `start_with_agent_present` for an arbitrary caller (`cookie`, `user_id`).
pub async fn start_as_with_agent_present(
    f: &Fixture,
    cookie: &str,
    user_id: Uuid,
    person_id: Uuid,
    cm: Uuid,
) -> (Uuid, Value) {
    let resp = start(&f.router, cookie, person_id, cm).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = crate::common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();
    f.provider.set_present(
        &Telephony::room_for(call_id),
        &Telephony::agent_identity(user_id),
        true,
    );
    (call_id, body)
}

pub async fn call_row(
    pool: &PgPool,
    call_id: Uuid,
) -> (String, Option<String>, Option<String>, Uuid) {
    sqlx::query_as(
        "SELECT status, failure_reason, end_reason, correlation_id FROM call WHERE id = $1",
    )
    .bind(call_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected a recording publisher");
    };
    recorded.lock().await.clone()
}

pub async fn today_has(router: &Router, cookie: &str, person_id: Uuid) -> bool {
    today_item(router, cookie, person_id).await.is_some()
}

/// `person_id`'s `TodayItem` on `cookie`'s Today, if any.
pub async fn today_item(router: &Router, cookie: &str, person_id: Uuid) -> Option<Value> {
    let body = crate::common::body_json(
        crate::common::get_with_cookie(router, "/api/today", cookie).await,
    )
    .await;
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["person"]["id"] == person_id.to_string())
        .cloned()
}

/// `person_id`'s priority on `cookie`'s Today (`None` = not listed).
pub async fn today_priority(router: &Router, cookie: &str, person_id: Uuid) -> Option<String> {
    today_item(router, cookie, person_id)
        .await
        .map(|item| item["priority"].as_str().unwrap().to_string())
}

pub async fn correct(
    router: &Router,
    cookie: &str,
    call_id: Uuid,
    outcome: &str,
) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        &format!("/api/calls/{call_id}/outcome"),
        cookie,
        json!({ "outcome": outcome }),
    )
    .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct CorrectionRow {
    pub id: Uuid,
    pub channel: String,
    pub outcome: String,
    pub actor_kind: String,
    pub actor_user_id: Option<Uuid>,
    pub origin: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub corrects_id: Option<Uuid>,
}

/// Every attempt of `person_id`, oldest `recorded_at` first.
pub async fn attempt_rows(pool: &PgPool, person_id: Uuid) -> Vec<CorrectionRow> {
    sqlx::query_as(
        r#"SELECT id, channel, outcome, actor_kind, actor_user_id, origin, occurred_at,
                  recorded_at, correlation_id, causation_id, corrects_id
           FROM contact_attempted WHERE person_id = $1 ORDER BY recorded_at, id"#,
    )
    .bind(person_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// An answered call, hung up by alice: `(call_id, person_id, phone)`.
pub async fn answered_and_ended(
    f: &Fixture,
    email: &str,
    assignee: Option<Uuid>,
) -> (Uuid, Uuid, Uuid) {
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, email, assignee).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(f, person_id, phone).await;
    assert_eq!(
        dial(&f.router, &f.alice, call_id).await.status(),
        StatusCode::ACCEPTED
    );
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    assert_eq!(
        hangup(&f.router, &f.alice, call_id).await.status(),
        StatusCode::OK
    );
    (call_id, person_id, phone)
}

/// A call that fails as busy, hung up-agent-side by the dial task's own
/// terminal transition (no explicit hangup call needed): `call_id`.
pub async fn busy_call(f: &Fixture, person_id: Uuid, phone: Uuid) -> Uuid {
    f.provider
        .push_dial(Ok(DialOutcome::Failed(SipFailure::Busy)));
    let (call_id, _) = start_with_agent_present(f, person_id, phone).await;
    assert_eq!(
        dial(&f.router, &f.alice, call_id).await.status(),
        StatusCode::ACCEPTED
    );
    wait_for_status(&f.router, &f.alice, call_id, "failed").await;
    call_id
}

pub trait CollectBytes {
    async fn collect_bytes(self) -> Vec<u8>;
}

impl CollectBytes for Body {
    async fn collect_bytes(self) -> Vec<u8> {
        use http_body_util::BodyExt;
        self.collect().await.unwrap().to_bytes().to_vec()
    }
}

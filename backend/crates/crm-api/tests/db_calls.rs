//! DB-backed tests for the Slice 006 call routes, commands, `settle`, the
//! dial task, and the LiveKit webhook, over the `ScriptedProvider`
//! (docs/specs/SLICE_006.md §13 item 2), plus the sweep (`run_once`,
//! driven directly with backdated rows) and the `call_completed` history
//! kind. Run only via ./scripts/check-db.
mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::domain::telephony::sweep;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use crm_api::telephony::{
    DialOutcome, ProviderError, RecordedCall, ScriptedProvider, SipFailure, Telephony,
    TelephonyLimits,
};

const PW: &str = "pw";
const API_KEY: &str = "APIkey-test";
const API_SECRET: &[u8] = b"test-livekit-secret-never-logged";
/// Intake identifies People by normalized phone, so every fixture Person
/// gets its own number: `(555) 555-01NN` → `+1555555 01NN`. The
/// log-capture test asserts the digits never reach the output.
static NEXT_PHONE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// `(as entered, last seven digits)`.
fn next_phone() -> (String, String) {
    let n = 100 + NEXT_PHONE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    (format!("(555) 555-{n:04}"), format!("555{n:04}"))
}

fn limits() -> TelephonyLimits {
    TelephonyLimits {
        ring_timeout: Duration::from_secs(10),
        max_call: Duration::from_secs(60),
        join_ttl: Duration::from_secs(300),
        agent_join_timeout: Duration::from_millis(400),
        presence_poll_interval: Duration::from_millis(10),
    }
}

struct Fixture {
    org_id: Uuid,
    alice_id: Uuid,
    carol_id: Uuid,
    other_org_id: Uuid,
    provider: Arc<ScriptedProvider>,
    telephony: Arc<Telephony>,
    publisher: Publisher,
    router: Router,
    alice: String,
    carol: String,
    bob: String,
}

async fn build_router_with_telephony(
    migrator_pool: &PgPool,
    publisher: Publisher,
    telephony: Option<Arc<Telephony>>,
) -> Router {
    let app_pool = common::connect_as_app(migrator_pool).await;
    let config = common::test_config();
    let mut state = AppState::for_tests(app_pool, &config, publisher);
    if let Some(telephony) = telephony {
        state = state.with_telephony(telephony);
    }
    crm_api::build_app(state)
}

/// Acme (alice, carol) and Best (bob); a scripted telephony runtime with
/// short dial-task timeouts; logged-in cookies for all three.
async fn fixture(migrator_pool: &PgPool) -> Fixture {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    let carol_id = common::create_user(migrator_pool, "carol@acme.test", "Carol", PW).await;
    common::add_membership(migrator_pool, org_id, carol_id).await;
    let (other_org_id, _bob_id) = common::create_org_with_stages_and_member(
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
    let alice = common::login_cookie(&router, "alice@acme.test", PW).await;
    let carol = common::login_cookie(&router, "carol@acme.test", PW).await;
    let bob = common::login_cookie(&router, "bob@best.test", PW).await;
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
async fn create_person_with_phone(
    router: &Router,
    cookie: &str,
    email: &str,
    assignee: Option<Uuid>,
) -> (Uuid, Uuid, Uuid) {
    let (ids, _) = create_person_with_phone_digits(router, cookie, email, assignee).await;
    ids
}

async fn create_person_with_phone_digits(
    router: &Router,
    cookie: &str,
    email: &str,
    assignee: Option<Uuid>,
) -> ((Uuid, Uuid, Uuid), String) {
    let (phone, digits) = next_phone();
    let resp = common::post_inquiry(
        router,
        cookie,
        "zillow",
        json!({ "email": email, "phone": phone, "message": "hi" }),
        assignee,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let person_id: Uuid = common::body_json(resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let detail = common::body_json(
        common::get_with_cookie(router, &format!("/api/people/{person_id}"), cookie).await,
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

async fn start(
    router: &Router,
    cookie: &str,
    person_id: Uuid,
    cm: Uuid,
) -> axum::response::Response {
    common::post_json_with_cookie(
        router,
        &format!("/api/people/{person_id}/calls"),
        cookie,
        json!({ "contact_method_id": cm }),
    )
    .await
}

async fn post_empty(router: &Router, uri: &str, cookie: &str) -> axum::response::Response {
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

async fn dial(router: &Router, cookie: &str, call_id: Uuid) -> axum::response::Response {
    post_empty(router, &format!("/api/calls/{call_id}/dial"), cookie).await
}

async fn hangup(router: &Router, cookie: &str, call_id: Uuid) -> axum::response::Response {
    post_empty(router, &format!("/api/calls/{call_id}/hangup"), cookie).await
}

async fn get_call(router: &Router, cookie: &str, call_id: Uuid) -> Value {
    let resp = common::get_with_cookie(router, &format!("/api/calls/{call_id}"), cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
    common::body_json(resp).await["call"].clone()
}

/// Polls `GET /api/calls/{id}` until `status`, or panics after 5 s.
async fn wait_for_status(router: &Router, cookie: &str, call_id: Uuid, status: &str) -> Value {
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
async fn start_with_agent_present(f: &Fixture, person_id: Uuid, cm: Uuid) -> (Uuid, Value) {
    let resp = start(&f.router, &f.alice, person_id, cm).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();
    f.provider.set_present(
        &Telephony::room_for(call_id),
        &Telephony::agent_identity(f.alice_id),
        true,
    );
    (call_id, body)
}

async fn webhook(router: &Router, telephony: &Telephony, event: Value) -> axum::response::Response {
    let body = event.to_string().into_bytes();
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

fn participant_left(call_id: Uuid, identity: &str) -> Value {
    json!({
        "event": "participant_left",
        "id": Uuid::new_v4(),
        "room": { "name": Telephony::room_for(call_id), "sid": "RM_x" },
        "participant": { "identity": identity, "sid": "PA_x",
                         "attributes": { "sip.phoneNumber": "+15555550100" } },
    })
}

fn room_finished(call_id: Uuid) -> Value {
    json!({
        "event": "room_finished",
        "id": Uuid::new_v4(),
        "room": { "name": Telephony::room_for(call_id), "sid": "RM_x" },
    })
}

// --- DB assertions ---------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct AttemptRow {
    channel: String,
    outcome: String,
    actor_kind: String,
    actor_user_id: Option<Uuid>,
    origin: String,
    correlation_id: Uuid,
    causation_id: Option<Uuid>,
}

async fn attempts_for(pool: &PgPool, person_id: Uuid) -> Vec<AttemptRow> {
    sqlx::query_as(
        r#"SELECT channel, outcome, actor_kind, actor_user_id, origin, correlation_id, causation_id
           FROM contact_attempted WHERE person_id = $1 ORDER BY recorded_at"#,
    )
    .bind(person_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[derive(Debug, sqlx::FromRow)]
struct CompletedRow {
    outcome: String,
    talk_seconds: Option<i32>,
    answered_at_present: bool,
    correlation_id: Uuid,
    causation_id: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    origin: String,
    contact_method_id: Uuid,
}

async fn completed_for(pool: &PgPool, call_id: Uuid) -> Vec<CompletedRow> {
    sqlx::query_as(
        r#"SELECT outcome, talk_seconds, answered_at IS NOT NULL AS answered_at_present,
                  correlation_id, causation_id, actor_user_id, origin, contact_method_id
           FROM call_completed WHERE call_id = $1 ORDER BY recorded_at"#,
    )
    .bind(call_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn call_row(pool: &PgPool, call_id: Uuid) -> (String, Option<String>, Option<String>, Uuid) {
    sqlx::query_as(
        "SELECT status, failure_reason, end_reason, correlation_id FROM call WHERE id = $1",
    )
    .bind(call_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected a recording publisher");
    };
    recorded.lock().await.clone()
}

async fn today_has(router: &Router, cookie: &str, person_id: Uuid) -> bool {
    let body = common::body_json(common::get_with_cookie(router, "/api/today", cookie).await).await;
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["person"]["id"] == person_id.to_string())
}

// --- Schema / grants ---------------------------------------------------

/// `call`: SELECT + INSERT, column-level UPDATE on exactly the
/// status/timestamp columns, no DELETE (docs/specs/SLICE_006.md §2).
#[sqlx::test]
#[ignore]
async fn crm_app_call_grants_are_exactly_section_2(migrator_pool: PgPool) {
    let app_pool = common::connect_as_app(&migrator_pool).await;
    assert!(sqlx::query("SELECT * FROM call")
        .fetch_all(&app_pool)
        .await
        .is_ok());
    for column in [
        "status",
        "failure_reason",
        "end_reason",
        "provider_call_ref",
        "dial_requested_at",
        "ringing_at",
        "answered_at",
        "ended_at",
        "updated_at",
    ] {
        let update = sqlx::query(&format!("UPDATE call SET {column} = {column} WHERE false"))
            .execute(&app_pool)
            .await;
        assert!(update.is_ok(), "call.{column}: UPDATE must be granted");
    }
    for column in [
        "id",
        "organization_id",
        "person_id",
        "contact_method_id",
        "caller_user_id",
        "origin",
        "correlation_id",
        "provider",
        "provider_room",
        "placed_at",
        "created_at",
    ] {
        let update = sqlx::query(&format!("UPDATE call SET {column} = {column} WHERE false"))
            .execute(&app_pool)
            .await;
        assert!(update.is_err(), "call.{column}: UPDATE must be denied");
    }
    assert!(sqlx::query("DELETE FROM call")
        .execute(&app_pool)
        .await
        .is_err());
    // The partial unique index exists and is what guards one active call.
    let (indexdef,): (String,) = sqlx::query_as(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'call_one_active_per_caller'",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(indexdef.contains("UNIQUE"), "{indexdef}");
    assert!(indexdef.contains("WHERE"), "{indexdef}");
}

// --- start ---------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn start_returns_201_with_a_join_grant_whose_claims_are_exactly_section_3(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let ((person_id, phone, _), digits) =
        create_person_with_phone_digits(&f.router, &f.alice, "lead1@example.com", Some(f.alice_id))
            .await;
    let before = recorded(&f.publisher).await.len();

    let resp = start(&f.router, &f.alice, person_id, phone).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = common::body_json(resp).await;
    let call = &body["call"];
    let call_id: Uuid = call["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(call["status"], "placing");
    assert_eq!(call["person_id"], person_id.to_string());
    assert_eq!(call["contact_method_id"], phone.to_string());
    assert_eq!(call["caller"]["id"], f.alice_id.to_string());
    assert_eq!(call["caller"]["display_name"], "Alice");
    assert!(call["placed_at"].is_string());
    for key in [
        "failure_reason",
        "end_reason",
        "ringing_at",
        "answered_at",
        "ended_at",
        "talk_seconds",
    ] {
        assert!(call[key].is_null(), "{key}");
    }
    assert_eq!(call.as_object().unwrap().len(), 12);
    // PII-free: no number anywhere in the response.
    assert!(!body.to_string().contains(&digits));

    let room = format!("call:{call_id}");
    assert_eq!(body["join"]["room"], room);
    assert_eq!(body["join"]["url"], "ws://127.0.0.1:7880");
    let token = body["join"]["token"].as_str().unwrap();
    let mut parts = token.split('.');
    let header: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts.next().unwrap()).unwrap()).unwrap();
    assert_eq!(header, json!({ "alg": "HS256", "typ": "JWT" }));
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts.next().unwrap()).unwrap()).unwrap();
    let exp = claims["exp"].as_i64().unwrap();
    let nbf = claims["nbf"].as_i64().unwrap();
    assert_eq!(exp - nbf, 300);
    assert!((nbf - Utc::now().timestamp()).abs() < 5);
    assert_eq!(
        claims,
        json!({
            "iss": API_KEY,
            "sub": format!("agent:{}", f.alice_id),
            "exp": exp,
            "nbf": nbf,
            "video": {
                "room": room,
                "roomJoin": true,
                "canPublish": true,
                "canSubscribe": true,
                "canPublishData": false,
            },
        })
    );

    // The room was created with the configured max-call duration.
    assert_eq!(
        f.provider.calls(),
        vec![RecordedCall::CreateRoom {
            room: room.clone(),
            max_call: Duration::from_secs(60),
        }]
    );
    // One `call.changed`, nothing else, and the stored row is `placing`
    // with `provider = 'scripted'`.
    let events = recorded(&f.publisher).await.split_off(before);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["type"], "call.changed");
    assert_eq!(events[0].1["data"]["call_id"], call_id.to_string());
    assert_eq!(events[0].1["data"]["person_id"], person_id.to_string());
    let (provider, provider_room): (String, String) =
        sqlx::query_as("SELECT provider, provider_room FROM call WHERE id = $1")
            .bind(call_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(provider, "scripted");
    assert_eq!(provider_room, room);
}

#[sqlx::test]
#[ignore]
async fn start_on_a_foreign_or_nonexistent_person_is_a_byte_identical_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead2@example.com", None).await;

    let foreign = start(&f.router, &f.bob, person_id, phone).await;
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    let foreign_body = foreign.into_body().collect_bytes().await;

    let missing = start(&f.router, &f.alice, Uuid::new_v4(), phone).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let missing_body = missing.into_body().collect_bytes().await;
    assert_eq!(foreign_body, missing_body);
    assert_eq!(
        serde_json::from_slice::<Value>(&foreign_body).unwrap()["error"],
        "not_found"
    );

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM call")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

trait CollectBytes {
    async fn collect_bytes(self) -> Vec<u8>;
}

impl CollectBytes for Body {
    async fn collect_bytes(self) -> Vec<u8> {
        use http_body_util::BodyExt;
        self.collect().await.unwrap().to_bytes().to_vec()
    }
}

#[sqlx::test]
#[ignore]
async fn start_with_an_invalid_contact_method_is_an_identical_422(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, _phone, email) =
        create_person_with_phone(&f.router, &f.alice, "lead3@example.com", None).await;
    let (_other_person, other_phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead3b@example.com", None).await;
    let (_bob_person, bob_phone, _) =
        create_person_with_phone(&f.router, &f.bob, "lead3c@example.com", None).await;

    let mut bodies = Vec::new();
    for (label, cm) in [
        ("email", email),
        ("other person's phone", other_phone),
        ("foreign phone", bob_phone),
        ("nonexistent", Uuid::new_v4()),
    ] {
        let resp = start(&f.router, &f.alice, person_id, cm).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY, "{label}");
        bodies.push(resp.into_body().collect_bytes().await);
    }
    assert!(bodies.iter().all(|b| b == &bodies[0]));
    assert_eq!(
        serde_json::from_slice::<Value>(&bodies[0]).unwrap()["error"],
        "invalid_contact_method"
    );

    // Unknown field and non-JSON are 400 `malformed_request`.
    let resp = common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/calls"),
        &f.alice,
        json!({ "contact_method_id": email, "phone": "+15555550100" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(common::body_json(resp).await["error"], "malformed_request");

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM call")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(f.provider.calls().is_empty(), "no room is created on a 422");
}

#[sqlx::test]
#[ignore]
async fn a_real_concurrent_second_call_is_409_from_the_unique_index(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead4@example.com", None).await;
    let (person2, phone2, _) =
        create_person_with_phone(&f.router, &f.alice, "lead4b@example.com", None).await;

    let (a, b) = tokio::join!(
        start(&f.router, &f.alice, person_id, phone),
        start(&f.router, &f.alice, person2, phone2),
    );
    let mut statuses = [a.status(), b.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::CONFLICT]);
    let (created, conflict) = if a.status() == StatusCode::CREATED {
        (a, b)
    } else {
        (b, a)
    };
    let created = common::body_json(created).await;
    let conflict = common::body_json(conflict).await;
    assert_eq!(conflict["error"], "call_in_progress");
    assert_eq!(conflict["call_id"], created["call"]["id"]);

    // A sequential third attempt is the same 409, and carol (another
    // member) is not blocked by alice's call.
    let resp = start(&f.router, &f.alice, person2, phone2).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let resp = start(&f.router, &f.carol, person2, phone2).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM call WHERE status = 'placing'")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[sqlx::test]
#[ignore]
async fn provider_create_room_failure_settles_provider_error_and_is_503(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead5@example.com", Some(f.alice_id)).await;
    f.provider
        .fail_create_room(ProviderError::Unavailable("connection refused".into()));

    let resp = start(&f.router, &f.alice, person_id, phone).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        common::body_json(resp).await["error"],
        "telephony_unavailable"
    );

    let (call_id, status, failure_reason): (Uuid, String, Option<String>) =
        sqlx::query_as("SELECT id, status, failure_reason FROM call")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(failure_reason.as_deref(), Some("provider_error"));
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "provider_error");
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    assert!(today_has(&f.router, &f.alice, person_id).await);
    // The caller is free to try again: the failed call is not "active".
    f.provider.calls(); // (create_room is still failing; just prove no 409)
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// --- the answered flow -------------------------------------------------

#[sqlx::test]
#[ignore]
async fn answered_call_writes_exactly_one_reached_attempt_and_advances_today(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let ((person_id, phone, _), digits) =
        create_person_with_phone_digits(&f.router, &f.alice, "lead6@example.com", Some(f.alice_id))
            .await;
    assert!(today_has(&f.router, &f.alice, person_id).await);
    let before = recorded(&f.publisher).await.len();
    f.provider.push_dial(Ok(DialOutcome::Answered {
        call_ref: Some("SCL_test".into()),
    }));

    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    let resp = dial(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(common::body_json(resp).await["call"]["status"], "placing");

    let call = wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    assert!(call["ringing_at"].is_string());
    assert!(call["answered_at"].is_string());
    assert!(call["ended_at"].is_null());

    // The dial used the normalized number and the `sip:<call_id>` identity.
    let dials: Vec<_> = f
        .provider
        .calls()
        .into_iter()
        .filter_map(|c| match c {
            RecordedCall::Dial {
                room,
                to_number,
                participant_identity,
                ring_timeout,
                max_call,
            } => Some((
                room,
                to_number,
                participant_identity,
                ring_timeout,
                max_call,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(dials.len(), 1);
    assert_eq!(dials[0].0, Telephony::room_for(call_id));
    assert_eq!(dials[0].1.expose(), format!("+1555{digits}"));
    assert_eq!(dials[0].2, Telephony::sip_identity(call_id));
    assert_eq!(dials[0].3, Duration::from_secs(10));
    assert_eq!(dials[0].4, Duration::from_secs(60));

    // Exactly one attempt, with the call's envelope.
    let (_, _, _, correlation_id) = call_row(&migrator_pool, call_id).await;
    let attempts = attempts_for(&migrator_pool, person_id).await;
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    let a = &attempts[0];
    assert_eq!(a.channel, "call");
    assert_eq!(a.outcome, "reached");
    assert_eq!(a.actor_kind, "user");
    assert_eq!(a.actor_user_id, Some(f.alice_id));
    assert_eq!(a.origin, "web_session");
    assert_eq!(a.correlation_id, correlation_id);
    assert_eq!(a.causation_id, Some(call_id));
    // No completed-call fact yet: the call is still answered.
    assert!(completed_for(&migrator_pool, call_id).await.is_empty());
    let (_, _, _, ref_row): (String, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT status, failure_reason, end_reason, provider_call_ref FROM call WHERE id = $1",
        )
        .bind(call_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(ref_row.as_deref(), Some("SCL_test"));

    // Today advances for the caller.
    assert!(!today_has(&f.router, &f.alice, person_id).await);

    // Publisher order: start → call.changed; ringing → call.changed;
    // answered → call.changed then person.changed{contact_attempted}.
    let events = recorded(&f.publisher).await.split_off(before);
    let types: Vec<String> = events
        .iter()
        .map(|(_, e)| e["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        types,
        vec![
            "call.changed",
            "call.changed",
            "call.changed",
            "person.changed"
        ]
    );
    assert_eq!(events[3].1["data"]["change"], "contact_attempted");
    assert_eq!(events[3].1["data"]["person_id"], person_id.to_string());
    assert_eq!(events[2].1["correlation_id"], correlation_id.to_string());
    assert_eq!(events[3].1["correlation_id"], correlation_id.to_string());
    for (channel, _) in &events {
        assert_eq!(channel, &format!("org:{}", f.org_id));
    }
}

#[sqlx::test]
#[ignore]
async fn answered_call_advances_a_non_caller_members_today(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead7@example.com", Some(f.carol_id)).await;
    assert!(today_has(&f.router, &f.carol, person_id).await);
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));

    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    assert_eq!(
        dial(&f.router, &f.alice, call_id).await.status(),
        StatusCode::ACCEPTED
    );
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    assert!(!today_has(&f.router, &f.carol, person_id).await);

    // Carol (same Organization, not the caller) can read the call; bob
    // (other Organization) cannot.
    let carol_view = get_call(&f.router, &f.carol, call_id).await;
    assert_eq!(carol_view["status"], "answered");
    let resp = common::get_with_cookie(&f.router, &format!("/api/calls/{call_id}"), &f.bob).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// --- failures ----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn busy_declined_and_ring_timeout_each_write_one_no_answer_attempt(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    for (failure, reason) in [
        (SipFailure::Busy, "busy"),
        (SipFailure::Declined, "declined"),
        (SipFailure::RingTimeout, "ring_timeout"),
        (SipFailure::NoAnswer, "no_answer"),
    ] {
        let (person_id, phone, _) = create_person_with_phone(
            &f.router,
            &f.alice,
            &format!("lead8-{reason}@example.com"),
            Some(f.alice_id),
        )
        .await;
        f.provider.push_dial(Ok(DialOutcome::Failed(failure)));
        let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
        assert_eq!(
            dial(&f.router, &f.alice, call_id).await.status(),
            StatusCode::ACCEPTED
        );
        let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
        assert_eq!(call["failure_reason"], reason, "{reason}");
        assert!(call["ringing_at"].is_string());
        assert!(call["ended_at"].is_string());
        assert!(call["talk_seconds"].is_null());

        let attempts = attempts_for(&migrator_pool, person_id).await;
        assert_eq!(attempts.len(), 1, "{reason}: {attempts:?}");
        assert_eq!(attempts[0].outcome, "no_answer");
        assert_eq!(attempts[0].causation_id, Some(call_id));
        let completed = completed_for(&migrator_pool, call_id).await;
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].outcome, reason);
        assert!(completed[0].talk_seconds.is_none());
        assert!(!completed[0].answered_at_present);
        assert_eq!(completed[0].contact_method_id, phone);
        assert!(!today_has(&f.router, &f.alice, person_id).await, "{reason}");
        // The room was deleted after the failure.
        assert!(f.provider.calls().iter().any(|c| matches!(
            c,
            RecordedCall::Hangup { room } if room == &Telephony::room_for(call_id)
        )));
    }
}

#[sqlx::test]
#[ignore]
async fn agent_not_joined_and_provider_error_write_no_attempt(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;

    // Agent never joins: the task gives up after `agent_join_timeout`.
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead9@example.com", Some(f.alice_id)).await;
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    let body = common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        dial(&f.router, &f.alice, call_id).await.status(),
        StatusCode::ACCEPTED
    );
    let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
    assert_eq!(call["failure_reason"], "agent_not_joined");
    assert!(call["ringing_at"].is_null());
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "agent_not_joined");
    assert!(today_has(&f.router, &f.alice, person_id).await);
    assert_eq!(f.provider.dials_completed(), 0, "no dial was attempted");

    // Provider error during the dial (and a SIP 5xx → provider_error).
    for (label, result) in [
        ("timeout", Err(ProviderError::Timeout)),
        ("sip 503", Ok(DialOutcome::Failed(SipFailure::Other(503)))),
    ] {
        let (person_id, phone, _) = create_person_with_phone(
            &f.router,
            &f.alice,
            &format!("lead9-{}@example.com", label.replace(' ', "")),
            Some(f.alice_id),
        )
        .await;
        f.provider.push_dial(result);
        let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
        assert_eq!(
            dial(&f.router, &f.alice, call_id).await.status(),
            StatusCode::ACCEPTED
        );
        let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
        assert_eq!(call["failure_reason"], "provider_error", "{label}");
        assert!(call["ringing_at"].is_string(), "{label}");
        assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
        let completed = completed_for(&migrator_pool, call_id).await;
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].outcome, "provider_error");
        assert!(today_has(&f.router, &f.alice, person_id).await, "{label}");
    }
}

// --- hangup ------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn hangup_before_ringing_is_cancelled_without_an_attempt(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead10@example.com", Some(f.alice_id)).await;
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    let body = common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();

    // Mic denied: the client hangs up before ever dialing.
    let resp = hangup(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let call = common::body_json(resp).await["call"].clone();
    assert_eq!(call["status"], "failed");
    assert_eq!(call["failure_reason"], "cancelled");
    assert!(call["ended_at"].is_string());
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "cancelled");
    assert!(today_has(&f.router, &f.alice, person_id).await);
    assert_eq!(
        f.provider.calls().last(),
        Some(&RecordedCall::Hangup {
            room: Telephony::room_for(call_id)
        })
    );

    // Idempotent: a second hangup is 200 with the same state, still
    // deletes the room best-effort, writes nothing new.
    let resp = hangup(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let again = common::body_json(resp).await["call"].clone();
    assert_eq!(again, call);
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    let hangups = f
        .provider
        .calls()
        .iter()
        .filter(|c| matches!(c, RecordedCall::Hangup { .. }))
        .count();
    assert_eq!(hangups, 2);

    // A dial after cancel is 409 `invalid_call_state`; the caller is free
    // to start a new call.
    let resp = dial(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(common::body_json(resp).await["error"], "invalid_call_state");
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
#[ignore]
async fn hangup_while_the_dial_is_in_flight_is_cancelled_with_one_attempt_and_the_late_result_is_a_noop(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead11@example.com", Some(f.alice_id)).await;
    let release = f.provider.push_blocked_dial();

    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    assert_eq!(
        dial(&f.router, &f.alice, call_id).await.status(),
        StatusCode::ACCEPTED
    );
    wait_for_status(&f.router, &f.alice, call_id, "ringing").await;

    // Double dial while ringing → 409.
    let resp = dial(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(common::body_json(resp).await["error"], "invalid_call_state");

    // Hang up while the provider's dial is parked.
    let resp = hangup(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let call = common::body_json(resp).await["call"].clone();
    assert_eq!(call["status"], "failed");
    assert_eq!(call["failure_reason"], "cancelled");
    let attempts = attempts_for(&migrator_pool, person_id).await;
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, "no_answer");
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    assert!(!today_has(&f.router, &f.alice, person_id).await);

    // Now the callee "answers": the dial task's late result is absorbed.
    release
        .send(Ok(DialOutcome::Answered {
            call_ref: Some("late".into()),
        }))
        .unwrap();
    f.provider.wait_for_dials_completed(1).await;
    // The task deletes the room after discovering the no-op; wait for it.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let hangups = f
            .provider
            .calls()
            .iter()
            .filter(|c| matches!(c, RecordedCall::Hangup { .. }))
            .count();
        if hangups >= 2 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (status, failure_reason, end_reason, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "failed");
    assert_eq!(failure_reason.as_deref(), Some("cancelled"));
    assert!(end_reason.is_none());
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    let (ref_row,): (Option<String>,) =
        sqlx::query_as("SELECT provider_call_ref FROM call WHERE id = $1")
            .bind(call_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(ref_row.is_none(), "the late Answered wrote nothing");
}

#[sqlx::test]
#[ignore]
async fn hangup_on_an_answered_call_ends_it_with_agent_hangup_and_talk_seconds(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead12@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    // Backdate answered_at so talk_seconds is non-trivial (the one
    // sanctioned migrator write: timestamps, never domain rows).
    sqlx::query("UPDATE call SET answered_at = answered_at - interval '72 seconds' WHERE id = $1")
        .bind(call_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let resp = hangup(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let call = common::body_json(resp).await["call"].clone();
    assert_eq!(call["status"], "ended");
    assert_eq!(call["end_reason"], "agent_hangup");
    assert!(call["failure_reason"].is_null());
    let talk = call["talk_seconds"].as_i64().unwrap();
    assert!((72..=75).contains(&talk), "{talk}");

    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "reached");
    assert_eq!(completed[0].talk_seconds, Some(talk as i32));
    assert!(completed[0].answered_at_present);
    assert_eq!(completed[0].actor_user_id, Some(f.alice_id));
    assert_eq!(completed[0].origin, "web_session");
    assert_eq!(completed[0].causation_id, Some(call_id));
    let (_, _, _, correlation_id) = call_row(&migrator_pool, call_id).await;
    assert_eq!(completed[0].correlation_id, correlation_id);
    // Still exactly the one attempt written at answer time.
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);
}

#[sqlx::test]
#[ignore]
async fn hangup_and_dial_are_caller_only_and_foreign_is_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead13@example.com", None).await;
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    let body = common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();

    for (label, cookie, expected) in [
        ("carol hangup", &f.carol, StatusCode::FORBIDDEN),
        ("bob hangup", &f.bob, StatusCode::NOT_FOUND),
    ] {
        let resp = hangup(&f.router, cookie, call_id).await;
        assert_eq!(resp.status(), expected, "{label}");
    }
    for (label, cookie, expected) in [
        ("carol dial", &f.carol, StatusCode::FORBIDDEN),
        ("bob dial", &f.bob, StatusCode::NOT_FOUND),
    ] {
        let resp = dial(&f.router, cookie, call_id).await;
        assert_eq!(resp.status(), expected, "{label}");
    }
    let resp = hangup(&f.router, &f.carol, call_id).await;
    assert_eq!(common::body_json(resp).await["error"], "forbidden");
    let resp = hangup(&f.router, &f.bob, Uuid::new_v4()).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Nothing changed; carol can still read it; the caller can still dial.
    let (status, _, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "placing");
    assert_eq!(
        get_call(&f.router, &f.carol, call_id).await["status"],
        "placing"
    );
    let _ = f.other_org_id;
}

// --- webhook ------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn webhook_remote_hangup_ends_an_answered_call_and_duplicates_are_noops(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead14@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    sqlx::query("UPDATE call SET answered_at = answered_at - interval '5 seconds' WHERE id = $1")
        .bind(call_id)
        .execute(&migrator_pool)
        .await
        .unwrap();
    let before = recorded(&f.publisher).await.len();

    let resp = webhook(
        &f.router,
        &f.telephony,
        participant_left(call_id, &Telephony::sip_identity(call_id)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(common::body_json(resp).await, json!({}));
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["status"], "ended");
    assert_eq!(call["end_reason"], "remote_hangup");
    let talk = call["talk_seconds"].as_i64().unwrap();
    assert!((5..=8).contains(&talk), "{talk}");
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "reached");
    assert_eq!(completed[0].talk_seconds, Some(talk as i32));
    assert_eq!(recorded(&f.publisher).await.len(), before + 1);

    // Duplicate / out-of-order: room_finished and another participant_left
    // after the terminal state are 200 no-ops.
    for event in [
        room_finished(call_id),
        participant_left(call_id, &Telephony::sip_identity(call_id)),
        participant_left(call_id, &Telephony::agent_identity(f.alice_id)),
    ] {
        let resp = webhook(&f.router, &f.telephony, event).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    assert_eq!(get_call(&f.router, &f.alice, call_id).await, call);
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    assert_eq!(recorded(&f.publisher).await.len(), before + 1);

    // The caller's own (late) hangup is idempotent too.
    let resp = hangup(&f.router, &f.alice, call_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(common::body_json(resp).await["call"], call);
}

#[sqlx::test]
#[ignore]
async fn webhook_unknown_room_is_200_and_writes_nothing(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead15@example.com", None).await;
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    let body = common::body_json(resp).await;
    let call_id: Uuid = body["call"]["id"].as_str().unwrap().parse().unwrap();
    let before = recorded(&f.publisher).await.len();

    let unknown = Uuid::new_v4();
    for event in [
        room_finished(unknown),
        participant_left(unknown, "sip:whatever"),
        json!({ "event": "room_finished", "room": { "name": "not-a-call-room" } }),
        json!({ "event": "room_finished" }),
        json!({ "event": "participant_joined", "room": { "name": Telephony::room_for(call_id) },
                "participant": { "identity": Telephony::sip_identity(call_id) } }),
    ] {
        let resp = webhook(&f.router, &f.telephony, event).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(common::body_json(resp).await, json!({}));
    }
    let (status, _, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "placing");
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM call_completed")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(recorded(&f.publisher).await.len(), before);
}

#[sqlx::test]
#[ignore]
async fn webhook_with_a_tampered_body_wrong_secret_or_expired_token_is_401(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead16@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    let genuine = room_finished(call_id).to_string().into_bytes();
    let tampered = participant_left(call_id, &Telephony::sip_identity(call_id))
        .to_string()
        .into_bytes();
    let valid_token = f
        .telephony
        .webhook
        .sign_for_tests(&genuine, Utc::now(), 300);
    let wrong_secret = crm_api::telephony::WebhookVerifier::new(API_KEY, b"wrong-secret")
        .sign_for_tests(&genuine, Utc::now(), 300);
    let wrong_key = crm_api::telephony::WebhookVerifier::new("OtherKey", API_SECRET)
        .sign_for_tests(&genuine, Utc::now(), 300);
    let expired = f.telephony.webhook.sign_for_tests(
        &genuine,
        Utc::now() - chrono::Duration::seconds(1000),
        300,
    );

    for (label, token, body) in [
        ("tampered body", valid_token.clone(), tampered.clone()),
        ("wrong secret", wrong_secret, genuine.clone()),
        ("wrong key", wrong_key, genuine.clone()),
        ("expired", expired, genuine.clone()),
        ("no header", String::new(), genuine.clone()),
    ] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/webhooks/livekit")
            .header("content-type", "application/json");
        if !token.is_empty() {
            builder = builder.header("authorization", token);
        }
        let resp = f
            .router
            .clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{label}");
        assert_eq!(
            common::body_json(resp).await["error"],
            "unauthenticated",
            "{label}"
        );
    }
    let (status, _, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "answered", "nothing was written");
    assert!(completed_for(&migrator_pool, call_id).await.is_empty());
}

#[sqlx::test]
#[ignore]
async fn webhook_agent_left_in_answered_is_agent_disconnected_and_in_ringing_is_cancelled(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;

    // answered → ended{agent_disconnected}
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead17@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    let before = f.provider.calls().len();
    let resp = webhook(
        &f.router,
        &f.telephony,
        participant_left(call_id, &Telephony::agent_identity(f.alice_id)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["status"], "ended");
    assert_eq!(call["end_reason"], "agent_disconnected");
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "reached");
    // Exactly one best-effort room delete after the settle.
    assert_eq!(
        f.provider.calls()[before..],
        [RecordedCall::Hangup {
            room: Telephony::room_for(call_id)
        }]
    );
    // A later `room_finished` is a no-op and records no hangup (the
    // room is already gone).
    let before = f.provider.calls().len();
    let resp = webhook(&f.router, &f.telephony, room_finished(call_id)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(f.provider.calls().len(), before);

    // And a fresh answered call finished by `room_finished` alone: the
    // transition applies, no hangup is recorded.
    let (person_rf, phone_rf, _) =
        create_person_with_phone(&f.router, &f.alice, "lead17c@example.com", Some(f.alice_id))
            .await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_rf, _) = start_with_agent_present(&f, person_rf, phone_rf).await;
    dial(&f.router, &f.alice, call_rf).await;
    wait_for_status(&f.router, &f.alice, call_rf, "answered").await;
    let before = f.provider.calls().len();
    let resp = webhook(&f.router, &f.telephony, room_finished(call_rf)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_call(&f.router, &f.alice, call_rf).await["end_reason"],
        "remote_hangup"
    );
    assert_eq!(f.provider.calls().len(), before);

    // ringing → failed{cancelled} with one no_answer attempt.
    let (person2, phone2, _) =
        create_person_with_phone(&f.router, &f.alice, "lead17b@example.com", Some(f.alice_id))
            .await;
    let release = f.provider.push_blocked_dial();
    let (call2, _) = start_with_agent_present(&f, person2, phone2).await;
    dial(&f.router, &f.alice, call2).await;
    wait_for_status(&f.router, &f.alice, call2, "ringing").await;
    let before = f.provider.calls().len();
    let resp = webhook(
        &f.router,
        &f.telephony,
        participant_left(call2, &Telephony::agent_identity(f.alice_id)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let call = get_call(&f.router, &f.alice, call2).await;
    assert_eq!(call["status"], "failed");
    assert_eq!(call["failure_reason"], "cancelled");
    assert_eq!(
        f.provider.calls()[before..],
        [RecordedCall::Hangup {
            room: Telephony::room_for(call2)
        }]
    );
    let attempts = attempts_for(&migrator_pool, person2).await;
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, "no_answer");
    assert!(!today_has(&f.router, &f.alice, person2).await);

    // The parked dial then fails: still exactly one attempt.
    release
        .send(Ok(DialOutcome::Failed(SipFailure::NoAnswer)))
        .unwrap();
    f.provider.wait_for_dials_completed(1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(attempts_for(&migrator_pool, person2).await.len(), 1);
    assert_eq!(completed_for(&migrator_pool, call2).await.len(), 1);
    let (status, failure_reason, _, _) = call_row(&migrator_pool, call2).await;
    assert_eq!(status, "failed");
    assert_eq!(failure_reason.as_deref(), Some("cancelled"));
}

#[sqlx::test]
#[ignore]
async fn out_of_order_sip_participant_left_in_ringing_is_a_noop_and_the_dial_failure_still_yields_one_attempt(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead18@example.com", Some(f.alice_id)).await;
    let release = f.provider.push_blocked_dial();
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "ringing").await;
    let before = recorded(&f.publisher).await.len();

    for event in [
        participant_left(call_id, &Telephony::sip_identity(call_id)),
        room_finished(call_id),
    ] {
        let resp = webhook(&f.router, &f.telephony, event).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["status"], "ringing");
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    assert_eq!(recorded(&f.publisher).await.len(), before);

    release
        .send(Ok(DialOutcome::Failed(SipFailure::Busy)))
        .unwrap();
    let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
    assert_eq!(call["failure_reason"], "busy");
    let attempts = attempts_for(&migrator_pool, person_id).await;
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, "no_answer");
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
}

#[sqlx::test]
#[ignore]
async fn answer_then_immediate_remote_leave_is_absorbed_by_the_presence_recheck(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead19@example.com", Some(f.alice_id)).await;
    let release = f.provider.push_blocked_dial();
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "ringing").await;

    // The callee answers and hangs up within the same instant: the
    // scripted provider normally marks the SIP participant present on
    // Answered; with the switch off, the task's one re-check finds it gone.
    f.provider.set_callee_leaves_immediately(true);
    release
        .send(Ok(DialOutcome::Answered { call_ref: None }))
        .unwrap();
    // `Answered` is settled first (attempt `reached`), then the re-check.
    let call = wait_for_status(&f.router, &f.alice, call_id, "ended").await;
    assert!(call["answered_at"].is_string());
    assert_eq!(call["end_reason"], "remote_hangup");
    let attempts = attempts_for(&migrator_pool, person_id).await;
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, "reached");
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
}

// --- telephony disabled --------------------------------------------------

#[sqlx::test]
#[ignore]
async fn reads_work_and_writes_are_503_with_telephony_disabled(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead20@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    hangup(&f.router, &f.alice, call_id).await;

    let disabled = build_router_with_telephony(&migrator_pool, Publisher::recording(), None).await;
    let cookie = common::login_cookie(&disabled, "alice@acme.test", PW).await;
    let call = get_call(&disabled, &cookie, call_id).await;
    assert_eq!(call["status"], "ended");
    assert_eq!(call["end_reason"], "agent_hangup");
    // The Person page (history included) still loads.
    let resp =
        common::get_with_cookie(&disabled, &format!("/api/people/{person_id}"), &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);

    for (label, resp) in [
        ("start", start(&disabled, &cookie, person_id, phone).await),
        ("dial", dial(&disabled, &cookie, call_id).await),
        ("hangup", hangup(&disabled, &cookie, call_id).await),
    ] {
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "{label}");
        assert_eq!(
            common::body_json(resp).await["error"],
            "telephony_disabled",
            "{label}"
        );
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM call")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// --- log capture -----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn the_fixture_number_never_appears_in_spans_or_log_lines(migrator_pool: PgPool) {
    use tracing_subscriber::layer::SubscriberExt;

    let f = fixture(&migrator_pool).await;
    let ((person_id, phone, _), digits) = create_person_with_phone_digits(
        &f.router,
        &f.alice,
        "lead21@example.com",
        Some(f.alice_id),
    )
    .await;

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

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = CaptureWriter(buffer.clone());
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    // Process-global on purpose: this is the only test in the binary that
    // installs a subscriber. A thread-local `set_default` was flaky under
    // the parallel test harness — tracing's per-callsite interest cache
    // is rebuilt process-wide, and while sibling tests hit the same
    // callsites with no dispatcher of their own, whole spans (`call.dial`)
    // were silently dropped from the capture. Sibling tests' output also
    // lands in the buffer; harmless, and it widens the PII sweep.
    tracing::subscriber::set_global_default(subscriber)
        .expect("the log-capture test must be the only one installing a subscriber");

    f.provider.push_dial(Ok(DialOutcome::Answered {
        call_ref: Some("SCL_log".into()),
    }));
    let (call_id, body) = start_with_agent_present(&f, person_id, phone).await;
    let token = body["join"]["token"].as_str().unwrap().to_string();
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    let _ = webhook(
        &f.router,
        &f.telephony,
        participant_left(call_id, &Telephony::sip_identity(call_id)),
    )
    .await;
    wait_for_status(&f.router, &f.alice, call_id, "ended").await;
    let _ = hangup(&f.router, &f.alice, call_id).await;
    // A failed dial too, for the warn! paths.
    f.provider
        .push_dial(Err(ProviderError::Rejected("trunk auth".into())));
    let (call2, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call2).await;
    wait_for_status(&f.router, &f.alice, call2, "failed").await;

    // The status flips to failed/ended *before* the dial task logs its
    // final line, so wait for both tasks to finish rather than racing them.
    let snapshot = || String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while snapshot().matches("dial task finished").count() < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "both dial tasks must have logged their final line"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let captured = snapshot();
    assert!(
        captured.contains("call.dial{") && captured.contains("call.start{"),
        "the capture must have seen the command spans at all"
    );
    assert!(
        !captured.contains(&digits),
        "captured output must never contain the fixture number: {captured}"
    );
    assert!(
        !captured.contains(&token),
        "captured output must never contain the join token"
    );
    assert!(
        !captured.contains(std::str::from_utf8(API_SECRET).unwrap()),
        "captured output must never contain the API secret"
    );
}

// --- sweep -----------------------------------------------------------------

/// The one sanctioned migrator write (SLICE_004 §11 fixture rule): backdate
/// a timestamp. `crm_app` cannot update `placed_at` by grant design, and
/// no domain command writes one, so the sweep's horizons are reached here.
async fn backdate(pool: &PgPool, call_id: Uuid, column: &str, seconds: i64) {
    let sql =
        format!("UPDATE call SET {column} = {column} - make_interval(secs => $2) WHERE id = $1");
    sqlx::query(&sql)
        .bind(call_id)
        .bind(seconds as f64)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
#[ignore]
async fn sweep_expires_a_backdated_placing_call_without_an_attempt(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead30@example.com", Some(f.alice_id)).await;
    let resp = start(&f.router, &f.alice, person_id, phone).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let call_id: Uuid = common::body_json(resp).await["call"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Not yet past the horizon: nothing happens.
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 0);
    assert_eq!(call_row(&migrator_pool, call_id).await.0, "placing");

    backdate(&migrator_pool, call_id, "placed_at", 10 + 30 + 5).await;
    let before = f.provider.calls().len();
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 1);
    assert_eq!(report.hangup_failures, 0);

    let (status, failure_reason, end_reason, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "failed");
    assert_eq!(failure_reason.as_deref(), Some("expired"));
    assert!(end_reason.is_none());
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "expired");
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    // Best-effort room delete after the settle.
    let calls = f.provider.calls();
    assert_eq!(
        calls[before..],
        [RecordedCall::Hangup {
            room: Telephony::room_for(call_id)
        }]
    );
    // Idempotent: a second pass finds nothing.
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 0);
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    // Any member reads the final state.
    let call = get_call(&f.router, &f.carol, call_id).await;
    assert_eq!(call["status"], "failed");
    assert_eq!(call["failure_reason"], "expired");
}

#[sqlx::test]
#[ignore]
async fn sweep_expires_a_backdated_ringing_call_without_an_attempt(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead31@example.com", Some(f.alice_id)).await;
    // A blocked dial parks the call in `ringing`.
    let release = f.provider.push_blocked_dial();
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "ringing").await;

    // `ringing_at` is the horizon's anchor, not `placed_at`.
    backdate(&migrator_pool, call_id, "placed_at", 3600).await;
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 0);
    assert_eq!(call_row(&migrator_pool, call_id).await.0, "ringing");

    backdate(&migrator_pool, call_id, "ringing_at", 10 + 30 + 5).await;
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 1);
    let (status, failure_reason, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "failed");
    assert_eq!(failure_reason.as_deref(), Some("expired"));
    assert_eq!(
        completed_for(&migrator_pool, call_id).await[0].outcome,
        "expired"
    );
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());

    // The dial task's late answer is a no-op against the terminal row.
    release
        .send(Ok(DialOutcome::Answered { call_ref: None }))
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while f.provider.dials_completed() < 1 {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (status, _, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "failed");
    assert_eq!(completed_for(&migrator_pool, call_id).await.len(), 1);
    assert!(attempts_for(&migrator_pool, person_id).await.is_empty());
    // The dial task's no-op against the terminal row still deleted the
    // room (the sweep's own hangup plus the task's).
    let hangups = f
        .provider
        .calls()
        .iter()
        .filter(
            |c| matches!(c, RecordedCall::Hangup { room } if *room == Telephony::room_for(call_id)),
        )
        .count();
    assert_eq!(hangups, 2);
}

#[sqlx::test]
#[ignore]
async fn sweep_reconciles_a_backdated_answered_call_and_records_hangup_best_effort(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead32@example.com", Some(f.alice_id)).await;
    f.provider.push_dial(Ok(DialOutcome::Answered {
        call_ref: Some("SCL_sweep".into()),
    }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);

    // max_call (60 s in the fixture) + 60 s, not yet reached.
    backdate(&migrator_pool, call_id, "answered_at", 60 + 60 - 10).await;
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 0);

    backdate(&migrator_pool, call_id, "answered_at", 20).await;
    let before_calls = f.provider.calls().len();
    let before_events = recorded(&f.publisher).await.len();
    let report = sweep::run_once(&migrator_pool, &f.publisher, &f.telephony, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.finalized, 1);
    assert_eq!(report.hangup_failures, 0);

    let (status, failure_reason, end_reason, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "ended");
    assert!(failure_reason.is_none());
    assert_eq!(end_reason.as_deref(), Some("reconciled"));
    let completed = completed_for(&migrator_pool, call_id).await;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].outcome, "reached");
    assert!(completed[0].answered_at_present);
    let talk = completed[0].talk_seconds.unwrap();
    assert!((130..=140).contains(&talk), "{talk}");
    // Still exactly the one attempt from answer time.
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);
    // Hangup recorded after the settle; one `call.changed`, no attempt event.
    let calls = f.provider.calls();
    assert_eq!(
        calls[before_calls..],
        [RecordedCall::Hangup {
            room: Telephony::room_for(call_id)
        }]
    );
    let events = recorded(&f.publisher).await;
    let new_events: Vec<&str> = events[before_events..]
        .iter()
        .map(|(_, e)| e["type"].as_str().unwrap())
        .collect();
    assert_eq!(new_events, ["call.changed"]);
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["end_reason"], "reconciled");
    assert_eq!(call["talk_seconds"].as_i64().unwrap() as i32, talk);
}

// --- history ---------------------------------------------------------------

/// `call_completed` in Person history (docs/specs/SLICE_006.md §2): kind
/// `call_completed`, PII-free detail, `kind_rank` 5 sorting after the
/// same-instant `contact_attempted` a failed dial writes in the same
/// transaction; readable with telephony disabled.
#[sqlx::test]
#[ignore]
async fn history_call_completed_sorts_after_a_same_instant_contact_attempted(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let ((person_id, phone, _), digits) = create_person_with_phone_digits(
        &f.router,
        &f.alice,
        "lead33@example.com",
        Some(f.alice_id),
    )
    .await;

    // A busy dial: `contact_attempted(call, no_answer)` and
    // `call_completed(busy)` at the same `occurred_at` and `recorded_at`.
    f.provider
        .push_dial(Ok(DialOutcome::Failed(SipFailure::Busy)));
    let (busy_call, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, busy_call).await;
    wait_for_status(&f.router, &f.alice, busy_call, "failed").await;

    // An answered call with talk time, hung up by the agent.
    f.provider.push_dial(Ok(DialOutcome::Answered {
        call_ref: Some("SCL_hist".into()),
    }));
    let (answered_call, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, answered_call).await;
    wait_for_status(&f.router, &f.alice, answered_call, "answered").await;
    backdate(&migrator_pool, answered_call, "answered_at", 72).await;
    let resp = hangup(&f.router, &f.alice, answered_call).await;
    assert_eq!(resp.status(), StatusCode::OK);

    for (label, router, cookie) in [
        ("enabled", f.router.clone(), f.alice.clone()),
        (
            "disabled",
            build_router_with_telephony(&migrator_pool, Publisher::recording(), None).await,
            String::new(),
        ),
    ] {
        let cookie = if cookie.is_empty() {
            common::login_cookie(&router, "carol@acme.test", PW).await
        } else {
            cookie
        };
        let resp =
            common::get_with_cookie(&router, &format!("/api/people/{person_id}"), &cookie).await;
        assert_eq!(resp.status(), StatusCode::OK, "{label}");
        let body = common::body_json(resp).await;
        let raw = body.to_string();
        assert!(!raw.contains(&digits), "{label}: history must be PII-free");
        let history = body["history"].as_array().unwrap();

        let index_of = |kind: &str, call_id: Uuid| {
            history
                .iter()
                .position(|e| {
                    e["kind"] == kind
                        && (kind != "call_completed"
                            || e["detail"]["call_id"] == call_id.to_string())
                })
                .unwrap_or_else(|| panic!("{label}: no {kind} for {call_id}: {history:?}"))
        };
        let busy_attempt = history
            .iter()
            .position(|e| e["kind"] == "contact_attempted" && e["detail"]["outcome"] == "no_answer")
            .unwrap();
        let busy_completed = index_of("call_completed", busy_call);
        assert_eq!(
            history[busy_attempt]["occurred_at"], history[busy_completed]["occurred_at"],
            "{label}: same instant"
        );
        assert_eq!(
            busy_completed,
            busy_attempt + 1,
            "{label}: call_completed (kind_rank 5) sorts directly after the attempt (4)"
        );
        let busy = &history[busy_completed];
        assert_eq!(busy["detail"]["outcome"], "busy");
        assert!(busy["detail"]["talk_seconds"].is_null());
        assert!(busy["detail"]["answered_at"].is_null());
        assert_eq!(busy["actor"]["id"], f.alice_id.to_string());
        assert_eq!(busy["origin"], "web_session");
        assert_eq!(
            busy["detail"].as_object().unwrap().len(),
            4,
            "{label}: exactly call_id, outcome, talk_seconds, answered_at"
        );

        let reached = &history[index_of("call_completed", answered_call)];
        assert_eq!(reached["detail"]["outcome"], "reached");
        let talk = reached["detail"]["talk_seconds"].as_i64().unwrap();
        assert!((72..=75).contains(&talk), "{label}: {talk}");
        assert!(reached["detail"]["answered_at"].is_string());
        let (_, _, _, correlation_id) = call_row(&migrator_pool, answered_call).await;
        assert_eq!(reached["correlation_id"], correlation_id.to_string());
        // Every call-derived fact shares the call's correlation id.
        let reached_attempt = history
            .iter()
            .find(|e| {
                e["kind"] == "contact_attempted"
                    && e["correlation_id"] == correlation_id.to_string()
            })
            .unwrap();
        assert_eq!(reached_attempt["detail"]["outcome"], "reached");
    }
}

// --- review additions ------------------------------------------------------

/// A `participant_left` naming another member's agent identity on alice's
/// call is ignored: 200, nothing written, no hangup.
#[sqlx::test]
#[ignore]
async fn webhook_for_a_foreign_participant_identity_is_ignored(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead40@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    let before_calls = f.provider.calls().len();
    let before_events = recorded(&f.publisher).await.len();
    for identity in [
        Telephony::agent_identity(f.carol_id),
        format!("sip:{}", Uuid::new_v4()),
    ] {
        let resp = webhook(
            &f.router,
            &f.telephony,
            participant_left(call_id, &identity),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "{identity}");
    }
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["status"], "answered");
    assert!(completed_for(&migrator_pool, call_id).await.is_empty());
    assert_eq!(f.provider.calls().len(), before_calls);
    assert_eq!(recorded(&f.publisher).await.len(), before_events);

    // The call's own SIP identity still applies.
    let resp = webhook(
        &f.router,
        &f.telephony,
        participant_left(call_id, &Telephony::sip_identity(call_id)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_call(&f.router, &f.alice, call_id).await["status"],
        "ended"
    );
}

/// Two concurrent `POST /dial`: exactly one 202, one 409, one dial.
#[sqlx::test]
#[ignore]
async fn two_concurrent_dials_yield_one_202_and_one_recorded_dial(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead41@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;

    let (a, b) = tokio::join!(
        dial(&f.router, &f.alice, call_id),
        dial(&f.router, &f.alice, call_id)
    );
    let mut statuses = [a.status(), b.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::ACCEPTED, StatusCode::CONFLICT]);
    let conflict = if a.status() == StatusCode::CONFLICT {
        a
    } else {
        b
    };
    assert_eq!(
        common::body_json(conflict).await["error"],
        "invalid_call_state"
    );

    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let dials = f
        .provider
        .calls()
        .iter()
        .filter(|c| matches!(c, RecordedCall::Dial { .. }))
        .count();
    assert_eq!(dials, 1);
    assert_eq!(f.provider.dials_completed(), 1);
    assert_eq!(attempts_for(&migrator_pool, person_id).await.len(), 1);
}

/// Reverse direction of the foreign probe: alice against bob's call.
#[sqlx::test]
#[ignore]
async fn alice_cannot_see_dial_or_hang_up_bobs_call(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (bob_person, bob_phone, _) =
        create_person_with_phone(&f.router, &f.bob, "lead42@best.test", None).await;
    let resp = start(&f.router, &f.bob, bob_person, bob_phone).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bob_call: Uuid = common::body_json(resp).await["call"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let get = common::get_with_cookie(&f.router, &format!("/api/calls/{bob_call}"), &f.alice).await;
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
    let get_body = get.into_body().collect_bytes().await;
    let dial_resp = dial(&f.router, &f.alice, bob_call).await;
    assert_eq!(dial_resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(dial_resp.into_body().collect_bytes().await, get_body);
    let hangup_resp = hangup(&f.router, &f.alice, bob_call).await;
    assert_eq!(hangup_resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(hangup_resp.into_body().collect_bytes().await, get_body);

    // Untouched: still bob's `placing` call, nothing recorded for it.
    let (status, _, _, _) = call_row(&migrator_pool, bob_call).await;
    assert_eq!(status, "placing");
    assert!(completed_for(&migrator_pool, bob_call).await.is_empty());
    assert_eq!(
        get_call(&f.router, &f.bob, bob_call).await["status"],
        "placing"
    );
}

/// A correctly signed body that is not JSON is 200 `{}` and writes nothing.
#[sqlx::test]
#[ignore]
async fn webhook_with_a_valid_signature_but_non_json_body_is_200_and_writes_nothing(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead43@example.com", Some(f.alice_id)).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    let before_calls = f.provider.calls().len();
    let before_events = recorded(&f.publisher).await.len();
    let body = format!("participant_left {}", Telephony::room_for(call_id)).into_bytes();
    let token = f.telephony.webhook.sign_for_tests(&body, Utc::now(), 300);
    let resp = f
        .router
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
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(common::body_json(resp).await, json!({}));
    assert_eq!(
        get_call(&f.router, &f.alice, call_id).await["status"],
        "answered"
    );
    assert!(completed_for(&migrator_pool, call_id).await.is_empty());
    assert_eq!(f.provider.calls().len(), before_calls);
    assert_eq!(recorded(&f.publisher).await.len(), before_events);
}

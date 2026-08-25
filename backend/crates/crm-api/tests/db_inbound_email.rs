//! DB-backed tests for Slice 007b (docs/specs/SLICE_007b.md §11):
//! `POST /inbound/email`, acceptance criteria 1-9, 11-15, 19-20. Run only
//! via ./scripts/check-db.
mod common;

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
use crm_api::domain::raw_payload::crypto;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const PLAIN_EML: &[u8] = include_bytes!("fixtures/email/plain.eml");
const MULTIPART_EML: &[u8] = include_bytes!("fixtures/email/multipart.eml");

/// >= 32 bytes, distinct from any other test secret in the suite.
const TEST_INBOUND_EMAIL_SECRET: &str = "test-inbound-email-secret-value-32b";

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

/// A router with `CRM_INBOUND_EMAIL_SECRET` unset — the endpoint-disabled
/// case (criterion 8, "secret unset behaves identically" to a wrong bearer).
async fn build_router_no_secret(migrator_pool: &PgPool, publisher: Publisher) -> Router {
    common::build_router_with_publisher(migrator_pool, publisher).await
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected Publisher::Recording");
    };
    recorded.lock().await.clone()
}

fn recipient(slug: &str, token: &str) -> String {
    format!("leads-{token}@{slug}.elysianfeld.com")
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
    bearer: Option<&str>,
    recipient: &str,
    raw: &[u8],
) -> axum::response::Response {
    let body = json!({ "recipient": recipient, "raw": STANDARD.encode(raw) });
    post_inbound_email_raw_body(router, bearer, &body.to_string()).await
}

async fn post_inbound_email_raw_body(
    router: &Router,
    bearer: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/inbound/email")
        .header("content-type", "application/json");
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

struct RawPayloadRow {
    source: String,
    payload_format: String,
    origin: String,
    resolution: String,
    unresolved_reason: Option<String>,
    byte_len: i32,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    received_at: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::type_complexity)]
async fn raw_payload_row(pool: &PgPool, id: Uuid) -> RawPayloadRow {
    let (
        source,
        payload_format,
        origin,
        resolution,
        unresolved_reason,
        byte_len,
        nonce,
        ciphertext,
        received_at,
    ): (
        String,
        String,
        String,
        String,
        Option<String>,
        i32,
        Vec<u8>,
        Vec<u8>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT source, payload_format, origin, resolution, unresolved_reason, byte_len, nonce, ciphertext, received_at FROM raw_payload WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    RawPayloadRow {
        source,
        payload_format,
        origin,
        resolution,
        unresolved_reason,
        byte_len,
        nonce,
        ciphertext,
        received_at,
    }
}

async fn raw_payload_count(pool: &PgPool, org_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Criteria 1, 2: a valid delivery stores exactly one correctly-shaped
/// `raw_payload` row whose ciphertext decrypts back to the exact bytes.
#[sqlx::test]
#[ignore]
async fn valid_delivery_stores_one_row_that_decrypts_to_the_exact_bytes(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let before = chrono::Utc::now();
    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, &token),
        PLAIN_EML,
    )
    .await;
    let after = chrono::Utc::now();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body, json!({ "status": "accepted" }));

    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 1);
    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM raw_payload WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    let row = raw_payload_row(&migrator_pool, id).await;
    assert_eq!(row.source, "email");
    assert_eq!(row.payload_format, "rfc822_v1");
    assert_eq!(row.origin, "webhook");
    assert_eq!(row.resolution, "unresolved");
    assert_eq!(row.unresolved_reason.as_deref(), Some("email_unparsed"));
    assert_eq!(row.byte_len as usize, PLAIN_EML.len());
    assert!(
        row.received_at >= before && row.received_at <= after,
        "received_at ({}) must be the receipt time, within [{before}, {after}]",
        row.received_at
    );

    let opened = crypto::open(
        &test_config().raw_payload_key,
        org_id,
        id,
        &row.nonce,
        &row.ciphertext,
    )
    .unwrap();
    assert_eq!(opened, PLAIN_EML);
}

/// Criterion 3: no Person, Inquiry, fact, or routing row is created.
#[sqlx::test]
#[ignore]
async fn valid_delivery_creates_nothing_beyond_the_raw_payload_row(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, &token),
        MULTIPART_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    for table in ["person", "inquiry", "inquiry_received", "routing_decision"] {
        let (count,): (i64,) = sqlx::query_as(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id = $1"
        ))
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "{table} must stay empty");
    }
}

/// Criterion 4: a byte-identical re-delivery stores nothing new and
/// publishes nothing new.
#[sqlx::test]
#[ignore]
async fn byte_identical_redelivery_is_a_noop(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let addr = recipient(&slug, &token);

    let first =
        post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), &addr, PLAIN_EML).await;
    assert_eq!(first.status(), StatusCode::OK);
    let second =
        post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), &addr, PLAIN_EML).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(second).await,
        json!({ "status": "accepted" })
    );

    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 1);
    assert_eq!(recorded(&publisher).await.len(), 1, "exactly one publish");
}

/// Criterion 5: the same raw bytes to two different orgs' addresses store
/// one row per org.
#[sqlx::test]
#[ignore]
async fn same_bytes_to_two_orgs_store_one_row_each(migrator_pool: PgPool) {
    let org_a = common::create_org(&migrator_pool, "Acme Realty").await;
    let org_b = common::create_org(&migrator_pool, "Best Realty").await;
    let (slug_a, token_a) = intake_row(&migrator_pool, org_a).await;
    let (slug_b, token_b) = intake_row(&migrator_pool, org_b).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp_a = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug_a, &token_a),
        PLAIN_EML,
    )
    .await;
    let resp_b = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug_b, &token_b),
        PLAIN_EML,
    )
    .await;
    assert_eq!(resp_a.status(), StatusCode::OK);
    assert_eq!(resp_b.status(), StatusCode::OK);
    assert_eq!(raw_payload_count(&migrator_pool, org_a).await, 1);
    assert_eq!(raw_payload_count(&migrator_pool, org_b).await, 1);
}

/// Criteria 6, 7: org A's slug with a wrong token, an unknown slug, and a
/// syntactically invalid recipient all produce the byte-identical 200
/// rejected response with nothing stored anywhere.
#[sqlx::test]
#[ignore]
async fn every_rejection_shape_is_byte_identical_and_stores_nothing(migrator_pool: PgPool) {
    let org_a = common::create_org(&migrator_pool, "Acme Realty").await;
    let org_b = common::create_org(&migrator_pool, "Best Realty").await;
    let (slug_a, _token_a) = intake_row(&migrator_pool, org_a).await;
    let (_slug_b, token_b) = intake_row(&migrator_pool, org_b).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let cases = [
        recipient(&slug_a, &token_b), // wrong token for org A's slug
        recipient("no-such-org-slug", "abcdefgh"), // unknown slug
        "not-an-email-address".to_string(), // syntactically invalid
    ];
    let mut bodies = Vec::new();
    for case in &cases {
        let resp =
            post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), case, PLAIN_EML).await;
        assert_eq!(resp.status(), StatusCode::OK, "{case}");
        bodies.push(common::body_json(resp).await);
    }
    for body in &bodies {
        assert_eq!(body, &json!({ "status": "rejected" }));
    }

    assert_eq!(raw_payload_count(&migrator_pool, org_a).await, 0);
    assert_eq!(raw_payload_count(&migrator_pool, org_b).await, 0);
}

/// Criterion 8: missing/wrong bearer, and an unset secret, all 401
/// `unauthenticated` with nothing stored.
#[sqlx::test]
#[ignore]
async fn bad_or_missing_or_disabled_bearer_is_401_and_stores_nothing(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let addr = recipient(&slug, &token);

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    for bearer in [None, Some("wrong-secret-value-that-is-long-enough")] {
        let resp = post_inbound_email(&router, bearer, &addr, PLAIN_EML).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{bearer:?}");
        assert_eq!(
            common::body_json(resp).await,
            json!({ "error": "unauthenticated" })
        );
    }

    let disabled_router = build_router_no_secret(&migrator_pool, Publisher::recording()).await;
    let resp = post_inbound_email(&disabled_router, Some("anything"), &addr, PLAIN_EML).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 0);
}

/// Criterion 9 (400 half): malformed JSON, invalid base64, and an empty
/// `raw` after decode are all `malformed_request`.
#[sqlx::test]
#[ignore]
async fn malformed_json_base64_and_empty_raw_are_400(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let addr = recipient(&slug, &token);
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let not_json = "not json at all";
    let resp =
        post_inbound_email_raw_body(&router, Some(TEST_INBOUND_EMAIL_SECRET), not_json).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "malformed_request" })
    );

    let bad_base64 = json!({ "recipient": addr, "raw": "not-valid-base64!!" }).to_string();
    let resp =
        post_inbound_email_raw_body(&router, Some(TEST_INBOUND_EMAIL_SECRET), &bad_base64).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "malformed_request" })
    );

    let empty_raw = json!({ "recipient": addr, "raw": "" }).to_string();
    let resp =
        post_inbound_email_raw_body(&router, Some(TEST_INBOUND_EMAIL_SECRET), &empty_raw).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "malformed_request" })
    );

    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 0);
}

/// Criterion 11: first storage publishes exactly one ids-only
/// `intake.unresolved_changed` on `org:<id>`.
#[sqlx::test]
#[ignore]
async fn first_storage_publishes_exactly_one_event(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;

    let before = chrono::Utc::now();
    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, &token),
        PLAIN_EML,
    )
    .await;
    let after = chrono::Utc::now();
    assert_eq!(resp.status(), StatusCode::OK);

    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    let (channel, data) = &events[0];
    assert_eq!(channel, &format!("org:{org_id}"));
    assert_eq!(data["type"], "intake.unresolved_changed");
    assert_eq!(data["v"], 1);
    assert_eq!(data["organization_id"], org_id.to_string());

    // Criterion 11: occurred_at is the receipt time (not publish time), and
    // correlation_id is a fresh, non-nil id.
    let occurred_at: chrono::DateTime<chrono::Utc> =
        data["occurred_at"].as_str().unwrap().parse().unwrap();
    assert!(
        occurred_at >= before && occurred_at <= after,
        "occurred_at ({occurred_at}) must be the receipt time, within [{before}, {after}]"
    );
    let correlation_id: Uuid = data["correlation_id"].as_str().unwrap().parse().unwrap();
    assert_ne!(correlation_id, Uuid::nil());
}

/// Criterion 12: the stored row shows up in `GET /api/intake/unresolved`
/// with reason `email_unparsed`, visible only to org A.
#[sqlx::test]
#[ignore]
async fn stored_row_appears_in_unresolved_for_its_own_org_only(migrator_pool: PgPool) {
    let (org_a, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (_org_b, _bob_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        "pw",
    )
    .await;
    let (slug, token) = intake_row(&migrator_pool, org_a).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, &token),
        PLAIN_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let resp = common::get_with_cookie(&router, "/api/intake/unresolved", &alice_cookie).await;
    let body = common::body_json(resp).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source"], "email");
    assert_eq!(items[0]["reason"], "email_unparsed");

    let bob_cookie = common::login_cookie(&router, "bob@best.test", "pw").await;
    let resp = common::get_with_cookie(&router, "/api/intake/unresolved", &bob_cookie).await;
    let body = common::body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

/// Criterion 13: no span or log line, across accepted / rejected /
/// malformed / bad-bearer requests, ever carries the recipient, slug,
/// token, bearer, or raw/base64 content.
#[sqlx::test]
#[ignore]
async fn no_span_or_log_line_ever_carries_secret_or_content(migrator_pool: PgPool) {
    use std::sync::{Arc, Mutex};
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
    // Set up the org and router *before* installing the capturing
    // subscriber: `create_org`'s own admin-creation span legitimately
    // carries the slug (docs/specs/SLICE_007a.md §9's own span does this
    // by design), so capturing only starts at the calls this criterion
    // actually covers.
    // A name unique to this test (not "Acme Realty", which every other test
    // in this file also creates): `cargo test` runs tests in this binary
    // concurrently, and the global subscriber installed below captures
    // every thread's tracing output, not just this test's — a same-named
    // concurrent org's own (legitimate, per SLICE_007a.md §9) slug-bearing
    // creation span would otherwise produce a false "leaked" failure.
    let org_id = common::create_org(&migrator_pool, "Tracing Capture Test Org").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let addr = recipient(&slug, &token);
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    // accepted
    let _ = post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), &addr, PLAIN_EML).await;
    // rejected (wrong token, still contains the real slug and a bogus token)
    let _ = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, "zzzzzzzz"),
        PLAIN_EML,
    )
    .await;
    // malformed (invalid base64) — the leak-prone path: base64's Display text
    let _ = post_inbound_email_raw_body(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &json!({ "recipient": addr, "raw": "not-valid-base64!!" }).to_string(),
    )
    .await;
    // bad bearer
    let _ = post_inbound_email(
        &router,
        Some("a-completely-wrong-bearer-value"),
        &addr,
        PLAIN_EML,
    )
    .await;

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("intake.inbound_email"), "span present");
    for secret in [
        &slug,
        &token,
        "zzzzzzzz",
        TEST_INBOUND_EMAIL_SECRET,
        "a-completely-wrong-bearer-value",
        "not-valid-base64!!",
    ] {
        assert!(!captured.contains(secret), "leaked: {secret}");
    }
    assert!(
        !captured.contains("Interested in the downtown listing"),
        "raw email content must never appear in a span/log line"
    );
}

/// Criterion 14: a delivery that finds an existing `pending` row for the
/// same bytes (the crash-between-two-commits window) transitions it to
/// unresolved and publishes exactly once.
#[sqlx::test]
#[ignore]
async fn stuck_pending_row_is_rescued_and_published_once(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, PLAIN_EML);
    let sealed = crypto::seal(&key, org_id, stuck_id, PLAIN_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(stuck_id)
    .bind(org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(PLAIN_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, &token),
        PLAIN_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(
        raw_payload_count(&migrator_pool, org_id).await,
        1,
        "rescued, not duplicated"
    );
    let row = raw_payload_row(&migrator_pool, stuck_id).await;
    assert_eq!(row.resolution, "unresolved");
    assert_eq!(row.unresolved_reason.as_deref(), Some("email_unparsed"));
    assert_eq!(recorded(&publisher).await.len(), 1, "exactly one publish");
}

/// Criterion 19: two concurrent identical POSTs collapse to exactly one
/// row and exactly one publish (the `ON CONFLICT` + `WHERE
/// resolution='pending'` re-check under READ COMMITTED).
#[sqlx::test]
#[ignore]
async fn concurrent_identical_posts_yield_one_row_and_one_publish(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let addr = recipient(&slug, &token);

    let fut_a = post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), &addr, PLAIN_EML);
    let fut_b = post_inbound_email(&router, Some(TEST_INBOUND_EMAIL_SECRET), &addr, PLAIN_EML);
    let (resp_a, resp_b) = tokio::join!(fut_a, fut_b);
    assert_eq!(resp_a.status(), StatusCode::OK);
    assert_eq!(resp_b.status(), StatusCode::OK);

    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 1);
    assert_eq!(recorded(&publisher).await.len(), 1);
}

/// Criterion 20: an org whose token was backfilled outside the mint
/// alphabet (hex, e.g. from a pre-007a org) accepts mail end-to-end —
/// the lookup + constant-time compare path is proven for legacy tokens
/// too, not only freshly minted `[a-z2-7]` ones.
#[sqlx::test]
#[ignore]
async fn legacy_hex_token_org_accepts_mail(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let (slug, _token) = intake_row(&migrator_pool, org_id).await;
    let legacy_token = "9f86d081"; // hex, outside [a-z2-7], inside [a-z0-9]
    sqlx::query("UPDATE organization SET intake_token = $1 WHERE id = $2")
        .bind(legacy_token)
        .bind(org_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let resp = post_inbound_email(
        &router,
        Some(TEST_INBOUND_EMAIL_SECRET),
        &recipient(&slug, legacy_token),
        PLAIN_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );
    assert_eq!(raw_payload_count(&migrator_pool, org_id).await, 1);
}

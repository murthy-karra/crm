//! DB-backed tests for Slice 007e (docs/specs/SLICE_007e.md §10-§11):
//! the Unresolved workbench — admin-only detail, Try again, Discard.
//! Run only via ./scripts/check-db.
mod common;

use std::time::Duration;

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
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
use crm_api::domain::raw_payload::crypto;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const CYPRESS_EML: &[u8] = include_bytes!("fixtures/email/cypress_bay_contact.eml");
const PLAIN_EML: &[u8] = include_bytes!("fixtures/email/plain.eml");
const GARBAGE_EML: &[u8] = include_bytes!("fixtures/email/garbage.eml");

const PW: &str = "pw";
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

async fn post_empty_with_cookie(
    router: &Router,
    uri: &str,
    cookie: &str,
) -> axum::response::Response {
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

/// An org with seeded stages, alice as admin, bob as active member set
/// as the intake default assignee, and carol as a plain member.
struct Fixture {
    org_id: Uuid,
    bob: Uuid,
}

async fn org_fixture(migrator_pool: &PgPool, name: &str) -> Fixture {
    let org_id = common::create_org(migrator_pool, name).await;
    common::seed_stages(migrator_pool, org_id).await;
    let slug: String = name.to_lowercase().replace(' ', "");
    let alice =
        common::create_user(migrator_pool, &format!("alice@{slug}.test"), "Alice", PW).await;
    let bob = common::create_user(migrator_pool, &format!("bob@{slug}.test"), "Bob", PW).await;
    let carol =
        common::create_user(migrator_pool, &format!("carol@{slug}.test"), "Carol", PW).await;
    common::add_membership_with(
        migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership(migrator_pool, org_id, bob).await;
    common::add_membership(migrator_pool, org_id, carol).await;
    admin_queries::update_intake_default_assignee(
        &mut migrator_pool.acquire().await.unwrap(),
        org_id,
        Some(bob),
    )
    .await
    .unwrap();
    Fixture { org_id, bob }
}

/// Delivers `raw` and returns the stored raw_payload id.
async fn deliver(router: &Router, pool: &PgPool, org_id: Uuid, raw: &[u8]) -> Uuid {
    let (slug, token) = intake_row(pool, org_id).await;
    let resp = post_inbound_email(router, &recipient(&slug, &token), raw).await;
    assert_eq!(resp.status(), StatusCode::OK);
    sqlx::query_scalar(
        "SELECT id FROM raw_payload WHERE organization_id = $1 ORDER BY received_at DESC LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn row_state(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT resolution, unresolved_reason FROM raw_payload WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Criterion 1: migration shape — CHECK accepts discarded (with both
/// fields), rejects mismatched attribution and unknown values; crm_app
/// can write the two new columns and still cannot touch ciphertext.
#[sqlx::test]
#[ignore]
async fn migration_check_and_grants_hold(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;

    // discarded without attribution → pair-CHECK rejects.
    let err = sqlx::query("UPDATE raw_payload SET resolution = 'discarded' WHERE id = $1")
        .bind(id)
        .execute(&app_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("raw_payload_discard_fields_check"));

    // attribution without discarded → pair-CHECK rejects.
    let err = sqlx::query("UPDATE raw_payload SET discarded_at = now() WHERE id = $1")
        .bind(id)
        .execute(&app_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("raw_payload_discard_fields_check"));

    // Unknown resolution still rejected.
    let err = sqlx::query("UPDATE raw_payload SET resolution = 'bogus' WHERE id = $1")
        .bind(id)
        .execute(&app_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("raw_payload_resolution_check"));

    // The full discarded write works as crm_app…
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

    // …and ciphertext/nonce/content_hmac stay untouchable.
    for column in ["ciphertext", "nonce", "content_hmac"] {
        let err = sqlx::query(&format!(
            "UPDATE raw_payload SET {column} = '\\x00' WHERE id = $1"
        ))
        .bind(id)
        .execute(&app_pool)
        .await
        .unwrap_err();
        assert!(err.to_string().contains("permission denied"), "{column}");
    }
}

/// Criteria 2, 3, 4: detail content per format and per viewer; the
/// byte-identical 404 set; pending rows viewable.
#[sqlx::test]
#[ignore]
async fn detail_content_authorization_and_404s(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let other = org_fixture(&migrator_pool, "Best Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;

    // An unresolved email row (plain.eml = unknown format).
    let email_id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    // A garbage row (email_unparsed → text fallback).
    let garbage_id = deliver(&router, &migrator_pool, f.org_id, GARBAGE_EML).await;
    // A generic_v1 unresolved row via /api/inquiries (no contact method).
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = common::post_json_with_cookie(
        &router,
        "/api/inquiries",
        &alice_cookie,
        json!({ "source": "website", "payload": { "first_name": "NoContact" } }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let generic_id: Uuid = common::body_json(resp).await["raw_payload_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Email detail: subject/from/date/body from the fixture.
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{email_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["payload_format"], "rfc822_v1");
    assert_eq!(body["content"]["kind"], "email");
    assert_eq!(
        body["content"]["subject"],
        "Interested in the downtown listing"
    );
    assert_eq!(body["content"]["from_addr"], "jordan.rivera@example.com");
    assert_eq!(body["content"]["from_display"], "Jordan Rivera");
    assert!(body["content"]["date"].as_str().is_some());
    assert!(body["content"]["text"]
        .as_str()
        .unwrap()
        .contains("123 Main St"));
    assert_eq!(body["content"]["truncated"], false);

    // Garbage: text fallback.
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{garbage_id}"),
        &alice_cookie,
    )
    .await;
    let body = common::body_json(resp).await;
    assert_eq!(body["content"]["kind"], "text");
    assert!(body["content"]["text"]
        .as_str()
        .unwrap()
        .contains("hello world"));

    // Generic: pretty-printed JSON.
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{generic_id}"),
        &alice_cookie,
    )
    .await;
    let body = common::body_json(resp).await;
    assert_eq!(body["content"]["kind"], "text");
    let text = body["content"]["text"].as_str().unwrap();
    assert!(text.contains("\"first_name\": \"NoContact\""), "{text}");

    // Member (bob) → 403 on all three endpoints; list stays visible.
    let bob_cookie = common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{email_id}"),
        &bob_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{email_id}/retry"),
        &bob_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{email_id}/discard"),
        &bob_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let resp = common::get_with_cookie(&router, "/api/intake/unresolved", &bob_cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 404 set, byte-identical: unknown, cross-org, resolved, discarded.
    let cypress_id = deliver(&router, &migrator_pool, f.org_id, CYPRESS_EML).await; // resolves
    let (resolution, _) = row_state(&migrator_pool, cypress_id).await;
    assert_eq!(resolution, "resolved");
    let discarded_id = deliver(&router, &migrator_pool, other.org_id, PLAIN_EML).await;
    let other_admin = common::login_cookie(&router, "alice@bestrealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{discarded_id}/discard"),
        &other_admin,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let mut bodies = Vec::new();
    for id in [
        Uuid::new_v4(), // unknown
        discarded_id,   // cross-org (org B's row, and discarded)
        cypress_id,     // resolved
    ] {
        let resp = common::get_with_cookie(
            &router,
            &format!("/api/intake/unresolved/{id}"),
            &alice_cookie,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{id}");
        bodies.push(common::body_json(resp).await);
    }
    // Org B's own admin also 404s on its now-discarded row.
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{discarded_id}"),
        &other_admin,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    bodies.push(common::body_json(resp).await);
    // Cross-org retry and discard probes 404 byte-identically too (the
    // §5 promise covers all three endpoints): org A's admin against org
    // B's row.
    for action in ["retry", "discard"] {
        let resp = post_empty_with_cookie(
            &router,
            &format!("/api/intake/unresolved/{discarded_id}/{action}"),
            &alice_cookie,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{action}");
        bodies.push(common::body_json(resp).await);
    }
    for body in &bodies {
        assert_eq!(body, &json!({ "error": "not_found" }));
    }

    // Non-UUID id → 400 malformed_request.
    let resp =
        common::get_with_cookie(&router, "/api/intake/unresolved/not-a-uuid", &alice_cookie).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Criterion 2 (pending + truncation): a pending row is viewable, and a
/// >64 KiB text field is capped with `truncated: true`.
#[sqlx::test]
#[ignore]
async fn pending_rows_are_viewable_and_large_bodies_truncate(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    // A stuck-pending row with a large plain-text email body.
    let big_body = "line of filler text\n".repeat(5000); // ~100 KiB
    let raw = format!(
        "From: someone@example.com\r\nSubject: big\r\nContent-Type: text/plain\r\n\r\n{big_body}"
    );
    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, raw.as_bytes());
    let sealed = crypto::seal(&key, f.org_id, stuck_id, raw.as_bytes()).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(stuck_id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(raw.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{stuck_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "pending rows are viewable");
    let body = common::body_json(resp).await;
    assert_eq!(body["resolution"], "pending");
    assert_eq!(body["content"]["truncated"], true);
    let text = body["content"]["text"].as_str().unwrap();
    assert!(text.len() <= 64 * 1024);
}

/// Criteria 5: retry rescues a stuck-pending in-format row end-to-end —
/// System actor, on_behalf_of = the admin, origin web_session, routing
/// per D-035 to bob (not alice, who clicked).
#[sqlx::test]
#[ignore]
async fn retry_rescues_stuck_pending_row_with_system_actor_on_behalf_of_admin(
    migrator_pool: PgPool,
) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, stuck_id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(stuck_id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let alice: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE email = 'alice@acmerealty.test'")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{stuck_id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["routing_strategy"], "organization_default");
    assert_eq!(body["assigned_user_id"], f.bob.to_string());
    assert_eq!(body["duplicate"], false);
    let person_id: Uuid = body["person_id"].as_str().unwrap().parse().unwrap();

    // Facts: System actor, on_behalf_of = alice, origin web_session, one
    // shared correlation id.
    for table in [
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
    ] {
        let (actor_kind, actor_user_id, on_behalf, origin): (
            String,
            Option<Uuid>,
            Option<Uuid>,
            String,
        ) = sqlx::query_as(&format!(
            "SELECT actor_kind, actor_user_id, on_behalf_of_user_id, origin FROM {table} WHERE person_id = $1"
        ))
        .bind(person_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(actor_kind, "system", "{table}");
        assert_eq!(actor_user_id, None, "{table}");
        assert_eq!(on_behalf, Some(alice), "{table}");
        assert_eq!(origin, "web_session", "{table}");
    }

    // The row is resolved; exactly one person_changed publish.
    let (resolution, _) = row_state(&migrator_pool, stuck_id).await;
    assert_eq!(resolution, "resolved");
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["type"], "person.changed");

    // On bob's Today, not alice's.
    let bob_cookie = common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let bob_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &bob_cookie).await).await;
    assert_eq!(bob_today["items"].as_array().unwrap().len(), 1);
    let alice_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(alice_today["items"].as_array().unwrap().len(), 0);
}

/// Criterion 6: retry on bytes that still don't parse — same reason
/// re-recorded, resolved_at updated, one intake_unresolved_changed.
#[sqlx::test]
#[ignore]
async fn retry_on_still_unparseable_bytes_rerecords_the_reason(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    let (_, reason_before) = row_state(&migrator_pool, id).await;
    assert_eq!(reason_before.as_deref(), Some("email_unrecognized_format"));
    let (resolved_at_before,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT resolved_at FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let events_before = recorded(&publisher).await.len();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "unresolved");
    assert_eq!(body["reason"], "email_unrecognized_format");
    assert_eq!(body["duplicate"], false);

    let (resolution, reason) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    let (resolved_at_after,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT resolved_at FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(resolved_at_after.unwrap() > resolved_at_before.unwrap());
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), events_before + 1);
    assert_eq!(
        events.last().unwrap().1["type"],
        "intake.unresolved_changed"
    );
}

/// Criterion 7: a generic_v1 retry re-runs the JSON parse with the row's
/// stored source. Made resolvable by adding the missing member first —
/// here, the payload is retried after nothing changed, so it stays
/// unresolved, pinning that the generic path re-runs and reuses the
/// stored source in the reason flow.
#[sqlx::test]
#[ignore]
async fn generic_v1_retry_reruns_with_the_stored_source(migrator_pool: PgPool) {
    let _f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;

    let resp = common::post_json_with_cookie(
        &router,
        "/api/inquiries",
        &alice_cookie,
        json!({ "source": "zillow_import", "payload": { "first_name": "NoContact" } }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let id: Uuid = common::body_json(resp).await["raw_payload_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "unresolved");
    assert_eq!(body["reason"], "no_contact_method");

    // The row's source column survives the retry unchanged.
    let (source,): (String,) = sqlx::query_as("SELECT source FROM raw_payload WHERE id = $1")
        .bind(id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(source, "zillow_import");
}

/// Criterion 8: two concurrent retries → one Person, one publish; the
/// loser gets the stored outcome duplicate: true.
#[sqlx::test]
#[ignore]
async fn concurrent_retries_yield_one_person_and_a_duplicate_loser(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, stuck_id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(stuck_id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let uri = format!("/api/intake/unresolved/{stuck_id}/retry");
    let (a, b) = tokio::join!(
        post_empty_with_cookie(&router, &uri, &alice_cookie),
        post_empty_with_cookie(&router, &uri, &alice_cookie)
    );
    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);
    let (body_a, body_b) = (common::body_json(a).await, common::body_json(b).await);
    let duplicates = [&body_a, &body_b]
        .iter()
        .filter(|b| b["duplicate"] == true)
        .count();
    // At least one saw the fresh resolution; a loser (if serialized after
    // the winner's commit) reports duplicate: true. Both racing to fresh
    // completion is impossible (row lock).
    assert!(duplicates <= 1);
    for body in [&body_a, &body_b] {
        assert_eq!(body["status"], "resolved");
    }

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(f.org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 1);
    let publishes = recorded(&publisher)
        .await
        .iter()
        .filter(|(_, d)| d["type"] == "person.changed")
        .count();
    assert_eq!(publishes, 1);
}

/// Criterion 9: a retry racing a byte-identical redelivery — one Person,
/// one Inquiry.
#[sqlx::test]
#[ignore]
async fn retry_racing_redelivery_yields_one_person(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, stuck_id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(stuck_id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let (slug, token) = intake_row(&migrator_pool, f.org_id).await;
    let addr = recipient(&slug, &token);
    let uri = format!("/api/intake/unresolved/{stuck_id}/retry");
    let (retry, redelivery) = tokio::join!(
        post_empty_with_cookie(&router, &uri, &alice_cookie),
        post_inbound_email(&router, &addr, CYPRESS_EML)
    );
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(redelivery.status(), StatusCode::OK);

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(f.org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 1);
    let (inquiry_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry WHERE organization_id = $1")
            .bind(f.org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(inquiry_count, 1);
}

/// Criterion 10: retry with the org lock held past budget → 503
/// intake_busy + Retry-After, row pending, no Person.
#[sqlx::test]
#[ignore]
async fn retry_surfaces_intake_busy_when_lock_held(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    // A stuck-pending in-format row (parse succeeds, so the retry
    // genuinely attempts the advisory lock).
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;
    let id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let hold_duration = ADVISORY_LOCK_BUDGET + Duration::from_secs(4);
    let external_pool = migrator_pool.clone();
    let org_text = f.org_id.to_string();
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

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(resp.headers().get("retry-after").is_some());
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "intake_busy" })
    );

    let (resolution, reason) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "pending", "reset survived; reason cleared");
    assert_eq!(reason, None);
    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(f.org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 0);
    hold.await.unwrap();
}

/// Adversarial finding 1: a failed retry has already committed the
/// reset-to-pending, so it must publish one ids-only queue invalidation
/// — otherwise every connected client keeps showing the stale
/// "Unresolved / <reason>" row.
#[sqlx::test]
#[ignore]
async fn failed_retry_publishes_a_queue_invalidation_for_the_reset(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let external_pool = migrator_pool.clone();
    let org_text = f.org_id.to_string();
    let hold_duration = ADVISORY_LOCK_BUDGET + Duration::from_secs(4);
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

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1, "one queue invalidation for the reset");
    assert_eq!(events[0].1["type"], "intake.unresolved_changed");
    hold.await.unwrap();
}

/// Adversarial finding 2: a retry that can never succeed (unknown
/// payload_format) fails closed BEFORE mutating — the stored reason
/// survives.
#[sqlx::test]
#[ignore]
async fn unretryable_format_fails_closed_without_destroying_the_reason(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, GARBAGE_EML);
    let sealed = crypto::seal(&key, f.org_id, id, GARBAGE_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution,
             resolved_at, unresolved_reason)
           VALUES ($1, $2, 'email', 'bogus_v9', 'webhook', now(), $3, $4, $5, $6,
                   'unresolved', now(), 'email_unparsed')"#,
    )
    .bind(id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(GARBAGE_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let (resolution, reason) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "unresolved", "not reset");
    assert_eq!(reason.as_deref(), Some("email_unparsed"), "reason kept");
}

/// Criterion 8's deterministic half + the §5 "resolved row → 200
/// duplicate, not 404" contract: retrying an already-resolved row
/// returns the stored outcome with duplicate: true.
#[sqlx::test]
#[ignore]
async fn retry_on_a_resolved_row_returns_the_stored_outcome_as_duplicate(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let id = deliver(&router, &migrator_pool, f.org_id, CYPRESS_EML).await;
    let (resolution, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "resolved");

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["duplicate"], true);
    assert_eq!(body["routing_strategy"], "organization_default");
    assert_eq!(body["assigned_user_id"], f.bob.to_string());
}

/// Criterion 14: a /api/inquiries replay whose bytes hit a discarded row
/// gets the existing unresolved-duplicate envelope — no resurrection, no
/// publish, no new vocabulary.
#[sqlx::test]
#[ignore]
async fn api_inquiries_replay_of_a_discarded_row_gets_the_duplicate_envelope(
    migrator_pool: PgPool,
) {
    let _f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;

    // A generic no-contact row via the real endpoint.
    let payload = json!({ "source": "website", "payload": { "first_name": "NoContact" } });
    let resp =
        common::post_json_with_cookie(&router, "/api/inquiries", &alice_cookie, payload.clone())
            .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let id: Uuid = common::body_json(resp).await["raw_payload_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let events_before = recorded(&publisher).await.len();

    // Byte-identical replay through the user endpoint.
    let resp =
        common::post_json_with_cookie(&router, "/api/inquiries", &alice_cookie, payload).await;
    assert_eq!(resp.status(), StatusCode::OK, "no panic, duplicate status");
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "unresolved");
    assert_eq!(body["duplicate"], true);

    let (resolution, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "discarded", "stays discarded");
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish"
    );
}

/// Criterion 11: a corrupted-ciphertext row → 500 on retry, row stays
/// pending, still discardable.
#[sqlx::test]
#[ignore]
async fn corrupted_row_retry_500s_and_stays_discardable(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    // Seal against a DIFFERENT id than the row's — AAD mismatch, decrypt
    // fails forever (the crash-window corruption shape).
    let row_id = Uuid::new_v4();
    let wrong_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, wrong_id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(row_id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    // Detail also 500s (decrypt-on-demand).
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{row_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{row_id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let (resolution, _) = row_state(&migrator_pool, row_id).await;
    assert_eq!(resolution, "pending");

    // Discard is the remedy.
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{row_id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, _) = row_state(&migrator_pool, row_id).await;
    assert_eq!(resolution, "discarded");
}

/// Criterion 12: discard mechanics — attribution, queue removal, one
/// publish, idempotency with first-writer attribution, the 409s.
#[sqlx::test]
#[ignore]
async fn discard_attributes_removes_and_is_idempotent(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    let events_before = recorded(&publisher).await.len();

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let alice: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE email = 'alice@acmerealty.test'")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "discarded" })
    );

    let (resolution, by, at): (String, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT resolution, discarded_by_user_id, discarded_at FROM raw_payload WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(resolution, "discarded");
    assert_eq!(by, Some(alice));
    assert!(at.is_some());

    // Gone from the member queue.
    let bob_cookie = common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let queue = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &bob_cookie).await,
    )
    .await;
    assert_eq!(queue["items"].as_array().unwrap().len(), 0);

    // Exactly one publish for the discard.
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), events_before + 1);
    assert_eq!(
        events.last().unwrap().1["type"],
        "intake.unresolved_changed"
    );

    // Repeat discard by a DIFFERENT admin: 200, attribution unchanged.
    let dora = common::create_user(&migrator_pool, "dora@acmerealty.test", "Dora", PW).await;
    common::add_membership_with(
        &migrator_pool,
        f.org_id,
        dora,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let dora_cookie = common::login_cookie(&router, "dora@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/discard"),
        &dora_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (by_after,): (Option<Uuid>,) =
        sqlx::query_as("SELECT discarded_by_user_id FROM raw_payload WHERE id = $1")
            .bind(id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(by_after, Some(alice), "first writer wins");
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before + 1,
        "no second publish"
    );

    // Retry on discarded → 409 discarded.
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "discarded" })
    );

    // Discard on resolved → 409 already_resolved.
    let resolved_id = deliver(&router, &migrator_pool, f.org_id, CYPRESS_EML).await;
    let (resolution, _) = row_state(&migrator_pool, resolved_id).await;
    assert_eq!(resolution, "resolved");
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{resolved_id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "error": "already_resolved" })
    );
}

/// Criteria 13, 14: redelivery of a discarded row's bytes — 200 accepted,
/// stays discarded, no rows, no publish, no panic; the /api/inquiries
/// replay shape.
#[sqlx::test]
#[ignore]
async fn discarded_bytes_never_resurrect_on_redelivery_or_replay(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    // A stuck-pending in-format row, discarded before it ever completed
    // — the strongest resurrection bait: its bytes WOULD create a Person
    // if reprocessed.
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;
    let id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(&key, f.org_id, id, CYPRESS_EML).unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(id)
    .bind(f.org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let events_before = recorded(&publisher).await.len();

    // Byte-identical redelivery: accepted, stays discarded, nothing new.
    let (slug, token) = intake_row(&migrator_pool, f.org_id).await;
    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK, "no panic");
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );
    let (resolution, _) = row_state(&migrator_pool, id).await;
    assert_eq!(resolution, "discarded");
    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(f.org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 0);
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish"
    );
}

/// Criterion 15: tracing capture over detail (email + text), a failed
/// retry, and a discard — no content in any span or log line.
#[sqlx::test]
#[ignore]
async fn workbench_spans_and_logs_carry_no_content(migrator_pool: PgPool) {
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

    // Unique org name (global-subscriber caveat — see
    // db_inbound_email.rs's capture test).
    let f = org_fixture(&migrator_pool, "Workbench Capture Org").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    let alice_cookie = common::login_cookie(&router, "alice@workbenchcaptureorg.test", PW).await;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    // Detail (email content), failed retry, discard.
    let _ = common::get_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}"),
        &alice_cookie,
    )
    .await;
    let _ = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/retry"),
        &alice_cookie,
    )
    .await;
    let _ = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{id}/discard"),
        &alice_cookie,
    )
    .await;

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("intake.unresolved_detail")
            && captured.contains("intake.retry")
            && captured.contains("intake.discard"),
        "spans present"
    );
    for leak in [
        "Interested in the downtown listing", // subject
        "jordan.rivera@example.com",          // sender
        "123 Main St",                        // body
        "Jordan Rivera",                      // display name
    ] {
        assert!(!captured.contains(leak), "leaked: {leak}");
    }
}

/// The queue filter change: pending and unresolved list; discarded does
/// not (SLICE_002 §5 declared amendment).
#[sqlx::test]
#[ignore]
async fn queue_lists_pending_and_unresolved_never_discarded(migrator_pool: PgPool) {
    let f = org_fixture(&migrator_pool, "Acme Realty").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let unresolved_id = deliver(&router, &migrator_pool, f.org_id, PLAIN_EML).await;
    let discarded_id = deliver(&router, &migrator_pool, f.org_id, GARBAGE_EML).await;

    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let resp = post_empty_with_cookie(
        &router,
        &format!("/api/intake/unresolved/{discarded_id}/discard"),
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let queue = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &alice_cookie).await,
    )
    .await;
    let items = queue["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], unresolved_id.to_string());
}

//! DB-backed tests for Slice 007d (docs/specs/SLICE_007d.md §10): the
//! inbound-email completion path — a pinned-format email becoming a real
//! Person/Inquiry through `complete_intake` as the System actor. Run only
//! via ./scripts/check-db.
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
use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
use crm_api::domain::raw_payload::crypto;
use crm_api::ids::{OrganizationId, RawPayloadId, UserId};
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const CYPRESS_EML: &[u8] = include_bytes!("fixtures/email/cypress_bay_contact.eml");
const CYPRESS_HTML_EML: &[u8] = include_bytes!("fixtures/email/cypress_bay_contact_html_only.eml");
const CYPRESS_NO_CONTACT_EML: &[u8] =
    include_bytes!("fixtures/email/cypress_bay_contact_no_contact.eml");
const CYPRESS_FORGED_EML: &[u8] = include_bytes!("fixtures/email/cypress_bay_forged_sender.eml");
const GARBAGE_EML: &[u8] = include_bytes!("fixtures/email/garbage.eml");
const PLAIN_EML: &[u8] = include_bytes!("fixtures/email/plain.eml");

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

/// An org with seeded stages, an active member `bob@…` set as the intake
/// default assignee, and a second active member `alice@…`.
async fn org_with_default(migrator_pool: &PgPool, name: &str) -> (Uuid, Uuid, Uuid) {
    let org_id = common::create_org(migrator_pool, name).await;
    common::seed_stages(migrator_pool, org_id).await;
    let slug: String = name.to_lowercase().replace(' ', "");
    let bob = common::create_user(migrator_pool, &format!("bob@{slug}.test"), "Bob", PW).await;
    let alice =
        common::create_user(migrator_pool, &format!("alice@{slug}.test"), "Alice", PW).await;
    common::add_membership(migrator_pool, org_id, bob).await;
    common::add_membership(migrator_pool, org_id, alice).await;
    // Slice 008 (D-041): mode dispatch replaced the old implicit
    // "assignee configured => organization_default" behavior — set
    // `default_assignee` mode alongside the assignee so this fixture's
    // downstream `organization_default` assertions stay byte-identical.
    admin_queries::update_intake_routing_settings(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
        crm_api::domain::intake::IntakeRoutingMode::DefaultAssignee,
        Some(UserId::new(bob)),
    )
    .await
    .unwrap();
    (org_id, bob, alice)
}

async fn count(pool: &PgPool, table: &str, org_id: Uuid) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {table} WHERE organization_id = $1"
    ))
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Criteria 1–5: the canonical fixture completes end-to-end — resolved
/// raw_payload keeping `source='email'`, one Person with normalized
/// contact methods, an Inquiry `source='website'` at receipt time (not
/// the Date header), all facts System/webhook with one correlation id,
/// routing to the default's Today, exactly one `person_changed` publish.
#[sqlx::test]
#[ignore]
async fn cypress_fixture_completes_intake_end_to_end(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;

    let before = chrono::Utc::now();
    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    let after = chrono::Utc::now();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" }),
        "the envelope reveals nothing about the parse outcome"
    );

    // Criterion 1: the raw_payload row is resolved, transport values kept.
    let (raw_source, payload_format, origin, resolution, inquiry_id): (
        String,
        String,
        String,
        String,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT source, payload_format, origin, resolution, inquiry_id FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(raw_source, "email");
    assert_eq!(payload_format, "rfc822_v1");
    assert_eq!(origin, "webhook");
    assert_eq!(resolution, "resolved");
    let inquiry_id = inquiry_id.expect("inquiry_id set");

    // Criterion 2: one Person, split name, both contact methods normalized
    // with raw values preserved.
    let (person_id, first_name, last_name): (Uuid, Option<String>, Option<String>) =
        sqlx::query_as("SELECT id, first_name, last_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(first_name.as_deref(), Some("Jordan"));
    assert_eq!(last_name.as_deref(), Some("Ellis"));
    let mut methods: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT kind, normalized_value, value FROM contact_method WHERE person_id = $1",
    )
    .bind(person_id)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    methods.sort();
    assert_eq!(
        methods,
        vec![
            (
                "email".to_string(),
                "jordan.ellis@example.com".to_string(),
                "jordan.ellis@example.com".to_string()
            ),
            (
                "phone".to_string(),
                "+15555550142".to_string(),
                "(555) 555-0142".to_string()
            ),
        ]
    );

    // Criterion 2: the Inquiry carries the detected source and the
    // multi-line Message; received_at is receipt time, not the fixture's
    // 2026-08-25 Date header.
    let (inq_source, message, received_at): (
        String,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as("SELECT source, message, received_at FROM inquiry WHERE id = $1")
        .bind(inquiry_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(inq_source, "website");
    assert_eq!(
        message.as_deref(),
        Some("Interested in the 45 Shoreline Dr listing.\nIs it still available this weekend?")
    );
    assert!(
        received_at >= before && received_at <= after,
        "received_at ({received_at}) must be receipt time, within [{before}, {after}]"
    );

    // Criterion 3: every fact is System/webhook, no user, one shared
    // correlation id; inquiry_received.source is the detected source.
    for table in [
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
    ] {
        let (actor_kind, actor_user_id, fact_origin): (String, Option<Uuid>, String) =
            sqlx::query_as(&format!(
                "SELECT actor_kind, actor_user_id, origin FROM {table} WHERE person_id = $1"
            ))
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
        assert_eq!(actor_kind, "system", "{table}");
        assert_eq!(actor_user_id, None, "{table}");
        assert_eq!(fact_origin, "webhook", "{table}");
    }
    let correlations: Vec<Uuid> = sqlx::query_scalar(
        "SELECT correlation_id FROM inquiry_received WHERE person_id = $1
         UNION SELECT correlation_id FROM routing_decision WHERE person_id = $1
         UNION SELECT correlation_id FROM assignment_changed WHERE person_id = $1
         UNION SELECT correlation_id FROM stage_changed WHERE person_id = $1",
    )
    .bind(person_id)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(correlations.len(), 1, "one shared correlation id");
    let (ir_source,): (String,) =
        sqlx::query_as("SELECT source FROM inquiry_received WHERE inquiry_id = $1")
            .bind(inquiry_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(ir_source, "website");

    // Criterion 4: routed to the default (007c matrix) with the causation
    // chain, and on bob's Today only.
    let (strategy, assignee): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT strategy, assignee_user_id FROM routing_decision WHERE inquiry_id = $1",
    )
    .bind(inquiry_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(strategy, "organization_default");
    assert_eq!(assignee, Some(bob));

    let bob_cookie = common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let bob_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &bob_cookie).await).await;
    let items = bob_today["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["person"]["id"], person_id.to_string());
    let alice_cookie = common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let alice_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(alice_today["items"].as_array().unwrap().len(), 0);

    // Criterion 5: exactly one person_changed publish, ids-only.
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    let (channel, data) = &events[0];
    assert_eq!(channel, &format!("org:{org_id}"));
    assert_eq!(data["type"], "person.changed");
    assert_eq!(data["v"], 1);
    assert_eq!(data["data"]["person_id"], person_id.to_string());
    assert_eq!(data["data"]["change"], "inquiry_received");
}

/// The HTML-only variant exercises the wrapper's HTML→text fallback and
/// still completes.
#[sqlx::test]
#[ignore]
async fn html_only_fixture_completes(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_HTML_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (first_name, assigned): (Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT first_name, assigned_user_id FROM person WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(first_name.as_deref(), Some("Casey"));
    assert_eq!(assigned, Some(bob));
    let (email,): (String,) = sqlx::query_as(
        "SELECT normalized_value FROM contact_method WHERE organization_id = $1 AND kind = 'email'",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(email, "casey.morgan@example.com");
}

/// Criterion 6: a second in-format email from the same lead address with
/// different content becomes a second Inquiry on the same Person
/// (`kept_existing` — the Person is already assigned).
#[sqlx::test]
#[ignore]
async fn second_in_format_email_dedups_onto_the_same_person(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let addr = recipient(&slug, &token);

    let resp = post_inbound_email(&router, &addr, CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let second = String::from_utf8_lossy(CYPRESS_EML)
        .replace("fixture-cypress-001", "fixture-cypress-002")
        .replace("Interested in the 45 Shoreline Dr listing.", "Second note.");
    let resp = post_inbound_email(&router, &addr, second.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(count(&migrator_pool, "person", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 2);
    let strategies: Vec<String> = sqlx::query_scalar(
        "SELECT strategy FROM routing_decision WHERE organization_id = $1 ORDER BY recorded_at",
    )
    .bind(org_id)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(strategies, vec!["organization_default", "kept_existing"]);
    // Still exactly one assignment fact (NULL→bob from the first intake).
    let assignments: Vec<Option<Uuid>> =
        sqlx::query_scalar("SELECT to_user_id FROM assignment_changed WHERE organization_id = $1")
            .bind(org_id)
            .fetch_all(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assignments, vec![Some(bob)]);
}

/// Criterion 7: byte-identical redelivery of a completed (`resolved`) row
/// is an idempotent no-op — same response, zero new rows, zero publishes.
#[sqlx::test]
#[ignore]
async fn byte_identical_redelivery_of_a_resolved_row_is_a_noop(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let addr = recipient(&slug, &token);

    let first = post_inbound_email(&router, &addr, CYPRESS_EML).await;
    assert_eq!(first.status(), StatusCode::OK);
    let second = post_inbound_email(&router, &addr, CYPRESS_EML).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(second).await,
        json!({ "status": "accepted" })
    );

    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "person", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 1);
    assert_eq!(recorded(&publisher).await.len(), 1, "one publish total");
}

/// Criterion 9: the correct template from a non-cypressbayrealty.com
/// sender lands `email_unrecognized_format` and never creates a Person —
/// the D-036 mitigation, pinned.
#[sqlx::test]
#[ignore]
async fn forged_sender_lands_unrecognized_and_creates_no_person(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_FORGED_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (resolution, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT resolution, unresolved_reason FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 0);
}

/// Criterion 10: an in-format email with no normalizable contact method
/// reuses the existing `no_contact_method` reason.
#[sqlx::test]
#[ignore]
async fn in_format_email_without_contact_method_lands_no_contact_method(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_NO_CONTACT_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (resolution, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT resolution, unresolved_reason FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("no_contact_method"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
}

/// Criterion 11: bytes the wrapper refuses (nothing email-shaped) land
/// `email_unparsed` — the path stays reachable despite mail-parser's
/// leniency.
#[sqlx::test]
#[ignore]
async fn garbage_bytes_land_email_unparsed(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GARBAGE_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (resolution, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT resolution, unresolved_reason FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unparsed"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
}

/// Criterion 12 (terminal half): a pre-existing terminal `unresolved` row
/// — e.g. a 007b-era `email_unparsed` row — is never reprocessed by a
/// byte-identical redelivery, even though its bytes are in-format now.
#[sqlx::test]
#[ignore]
async fn terminal_unresolved_row_is_never_reprocessed_by_redelivery(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    // A 007b-era terminal row for the (now in-format) cypress bytes.
    let old_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(
        &key,
        OrganizationId::new(org_id),
        RawPayloadId::new(old_id),
        CYPRESS_EML,
    )
    .unwrap();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution,
             resolved_at, unresolved_reason)
           VALUES ($1, $2, 'email', 'rfc822_v1', 'webhook', now(), $3, $4, $5, $6,
                   'unresolved', now(), 'email_unparsed')"#,
    )
    .bind(old_id)
    .bind(org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(content_hmac.to_vec())
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );

    let (resolution, reason): (String, Option<String>) =
        sqlx::query_as("SELECT resolution, unresolved_reason FROM raw_payload WHERE id = $1")
            .bind(old_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "unresolved", "terminal row untouched");
    assert_eq!(reason.as_deref(), Some("email_unparsed"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
    assert_eq!(recorded(&publisher).await.len(), 0, "no publish");
}

/// Criterion 12 (rescue half): a stuck-`pending` row whose bytes are
/// in-format is fully parsed on redelivery and resolves end-to-end —
/// 007b's mark-only rescue upgraded (SLICE_007d §4d).
#[sqlx::test]
#[ignore]
async fn stuck_pending_in_format_row_resolves_end_to_end_on_redelivery(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let key = test_config().raw_payload_key;

    let stuck_id = Uuid::new_v4();
    let content_hmac = crypto::content_hmac(&key, CYPRESS_EML);
    let sealed = crypto::seal(
        &key,
        OrganizationId::new(org_id),
        RawPayloadId::new(stuck_id),
        CYPRESS_EML,
    )
    .unwrap();
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
    .bind(CYPRESS_EML.len() as i32)
    .execute(&app_pool)
    .await
    .unwrap();

    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (resolution, inquiry_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT resolution, inquiry_id FROM raw_payload WHERE id = $1")
            .bind(stuck_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "resolved");
    assert!(inquiry_id.is_some());
    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 1);
    let (assigned,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assigned_user_id FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assigned, Some(bob));
    assert_eq!(recorded(&publisher).await.len(), 1, "one publish");
}

/// Criterion 8's lock property, inverted: an unknown-format delivery
/// never contends for the advisory lock — it completes to `unresolved`
/// immediately even while the org's intake lock is held externally.
#[sqlx::test]
#[ignore]
async fn unknown_format_mail_completes_even_while_the_lock_is_held(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let external_pool = migrator_pool.clone();
    let org_text = org_id.to_string();
    let hold = tokio::spawn(async move {
        let mut tx = external_pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0))")
            .bind(&org_text)
            .execute(&mut *tx)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        tx.rollback().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let started = std::time::Instant::now();
    let resp = post_inbound_email(&router, &recipient(&slug, &token), PLAIN_EML).await;
    let elapsed = started.elapsed();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        elapsed < Duration::from_secs(1),
        "unknown-format mail must not wait on the lock (took {elapsed:?})"
    );
    let (resolution, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT resolution, unresolved_reason FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    hold.await.unwrap();
}

/// Criterion 13: with the org's lock held past the budget, an in-format
/// delivery returns 200 accepted with the row left `pending` (never
/// `intake_busy`), publishes one queue invalidation, and a later
/// redelivery completes it.
#[sqlx::test]
#[ignore]
async fn lock_held_past_budget_defers_the_row_pending_then_redelivery_completes(
    migrator_pool: PgPool,
) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let addr = recipient(&slug, &token);

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

    let resp = post_inbound_email(&router, &addr, CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK, "never intake_busy");
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );

    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "pending", "deferred, not lost");
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1, "one queue invalidation");
    assert_eq!(events[0].1["type"], "intake.unresolved_changed");

    // Wait out the hold, then redeliver: the stuck-pending rescue path
    // completes the intake.
    hold.await.unwrap();
    let resp = post_inbound_email(&router, &addr, CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "resolved");
    let (assigned,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assigned_user_id FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assigned, Some(bob));
}

/// Criterion 14: two concurrent byte-identical in-format POSTs collapse
/// to exactly one Person, one Inquiry, one publish.
#[sqlx::test]
#[ignore]
async fn concurrent_identical_in_format_posts_yield_one_person_one_publish(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let addr = recipient(&slug, &token);

    let (resp_a, resp_b) = tokio::join!(
        post_inbound_email(&router, &addr, CYPRESS_EML),
        post_inbound_email(&router, &addr, CYPRESS_EML)
    );
    assert_eq!(resp_a.status(), StatusCode::OK);
    assert_eq!(resp_b.status(), StatusCode::OK);

    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "person", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 1);
    assert_eq!(recorded(&publisher).await.len(), 1);
}

/// Criterion 15: two concurrent *different* in-format emails sharing one
/// contact value serialize on the advisory lock — one Person, two
/// Inquiries. The lock's reason to exist on this path.
#[sqlx::test]
#[ignore]
async fn concurrent_different_emails_sharing_a_contact_yield_one_person_two_inquiries(
    migrator_pool: PgPool,
) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let addr = recipient(&slug, &token);

    let second = String::from_utf8_lossy(CYPRESS_EML)
        .replace("fixture-cypress-001", "fixture-cypress-003")
        .replace("Name: Jordan Ellis", "Name: J. Ellis")
        .replace(
            "Interested in the 45 Shoreline Dr listing.",
            "Another note.",
        );
    let (resp_a, resp_b) = tokio::join!(
        post_inbound_email(&router, &addr, CYPRESS_EML),
        post_inbound_email(&router, &addr, second.as_bytes())
    );
    assert_eq!(resp_a.status(), StatusCode::OK);
    assert_eq!(resp_b.status(), StatusCode::OK);

    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 2);
    assert_eq!(count(&migrator_pool, "person", org_id).await, 1, "deduped");
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 2);
}

/// Criterion 19: org A's valid-format mail writes zero rows in org B, and
/// the created Person is invisible to org B's members.
#[sqlx::test]
#[ignore]
async fn valid_format_mail_writes_nothing_in_another_org(migrator_pool: PgPool) {
    let (org_a, _bob_a, _alice_a) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (org_b, _bob_b, _alice_b) = org_with_default(&migrator_pool, "Best Realty").await;
    let (slug_a, token_a) = intake_row(&migrator_pool, org_a).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug_a, &token_a), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    for table in [
        "raw_payload",
        "person",
        "inquiry",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
    ] {
        assert_eq!(count(&migrator_pool, table, org_b).await, 0, "{table}");
    }
    let bob_b_cookie = common::login_cookie(&router, "bob@bestrealty.test", PW).await;
    let people =
        common::body_json(common::get_with_cookie(&router, "/api/people", &bob_b_cookie).await)
            .await;
    assert_eq!(people["people"].as_array().unwrap().len(), 0);
}

/// Criterion 18 (completed, unparsed, and no-contact paths — the
/// unrecognized path is pinned by db_inbound_email.rs's capture test):
/// no span or log line carries the lead's name, contact values, subject,
/// body, or the org's slug/token — only the static format name may
/// appear.
#[sqlx::test]
#[ignore]
async fn completed_path_leaks_no_content_into_spans_or_logs(migrator_pool: PgPool) {
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

    // Unique org name: the global subscriber captures every concurrent
    // test's output (see db_inbound_email.rs's capture test for the full
    // rationale).
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Completed Capture Org").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
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

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    // The unparsed and no-contact paths, under the same capture.
    let resp = post_inbound_email(&router, &recipient(&slug, &token), GARBAGE_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_NO_CONTACT_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("intake.inbound_email"), "span present");
    assert!(
        captured.contains("cypress_bay_contact_v1"),
        "the static format name is the allowed signal"
    );
    for leak in [
        "Jordan",
        "Ellis",
        "jordan.ellis@example.com",
        "555-0142",
        "New contact form submission",
        "Shoreline",
        "hello world this is not an email",
        "Riley",
        "Quinn",
        "Please call me",
        slug.as_str(),
        token.as_str(),
    ] {
        assert!(!captured.contains(leak), "leaked: {leak}");
    }
}

/// Adversarial-finding pin: an in-format email with NUL (0x00) bytes in
/// its fields must complete normally — Postgres TEXT rejects 0x00, so
/// without the mime-wrapper strip this was an attacker-triggerable 503
/// plus a permanently stuck, queue-invisible `pending` row.
#[sqlx::test]
#[ignore]
async fn nul_bytes_in_an_in_format_email_complete_without_error_or_poison_row(
    migrator_pool: PgPool,
) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let mut raw = Vec::new();
    raw.extend_from_slice(b"From: \"Cypress Bay Realty\" <forms@cypressbayrealty.com>\r\n");
    raw.extend_from_slice(b"Subject: New contact form submission\r\n");
    raw.extend_from_slice(b"Content-Type: text/plain; charset=utf-8\r\n\r\n");
    raw.extend_from_slice(b"Name: Jor\x00dan Ellis\r\n");
    raw.extend_from_slice(b"Email: jordan.ellis@example.com\r\n");
    raw.extend_from_slice(b"Message: hi\x00there\r\n");

    let resp = post_inbound_email(&router, &recipient(&slug, &token), &raw).await;
    assert_eq!(resp.status(), StatusCode::OK, "must not 503");
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );

    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "resolved", "no stuck-pending poison row");
    let (first_name, message, assigned): (Option<String>, Option<String>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT p.first_name, i.message, p.assigned_user_id
             FROM person p JOIN inquiry i ON i.person_id = p.id
             WHERE p.organization_id = $1",
        )
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(first_name.as_deref(), Some("Jordan"));
    assert_eq!(message.as_deref(), Some("hithere"));
    assert_eq!(assigned, Some(bob));
}

/// Criterion 4 (default-unset half, on the email path): no default
/// assignee → the Person is created unassigned via `unassigned` routing,
/// no assignment fact, in People, on no member's Today.
#[sqlx::test]
#[ignore]
async fn email_intake_without_a_default_creates_an_unassigned_person(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    // Slice 008 (D-041): clear back to `unassigned` mode too, not just the
    // assignee column — otherwise this org would sit in `default_assignee`
    // mode with no assignee, which still routes `unassigned` (the D-035
    // stale/NULL fallback, unchanged), but explicitly clearing both is the
    // faithful equivalent of the old single-column clear this test relies
    // on.
    admin_queries::update_intake_routing_settings(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
        crm_api::domain::intake::IntakeRoutingMode::Unassigned,
        None,
    )
    .await
    .unwrap();
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (assigned,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assigned_user_id FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assigned, None);
    let (strategy,): (String,) =
        sqlx::query_as("SELECT strategy FROM routing_decision WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(strategy, "unassigned");
    assert_eq!(count(&migrator_pool, "assignment_changed", org_id).await, 0);

    for member in ["bob@acmerealty.test", "alice@acmerealty.test"] {
        let cookie = common::login_cookie(&router, member, PW).await;
        let today =
            common::body_json(common::get_with_cookie(&router, "/api/today", &cookie).await).await;
        assert_eq!(today["items"].as_array().unwrap().len(), 0, "{member}");
    }
    let cookie = common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let people =
        common::body_json(common::get_with_cookie(&router, "/api/people", &cookie).await).await;
    assert_eq!(people["people"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------
// Slice 007h1 (docs/specs/SLICE_007h1.md §6): forwarded-wrapper
// unwrapping. "LLM path not invoked" holds by construction in every
// deterministic-outcome test here: `build_app` never starts the
// extraction worker (crm-api/src/lib.rs), so a `resolved` row can only
// have come from the pinned-format path.

const GMAIL_FWD_CYPRESS_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_cypress_bay.eml");
const FAKE_FWD_SUBJECT_EML: &[u8] = include_bytes!("fixtures/email/fake_fwd_subject.eml");
const GMAIL_FWD_FORGED_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_forged_inner.eml");
const CYPRESS_BANNER_IN_MESSAGE_EML: &[u8] =
    include_bytes!("fixtures/email/cypress_bay_banner_in_message.eml");
const GMAIL_FWD_NESTED_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_nested.eml");
const GMAIL_FWD_UNKNOWN_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_unknown_lead.eml");
const GMAIL_FWD_EMPTY_INNER_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_empty_inner.eml");
const GMAIL_FWD_HTML_EML: &[u8] = include_bytes!("fixtures/email/gmail_fwd_html_only.eml");

async fn resolution_row(pool: &PgPool, org_id: Uuid) -> (String, Option<String>) {
    sqlx::query_as(
        "SELECT resolution, unresolved_reason FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Criterion 1: a Gmail forward of the pinned form mail completes
/// end-to-end — inner fields, D-035 routing, `source='website'` — with
/// the preamble above the banner playing no part.
#[sqlx::test]
#[ignore]
async fn gmail_forward_of_pinned_format_mail_completes_deterministically(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (resolution, _) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "resolved");
    let (first_name, last_name, assigned): (Option<String>, Option<String>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT first_name, last_name, assigned_user_id FROM person WHERE organization_id = $1",
        )
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(first_name.as_deref(), Some("Jordan"));
    assert_eq!(last_name.as_deref(), Some("Ellis"));
    assert_eq!(assigned, Some(bob), "D-035 default-assignee routing");
    let source: String =
        sqlx::query_scalar("SELECT source FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(source, "website", "the inner format's source label");
}

/// Criterion 3 (first half): a "Fwd:" subject with no real banner is a
/// no-op — exactly today's unrecognized outcome, nothing created.
#[sqlx::test]
#[ignore]
async fn fake_fwd_subject_is_a_noop_and_lands_unrecognized(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), FAKE_FWD_SUBJECT_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, reason) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
}

/// Criterion 4: a hand-forged forward whose inner block claims the
/// pinned domain creates the D-040-accepted lead through the
/// ForwardedClaim arm — the accepted blast radius (one bogus,
/// recognizable lead; spoofable source label), never anything more.
#[sqlx::test]
#[ignore]
async fn forged_inner_forward_creates_the_d040_accepted_lead(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_FORGED_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, _) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "resolved");
    let first_name: Option<String> =
        sqlx::query_scalar("SELECT first_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(first_name.as_deref(), Some("Forged"));
}

/// Reviewer S-2 pin: a genuine DIRECT form mail whose Message field
/// quotes a forward banner keeps its deterministic parse — detect runs
/// before resolve, so the outer fields win and the quoted text stays
/// inside the Message.
#[sqlx::test]
#[ignore]
async fn direct_mail_with_a_quoted_banner_keeps_its_deterministic_parse(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(
        &router,
        &recipient(&slug, &token),
        CYPRESS_BANNER_IN_MESSAGE_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, _) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "resolved");
    let first_name: Option<String> =
        sqlx::query_scalar("SELECT first_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        first_name.as_deref(),
        Some("Quoting"),
        "the OUTER form's lead"
    );
    let message: Option<String> =
        sqlx::query_scalar("SELECT message FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let message = message.unwrap();
    assert!(
        message.contains("Forwarded message"),
        "the quoted banner stays inside the Message field"
    );
    assert!(message.contains("quoted text that must stay"));
}

/// Criterion 5: a forward-of-a-forward unwraps to the innermost form
/// mail within the depth cap.
#[sqlx::test]
#[ignore]
async fn nested_forward_unwraps_to_the_innermost_form_mail(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_NESTED_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, _) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "resolved");
    let first_name: Option<String> =
        sqlx::query_scalar("SELECT first_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(first_name.as_deref(), Some("Nested"));
}

/// Criterion 2 (delivery half): a forward of an unknown-format lead
/// stays extraction-eligible — same reason, same partial-index
/// predicate; the worker seeing the INNER view is pinned at the unit
/// seam (`forwarded_mail_extraction_input_is_the_inner_view`).
#[sqlx::test]
#[ignore]
async fn forwarded_unknown_lead_lands_unrecognized_for_extraction(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_UNKNOWN_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, reason) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
}

/// Criterion 3 (second half): a real banner with an empty inner body is
/// a no-op — whole-message fallback, no panic, nothing created.
#[sqlx::test]
#[ignore]
async fn forward_banner_with_empty_inner_body_falls_back_whole_message(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(
        &router,
        &recipient(&slug, &token),
        GMAIL_FWD_EMPTY_INNER_EML,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, reason) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "unresolved");
    assert_eq!(reason.as_deref(), Some("email_unrecognized_format"));
    assert_eq!(count(&migrator_pool, "person", org_id).await, 0);
}

/// Criterion 6: an HTML-only Gmail forward — the banner survives the
/// HTML→text conversion, unwraps, and the inner form mail completes.
#[sqlx::test]
#[ignore]
async fn html_only_forward_unwraps_and_completes(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_HTML_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (resolution, _) = resolution_row(&migrator_pool, org_id).await;
    assert_eq!(resolution, "resolved");
    let first_name: Option<String> =
        sqlx::query_scalar("SELECT first_name FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(first_name.as_deref(), Some("Hilda"));
}

const GMAIL_FWD_CYPRESS_OTHER_AGENT_EML: &[u8] =
    include_bytes!("fixtures/email/gmail_fwd_cypress_bay_other_agent.eml");

/// Criterion 8 (redelivery half): byte-identical redelivery of a
/// forwarded mail is the same HMAC no-op as any other — the dedup runs
/// on raw OUTER bytes before any unwrapping.
#[sqlx::test]
#[ignore]
async fn byte_identical_redelivery_of_a_forwarded_mail_is_a_noop(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    for _ in 0..2 {
        let resp =
            post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_CYPRESS_EML).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "person", org_id).await, 1);
    assert_eq!(count(&migrator_pool, "inquiry", org_id).await, 1);
}

/// The duplicate scenario 007h1 newly enables (adversarial M2): two
/// different agents forward the same original lead — different outer
/// bytes (two raw_payload rows) converging on ONE Person via the
/// existing contact-method identify step.
#[sqlx::test]
#[ignore]
async fn two_forwarders_of_the_same_inner_lead_converge_on_one_person(migrator_pool: PgPool) {
    let (org_id, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    for raw in [GMAIL_FWD_CYPRESS_EML, GMAIL_FWD_CYPRESS_OTHER_AGENT_EML] {
        let resp = post_inbound_email(&router, &recipient(&slug, &token), raw).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    assert_eq!(count(&migrator_pool, "raw_payload", org_id).await, 2);
    assert_eq!(count(&migrator_pool, "person", org_id).await, 1);
}

/// Criterion 8 (tenant half): a forwarded mail delivered to org A's
/// intake address — whatever domain its inner block claims — writes
/// nothing in org B. Org resolution is the recipient token, never any
/// From line.
#[sqlx::test]
#[ignore]
async fn forwarded_mail_writes_nothing_in_another_org(migrator_pool: PgPool) {
    let (org_a, _bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    let (org_b, _, _) = org_with_default(&migrator_pool, "Rival Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_a).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), GMAIL_FWD_FORGED_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(count(&migrator_pool, "person", org_a).await, 1);
    for table in ["raw_payload", "person", "inquiry"] {
        assert_eq!(count(&migrator_pool, table, org_b).await, 0, "{table}");
    }
}

const GMAIL_FWD_REAL_STRUCTURE_EML: &[u8] =
    include_bytes!("fixtures/email/gmail_fwd_real_structure.eml");

/// Reconciliation against REAL Gmail bytes (2026-08-25): the user's
/// actual forwards (plain, HTML-heavy, nested) were verified locally
/// against the unwrapper — banner, quoted-printable date line with
/// U+202F, multipart/alternative, and the trailing forwarder signature
/// all as this sanitized replica preserves them. Runs without a
/// database so the structural pin lives in the fast gate.
#[test]
fn real_gmail_structure_unwraps_to_the_inner_view() {
    use crm_api::domain::intake::email::{forward, mime, SenderTrust};
    let mail = mime::parse(GMAIL_FWD_REAL_STRUCTURE_EML).expect("parses");
    let resolved = forward::resolve(mail);
    assert_eq!(resolved.trust, SenderTrust::ForwardedClaim { depth: 1 });
    assert_eq!(resolved.style, Some("gmail_inline_v1"));
    assert_eq!(
        resolved.mail.from_addr.as_deref(),
        Some("maya.l@example.com")
    );
    assert_eq!(
        resolved.mail.subject.as_deref(),
        Some("Looking at 12 Harbor Lane")
    );
    let body = resolved.mail.text_body.as_deref().unwrap();
    assert!(body.contains("(415) 555-0173"));
    // Known accepted edge (SLICE_007h1 §5 note): Gmail places the
    // FORWARDER's signature after the quoted message at the same text
    // level — it is part of the inner body in plain text. The HTML
    // gmail_quote structure could separate it; that is a later rung.
    assert!(body.contains("(555) 555-0100"), "trailing signature stays");
}

// ---------------------------------------------------------------------
// Slice 008 adversarial M2 (email half): mode dispatch applies to the
// real inbound-email path — a pinned-format mail into a round_robin org
// routes via rotation, not the default assignee.

#[sqlx::test]
#[ignore]
async fn inbound_email_in_a_round_robin_org_routes_via_rotation(migrator_pool: PgPool) {
    let (org_id, bob, _alice) = org_with_default(&migrator_pool, "Acme Realty").await;
    admin_queries::update_intake_routing_settings(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
        crm_api::domain::intake::IntakeRoutingMode::RoundRobin,
        Some(UserId::new(bob)),
    )
    .await
    .unwrap();
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(&router, &recipient(&slug, &token), CYPRESS_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // First rotation starts at the first member in join order (bob — the
    // helper adds him before alice), recorded as a round_robin decision.
    let (assigned, strategy): (Option<Uuid>, String) = sqlx::query_as(
        "SELECT p.assigned_user_id, rd.strategy
         FROM person p JOIN routing_decision rd ON rd.person_id = p.id
         WHERE p.organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .expect("one person with a routing decision");
    assert_eq!(strategy, "round_robin");
    assert_eq!(assigned, Some(bob));
    let pointer: Option<Uuid> = sqlx::query_scalar(
        "SELECT last_assigned_user_id FROM intake_rotation WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(pointer, Some(bob));
}

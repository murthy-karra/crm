//! DB-backed tests for Slice 007g (docs/specs/SLICE_007g.md §9): token
//! rotation. Run only via ./scripts/check-db.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const PLAIN_EML: &[u8] = include_bytes!("fixtures/email/plain.eml");
const PW: &str = "pw";
const TEST_INBOUND_EMAIL_SECRET: &str = "test-inbound-email-secret-value-32b";

fn test_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(crate::common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => Some(crate::common::TEST_CENTRIFUGO_HTTP_API_KEY.to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => {
            Some(crate::common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.to_string())
        }
        "CRM_INBOUND_EMAIL_SECRET" => Some(TEST_INBOUND_EMAIL_SECRET.to_string()),
        _ => None,
    })
    .unwrap()
}

async fn build_router(migrator_pool: &PgPool) -> Router {
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, Publisher::recording());
    crm_api::build_app(state)
}

async fn org_with_admin(migrator_pool: &PgPool, name: &str) -> Uuid {
    use crm_api::domain::admin::{MembershipStatus, Role};
    let org_id = crate::common::create_org(migrator_pool, name).await;
    crate::common::seed_stages(migrator_pool, org_id).await;
    let slug: String = name.to_lowercase().replace(' ', "");
    let alice =
        crate::common::create_user(migrator_pool, &format!("alice@{slug}.test"), "Alice", PW).await;
    let bob =
        crate::common::create_user(migrator_pool, &format!("bob@{slug}.test"), "Bob", PW).await;
    crate::common::add_membership_with(
        migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership(migrator_pool, org_id, bob).await;
    org_id
}

async fn intake_row(pool: &PgPool, org_id: Uuid) -> (String, String) {
    sqlx::query_as("SELECT intake_slug, intake_token FROM organization WHERE id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
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

async fn deliver(router: &Router, slug: &str, token: &str, raw: &[u8]) -> axum::response::Response {
    let body = json!({
        "recipient": format!("leads-{token}@{slug}.elysianfeld.com"),
        "raw": STANDARD.encode(raw),
    })
    .to_string();
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

/// Criteria 1, 2: rotate mints a fresh in-alphabet token, writes exactly
/// one envelope fact row (no token material anywhere in it), returns the
/// new address; repeated rotation works; the fact table is append-only.
#[sqlx::test]
#[ignore]
async fn rotate_mints_audits_and_returns_the_new_address(migrator_pool: PgPool) {
    let org_id = org_with_admin(&migrator_pool, "Acme Realty").await;
    let (_slug, old_token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmerealty.test", PW).await;
    let alice: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE email = 'alice@acmerealty.test'")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(
        body["scheme"], "subdomain",
        "the GET shape, scheme included"
    );
    let new_address = body["address"].as_str().unwrap().to_string();
    assert!(!new_address.contains(&old_token), "old token gone");

    let (_slug, new_token) = intake_row(&migrator_pool, org_id).await;
    assert_ne!(new_token, old_token);
    assert_eq!(new_token.len(), 8);
    assert!(new_token
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '2'..='7')));
    assert!(new_address.contains(&new_token));

    // Exactly one fact row, user actor, no token columns exist at all.
    let (count, actor_kind, actor, origin): (i64, String, Option<Uuid>, String) = sqlx::query_as(
        "SELECT count(*) OVER (), actor_kind, actor_user_id, origin FROM intake_token_rotated WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(actor_kind, "user");
    assert_eq!(actor, Some(alice));
    assert_eq!(origin, "web_session");

    // Repeated rotation works and appends.
    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM intake_token_rotated WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 2);

    // Append-only: crm_app grant denies; the trigger backstops migrator.
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let err = sqlx::query("DELETE FROM intake_token_rotated WHERE organization_id = $1")
        .bind(org_id)
        .execute(&app_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    let err =
        sqlx::query("UPDATE intake_token_rotated SET origin = 'x' WHERE organization_id = $1")
            .bind(org_id)
            .execute(&migrator_pool)
            .await
            .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
    let err = sqlx::query("DELETE FROM intake_token_rotated WHERE organization_id = $1")
        .bind(org_id)
        .execute(&migrator_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
    let err = sqlx::query("TRUNCATE intake_token_rotated")
        .execute(&migrator_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
}

/// Criterion 3: old-token delivery → 200 rejected, nothing stored;
/// new-token delivery → stored/processed. The 007b properties re-pinned
/// across a rotation.
#[sqlx::test]
#[ignore]
async fn old_address_dies_and_new_address_flows_after_rotation(migrator_pool: PgPool) {
    let org_id = org_with_admin(&migrator_pool, "Acme Realty").await;
    let (slug, old_token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmerealty.test", PW).await;

    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (_slug, new_token) = intake_row(&migrator_pool, org_id).await;

    // Old token: 200 rejected (byte-identical with any other rejection),
    // nothing stored — including mail "accepted upstream pre-rotation".
    let resp = deliver(&router, &slug, &old_token, PLAIN_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "status": "rejected" })
    );
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    // New token: flows end-to-end.
    let resp = deliver(&router, &slug, &new_token, PLAIN_EML).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "status": "accepted" })
    );
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

/// Criterion 4: member 403; org-B admin's rotate changes only org B.
#[sqlx::test]
#[ignore]
async fn rotation_is_admin_only_and_tenant_isolated(migrator_pool: PgPool) {
    let org_a = org_with_admin(&migrator_pool, "Acme Realty").await;
    let org_b = org_with_admin(&migrator_pool, "Best Realty").await;
    let (_, token_a_before) = intake_row(&migrator_pool, org_a).await;
    let router = build_router(&migrator_pool).await;

    // Member (bob) → 403.
    let bob_cookie = crate::common::login_cookie(&router, "bob@acmerealty.test", PW).await;
    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &bob_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Org B's admin rotates: only org B's token changes.
    let b_admin_cookie = crate::common::login_cookie(&router, "alice@bestrealty.test", PW).await;
    let (_, token_b_before) = intake_row(&migrator_pool, org_b).await;
    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &b_admin_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (_, token_a_after) = intake_row(&migrator_pool, org_a).await;
    let (_, token_b_after) = intake_row(&migrator_pool, org_b).await;
    assert_eq!(token_a_after, token_a_before, "org A untouched");
    assert_ne!(token_b_after, token_b_before);
    let (count_a,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM intake_token_rotated WHERE organization_id = $1")
            .bind(org_a)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count_a, 0, "no org-A fact");
}

/// The §9 leak pin: the rotate path's spans and logs carry ids only —
/// never token material (old or new).
#[sqlx::test]
#[ignore]
async fn rotation_spans_and_logs_carry_no_token_material(migrator_pool: PgPool) {
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

    // Unique org name (global-subscriber caveat; see the other capture
    // tests). Note the org-creation span legitimately logs the SLUG
    // (SLICE_007a §9) — only the TOKENS are secrets here.
    let org_id = org_with_admin(&migrator_pool, "Rotation Capture Org").await;
    let (_slug, old_token) = intake_row(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool).await;
    let alice_cookie =
        crate::common::login_cookie(&router, "alice@rotationcaptureorg.test", PW).await;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    let resp = post_empty(
        &router,
        "/api/organization/intake-address/rotate",
        &alice_cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (_slug, new_token) = intake_row(&migrator_pool, org_id).await;

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("intake.rotate_token"), "span present");
    for secret in [old_token.as_str(), new_token.as_str()] {
        assert!(!captured.contains(secret), "leaked token");
    }
}

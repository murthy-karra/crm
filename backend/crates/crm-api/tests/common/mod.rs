//! Shared DB-backed test helpers for Slice 002 (docs/specs/SLICE_002.md
//! §13). Mirrors the per-file helper pattern `tests/db_identity.rs`
//! established for Slice 001, factored out because Slice 002's fixtures
//! (encrypted payload sealing, router construction, HTTP helpers) are
//! reused across several test files.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::auth::password;
use crm_api::config::Config;
use crm_api::domain::raw_payload::crypto;
use crm_api::domain::stage;
use crm_api::state::AppState;

/// The fixed raw-payload key every test in this suite uses, so a fixture
/// sealed via `seal_fixture` and a request served by `build_router`'s
/// `AppState` always agree. Exactly 64 hex characters.
pub const TEST_RAW_PAYLOAD_KEY_HEX: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";

const _ASSERT_TEST_KEY_LEN: () = assert!(TEST_RAW_PAYLOAD_KEY_HEX.len() == 64);

pub fn app_password() -> String {
    std::env::var("CRM_DB_APP_PASSWORD").expect("CRM_DB_APP_PASSWORD must be set for check-db")
}

pub fn migrator_password() -> String {
    std::env::var("CRM_DB_MIGRATOR_PASSWORD")
        .expect("CRM_DB_MIGRATOR_PASSWORD must be set for check-db")
}

/// A connection URL for the same ephemeral database `#[sqlx::test]`
/// created, usable to run the real `seed` binary as a subprocess against it
/// (docs/specs/SLICE_002.md §13, criterion 14: seed idempotency).
pub fn migrator_url_for(migrator_pool: &PgPool) -> String {
    let opts = migrator_pool.connect_options();
    format!(
        "postgres://{}:{}@{}:{}/{}",
        opts.get_username(),
        migrator_password(),
        opts.get_host(),
        opts.get_port(),
        opts.get_database()
            .expect("ephemeral test database has a name"),
    )
}

pub async fn connect_as_app(migrator_pool: &PgPool) -> PgPool {
    let options = migrator_pool
        .connect_options()
        .as_ref()
        .clone()
        .username("crm_app")
        .password(&app_password());
    PgPoolOptions::new()
        .connect_with(options)
        .await
        .expect("connect with swapped crm_app credentials")
}

pub fn test_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        _ => None,
    })
    .unwrap()
}

/// The router under test runs as `crm_app`, not the migrator — a forgotten
/// GRANT must fail here, not only in `dev-api` (docs/specs/SLICE_001.md §9).
pub async fn build_router(migrator_pool: &PgPool) -> Router {
    let app_pool = connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config);
    crm_api::build_app(state)
}

// --- Fixtures: Organizations, users, memberships, stages -----------------

pub async fn create_org(pool: &PgPool, name: &str) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as("INSERT INTO organization (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
    id
}

pub async fn create_user(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password_plain: &str,
) -> Uuid {
    let (id,): (Uuid,) =
        sqlx::query_as("INSERT INTO app_user (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(email)
            .bind(display_name)
            .fetch_one(pool)
            .await
            .unwrap();
    let hash = password::hash_password(password_plain).unwrap();
    sqlx::query("INSERT INTO local_credential (user_id, password_hash) VALUES ($1, $2)")
        .bind(id)
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();
    id
}

pub async fn add_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid) {
    sqlx::query("INSERT INTO organization_membership (organization_id, user_id) VALUES ($1, $2)")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Seeds the nine D-019 default stages for `org_id` via the same library
/// helper `scripts/dev-seed` uses.
pub async fn seed_stages(pool: &PgPool, org_id: Uuid) {
    let mut tx = pool.begin().await.unwrap();
    stage::seed_defaults(&mut tx, org_id).await.unwrap();
    tx.commit().await.unwrap();
}

/// A full fixture: one Organization, nine stages, and one member with the
/// given credentials — the common starting point for intake tests.
pub async fn create_org_with_stages_and_member(
    pool: &PgPool,
    org_name: &str,
    email: &str,
    display_name: &str,
    password_plain: &str,
) -> (Uuid, Uuid) {
    let org_id = create_org(pool, org_name).await;
    seed_stages(pool, org_id).await;
    let user_id = create_user(pool, email, display_name, password_plain).await;
    add_membership(pool, org_id, user_id).await;
    (org_id, user_id)
}

// --- HTTP helpers ----------------------------------------------------

pub async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

pub fn extract_cookie(response: &Response) -> String {
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("must set a cookie")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

pub async fn login(router: &Router, email: &str, password: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Logs in and returns just the session cookie — the common case for
/// fixture setup in tests that don't need the login response body.
pub async fn login_cookie(router: &Router, email: &str, password: &str) -> String {
    let response = login(router, email, password).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "fixture login must succeed"
    );
    extract_cookie(&response)
}

pub async fn get_with_cookie(router: &Router, uri: &str, cookie: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn post_json_with_cookie(
    router: &Router,
    uri: &str,
    cookie: &str,
    body: serde_json::Value,
) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn post_inquiry(
    router: &Router,
    cookie: &str,
    source: &str,
    payload: serde_json::Value,
    assign_to_user_id: Option<Uuid>,
) -> Response {
    let mut body = serde_json::json!({ "source": source, "payload": payload });
    if let Some(assignee) = assign_to_user_id {
        body["assign_to_user_id"] = serde_json::json!(assignee);
    }
    post_json_with_cookie(router, "/api/inquiries", cookie, body).await
}

// --- Crypto fixture helper -------------------------------------------

/// Seals `payload` under the suite's fixed test key exactly as
/// `receive_inquiry` would, for tests that insert a `raw_payload` row
/// directly rather than going through the HTTP endpoint (spec §13: "a
/// `pending` row (fixture-inserted, encrypted with the test key)").
pub fn seal_fixture(
    organization_id: Uuid,
    raw_payload_id: Uuid,
    payload: &serde_json::Value,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, i32) {
    let config = test_config();
    let plaintext = serde_json::to_vec(payload).unwrap();
    let sealed = crypto::seal(
        &config.raw_payload_key,
        organization_id,
        raw_payload_id,
        &plaintext,
    )
    .unwrap();
    let hmac = crypto::content_hmac(&config.raw_payload_key, &plaintext);
    (
        sealed.nonce.to_vec(),
        sealed.ciphertext,
        hmac.to_vec(),
        plaintext.len() as i32,
    )
}

pub async fn insert_raw_payload_fixture(
    pool: &PgPool,
    id: Uuid,
    organization_id: Uuid,
    source: &str,
    resolution: &str,
    payload: &serde_json::Value,
) {
    let (nonce, ciphertext, content_hmac, byte_len) = seal_fixture(organization_id, id, payload);
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, $3, 'generic_v1', 'web_session', now(), $4, $5, $6, $7, $8)"#,
    )
    .bind(id)
    .bind(organization_id)
    .bind(source)
    .bind(nonce)
    .bind(ciphertext)
    .bind(content_hmac)
    .bind(byte_len)
    .bind(resolution)
    .execute(pool)
    .await
    .unwrap();
}

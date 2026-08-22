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
use crm_api::domain::admin::commands::{
    create_organization, grant_platform_admin, CreateOrganization, GrantPlatformAdmin,
};
use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::admin::{AdminActor, MembershipStatus, Role};
use crm_api::domain::envelope::Origin;
use crm_api::domain::raw_payload::crypto;
use crm_api::domain::stage;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

/// The fixed raw-payload key every test in this suite uses, so a fixture
/// sealed via `seal_fixture` and a request served by `build_router`'s
/// `AppState` always agree. Exactly 64 hex characters.
pub const TEST_RAW_PAYLOAD_KEY_HEX: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";

const _ASSERT_TEST_KEY_LEN: () = assert!(TEST_RAW_PAYLOAD_KEY_HEX.len() == 64);

/// A fixed test value for `CENTRIFUGO_TOKEN_HMAC_SECRET` (≥ 32 bytes),
/// used by every test that builds a router with `Publisher::recording()`
/// (i.e. every DB-backed test except the Centrifugo-backed suite, which
/// deliberately reads the real container secret from the environment —
/// docs/specs/SLICE_003.md §13).
pub const TEST_CENTRIFUGO_TOKEN_HMAC_SECRET: &str = "test-centrifugo-token-hmac-secret-32bytes!!";
pub const TEST_CENTRIFUGO_HTTP_API_KEY: &str = "test-centrifugo-http-api-key";

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

/// A `crm_app` connection URL for the same ephemeral database, usable to
/// run `crm-admin` as a subprocess against it (docs/specs/SLICE_004.md
/// §11: `bootstrap-platform-admin` needs `MIGRATION_DATABASE_URL`; every
/// other subcommand needs this as `DATABASE_URL`).
pub fn app_url_for(migrator_pool: &PgPool) -> String {
    let opts = migrator_pool.connect_options();
    format!(
        "postgres://crm_app:{}@{}:{}/{}",
        app_password(),
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
        "CENTRIFUGO_HTTP_API_KEY" => Some(TEST_CENTRIFUGO_HTTP_API_KEY.to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some(TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.to_string()),
        _ => None,
    })
    .unwrap()
}

/// The router under test runs as `crm_app`, not the migrator — a forgotten
/// GRANT must fail here, not only in `dev-api` (docs/specs/SLICE_001.md §9).
/// Uses a `Publisher::recording()` — most tests don't need a real
/// Centrifugo; the handful of publisher-contract tests build their own
/// router with `build_router_with_publisher`.
pub async fn build_router(migrator_pool: &PgPool) -> Router {
    build_router_with_publisher(migrator_pool, Publisher::recording()).await
}

pub async fn build_router_with_publisher(migrator_pool: &PgPool, publisher: Publisher) -> Router {
    let app_pool = connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, publisher);
    crm_api::build_app(state)
}

// --- Fixtures: Organizations, users, memberships, stages -----------------
//
// Slice 004 (D-021, docs/specs/SLICE_004.md §11 fixture rule): tests may
// use the migrator connection only to backdate timestamps or delete rows
// for negative cases, never to create domain rows. `create_org` therefore
// bootstraps a fixture platform admin via the real `GrantPlatformAdmin`
// migrator-only command (the one sanctioned migrator-write path) and then
// creates the Organization through `CreateOrganization` as `crm_app`, same
// as production. `create_user`/`add_membership` use the lower-level
// `domain::admin::queries` helpers — the same functions `AcceptInvitation`
// itself calls — as `crm_app`, rather than ad hoc SQL: there is no generic
// "create a bare user with no Organization" command by design (users are
// only ever created via `AcceptInvitation` or `GrantPlatformAdmin`), and
// routing every fixture user through a full issue+accept invitation
// round-trip is impractical for fixtures that intentionally build orphan
// (zero-membership) users.

/// A stable fixture platform admin, created idempotently per ephemeral
/// test database via the real migrator-only `GrantPlatformAdmin` command.
/// Never appears in any Organization's membership list (it is not added
/// to any Organization it creates), so it cannot leak into an assertion
/// about members/counts.
async fn fixture_platform_admin(migrator_pool: &PgPool) -> Uuid {
    grant_platform_admin(
        migrator_pool,
        GrantPlatformAdmin {
            email: "fixture-actor@test.internal".to_string(),
            display_name: "Fixture Actor".to_string(),
            password: "fixture-actor-password-123456".to_string(),
        },
    )
    .await
    .expect("fixture platform-admin bootstrap must succeed")
}

pub async fn create_org(pool: &PgPool, name: &str) -> Uuid {
    let actor_id = fixture_platform_admin(pool).await;
    let app_pool = connect_as_app(pool).await;
    let actor = AdminActor {
        actor_user_id: actor_id,
        origin: Origin::Cli,
    };
    let organization = create_organization(
        &app_pool,
        actor,
        CreateOrganization {
            name: name.to_string(),
        },
    )
    .await
    .expect("fixture organization creation must succeed");
    organization.id
}

pub async fn create_user(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password_plain: &str,
) -> Uuid {
    let app_pool = connect_as_app(pool).await;
    let mut conn = app_pool.acquire().await.unwrap();
    let user_id = admin_queries::insert_app_user(&mut conn, email, display_name)
        .await
        .unwrap();
    let hash = password::hash_password(password_plain).unwrap();
    admin_queries::insert_local_credential(&mut conn, user_id, &hash)
        .await
        .unwrap();
    user_id
}

/// A `member`/`active` membership — the common case every pre-004 fixture
/// used implicitly. Use `add_membership_with` for admin or inactive
/// fixtures.
pub async fn add_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid) {
    add_membership_with(
        pool,
        org_id,
        user_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await
}

pub async fn add_membership_with(
    pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
    role: Role,
    status: MembershipStatus,
) {
    let app_pool = connect_as_app(pool).await;
    let mut conn = app_pool.acquire().await.unwrap();
    admin_queries::insert_membership(&mut conn, org_id, user_id, role, status)
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

//! Service-free HTTP tests for the correspondence-capture endpoints
//! (docs/specs/SLICE_009.md §8): 401 without a cookie on all five
//! member-self routes, 400 on a non-UUID `{id}` path segment ahead of
//! authentication (the `CaptureMessageIdPath` extractor precedes
//! `AuthContext` in the handler signature, mirroring routes/people.rs's
//! `PersonIdPath` ordering — see tests/people.rs), and 503 against an
//! unreachable database for a representative GET and POST. Body-shape
//! and business-logic rejections need a real authenticated session and
//! are therefore covered in tests/db_capture_address.rs,
//! tests/db_capture_receive.rs, and tests/db_capture_unmatched.rs
//! instead of here.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::state::AppState;

fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    map.insert("CRM_RAW_PAYLOAD_KEY".to_string(), "ab".repeat(32));
    map.insert(
        "CENTRIFUGO_HTTP_API_KEY".to_string(),
        "test-centrifugo-api-key".to_string(),
    );
    map.insert("CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(), "c".repeat(32));
    for (k, v) in overrides {
        map.insert((*k).to_string(), (*v).to_string());
    }
    Config::from_source(move |key| map.get(key).cloned()).expect("valid test config")
}

fn unreachable_database_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("postgres://user:pass@{addr}/db")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn request(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri)
}

fn json_body(value: serde_json::Value) -> Body {
    Body::from(value.to_string())
}

// --- 401 without a cookie, on every route ----------------------------------

#[tokio::test]
async fn get_address_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/capture/address")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "unauthenticated" }));
}

#[tokio::test]
async fn rotate_address_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/capture/address/rotate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_unmatched_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/capture/unmatched")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn link_unmatched_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();

    let response = app
        .oneshot(
            request("POST", &format!("/api/capture/unmatched/{id}/link"))
                .header("content-type", "application/json")
                .body(json_body(
                    serde_json::json!({ "person_id": Uuid::new_v4() }),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dismiss_unmatched_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();

    let response = app
        .oneshot(
            request("POST", &format!("/api/capture/unmatched/{id}/dismiss"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- 400 on a non-UUID path parameter (Path precedes AuthContext, so this
// is independent of authentication state and needs no database) -----------

#[tokio::test]
async fn link_unmatched_with_non_uuid_path_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/capture/unmatched/not-a-uuid/link")
                .header("content-type", "application/json")
                .body(json_body(
                    serde_json::json!({ "person_id": Uuid::new_v4() }),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "malformed_request" }));
}

#[tokio::test]
async fn dismiss_unmatched_with_non_uuid_path_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/capture/unmatched/not-a-uuid/dismiss")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "malformed_request" }));
}

// --- 503 when the database is unreachable, for a representative GET and
// POST (mirrors tests/intake.rs's exact pair of checks) --------------------

#[tokio::test]
async fn get_address_returns_503_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    // A syntactically valid (right shape) but meaningless token, so the
    // AuthContext extractor passes its format check and actually reaches
    // the database (mirrors tests/session.rs's established pattern).
    let plausible_token = "a".repeat(43);

    let start = Instant::now();
    let response = app
        .oneshot(
            request("GET", "/api/capture/address")
                .header("cookie", format!("crm_session={plausible_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "did not respect the timeout bound: {elapsed:?}"
    );
}

#[tokio::test]
async fn rotate_address_returns_503_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let plausible_token = "a".repeat(43);

    let start = Instant::now();
    let response = app
        .oneshot(
            request("POST", "/api/capture/address/rotate")
                .header("cookie", format!("crm_session={plausible_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "did not respect the timeout bound: {elapsed:?}"
    );
}

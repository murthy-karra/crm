//! Service-free HTTP tests for `POST /api/operator/turns`
//! (docs/specs/SLICE_005.md §13 item 3). Every other case needs a session,
//! which `AuthContext` resolves against the database — same split as
//! `tests/today.rs`.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

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

fn turn_request(cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/operator/turns")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    builder
        .body(Body::from(
            serde_json::json!({ "message": "Who should I call next?" }).to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn post_turn_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app.oneshot(turn_request(None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_turn_returns_503_unavailable_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
        ("GROQ_API_KEY", "gsk_test_key_never_used"),
    ]))
    .unwrap();
    assert!(state.operator.is_some(), "a key enables the runtime");
    let app = crm_api::build_app(state);
    let plausible_token = "a".repeat(43);
    let cookie = format!("crm_session={plausible_token}");

    let start = Instant::now();
    let response = app.oneshot(turn_request(Some(&cookie))).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "did not respect the timeout bound: {elapsed:?}"
    );
}

#[tokio::test]
async fn runtime_is_absent_without_a_key() {
    let state = AppState::new(&test_config(&[])).unwrap();
    assert!(state.operator.is_none());
}

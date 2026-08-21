use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crm_api::config::Config;
use crm_api::state::AppState;

fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    for (k, v) in overrides {
        map.insert((*k).to_string(), (*v).to_string());
    }
    Config::from_source(move |key| map.get(key).cloned()).expect("valid test config")
}

/// A closed loopback port: connecting to it fails, exercising the same
/// bounded-unavailable path as tests/health.rs uses for readiness.
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

#[tokio::test]
async fn me_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(request("GET", "/api/me").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "unauthenticated" }));
}

#[tokio::test]
async fn members_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/organization/members")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_with_syntactically_invalid_cookie_returns_401_without_touching_database() {
    // No DATABASE_URL configured at all: if the extractor tried to reach
    // the database it would hit the Unavailable branch (503), not this
    // one, so a 401 here proves the format check runs first.
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/me")
                .header("cookie", "crm_session=not-the-right-length")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_returns_503_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    // A syntactically valid (right shape) but meaningless token, so the
    // extractor passes the format check and actually reaches the database.
    let plausible_token = "a".repeat(43);

    let start = Instant::now();
    let response = app
        .oneshot(
            request("GET", "/api/me")
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
async fn login_with_non_json_content_type_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/session")
                .header("content-type", "text/plain")
                .body(Body::from("email=a@example.com&password=x"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "malformed_request" }));
}

#[tokio::test]
async fn login_with_malformed_json_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/session")
                .header("content-type", "application/json")
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_with_missing_field_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("POST", "/api/session")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email":"a@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_returns_503_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let start = Instant::now();
    let response = app
        .oneshot(
            request("POST", "/api/session")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email":"a@example.com","password":"x"}"#))
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
async fn logout_without_cookie_is_idempotent_and_clears_cookie() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("DELETE", "/api/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("logout must clear the cookie")
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("crm_session="));
    assert!(set_cookie.to_lowercase().contains("max-age=0"));
}

#[tokio::test]
async fn logout_with_cookie_returns_503_and_leaves_cookie_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let plausible_token = "a".repeat(43);
    let response = app
        .oneshot(
            request("DELETE", "/api/session")
                .header("cookie", format!("crm_session={plausible_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    // The user must not be told they are logged out while the token
    // stays valid server-side (docs/specs/SLICE_001.md §4).
    assert!(response.headers().get("set-cookie").is_none());
}

#[tokio::test]
async fn no_cors_header_when_unconfigured() {
    // Same-origin loopback dev: no CRM_CORS_ALLOWED_ORIGIN set, so the
    // response must carry no CORS header at all, even when a browser-like
    // Origin header is present on the request.
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/health")
                .header("origin", "https://app.tarams.org")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
}

#[tokio::test]
async fn cors_header_present_for_configured_origin_only() {
    let state = AppState::new(&test_config(&[(
        "CRM_CORS_ALLOWED_ORIGIN",
        "https://app.tarams.org",
    )]))
    .unwrap();
    let app = crm_api::build_app(state);

    let allowed = app
        .clone()
        .oneshot(
            request("GET", "/api/health")
                .header("origin", "https://app.tarams.org")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        allowed
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "https://app.tarams.org"
    );
    assert_eq!(
        allowed
            .headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );

    // A different Origin must not be reflected — CORS must not degrade
    // into an open allow-any-origin policy.
    let other = app
        .oneshot(
            request("GET", "/api/health")
                .header("origin", "https://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(other.headers().get("access-control-allow-origin").is_none());
}

//! Service-free HTTP tests for `POST /inbound/email`
//! (docs/specs/SLICE_007b.md §11): 401 without a bearer on a stateless
//! app, 503 against an unreachable database, no CORS headers, and an
//! oversize body with a bad bearer still 413ing with the envelope
//! (pinning the extractor-before-handler ordering, §5).
use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crm_api::config::Config;
use crm_api::state::AppState;

const SECRET: &str = "service-free-test-inbound-email-secret-32b";

fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    map.insert("CRM_RAW_PAYLOAD_KEY".to_string(), "ab".repeat(32));
    map.insert(
        "CENTRIFUGO_HTTP_API_KEY".to_string(),
        "test-centrifugo-api-key".to_string(),
    );
    map.insert("CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(), "c".repeat(32));
    map.insert("CRM_INBOUND_EMAIL_SECRET".to_string(), SECRET.to_string());
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

fn well_formed_body() -> String {
    serde_json::json!({
        "recipient": "leads-k7f3q2wd@acme-realty.elysianfeld.com",
        "raw": STANDARD.encode(b"From: a@example.com\r\n\r\nhello"),
    })
    .to_string()
}

fn request(bearer: Option<&str>, body: Vec<u8>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/inbound/email")
        .header("content-type", "application/json");
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body)).unwrap()
}

#[tokio::test]
async fn no_bearer_is_401_on_a_stateless_app() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(request(None, well_formed_body().into_bytes()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({ "error": "unauthenticated" })
    );
}

#[tokio::test]
async fn secret_unset_is_401_even_with_a_bearer_present() {
    let mut state = AppState::new(&test_config(&[])).unwrap();
    state.inbound_email_secret = None;
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(request(Some("anything"), well_formed_body().into_bytes()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unreachable_database_with_valid_bearer_and_body_is_503() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let start = Instant::now();
    let response = app
        .oneshot(request(Some(SECRET), well_formed_body().into_bytes()))
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({ "error": "unavailable" })
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "did not respect the connect timeout bound: {elapsed:?}"
    );
}

#[tokio::test]
async fn route_is_outside_cors_and_carries_no_cors_headers() {
    let state = AppState::new(&test_config(&[(
        "CRM_CORS_ALLOWED_ORIGIN",
        "https://app.tarams.org",
    )]))
    .unwrap();
    let app = crm_api::build_app(state);

    let mut req = request(Some(SECRET), well_formed_body().into_bytes());
    req.headers_mut()
        .insert("origin", "https://app.tarams.org".parse().unwrap());
    let response = app.oneshot(req).await.unwrap();

    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
}

/// §5: an oversize body 413s even with a bad bearer — the body is fully
/// (un)buffered before this route's own bearer check runs — and still
/// carries the `ApiError::PayloadTooLarge` JSON envelope, not axum's
/// default plain-text rejection.
#[tokio::test]
async fn oversize_body_with_bad_bearer_is_413_with_the_envelope() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let oversize = vec![b'x'; 2 * 1024 * 1024 + 1];
    let response = app
        .oneshot(request(Some("a-wrong-bearer-value"), oversize))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({ "error": "payload_too_large" })
    );
}

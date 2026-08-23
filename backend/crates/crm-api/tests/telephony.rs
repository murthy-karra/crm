//! Service-free HTTP tests for the Slice 006 call routes, the Slice 006c
//! outcome route, and the LiveKit webhook (docs/specs/SLICE_006.md §13
//! item 1; SLICE_006c §13 item 1): 401/400/503 on every new route with
//! telephony disabled, and the webhook's signature gate. Every
//! other case needs a session, which `AuthContext` resolves against the
//! database — same split as `tests/operator.rs`.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::state::AppState;
use crm_api::telephony::{ScriptedProvider, Telephony, TelephonyLimits};

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

fn scripted_telephony() -> Arc<Telephony> {
    Arc::new(Telephony::with_provider(
        Arc::new(ScriptedProvider::new()),
        "scripted",
        "APIkey",
        b"test-livekit-secret",
        TelephonyLimits {
            ring_timeout: Duration::from_secs(10),
            max_call: Duration::from_secs(60),
            join_ttl: Duration::from_secs(60),
            agent_join_timeout: Duration::from_secs(1),
            presence_poll_interval: Duration::from_millis(20),
        },
    ))
}

/// `(method, uri, json body)` for every route under test, with a valid
/// UUID in the path.
fn routes() -> Vec<(&'static str, String, Option<serde_json::Value>)> {
    let id = Uuid::new_v4();
    vec![
        (
            "POST",
            format!("/api/people/{id}/calls"),
            Some(serde_json::json!({ "contact_method_id": Uuid::new_v4() })),
        ),
        ("POST", format!("/api/calls/{id}/dial"), None),
        ("POST", format!("/api/calls/{id}/hangup"), None),
        (
            "POST",
            format!("/api/calls/{id}/outcome"),
            Some(serde_json::json!({ "outcome": "left_message" })),
        ),
        ("GET", format!("/api/calls/{id}"), None),
    ]
}

fn request(
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn every_call_route_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    for (method, uri, body) in routes() {
        let response = app
            .clone()
            .oneshot(request(method, &uri, None, body))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
        assert_eq!(body_json(response).await["error"], "unauthenticated");
    }
}

#[tokio::test]
async fn every_call_route_with_a_non_uuid_id_returns_400_before_auth() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    for (method, uri, body) in [
        (
            "POST",
            "/api/people/not-a-uuid/calls".to_string(),
            Some(serde_json::json!({ "contact_method_id": Uuid::new_v4() })),
        ),
        ("POST", "/api/calls/not-a-uuid/dial".to_string(), None),
        ("POST", "/api/calls/not-a-uuid/hangup".to_string(), None),
        (
            "POST",
            "/api/calls/not-a-uuid/outcome".to_string(),
            Some(serde_json::json!({ "outcome": "left_message" })),
        ),
        ("GET", "/api/calls/not-a-uuid".to_string(), None),
    ] {
        let response = app
            .clone()
            .oneshot(request(method, &uri, None, body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        assert_eq!(body_json(response).await["error"], "malformed_request");
    }
}

#[tokio::test]
async fn every_call_route_returns_503_unavailable_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap()
    .with_telephony(scripted_telephony());
    let app = crm_api::build_app(state);
    let plausible_token = "a".repeat(43);
    let cookie = format!("crm_session={plausible_token}");

    for (method, uri, body) in routes() {
        let start = Instant::now();
        let response = app
            .clone()
            .oneshot(request(method, &uri, Some(&cookie), body))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {uri}"
        );
        assert_eq!(body_json(response).await["error"], "unavailable");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[tokio::test]
async fn telephony_is_absent_without_a_livekit_key() {
    let state = AppState::new(&test_config(&[])).unwrap();
    assert!(state.telephony.is_none());
}

// --- POST /webhooks/livekit ---------------------------------------------

fn webhook_request(auth: Option<&str>, body: &[u8]) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/webhooks/livekit")
        .header("content-type", "application/json");
    if let Some(auth) = auth {
        builder = builder.header("authorization", auth);
    }
    builder.body(Body::from(body.to_vec())).unwrap()
}

#[tokio::test]
async fn webhook_without_signature_or_with_telephony_disabled_is_401() {
    let body = br#"{"event":"room_finished","room":{"name":"call:x"}}"#;

    // Telephony disabled: nothing can be verified.
    let app = crm_api::build_app(AppState::new(&test_config(&[])).unwrap());
    let response = app.oneshot(webhook_request(None, body)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(response).await["error"], "unauthenticated");

    // Telephony enabled, no / bad / other-secret signatures.
    let telephony = scripted_telephony();
    let app = crm_api::build_app(
        AppState::new(&test_config(&[]))
            .unwrap()
            .with_telephony(telephony.clone()),
    );
    let response = app
        .clone()
        .oneshot(webhook_request(None, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(webhook_request(Some("not.a.jwt"), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let other = crm_api::telephony::WebhookVerifier::new("APIkey", b"another-secret");
    let token = other.sign_for_tests(body, Utc::now(), 300);
    let response = app
        .clone()
        .oneshot(webhook_request(Some(&token), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // A valid signature over a *different* body.
    let token = telephony
        .webhook
        .sign_for_tests(b"{\"event\":\"room_finished\"}", Utc::now(), 300);
    let response = app
        .clone()
        .oneshot(webhook_request(Some(&token), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_with_a_valid_signature_but_an_ignored_event_is_200_without_a_database() {
    let telephony = scripted_telephony();
    let app = crm_api::build_app(
        AppState::new(&test_config(&[]))
            .unwrap()
            .with_telephony(telephony.clone()),
    );
    let body = br#"{"event":"room_started","room":{"name":"call:x"}}"#;
    let token = telephony.webhook.sign_for_tests(body, Utc::now(), 300);
    let response = app
        .oneshot(webhook_request(Some(&token), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, serde_json::json!({}));
}

#[tokio::test]
async fn webhook_body_over_64_kib_is_413() {
    let telephony = scripted_telephony();
    let app = crm_api::build_app(
        AppState::new(&test_config(&[]))
            .unwrap()
            .with_telephony(telephony.clone()),
    );
    let body = vec![b'x'; 64 * 1024 + 1];
    let token = telephony.webhook.sign_for_tests(&body, Utc::now(), 300);
    let response = app
        .oneshot(webhook_request(Some(&token), &body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn webhook_is_outside_the_cors_layer() {
    // With CORS configured, an API route answers a preflight; the webhook
    // does not carry CORS headers at all (it is a server-to-server call).
    let telephony = scripted_telephony();
    let app = crm_api::build_app(
        AppState::new(&test_config(&[(
            "CRM_CORS_ALLOWED_ORIGIN",
            "https://app.tarams.org",
        )]))
        .unwrap()
        .with_telephony(telephony.clone()),
    );
    let body = br#"{"event":"room_started"}"#;
    let token = telephony.webhook.sign_for_tests(body, Utc::now(), 300);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/livekit")
                .header("origin", "https://app.tarams.org")
                .header("authorization", token)
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/calls/not-a-uuid")
                .header("origin", "https://app.tarams.org")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("https://app.tarams.org")
    );
}

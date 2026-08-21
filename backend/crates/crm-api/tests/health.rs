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

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_returns_ok_with_request_id_and_body() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").is_some());
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn ready_returns_503_without_database_url() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("x-request-id").is_some());

    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "status": "not_ready" }));
}

#[tokio::test]
async fn ready_returns_503_within_timeout_when_database_refuses_connection() {
    // Bind an ephemeral loopback port, then drop it immediately, so
    // connecting afterwards gets "connection refused." sqlx treats a
    // refused connection as "the database is still starting up" and
    // retries with backoff until the configured timeout, so this still
    // exercises close to the full bound rather than failing instantly.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let database_url = format!("postgres://user:pass@{addr}/db");
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let start = Instant::now();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "readiness did not respect the timeout bound: {elapsed:?}"
    );
}

#[tokio::test]
async fn ready_returns_503_within_timeout_when_database_accepts_but_never_responds() {
    // A listener that completes the TCP handshake but never writes a byte:
    // a materially different failure than "connection refused" above (no
    // sqlx connect-retry/backoff applies here), closer to a database that
    // is reachable but wedged (lock contention, a stuck failover).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let hold_open = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(socket);
    });

    let database_url = format!("postgres://user:pass@{addr}/db");
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
    ]))
    .unwrap();
    let app = crm_api::build_app(state);

    let start = Instant::now();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "readiness did not respect the timeout bound against a silent peer: {elapsed:?}"
    );

    hold_open.abort();
}

#[tokio::test]
async fn unknown_route_returns_404_with_request_id() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get("x-request-id").is_some());
}

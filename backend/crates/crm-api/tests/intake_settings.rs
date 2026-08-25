//! Service-free HTTP tests for the intake-settings endpoints
//! (docs/specs/SLICE_007c.md §11, `tests/intake.rs` pattern): 401 without
//! a cookie, 503 against an unreachable database. Both routes sit behind
//! `OrgAdminContext`, whose 403 role check and every malformed-body
//! rejection require a real authenticated session (`AuthContext` touches
//! the database via `session::verify`) and are therefore covered in
//! tests/db_intake_settings.rs instead of here — the same split
//! tests/intake.rs already established for the Slice 002 routes.
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

fn request(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri)
}

#[tokio::test]
async fn get_intake_settings_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("GET", "/api/organization/intake-settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_intake_settings_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);

    let response = app
        .oneshot(
            request("PUT", "/api/organization/intake-settings")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "intake_default_assignee_user_id": null }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_intake_settings_returns_503_when_database_unreachable() {
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
            request("GET", "/api/organization/intake-settings")
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
async fn put_intake_settings_returns_503_when_database_unreachable() {
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
            request("PUT", "/api/organization/intake-settings")
                .header("content-type", "application/json")
                .header("cookie", format!("crm_session={plausible_token}"))
                .body(Body::from(
                    serde_json::json!({ "intake_default_assignee_user_id": null }).to_string(),
                ))
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

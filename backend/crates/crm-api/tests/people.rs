//! Service-free HTTP tests for the People endpoints
//! (docs/specs/SLICE_002.md §13): 401 without a cookie, 400 on a non-UUID
//! path parameter, 503 against an unreachable database. Malformed-body
//! rejections require a real authenticated session (AuthContext follows
//! Path but precedes the body extractor — see routes/people.rs's ordering
//! comment) and are covered in tests/db_people.rs instead.
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

// --- 401 without a cookie -------------------------------------------

#[tokio::test]
async fn list_people_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app
        .oneshot(request("GET", "/api/people").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_person_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();
    let response = app
        .oneshot(
            request("GET", &format!("/api/people/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_assignment_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();
    let response = app
        .oneshot(
            request("POST", &format!("/api/people/{id}/assignment"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "assigned_user_id": null }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_stage_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();
    let response = app
        .oneshot(
            request("POST", &format!("/api/people/{id}/stage"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "stage_id": Uuid::new_v4() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- 400 on a non-UUID path parameter (Path precedes AuthContext, so this
// is independent of authentication state and needs no database) ---------

#[tokio::test]
async fn get_person_with_non_uuid_path_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app
        .oneshot(
            request("GET", "/api/people/not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "malformed_request" }));
}

#[tokio::test]
async fn set_assignment_with_non_uuid_path_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app
        .oneshot(
            request("POST", "/api/people/not-a-uuid/assignment")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "assigned_user_id": null }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_stage_with_non_uuid_path_returns_400() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app
        .oneshot(
            request("POST", "/api/people/not-a-uuid/stage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "stage_id": Uuid::new_v4() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// --- 503 against an unreachable database -----------------------------

async fn assert_returns_503(builder: axum::http::request::Builder, body: Body) {
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
            builder
                .header("cookie", format!("crm_session={plausible_token}"))
                .body(body)
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
async fn list_people_returns_503_when_database_unreachable() {
    assert_returns_503(request("GET", "/api/people"), Body::empty()).await;
}

#[tokio::test]
async fn get_person_returns_503_when_database_unreachable() {
    let id = Uuid::new_v4();
    assert_returns_503(request("GET", &format!("/api/people/{id}")), Body::empty()).await;
}

#[tokio::test]
async fn set_assignment_returns_503_when_database_unreachable() {
    let id = Uuid::new_v4();
    assert_returns_503(
        request("POST", &format!("/api/people/{id}/assignment"))
            .header("content-type", "application/json"),
        Body::from(serde_json::json!({ "assigned_user_id": null }).to_string()),
    )
    .await;
}

#[tokio::test]
async fn set_stage_returns_503_when_database_unreachable() {
    let id = Uuid::new_v4();
    assert_returns_503(
        request("POST", &format!("/api/people/{id}/stage"))
            .header("content-type", "application/json"),
        Body::from(serde_json::json!({ "stage_id": Uuid::new_v4() }).to_string()),
    )
    .await;
}

// --- SLICE_011a §7, review R2: both new surfaces also 503 without a
// database (never a 422 that would misreport a valid filter as invalid) ---

#[tokio::test]
async fn list_people_with_filter_returns_503_when_database_unreachable() {
    let filter = serde_json::json!({"version": 1, "clauses": []}).to_string();
    let encoded: String = filter
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    assert_returns_503(
        request("GET", &format!("/api/people?filter={encoded}")),
        Body::empty(),
    )
    .await;
}

#[tokio::test]
async fn inquiry_sources_returns_503_when_database_unreachable() {
    assert_returns_503(request("GET", "/api/inquiry-sources"), Body::empty()).await;
}

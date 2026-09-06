//! Service-free boundary tests for the saved-list routes. DB-backed command,
//! privacy, and response tests live in `db_saved_lists.rs`; these pin the
//! extractor order that must hold even without a reachable database.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crm_api::config::Config;
use crm_api::state::AppState;

fn test_config() -> Config {
    let mut values = HashMap::new();
    values.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    values.insert("CRM_RAW_PAYLOAD_KEY".to_string(), "ab".repeat(32));
    values.insert(
        "CENTRIFUGO_HTTP_API_KEY".to_string(),
        "test-centrifugo-api-key".to_string(),
    );
    values.insert("CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(), "c".repeat(32));
    Config::from_source(move |key| values.get(key).cloned()).expect("valid test config")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn response(method: &str, uri: &str, body: Body) -> axum::response::Response {
    let app = crm_api::build_app(AppState::new(&test_config()).unwrap());
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body)
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn saved_list_index_requires_authentication_before_any_read() {
    let response = response("GET", "/api/saved-lists", Body::empty()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({ "error": "unauthenticated" })
    );
}

#[tokio::test]
async fn saved_list_mutations_require_authentication_before_body_parsing() {
    for (method, uri, body) in [
        ("POST", "/api/saved-lists", Body::from("not json")),
        (
            "PUT",
            "/api/saved-lists/11111111-1111-1111-1111-111111111111",
            Body::from("not json"),
        ),
        (
            "DELETE",
            "/api/saved-lists/11111111-1111-1111-1111-111111111111",
            Body::from("not json"),
        ),
    ] {
        let response = response(method, uri, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn malformed_saved_list_path_is_400_before_authentication() {
    for (method, uri) in [
        ("GET", "/api/saved-lists/not-a-uuid"),
        ("PUT", "/api/saved-lists/not-a-uuid"),
        ("DELETE", "/api/saved-lists/not-a-uuid"),
        ("GET", "/api/saved-lists/not-a-uuid/count?revision=1"),
    ] {
        let response = response(method, uri, Body::from("{}")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        assert_eq!(
            body_json(response).await,
            serde_json::json!({ "error": "malformed_request" })
        );
    }
}

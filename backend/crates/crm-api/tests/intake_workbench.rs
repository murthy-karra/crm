//! Service-free HTTP tests for the Slice 007e workbench endpoints
//! (docs/specs/SLICE_007e.md §11): 400 on a non-UUID id ahead of auth,
//! 401 without a cookie. A DB-down 503 is unreachable service-free
//! (authentication 401s first); the 403-member and 404 shapes need a
//! real session — all covered in tests/db_intake_workbench.rs.
use std::collections::HashMap;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::state::AppState;

fn test_config() -> Config {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    map.insert("CRM_RAW_PAYLOAD_KEY".to_string(), "ab".repeat(32));
    map.insert(
        "CENTRIFUGO_HTTP_API_KEY".to_string(),
        "test-centrifugo-api-key".to_string(),
    );
    map.insert("CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(), "c".repeat(32));
    Config::from_source(move |key| map.get(key).cloned()).expect("valid test config")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn endpoints(id: &str) -> [(&'static str, String); 3] {
    [
        ("GET", format!("/api/intake/unresolved/{id}")),
        ("POST", format!("/api/intake/unresolved/{id}/retry")),
        ("POST", format!("/api/intake/unresolved/{id}/discard")),
    ]
}

/// A non-UUID id is 400 `malformed_request` ahead of authentication
/// (the RawPayloadId extractor is declared before OrgAdminContext).
#[tokio::test]
async fn non_uuid_id_is_400_before_auth() {
    let state = AppState::new(&test_config()).unwrap();
    let app = crm_api::build_app(state);

    for (method, uri) in endpoints("not-a-uuid") {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        assert_eq!(
            body_json(response).await,
            serde_json::json!({ "error": "malformed_request" })
        );
    }
}

/// No session cookie → 401 on all three endpoints, before any DB work
/// (the state has no database at all).
#[tokio::test]
async fn workbench_endpoints_without_cookie_return_401() {
    let state = AppState::new(&test_config()).unwrap();
    let app = crm_api::build_app(state);
    let id = Uuid::new_v4();

    for (method, uri) in endpoints(&id.to_string()) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
        assert_eq!(
            body_json(response).await,
            serde_json::json!({ "error": "unauthenticated" })
        );
    }
}

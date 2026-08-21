use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

/// `{"error": "<code>"}` envelope shared across authenticated endpoints
/// (docs/specs/SLICE_001.md §4).
pub enum ApiError {
    MalformedRequest,
    InvalidCredentials,
    NoMembership,
    Unauthenticated,
    Unavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            ApiError::MalformedRequest => (StatusCode::BAD_REQUEST, "malformed_request"),
            ApiError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            ApiError::NoMembership => (StatusCode::FORBIDDEN, "no_membership"),
            ApiError::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            ApiError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
        (status, Json(json!({ "error": code }))).into_response()
    }
}

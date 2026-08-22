use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

use crate::domain::commands::CommandError;

/// `{"error": "<code>"}` envelope shared across authenticated endpoints
/// (docs/specs/SLICE_001.md §4). Slice 002 (docs/specs/SLICE_002.md §5)
/// adds variants additively; the envelope shape is unchanged.
pub enum ApiError {
    MalformedRequest,
    InvalidCredentials,
    NoMembership,
    Unauthenticated,
    Unavailable,
    NotFound,
    InvalidAssignee,
    InvalidStage,
    InternalError,
    /// `receive_inquiry`'s bounded retry around the per-Organization
    /// advisory lock exhausted its wall-clock budget without acquiring it
    /// (the cross-tenant pool-starvation fix). Distinct from `Unavailable`
    /// so a client can tell "this Organization's intake is contended,
    /// retry shortly" apart from "the database itself is down"; carries
    /// `Retry-After`.
    IntakeBusy,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, retry_after_secs) = match self {
            ApiError::MalformedRequest => (StatusCode::BAD_REQUEST, "malformed_request", None),
            ApiError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials", None),
            ApiError::NoMembership => (StatusCode::FORBIDDEN, "no_membership", None),
            ApiError::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated", None),
            ApiError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", None),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", None),
            ApiError::InvalidAssignee => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_assignee", None)
            }
            ApiError::InvalidStage => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_stage", None),
            ApiError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", None),
            ApiError::IntakeBusy => (StatusCode::SERVICE_UNAVAILABLE, "intake_busy", Some(2u64)),
        };

        let body = Json(json!({ "error": code }));
        match retry_after_secs {
            Some(secs) => (
                status,
                [(axum::http::header::RETRY_AFTER, secs.to_string())],
                body,
            )
                .into_response(),
            None => (status, body).into_response(),
        }
    }
}

/// Shared `CommandError` -> `ApiError` mapping for every route that runs a
/// typed command (docs/specs/SLICE_002.md §5, §9): `500 internal_error` is
/// reserved for decrypt failure and an Organization without stages; any
/// other database failure is `503 unavailable`, exactly as Slice 001.
impl From<CommandError> for ApiError {
    fn from(err: CommandError) -> Self {
        match err {
            CommandError::PersonNotFound => ApiError::NotFound,
            CommandError::InvalidAssignee => ApiError::InvalidAssignee,
            CommandError::InvalidStage => ApiError::InvalidStage,
            CommandError::NoStagesConfigured | CommandError::Crypto | CommandError::Corrupt => {
                ApiError::InternalError
            }
            CommandError::IntakeBusy => ApiError::IntakeBusy,
            CommandError::Database(_) => ApiError::Unavailable,
        }
    }
}

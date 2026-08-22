use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

use crate::domain::admin::AdminCommandError;
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
    // --- Slice 004 (docs/specs/SLICE_004.md §5) -------------------------
    Forbidden,
    LastAdmin,
    InvitationUsed,
    InvitationExpired,
    InvitationNotAcceptable,
    OrganizationNameTaken,
    WeakPassword,
    InvalidEmail,
    AlreadyMember,
    // --- Slice 005 (docs/specs/SLICE_005.md §5) -------------------------
    /// Server concurrency cap reached or the same user already has a turn
    /// in flight; carries `Retry-After: 2`.
    OperatorBusy,
    /// No inference provider configured (`GROQ_API_KEY` unset).
    OperatorDisabled,
    /// Provider timeout/error/rate-limit, the turn deadline, or a tool
    /// backend failure.
    OperatorUnavailable,
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
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None),
            ApiError::LastAdmin => (StatusCode::CONFLICT, "last_admin", None),
            ApiError::InvitationUsed => (StatusCode::CONFLICT, "invitation_used", None),
            ApiError::InvitationExpired => (StatusCode::GONE, "invitation_expired", None),
            ApiError::InvitationNotAcceptable => {
                (StatusCode::CONFLICT, "invitation_not_acceptable", None)
            }
            ApiError::OrganizationNameTaken => {
                (StatusCode::CONFLICT, "organization_name_taken", None)
            }
            ApiError::WeakPassword => (StatusCode::UNPROCESSABLE_ENTITY, "weak_password", None),
            ApiError::InvalidEmail => (StatusCode::BAD_REQUEST, "invalid_email", None),
            ApiError::AlreadyMember => (StatusCode::CONFLICT, "already_member", None),
            ApiError::OperatorBusy => (StatusCode::TOO_MANY_REQUESTS, "operator_busy", Some(2u64)),
            ApiError::OperatorDisabled => {
                (StatusCode::SERVICE_UNAVAILABLE, "operator_disabled", None)
            }
            ApiError::OperatorUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "operator_unavailable",
                None,
            ),
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

/// `AdminCommandError` -> `ApiError` mapping for the Slice 004 admin
/// commands (docs/specs/SLICE_004.md §5, §9).
impl From<AdminCommandError> for ApiError {
    fn from(err: AdminCommandError) -> Self {
        match err {
            AdminCommandError::NotFound => ApiError::NotFound,
            AdminCommandError::OrganizationNameTaken => ApiError::OrganizationNameTaken,
            AdminCommandError::InvalidEmail => ApiError::InvalidEmail,
            AdminCommandError::AlreadyMember => ApiError::AlreadyMember,
            AdminCommandError::InvitationUsed => ApiError::InvitationUsed,
            AdminCommandError::InvitationExpired => ApiError::InvitationExpired,
            AdminCommandError::InvitationNotAcceptable => ApiError::InvitationNotAcceptable,
            AdminCommandError::WeakPassword => ApiError::WeakPassword,
            AdminCommandError::MalformedRequest => ApiError::MalformedRequest,
            AdminCommandError::LastAdmin => ApiError::LastAdmin,
            AdminCommandError::Crypto | AdminCommandError::Corrupt => ApiError::InternalError,
            AdminCommandError::Database(_) => ApiError::Unavailable,
        }
    }
}

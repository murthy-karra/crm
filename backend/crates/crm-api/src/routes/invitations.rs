//! Public invitation routes (docs/specs/SLICE_004.md §5): no session: the
//! token is the credential. The token travels only in the JSON body, never
//! a URL segment the API serves — `/invite/:token` is a SPA route Lane B
//! reads `route.params` from and posts.

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::auth::session;
use crate::domain::admin::commands::accept_invitation as accept_invitation_command;
use crate::domain::admin::commands::token;
use crate::domain::admin::commands::AcceptInvitation;
use crate::domain::admin::queries as admin_queries;
use crate::domain::admin::queries::InvitationStatus;
use crate::domain::envelope::Origin;
use crate::error::ApiError;
use crate::routes::session::SessionResponse;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/invitations/preview", post(preview))
        .route("/api/invitations/accept", post(accept))
}

#[derive(Deserialize)]
struct PreviewRequest {
    token: String,
}

/// Token format is checked before any database access, exactly as
/// `session::is_valid_token_format` — a malformed token is 404, not 400,
/// so this endpoint leaks nothing about format (docs/specs/SLICE_004.md
/// §5).
async fn preview(
    State(state): State<AppState>,
    body: Result<Json<PreviewRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    if !token::is_valid_format(&req.token) {
        return Err(ApiError::NotFound);
    }
    let token_hash = token::hash(&req.token);

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let invitation = admin_queries::find_invitation_by_token_hash(&mut conn, &token_hash)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::NotFound)?;

    match invitation.status(Utc::now()) {
        InvitationStatus::Pending => Ok(Json(json!({
            "organization_name": invitation.organization_name,
            "email": invitation.email,
            "role": invitation.role,
            "expires_at": invitation.expires_at,
        }))),
        InvitationStatus::Expired => Err(ApiError::InvitationExpired),
        InvitationStatus::Accepted => Err(ApiError::InvitationUsed),
        // A revoked invitation is indistinguishable from an unknown one.
        InvitationStatus::Revoked => Err(ApiError::NotFound),
    }
}

#[derive(Deserialize)]
struct AcceptRequest {
    token: String,
    display_name: String,
    password: String,
}

/// Ignores any presented `crm_session` cookie and always mints a fresh
/// token (SLICE_001 §3 fixation rule); body identical to
/// `POST /api/session`'s success response (docs/specs/SLICE_004.md §5).
async fn accept(
    State(state): State<AppState>,
    body: Result<Json<AcceptRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let outcome = accept_invitation_command(
        pool,
        AcceptInvitation {
            token: req.token,
            display_name: req.display_name,
            password: req.password,
            origin: Origin::WebSession,
        },
    )
    .await?;

    let (token, _expires_at) = session::create(
        pool,
        &state.session_secret,
        outcome.user_id,
        Some(outcome.organization_id.as_uuid()),
        state.session_ttl,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let cookie = session::build_cookie(
        token,
        state.session_ttl,
        state.session_cookie_secure,
        state.session_cookie_domain.as_deref(),
    );
    let response_jar = CookieJar::new().add(cookie);

    let identity = session::SessionIdentity {
        user_id: outcome.user_id,
        email: outcome.email,
        display_name: outcome.display_name,
        organization: Some(session::SessionOrganization {
            id: outcome.organization_id.as_uuid(),
            name: outcome.organization_name,
            role: outcome.role,
        }),
        platform_admin: false,
    };
    let body = SessionResponse::from_identity(&identity);

    Ok((StatusCode::OK, response_jar, Json(body)).into_response())
}

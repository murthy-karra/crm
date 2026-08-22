use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{password, session, SessionContext};
use crate::domain::admin::Role;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/session", post(login).delete(logout))
        .route("/api/me", get(me))
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct UserPayload {
    id: Uuid,
    email: String,
    display_name: String,
}

#[derive(Serialize)]
struct OrganizationPayload {
    id: Uuid,
    name: String,
    role: Role,
}

/// Amended by docs/specs/SLICE_004.md §5 (declared change): `organization`
/// is nullable and gains `role`; `platform_admin` is added. `pub(crate)`
/// so the public invitation-accept route (`routes/invitations.rs`) can
/// build the identical body its own success response requires.
#[derive(Serialize)]
pub(crate) struct SessionResponse {
    user: UserPayload,
    organization: Option<OrganizationPayload>,
    platform_admin: bool,
}

impl SessionResponse {
    pub(crate) fn from_identity(identity: &session::SessionIdentity) -> Self {
        SessionResponse {
            user: UserPayload {
                id: identity.user_id,
                email: identity.email.clone(),
                display_name: identity.display_name.clone(),
            },
            organization: identity
                .organization
                .as_ref()
                .map(|org| OrganizationPayload {
                    id: org.id,
                    name: org.name.clone(),
                    role: org.role,
                }),
            platform_admin: identity.platform_admin,
        }
    }
}

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: Uuid,
    email: String,
    display_name: String,
    password_hash: String,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let credential = sqlx::query_as::<_, CredentialRow>(
        "SELECT u.id, u.email, u.display_name, c.password_hash
         FROM app_user u
         JOIN local_credential c ON c.user_id = u.id
         WHERE lower(u.email) = lower($1)",
    )
    .bind(&req.email)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    // Verify against a dummy hash when no credential row exists (unknown
    // user, or an existing user without a local password) so failure
    // timing never reveals which case occurred (docs/specs/SLICE_001.md
    // §3). Argon2id is deliberately expensive (tens of ms); running it via
    // spawn_blocking keeps it off the async runtime's worker threads so a
    // burst of login attempts can't stall unrelated concurrent requests.
    let credential = match credential {
        Some(credential) => {
            let candidate = req.password.clone();
            let hash = credential.password_hash.clone();
            let is_valid =
                tokio::task::spawn_blocking(move || password::verify_password(&candidate, &hash))
                    .await
                    .map_err(|_| ApiError::Unavailable)?;
            if !is_valid {
                return Err(ApiError::InvalidCredentials);
            }
            credential
        }
        None => {
            let candidate = req.password.clone();
            tokio::task::spawn_blocking(move || password::verify_dummy_password(&candidate))
                .await
                .map_err(|_| ApiError::Unavailable)?;
            return Err(ApiError::InvalidCredentials);
        }
    };

    // Earliest *active* membership only (docs/specs/SLICE_004.md §3: an
    // inactive membership is not a membership for login purposes).
    let membership = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT o.id, o.name, m.role
         FROM organization_membership m
         JOIN organization o ON o.id = m.organization_id
         WHERE m.user_id = $1 AND m.status = 'active'
         ORDER BY m.created_at, m.organization_id
         LIMIT 1",
    )
    .bind(credential.id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let active_organization = match membership {
        Some((organization_id, organization_name, role_str)) => {
            let role = Role::from_db_str(&role_str).ok_or(ApiError::Unavailable)?;
            Some((organization_id, organization_name, role))
        }
        None => {
            // No active membership: a platform admin logs in with no
            // active Organization; everyone else is 403 `no_membership`
            // (docs/specs/SLICE_004.md §3, the SLICE_001 §3 declared
            // change).
            let is_platform_admin =
                crate::domain::admin::queries::is_platform_admin(pool, credential.id)
                    .await
                    .map_err(|_| ApiError::Unavailable)?;
            if !is_platform_admin {
                return Err(ApiError::NoMembership);
            }
            None
        }
    };

    let (token, _expires_at) = session::create(
        pool,
        &state.session_secret,
        credential.id,
        active_organization.as_ref().map(|(id, _, _)| *id),
        state.session_ttl,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;

    // Session fixation: revoke the previously presented session, but only
    // now that login has actually succeeded, and only if it belonged to
    // this same user — a token for a different account must never be
    // revocable by presenting it on someone else's login request.
    if let Some(existing) = jar.get(session::COOKIE_NAME) {
        if let Ok(Some(previous)) =
            session::verify(pool, &state.session_secret, existing.value()).await
        {
            if previous.user_id == credential.id {
                let _ =
                    session::revoke_by_token(pool, &state.session_secret, existing.value()).await;
            }
        }
    }

    let cookie = session::build_cookie(
        token,
        state.session_ttl,
        state.session_cookie_secure,
        state.session_cookie_domain.as_deref(),
    );
    let response_jar = CookieJar::new().add(cookie);

    // A platform admin who also holds a membership logs in as that member
    // *and* is `platform_admin: true` — session::verify recomputes both on
    // every request, but the login response is built here from what we
    // already know rather than a redundant verify() round-trip.
    let platform_admin = active_organization.is_none()
        || crate::domain::admin::queries::is_platform_admin(pool, credential.id)
            .await
            .map_err(|_| ApiError::Unavailable)?;

    let identity =
        session::SessionIdentity {
            user_id: credential.id,
            email: credential.email,
            display_name: credential.display_name,
            organization: active_organization
                .map(|(id, name, role)| session::SessionOrganization { id, name, role }),
            platform_admin,
        };

    let body = SessionResponse::from_identity(&identity);

    Ok((StatusCode::OK, response_jar, Json(body)).into_response())
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<Response, ApiError> {
    if let Some(cookie) = jar.get(session::COOKIE_NAME) {
        let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
        // Not cleared on failure: the user must not be told they are
        // logged out while the token stays valid server-side (spec §4).
        session::revoke_by_token(pool, &state.session_secret, cookie.value())
            .await
            .map_err(|_| ApiError::Unavailable)?;
    }

    let clearing = session::build_clearing_cookie(
        state.session_cookie_secure,
        state.session_cookie_domain.as_deref(),
    );
    let response_jar = CookieJar::new().add(clearing);
    Ok((StatusCode::NO_CONTENT, response_jar).into_response())
}

/// Unlike every tenant route, `/api/me` must reflect a platform-only
/// session's shape (`organization: null`) rather than 401
/// (docs/specs/SLICE_004.md §5) — so it takes `SessionContext`, not
/// `AuthContext`.
async fn me(SessionContext(identity): SessionContext) -> Json<SessionResponse> {
    Json(SessionResponse::from_identity(&identity))
}

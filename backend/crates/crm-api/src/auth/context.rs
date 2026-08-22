use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use uuid::Uuid;

use crate::auth::session;
use crate::domain::admin::Role;
use crate::error::ApiError;
use crate::state::AppState;

/// Resolves the caller's session identity from the cookie, rejecting with
/// 401 before touching the database on a syntactically invalid cookie, and
/// with 503/401 as `session::verify` dictates otherwise. Shared by every
/// extractor below (docs/specs/SLICE_004.md §3).
async fn resolve_session(
    parts: &mut Parts,
    state: &AppState,
) -> Result<session::SessionIdentity, ApiError> {
    let jar = CookieJar::from_headers(&parts.headers);
    let token = jar
        .get(session::COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
        .ok_or(ApiError::Unauthenticated)?;

    if !session::is_valid_token_format(&token) {
        return Err(ApiError::Unauthenticated);
    }

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    session::verify(pool, &state.session_secret, &token)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::Unauthenticated)
}

/// The trusted actor and active Organization for this request, derived
/// entirely server-side from the session cookie. Handlers take this as a
/// parameter and never see the cookie; an Organization ID never enters a
/// query from client input (AGENTS.md §4.2).
///
/// Requires an active Organization (docs/specs/SLICE_004.md §3): a
/// platform-only session (no Organization) gets 401 `unauthenticated` here,
/// so every existing tenant route fails closed without modification.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub actor_user_id: Uuid,
    pub actor_email: String,
    pub actor_display_name: String,
    pub active_organization_id: Uuid,
    pub active_organization_name: String,
    pub role: Role,
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = resolve_session(parts, state).await?;
        let organization = identity.organization.ok_or(ApiError::Unauthenticated)?;

        Ok(AuthContext {
            actor_user_id: identity.user_id,
            actor_email: identity.email,
            actor_display_name: identity.display_name,
            active_organization_id: organization.id,
            active_organization_name: organization.name,
            role: organization.role,
        })
    }
}

/// Wraps `AuthContext` and rejects 403 `forbidden` unless `role == admin`
/// (docs/specs/SLICE_004.md §3). Organization-admin routes take this
/// extractor; the Organization comes from the session, never the client.
#[derive(Debug, Clone)]
pub struct OrgAdminContext {
    pub auth: AuthContext,
}

impl FromRequestParts<AppState> for OrgAdminContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;
        if auth.role != Role::Admin {
            return Err(ApiError::Forbidden);
        }
        Ok(OrgAdminContext { auth })
    }
}

/// Any valid session, Organization present or not, platform admin or not —
/// used only by `GET /api/me`, which must reflect a platform-only
/// session's shape (`organization: null`) rather than 401 like every
/// tenant route (docs/specs/SLICE_004.md §5).
#[derive(Debug, Clone)]
pub struct SessionContext(pub session::SessionIdentity);

impl FromRequestParts<AppState> for SessionContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(SessionContext(resolve_session(parts, state).await?))
    }
}

/// Requires a valid session and a `platform_admin` row, ignoring the
/// session's active Organization entirely (docs/specs/SLICE_004.md §3).
/// Platform handlers take the target Organization id only from the path
/// (§7) — never from this context.
#[derive(Debug, Clone)]
pub struct PlatformAuthContext {
    pub actor_user_id: Uuid,
    pub actor_email: String,
    pub actor_display_name: String,
}

impl FromRequestParts<AppState> for PlatformAuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = resolve_session(parts, state).await?;
        if !identity.platform_admin {
            return Err(ApiError::Forbidden);
        }
        Ok(PlatformAuthContext {
            actor_user_id: identity.user_id,
            actor_email: identity.email,
            actor_display_name: identity.display_name,
        })
    }
}

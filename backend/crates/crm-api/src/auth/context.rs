use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use uuid::Uuid;

use crate::auth::session;
use crate::error::ApiError;
use crate::state::AppState;

/// The trusted actor and active Organization for this request, derived
/// entirely server-side from the session cookie. Handlers take this as a
/// parameter and never see the cookie; an Organization ID never enters a
/// query from client input (AGENTS.md §4.2).
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub actor_user_id: Uuid,
    pub actor_email: String,
    pub actor_display_name: String,
    pub active_organization_id: Uuid,
    pub active_organization_name: String,
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(session::COOKIE_NAME)
            .map(|cookie| cookie.value().to_string())
            .ok_or(ApiError::Unauthenticated)?;

        // Reject a syntactically invalid cookie before touching the
        // database (docs/specs/SLICE_001.md §9).
        if !session::is_valid_token_format(&token) {
            return Err(ApiError::Unauthenticated);
        }

        let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

        let identity = session::verify(pool, &state.session_secret, &token)
            .await
            .map_err(|_| ApiError::Unavailable)?
            .ok_or(ApiError::Unauthenticated)?;

        Ok(AuthContext {
            actor_user_id: identity.user_id,
            actor_email: identity.email,
            actor_display_name: identity.display_name,
            active_organization_id: identity.organization_id,
            active_organization_name: identity.organization_name,
        })
    }
}

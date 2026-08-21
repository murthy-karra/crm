use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{password, session, AuthContext};
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
}

#[derive(Serialize)]
struct SessionResponse {
    user: UserPayload,
    organization: OrganizationPayload,
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

    let membership = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT o.id, o.name
         FROM organization_membership m
         JOIN organization o ON o.id = m.organization_id
         WHERE m.user_id = $1
         ORDER BY m.created_at, m.organization_id
         LIMIT 1",
    )
    .bind(credential.id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unavailable)?;

    let Some((organization_id, organization_name)) = membership else {
        return Err(ApiError::NoMembership);
    };

    let (token, _expires_at) = session::create(
        pool,
        &state.session_secret,
        credential.id,
        organization_id,
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

    let cookie = session::build_cookie(token, state.session_ttl, state.session_cookie_secure);
    let response_jar = CookieJar::new().add(cookie);

    let body = SessionResponse {
        user: UserPayload {
            id: credential.id,
            email: credential.email,
            display_name: credential.display_name,
        },
        organization: OrganizationPayload {
            id: organization_id,
            name: organization_name,
        },
    };

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

    let clearing = session::build_clearing_cookie(state.session_cookie_secure);
    let response_jar = CookieJar::new().add(clearing);
    Ok((StatusCode::NO_CONTENT, response_jar).into_response())
}

async fn me(auth: AuthContext) -> Json<SessionResponse> {
    Json(SessionResponse {
        user: UserPayload {
            id: auth.actor_user_id,
            email: auth.actor_email,
            display_name: auth.actor_display_name,
        },
        organization: OrganizationPayload {
            id: auth.active_organization_id,
            name: auth.active_organization_name,
        },
    })
}

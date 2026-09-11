//! Whole-response workspace read permits, separate from session presentation.
use crate::{
    auth::{workspace, AuthContext, OrgAdminContext},
    error::ApiError,
    state::AppState,
};
use axum::extract::{FromRequestParts, MatchedPath, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let protects = path.starts_with("/api/")
        && !matches!(path, "/api/me" | "/api/login" | "/api/logout")
        && !path.starts_with("/api/platform")
        && !path.starts_with("/api/invitations");
    if !protects {
        return next.run(request).await;
    }
    let read = request.method() == Method::GET;
    let operational = path.starts_with("/api/today")
        || path.starts_with("/api/realtime")
        || path.starts_with("/api/operator");
    // These routes already require OrgAdminContext. Preserve their forbidden
    // actor response before the generic review-mode rejection for member reads.
    // Match server-owned route templates, not arbitrary source/resource text.
    let admin_read = read
        && request
            .extensions()
            .get::<MatchedPath>()
            .is_some_and(|route| {
                let route = route.as_str();
                route.starts_with("/api/migrations/fub/")
                    || matches!(
                        route,
                        "/api/people/{id}/import-provenance"
                            | "/api/people/{id}/import-provenance/fields/{field}"
                    )
            });
    let (mut parts, body) = request.into_parts();
    let auth = if admin_read {
        OrgAdminContext::from_request_parts(&mut parts, &state)
            .await
            .map(|admin| admin.auth)
    } else {
        AuthContext::from_request_parts(&mut parts, &state).await
    };
    let request = Request::from_parts(parts, body);
    let auth = match auth {
        Ok(auth) => auth,
        Err(ApiError::Forbidden) if admin_read => return ApiError::Forbidden.into_response(),
        // Leave malformed-path and missing-session precedence to existing handlers.
        Err(_) => return next.run(request).await,
    };
    if !read {
        return workspace::with_reader(&auth, next.run(request)).await;
    }
    let Some(pool) = state.db.as_ref() else {
        return ApiError::Unavailable.into_response();
    };
    if pool.options().get_max_connections() < 2 {
        return ApiError::Unavailable.into_response();
    }
    let Ok(Ok(_slot)) =
        tokio::time::timeout(workspace::WAIT, state.workspace_read_slots.acquire()).await
    else {
        return ApiError::Unavailable.into_response();
    };
    let Ok(Ok(mut tx)) = tokio::time::timeout(workspace::WAIT, pool.begin()).await else {
        return ApiError::Unavailable.into_response();
    };
    if let Err(error) = workspace::read_check(
        &mut tx,
        auth.active_organization_id,
        auth.actor_user_id,
        operational,
    )
    .await
    {
        return ApiError::database(error).into_response();
    }
    let response = workspace::with_reader(&auth, next.run(request)).await;
    // Body data is now loaded; no guard spans response streaming or a browser.
    if tx.rollback().await.is_err() {
        return ApiError::Unavailable.into_response();
    }
    response
}

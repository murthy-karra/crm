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
    let mobile = request.uri().path().starts_with("/api/mobile/v1/");
    let family_refresh = request
        .uri()
        .path()
        .starts_with("/api/migrations/fub/family-refreshes");
    let history_versions = request
        .extensions()
        .get::<MatchedPath>()
        .is_some_and(|route| {
            matches!(
                route.as_str(),
                "/api/people/{id}/history/{kind}/{identity}/versions"
                    | "/api/people/{id}/history/{kind}/{identity}/versions/{version}"
            )
        });
    let no_store = family_refresh
        || history_versions
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/activity-imports")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/history-captures")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/history-imports")
        || mobile
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/core-change-reports")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/people-refreshes")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/people-admissions")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/admitted-people-refreshes")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/admitted-metadata-imports")
        || request
            .uri()
            .path()
            .starts_with("/api/migrations/fub/admitted-activity-imports")
        || request.uri().path().starts_with("/api/people/")
            && (request.uri().path().contains("/admission-provenance")
                || request
                    .uri()
                    .path()
                    .contains("/admitted-metadata-import-provenance"))
        || request.uri().path().starts_with("/api/people/")
            && request.uri().path().contains("/migration-review");
    // Include session extraction and workspace admission in the native request
    // budget, since both happen before its route middleware is entered.
    let mut response = if mobile {
        match tokio::time::timeout(
            std::time::Duration::from_secs(20),
            guard_inner(State(state), request, next),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => ApiError::Unavailable.into_response(),
        }
    } else {
        guard_inner(State(state), request, next).await
    };
    if no_store {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}
async fn guard_inner(State(state): State<AppState>, request: Request, next: Next) -> Response {
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
                        "/api/people/{id}/migration-review"
                            | "/api/people/{id}/migration-review/v2"
                            | "/api/people/{id}/migration-review/inquiries"
                            | "/api/people/{id}/migration-review/timeline"
                            | "/api/people/{id}/migration-review/timeline/{kind}/{entry_id}"
                            | "/api/people/{id}/history/{kind}/{identity}/versions"
                            | "/api/people/{id}/history/{kind}/{identity}/versions/{version}"
                            | "/api/people/{id}/migration-review/notes"
                            | "/api/people/{id}/migration-review/notes/{note_id}"
                            | "/api/people/{id}/migration-review/tasks"
                            | "/api/people/{id}/admission-provenance"
                            | "/api/people/{id}/admission-provenance/contacts"
                            | "/api/people/{id}/admission-provenance/fields/{field}"
                            | "/api/people/{id}/import-provenance"
                            | "/api/people/{id}/import-provenance/fields/{field}"
                            | "/api/people/{person}/admitted-metadata-import-provenance"
                            | "/api/people/{person}/admitted-metadata-import-provenance/{result}/fields/{field}"
                            | "/api/people/{id}/metadata-import-provenance"
                            | "/api/people/{id}/metadata-import-provenance/{result}/fields/{field}"
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

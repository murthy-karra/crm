//! Organization-admin routes (docs/specs/SLICE_004.md §5): members
//! list/role/status, and invitation issue/list/revoke. Organization comes
//! solely from the session (`AuthContext`/`OrgAdminContext`), never from
//! the client.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::{AuthContext, OrgAdminContext};
use crate::domain::admin::commands::{
    change_member_role, issue_invitation, revoke_invitation, set_member_status, ChangeMemberRole,
    IssueInvitation, RevokeInvitation, SetMemberStatus,
};
use crate::domain::admin::queries as admin_queries;
use crate::domain::admin::{AdminActor, MembershipStatus, Role};
use crate::domain::envelope::Origin;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/organization/members", get(members))
        .route("/api/organization/members/{user_id}/role", put(update_role))
        .route(
            "/api/organization/members/{user_id}/status",
            put(update_status),
        )
        .route(
            "/api/organization/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route(
            "/api/organization/invitations/{id}",
            delete(revoke_invitation_route),
        )
}

/// A `{user_id}`/`{id}` path segment parsed as a UUID, rejecting straight
/// to `400 malformed_request` rather than axum's default `PathRejection`
/// body — the same pattern as `routes/people.rs`'s `PersonId`. Used as a
/// bare (non-`Result`-wrapped) extractor, listed before the auth extractor
/// so a malformed id short-circuits ahead of authentication.
struct UuidPathId(Uuid);

impl FromRequestParts<AppState> for UuidPathId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(UuidPathId(id))
    }
}

/// `auth.active_organization_id` is the only Organization selector here —
/// it comes solely from the server-verified session (AGENTS.md §4.2), never
/// from a client-supplied query string, header, or body field. Any member
/// may read (unchanged from SLICE_001 §4); `role`/`status`/
/// `assigned_people_count` are additive (docs/specs/SLICE_004.md §5).
async fn members(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let members = admin_queries::members(&mut conn, auth.active_organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "members": members })))
}

#[derive(Deserialize)]
struct RoleBody {
    role: Role,
}

async fn update_role(
    State(state): State<AppState>,
    UuidPathId(user_id): UuidPathId,
    ctx: OrgAdminContext,
    body: Result<Json<RoleBody>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.auth.actor_user_id,
        origin: Origin::WebSession,
    };
    let member = change_member_role(
        pool,
        actor,
        ChangeMemberRole {
            organization_id: ctx.auth.active_organization_id,
            user_id,
            role: req.role,
        },
    )
    .await?;

    Ok(Json(json!({ "member": member })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: MembershipStatus,
}

async fn update_status(
    State(state): State<AppState>,
    UuidPathId(user_id): UuidPathId,
    ctx: OrgAdminContext,
    body: Result<Json<StatusBody>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.auth.actor_user_id,
        origin: Origin::WebSession,
    };
    let member = set_member_status(
        pool,
        &state.publisher,
        actor,
        SetMemberStatus {
            organization_id: ctx.auth.active_organization_id,
            user_id,
            status: req.status,
        },
    )
    .await?;

    Ok(Json(json!({ "member": member })))
}

async fn list_invitations(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let invitations = admin_queries::list_invitations(&mut conn, ctx.auth.active_organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "invitations": invitations })))
}

#[derive(Deserialize)]
struct IssueInvitationBody {
    email: String,
    role: Role,
}

async fn create_invitation(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
    body: Result<Json<IssueInvitationBody>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.auth.actor_user_id,
        origin: Origin::WebSession,
    };
    let outcome = issue_invitation(
        pool,
        actor,
        &ctx.auth.actor_display_name,
        state.invitation_ttl,
        IssueInvitation {
            organization_id: ctx.auth.active_organization_id,
            email: req.email,
            role: req.role,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "invitation": outcome.invitation,
            "accept_path": outcome.accept_path,
        })),
    )
        .into_response())
}

async fn revoke_invitation_route(
    State(state): State<AppState>,
    UuidPathId(invitation_id): UuidPathId,
    ctx: OrgAdminContext,
) -> Result<StatusCode, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.auth.actor_user_id,
        origin: Origin::WebSession,
    };
    revoke_invitation(
        pool,
        actor,
        RevokeInvitation {
            organization_id: ctx.auth.active_organization_id,
            invitation_id,
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

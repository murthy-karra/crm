//! Platform routes (docs/specs/SLICE_004.md §5, §7): `PlatformAuthContext`
//! only; the Organization id comes from the path, never the session's
//! active Organization. Platform scope is exactly: create Organization,
//! list, read one Organization's members and invitations, promote to
//! admin, issue/revoke admin invitations. No platform route reads or
//! writes `person`, `inquiry`, `raw_payload`, `stage`, `contact_attempted`,
//! or any CRM fact.

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

use crate::auth::PlatformAuthContext;
use crate::domain::admin::commands::{
    change_member_role, create_organization, issue_invitation, revoke_invitation, ChangeMemberRole,
    CreateOrganization, IssueInvitation, RevokeInvitation,
};
use crate::domain::admin::queries as admin_queries;
use crate::domain::admin::{AdminActor, Role};
use crate::domain::envelope::Origin;
use crate::domain::intake::IntakeAddress;
use crate::error::ApiError;
use crate::ids::{OrganizationId, UserId};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/platform/organizations",
            get(list_organizations).post(create_organization_route),
        )
        .route(
            "/api/platform/organizations/{id}",
            get(get_organization_detail),
        )
        .route(
            "/api/platform/organizations/{id}/members/{user_id}/role",
            put(promote_member),
        )
        .route(
            "/api/platform/organizations/{id}/invitations",
            axum::routing::post(create_admin_invitation),
        )
        .route(
            "/api/platform/organizations/{id}/invitations/{inv_id}",
            delete(revoke_admin_invitation),
        )
}

/// A path segment parsed as a UUID, rejecting straight to `400
/// malformed_request` — the same pattern as `routes/organization.rs`'s
/// `UuidPathId`.
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

/// Two UUID path segments (`{id}/members/{user_id}/...`), same rejection
/// behavior as `UuidPathId`.
struct TwoUuidPathIds(Uuid, Uuid);

impl FromRequestParts<AppState> for TwoUuidPathIds {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path((a, b)) = Path::<(Uuid, Uuid)>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(TwoUuidPathIds(a, b))
    }
}

async fn list_organizations(
    State(state): State<AppState>,
    _ctx: PlatformAuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let organizations = admin_queries::list_for_platform(&mut conn)
        .await
        .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(json!({ "organizations": organizations })))
}

#[derive(Deserialize)]
struct CreateOrganizationBody {
    name: String,
}

async fn create_organization_route(
    State(state): State<AppState>,
    ctx: PlatformAuthContext,
    body: Result<Json<CreateOrganizationBody>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.actor_user_id,
        origin: Origin::Platform,
    };
    let organization =
        create_organization(pool, actor, CreateOrganization { name: req.name }).await?;

    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let item = admin_queries::platform_organization_by_id(&mut conn, organization.id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::Unavailable)?;

    Ok((StatusCode::CREATED, Json(json!({ "organization": item }))).into_response())
}

async fn get_organization_detail(
    State(state): State<AppState>,
    UuidPathId(organization_id): UuidPathId,
    _ctx: PlatformAuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    // The path segment is the org-id entry point for platform routes
    // (hardening chunk N1) — a platform admin explicitly targets an
    // Organization by id, never the session's active one (module doc).
    let organization_id = OrganizationId::new(organization_id);
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let organization = admin_queries::platform_organization_by_id(&mut conn, organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::NotFound)?;
    let members = admin_queries::members(&mut conn, organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    let invitations = admin_queries::list_invitations(&mut conn, organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    // Slice 007a: an onboarding-configuration value (not tenant CRM data —
    // a recorded exclusion to D-021), top-level so the list stays untouched.
    let intake_address = admin_queries::organization_intake_address(&mut conn, organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .map(|(slug, token)| IntakeAddress { slug, token }.render(&state.intake_mail))
        .ok_or(ApiError::Unavailable)?;

    Ok(Json(json!({
        "organization": organization,
        "members": members,
        "invitations": invitations,
        "intake_address": intake_address,
    })))
}

#[derive(Deserialize)]
struct PromoteBody {
    role: Role,
}

/// **Only** `admin` is accepted (D-026 §4): a `member` value never reaches
/// the domain layer, the route rejects it (docs/specs/SLICE_004.md §5).
async fn promote_member(
    State(state): State<AppState>,
    TwoUuidPathIds(organization_id, user_id): TwoUuidPathIds,
    ctx: PlatformAuthContext,
    body: Result<Json<PromoteBody>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    if req.role != Role::Admin {
        return Err(ApiError::MalformedRequest);
    }
    let organization_id = OrganizationId::new(organization_id);
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.actor_user_id,
        origin: Origin::Platform,
    };
    let member = change_member_role(
        pool,
        actor,
        ChangeMemberRole {
            organization_id,
            user_id: UserId::new(user_id),
            role: Role::Admin,
        },
    )
    .await?;

    Ok(Json(json!({ "member": member })))
}

#[derive(Deserialize)]
struct PlatformInviteBody {
    email: String,
    role: Role,
}

/// **Only** `admin` is accepted (D-021 §1, D-026 §4: the platform power is
/// admin continuity) — docs/specs/SLICE_004.md §5.
async fn create_admin_invitation(
    State(state): State<AppState>,
    UuidPathId(organization_id): UuidPathId,
    ctx: PlatformAuthContext,
    body: Result<Json<PlatformInviteBody>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    if req.role != Role::Admin {
        return Err(ApiError::MalformedRequest);
    }
    let organization_id = OrganizationId::new(organization_id);
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    // The FK on invitation.organization_id would otherwise turn a
    // nonexistent Organization into an opaque database error rather than
    // a clean 404 (docs/specs/SLICE_004.md §7).
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    if !admin_queries::organization_exists(&mut conn, organization_id)
        .await
        .map_err(|_| ApiError::Unavailable)?
    {
        return Err(ApiError::NotFound);
    }

    let actor = AdminActor {
        actor_user_id: ctx.actor_user_id,
        origin: Origin::Platform,
    };
    let outcome = issue_invitation(
        pool,
        actor,
        &ctx.actor_display_name,
        state.invitation_ttl,
        IssueInvitation {
            organization_id,
            email: req.email,
            role: Role::Admin,
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

async fn revoke_admin_invitation(
    State(state): State<AppState>,
    TwoUuidPathIds(organization_id, invitation_id): TwoUuidPathIds,
    ctx: PlatformAuthContext,
) -> Result<StatusCode, ApiError> {
    let organization_id = OrganizationId::new(organization_id);
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let actor = AdminActor {
        actor_user_id: ctx.actor_user_id,
        origin: Origin::Platform,
    };
    revoke_invitation(
        pool,
        actor,
        RevokeInvitation {
            organization_id,
            invitation_id,
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

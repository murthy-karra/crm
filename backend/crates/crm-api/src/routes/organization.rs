//! Organization-admin routes (docs/specs/SLICE_004.md §5): members
//! list/role/status, and invitation issue/list/revoke. Organization comes
//! solely from the session (`AuthContext`/`OrgAdminContext`), never from
//! the client.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post, put};
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
use crate::domain::envelope::CommandContext;
use crate::domain::envelope::Origin;
use crate::domain::intake::IntakeAddress;
use crate::error::ApiError;
use crate::ids::UserId;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/organization/members", get(members))
        .route("/api/organization/intake-address", get(intake_address))
        .route(
            "/api/organization/intake-address/rotate",
            post(rotate_intake_address),
        )
        .route(
            "/api/organization/intake-settings",
            get(intake_settings).put(update_intake_settings),
        )
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
            user_id: UserId::new(user_id),
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
            user_id: UserId::new(user_id),
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

/// `GET /api/organization/intake-address` (docs/specs/SLICE_007a.md §5):
/// the Organization's rendered intake address. Org admins only — the
/// token in it is the anti-forgery secret. Rendered from
/// `state.intake_mail`, never stored as text.
/// `POST /api/organization/intake-address/rotate`
/// (docs/specs/SLICE_007g.md §5): break-glass rotation, admin-only
/// (D-037/7a). Returns the NEW address in the GET shape. The span
/// records ids only — never token material.
#[tracing::instrument(
    name = "intake.rotate_token",
    skip_all,
    fields(
        organization_id = tracing::field::Empty,
        actor_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn rotate_intake_address(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let span = tracing::Span::current();
    span.record(
        "organization_id",
        tracing::field::display(ctx.auth.active_organization_id),
    );
    span.record("actor_id", tracing::field::display(ctx.auth.actor_user_id));

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    // The slug is immutable — read it BEFORE rotating so nothing after
    // the commit can fail and misreport the (already-effective)
    // rotation.
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let (slug, _old_token) =
        admin_queries::organization_intake_address(&mut conn, ctx.auth.active_organization_id)
            .await
            .map_err(|_| ApiError::Unavailable)?
            .ok_or(ApiError::Unavailable)?;
    drop(conn);

    let command_ctx = CommandContext::from_auth(&ctx.auth);
    let new_token = crate::domain::intake::rotate::rotate_intake_token(pool, &command_ctx)
        .await
        .map_err(|err| {
            span.record("outcome", err.kind());
            ApiError::Unavailable
        })?;
    span.record("outcome", "rotated");

    let address = IntakeAddress {
        slug,
        token: new_token,
    }
    .render(&state.intake_mail);
    Ok(Json(json!({
        "address": address,
        "scheme": state.intake_mail.scheme.as_str(),
    })))
}

/// `GET /api/organization/intake-address` (docs/specs/SLICE_007a.md §5):
/// admins only; renders the address from storage + config.
async fn intake_address(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let (slug, token) =
        admin_queries::organization_intake_address(&mut conn, ctx.auth.active_organization_id)
            .await
            .map_err(|_| ApiError::Unavailable)?
            .ok_or(ApiError::Unavailable)?;
    let address = IntakeAddress { slug, token }.render(&state.intake_mail);
    Ok(Json(json!({
        "address": address,
        "scheme": state.intake_mail.scheme.as_str(),
    })))
}

/// `GET /api/organization/intake-settings` (docs/specs/SLICE_007c.md §5):
/// the Organization's default assignee for unattended intake, whatever its
/// current membership status — the deactivated-warning state is computed
/// client-side from `GET /api/organization/members`, not duplicated here.
/// Org admins only.
async fn intake_settings(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let value =
        admin_queries::intake_default_assignee_user_id(&mut conn, ctx.auth.active_organization_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;
    Ok(Json(json!({ "intake_default_assignee_user_id": value })))
}

/// `PUT /api/organization/intake-settings` (docs/specs/SLICE_007c.md §5):
/// 422 `invalid_assignee` — byte-identical for a nonexistent user, another
/// Organization's member, and an inactive member (no existence leak) — is
/// checked before the write. Span records the Organization and whether the
/// value was set or cleared (§8) — the assignee UUID is an id, not
/// content, but is not itself recorded here.
///
/// The body is parsed as a raw `serde_json::Value`, not a struct with an
/// `Option<Uuid>` field: serde's derive implicitly defaults *any*
/// `Option<T>` field to `None` when its key is absent — there is no
/// attribute that turns that off for a field literally typed `Option<T>`
/// — which would silently equate "key omitted" with "explicit null" and
/// defeat the very distinction this endpoint's contract requires.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %ctx.auth.active_organization_id,
        assignee_action = tracing::field::Empty,
    )
)]
async fn update_intake_settings(
    State(state): State<AppState>,
    ctx: OrgAdminContext,
    body: Result<Json<serde_json::Value>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(body) = body.map_err(|_| ApiError::MalformedRequest)?;
    let raw = body
        .as_object()
        .and_then(|obj| obj.get("intake_default_assignee_user_id"))
        .ok_or(ApiError::MalformedRequest)?;
    let assignee_user_id: Option<UserId> = match raw {
        serde_json::Value::Null => None,
        other => {
            Some(serde_json::from_value(other.clone()).map_err(|_| ApiError::MalformedRequest)?)
        }
    };
    tracing::Span::current().record(
        "assignee_action",
        if assignee_user_id.is_some() {
            "set"
        } else {
            "cleared"
        },
    );
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    if let Some(user_id) = assignee_user_id {
        let is_active =
            admin_queries::is_active_member(&mut conn, ctx.auth.active_organization_id, user_id)
                .await
                .map_err(|_| ApiError::Unavailable)?;
        if !is_active {
            return Err(ApiError::InvalidAssignee);
        }
    }

    admin_queries::update_intake_default_assignee(
        &mut conn,
        ctx.auth.active_organization_id,
        assignee_user_id,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;

    Ok(Json(
        json!({ "intake_default_assignee_user_id": assignee_user_id }),
    ))
}

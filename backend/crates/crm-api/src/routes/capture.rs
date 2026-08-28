//! Correspondence capture routes (docs/specs/SLICE_009.md §8): member-
//! self (`AuthContext`, NOT admin — this is the agent's own credential,
//! deliberately not on the admin-surface `IntakeSettingsView`/
//! `organization.rs` routes). The held-queue routes are additionally
//! attributed-agent-only (D-042.3): `agent_user_id` always comes from the
//! session, never the client, and a foreign row 404s exactly like a
//! nonexistent one.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::capture::commands::{self, LinkUnmatched};
use crate::domain::capture::{address, store, Direction};
use crate::domain::envelope::CommandContext;
use crate::error::ApiError;
use crate::ids::{CaptureMessageId, PersonId};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/capture/address", get(capture_address))
        .route("/api/capture/address/rotate", post(rotate_capture_address))
        .route("/api/capture/unmatched", get(list_unmatched))
        .route("/api/capture/unmatched/{id}/link", post(link_unmatched))
        .route(
            "/api/capture/unmatched/{id}/dismiss",
            post(dismiss_unmatched),
        )
}

/// A `{id}` path segment parsed as a UUID and typed as `CaptureMessageId`
/// (hardening chunk N3 style), rejecting straight to 400
/// `malformed_request` — the `routes/people.rs` `PersonIdPath` pattern,
/// listed before the auth extractor so a non-UUID id short-circuits
/// ahead of authentication.
struct CaptureMessageIdPath(CaptureMessageId);

impl FromRequestParts<AppState> for CaptureMessageIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(CaptureMessageIdPath(CaptureMessageId::new(id)))
    }
}

fn direction_str(d: Direction) -> &'static str {
    d.as_str()
}

/// `GET /api/capture/address` (docs/specs/SLICE_009.md §8): renders like
/// intake's own GET. Self-healing mint-if-absent (defensive): every
/// active member should already have a row from backfill or
/// `AcceptInvitation`, but a read path here fails toward "mint one now"
/// rather than 503ing a legitimate agent for a state that should be
/// unreachable.
async fn capture_address(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let mut token =
        address::current_token(&mut conn, auth.active_organization_id, auth.actor_user_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;

    if token.is_none() {
        let mut tx = pool.begin().await.map_err(|_| ApiError::Unavailable)?;
        address::mint_capture_address_if_absent(
            &mut tx,
            auth.active_organization_id,
            auth.actor_user_id,
        )
        .await
        .map_err(|_| ApiError::Unavailable)?;
        tx.commit().await.map_err(|_| ApiError::Unavailable)?;
        token = address::current_token(&mut conn, auth.active_organization_id, auth.actor_user_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;
    }
    let token = token.ok_or(ApiError::Unavailable)?;

    Ok(Json(json!({ "address": token.render(&state.intake_mail) })))
}

/// `POST /api/capture/address/rotate` (docs/specs/SLICE_009.md §3, §8):
/// self-service, immediate invalidation — mirrors
/// `routes/organization.rs::rotate_intake_address`'s shape, member-self
/// instead of org-admin.
#[tracing::instrument(
    name = "capture.rotate_token",
    skip_all,
    fields(
        organization_id = %auth.active_organization_id,
        actor_id = %auth.actor_user_id,
        outcome = tracing::field::Empty,
    )
)]
async fn rotate_capture_address(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let span = tracing::Span::current();
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    let ctx = CommandContext::from_auth(&auth);
    let new_token = address::rotate_capture_token(pool, &ctx)
        .await
        .map_err(|err| {
            span.record("outcome", err.kind());
            ApiError::from(err)
        })?;
    span.record("outcome", "rotated");

    Ok(Json(
        json!({ "address": new_token.render(&state.intake_mail) }),
    ))
}

/// `GET /api/capture/unmatched` (docs/specs/SLICE_009.md §8): the
/// attributed agent's own held list only — `auth.actor_user_id`, never a
/// client-chosen agent id (D-042.3).
async fn list_unmatched(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;

    let (items, truncated) =
        store::list_unmatched(&mut conn, auth.active_organization_id, auth.actor_user_id)
            .await
            .map_err(|_| ApiError::Unavailable)?;

    let items: Vec<_> = items
        .into_iter()
        .map(|item| {
            json!({
                "id": item.id,
                "counterparty_email": item.counterparty_email,
                "captured_at": item.captured_at,
                "direction_hint": item.direction_hint.map(direction_str),
                "status": item.status.as_str(),
            })
        })
        .collect();

    Ok(Json(json!({ "items": items, "truncated": truncated })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkUnmatchedRequest {
    person_id: PersonId,
    #[serde(default)]
    add_contact_method: bool,
}

/// `POST /api/capture/unmatched/{id}/link` (docs/specs/SLICE_009.md §8).
async fn link_unmatched(
    id: CaptureMessageIdPath,
    State(state): State<AppState>,
    auth: AuthContext,
    body: Result<Json<LinkUnmatchedRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    commands::link_unmatched(
        pool,
        &state.raw_payload_key,
        &state.publisher,
        auth.active_organization_id,
        auth.actor_user_id,
        LinkUnmatched {
            id: id.0,
            person_id: req.person_id,
            add_contact_method: req.add_contact_method,
        },
    )
    .await?;

    Ok(Json(json!({ "status": "linked" })))
}

/// `POST /api/capture/unmatched/{id}/dismiss` (docs/specs/SLICE_009.md
/// §8): idempotent.
async fn dismiss_unmatched(
    id: CaptureMessageIdPath,
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;

    commands::dismiss_unmatched(pool, auth.active_organization_id, auth.actor_user_id, id.0)
        .await?;

    Ok(Json(json!({ "status": "dismissed" })))
}

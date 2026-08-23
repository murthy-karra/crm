//! `POST /webhooks/livekit` (docs/specs/SLICE_006.md §5, §7, §9): no
//! `AuthContext`, `DefaultBodyLimit::max(64 KiB)` as `routes/intake.rs`,
//! mounted outside the CORS layer (`lib.rs`). The signature is verified
//! with `LIVEKIT_API_SECRET`; the room `call:<id>` is resolved to a call
//! and its Organization server-side, so a webhook naming another
//! Organization's room cannot touch it. 200 `{}` for every valid
//! signature — unknown room or event included — and 401 `unauthenticated`
//! otherwise. The span records `event`, `room`, `outcome`; never the body,
//! the token, or participant attributes (LiveKit puts `sip.phoneNumber`
//! in them).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::HeaderMap;
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::domain::telephony::queries::{self as call_queries, CallRow};
use crate::domain::telephony::settle::transition_tag;
use crate::domain::telephony::{settle, Signal};
use crate::error::ApiError;
use crate::state::AppState;
use crate::telephony::Telephony;

const MAX_WEBHOOK_BODY_BYTES: usize = 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/webhooks/livekit",
        post(livekit_webhook).layer(DefaultBodyLimit::max(MAX_WEBHOOK_BODY_BYTES)),
    )
}

/// The three fields we read; everything else (participant attributes,
/// metadata, track info) is ignored and never deserialized.
#[derive(Deserialize)]
struct WebhookEvent {
    #[serde(default)]
    event: String,
    #[serde(default)]
    room: Option<WebhookRoom>,
    #[serde(default)]
    participant: Option<WebhookParticipant>,
}

#[derive(Deserialize)]
struct WebhookRoom {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct WebhookParticipant {
    #[serde(default)]
    identity: String,
}

/// Which participant a `participant_left` names, relative to the call it
/// will be matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Agent,
    Sip,
}

/// LiveKit event + participant identity → the signal *shape*, or `None`
/// for events this slice ignores (docs/specs/SLICE_006.md §2, §9). A
/// `participant_left` is only applied once the identity is matched to the
/// call's own `agent:<caller_user_id>` / `sip:<call_id>` (`signal_for_call`).
fn signal_for(event: &str, participant_identity: Option<&str>) -> Option<(Signal, Option<Who>)> {
    match event {
        "participant_left" => match participant_identity {
            Some(identity) if identity.starts_with("sip:") => {
                Some((Signal::RemoteLeft, Some(Who::Sip)))
            }
            Some(identity) if identity.starts_with("agent:") => {
                Some((Signal::AgentLeft, Some(Who::Agent)))
            }
            _ => None,
        },
        "room_finished" => Some((Signal::RoomFinished, None)),
        _ => None,
    }
}

/// The identity must be *this* call's agent or SIP participant; any other
/// identity (another member, a stray participant) is ignored.
fn identity_matches(who: Option<Who>, identity: Option<&str>, call: &CallRow) -> bool {
    match who {
        None => true,
        Some(Who::Agent) => {
            identity == Some(Telephony::agent_identity(call.caller_user_id).as_str())
        }
        Some(Who::Sip) => identity == Some(Telephony::sip_identity(call.id).as_str()),
    }
}

#[tracing::instrument(
    name = "call.webhook",
    skip_all,
    fields(
        event = tracing::field::Empty,
        room = tracing::field::Empty,
        call_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
async fn livekit_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let span = tracing::Span::current();

    let Some(telephony) = state.telephony.as_ref() else {
        span.record("outcome", "invalid_signature");
        tracing::warn!("livekit webhook received while telephony is disabled");
        return Err(ApiError::Unauthenticated);
    };
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Err(err) = telephony.webhook.verify(token, &body, Utc::now()) {
        span.record("outcome", "invalid_signature");
        tracing::warn!(reason = err.kind(), "livekit webhook signature rejected");
        return Err(ApiError::Unauthenticated);
    }

    // Signature valid from here: every outcome is 200 `{}`.
    let event: WebhookEvent = match serde_json::from_slice(&body) {
        Ok(event) => event,
        Err(_) => {
            span.record("outcome", "ignored");
            return Ok(Json(json!({})));
        }
    };
    span.record("event", event.event.as_str());
    let room = event.room.as_ref().map(|r| r.name.as_str()).unwrap_or("");
    span.record("room", room);

    let identity = event.participant.as_ref().map(|p| p.identity.as_str());
    let Some((signal, who)) = signal_for(&event.event, identity) else {
        span.record("outcome", "ignored");
        return Ok(Json(json!({})));
    };

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let mut conn = pool.acquire().await.map_err(|_| ApiError::Unavailable)?;
    let call = match call_queries::call_by_room(&mut conn, room).await {
        Ok(Some(call)) => call,
        Ok(None) => {
            span.record("outcome", "unknown_room");
            return Ok(Json(json!({})));
        }
        Err(_) => return Err(ApiError::Unavailable),
    };
    drop(conn);
    span.record("call_id", tracing::field::display(call.id));
    if !identity_matches(who, identity, &call) {
        span.record("outcome", "ignored");
        return Ok(Json(json!({})));
    }

    let outcome = settle(
        pool,
        &state.publisher,
        call.organization_id,
        call.id,
        &signal,
        Utc::now(),
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    match outcome {
        Some(outcome) if !outcome.is_noop() => {
            span.record("outcome", "applied");
            tracing::info!(
                transition = transition_tag(&outcome.transition),
                "livekit webhook applied"
            );
            // A terminal transition deletes the room best-effort so the
            // other side sees the disconnect — except on `room_finished`,
            // where the room is already gone.
            if outcome.transition.is_terminal() && !matches!(signal, Signal::RoomFinished) {
                if let Err(err) = telephony.provider.hangup(&call.provider_room).await {
                    tracing::warn!(
                        call_id = %call.id,
                        error_kind = err.kind(),
                        "room delete after webhook transition failed"
                    );
                }
            }
        }
        _ => {
            span.record("outcome", "ignored");
        }
    }
    Ok(Json(json!({})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_mapping() {
        assert_eq!(
            signal_for("participant_left", Some("sip:abc")),
            Some((Signal::RemoteLeft, Some(Who::Sip)))
        );
        assert_eq!(
            signal_for("participant_left", Some("agent:abc")),
            Some((Signal::AgentLeft, Some(Who::Agent)))
        );
        assert_eq!(signal_for("participant_left", Some("other")), None);
        assert_eq!(signal_for("participant_left", None), None);
        assert_eq!(
            signal_for("room_finished", None),
            Some((Signal::RoomFinished, None))
        );
        assert_eq!(signal_for("room_started", None), None);
        assert_eq!(signal_for("participant_joined", Some("sip:x")), None);
        assert_eq!(signal_for("track_published", Some("sip:x")), None);
    }
}

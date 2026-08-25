//! `POST /inbound/email` (docs/specs/SLICE_007b.md §5): its own router,
//! mounted outside CORS exactly like `routes/livekit_webhook.rs`, with a
//! per-route `DefaultBodyLimit::max(2 MiB)`. Order: an oversize body 413s
//! ahead of everything else (`Bytes` extraction fails before the handler's
//! own checks run — the accepted livekit-webhook-precedent deviation from
//! "bearer first" for this one case); then bearer (constant-time compare);
//! then JSON; then base64. Every rejection shape (unparseable recipient,
//! unknown slug, wrong token) collapses to the same 200
//! `{"status":"rejected"}` — no address-enumeration oracle (§8).

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use crate::domain::intake::{receive_inbound_email, InboundEmailOutcome, ReceiveInboundEmailError};
use crate::error::ApiError;
use crate::state::AppState;

const MAX_INBOUND_EMAIL_BODY_BYTES: usize = 2 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/inbound/email",
        post(inbound_email).layer(DefaultBodyLimit::max(MAX_INBOUND_EMAIL_BODY_BYTES)),
    )
}

#[derive(Deserialize)]
struct InboundEmailRequest {
    recipient: String,
    raw: String,
}

#[tracing::instrument(
    name = "intake.inbound_email",
    skip_all,
    fields(
        outcome = tracing::field::Empty,
        byte_len = tracing::field::Empty,
        raw_payload_id = tracing::field::Empty,
    )
)]
async fn inbound_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let span = tracing::Span::current();

    // Body-size failures win over everything else (§5, accepted
    // deviation): the body is already fully buffered (or failed to
    // buffer) before any of this function's own checks run.
    let body = body.map_err(|_| {
        span.record("outcome", "payload_too_large");
        ApiError::PayloadTooLarge
    })?;

    let Some(secret) = state.inbound_email_secret.as_ref() else {
        span.record("outcome", "unauthenticated");
        return Err(ApiError::Unauthenticated);
    };
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_time_eq(presented.as_bytes(), secret.as_bytes()) {
        span.record("outcome", "unauthenticated");
        return Err(ApiError::Unauthenticated);
    }

    // Never record the JsonRejection/base64-error Display text (§9,
    // criterion 13) — the `?`/`map_err` below always discard it.
    let req: InboundEmailRequest = serde_json::from_slice(&body).map_err(|_| {
        span.record("outcome", "malformed");
        ApiError::MalformedRequest
    })?;

    let raw = STANDARD.decode(req.raw.as_bytes()).map_err(|_| {
        span.record("outcome", "malformed");
        ApiError::MalformedRequest
    })?;
    if raw.is_empty() {
        span.record("outcome", "malformed");
        return Err(ApiError::MalformedRequest);
    }
    span.record("byte_len", raw.len());

    let Some(pool) = state.db.as_ref() else {
        span.record("outcome", "unavailable");
        return Err(ApiError::Unavailable);
    };

    let outcome = receive_inbound_email(
        pool,
        &state.raw_payload_key,
        &state.publisher,
        &state.intake_mail,
        &req.recipient,
        &raw,
    )
    .await;

    match outcome {
        Ok(InboundEmailOutcome::Stored { raw_payload_id }) => {
            span.record("outcome", "accepted");
            span.record("raw_payload_id", tracing::field::display(raw_payload_id));
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        Ok(InboundEmailOutcome::Duplicate) => {
            span.record("outcome", "duplicate");
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        Ok(InboundEmailOutcome::Rejected)
        | Err(ReceiveInboundEmailError::InvalidRecipient)
        | Err(ReceiveInboundEmailError::OrgNotFound) => {
            // One undifferentiated "rejected" even for the two Err
            // variants (§9): nothing recipient-derived leaks into the span.
            span.record("outcome", "rejected");
            Ok((StatusCode::OK, Json(json!({ "status": "rejected" }))).into_response())
        }
        Err(ReceiveInboundEmailError::Crypto) => {
            span.record("outcome", "crypto");
            Err(ApiError::InternalError)
        }
        Err(ReceiveInboundEmailError::Database(_)) => {
            span.record("outcome", "database");
            Err(ApiError::Unavailable)
        }
    }
}

/// Never `==` on secrets (repo-wide rule; copied from
/// `domain/intake/receive.rs`'s own copy of the same 10-line helper —
/// duplicated rather than shared per that module's own precedent).
fn constant_time_eq(presented: &[u8], stored: &[u8]) -> bool {
    if presented.len() != stored.len() {
        return false;
    }
    let mut result = 0u8;
    for (a, b) in presented.iter().zip(stored.iter()) {
        result |= a ^ b;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_rejects_length_mismatch_and_wrong_bytes() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"abcd"));
    }
}

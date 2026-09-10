//! `POST /inbound/email` (docs/specs/SLICE_007b.md §5): its own router,
//! mounted outside CORS exactly like `routes/livekit_webhook.rs`, with a
//! per-route `DefaultBodyLimit::max(34 MiB)` (raised from 2 MiB by
//! D-056/docs/specs/SLICE_017.md §3). Order: an oversize body 413s
//! ahead of everything else (`Bytes` extraction fails before the handler's
//! own checks run — the accepted livekit-webhook-precedent deviation from
//! "bearer first" for this one case); then bearer (constant-time compare);
//! then JSON; then base64. Every rejection shape (unparseable recipient,
//! unknown slug, wrong token) collapses to the same 200
//! `{"status":"rejected"}` — no address-enumeration oracle (§8).

use std::borrow::Cow;

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

// Cloudflare's Email Routing inbound ceiling is 25 MiB (D-056); base64
// inflates that by 4/3 (`4 * ceil(25 MiB / 3) = 34_952_536` bytes) and the
// JSON envelope adds a few KiB, so 34 MiB is the clean endpoint constant
// that admits everything the relay can legitimately send
// (docs/specs/SLICE_017.md §3; the derivation is pinned by
// `max_inbound_email_body_bytes_admits_the_relays_largest_legitimate_body`
// below).
const MAX_INBOUND_EMAIL_BODY_BYTES: usize = 34 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/inbound/email",
        post(inbound_email).layer(DefaultBodyLimit::max(MAX_INBOUND_EMAIL_BODY_BYTES)),
    )
}

#[derive(Deserialize)]
struct InboundEmailRequest<'a> {
    recipient: String,
    // Borrowed from the buffered body (docs/specs/SLICE_017.md §3):
    // serde_json borrows a `Cow::Borrowed` when the JSON string has no
    // escapes, which base64 never does, so the 34 MiB text is not copied
    // a second time here.
    #[serde(borrow)]
    raw: Cow<'a, str>,
}

#[tracing::instrument(
    name = "intake.inbound_email",
    skip_all,
    fields(
        outcome = tracing::field::Empty,
        byte_len = tracing::field::Empty,
        raw_payload_id = tracing::field::Empty,
        // SLICE_007d §8: the static format name (recorded by the email
        // parse closure only when a format matched), and the created ids
        // on a completed intake. Ids only — never subject, sender, body,
        // recipient, slug, or token.
        format = tracing::field::Empty,
        person_id = tracing::field::Empty,
        inquiry_id = tracing::field::Empty,
        // SLICE_007h1 §4: forwarded-wrapper provenance, recorded by the
        // parse closure — statics/scalars only, never inner content.
        forwarded = tracing::field::Empty,
        forward_style = tracing::field::Empty,
        forward_depth = tracing::field::Empty,
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
    //
    // LATER batch (2026-09-10) item 8: `req.raw` borrows from `body`
    // (`Cow<'a, str>`), so this block ends that borrow and produces
    // `raw: Vec<u8>` before `body` (the buffered base64 JSON, ~1.33x the
    // decoded size) and `req` are dropped — `receive_inbound_email`'s
    // Phase B holds the decoded ciphertext/plaintext/parsed-attachment
    // buffers concurrently and never needs the original request body
    // again. `recipient` is a plain `String` (never borrowed from `body`),
    // so moving it out of the block is free.
    let (recipient, raw) = {
        let req: InboundEmailRequest<'_> = serde_json::from_slice(&body).map_err(|_| {
            span.record("outcome", "malformed");
            ApiError::MalformedRequest
        })?;
        let raw = STANDARD.decode(req.raw.as_bytes()).map_err(|_| {
            span.record("outcome", "malformed");
            ApiError::MalformedRequest
        })?;
        (req.recipient, raw)
    };
    drop(body);
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
        &recipient,
        &raw,
    )
    .await;

    // Every stored outcome — completed, unresolved, deferred, duplicate —
    // is the same 200 `{"status":"accepted"}` (SLICE_007d §5: the frozen
    // 007b envelope reveals nothing about the parse outcome).
    match outcome {
        Ok(InboundEmailOutcome::Completed {
            person_id,
            inquiry_id,
            raw_payload_id,
        }) => {
            span.record("outcome", "completed");
            span.record("raw_payload_id", tracing::field::display(raw_payload_id));
            span.record("person_id", tracing::field::display(person_id));
            span.record("inquiry_id", tracing::field::display(inquiry_id));
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        Ok(InboundEmailOutcome::Unresolved {
            raw_payload_id,
            reason,
        }) => {
            // SLICE_007d §8's outcome vocabulary, keyed by the reason
            // variant only — never content.
            span.record(
                "outcome",
                match reason.as_str() {
                    "email_unparsed" => "unresolved_unparsed",
                    "email_unrecognized_format" => "unresolved_unrecognized_format",
                    "no_contact_method" => "unresolved_no_contact_method",
                    _ => "unresolved",
                },
            );
            span.record("raw_payload_id", tracing::field::display(raw_payload_id));
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        Ok(InboundEmailOutcome::DeferredPending { raw_payload_id }) => {
            span.record("outcome", "deferred_busy");
            span.record("raw_payload_id", tracing::field::display(raw_payload_id));
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        Ok(InboundEmailOutcome::Duplicate) => {
            span.record("outcome", "duplicate");
            Ok((StatusCode::OK, Json(json!({ "status": "accepted" }))).into_response())
        }
        // Slice 009 (docs/specs/SLICE_009.md §3, §9): the recipient
        // resolved to a capture address. The entire correspondence
        // pipeline ran under its own nested `capture.inbound_email` span
        // (`domain::capture::receive`) — this handler's own span only
        // records that a capture request was accepted, never which
        // sub-case occurred (matched/held/duplicate/unparseable), exactly
        // like every other outcome on this frozen envelope.
        Ok(InboundEmailOutcome::Captured) => {
            span.record("outcome", "captured");
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
        Err(ReceiveInboundEmailError::Internal) => {
            span.record("outcome", "internal");
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

    /// docs/specs/SLICE_017.md §5.7: the endpoint must never reject a body
    /// the relay can legitimately send — the base64 of a full 25 MiB
    /// message plus a 4 KiB envelope allowance.
    #[test]
    fn max_inbound_email_body_bytes_admits_the_relays_largest_legitimate_body() {
        let raw_ceiling: usize = 25 * 1024 * 1024;
        assert!(MAX_INBOUND_EMAIL_BODY_BYTES >= 4 * raw_ceiling.div_ceil(3) + 4096);
    }

    /// docs/specs/SLICE_017.md §3: an escape-free base64 string must
    /// borrow from the buffered body rather than copy it — dropping
    /// `#[serde(borrow)]` would silently regress to an owned `String`
    /// with no compile error, doubling peak memory for the 34 MiB body.
    #[test]
    fn raw_field_borrows_when_the_json_string_has_no_escapes() {
        let req: InboundEmailRequest<'_> =
            serde_json::from_slice(br#"{"recipient":"a","raw":"QUJD"}"#).unwrap();
        assert!(matches!(req.raw, Cow::Borrowed(_)));
    }
}

//! LiveKit-backed telephony tests (docs/specs/SLICE_006.md §13 item 3).
//! Run only via ./scripts/check-telephony, gated on
//! `CRM_TEST_LIVEKIT_API_URL` (+ `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET`
//! from `.env`) — the third documented exception to "tests never read
//! ambient environment". Fails loudly, never skips: an unset variable or
//! an unreachable host is a failure with a clear message. Not part of
//! `check-db` (which stays loopback-only) nor of `check`.
//!
//! Covers: create room → list participants → delete with real auth against
//! the real server; a webhook round-trip where the signature is produced
//! by the same algorithm LiveKit uses and checked by the API's verifier.
//! No SIP participant is created: this never places a PSTN call.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::state::AppState;
use crm_api::telephony::{
    LiveKitProvider, ProviderError, Telephony, TelephonyLimits, TelephonyProvider,
};

fn required(name: &str) -> String {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "{name} must be set for tests/livekit_telephony.rs — run it through \
             ./scripts/check-telephony, never directly"
        ),
    }
}

struct Live {
    api_url: String,
    api_key: String,
    api_secret: String,
}

fn live() -> Live {
    Live {
        api_url: required("CRM_TEST_LIVEKIT_API_URL")
            .trim_end_matches('/')
            .to_string(),
        api_key: required("LIVEKIT_API_KEY"),
        api_secret: required("LIVEKIT_API_SECRET"),
    }
}

/// Fails loudly with a clear message, never skips: the host must answer
/// on its API URL before anything else is asserted.
async fn assert_livekit_reachable(api_url: &str) {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    match client.get(api_url).send().await {
        Ok(_) => {}
        Err(err) => panic!(
            "LiveKit not reachable at CRM_TEST_LIVEKIT_API_URL ({api_url}): {err}. \
             Bring the telephony host up (README: Telephony host) — this test never skips."
        ),
    }
}

fn provider(live: &Live) -> LiveKitProvider {
    LiveKitProvider::with_url(
        live.api_url.clone(),
        &live.api_key,
        live.api_secret.as_bytes(),
        "ST_unused_by_this_test",
    )
}

#[tokio::test]
#[ignore]
async fn create_room_list_participants_and_delete_with_real_auth() {
    let live = live();
    assert_livekit_reachable(&live.api_url).await;
    let provider = provider(&live);
    let room = format!("call:{}", Uuid::new_v4());

    provider
        .create_room(&room, Duration::from_secs(60))
        .await
        .expect("CreateRoom with a roomCreate grant");
    // Nobody has joined: the agent is absent, and the answer is a real
    // ListParticipants with a roomAdmin grant scoped to this room.
    let present = provider
        .participant_present(&room, "agent:nobody")
        .await
        .expect("ListParticipants with a roomAdmin grant");
    assert!(!present);

    provider.hangup(&room).await.expect("DeleteRoom");
    // Idempotent: deleting a room that is already gone is Ok.
    provider
        .hangup(&room)
        .await
        .expect("DeleteRoom on a missing room is Ok");

    // A wrong secret is rejected by the server, not accepted silently.
    let bad = LiveKitProvider::with_url(
        live.api_url.clone(),
        &live.api_key,
        b"not-the-secret",
        "ST_unused",
    );
    match bad.create_room(&room, Duration::from_secs(60)).await {
        Err(ProviderError::Rejected(_)) => {}
        other => panic!("a bad secret must be Rejected, got {other:?}"),
    }
}

/// The webhook verifier accepts a body signed the way LiveKit signs
/// (`Authorization: <JWT>` with `iss = api_key`, `sha256` = standard
/// base64 of the body hash) using the real key pair, through the real
/// route; a tampered body is 401.
#[tokio::test]
#[ignore]
async fn webhook_round_trip_with_the_real_key_pair() {
    let live = live();
    assert_livekit_reachable(&live.api_url).await;

    let config = Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some("ab".repeat(32)),
        "CENTRIFUGO_HTTP_API_KEY" => Some("test-centrifugo-api-key".to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some("c".repeat(32)),
        _ => None,
    })
    .unwrap();
    let telephony = Arc::new(Telephony::with_provider(
        Arc::new(provider(&live)),
        "livekit",
        &live.api_key,
        live.api_secret.as_bytes(),
        TelephonyLimits::from_config(&config),
    ));
    // No database needed: `room_started` is verified, then ignored before
    // any lookup, so the lazy pool below is never touched.
    let state = AppState::for_tests(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap(),
        &config,
        crm_api::realtime::Publisher::recording(),
    )
    .with_telephony(telephony.clone());
    let app = crm_api::build_app(state);

    let body = serde_json::json!({
        "event": "room_started",
        "id": Uuid::new_v4(),
        "room": { "name": format!("call:{}", Uuid::new_v4()) },
    })
    .to_string()
    .into_bytes();
    let token = telephony.webhook.sign_for_tests(&body, Utc::now(), 300);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/livekit")
                .header("content-type", "application/json")
                .header("authorization", &token)
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"{}");

    let mut tampered = body.clone();
    tampered.push(b' ');
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/livekit")
                .header("content-type", "application/json")
                .header("authorization", &token)
                .body(Body::from(tampered))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

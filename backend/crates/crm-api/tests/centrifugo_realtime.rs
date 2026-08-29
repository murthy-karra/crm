//! Centrifugo-backed realtime tests (docs/specs/SLICE_003.md §13, acceptance
//! criterion 7). Run only via ./scripts/check-db, against the real
//! Centrifugo container started by ./scripts/dev-services. Reads
//! `CENTRIFUGO_TOKEN_HMAC_SECRET` / `CENTRIFUGO_HTTP_API_KEY` /
//! `CRM_CENTRIFUGO_API_URL` from the environment because they must match
//! the running container — the second documented exception to "tests
//! never read ambient environment" (beside `CRM_DB_APP_PASSWORD` in
//! tests/common/mod.rs); check-db already sources .env with `set -a`.
//! Random Organization ids for the negative-isolation half keep this test
//! isolated from real dev use (Centrifugo itself is stateless).
//!
//! `#[sqlx::test]` runs on a current-thread runtime: a `Publisher::
//! Centrifugo` publish is a detached `tokio::spawn`, which only progresses
//! at await points, so every assertion below awaits the WebSocket receipt
//! itself rather than asserting a publish "happened" without awaiting.

use std::env;
use std::time::Duration;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::ids::{OrganizationId, UserId};
use crm_api::realtime::{token, CentrifugoTransport, Publisher};
use crm_api::state::AppState;

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn centrifugo_api_url() -> String {
    env::var("CRM_CENTRIFUGO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8000/api".to_string())
}

fn centrifugo_ws_url() -> String {
    let api_url = centrifugo_api_url();
    let base = api_url.strip_suffix("/api").unwrap_or(&api_url);
    let ws_base = base.replacen("http://", "ws://", 1);
    format!("{ws_base}/connection/websocket")
}

fn centrifugo_host_port() -> String {
    let api_url = centrifugo_api_url();
    api_url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap()
        .to_string()
}

/// Fails loudly with a clear message, never skips (docs/specs/SLICE_003.md
/// §13): a missing container must not silently pass this suite.
async fn assert_centrifugo_reachable() {
    let host_port = centrifugo_host_port();
    match tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(&host_port),
    )
    .await
    {
        Ok(Ok(_)) => {}
        _ => panic!(
            "Centrifugo not reachable at {host_port} (CRM_CENTRIFUGO_API_URL={}). \
             Run ./scripts/dev-services up first.",
            centrifugo_api_url()
        ),
    }
}

/// A `Config` built from the *real* environment secrets so minted tokens
/// and published events are authentic to the running container, not the
/// fixed 32-byte test secret every other DB-backed test uses.
fn real_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(crate::common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => env::var("CENTRIFUGO_HTTP_API_KEY").ok(),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => env::var("CENTRIFUGO_TOKEN_HMAC_SECRET").ok(),
        "CRM_CENTRIFUGO_API_URL" => env::var("CRM_CENTRIFUGO_API_URL").ok(),
        _ => None,
    })
    .expect(
        "CENTRIFUGO_HTTP_API_KEY and CENTRIFUGO_TOKEN_HMAC_SECRET must be set in the \
         environment (check-db sources .env with `set -a`)",
    )
}

async fn connect_ws(token: &str) -> Result<WsStream, ()> {
    let (mut ws, _) = tokio_tungstenite::connect_async(centrifugo_ws_url())
        .await
        .expect(
        "Centrifugo WebSocket upgrade must succeed (container reachable but rejecting upgrades?)",
    );
    ws.send(Message::Text(
        json!({ "id": 1, "connect": { "token": token } }).to_string(),
    ))
    .await
    .map_err(|_| ())?;
    Ok(ws)
}

/// Reads the next non-ping JSON frame, answering any `{}` ping with `{}`
/// along the way (docs/specs/SLICE_003.md §13). `None` on timeout or a
/// closed connection.
async fn recv_json(ws: &mut WsStream, timeout: Duration) -> Option<Value> {
    loop {
        return match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).ok()?;
                if value == json!({}) {
                    let _ = ws.send(Message::Text("{}".to_string())).await;
                    continue;
                }
                Some(value)
            }
            _ => None,
        };
    }
}

/// A connect attempt that must be refused: either a `{"error": ...}` reply
/// (e.g. an expired token) or the connection closing outright (e.g. a
/// bad-signature token — Centrifugo v6.9.2 closes with code 3500 before
/// any JSON reply).
async fn assert_connect_refused(token: &str, label: &str) {
    let (mut ws, _) = tokio_tungstenite::connect_async(centrifugo_ws_url())
        .await
        .unwrap_or_else(|e| panic!("{label}: WebSocket upgrade must succeed: {e}"));
    ws.send(Message::Text(
        json!({ "id": 1, "connect": { "token": token } }).to_string(),
    ))
    .await
    .unwrap();

    match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert!(
                value.get("error").is_some(),
                "{label}: expected a connect error, got {value}"
            );
        }
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => {
            // Connection closed outright — also a refusal.
        }
        Err(_) => panic!("{label}: connect attempt neither replied nor closed within 5s"),
        other => panic!("{label}: unexpected frame: {other:?}"),
    }
}

/// Criterion 7, all in one test so the negative assertion (B receives
/// nothing) is meaningful against a real A publish in the same run.
#[sqlx::test]
#[ignore]
async fn centrifugo_delivers_scoped_events_denies_cross_org_and_never_replays(
    migrator_pool: PgPool,
) {
    assert_centrifugo_reachable().await;

    let (org_a_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;

    let config = real_config();
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::Centrifugo(CentrifugoTransport::new(
        config.centrifugo_api_url.clone(),
        &config.centrifugo_api_key,
    ));
    let state = AppState::for_tests(app_pool, &config, publisher);
    let router = crm_api::build_app(state);
    let alice_cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    // Two live connections: A holds a real Organization's token; B holds a
    // token for an unrelated, random Organization (Centrifugo is
    // stateless, so B needs no DB row at all).
    let token_a = token::mint(
        &config.realtime_token_secret,
        UserId::new(alice_id),
        OrganizationId::new(org_a_id),
        Utc::now(),
        Duration::from_secs(600),
    );
    let org_b_id = Uuid::new_v4();
    let token_b = token::mint(
        &config.realtime_token_secret,
        UserId::new(Uuid::new_v4()),
        OrganizationId::new(org_b_id),
        Utc::now(),
        Duration::from_secs(600),
    );

    let mut ws_a = connect_ws(&token_a).await.expect("A connects");
    let connect_reply_a = recv_json(&mut ws_a, Duration::from_secs(5))
        .await
        .expect("A's connect reply");
    assert!(
        connect_reply_a["connect"]["subs"]
            .get(format!("org:{org_a_id}"))
            .is_some(),
        "A must be server-side subscribed to exactly its own Organization channel: {connect_reply_a}"
    );

    let mut ws_b = connect_ws(&token_b).await.expect("B connects");
    let connect_reply_b = recv_json(&mut ws_b, Duration::from_secs(5))
        .await
        .expect("B's connect reply");
    assert!(
        connect_reply_b["connect"]["subs"]
            .get(format!("org:{org_b_id}"))
            .is_some(),
        "B must be subscribed to its own channel: {connect_reply_b}"
    );

    // Trigger a real command: creates a Person assigned to Alice with a
    // fresh Inquiry, publishing person.changed{inquiry_received} to A's
    // channel.
    let intake = crate::common::post_inquiry(
        &router,
        &alice_cookie,
        "zillow",
        json!({ "email": "realtime-lead@example.com" }),
        None,
    )
    .await;
    assert_eq!(intake.status(), axum::http::StatusCode::CREATED);
    let person_id: Uuid = crate::common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // A receives its event within 5s.
    let push_a = recv_json(&mut ws_a, Duration::from_secs(5))
        .await
        .expect("A must receive the person.changed push within 5s");
    let event_a = push_a["push"]["pub"]["data"].clone();
    assert_eq!(event_a["v"], 1);
    assert_eq!(event_a["type"], "person.changed");
    assert_eq!(event_a["organization_id"], org_a_id.to_string());
    assert_eq!(event_a["data"]["person_id"], person_id.to_string());
    assert_eq!(event_a["data"]["change"], "inquiry_received");
    assert_eq!(push_a["push"]["channel"], format!("org:{org_a_id}"));

    // B receives nothing from A's Organization in the following 500ms
    // (pings filtered by recv_json) — the meaningful negative assertion,
    // proven against a real publish that just happened.
    let push_b = recv_json(&mut ws_b, Duration::from_millis(500)).await;
    assert!(
        push_b.is_none(),
        "B must receive nothing from A's Organization, got {push_b:?}"
    );

    // B's client-initiated subscribe to A's channel is denied (error 103 —
    // a 102 would mean the `org` namespace is missing from Centrifugo's
    // config; restart dev-services if this ever flips to 102).
    ws_b.send(Message::Text(
        json!({ "id": 2, "subscribe": { "channel": format!("org:{org_a_id}") } }).to_string(),
    ))
    .await
    .unwrap();
    let subscribe_reply = recv_json(&mut ws_b, Duration::from_secs(5))
        .await
        .expect("subscribe reply");
    assert_eq!(
        subscribe_reply["error"]["code"], 103,
        "expected permission denied (103), got {subscribe_reply}"
    );

    // An expired token and a token signed with a different secret are both
    // refused at connect.
    let expired_token = token::mint(
        &config.realtime_token_secret,
        UserId::new(alice_id),
        OrganizationId::new(org_a_id),
        Utc::now() - chrono::Duration::hours(1),
        Duration::from_secs(60),
    );
    assert_connect_refused(&expired_token, "expired token").await;

    let wrong_secret_config = Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(crate::common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => Some("unused".to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some("z".repeat(32)),
        _ => None,
    })
    .unwrap();
    let wrong_secret_token = token::mint(
        &wrong_secret_config.realtime_token_secret,
        UserId::new(alice_id),
        OrganizationId::new(org_a_id),
        Utc::now(),
        Duration::from_secs(600),
    );
    assert_connect_refused(&wrong_secret_token, "wrong-secret token").await;

    // Recovery: close A's socket, run a second command (removes the Person
    // from Alice's Today), then reconnect A with a fresh token. No push
    // for the missed event arrives within 500ms (no replay — Centrifugo
    // stays stateless, no history) while GET /api/today already reflects
    // the change over plain HTTP.
    ws_a.close(None).await.ok();

    let log_contact = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &alice_cookie,
        json!({ "channel": "call", "outcome": "reached" }),
    )
    .await;
    assert_eq!(log_contact.status(), axum::http::StatusCode::CREATED);

    let fresh_token_a = token::mint(
        &config.realtime_token_secret,
        UserId::new(alice_id),
        OrganizationId::new(org_a_id),
        Utc::now(),
        Duration::from_secs(600),
    );
    let mut ws_a_reconnected = connect_ws(&fresh_token_a).await.expect("A reconnects");
    let _reconnect_reply = recv_json(&mut ws_a_reconnected, Duration::from_secs(5))
        .await
        .expect("A's reconnect reply");

    let missed_push = recv_json(&mut ws_a_reconnected, Duration::from_millis(500)).await;
    assert!(
        missed_push.is_none(),
        "no replay of the missed contact_attempted event: got {missed_push:?}"
    );

    let today = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/today", &alice_cookie).await,
    )
    .await;
    assert_eq!(
        today["items"].as_array().unwrap().len(),
        0,
        "GET /api/today must already reflect the second command over plain HTTP"
    );
}

//! `LiveKitProvider` (docs/specs/SLICE_006.md §3): Twirp JSON over HTTPS
//! to `LIVEKIT_API_URL` — `livekit.RoomService/{CreateRoom,
//! ListParticipants, DeleteRoom}` and `livekit.SIP/CreateSIPParticipant`.
//! Auth is a short-lived HS256 JWT per request carrying exactly the grant
//! the call needs (the same hand-rolled signer as `token.rs`). Nothing here
//! logs a request or response body: `CreateSIPParticipant` carries the
//! number and `ListParticipants` responses carry participant attributes
//! (`sip.phoneNumber`), so both are decoded into narrow structs and
//! dropped (§2 PII rule, §9).

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::token::sign_hs256;
use super::{DialOutcome, DialRequest, ProviderError, SipFailure, TelephonyProvider};
use crate::config::LiveKitConfig;

/// Room/participant calls are quick; `dial` gets `ring_timeout + 15 s`
/// (docs/specs/SLICE_006.md §9).
pub const ADMIN_CALL_TIMEOUT: Duration = Duration::from_secs(5);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DIAL_TIMEOUT_GRACE: Duration = Duration::from_secs(15);
/// Lifetime of each per-request API token.
const API_TOKEN_TTL_SECONDS: i64 = 60;
/// Tolerated clock skew between the API host and the LiveKit host.
const API_TOKEN_NBF_SKEW_SECONDS: i64 = 30;
/// Responses are tiny (`{}`, a room, a participant list); anything larger
/// is not a LiveKit answer and is cut off here.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Seconds an empty room waits for the agent to join before LiveKit
/// closes it on its own (the dial task gives up after 10 s).
const ROOM_EMPTY_TIMEOUT_SECONDS: u32 = 60;
/// Seconds after the last participant leaves before LiveKit closes the
/// room (→ `room_finished` webhook).
const ROOM_DEPARTURE_TIMEOUT_SECONDS: u32 = 5;

pub struct LiveKitProvider {
    http: reqwest::Client,
    /// `LIVEKIT_API_URL`, no trailing slash (validated by `Config`).
    api_url: String,
    api_key: String,
    api_secret: Vec<u8>,
    sip_trunk_id: String,
    /// `DIAL_TIMEOUT_GRACE`; shortened only by the unit tests.
    dial_grace: Duration,
}

impl fmt::Debug for LiveKitProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveKitProvider")
            .field("api_url", &self.api_url)
            .field("api_key", &self.api_key)
            .field("api_secret", &"REDACTED")
            .field("sip_trunk_id", &self.sip_trunk_id)
            .finish()
    }
}

// --- Token grants -----------------------------------------------------------

/// `video` grant for the RoomService calls. Only the fields a call needs
/// are set; `None` is omitted from the wire.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct VideoGrant {
    #[serde(skip_serializing_if = "Option::is_none")]
    room_create: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    room_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    room: Option<String>,
}

/// `sip` grant for `CreateSIPParticipant` (LiveKit ≥ 1.7: `call`).
#[derive(Serialize)]
struct SipGrant {
    call: bool,
}

#[derive(Serialize)]
struct ApiClaims {
    iss: String,
    nbf: i64,
    exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<VideoGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sip: Option<SipGrant>,
}

enum Grant {
    RoomCreate,
    RoomAdmin(String),
    SipCall,
}

// --- Wire shapes -------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRoomRequest<'a> {
    name: &'a str,
    empty_timeout: u32,
    departure_timeout: u32,
    max_participants: u32,
}

#[derive(Serialize)]
struct RoomRequest<'a> {
    room: &'a str,
}

#[derive(Deserialize, Default)]
struct ListParticipantsResponse {
    #[serde(default)]
    participants: Vec<ParticipantIdentity>,
}

/// Only the identity is read; attributes are never deserialized.
#[derive(Deserialize)]
struct ParticipantIdentity {
    #[serde(default)]
    identity: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSipParticipantRequest<'a> {
    sip_trunk_id: &'a str,
    sip_call_to: &'a str,
    room_name: &'a str,
    participant_identity: &'a str,
    wait_until_answered: bool,
    /// protobuf-JSON durations: `"45s"`.
    ringing_timeout: String,
    max_call_duration: String,
    krisp_enabled: bool,
}

#[derive(Deserialize, Default)]
struct SipParticipantResponse {
    #[serde(default, alias = "sipCallId")]
    sip_call_id: Option<String>,
}

/// Twirp error envelope: `{code, msg, meta}`. `msg` is never logged (it
/// may echo the request).
#[derive(Deserialize, Default)]
struct TwirpError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    /// Values are strings in practice; anything else is tolerated (a
    /// non-string value is simply not a SIP status).
    meta: std::collections::HashMap<String, serde_json::Value>,
}

fn proto_duration(d: Duration) -> String {
    format!("{}s", d.as_secs())
}

impl LiveKitProvider {
    pub fn new(config: &LiveKitConfig) -> Self {
        Self::with_url(
            config.api_url.clone(),
            &config.api_key,
            config.api_secret.as_bytes(),
            &config.sip_outbound_trunk_id,
        )
    }

    /// Builds a provider against an explicit URL/key pair (the unit tests'
    /// mock Twirp server and `scripts/check-telephony`).
    pub fn with_url(api_url: String, api_key: &str, api_secret: &[u8], sip_trunk_id: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client builds with static, valid configuration");
        Self {
            http,
            api_url,
            api_key: api_key.to_string(),
            api_secret: api_secret.to_vec(),
            sip_trunk_id: sip_trunk_id.to_string(),
            dial_grace: DIAL_TIMEOUT_GRACE,
        }
    }

    fn token(&self, grant: Grant) -> String {
        let now = Utc::now().timestamp();
        let (video, sip) = match grant {
            Grant::RoomCreate => (
                Some(VideoGrant {
                    room_create: Some(true),
                    ..VideoGrant::default()
                }),
                None,
            ),
            Grant::RoomAdmin(room) => (
                Some(VideoGrant {
                    room_admin: Some(true),
                    room: Some(room),
                    ..VideoGrant::default()
                }),
                None,
            ),
            Grant::SipCall => (None, Some(SipGrant { call: true })),
        };
        sign_hs256(
            &self.api_secret,
            &ApiClaims {
                iss: self.api_key.clone(),
                nbf: now - API_TOKEN_NBF_SKEW_SECONDS,
                exp: now + API_TOKEN_TTL_SECONDS,
                video,
                sip,
            },
        )
    }

    /// One Twirp call. `Ok(bytes)` is the (capped) 2xx body; a non-2xx
    /// answer is mapped: 5xx → `Unavailable`, anything else → the Twirp
    /// error envelope for the caller to interpret.
    async fn twirp(
        &self,
        service: &str,
        method: &str,
        grant: Grant,
        body: &impl Serialize,
        timeout: Duration,
    ) -> Result<Vec<u8>, TwirpFailure> {
        let url = format!("{}/twirp/{service}/{method}", self.api_url);
        let token = self.token(grant);
        let response = self
            .http
            .post(url)
            .timeout(timeout)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|err| TwirpFailure::Transport(transport_error(&err)))?;
        let status = response.status();
        let bytes = read_capped(response).await?;
        if status.is_success() {
            return Ok(bytes);
        }
        // A Twirp error envelope rides on 4xx *and* 5xx (LiveKit reports
        // SIP failures as `unavailable` = 503 with `meta`), so the body is
        // decoded either way and the status is mapped by the caller.
        let error: TwirpError = serde_json::from_slice(&bytes).unwrap_or_default();
        Err(TwirpFailure::Twirp {
            http_status: status.as_u16(),
            error,
        })
    }
}

enum TwirpFailure {
    Transport(ProviderError),
    Twirp { http_status: u16, error: TwirpError },
}

impl TwirpFailure {
    /// The generic mapping for the room calls: transport errors as they
    /// are, HTTP 5xx → `Unavailable`, any other Twirp error →
    /// `Rejected(<code>)`.
    fn rejected(self, method: &str) -> ProviderError {
        match self {
            TwirpFailure::Transport(err) => err,
            TwirpFailure::Twirp { http_status, error } if http_status >= 500 => {
                ProviderError::Unavailable(format!(
                    "{method}: {} (http {http_status})",
                    if error.code.is_empty() {
                        "-"
                    } else {
                        &error.code
                    }
                ))
            }
            TwirpFailure::Twirp { http_status, error } => {
                ProviderError::Rejected(format!("{method}: {} (http {http_status})", error.code))
            }
        }
    }
}

fn transport_error(err: &reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::Timeout
    } else if err.is_connect() {
        ProviderError::Unavailable("connect failed".to_string())
    } else if err.is_builder() || err.is_request() {
        ProviderError::Unavailable("request failed".to_string())
    } else {
        ProviderError::Unavailable("transport error".to_string())
    }
}

async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, TwirpFailure> {
    let mut bytes = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|err| {
            TwirpFailure::Transport(if err.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Unavailable("body read failed".to_string())
            })
        })?;
        let Some(chunk) = chunk else { break };
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(TwirpFailure::Transport(ProviderError::Rejected(
                "response too large".to_string(),
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// `CreateSIPParticipant` failure → outcome (docs/specs/SLICE_006.md §3,
/// review note: the SIP status arrives as Twirp error metadata). A SIP
/// status maps through `SipFailure::from_sip_status`; 487 (Request
/// Terminated — LiveKit cancelling the INVITE at `ringing_timeout`) and a
/// `deadline_exceeded` code *after at least `ring_timeout` has elapsed*
/// are the ring timeout; a `deadline_exceeded` that came back early is
/// not one (the callee was never rung that long) and is `Rejected`, as is
/// anything else without a SIP status.
fn map_dial_failure(
    failure: TwirpFailure,
    elapsed: Duration,
    ring_timeout: Duration,
) -> Result<DialOutcome, ProviderError> {
    match failure {
        TwirpFailure::Transport(err) => Err(err),
        TwirpFailure::Twirp { http_status, error } => {
            let sip_status = error
                .meta
                .get("sip_status_code")
                .and_then(|code| match code {
                    serde_json::Value::String(code) => code.trim().parse::<u16>().ok(),
                    serde_json::Value::Number(code) => {
                        code.as_u64().and_then(|c| u16::try_from(c).ok())
                    }
                    _ => None,
                });
            match sip_status {
                Some(487) => Ok(DialOutcome::Failed(SipFailure::RingTimeout)),
                Some(code) => Ok(DialOutcome::Failed(SipFailure::from_sip_status(code))),
                None if error.code == "deadline_exceeded" && elapsed >= ring_timeout => {
                    Ok(DialOutcome::Failed(SipFailure::RingTimeout))
                }
                None => {
                    Err(TwirpFailure::Twirp { http_status, error }.rejected("CreateSIPParticipant"))
                }
            }
        }
    }
}

#[async_trait]
impl TelephonyProvider for LiveKitProvider {
    /// `max_call` is enforced on the SIP leg (`max_call_duration`), not the
    /// room: LiveKit rooms have no duration cap of their own.
    async fn create_room(&self, room: &str, _max_call: Duration) -> Result<(), ProviderError> {
        self.twirp(
            "livekit.RoomService",
            "CreateRoom",
            Grant::RoomCreate,
            &CreateRoomRequest {
                name: room,
                empty_timeout: ROOM_EMPTY_TIMEOUT_SECONDS,
                departure_timeout: ROOM_DEPARTURE_TIMEOUT_SECONDS,
                max_participants: 2,
            },
            ADMIN_CALL_TIMEOUT,
        )
        .await
        .map(drop)
        .map_err(|f| f.rejected("CreateRoom"))
    }

    /// The response is decoded to identities only and dropped; it is
    /// never logged (participant attributes carry `sip.phoneNumber`).
    async fn participant_present(&self, room: &str, identity: &str) -> Result<bool, ProviderError> {
        let bytes = self
            .twirp(
                "livekit.RoomService",
                "ListParticipants",
                Grant::RoomAdmin(room.to_string()),
                &RoomRequest { room },
                ADMIN_CALL_TIMEOUT,
            )
            .await
            .map_err(|f| f.rejected("ListParticipants"))?;
        let list: ListParticipantsResponse = serde_json::from_slice(&bytes)
            .map_err(|_| ProviderError::Rejected("ListParticipants: undecodable".to_string()))?;
        Ok(list.participants.iter().any(|p| p.identity == identity))
    }

    async fn dial(&self, req: DialRequest) -> Result<DialOutcome, ProviderError> {
        let timeout = req.ring_timeout + self.dial_grace;
        let body = CreateSipParticipantRequest {
            sip_trunk_id: &self.sip_trunk_id,
            sip_call_to: req.to_number.expose(),
            room_name: &req.room,
            participant_identity: &req.participant_identity,
            wait_until_answered: true,
            ringing_timeout: proto_duration(req.ring_timeout),
            max_call_duration: proto_duration(req.max_call),
            krisp_enabled: false,
        };
        let started = std::time::Instant::now();
        match self
            .twirp(
                "livekit.SIP",
                "CreateSIPParticipant",
                Grant::SipCall,
                &body,
                timeout,
            )
            .await
        {
            Ok(bytes) => {
                // A 2xx that is not the participant-info JSON is not an
                // answer: never report `Answered` on an undecodable body.
                let info: SipParticipantResponse =
                    serde_json::from_slice(&bytes).map_err(|_| {
                        ProviderError::Rejected("CreateSIPParticipant: undecodable".to_string())
                    })?;
                Ok(DialOutcome::Answered {
                    call_ref: info.sip_call_id.filter(|id| !id.is_empty()),
                })
            }
            Err(failure) => map_dial_failure(failure, started.elapsed(), req.ring_timeout),
        }
    }

    /// Deletes the room; `not_found` is `Ok` (docs/specs/SLICE_006.md §3).
    async fn hangup(&self, room: &str) -> Result<(), ProviderError> {
        match self
            .twirp(
                "livekit.RoomService",
                "DeleteRoom",
                Grant::RoomCreate,
                &RoomRequest { room },
                ADMIN_CALL_TIMEOUT,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(TwirpFailure::Twirp { error, .. }) if error.code == "not_found" => Ok(()),
            Err(failure) => Err(failure.rejected("DeleteRoom")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Bytes;
    use axum::extract::{Path, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use hmac::{Hmac, KeyInit, Mac};
    use serde_json::{json, Value};
    use sha2::Sha256;

    use super::*;
    use crate::telephony::PhoneNumber;

    const KEY: &str = "APIkey-test";
    const SECRET: &[u8] = b"livekit-secret-for-provider-tests";
    const TRUNK: &str = "ST_trunk";

    #[derive(Clone)]
    struct Seen {
        method: String,
        authorization: Option<String>,
        body: Value,
    }

    #[derive(Clone)]
    enum Reply {
        Json(StatusCode, Value),
        Raw(StatusCode, Vec<u8>),
        Hang(Duration),
        HangThen(Duration, StatusCode, Value),
    }

    #[derive(Clone, Default)]
    struct MockState {
        seen: Arc<Mutex<Vec<Seen>>>,
        replies: Arc<Mutex<HashMap<String, Reply>>>,
    }

    async fn handle(
        State(state): State<MockState>,
        Path((service, method)): Path<(String, String)>,
        headers: HeaderMap,
        body: Bytes,
    ) -> (StatusCode, Vec<u8>) {
        let full = format!("{service}/{method}");
        state.seen.lock().unwrap().push(Seen {
            method: full.clone(),
            authorization: headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        });
        let reply = state.replies.lock().unwrap().get(&full).cloned();
        match reply {
            Some(Reply::Json(status, value)) => (status, value.to_string().into_bytes()),
            Some(Reply::Raw(status, bytes)) => (status, bytes),
            Some(Reply::Hang(for_how_long)) => {
                tokio::time::sleep(for_how_long).await;
                (StatusCode::OK, b"{}".to_vec())
            }
            Some(Reply::HangThen(for_how_long, status, value)) => {
                tokio::time::sleep(for_how_long).await;
                (status, value.to_string().into_bytes())
            }
            None => (StatusCode::OK, b"{}".to_vec()),
        }
    }

    /// A local Twirp mock; returns `(provider, state)`.
    async fn mock() -> (LiveKitProvider, MockState) {
        let state = MockState::default();
        let app = Router::new()
            .route("/twirp/{service}/{method}", post(handle))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = LiveKitProvider::with_url(format!("http://{addr}"), KEY, SECRET, TRUNK);
        (provider, state)
    }

    fn reply(state: &MockState, method: &str, reply: Reply) {
        state
            .replies
            .lock()
            .unwrap()
            .insert(method.to_string(), reply);
    }

    fn seen(state: &MockState) -> Vec<Seen> {
        state.seen.lock().unwrap().clone()
    }

    /// Verifies the bearer token with the secret and returns its claims.
    fn claims_of(authorization: &str) -> Value {
        let token = authorization
            .strip_prefix("Bearer ")
            .expect("Authorization: Bearer <jwt>");
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
        mac.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
        mac.verify_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap())
            .expect("signed with the API secret");
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], KEY);
        let now = Utc::now().timestamp();
        assert!(claims["nbf"].as_i64().unwrap() <= now);
        assert!(claims["exp"].as_i64().unwrap() > now);
        assert!(claims["exp"].as_i64().unwrap() <= now + API_TOKEN_TTL_SECONDS);
        claims
    }

    fn dial_request(room: &str) -> DialRequest {
        DialRequest {
            room: room.to_string(),
            to_number: PhoneNumber::new("+15555550100".to_string()),
            participant_identity: "sip:x".to_string(),
            ring_timeout: Duration::from_secs(45),
            max_call: Duration::from_secs(3600),
        }
    }

    #[tokio::test]
    async fn create_room_sends_the_request_shape_with_a_room_create_grant() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/CreateRoom",
            Reply::Json(StatusCode::OK, json!({"sid": "RM_x", "name": "call:x"})),
        );
        provider
            .create_room("call:x", Duration::from_secs(3600))
            .await
            .unwrap();
        let seen = seen(&state);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "livekit.RoomService/CreateRoom");
        assert_eq!(
            seen[0].body,
            json!({
                "name": "call:x",
                "emptyTimeout": ROOM_EMPTY_TIMEOUT_SECONDS,
                "departureTimeout": ROOM_DEPARTURE_TIMEOUT_SECONDS,
                "maxParticipants": 2,
            })
        );
        let claims = claims_of(seen[0].authorization.as_deref().unwrap());
        assert_eq!(claims["video"], json!({"roomCreate": true}));
        assert!(claims.get("sip").is_none());
    }

    #[tokio::test]
    async fn list_participants_uses_a_room_admin_grant_and_matches_identity() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/ListParticipants",
            Reply::Json(
                StatusCode::OK,
                json!({"participants": [
                    {"identity": "agent:u1", "attributes": {}},
                    {"identity": "sip:c1", "attributes": {"sip.phoneNumber": "+15555550100"}},
                ]}),
            ),
        );
        assert!(provider
            .participant_present("call:x", "sip:c1")
            .await
            .unwrap());
        assert!(!provider
            .participant_present("call:x", "agent:u2")
            .await
            .unwrap());
        let seen = seen(&state);
        assert_eq!(seen[0].body, json!({"room": "call:x"}));
        let claims = claims_of(seen[0].authorization.as_deref().unwrap());
        assert_eq!(
            claims["video"],
            json!({"roomAdmin": true, "room": "call:x"})
        );
    }

    #[tokio::test]
    async fn empty_participant_list_is_absent() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/ListParticipants",
            Reply::Json(StatusCode::OK, json!({})),
        );
        assert!(!provider
            .participant_present("call:x", "agent:u1")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn delete_room_not_found_is_ok_and_other_errors_are_rejected() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/DeleteRoom",
            Reply::Json(
                StatusCode::NOT_FOUND,
                json!({"code": "not_found", "msg": "room not found"}),
            ),
        );
        provider.hangup("call:x").await.unwrap();
        let seen_calls = seen(&state);
        assert_eq!(seen_calls[0].body, json!({"room": "call:x"}));
        let claims = claims_of(seen_calls[0].authorization.as_deref().unwrap());
        assert_eq!(claims["video"], json!({"roomCreate": true}));

        reply(
            &state,
            "livekit.RoomService/DeleteRoom",
            Reply::Json(
                StatusCode::FORBIDDEN,
                json!({"code": "permission_denied", "msg": "nope"}),
            ),
        );
        assert_eq!(
            provider.hangup("call:x").await,
            Err(ProviderError::Rejected(
                "DeleteRoom: permission_denied (http 403)".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn dial_sends_the_sip_request_shape_with_a_sip_call_grant() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(
                StatusCode::OK,
                json!({"participantId": "PA_x", "participantIdentity": "sip:x",
                       "roomName": "call:x", "sipCallId": "SCL_123"}),
            ),
        );
        let outcome = provider.dial(dial_request("call:x")).await.unwrap();
        assert_eq!(
            outcome,
            DialOutcome::Answered {
                call_ref: Some("SCL_123".to_string())
            }
        );
        let seen = seen(&state);
        assert_eq!(seen[0].method, "livekit.SIP/CreateSIPParticipant");
        assert_eq!(
            seen[0].body,
            json!({
                "sipTrunkId": TRUNK,
                "sipCallTo": "+15555550100",
                "roomName": "call:x",
                "participantIdentity": "sip:x",
                "waitUntilAnswered": true,
                "ringingTimeout": "45s",
                "maxCallDuration": "3600s",
                "krispEnabled": false,
            })
        );
        let claims = claims_of(seen[0].authorization.as_deref().unwrap());
        assert_eq!(claims["sip"], json!({"call": true}));
        assert!(claims.get("video").is_none());
    }

    #[tokio::test]
    async fn dial_accepts_snake_case_responses_and_empty_call_ids() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(StatusCode::OK, json!({"sip_call_id": "SCL_snake"})),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await.unwrap(),
            DialOutcome::Answered {
                call_ref: Some("SCL_snake".to_string())
            }
        );
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(StatusCode::OK, json!({"sipCallId": ""})),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await.unwrap(),
            DialOutcome::Answered { call_ref: None }
        );
    }

    #[tokio::test]
    async fn dial_maps_sip_status_from_twirp_meta() {
        let (provider, state) = mock().await;
        for (code, expected) in [
            ("486", SipFailure::Busy),
            ("603", SipFailure::Declined),
            ("480", SipFailure::NoAnswer),
            ("408", SipFailure::NoAnswer),
            ("487", SipFailure::RingTimeout),
            ("503", SipFailure::Other(503)),
        ] {
            reply(
                &state,
                "livekit.SIP/CreateSIPParticipant",
                Reply::Json(
                    // LiveKit reports SIP failures as `unavailable` (503).
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"code": "unavailable", "msg": "sip failed",
                           "meta": {"sip_status_code": code, "sip_status": "x"}}),
                ),
            );
            assert_eq!(
                provider.dial(dial_request("call:x")).await.unwrap(),
                DialOutcome::Failed(expected),
                "sip {code}"
            );
        }
        // A numeric meta value is read too; a non-string/non-number one
        // is tolerated (no panic, no SIP status → rejected).
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"code": "unavailable", "meta": {"sip_status_code": 486}}),
            ),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await.unwrap(),
            DialOutcome::Failed(SipFailure::Busy)
        );
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"code": "unavailable", "meta": {"sip_status_code": {"x": [1]}, "n": null}}),
            ),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await,
            Err(ProviderError::Unavailable(
                "CreateSIPParticipant: unavailable (http 503)".to_string()
            ))
        );
        // A Twirp deadline is the ring timeout only once the ring timeout
        // has actually elapsed (`dial_maps_a_twirp_deadline_by_elapsed_time`
        // covers the late branch); an immediate one is a rejection.
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(
                StatusCode::REQUEST_TIMEOUT,
                json!({"code": "deadline_exceeded", "msg": "ringing timeout"}),
            ),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await,
            Err(ProviderError::Rejected(
                "CreateSIPParticipant: deadline_exceeded (http 408)".to_string()
            ))
        );
        // No SIP status at all: a provider rejection (→ provider_error).
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Json(
                StatusCode::BAD_REQUEST,
                json!({"code": "invalid_argument", "msg": "bad trunk"}),
            ),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await,
            Err(ProviderError::Rejected(
                "CreateSIPParticipant: invalid_argument (http 400)".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn dial_maps_a_twirp_deadline_by_elapsed_time() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::HangThen(
                Duration::from_millis(150),
                StatusCode::REQUEST_TIMEOUT,
                json!({"code": "deadline_exceeded", "msg": "ringing timeout"}),
            ),
        );
        let mut req = dial_request("call:x");
        req.ring_timeout = Duration::from_millis(100);
        assert_eq!(
            provider.dial(req).await.unwrap(),
            DialOutcome::Failed(SipFailure::RingTimeout)
        );
        let mut req = dial_request("call:x");
        req.ring_timeout = Duration::from_secs(5);
        assert_eq!(
            provider.dial(req).await,
            Err(ProviderError::Rejected(
                "CreateSIPParticipant: deadline_exceeded (http 408)".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn a_2xx_non_json_dial_response_is_rejected_not_answered() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Raw(StatusCode::OK, b"<html>gateway</html>".to_vec()),
        );
        assert_eq!(
            provider.dial(dial_request("call:x")).await,
            Err(ProviderError::Rejected(
                "CreateSIPParticipant: undecodable".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn http_5xx_is_unavailable_and_a_hung_server_is_a_timeout() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/CreateRoom",
            Reply::Json(StatusCode::BAD_GATEWAY, json!({"code": "internal"})),
        );
        assert_eq!(
            provider
                .create_room("call:x", Duration::from_secs(60))
                .await,
            Err(ProviderError::Unavailable(
                "CreateRoom: internal (http 502)".to_string()
            ))
        );

        reply(
            &state,
            "livekit.SIP/CreateSIPParticipant",
            Reply::Hang(Duration::from_secs(30)),
        );
        let mut req = dial_request("call:x");
        req.ring_timeout = Duration::from_millis(100);
        // The dial timeout is ring_timeout + the grace; shrink the grace so
        // the test does not wait 15 s.
        let mut provider = provider;
        provider.dial_grace = Duration::from_millis(100);
        assert_eq!(provider.dial(req).await, Err(ProviderError::Timeout));
    }

    #[tokio::test]
    async fn an_unreachable_host_is_unavailable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let provider = LiveKitProvider::with_url(format!("http://{addr}"), KEY, SECRET, TRUNK);
        assert!(matches!(
            provider.hangup("call:x").await,
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn oversized_responses_are_cut_off() {
        let (provider, state) = mock().await;
        reply(
            &state,
            "livekit.RoomService/ListParticipants",
            Reply::Raw(StatusCode::OK, vec![b' '; MAX_RESPONSE_BYTES + 1]),
        );
        assert_eq!(
            provider.participant_present("call:x", "agent:u1").await,
            Err(ProviderError::Rejected("response too large".to_string()))
        );
    }

    #[test]
    fn debug_is_redacted_and_numbers_never_reach_errors() {
        let provider = LiveKitProvider::with_url("https://lk".into(), KEY, SECRET, TRUNK);
        let debug = format!("{provider:?}");
        assert!(!debug.contains(std::str::from_utf8(SECRET).unwrap()));
        assert!(debug.contains("REDACTED"));
        assert_eq!(proto_duration(Duration::from_secs(45)), "45s");
    }
}

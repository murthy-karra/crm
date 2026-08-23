//! LiveKit join-grant minting (docs/specs/SLICE_006.md §3, §7). The same
//! hand-rolled sign-only HS256 as `realtime/token.rs`: one room, one
//! identity, join/publish/subscribe only, short TTL. The token is returned
//! once to the caller and never stored or logged — `JoinToken`'s `Debug`
//! is redacted.

use std::fmt;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// `{"alg":"HS256","typ":"JWT"}` — exact bytes, no whitespace.
pub(crate) const HEADER_JSON: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

/// Exactly §3's `video` grant. No `roomCreate`, `roomAdmin`, `roomList`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoGrant {
    room: String,
    room_join: bool,
    can_publish: bool,
    can_subscribe: bool,
    can_publish_data: bool,
}

/// Declaration order is the wire order (`serde_json` writes struct fields
/// in declaration order): `iss, sub, exp, nbf, video`.
#[derive(Serialize)]
struct Claims {
    iss: String,
    sub: String,
    exp: i64,
    nbf: i64,
    video: VideoGrant,
}

/// A minted join token. `Debug` is redacted (docs/specs/SLICE_006.md §7);
/// `into_string` is the only way to read it — used once, by the route,
/// to serialize the response.
#[derive(Clone)]
pub struct JoinToken(String);

impl JoinToken {
    pub fn into_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for JoinToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JoinToken(REDACTED)")
    }
}

/// `join` in the `POST /api/people/{id}/calls` response
/// (docs/specs/SLICE_006.md §5): `{url, token, room}`.
#[derive(Debug, Clone)]
pub struct JoinGrant {
    pub url: String,
    pub token: JoinToken,
    pub room: String,
}

/// Mints join grants with the LiveKit API key/secret. Holds the secret
/// bytes; `Debug` shows only the key id.
#[derive(Clone)]
pub struct JoinTokenSigner {
    api_key: String,
    api_secret: Vec<u8>,
}

impl fmt::Debug for JoinTokenSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinTokenSigner")
            .field("api_key", &self.api_key)
            .field("api_secret", &"REDACTED")
            .finish()
    }
}

impl JoinTokenSigner {
    pub fn new(api_key: &str, api_secret: &[u8]) -> Self {
        Self {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_vec(),
        }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Mints `{iss: api_key, sub: "agent:<user_id>", exp, nbf, video:
    /// {room, roomJoin, canPublish, canSubscribe, canPublishData: false}}`
    /// for exactly `room` (docs/specs/SLICE_006.md §3).
    pub fn mint(&self, user_id: Uuid, room: &str, now: DateTime<Utc>, ttl: Duration) -> JoinToken {
        let nbf = now.timestamp();
        let exp = nbf + ttl.as_secs() as i64;
        let claims = Claims {
            iss: self.api_key.clone(),
            sub: format!("agent:{user_id}"),
            exp,
            nbf,
            video: VideoGrant {
                room: room.to_string(),
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                can_publish_data: false,
            },
        };
        JoinToken(sign_hs256(&self.api_secret, &claims))
    }
}

/// Signs `claims` as a compact HS256 JWS. Shared with the LiveKit provider
/// (step 4), whose API tokens carry a different `video` grant.
pub(crate) fn sign_hs256<T: Serialize>(secret: &[u8], claims: &T) -> String {
    let header_b64 = URL_SAFE_NO_PAD.encode(HEADER_JSON.as_bytes());
    let payload_json = serde_json::to_vec(claims).expect("claims serialization cannot fail");
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let signature_b64 = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{signing_input}.{signature_b64}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn signer() -> JoinTokenSigner {
        JoinTokenSigner::new("APIkey", b"a-livekit-secret-for-tests")
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 25, 10, 0, 0).unwrap()
    }

    fn split(token: &str) -> (String, String, String) {
        let mut parts = token.split('.');
        let header = parts.next().unwrap().to_string();
        let payload = parts.next().unwrap().to_string();
        let signature = parts.next().unwrap().to_string();
        assert!(parts.next().is_none());
        (header, payload, signature)
    }

    #[test]
    fn header_is_exact() {
        let token = signer().mint(Uuid::new_v4(), "call:x", ts(), Duration::from_secs(300));
        let (header_b64, _, _) = split(token.as_str());
        assert_eq!(
            URL_SAFE_NO_PAD.decode(header_b64).unwrap(),
            HEADER_JSON.as_bytes()
        );
    }

    #[test]
    fn claims_are_exactly_section_3() {
        let user_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let room = format!("call:{call_id}");
        let token = signer().mint(user_id, &room, ts(), Duration::from_secs(300));
        let (_, payload_b64, _) = split(token.as_str());
        let payload = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "iss": "APIkey",
                "sub": format!("agent:{user_id}"),
                "exp": ts().timestamp() + 300,
                "nbf": ts().timestamp(),
                "video": {
                    "room": room,
                    "roomJoin": true,
                    "canPublish": true,
                    "canSubscribe": true,
                    "canPublishData": false,
                },
            })
        );
        // Exact key set: nothing else is granted.
        let video = value["video"].as_object().unwrap();
        assert_eq!(video.len(), 5);
        assert!(!video.contains_key("roomCreate"));
        assert!(!video.contains_key("roomAdmin"));
        assert!(!video.contains_key("roomList"));
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn signature_verifies_with_the_secret_and_not_another() {
        let token = signer().mint(Uuid::new_v4(), "call:x", ts(), Duration::from_secs(300));
        let (header_b64, payload_b64, signature_b64) = split(token.as_str());
        let signing_input = format!("{header_b64}.{payload_b64}");
        let signature = URL_SAFE_NO_PAD.decode(signature_b64).unwrap();

        let mut mac = HmacSha256::new_from_slice(b"a-livekit-secret-for-tests").unwrap();
        mac.update(signing_input.as_bytes());
        assert!(mac.verify_slice(&signature).is_ok());

        let mut other = HmacSha256::new_from_slice(b"another-secret").unwrap();
        other.update(signing_input.as_bytes());
        assert!(other.verify_slice(&signature).is_err());
    }

    #[test]
    fn join_token_debug_is_redacted() {
        let token = signer().mint(Uuid::new_v4(), "call:x", ts(), Duration::from_secs(300));
        let raw = token.as_str().to_string();
        assert!(!format!("{token:?}").contains(&raw));
        assert_eq!(format!("{token:?}"), "JoinToken(REDACTED)");
        let grant = JoinGrant {
            url: "wss://x".into(),
            token: token.clone(),
            room: "call:x".into(),
        };
        assert!(!format!("{grant:?}").contains(&raw));
        assert!(!format!("{:?}", signer()).contains("a-livekit-secret-for-tests"));
    }
}

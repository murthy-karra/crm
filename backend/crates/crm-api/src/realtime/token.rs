//! Centrifugo connection-token minting (docs/specs/SLICE_003.md §6, §14).
//! Hand-rolled sign-only HS256 on the existing `hmac`/`sha2`/`base64`
//! crates — no `jsonwebtoken` dependency for three base64url segments
//! (spec §14 safe default). Minting is entirely local: it never verifies a
//! token and never contacts Centrifugo.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::time::Duration;
use uuid::Uuid;

use crate::config::RealtimeTokenSecret;
use crate::realtime::events::channel_for;

type HmacSha256 = Hmac<Sha256>;

/// `{"alg":"HS256","typ":"JWT"}` — exact bytes, no whitespace
/// (docs/specs/SLICE_003.md §6 acceptance criterion 4).
const HEADER_JSON: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

/// Field order (`sub, iat, exp, channels`) is the struct's declared field
/// order: `serde_json`'s struct serializer writes fields in declaration
/// order, not sorted, because a Rust struct is not a `Value::Object`/`Map`
/// (docs/specs/SLICE_003.md §6 acceptance criterion 4).
#[derive(Serialize)]
struct Claims {
    sub: String,
    iat: i64,
    exp: i64,
    channels: Vec<String>,
}

/// Mints a Centrifugo connection token for `user_id`, server-side
/// subscribed to exactly `organization_id`'s channel
/// (docs/specs/SLICE_003.md §6, D-023 item 1). The client never chooses a
/// channel.
pub fn mint(
    secret: &RealtimeTokenSecret,
    user_id: Uuid,
    organization_id: Uuid,
    now: DateTime<Utc>,
    ttl: Duration,
) -> String {
    let iat = now.timestamp();
    let exp = iat + ttl.as_secs() as i64;
    let claims = Claims {
        sub: user_id.to_string(),
        iat,
        exp,
        channels: vec![channel_for(organization_id)],
    };

    let header_b64 = URL_SAFE_NO_PAD.encode(HEADER_JSON.as_bytes());
    let payload_json =
        serde_json::to_vec(&claims).expect("Claims serialization to JSON cannot fail");
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
    let signing_input = format!("{header_b64}.{payload_b64}");

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let signature_b64 = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{signing_input}.{signature_b64}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn secret(byte: u8) -> RealtimeTokenSecret {
        let hex: String = format!("{byte:02x}").repeat(16);
        crate::config::Config::from_source(move |key| match key {
            "CRM_SESSION_SECRET" => Some("a".repeat(32)),
            "CRM_RAW_PAYLOAD_KEY" => Some("ab".repeat(32)),
            "CENTRIFUGO_HTTP_API_KEY" => Some("test-key".to_string()),
            "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some(hex.clone()),
            _ => None,
        })
        .unwrap()
        .realtime_token_secret
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 18, 26, 40).unwrap()
    }

    fn split(token: &str) -> (String, String, String) {
        let mut parts = token.split('.');
        let header = parts.next().unwrap().to_string();
        let payload = parts.next().unwrap().to_string();
        let signature = parts.next().unwrap().to_string();
        assert!(parts.next().is_none(), "token must have exactly 3 parts");
        (header, payload, signature)
    }

    #[test]
    fn header_is_exact() {
        let token = mint(
            &secret(1),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ts(),
            Duration::from_secs(600),
        );
        let (header_b64, _, _) = split(&token);
        let header_bytes = URL_SAFE_NO_PAD.decode(header_b64).unwrap();
        assert_eq!(header_bytes, HEADER_JSON.as_bytes());
    }

    #[test]
    fn claims_are_exact_and_in_order() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let now = ts();
        let ttl = Duration::from_secs(600);
        let token = mint(&secret(1), user_id, org_id, now, ttl);
        let (_, payload_b64, _) = split(&token);
        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        let payload_str = String::from_utf8(payload_bytes).unwrap();

        let expected = format!(
            r#"{{"sub":"{user_id}","iat":{iat},"exp":{exp},"channels":["org:{org_id}"]}}"#,
            iat = now.timestamp(),
            exp = now.timestamp() + 600,
        );
        assert_eq!(payload_str, expected);
    }

    #[test]
    fn ttl_controls_exp_minus_iat() {
        let now = ts();
        let ttl = Duration::from_secs(120);
        let token = mint(&secret(1), Uuid::new_v4(), Uuid::new_v4(), now, ttl);
        let (_, payload_b64, _) = split(&token);
        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        let iat = value["iat"].as_i64().unwrap();
        let exp = value["exp"].as_i64().unwrap();
        assert_eq!(exp - iat, 120);
    }

    #[test]
    fn channels_is_exactly_the_active_organization_channel() {
        let org_id = Uuid::new_v4();
        let token = mint(
            &secret(1),
            Uuid::new_v4(),
            org_id,
            ts(),
            Duration::from_secs(600),
        );
        let (_, payload_b64, _) = split(&token);
        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(
            value["channels"],
            serde_json::json!([format!("org:{org_id}")])
        );
    }

    #[test]
    fn signature_verifies_with_the_configured_secret() {
        let s = secret(7);
        let token = mint(
            &s,
            Uuid::new_v4(),
            Uuid::new_v4(),
            ts(),
            Duration::from_secs(600),
        );
        let (header_b64, payload_b64, signature_b64) = split(&token);
        let signing_input = format!("{header_b64}.{payload_b64}");

        let mut mac = HmacSha256::new_from_slice(s.as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        let expected_signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        assert_eq!(signature_b64, expected_signature);
    }

    #[test]
    fn signature_differs_across_secrets() {
        let token_a = mint(
            &secret(1),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ts(),
            Duration::from_secs(600),
        );
        let token_b = mint(
            &secret(2),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ts(),
            Duration::from_secs(600),
        );
        let (_, _, sig_a) = split(&token_a);
        let (_, _, sig_b) = split(&token_b);
        assert_ne!(sig_a, sig_b);
    }
}

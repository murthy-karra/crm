//! LiveKit webhook signature verification (docs/specs/SLICE_006.md §3,
//! §7): the `Authorization` header carries a bare HS256 JWT signed with the
//! API secret whose claims are `iss = api_key`, `exp`/`nbf`, and `sha256` =
//! standard (padded) base64 of the SHA-256 of the raw body. The HMAC is
//! checked with `Mac::verify_slice` and the body hash with a constant-time
//! compare — never `==` on either (§7 implementation note). Nothing here
//! logs the body or the token.

use std::fmt;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Clock skew tolerated on `exp`/`nbf` (the telephony host and the API
/// host keep independent clocks).
pub const CLOCK_SKEW_SECONDS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookVerifyError {
    /// Not three base64url segments, or a header/payload that is not JSON.
    Malformed,
    /// The header names an algorithm other than HS256.
    UnsupportedAlgorithm,
    /// HMAC mismatch.
    BadSignature,
    /// `iss` is not this deployment's API key.
    WrongIssuer,
    /// `exp` is in the past (beyond skew) or `nbf` in the future.
    Expired,
    /// The `sha256` claim does not match the body.
    BodyMismatch,
}

impl WebhookVerifyError {
    pub fn kind(self) -> &'static str {
        match self {
            WebhookVerifyError::Malformed => "malformed",
            WebhookVerifyError::UnsupportedAlgorithm => "unsupported_algorithm",
            WebhookVerifyError::BadSignature => "bad_signature",
            WebhookVerifyError::WrongIssuer => "wrong_issuer",
            WebhookVerifyError::Expired => "expired",
            WebhookVerifyError::BodyMismatch => "body_mismatch",
        }
    }
}

#[derive(Deserialize)]
struct Header {
    alg: String,
}

#[derive(Deserialize)]
struct Claims {
    iss: Option<String>,
    exp: Option<i64>,
    nbf: Option<i64>,
    sha256: Option<String>,
}

#[derive(Clone)]
pub struct WebhookVerifier {
    api_key: String,
    api_secret: Vec<u8>,
}

impl fmt::Debug for WebhookVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookVerifier")
            .field("api_key", &self.api_key)
            .field("api_secret", &"REDACTED")
            .finish()
    }
}

impl WebhookVerifier {
    pub fn new(api_key: &str, api_secret: &[u8]) -> Self {
        Self {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_vec(),
        }
    }

    /// Verifies `token` (the raw `Authorization` header value) against
    /// `body` at `now`. Signature first, then issuer, then time, then the
    /// body hash — every failure is a 401 to the caller; the variant only
    /// feeds the `outcome` span field.
    pub fn verify(
        &self,
        token: &str,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), WebhookVerifyError> {
        let token = token.trim();
        let mut parts = token.split('.');
        let (Some(header_b64), Some(payload_b64), Some(signature_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(WebhookVerifyError::Malformed);
        };

        let header_bytes = URL_SAFE_NO_PAD
            .decode(header_b64)
            .map_err(|_| WebhookVerifyError::Malformed)?;
        let header: Header =
            serde_json::from_slice(&header_bytes).map_err(|_| WebhookVerifyError::Malformed)?;
        if header.alg != "HS256" {
            return Err(WebhookVerifyError::UnsupportedAlgorithm);
        }

        let signature = URL_SAFE_NO_PAD
            .decode(signature_b64)
            .map_err(|_| WebhookVerifyError::Malformed)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.api_secret).expect("HMAC accepts any key length");
        mac.update(header_b64.as_bytes());
        mac.update(b".");
        mac.update(payload_b64.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| WebhookVerifyError::BadSignature)?;

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| WebhookVerifyError::Malformed)?;
        let claims: Claims =
            serde_json::from_slice(&payload_bytes).map_err(|_| WebhookVerifyError::Malformed)?;

        if claims.iss.as_deref() != Some(self.api_key.as_str()) {
            return Err(WebhookVerifyError::WrongIssuer);
        }

        let now_ts = now.timestamp();
        match claims.exp {
            Some(exp) if exp.saturating_add(CLOCK_SKEW_SECONDS) >= now_ts => {}
            _ => return Err(WebhookVerifyError::Expired),
        }
        if let Some(nbf) = claims.nbf {
            if nbf.saturating_sub(CLOCK_SKEW_SECONDS) > now_ts {
                return Err(WebhookVerifyError::Expired);
            }
        }

        let claimed = claims.sha256.ok_or(WebhookVerifyError::BodyMismatch)?;
        let claimed = STANDARD
            .decode(claimed.as_bytes())
            .map_err(|_| WebhookVerifyError::BodyMismatch)?;
        let actual = Sha256::digest(body);
        if !constant_time_eq(&claimed, actual.as_slice()) {
            return Err(WebhookVerifyError::BodyMismatch);
        }
        Ok(())
    }

    /// Test-support and `scripts/check-telephony`: produces a header value
    /// exactly as LiveKit would for `body`, valid from `now` for `ttl_secs`.
    pub fn sign_for_tests(&self, body: &[u8], now: DateTime<Utc>, ttl_secs: i64) -> String {
        #[derive(serde::Serialize)]
        struct Out {
            iss: String,
            exp: i64,
            nbf: i64,
            sha256: String,
        }
        let claims = Out {
            iss: self.api_key.clone(),
            exp: now.timestamp() + ttl_secs,
            nbf: now.timestamp(),
            sha256: STANDARD.encode(Sha256::digest(body)),
        };
        super::token::sign_hs256(&self.api_secret, &claims)
    }
}

/// Length-then-bytes constant-time compare for the body hash. The hash is
/// not a secret, but comparing it non-constant-time would leak nothing of
/// value either way; the uniform rule (§7) is simply "never `==`".
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const KEY: &str = "APIkey";
    const SECRET: &[u8] = b"webhook-secret-for-tests";

    fn verifier() -> WebhookVerifier {
        WebhookVerifier::new(KEY, SECRET)
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 25, 10, 0, 0).unwrap()
    }

    #[test]
    fn valid_signature_verifies() {
        let body = br#"{"event":"room_finished","room":{"name":"call:x"}}"#;
        let token = verifier().sign_for_tests(body, now(), 300);
        assert_eq!(verifier().verify(&token, body, now()), Ok(()));
        // Within skew on either side of the window.
        assert_eq!(
            verifier().verify(&token, body, now() + chrono::Duration::seconds(300 + 30)),
            Ok(())
        );
        assert_eq!(
            verifier().verify(&token, body, now() - chrono::Duration::seconds(30)),
            Ok(())
        );
    }

    #[test]
    fn tampered_body_is_rejected() {
        let body = br#"{"event":"room_finished","room":{"name":"call:x"}}"#;
        let token = verifier().sign_for_tests(body, now(), 300);
        let tampered = br#"{"event":"room_finished","room":{"name":"call:y"}}"#;
        assert_eq!(
            verifier().verify(&token, tampered, now()),
            Err(WebhookVerifyError::BodyMismatch)
        );
    }

    #[test]
    fn expired_or_not_yet_valid_is_rejected() {
        let body = b"{}";
        let token = verifier().sign_for_tests(body, now(), 300);
        assert_eq!(
            verifier().verify(&token, body, now() + chrono::Duration::seconds(300 + 61)),
            Err(WebhookVerifyError::Expired)
        );
        assert_eq!(
            verifier().verify(&token, body, now() - chrono::Duration::seconds(61)),
            Err(WebhookVerifyError::Expired)
        );
    }

    #[test]
    fn wrong_key_or_secret_is_rejected() {
        let body = b"{}";
        let other_secret = WebhookVerifier::new(KEY, b"another-secret");
        let token = other_secret.sign_for_tests(body, now(), 300);
        assert_eq!(
            verifier().verify(&token, body, now()),
            Err(WebhookVerifyError::BadSignature)
        );
        let other_key = WebhookVerifier::new("OtherKey", SECRET);
        let token = other_key.sign_for_tests(body, now(), 300);
        assert_eq!(
            verifier().verify(&token, body, now()),
            Err(WebhookVerifyError::WrongIssuer)
        );
    }

    #[test]
    fn malformed_tokens_and_none_alg_are_rejected() {
        assert_eq!(
            verifier().verify("", b"{}", now()),
            Err(WebhookVerifyError::Malformed)
        );
        assert_eq!(
            verifier().verify("a.b", b"{}", now()),
            Err(WebhookVerifyError::Malformed)
        );
        assert_eq!(
            verifier().verify("not base64!.x.y", b"{}", now()),
            Err(WebhookVerifyError::Malformed)
        );
        let none_header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(br#"{"iss":"APIkey"}"#);
        assert_eq!(
            verifier().verify(&format!("{none_header}.{payload}."), b"{}", now()),
            Err(WebhookVerifyError::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn sha256_claim_is_standard_padded_base64() {
        let body = b"abc";
        let token = verifier().sign_for_tests(body, now(), 300);
        let payload_b64 = token.split('.').nth(1).unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload_b64).unwrap()).unwrap();
        // SHA-256("abc") in standard base64 ends with '=' padding.
        assert_eq!(
            payload["sha256"],
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
    }

    /// A token with arbitrary claims, correctly signed with `SECRET`.
    fn signed(claims: serde_json::Value) -> String {
        crate::telephony::token::sign_hs256(SECRET, &claims)
    }

    fn body_sha256(body: &[u8]) -> String {
        STANDARD.encode(Sha256::digest(body))
    }

    #[test]
    fn claim_edge_cases() {
        let body = b"{}";
        let exp = now().timestamp() + 300;
        let sha = body_sha256(body);
        // exp missing → Expired.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"iss": KEY, "sha256": sha})),
                body,
                now()
            ),
            Err(WebhookVerifyError::Expired)
        );
        // sha256 missing → BodyMismatch.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"iss": KEY, "exp": exp})),
                body,
                now()
            ),
            Err(WebhookVerifyError::BodyMismatch)
        );
        // Unpadded (URL-safe style) sha256 → BodyMismatch: only the
        // standard padded alphabet is accepted.
        let unpadded = sha.trim_end_matches('=').to_string();
        assert_ne!(unpadded, sha);
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"iss": KEY, "exp": exp, "sha256": unpadded})),
                body,
                now()
            ),
            Err(WebhookVerifyError::BodyMismatch)
        );
        // exp as a float → Malformed.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"iss": KEY, "exp": 1.5, "sha256": sha})),
                body,
                now()
            ),
            Err(WebhookVerifyError::Malformed)
        );
        // iss missing → WrongIssuer.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"exp": exp, "sha256": sha})),
                body,
                now()
            ),
            Err(WebhookVerifyError::WrongIssuer)
        );
        // Extreme exp/nbf: no overflow panic, and they are simply valid.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({
                    "iss": KEY, "exp": i64::MAX, "nbf": i64::MIN, "sha256": sha
                })),
                body,
                now()
            ),
            Ok(())
        );
        // Extreme in the other direction: rejected, still no panic.
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({"iss": KEY, "exp": i64::MIN, "sha256": sha})),
                body,
                now()
            ),
            Err(WebhookVerifyError::Expired)
        );
        assert_eq!(
            verifier().verify(
                &signed(serde_json::json!({
                    "iss": KEY, "exp": exp, "nbf": i64::MAX, "sha256": sha
                })),
                body,
                now()
            ),
            Err(WebhookVerifyError::Expired)
        );
    }

    #[test]
    fn debug_never_prints_the_secret() {
        assert!(!format!("{:?}", verifier()).contains("webhook-secret-for-tests"));
    }
}

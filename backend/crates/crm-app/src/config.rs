//! The application-layer configuration types (docs/specs/SLICE_006a.md
//! §4): the validated secret newtypes and the telephony sub-config that
//! `realtime`, `telephony`, and `domain::raw_payload` consume. Parsing
//! (`Config::from_source`) and everything HTTP-facing stay in crm-api;
//! the invariants live here, on the constructors, so they hold for every
//! caller.

use std::fmt;
use std::time::Duration;

pub const MIN_REALTIME_TOKEN_SECRET_BYTES: usize = 32;

/// Construction failure for the validated secret newtypes below. The
/// invariants live on the constructors (docs/specs/SLICE_006a.md §4) so
/// they hold for every caller, not only `Config::from_source`; crm-api
/// maps these onto the exact `ConfigError` variants it raised before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    Empty,
    TooShort { min: usize, len: usize },
}

/// `LIVEKIT_API_SECRET` (docs/specs/SLICE_006.md §7): signs join grants
/// and verifies webhooks. `Debug` is redacted like `SessionSecret`.
#[derive(Clone)]
pub struct LiveKitApiSecret(Vec<u8>);

impl LiveKitApiSecret {
    /// Non-empty, as `Config::from_source` has always required. The
    /// caller trims (from_source does); whitespace here is stored as-is.
    pub fn parse(raw: String) -> Result<Self, SecretError> {
        if raw.is_empty() {
            return Err(SecretError::Empty);
        }
        Ok(Self(raw.into_bytes()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for LiveKitApiSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LiveKitApiSecret(REDACTED)")
    }
}

/// The LiveKit connection (docs/specs/SLICE_006.md §11): present only when
/// `LIVEKIT_API_KEY` is non-empty; then every other value is required.
#[derive(Debug, Clone)]
pub struct LiveKitConfig {
    /// Browser signaling URL (`ws://`/`wss://`), returned in the join grant.
    pub url: String,
    /// Twirp API base (`https://`; `http://` only for loopback); no
    /// trailing slash.
    pub api_url: String,
    pub api_key: String,
    pub api_secret: LiveKitApiSecret,
    pub sip_outbound_trunk_id: String,
}

/// `CRM_TELEPHONY_*` (docs/specs/SLICE_006.md §11). Validated even when
/// `LIVEKIT_API_KEY` is unset, like `OperatorConfig`.
#[derive(Debug, Clone)]
pub struct TelephonyConfig {
    /// `None` = calling disabled (503 `telephony_disabled`), not an error.
    pub livekit: Option<LiveKitConfig>,
    pub ring_timeout: Duration,
    pub max_call: Duration,
    pub join_ttl: Duration,
}

/// The raw-payload encryption key (docs/specs/SLICE_002.md §7): exactly 64
/// hex characters (32 bytes), decoded once at startup. `Debug` is redacted
/// like `SessionSecret` so an accidental `{:?}` never leaks it. The API
/// refuses to start if it is missing, the wrong length, or not hex.
#[derive(Clone)]
pub struct RawPayloadKey([u8; 32]);

impl RawPayloadKey {
    /// Exactly 32 bytes, type-enforced; the hex decoding (and its
    /// errors) stay with `Config::from_source`.
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for RawPayloadKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPayloadKey(REDACTED)")
    }
}

/// The Centrifugo HTTP API publish credential (docs/specs/SLICE_003.md
/// §11): the same value compose passes to the Centrifugo container.
/// `Debug` is redacted like `SessionSecret`/`RawPayloadKey`.
#[derive(Clone)]
pub struct CentrifugoApiKey(String);

impl CentrifugoApiKey {
    /// Non-empty, as `Config::from_source` has always required.
    pub fn parse(raw: String) -> Result<Self, SecretError> {
        if raw.is_empty() {
            return Err(SecretError::Empty);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CentrifugoApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CentrifugoApiKey(REDACTED)")
    }
}

/// The Centrifugo connection-token HMAC signing secret
/// (docs/specs/SLICE_003.md §6, §11): the same value compose passes to the
/// Centrifugo container as `client.token.hmac_secret_key`. Must be at
/// least 32 bytes. `Debug` is redacted like `SessionSecret`.
#[derive(Clone)]
pub struct RealtimeTokenSecret(Vec<u8>);

impl RealtimeTokenSecret {
    /// At least `MIN_REALTIME_TOKEN_SECRET_BYTES` (32) bytes, as
    /// `Config::from_source` has always required.
    pub fn parse(raw: String) -> Result<Self, SecretError> {
        if raw.len() < MIN_REALTIME_TOKEN_SECRET_BYTES {
            return Err(SecretError::TooShort {
                min: MIN_REALTIME_TOKEN_SECRET_BYTES,
                len: raw.len(),
            });
        }
        Ok(Self(raw.into_bytes()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for RealtimeTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RealtimeTokenSecret(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_token_secret_rejects_short_and_empty_as_too_short() {
        assert_eq!(
            RealtimeTokenSecret::parse(String::new()).unwrap_err(),
            SecretError::TooShort { min: 32, len: 0 },
            "empty is TooShort, not Empty — crm-api maps TooShort to the \
             RealtimeTokenSecretTooShort error from_source always raised"
        );
        assert_eq!(
            RealtimeTokenSecret::parse("a".repeat(31)).unwrap_err(),
            SecretError::TooShort { min: 32, len: 31 },
        );
        assert!(RealtimeTokenSecret::parse("a".repeat(32)).is_ok());
    }

    #[test]
    fn centrifugo_api_key_and_livekit_secret_reject_empty() {
        assert_eq!(
            CentrifugoApiKey::parse(String::new()).unwrap_err(),
            SecretError::Empty
        );
        assert_eq!(
            LiveKitApiSecret::parse(String::new()).unwrap_err(),
            SecretError::Empty
        );
        assert!(CentrifugoApiKey::parse("k".into()).is_ok());
        assert!(LiveKitApiSecret::parse("s".into()).is_ok());
    }

    #[test]
    fn raw_payload_key_round_trips_its_bytes() {
        assert_eq!(RawPayloadKey::new([7u8; 32]).as_bytes(), &[7u8; 32]);
    }
}

/// How an Organization's intake address is rendered (docs/specs/
/// SLICE_007a.md §4; O-014 decision 1). Storage is scheme-neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeAddressScheme {
    /// `leads-<token>@<slug>.<domain>` — the user's preference, default.
    Subdomain,
    /// `<slug>-<token>@leads.<domain>` — for receiving paths without
    /// wildcard-subdomain support.
    LocalPart,
}

impl IntakeAddressScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            IntakeAddressScheme::Subdomain => "subdomain",
            IntakeAddressScheme::LocalPart => "local_part",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "subdomain" => Some(IntakeAddressScheme::Subdomain),
            "local_part" => Some(IntakeAddressScheme::LocalPart),
            _ => None,
        }
    }
}

/// `CRM_INTAKE_MAIL_DOMAIN` + `CRM_INTAKE_ADDRESS_SCHEME`. `domain` is a
/// bare hostname (validated by `Config::from_source`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntakeMailConfig {
    pub domain: String,
    pub scheme: IntakeAddressScheme,
}

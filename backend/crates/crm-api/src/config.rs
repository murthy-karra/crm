use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use crm_operator::GroqApiKey;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 2000;
const MIN_CONNECT_TIMEOUT_MS: u64 = 1;
const MAX_CONNECT_TIMEOUT_MS: u64 = 30_000;

use crm_app::config::MIN_REALTIME_TOKEN_SECRET_BYTES;
pub use crm_app::config::{
    CentrifugoApiKey, InboundEmailSecret, IntakeAddressScheme, IntakeMailConfig, LiveKitApiSecret,
    LiveKitConfig, RawPayloadKey, RealtimeTokenSecret, SecretError, TelephonyConfig,
};

const MIN_SESSION_SECRET_BYTES: usize = 32;
const DEFAULT_SESSION_TTL_HOURS: u64 = 168;
const MIN_SESSION_TTL_HOURS: u64 = 1;
const MAX_SESSION_TTL_HOURS: u64 = 720;

/// The session-token HMAC pepper. `Debug` is redacted so an accidental
/// `{:?}` of `Config` (or anything holding this) never leaks it (AGENTS.md
/// §9).
#[derive(Clone)]
pub struct SessionSecret(Vec<u8>);

impl SessionSecret {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SessionSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionSecret(REDACTED)")
    }
}

const DEFAULT_CENTRIFUGO_API_URL: &str = "http://127.0.0.1:8000/api";
const DEFAULT_REALTIME_TOKEN_TTL_SECONDS: u64 = 600;
const MIN_REALTIME_TOKEN_TTL_SECONDS: u64 = 60;
const MAX_REALTIME_TOKEN_TTL_SECONDS: u64 = 3600;

const DEFAULT_INVITATION_TTL_HOURS: u64 = 168;
const MIN_INVITATION_TTL_HOURS: u64 = 1;
const MAX_INVITATION_TTL_HOURS: u64 = 720;

const RAW_PAYLOAD_KEY_HEX_LEN: usize = 64;

// --- Operator (docs/specs/SLICE_005.md §11) -----------------------------
const DEFAULT_OPERATOR_BASE_URL: &str = "https://api.groq.com/openai/v1";
// `llama-3.3-70b-versatile` (the spec's original default) was retired by
// Groq before the Slice 005 walkthrough; docs/specs/SLICE_005.md §14 item 3
// pre-authorises this switch as a config change, not a contract change.
const DEFAULT_OPERATOR_MODEL: &str = "openai/gpt-oss-120b";
const DEFAULT_OPERATOR_TURN_TIMEOUT_MS: u64 = 20_000;
const MIN_OPERATOR_TURN_TIMEOUT_MS: u64 = 2_000;
const MAX_OPERATOR_TURN_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_OPERATOR_CALL_TIMEOUT_MS: u64 = 10_000;
const MIN_OPERATOR_CALL_TIMEOUT_MS: u64 = 1_000;
const MAX_OPERATOR_CALL_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_OPERATOR_MAX_CONCURRENT: usize = 4;
const DEFAULT_OPERATOR_PROPOSAL_TTL_SECONDS: u64 = 120;
const MIN_OPERATOR_PROPOSAL_TTL_SECONDS: u64 = 30;
const MAX_OPERATOR_PROPOSAL_TTL_SECONDS: u64 = 600;
const MIN_OPERATOR_MAX_CONCURRENT: usize = 1;
const MAX_OPERATOR_MAX_CONCURRENT: usize = 64;

// --- Telephony (docs/specs/SLICE_006.md §11) ----------------------------
const DEFAULT_TELEPHONY_RING_TIMEOUT_SECONDS: u64 = 45;
const MIN_TELEPHONY_RING_TIMEOUT_SECONDS: u64 = 10;
const MAX_TELEPHONY_RING_TIMEOUT_SECONDS: u64 = 120;
const DEFAULT_TELEPHONY_MAX_CALL_SECONDS: u64 = 3600;
const MIN_TELEPHONY_MAX_CALL_SECONDS: u64 = 60;
const MAX_TELEPHONY_MAX_CALL_SECONDS: u64 = 14_400;
const DEFAULT_TELEPHONY_JOIN_TTL_SECONDS: u64 = 300;
const MIN_TELEPHONY_JOIN_TTL_SECONDS: u64 = 60;
const MAX_TELEPHONY_JOIN_TTL_SECONDS: u64 = 900;

/// Operator settings (docs/specs/SLICE_005.md §11). Validated even when no
/// `GROQ_API_KEY` is present, so a keyless dev box still fails fast on a
/// bad value.
#[derive(Debug, Clone)]
pub struct OperatorConfig {
    /// Any OpenAI-compatible endpoint; `https://` required except for
    /// loopback hosts; no trailing slash.
    pub base_url: String,
    pub model: String,
    pub turn_timeout: Duration,
    /// Per provider call; must be ≤ `turn_timeout`.
    pub call_timeout: Duration,
    pub max_concurrent: usize,
    /// How long a `start_call` proposal stays confirmable
    /// (docs/specs/SLICE_006b.md §2).
    pub proposal_ttl: Duration,
}

/// Extraction-worker settings (docs/specs/SLICE_007f.md §5).
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    pub poll_interval: Duration,
    pub model: String,
    /// Per-LLM-call budget; the lease invariant below keeps the whole
    /// attempt under the 60 s claim lease.
    pub call_timeout: Duration,
}

const DEFAULT_EXTRACTION_POLL_SECONDS: u64 = 10;
const MIN_EXTRACTION_POLL_SECONDS: u64 = 1;
const MAX_EXTRACTION_POLL_SECONDS: u64 = 300;
const DEFAULT_EXTRACTION_CALL_TIMEOUT_MS: u64 = 15_000;
const MIN_EXTRACTION_CALL_TIMEOUT_MS: u64 = 1_000;
const MAX_EXTRACTION_CALL_TIMEOUT_MS: u64 = 30_000;
/// Overhead allowance in the lease invariant (spec §5): call timeout +
/// the advisory-lock budget + this must stay under the claim lease.
const EXTRACTION_LEASE_OVERHEAD_MS: u64 = 5_000;

/// No `hex` crate dependency for one 32-byte decode (docs/specs/SLICE_002.md
/// §14a: "no other new backend dependencies are expected" beyond
/// chacha20poly1305).
fn decode_hex_32(raw: &str) -> Option<[u8; 32]> {
    if raw.len() != RAW_PAYLOAD_KEY_HEX_LEN {
        return None;
    }
    let bytes = raw.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = (bytes[i * 2] as char).to_digit(16)?;
        let lo = (bytes[i * 2 + 1] as char).to_digit(16)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: Option<String>,
    pub database_connect_timeout: Duration,
    pub session_secret: SessionSecret,
    pub session_ttl: Duration,
    pub session_cookie_secure: bool,
    /// When set, adds a CORS layer permitting exactly this origin (with
    /// credentials) — needed only when the browser app and API are served
    /// from different hostnames (e.g. app.tarams.org calling
    /// api.tarams.org through the tunnel). Unset means no CORS layer at
    /// all, which is the same-origin default for loopback dev.
    pub cors_allowed_origin: Option<String>,
    /// When set, scopes the session cookie's `Domain` attribute to this
    /// value instead of leaving it host-only. Only needed alongside
    /// `cors_allowed_origin` for the same cross-subdomain case.
    pub session_cookie_domain: Option<String>,
    /// Raw lead-payload encryption key (docs/specs/SLICE_002.md §7).
    pub raw_payload_key: RawPayloadKey,
    /// Centrifugo HTTP API publish credential (docs/specs/SLICE_003.md
    /// §11).
    pub centrifugo_api_key: CentrifugoApiKey,
    /// Centrifugo connection-token HMAC signing secret
    /// (docs/specs/SLICE_003.md §6, §11).
    pub realtime_token_secret: RealtimeTokenSecret,
    /// Centrifugo HTTP API base URL, e.g. `http://127.0.0.1:8000/api`.
    /// `http://` only, no trailing slash (docs/specs/SLICE_003.md §11).
    pub centrifugo_api_url: String,
    /// Connection-token lifetime, bounds 60–3600s
    /// (docs/specs/SLICE_003.md §6, §11).
    pub realtime_token_ttl: Duration,
    /// Invitation expiry, bounds 1–720 hours, default 168 (7 days)
    /// (docs/specs/SLICE_004.md §11).
    pub invitation_ttl: Duration,
    /// Operator settings (docs/specs/SLICE_005.md §11); always validated.
    pub operator: OperatorConfig,
    /// `GROQ_API_KEY`; `None` (unset or empty) disables the Operator
    /// without failing startup (docs/specs/SLICE_005.md §9, §14 item 5).
    pub groq_api_key: Option<GroqApiKey>,
    /// Extraction-worker settings (docs/specs/SLICE_007f.md §5); always
    /// validated. The worker itself is spawned only when `groq_api_key`
    /// is set.
    pub extraction: ExtractionConfig,
    /// Telephony settings (docs/specs/SLICE_006.md §11); limits always
    /// validated, LiveKit present only with `LIVEKIT_API_KEY`.
    pub telephony: TelephonyConfig,
    /// Slice 007a: how Organization intake addresses are rendered.
    pub intake_mail: IntakeMailConfig,
    /// Slice 007b: `POST /inbound/email` bearer credential. `None` (unset
    /// or empty) disables the endpoint (every request 401) without
    /// failing startup, mirroring `LIVEKIT_API_KEY`.
    pub inbound_email_secret: Option<InboundEmailSecret>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    InvalidBindAddr(String),
    NonLoopbackBindAddr(SocketAddr),
    InvalidConnectTimeout(String),
    ConnectTimeoutOutOfBounds(u64),
    MissingSessionSecret,
    SessionSecretTooShort(usize),
    InvalidSessionTtl(String),
    SessionTtlOutOfBounds(u64),
    InvalidCookieSecure(String),
    InvalidCorsOrigin(String),
    InvalidCookieDomain(String),
    MissingRawPayloadKey,
    InvalidRawPayloadKeyLength(usize),
    InvalidRawPayloadKeyEncoding,
    MissingCentrifugoApiKey,
    MissingRealtimeTokenSecret,
    RealtimeTokenSecretTooShort(usize),
    InvalidCentrifugoApiUrl(String),
    InvalidRealtimeTokenTtl(String),
    RealtimeTokenTtlOutOfBounds(u64),
    InvalidInvitationTtl(String),
    InvitationTtlOutOfBounds(u64),
    InvalidOperatorBaseUrl(String),
    InvalidOperatorTurnTimeout(String),
    OperatorTurnTimeoutOutOfBounds(u64),
    InvalidOperatorCallTimeout(String),
    OperatorCallTimeoutOutOfBounds(u64),
    OperatorCallTimeoutExceedsTurnTimeout {
        call_ms: u64,
        turn_ms: u64,
    },
    InvalidOperatorMaxConcurrent(String),
    OperatorMaxConcurrentOutOfBounds(usize),
    InvalidOperatorProposalTtl(String),
    InvalidExtractionPollSeconds(String),
    ExtractionPollSecondsOutOfBounds(u64),
    InvalidExtractionCallTimeout(String),
    ExtractionCallTimeoutOutOfBounds(u64),
    /// The lease invariant (docs/specs/SLICE_007f.md §5): a slow attempt
    /// under a lapsed claim lease could be double-extracted.
    ExtractionCallTimeoutBreaksLease {
        call_ms: u64,
        lease_ms: u64,
    },
    OperatorProposalTtlOutOfBounds(u64),
    // --- Slice 006 (docs/specs/SLICE_006.md §11) -------------------------
    MissingLiveKitUrl,
    InvalidLiveKitUrl(String),
    MissingLiveKitApiUrl,
    InvalidLiveKitApiUrl(String),
    MissingLiveKitApiSecret,
    MissingLiveKitSipOutboundTrunkId,
    InvalidTelephonyRingTimeout(String),
    TelephonyRingTimeoutOutOfBounds(u64),
    InvalidTelephonyMaxCall(String),
    TelephonyMaxCallOutOfBounds(u64),
    InvalidTelephonyJoinTtl(String),
    TelephonyJoinTtlOutOfBounds(u64),
    // --- Slice 007a (docs/specs/SLICE_007a.md §4) ----------------------
    InvalidIntakeMailDomain(String),
    InvalidIntakeAddressScheme(String),
    // --- Slice 007b (docs/specs/SLICE_007b.md §6) -----------------------
    InboundEmailSecretTooShort(usize),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidBindAddr(value) => {
                write!(f, "CRM_API_BIND_ADDR is not a valid socket address: {value}")
            }
            ConfigError::NonLoopbackBindAddr(addr) => write!(
                f,
                "CRM_API_BIND_ADDR must be a loopback address in development, got {addr}"
            ),
            ConfigError::InvalidConnectTimeout(value) => write!(
                f,
                "CRM_DATABASE_CONNECT_TIMEOUT_MS is not a valid integer: {value}"
            ),
            ConfigError::ConnectTimeoutOutOfBounds(value) => write!(
                f,
                "CRM_DATABASE_CONNECT_TIMEOUT_MS must be between {MIN_CONNECT_TIMEOUT_MS} and {MAX_CONNECT_TIMEOUT_MS}, got {value}"
            ),
            ConfigError::MissingSessionSecret => write!(f, "CRM_SESSION_SECRET is required"),
            ConfigError::SessionSecretTooShort(len) => write!(
                f,
                "CRM_SESSION_SECRET must be at least {MIN_SESSION_SECRET_BYTES} bytes, got {len}"
            ),
            ConfigError::InvalidSessionTtl(value) => {
                write!(f, "CRM_SESSION_TTL_HOURS is not a valid integer: {value}")
            }
            ConfigError::SessionTtlOutOfBounds(value) => write!(
                f,
                "CRM_SESSION_TTL_HOURS must be between {MIN_SESSION_TTL_HOURS} and {MAX_SESSION_TTL_HOURS}, got {value}"
            ),
            ConfigError::InvalidCookieSecure(value) => write!(
                f,
                "CRM_SESSION_COOKIE_SECURE must be \"true\" or \"false\", got {value}"
            ),
            ConfigError::InvalidCorsOrigin(value) => write!(
                f,
                "CRM_CORS_ALLOWED_ORIGIN must be a bare http(s) origin with no path or trailing slash, got {value}"
            ),
            ConfigError::InvalidCookieDomain(value) => {
                write!(f, "CRM_SESSION_COOKIE_DOMAIN must not be empty or contain whitespace, got {value}")
            }
            ConfigError::MissingRawPayloadKey => write!(f, "CRM_RAW_PAYLOAD_KEY is required"),
            ConfigError::InvalidRawPayloadKeyLength(len) => write!(
                f,
                "CRM_RAW_PAYLOAD_KEY must be exactly {RAW_PAYLOAD_KEY_HEX_LEN} hex characters, got {len}"
            ),
            ConfigError::InvalidRawPayloadKeyEncoding => {
                write!(f, "CRM_RAW_PAYLOAD_KEY must be hex-encoded")
            }
            ConfigError::MissingCentrifugoApiKey => {
                write!(f, "CENTRIFUGO_HTTP_API_KEY is required")
            }
            ConfigError::MissingRealtimeTokenSecret => {
                write!(f, "CENTRIFUGO_TOKEN_HMAC_SECRET is required")
            }
            ConfigError::RealtimeTokenSecretTooShort(len) => write!(
                f,
                "CENTRIFUGO_TOKEN_HMAC_SECRET must be at least {MIN_REALTIME_TOKEN_SECRET_BYTES} bytes, got {len}"
            ),
            ConfigError::InvalidCentrifugoApiUrl(value) => write!(
                f,
                "CRM_CENTRIFUGO_API_URL must be a bare http:// URL with no trailing slash, got {value}"
            ),
            ConfigError::InvalidRealtimeTokenTtl(value) => write!(
                f,
                "CRM_REALTIME_TOKEN_TTL_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::RealtimeTokenTtlOutOfBounds(value) => write!(
                f,
                "CRM_REALTIME_TOKEN_TTL_SECONDS must be between {MIN_REALTIME_TOKEN_TTL_SECONDS} and {MAX_REALTIME_TOKEN_TTL_SECONDS}, got {value}"
            ),
            ConfigError::InvalidInvitationTtl(value) => write!(
                f,
                "CRM_INVITATION_TTL_HOURS is not a valid integer: {value}"
            ),
            ConfigError::InvitationTtlOutOfBounds(value) => write!(
                f,
                "CRM_INVITATION_TTL_HOURS must be between {MIN_INVITATION_TTL_HOURS} and {MAX_INVITATION_TTL_HOURS}, got {value}"
            ),
            ConfigError::InvalidOperatorBaseUrl(value) => write!(
                f,
                "CRM_OPERATOR_BASE_URL must be an https:// URL (http:// only for loopback) with no trailing slash, got {value}"
            ),
            ConfigError::InvalidOperatorTurnTimeout(value) => write!(
                f,
                "CRM_OPERATOR_TURN_TIMEOUT_MS is not a valid integer: {value}"
            ),
            ConfigError::OperatorTurnTimeoutOutOfBounds(value) => write!(
                f,
                "CRM_OPERATOR_TURN_TIMEOUT_MS must be between {MIN_OPERATOR_TURN_TIMEOUT_MS} and {MAX_OPERATOR_TURN_TIMEOUT_MS}, got {value}"
            ),
            ConfigError::InvalidOperatorCallTimeout(value) => write!(
                f,
                "CRM_OPERATOR_CALL_TIMEOUT_MS is not a valid integer: {value}"
            ),
            ConfigError::InvalidExtractionPollSeconds(value) => write!(
                f,
                "CRM_EXTRACTION_POLL_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::ExtractionPollSecondsOutOfBounds(value) => write!(
                f,
                "CRM_EXTRACTION_POLL_SECONDS must be between {MIN_EXTRACTION_POLL_SECONDS} and {MAX_EXTRACTION_POLL_SECONDS}, got {value}"
            ),
            ConfigError::InvalidExtractionCallTimeout(value) => write!(
                f,
                "CRM_EXTRACTION_CALL_TIMEOUT_MS is not a valid integer: {value}"
            ),
            ConfigError::ExtractionCallTimeoutOutOfBounds(value) => write!(
                f,
                "CRM_EXTRACTION_CALL_TIMEOUT_MS must be between {MIN_EXTRACTION_CALL_TIMEOUT_MS} and {MAX_EXTRACTION_CALL_TIMEOUT_MS}, got {value}"
            ),
            ConfigError::ExtractionCallTimeoutBreaksLease { call_ms, lease_ms } => write!(
                f,
                "CRM_EXTRACTION_CALL_TIMEOUT_MS ({call_ms}) plus lock budget and overhead must stay under the {lease_ms} ms extraction claim lease (docs/specs/SLICE_007f.md §5)"
            ),
            ConfigError::OperatorCallTimeoutOutOfBounds(value) => write!(
                f,
                "CRM_OPERATOR_CALL_TIMEOUT_MS must be between {MIN_OPERATOR_CALL_TIMEOUT_MS} and {MAX_OPERATOR_CALL_TIMEOUT_MS}, got {value}"
            ),
            ConfigError::OperatorCallTimeoutExceedsTurnTimeout { call_ms, turn_ms } => write!(
                f,
                "CRM_OPERATOR_CALL_TIMEOUT_MS ({call_ms}) must not exceed CRM_OPERATOR_TURN_TIMEOUT_MS ({turn_ms})"
            ),
            ConfigError::InvalidOperatorMaxConcurrent(value) => write!(
                f,
                "CRM_OPERATOR_MAX_CONCURRENT is not a valid integer: {value}"
            ),
            ConfigError::OperatorMaxConcurrentOutOfBounds(value) => write!(
                f,
                "CRM_OPERATOR_MAX_CONCURRENT must be between {MIN_OPERATOR_MAX_CONCURRENT} and {MAX_OPERATOR_MAX_CONCURRENT}, got {value}"
            ),
            ConfigError::InvalidOperatorProposalTtl(value) => write!(
                f,
                "CRM_OPERATOR_PROPOSAL_TTL_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::OperatorProposalTtlOutOfBounds(value) => write!(
                f,
                "CRM_OPERATOR_PROPOSAL_TTL_SECONDS must be between {MIN_OPERATOR_PROPOSAL_TTL_SECONDS} and {MAX_OPERATOR_PROPOSAL_TTL_SECONDS}, got {value}"
            ),
            ConfigError::MissingLiveKitUrl => {
                write!(f, "LIVEKIT_URL is required when LIVEKIT_API_KEY is set")
            }
            ConfigError::InvalidLiveKitUrl(value) => write!(
                f,
                "LIVEKIT_URL must be a ws:// or wss:// URL with no trailing slash, got {value}"
            ),
            ConfigError::MissingLiveKitApiUrl => {
                write!(f, "LIVEKIT_API_URL is required when LIVEKIT_API_KEY is set")
            }
            ConfigError::InvalidLiveKitApiUrl(value) => write!(
                f,
                "LIVEKIT_API_URL must be an https:// URL (http:// only for loopback) with no trailing slash, got {value}"
            ),
            ConfigError::MissingLiveKitApiSecret => {
                write!(f, "LIVEKIT_API_SECRET is required when LIVEKIT_API_KEY is set")
            }
            ConfigError::MissingLiveKitSipOutboundTrunkId => write!(
                f,
                "LIVEKIT_SIP_OUTBOUND_TRUNK_ID is required when LIVEKIT_API_KEY is set"
            ),
            ConfigError::InvalidTelephonyRingTimeout(value) => write!(
                f,
                "CRM_TELEPHONY_RING_TIMEOUT_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::TelephonyRingTimeoutOutOfBounds(value) => write!(
                f,
                "CRM_TELEPHONY_RING_TIMEOUT_SECONDS must be between {MIN_TELEPHONY_RING_TIMEOUT_SECONDS} and {MAX_TELEPHONY_RING_TIMEOUT_SECONDS}, got {value}"
            ),
            ConfigError::InvalidTelephonyMaxCall(value) => write!(
                f,
                "CRM_TELEPHONY_MAX_CALL_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::TelephonyMaxCallOutOfBounds(value) => write!(
                f,
                "CRM_TELEPHONY_MAX_CALL_SECONDS must be between {MIN_TELEPHONY_MAX_CALL_SECONDS} and {MAX_TELEPHONY_MAX_CALL_SECONDS}, got {value}"
            ),
            ConfigError::InvalidTelephonyJoinTtl(value) => write!(
                f,
                "CRM_TELEPHONY_JOIN_TTL_SECONDS is not a valid integer: {value}"
            ),
            ConfigError::TelephonyJoinTtlOutOfBounds(value) => write!(
                f,
                "CRM_TELEPHONY_JOIN_TTL_SECONDS must be between {MIN_TELEPHONY_JOIN_TTL_SECONDS} and {MAX_TELEPHONY_JOIN_TTL_SECONDS}, got {value}"
            ),
            ConfigError::InvalidIntakeMailDomain(value) => write!(
                f,
                "CRM_INTAKE_MAIL_DOMAIN must be a bare hostname (no scheme, port, path, or trailing dot), got {value}"
            ),
            ConfigError::InvalidIntakeAddressScheme(value) => write!(
                f,
                "CRM_INTAKE_ADDRESS_SCHEME must be \"subdomain\" or \"local_part\", got {value}"
            ),
            ConfigError::InboundEmailSecretTooShort(len) => write!(
                f,
                "CRM_INBOUND_EMAIL_SECRET must be at least 32 bytes when set, got {len}"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Reads configuration from the process environment. Callers that need
    /// `.env` values loaded first must call `dotenvy::dotenv()` before this.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_source(|key| std::env::var(key).ok())
    }

    /// Builds configuration from an arbitrary key lookup so tests never
    /// depend on ambient process environment or `.env` state.
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind_addr_raw =
            get("CRM_API_BIND_ADDR").unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());
        let bind_addr: SocketAddr = bind_addr_raw
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddr(bind_addr_raw.clone()))?;
        if !bind_addr.ip().is_loopback() {
            return Err(ConfigError::NonLoopbackBindAddr(bind_addr));
        }

        let database_url = get("DATABASE_URL");

        let timeout_ms = match get("CRM_DATABASE_CONNECT_TIMEOUT_MS") {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidConnectTimeout(value.clone()))?,
            None => DEFAULT_CONNECT_TIMEOUT_MS,
        };
        if !(MIN_CONNECT_TIMEOUT_MS..=MAX_CONNECT_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(ConfigError::ConnectTimeoutOutOfBounds(timeout_ms));
        }

        let session_secret_raw =
            get("CRM_SESSION_SECRET").ok_or(ConfigError::MissingSessionSecret)?;
        if session_secret_raw.len() < MIN_SESSION_SECRET_BYTES {
            return Err(ConfigError::SessionSecretTooShort(session_secret_raw.len()));
        }
        let session_secret = SessionSecret(session_secret_raw.into_bytes());

        let ttl_hours = match get("CRM_SESSION_TTL_HOURS") {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidSessionTtl(value.clone()))?,
            None => DEFAULT_SESSION_TTL_HOURS,
        };
        if !(MIN_SESSION_TTL_HOURS..=MAX_SESSION_TTL_HOURS).contains(&ttl_hours) {
            return Err(ConfigError::SessionTtlOutOfBounds(ttl_hours));
        }
        let session_ttl = Duration::from_secs(ttl_hours * 3600);

        let session_cookie_secure = match get("CRM_SESSION_COOKIE_SECURE") {
            Some(value) => match value.as_str() {
                "true" => true,
                "false" => false,
                _ => return Err(ConfigError::InvalidCookieSecure(value.clone())),
            },
            None => false,
        };

        let cors_allowed_origin = match get("CRM_CORS_ALLOWED_ORIGIN").filter(|v| !v.is_empty()) {
            Some(value) if is_plausible_origin(&value) => Some(value),
            Some(value) => return Err(ConfigError::InvalidCorsOrigin(value)),
            None => None,
        };

        let session_cookie_domain = match get("CRM_SESSION_COOKIE_DOMAIN").filter(|v| !v.is_empty())
        {
            Some(value) if !value.contains(char::is_whitespace) => Some(value),
            Some(value) => return Err(ConfigError::InvalidCookieDomain(value)),
            None => None,
        };

        let raw_payload_key = raw_payload_key_config(&get)?;

        let centrifugo_api_key_raw = get("CENTRIFUGO_HTTP_API_KEY")
            .filter(|v| !v.is_empty())
            .ok_or(ConfigError::MissingCentrifugoApiKey)?;
        // Unreachable (the .filter above already rejects ""), kept for
        // totality; the variant matches what that filter raises.
        let centrifugo_api_key = CentrifugoApiKey::parse(centrifugo_api_key_raw)
            .map_err(|_| ConfigError::MissingCentrifugoApiKey)?;

        let realtime_token_secret_raw =
            get("CENTRIFUGO_TOKEN_HMAC_SECRET").ok_or(ConfigError::MissingRealtimeTokenSecret)?;
        let realtime_token_secret =
            RealtimeTokenSecret::parse(realtime_token_secret_raw).map_err(|err| match err {
                SecretError::TooShort { len, .. } => ConfigError::RealtimeTokenSecretTooShort(len),
                // Unreachable ("" is TooShort{len:0}), kept for totality.
                SecretError::Empty => ConfigError::MissingRealtimeTokenSecret,
            })?;

        let centrifugo_api_url = match get("CRM_CENTRIFUGO_API_URL").filter(|v| !v.is_empty()) {
            Some(value) if is_plausible_centrifugo_url(&value) => value,
            Some(value) => return Err(ConfigError::InvalidCentrifugoApiUrl(value)),
            None => DEFAULT_CENTRIFUGO_API_URL.to_string(),
        };

        let realtime_ttl_secs = match get("CRM_REALTIME_TOKEN_TTL_SECONDS") {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidRealtimeTokenTtl(value.clone()))?,
            None => DEFAULT_REALTIME_TOKEN_TTL_SECONDS,
        };
        if !(MIN_REALTIME_TOKEN_TTL_SECONDS..=MAX_REALTIME_TOKEN_TTL_SECONDS)
            .contains(&realtime_ttl_secs)
        {
            return Err(ConfigError::RealtimeTokenTtlOutOfBounds(realtime_ttl_secs));
        }
        let realtime_token_ttl = Duration::from_secs(realtime_ttl_secs);

        let invitation_ttl_hours = match get("CRM_INVITATION_TTL_HOURS") {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidInvitationTtl(value.clone()))?,
            None => DEFAULT_INVITATION_TTL_HOURS,
        };
        if !(MIN_INVITATION_TTL_HOURS..=MAX_INVITATION_TTL_HOURS).contains(&invitation_ttl_hours) {
            return Err(ConfigError::InvitationTtlOutOfBounds(invitation_ttl_hours));
        }
        let invitation_ttl = Duration::from_secs(invitation_ttl_hours * 3600);

        let operator = operator_config(&get)?;
        let extraction = extraction_config(&get)?;
        let groq_api_key = get("GROQ_API_KEY")
            .filter(|v| !v.trim().is_empty())
            .map(GroqApiKey::new);

        let telephony = telephony_config(&get)?;
        let intake_mail = intake_mail_config(&get)?;

        let inbound_email_secret = match get("CRM_INBOUND_EMAIL_SECRET").filter(|v| !v.is_empty()) {
            Some(value) => Some(InboundEmailSecret::parse(value).map_err(|err| match err {
                SecretError::TooShort { len, .. } => ConfigError::InboundEmailSecretTooShort(len),
                // Unreachable (the .filter above already rejects ""), kept
                // for totality.
                SecretError::Empty => ConfigError::InboundEmailSecretTooShort(0),
            })?),
            None => None,
        };

        Ok(Config {
            bind_addr,
            database_url,
            database_connect_timeout: Duration::from_millis(timeout_ms),
            session_secret,
            session_ttl,
            session_cookie_secure,
            cors_allowed_origin,
            session_cookie_domain,
            raw_payload_key,
            centrifugo_api_key,
            realtime_token_secret,
            centrifugo_api_url,
            realtime_token_ttl,
            invitation_ttl,
            operator,
            groq_api_key,
            extraction,
            telephony,
            intake_mail,
            inbound_email_secret,
        })
    }
}

/// `LIVEKIT_*` / `CRM_TELEPHONY_*` (docs/specs/SLICE_006.md §11). The
/// bounds and URL rules are validated regardless of whether a key is
/// present; an empty/unset `LIVEKIT_API_KEY` disables calling without
/// failing startup.
const DEFAULT_INTAKE_MAIL_DOMAIN: &str = "elysianfeld.com";

/// `CRM_RAW_PAYLOAD_KEY` alone (docs/specs/SLICE_002.md §7): exactly 64 hex
/// characters, decoded once. Public so `crm-admin receive-inquiry`
/// (docs/specs/SLICE_007c.md §4) can build a `RawPayloadKey` from the same
/// variable the API uses without a full `Config` — the `intake_mail_config`
/// precedent above.
pub fn raw_payload_key_config(
    get: &impl Fn(&str) -> Option<String>,
) -> Result<RawPayloadKey, ConfigError> {
    let raw = get("CRM_RAW_PAYLOAD_KEY").ok_or(ConfigError::MissingRawPayloadKey)?;
    if raw.len() != RAW_PAYLOAD_KEY_HEX_LEN {
        return Err(ConfigError::InvalidRawPayloadKeyLength(raw.len()));
    }
    Ok(RawPayloadKey::new(
        decode_hex_32(&raw).ok_or(ConfigError::InvalidRawPayloadKeyEncoding)?,
    ))
}

/// `CRM_INTAKE_MAIL_DOMAIN` (bare hostname; default `elysianfeld.com`) and
/// `CRM_INTAKE_ADDRESS_SCHEME` (`subdomain` default | `local_part`)
/// (docs/specs/SLICE_007a.md §4). Public so `crm-admin` can render an
/// address from the same two variables without a full `Config`.
pub fn intake_mail_config(
    get: &impl Fn(&str) -> Option<String>,
) -> Result<IntakeMailConfig, ConfigError> {
    let domain = match get("CRM_INTAKE_MAIL_DOMAIN").map(|v| v.trim().to_string()) {
        Some(value) if !value.is_empty() => value,
        _ => DEFAULT_INTAKE_MAIL_DOMAIN.to_string(),
    };
    if !is_bare_hostname(&domain) {
        return Err(ConfigError::InvalidIntakeMailDomain(domain));
    }
    let scheme = match get("CRM_INTAKE_ADDRESS_SCHEME").map(|v| v.trim().to_string()) {
        Some(value) if !value.is_empty() => IntakeAddressScheme::parse(&value)
            .ok_or(ConfigError::InvalidIntakeAddressScheme(value))?,
        _ => IntakeAddressScheme::Subdomain,
    };
    Ok(IntakeMailConfig { domain, scheme })
}

/// Labels of `[a-z0-9-]`, dot-separated, at least two, no leading/trailing
/// dash or dot, no scheme/port/path.
fn is_bare_hostname(value: &str) -> bool {
    let labels: Vec<&str> = value.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn telephony_config(get: &impl Fn(&str) -> Option<String>) -> Result<TelephonyConfig, ConfigError> {
    let ring_secs = match get("CRM_TELEPHONY_RING_TIMEOUT_SECONDS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidTelephonyRingTimeout(value.clone()))?,
        None => DEFAULT_TELEPHONY_RING_TIMEOUT_SECONDS,
    };
    if !(MIN_TELEPHONY_RING_TIMEOUT_SECONDS..=MAX_TELEPHONY_RING_TIMEOUT_SECONDS)
        .contains(&ring_secs)
    {
        return Err(ConfigError::TelephonyRingTimeoutOutOfBounds(ring_secs));
    }

    let max_call_secs = match get("CRM_TELEPHONY_MAX_CALL_SECONDS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidTelephonyMaxCall(value.clone()))?,
        None => DEFAULT_TELEPHONY_MAX_CALL_SECONDS,
    };
    if !(MIN_TELEPHONY_MAX_CALL_SECONDS..=MAX_TELEPHONY_MAX_CALL_SECONDS).contains(&max_call_secs) {
        return Err(ConfigError::TelephonyMaxCallOutOfBounds(max_call_secs));
    }

    let join_ttl_secs = match get("CRM_TELEPHONY_JOIN_TTL_SECONDS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidTelephonyJoinTtl(value.clone()))?,
        None => DEFAULT_TELEPHONY_JOIN_TTL_SECONDS,
    };
    if !(MIN_TELEPHONY_JOIN_TTL_SECONDS..=MAX_TELEPHONY_JOIN_TTL_SECONDS).contains(&join_ttl_secs) {
        return Err(ConfigError::TelephonyJoinTtlOutOfBounds(join_ttl_secs));
    }

    // URL rules are validated even when calling is disabled, so a
    // half-configured `.env` fails fast rather than at the first call.
    let url = match get("LIVEKIT_URL").filter(|v| !v.is_empty()) {
        Some(value) if is_plausible_livekit_ws_url(&value) => Some(value),
        Some(value) => return Err(ConfigError::InvalidLiveKitUrl(value)),
        None => None,
    };
    let api_url = match get("LIVEKIT_API_URL").filter(|v| !v.is_empty()) {
        Some(value) if is_plausible_operator_url(&value) => Some(value),
        Some(value) => return Err(ConfigError::InvalidLiveKitApiUrl(value)),
        None => None,
    };

    let livekit = match get("LIVEKIT_API_KEY")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        None => None,
        Some(api_key) => {
            let url = url.ok_or(ConfigError::MissingLiveKitUrl)?;
            let api_url = api_url.ok_or(ConfigError::MissingLiveKitApiUrl)?;
            let api_secret = get("LIVEKIT_API_SECRET")
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or(ConfigError::MissingLiveKitApiSecret)?;
            let sip_outbound_trunk_id = get("LIVEKIT_SIP_OUTBOUND_TRUNK_ID")
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or(ConfigError::MissingLiveKitSipOutboundTrunkId)?;
            Some(LiveKitConfig {
                url,
                api_url,
                api_key,
                // Unreachable (trimmed + filtered non-empty above), kept
                // for totality; parse's contract is "caller trims".
                api_secret: LiveKitApiSecret::parse(api_secret)
                    .map_err(|_| ConfigError::MissingLiveKitApiSecret)?,
                sip_outbound_trunk_id,
            })
        }
    };

    Ok(TelephonyConfig {
        livekit,
        ring_timeout: Duration::from_secs(ring_secs),
        max_call: Duration::from_secs(max_call_secs),
        join_ttl: Duration::from_secs(join_ttl_secs),
    })
}

/// `LIVEKIT_URL` (docs/specs/SLICE_006.md §11): the browser signaling URL,
/// `ws://` or `wss://`, no trailing slash, no whitespace.
fn is_plausible_livekit_ws_url(value: &str) -> bool {
    if value.contains(char::is_whitespace) || value.ends_with('/') {
        return false;
    }
    value
        .strip_prefix("wss://")
        .or_else(|| value.strip_prefix("ws://"))
        .is_some_and(|rest| !rest.is_empty())
}

/// `CRM_OPERATOR_*` (docs/specs/SLICE_005.md §11).
fn operator_config(get: &impl Fn(&str) -> Option<String>) -> Result<OperatorConfig, ConfigError> {
    let base_url = match get("CRM_OPERATOR_BASE_URL").filter(|v| !v.is_empty()) {
        Some(value) if is_plausible_operator_url(&value) => value,
        Some(value) => return Err(ConfigError::InvalidOperatorBaseUrl(value)),
        None => DEFAULT_OPERATOR_BASE_URL.to_string(),
    };

    let model = get("CRM_OPERATOR_MODEL")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_OPERATOR_MODEL.to_string());

    let turn_ms = match get("CRM_OPERATOR_TURN_TIMEOUT_MS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidOperatorTurnTimeout(value.clone()))?,
        None => DEFAULT_OPERATOR_TURN_TIMEOUT_MS,
    };
    if !(MIN_OPERATOR_TURN_TIMEOUT_MS..=MAX_OPERATOR_TURN_TIMEOUT_MS).contains(&turn_ms) {
        return Err(ConfigError::OperatorTurnTimeoutOutOfBounds(turn_ms));
    }

    let call_ms = match get("CRM_OPERATOR_CALL_TIMEOUT_MS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidOperatorCallTimeout(value.clone()))?,
        None => DEFAULT_OPERATOR_CALL_TIMEOUT_MS,
    };
    if !(MIN_OPERATOR_CALL_TIMEOUT_MS..=MAX_OPERATOR_CALL_TIMEOUT_MS).contains(&call_ms) {
        return Err(ConfigError::OperatorCallTimeoutOutOfBounds(call_ms));
    }
    if call_ms > turn_ms {
        return Err(ConfigError::OperatorCallTimeoutExceedsTurnTimeout { call_ms, turn_ms });
    }

    let max_concurrent = match get("CRM_OPERATOR_MAX_CONCURRENT") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| ConfigError::InvalidOperatorMaxConcurrent(value.clone()))?,
        None => DEFAULT_OPERATOR_MAX_CONCURRENT,
    };
    if !(MIN_OPERATOR_MAX_CONCURRENT..=MAX_OPERATOR_MAX_CONCURRENT).contains(&max_concurrent) {
        return Err(ConfigError::OperatorMaxConcurrentOutOfBounds(
            max_concurrent,
        ));
    }

    let proposal_ttl_secs = match get("CRM_OPERATOR_PROPOSAL_TTL_SECONDS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidOperatorProposalTtl(value.clone()))?,
        None => DEFAULT_OPERATOR_PROPOSAL_TTL_SECONDS,
    };
    if !(MIN_OPERATOR_PROPOSAL_TTL_SECONDS..=MAX_OPERATOR_PROPOSAL_TTL_SECONDS)
        .contains(&proposal_ttl_secs)
    {
        return Err(ConfigError::OperatorProposalTtlOutOfBounds(
            proposal_ttl_secs,
        ));
    }

    Ok(OperatorConfig {
        base_url,
        model,
        turn_timeout: Duration::from_millis(turn_ms),
        call_timeout: Duration::from_millis(call_ms),
        max_concurrent,
        proposal_ttl: Duration::from_secs(proposal_ttl_secs),
    })
}

/// `CRM_EXTRACTION_*` (docs/specs/SLICE_007f.md §5). Always validated;
/// the worker is gated separately on `GROQ_API_KEY`.
fn extraction_config(
    get: &impl Fn(&str) -> Option<String>,
) -> Result<ExtractionConfig, ConfigError> {
    let poll_secs = match get("CRM_EXTRACTION_POLL_SECONDS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidExtractionPollSeconds(value.clone()))?,
        None => DEFAULT_EXTRACTION_POLL_SECONDS,
    };
    if !(MIN_EXTRACTION_POLL_SECONDS..=MAX_EXTRACTION_POLL_SECONDS).contains(&poll_secs) {
        return Err(ConfigError::ExtractionPollSecondsOutOfBounds(poll_secs));
    }

    let model = get("CRM_EXTRACTION_MODEL")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_OPERATOR_MODEL.to_string());

    let call_ms = match get("CRM_EXTRACTION_CALL_TIMEOUT_MS") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidExtractionCallTimeout(value.clone()))?,
        None => DEFAULT_EXTRACTION_CALL_TIMEOUT_MS,
    };
    if !(MIN_EXTRACTION_CALL_TIMEOUT_MS..=MAX_EXTRACTION_CALL_TIMEOUT_MS).contains(&call_ms) {
        return Err(ConfigError::ExtractionCallTimeoutOutOfBounds(call_ms));
    }

    // The lease invariant (spec §5): call + advisory-lock budget +
    // overhead < the 60 s claim lease, or a slow attempt under a lapsed
    // lease could be double-extracted.
    let lease_ms = crm_app::domain::intake::extraction::worker::TRANSPORT_RETRY.as_millis() as u64;
    let budget_ms =
        crm_app::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET.as_millis() as u64;
    if call_ms + budget_ms + EXTRACTION_LEASE_OVERHEAD_MS > lease_ms {
        return Err(ConfigError::ExtractionCallTimeoutBreaksLease { call_ms, lease_ms });
    }

    Ok(ExtractionConfig {
        poll_interval: Duration::from_secs(poll_secs),
        model,
        call_timeout: Duration::from_millis(call_ms),
    })
}

/// `CRM_OPERATOR_BASE_URL` (docs/specs/SLICE_005.md §11): `https://`
/// required because the key travels as a bearer header; `http://` only for
/// loopback hosts (the same exception `centrifugo_api_url` relies on); no
/// trailing slash, no whitespace.
fn is_plausible_operator_url(value: &str) -> bool {
    if value.contains(char::is_whitespace) || value.ends_with('/') {
        return false;
    }
    if value.starts_with("https://") {
        return value.len() > "https://".len();
    }
    if let Some(rest) = value.strip_prefix("http://") {
        let authority = rest.split('/').next().unwrap_or("");
        let host = authority
            .strip_prefix('[')
            .and_then(|s| s.split(']').next())
            .unwrap_or_else(|| authority.rsplit_once(':').map_or(authority, |(h, _)| h));
        return matches!(host, "127.0.0.1" | "localhost" | "::1")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
    }
    false
}

fn is_plausible_origin(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://"))
        && !value.contains(char::is_whitespace)
        && !value.ends_with('/')
}

/// `CRM_CENTRIFUGO_API_URL` validation (docs/specs/SLICE_003.md §11):
/// `http://` only (Centrifugo's HTTP API is a loopback call; no
/// `https://` scheme is accepted here, unlike `CRM_CORS_ALLOWED_ORIGIN`),
/// no trailing slash, no whitespace.
fn is_plausible_centrifugo_url(value: &str) -> bool {
    value.starts_with("http://") && !value.contains(char::is_whitespace) && !value.ends_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Exact-map source with no defaults, for tests that need full control
    /// (e.g. asserting behavior when a required variable is truly absent).
    fn raw_source(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    /// A valid baseline (including a valid session secret and a valid raw
    /// payload key) overlaid with per-test overrides, so existing tests
    /// don't all need updating for every newly-required variable.
    fn source(overrides: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert(
            "CRM_SESSION_SECRET".to_string(),
            "a".repeat(MIN_SESSION_SECRET_BYTES),
        );
        map.insert(
            "CRM_RAW_PAYLOAD_KEY".to_string(),
            "ab".repeat(RAW_PAYLOAD_KEY_HEX_LEN / 2),
        );
        map.insert(
            "CENTRIFUGO_HTTP_API_KEY".to_string(),
            "test-centrifugo-api-key".to_string(),
        );
        map.insert(
            "CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(),
            "c".repeat(MIN_REALTIME_TOKEN_SECRET_BYTES),
        );
        for (k, v) in overrides {
            map.insert((*k).to_string(), (*v).to_string());
        }
        move |key: &str| map.get(key).cloned()
    }

    /// Like `source`, but omits `CRM_RAW_PAYLOAD_KEY` entirely instead of
    /// defaulting it, for testing that key's own required-ness without
    /// tripping the (earlier-checked) session secret requirement.
    fn source_no_raw_payload_key(overrides: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert(
            "CRM_SESSION_SECRET".to_string(),
            "a".repeat(MIN_SESSION_SECRET_BYTES),
        );
        for (k, v) in overrides {
            map.insert((*k).to_string(), (*v).to_string());
        }
        move |key: &str| map.get(key).cloned()
    }

    /// Like `source`, but omits the two `CENTRIFUGO_*` variables entirely,
    /// for testing their own required-ness without tripping the
    /// earlier-checked session secret / raw payload key requirements.
    fn source_no_realtime(overrides: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert(
            "CRM_SESSION_SECRET".to_string(),
            "a".repeat(MIN_SESSION_SECRET_BYTES),
        );
        map.insert(
            "CRM_RAW_PAYLOAD_KEY".to_string(),
            "ab".repeat(RAW_PAYLOAD_KEY_HEX_LEN / 2),
        );
        for (k, v) in overrides {
            map.insert((*k).to_string(), (*v).to_string());
        }
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_unset() {
        let config = Config::from_source(source(&[])).unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.database_url, None);
        assert_eq!(config.database_connect_timeout, Duration::from_millis(2000));
        assert_eq!(config.session_ttl, Duration::from_secs(168 * 3600));
        assert!(!config.session_cookie_secure);
        assert_eq!(config.cors_allowed_origin, None);
        assert_eq!(config.session_cookie_domain, None);
    }

    #[test]
    fn accepts_valid_cors_origin() {
        let config = Config::from_source(source(&[(
            "CRM_CORS_ALLOWED_ORIGIN",
            "https://app.tarams.org",
        )]))
        .unwrap();
        assert_eq!(
            config.cors_allowed_origin.as_deref(),
            Some("https://app.tarams.org")
        );
    }

    #[test]
    fn rejects_cors_origin_with_trailing_slash() {
        let err = Config::from_source(source(&[(
            "CRM_CORS_ALLOWED_ORIGIN",
            "https://app.tarams.org/",
        )]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCorsOrigin(_)));
    }

    #[test]
    fn rejects_cors_origin_without_scheme() {
        let err = Config::from_source(source(&[("CRM_CORS_ALLOWED_ORIGIN", "app.tarams.org")]))
            .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCorsOrigin(_)));
    }

    #[test]
    fn accepts_valid_cookie_domain() {
        let config =
            Config::from_source(source(&[("CRM_SESSION_COOKIE_DOMAIN", ".tarams.org")])).unwrap();
        assert_eq!(config.session_cookie_domain.as_deref(), Some(".tarams.org"));
    }

    #[test]
    fn rejects_cookie_domain_with_whitespace() {
        let err = Config::from_source(source(&[("CRM_SESSION_COOKIE_DOMAIN", "tarams .org")]))
            .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCookieDomain(_)));
    }

    #[test]
    fn rejects_non_loopback_bind_addr() {
        let err =
            Config::from_source(source(&[("CRM_API_BIND_ADDR", "0.0.0.0:3000")])).unwrap_err();
        assert!(matches!(err, ConfigError::NonLoopbackBindAddr(_)));
    }

    #[test]
    fn rejects_invalid_bind_addr() {
        let err = Config::from_source(source(&[("CRM_API_BIND_ADDR", "not-an-addr")])).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBindAddr(_)));
    }

    #[test]
    fn rejects_timeout_below_bounds() {
        let err =
            Config::from_source(source(&[("CRM_DATABASE_CONNECT_TIMEOUT_MS", "0")])).unwrap_err();
        assert_eq!(err, ConfigError::ConnectTimeoutOutOfBounds(0));
    }

    #[test]
    fn rejects_timeout_above_bounds() {
        let err = Config::from_source(source(&[("CRM_DATABASE_CONNECT_TIMEOUT_MS", "30001")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::ConnectTimeoutOutOfBounds(30001));
    }

    #[test]
    fn accepts_custom_values() {
        let config = Config::from_source(source(&[
            ("CRM_API_BIND_ADDR", "127.0.0.1:4000"),
            ("DATABASE_URL", "postgres://localhost/test"),
            ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "500"),
        ]))
        .unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:4000".parse().unwrap());
        assert_eq!(
            config.database_url.as_deref(),
            Some("postgres://localhost/test")
        );
        assert_eq!(config.database_connect_timeout, Duration::from_millis(500));
    }

    #[test]
    fn rejects_missing_session_secret() {
        let err = Config::from_source(raw_source(&[])).unwrap_err();
        assert_eq!(err, ConfigError::MissingSessionSecret);
    }

    #[test]
    fn rejects_short_session_secret() {
        let err = Config::from_source(source(&[("CRM_SESSION_SECRET", "too-short")])).unwrap_err();
        assert_eq!(err, ConfigError::SessionSecretTooShort(9));
    }

    #[test]
    fn session_secret_is_redacted_in_debug() {
        let config = Config::from_source(source(&[(
            "CRM_SESSION_SECRET",
            "super-secret-value-that-must-never-print",
        )]))
        .unwrap();
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains("super-secret-value-that-must-never-print"));
        assert!(debug_output.contains("REDACTED"));
    }

    #[test]
    fn rejects_session_ttl_below_bounds() {
        let err = Config::from_source(source(&[("CRM_SESSION_TTL_HOURS", "0")])).unwrap_err();
        assert_eq!(err, ConfigError::SessionTtlOutOfBounds(0));
    }

    #[test]
    fn rejects_session_ttl_above_bounds() {
        let err = Config::from_source(source(&[("CRM_SESSION_TTL_HOURS", "721")])).unwrap_err();
        assert_eq!(err, ConfigError::SessionTtlOutOfBounds(721));
    }

    #[test]
    fn rejects_invalid_cookie_secure() {
        let err = Config::from_source(source(&[("CRM_SESSION_COOKIE_SECURE", "yes")])).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCookieSecure(_)));
    }

    #[test]
    fn accepts_cookie_secure_true() {
        let config = Config::from_source(source(&[("CRM_SESSION_COOKIE_SECURE", "true")])).unwrap();
        assert!(config.session_cookie_secure);
    }

    // --- CRM_RAW_PAYLOAD_KEY (docs/specs/SLICE_002.md §7) ------------------

    #[test]
    fn rejects_missing_raw_payload_key() {
        let err = Config::from_source(source_no_raw_payload_key(&[])).unwrap_err();
        assert_eq!(err, ConfigError::MissingRawPayloadKey);
    }

    #[test]
    fn rejects_raw_payload_key_63_chars() {
        let err =
            Config::from_source(source(&[("CRM_RAW_PAYLOAD_KEY", &"a".repeat(63))])).unwrap_err();
        assert_eq!(err, ConfigError::InvalidRawPayloadKeyLength(63));
    }

    #[test]
    fn rejects_raw_payload_key_65_chars() {
        let err =
            Config::from_source(source(&[("CRM_RAW_PAYLOAD_KEY", &"a".repeat(65))])).unwrap_err();
        assert_eq!(err, ConfigError::InvalidRawPayloadKeyLength(65));
    }

    #[test]
    fn rejects_non_hex_raw_payload_key() {
        // 64 characters, but 'z' is not a hex digit.
        let non_hex = format!("{}z", "a".repeat(63));
        let err = Config::from_source(source(&[("CRM_RAW_PAYLOAD_KEY", &non_hex)])).unwrap_err();
        assert_eq!(err, ConfigError::InvalidRawPayloadKeyEncoding);
    }

    #[test]
    fn accepts_valid_raw_payload_key_and_decodes_it() {
        let config = Config::from_source(source(&[(
            "CRM_RAW_PAYLOAD_KEY",
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )]))
        .unwrap();
        assert_eq!(
            config.raw_payload_key.as_bytes(),
            &[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
                0x1c, 0x1d, 0x1e, 0x1f,
            ]
        );
    }

    #[test]
    fn raw_payload_key_is_redacted_in_debug() {
        let config =
            Config::from_source(source(&[("CRM_RAW_PAYLOAD_KEY", &"be".repeat(32))])).unwrap();
        let debug_output = format!("{config:?}");
        // A distinctive repeated run, not just "be", so this cannot pass by
        // coincidentally matching an unrelated field's Debug output.
        assert!(!debug_output.contains(&"be".repeat(8)));
        assert!(debug_output.contains("RawPayloadKey(REDACTED)"));
    }

    // --- CENTRIFUGO_HTTP_API_KEY / CENTRIFUGO_TOKEN_HMAC_SECRET / --------
    // --- CRM_CENTRIFUGO_API_URL / CRM_REALTIME_TOKEN_TTL_SECONDS ---------
    // (docs/specs/SLICE_003.md §11) -----------------------------------------

    #[test]
    fn rejects_missing_centrifugo_api_key() {
        let err = Config::from_source(source_no_realtime(&[(
            "CENTRIFUGO_TOKEN_HMAC_SECRET",
            &"c".repeat(32),
        )]))
        .unwrap_err();
        assert_eq!(err, ConfigError::MissingCentrifugoApiKey);
    }

    #[test]
    fn rejects_empty_centrifugo_api_key() {
        let err = Config::from_source(source_no_realtime(&[
            ("CENTRIFUGO_HTTP_API_KEY", ""),
            ("CENTRIFUGO_TOKEN_HMAC_SECRET", &"c".repeat(32)),
        ]))
        .unwrap_err();
        assert_eq!(err, ConfigError::MissingCentrifugoApiKey);
    }

    #[test]
    fn rejects_missing_realtime_token_secret() {
        let err = Config::from_source(source_no_realtime(&[(
            "CENTRIFUGO_HTTP_API_KEY",
            "test-key",
        )]))
        .unwrap_err();
        assert_eq!(err, ConfigError::MissingRealtimeTokenSecret);
    }

    #[test]
    fn rejects_short_realtime_token_secret() {
        let err = Config::from_source(source(&[("CENTRIFUGO_TOKEN_HMAC_SECRET", "too-short")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::RealtimeTokenSecretTooShort(9));
    }

    #[test]
    fn empty_realtime_token_secret_is_too_short_zero_not_missing() {
        // Pins the error contract behind the (unreachable) SecretError::
        // Empty arm in from_source: "" is TooShort{len:0}, exactly what
        // main produced before the 006a constructor cutover.
        let err = Config::from_source(source(&[("CENTRIFUGO_TOKEN_HMAC_SECRET", "")])).unwrap_err();
        assert_eq!(err, ConfigError::RealtimeTokenSecretTooShort(0));
    }

    #[test]
    fn accepts_realtime_token_secret_exactly_32_bytes() {
        let config =
            Config::from_source(source(&[("CENTRIFUGO_TOKEN_HMAC_SECRET", &"d".repeat(32))]))
                .unwrap();
        assert_eq!(config.realtime_token_secret.as_bytes().len(), 32);
    }

    #[test]
    fn realtime_token_secret_is_redacted_in_debug() {
        let config = Config::from_source(source(&[(
            "CENTRIFUGO_TOKEN_HMAC_SECRET",
            &"df".repeat(16),
        )]))
        .unwrap();
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains(&"df".repeat(8)));
        assert!(debug_output.contains("RealtimeTokenSecret(REDACTED)"));
    }

    #[test]
    fn centrifugo_api_key_is_redacted_in_debug() {
        let config = Config::from_source(source(&[(
            "CENTRIFUGO_HTTP_API_KEY",
            "super-secret-publish-key-value",
        )]))
        .unwrap();
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains("super-secret-publish-key-value"));
        assert!(debug_output.contains("CentrifugoApiKey(REDACTED)"));
    }

    #[test]
    fn defaults_centrifugo_api_url_and_ttl() {
        let config = Config::from_source(source(&[])).unwrap();
        assert_eq!(config.centrifugo_api_url, "http://127.0.0.1:8000/api");
        assert_eq!(config.realtime_token_ttl, Duration::from_secs(600));
    }

    #[test]
    fn accepts_custom_centrifugo_api_url() {
        let config = Config::from_source(source(&[(
            "CRM_CENTRIFUGO_API_URL",
            "http://127.0.0.1:9000/api",
        )]))
        .unwrap();
        assert_eq!(config.centrifugo_api_url, "http://127.0.0.1:9000/api");
    }

    #[test]
    fn rejects_https_centrifugo_api_url() {
        let err = Config::from_source(source(&[(
            "CRM_CENTRIFUGO_API_URL",
            "https://127.0.0.1:8000/api",
        )]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCentrifugoApiUrl(_)));
    }

    #[test]
    fn rejects_centrifugo_api_url_with_trailing_slash() {
        let err = Config::from_source(source(&[(
            "CRM_CENTRIFUGO_API_URL",
            "http://127.0.0.1:8000/api/",
        )]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCentrifugoApiUrl(_)));
    }

    #[test]
    fn rejects_realtime_ttl_below_bounds() {
        let err =
            Config::from_source(source(&[("CRM_REALTIME_TOKEN_TTL_SECONDS", "59")])).unwrap_err();
        assert_eq!(err, ConfigError::RealtimeTokenTtlOutOfBounds(59));
    }

    #[test]
    fn rejects_realtime_ttl_above_bounds() {
        let err =
            Config::from_source(source(&[("CRM_REALTIME_TOKEN_TTL_SECONDS", "3601")])).unwrap_err();
        assert_eq!(err, ConfigError::RealtimeTokenTtlOutOfBounds(3601));
    }

    #[test]
    fn accepts_custom_realtime_ttl() {
        let config =
            Config::from_source(source(&[("CRM_REALTIME_TOKEN_TTL_SECONDS", "120")])).unwrap();
        assert_eq!(config.realtime_token_ttl, Duration::from_secs(120));
    }

    #[test]
    fn rejects_invalid_realtime_ttl() {
        let err = Config::from_source(source(&[(
            "CRM_REALTIME_TOKEN_TTL_SECONDS",
            "not-a-number",
        )]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidRealtimeTokenTtl(_)));
    }

    // --- CRM_INVITATION_TTL_HOURS (docs/specs/SLICE_004.md §11) -----------

    #[test]
    fn defaults_invitation_ttl() {
        let config = Config::from_source(source(&[])).unwrap();
        assert_eq!(config.invitation_ttl, Duration::from_secs(168 * 3600));
    }

    #[test]
    fn rejects_invitation_ttl_below_bounds() {
        let err = Config::from_source(source(&[("CRM_INVITATION_TTL_HOURS", "0")])).unwrap_err();
        assert_eq!(err, ConfigError::InvitationTtlOutOfBounds(0));
    }

    #[test]
    fn rejects_invitation_ttl_above_bounds() {
        let err = Config::from_source(source(&[("CRM_INVITATION_TTL_HOURS", "721")])).unwrap_err();
        assert_eq!(err, ConfigError::InvitationTtlOutOfBounds(721));
    }

    #[test]
    fn accepts_custom_invitation_ttl() {
        let config = Config::from_source(source(&[("CRM_INVITATION_TTL_HOURS", "24")])).unwrap();
        assert_eq!(config.invitation_ttl, Duration::from_secs(24 * 3600));
    }

    // --- CRM_OPERATOR_* / GROQ_API_KEY (docs/specs/SLICE_005.md §11) -------

    #[test]
    fn operator_defaults_and_disabled_without_key() {
        let config = Config::from_source(source(&[])).unwrap();
        assert!(config.groq_api_key.is_none());
        assert_eq!(config.operator.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(config.operator.model, "openai/gpt-oss-120b");
        assert_eq!(config.operator.turn_timeout, Duration::from_millis(20_000));
        assert_eq!(config.operator.call_timeout, Duration::from_millis(10_000));
        assert_eq!(config.operator.max_concurrent, 4);
        assert_eq!(config.operator.proposal_ttl, Duration::from_secs(120));
    }

    #[test]
    fn empty_groq_key_is_disabled_not_an_error() {
        let config = Config::from_source(source(&[("GROQ_API_KEY", "   ")])).unwrap();
        assert!(config.groq_api_key.is_none());
    }

    #[test]
    fn groq_key_is_redacted_in_debug() {
        let config =
            Config::from_source(source(&[("GROQ_API_KEY", "gsk_live_super_secret_key")])).unwrap();
        assert!(config.groq_api_key.is_some());
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains("gsk_live_super_secret_key"));
        assert!(debug_output.contains("GroqApiKey(REDACTED)"));
    }

    #[test]
    fn operator_bounds_are_validated_even_without_a_key() {
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_TURN_TIMEOUT_MS", "1999")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorTurnTimeoutOutOfBounds(1999));
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_TURN_TIMEOUT_MS", "60001")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorTurnTimeoutOutOfBounds(60001));
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_CALL_TIMEOUT_MS", "999")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorCallTimeoutOutOfBounds(999));
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_CALL_TIMEOUT_MS", "30001")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorCallTimeoutOutOfBounds(30001));
        let err = Config::from_source(source(&[("CRM_OPERATOR_MAX_CONCURRENT", "0")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorMaxConcurrentOutOfBounds(0));
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_MAX_CONCURRENT", "65")])).unwrap_err();
        assert_eq!(err, ConfigError::OperatorMaxConcurrentOutOfBounds(65));
        let err = Config::from_source(source(&[("CRM_OPERATOR_PROPOSAL_TTL_SECONDS", "29")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::OperatorProposalTtlOutOfBounds(29));
        let err = Config::from_source(source(&[("CRM_OPERATOR_PROPOSAL_TTL_SECONDS", "601")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::OperatorProposalTtlOutOfBounds(601));
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_PROPOSAL_TTL_SECONDS", "x")])).unwrap_err();
        assert_eq!(
            err,
            ConfigError::InvalidOperatorProposalTtl("x".to_string())
        );
        let err =
            Config::from_source(source(&[("CRM_OPERATOR_MAX_CONCURRENT", "four")])).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidOperatorMaxConcurrent(_)));
    }

    #[test]
    fn operator_call_timeout_must_not_exceed_turn_timeout() {
        let err = Config::from_source(source(&[
            ("CRM_OPERATOR_TURN_TIMEOUT_MS", "5000"),
            ("CRM_OPERATOR_CALL_TIMEOUT_MS", "6000"),
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            ConfigError::OperatorCallTimeoutExceedsTurnTimeout {
                call_ms: 6000,
                turn_ms: 5000
            }
        );
        let config = Config::from_source(source(&[
            ("CRM_OPERATOR_TURN_TIMEOUT_MS", "5000"),
            ("CRM_OPERATOR_CALL_TIMEOUT_MS", "5000"),
        ]))
        .unwrap();
        assert_eq!(config.operator.call_timeout, config.operator.turn_timeout);
    }

    #[test]
    fn operator_base_url_requires_https_except_loopback() {
        for ok in [
            "https://api.groq.com/openai/v1",
            "http://127.0.0.1:11434/v1",
            "http://localhost:8080/v1",
            "http://[::1]:8080/v1",
        ] {
            let config = Config::from_source(source(&[("CRM_OPERATOR_BASE_URL", ok)])).unwrap();
            assert_eq!(config.operator.base_url, ok);
        }
        for bad in [
            "http://api.groq.com/openai/v1",
            "http://10.0.0.5/v1",
            "https://api.groq.com/openai/v1/",
            "api.groq.com/openai/v1",
            "https://api.groq.com/open ai/v1",
            "https://",
        ] {
            let err = Config::from_source(source(&[("CRM_OPERATOR_BASE_URL", bad)])).unwrap_err();
            assert!(
                matches!(err, ConfigError::InvalidOperatorBaseUrl(_)),
                "{bad}: {err:?}"
            );
        }
    }

    #[test]
    fn operator_model_override_and_empty_falls_back() {
        let config =
            Config::from_source(source(&[("CRM_OPERATOR_MODEL", " openai/gpt-oss-20b ")])).unwrap();
        assert_eq!(config.operator.model, "openai/gpt-oss-20b");
        let config = Config::from_source(source(&[("CRM_OPERATOR_MODEL", "")])).unwrap();
        assert_eq!(config.operator.model, "openai/gpt-oss-120b");
    }

    #[test]
    fn rejects_invalid_invitation_ttl() {
        let err = Config::from_source(source(&[("CRM_INVITATION_TTL_HOURS", "not-a-number")]))
            .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidInvitationTtl(_)));
    }
    // --- LIVEKIT_* / CRM_TELEPHONY_* (docs/specs/SLICE_006.md §11) ---------

    const LIVEKIT_OK: &[(&str, &str)] = &[
        ("LIVEKIT_URL", "wss://livekit.tarams.org"),
        ("LIVEKIT_API_URL", "https://livekit.tarams.org"),
        ("LIVEKIT_API_KEY", "APIkey123"),
        ("LIVEKIT_API_SECRET", "livekit-secret-value-never-printed"),
        ("LIVEKIT_SIP_OUTBOUND_TRUNK_ID", "ST_abc"),
    ];

    fn with_livekit(overrides: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut all: Vec<(&str, &str)> = LIVEKIT_OK.to_vec();
        all.extend_from_slice(overrides);
        source(&all)
    }

    #[test]
    fn telephony_defaults_and_disabled_without_key() {
        let config = Config::from_source(source(&[])).unwrap();
        assert!(config.telephony.livekit.is_none());
        assert_eq!(config.telephony.ring_timeout, Duration::from_secs(45));
        assert_eq!(config.telephony.max_call, Duration::from_secs(3600));
        assert_eq!(config.telephony.join_ttl, Duration::from_secs(300));
    }

    #[test]
    fn empty_livekit_key_is_disabled_not_an_error() {
        let config = Config::from_source(source(&[("LIVEKIT_API_KEY", "  ")])).unwrap();
        assert!(config.telephony.livekit.is_none());
    }

    #[test]
    fn livekit_enabled_with_full_config() {
        let config = Config::from_source(with_livekit(&[])).unwrap();
        let lk = config.telephony.livekit.as_ref().unwrap();
        assert_eq!(lk.url, "wss://livekit.tarams.org");
        assert_eq!(lk.api_url, "https://livekit.tarams.org");
        assert_eq!(lk.api_key, "APIkey123");
        assert_eq!(
            lk.api_secret.as_bytes(),
            b"livekit-secret-value-never-printed"
        );
        assert_eq!(lk.sip_outbound_trunk_id, "ST_abc");
    }

    #[test]
    fn livekit_secret_is_trimmed_like_the_key() {
        let config = Config::from_source(with_livekit(&[(
            "LIVEKIT_API_SECRET",
            "  livekit-secret-value-never-printed \n",
        )]))
        .unwrap();
        let lk = config.telephony.livekit.as_ref().unwrap();
        assert_eq!(
            lk.api_secret.as_bytes(),
            b"livekit-secret-value-never-printed"
        );
        let err = Config::from_source(with_livekit(&[("LIVEKIT_API_SECRET", "   ")])).unwrap_err();
        assert!(matches!(err, ConfigError::MissingLiveKitApiSecret));
    }

    #[test]
    fn livekit_secret_is_redacted_in_debug() {
        let config = Config::from_source(with_livekit(&[])).unwrap();
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains("livekit-secret-value-never-printed"));
        assert!(debug_output.contains("LiveKitApiSecret(REDACTED)"));
    }

    #[test]
    fn livekit_key_requires_every_other_livekit_value() {
        let err = Config::from_source(with_livekit(&[("LIVEKIT_URL", "")])).unwrap_err();
        assert_eq!(err, ConfigError::MissingLiveKitUrl);
        let err = Config::from_source(with_livekit(&[("LIVEKIT_API_URL", "")])).unwrap_err();
        assert_eq!(err, ConfigError::MissingLiveKitApiUrl);
        let err = Config::from_source(with_livekit(&[("LIVEKIT_API_SECRET", "")])).unwrap_err();
        assert_eq!(err, ConfigError::MissingLiveKitApiSecret);
        let err = Config::from_source(with_livekit(&[("LIVEKIT_SIP_OUTBOUND_TRUNK_ID", "")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::MissingLiveKitSipOutboundTrunkId);
    }

    #[test]
    fn livekit_urls_are_validated_even_without_a_key() {
        for bad in [
            "https://livekit.tarams.org",
            "wss://",
            "wss://x/",
            "livekit",
        ] {
            let err = Config::from_source(source(&[("LIVEKIT_URL", bad)])).unwrap_err();
            assert!(matches!(err, ConfigError::InvalidLiveKitUrl(_)), "{bad}");
        }
        for bad in ["http://livekit.tarams.org", "wss://x", "https://x/"] {
            let err = Config::from_source(source(&[("LIVEKIT_API_URL", bad)])).unwrap_err();
            assert!(matches!(err, ConfigError::InvalidLiveKitApiUrl(_)), "{bad}");
        }
        for ok in ["https://livekit.tarams.org", "http://127.0.0.1:7880"] {
            Config::from_source(source(&[("LIVEKIT_API_URL", ok)])).unwrap();
        }
        for ok in ["wss://livekit.tarams.org", "ws://127.0.0.1:7880"] {
            Config::from_source(source(&[("LIVEKIT_URL", ok)])).unwrap();
        }
    }

    #[test]
    fn telephony_bounds_are_validated_even_without_a_key() {
        let err = Config::from_source(source(&[("CRM_TELEPHONY_RING_TIMEOUT_SECONDS", "9")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::TelephonyRingTimeoutOutOfBounds(9));
        let err = Config::from_source(source(&[("CRM_TELEPHONY_RING_TIMEOUT_SECONDS", "121")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::TelephonyRingTimeoutOutOfBounds(121));
        let err =
            Config::from_source(source(&[("CRM_TELEPHONY_MAX_CALL_SECONDS", "59")])).unwrap_err();
        assert_eq!(err, ConfigError::TelephonyMaxCallOutOfBounds(59));
        let err = Config::from_source(source(&[("CRM_TELEPHONY_MAX_CALL_SECONDS", "14401")]))
            .unwrap_err();
        assert_eq!(err, ConfigError::TelephonyMaxCallOutOfBounds(14401));
        let err =
            Config::from_source(source(&[("CRM_TELEPHONY_JOIN_TTL_SECONDS", "59")])).unwrap_err();
        assert_eq!(err, ConfigError::TelephonyJoinTtlOutOfBounds(59));
        let err =
            Config::from_source(source(&[("CRM_TELEPHONY_JOIN_TTL_SECONDS", "901")])).unwrap_err();
        assert_eq!(err, ConfigError::TelephonyJoinTtlOutOfBounds(901));
        let err =
            Config::from_source(source(&[("CRM_TELEPHONY_JOIN_TTL_SECONDS", "soon")])).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidTelephonyJoinTtl(_)));
        let err = Config::from_source(source(&[("CRM_TELEPHONY_RING_TIMEOUT_SECONDS", "x")]))
            .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidTelephonyRingTimeout(_)));
        let err =
            Config::from_source(source(&[("CRM_TELEPHONY_MAX_CALL_SECONDS", "x")])).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidTelephonyMaxCall(_)));
    }

    #[test]
    fn accepts_custom_telephony_limits() {
        let config = Config::from_source(source(&[
            ("CRM_TELEPHONY_RING_TIMEOUT_SECONDS", "30"),
            ("CRM_TELEPHONY_MAX_CALL_SECONDS", "600"),
            ("CRM_TELEPHONY_JOIN_TTL_SECONDS", "120"),
        ]))
        .unwrap();
        assert_eq!(config.telephony.ring_timeout, Duration::from_secs(30));
        assert_eq!(config.telephony.max_call, Duration::from_secs(600));
        assert_eq!(config.telephony.join_ttl, Duration::from_secs(120));
    }

    // --- Slice 007a ---------------------------------------------------

    #[test]
    fn intake_mail_defaults_to_elysianfeld_subdomain_scheme() {
        let config = Config::from_source(source(&[])).unwrap();
        assert_eq!(config.intake_mail.domain, "elysianfeld.com");
        assert_eq!(config.intake_mail.scheme, IntakeAddressScheme::Subdomain);
    }

    #[test]
    fn intake_mail_accepts_a_bare_hostname_and_the_local_part_scheme() {
        let config = Config::from_source(source(&[
            ("CRM_INTAKE_MAIL_DOMAIN", "leads.example.co.uk"),
            ("CRM_INTAKE_ADDRESS_SCHEME", "local_part"),
        ]))
        .unwrap();
        assert_eq!(config.intake_mail.domain, "leads.example.co.uk");
        assert_eq!(config.intake_mail.scheme, IntakeAddressScheme::LocalPart);
    }

    #[test]
    fn intake_mail_rejects_non_bare_hostnames_and_unknown_schemes() {
        for bad in [
            "https://elysianfeld.com",
            "elysianfeld.com.",
            "elysianfeld.com:25",
            "elysianfeld",
            "-bad.com",
            "Elysianfeld.com",
            "a b.com",
        ] {
            let err = Config::from_source(source(&[("CRM_INTAKE_MAIL_DOMAIN", bad)])).unwrap_err();
            assert_eq!(
                err,
                ConfigError::InvalidIntakeMailDomain(bad.to_string()),
                "{bad}"
            );
        }
        let err =
            Config::from_source(source(&[("CRM_INTAKE_ADDRESS_SCHEME", "wildcard")])).unwrap_err();
        assert_eq!(
            err,
            ConfigError::InvalidIntakeAddressScheme("wildcard".to_string())
        );
    }

    // --- CRM_INBOUND_EMAIL_SECRET (docs/specs/SLICE_007b.md §6) -----------

    #[test]
    fn inbound_email_secret_unset_is_none() {
        let config = Config::from_source(source(&[])).unwrap();
        assert!(config.inbound_email_secret.is_none());
    }

    #[test]
    fn inbound_email_secret_empty_is_none() {
        let config = Config::from_source(source(&[("CRM_INBOUND_EMAIL_SECRET", "")])).unwrap();
        assert!(config.inbound_email_secret.is_none());
    }

    #[test]
    fn inbound_email_secret_short_is_an_error() {
        let err = Config::from_source(source(&[("CRM_INBOUND_EMAIL_SECRET", &"a".repeat(31))]))
            .unwrap_err();
        assert_eq!(err, ConfigError::InboundEmailSecretTooShort(31));
    }

    #[test]
    fn inbound_email_secret_accepts_exactly_32_bytes() {
        let config =
            Config::from_source(source(&[("CRM_INBOUND_EMAIL_SECRET", &"a".repeat(32))])).unwrap();
        assert_eq!(config.inbound_email_secret.unwrap().as_bytes().len(), 32);
    }

    #[test]
    fn inbound_email_secret_is_redacted_in_debug() {
        let config =
            Config::from_source(source(&[("CRM_INBOUND_EMAIL_SECRET", &"ee".repeat(20))])).unwrap();
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains(&"ee".repeat(8)));
        assert!(debug_output.contains("InboundEmailSecret(REDACTED)"));
    }
}

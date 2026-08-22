use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 2000;
const MIN_CONNECT_TIMEOUT_MS: u64 = 1;
const MAX_CONNECT_TIMEOUT_MS: u64 = 30_000;

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

const RAW_PAYLOAD_KEY_HEX_LEN: usize = 64;

/// The raw-payload encryption key (docs/specs/SLICE_002.md §7): exactly 64
/// hex characters (32 bytes), decoded once at startup. `Debug` is redacted
/// like `SessionSecret` so an accidental `{:?}` never leaks it. The API
/// refuses to start if it is missing, the wrong length, or not hex.
#[derive(Clone)]
pub struct RawPayloadKey([u8; 32]);

impl RawPayloadKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for RawPayloadKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPayloadKey(REDACTED)")
    }
}

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

        let raw_payload_key_raw =
            get("CRM_RAW_PAYLOAD_KEY").ok_or(ConfigError::MissingRawPayloadKey)?;
        if raw_payload_key_raw.len() != RAW_PAYLOAD_KEY_HEX_LEN {
            return Err(ConfigError::InvalidRawPayloadKeyLength(
                raw_payload_key_raw.len(),
            ));
        }
        let raw_payload_key = RawPayloadKey(
            decode_hex_32(&raw_payload_key_raw).ok_or(ConfigError::InvalidRawPayloadKeyEncoding)?,
        );

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
        })
    }
}

fn is_plausible_origin(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://"))
        && !value.contains(char::is_whitespace)
        && !value.ends_with('/')
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
}

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 2000;
const MIN_CONNECT_TIMEOUT_MS: u64 = 1;
const MAX_CONNECT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: Option<String>,
    pub database_connect_timeout: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    InvalidBindAddr(String),
    NonLoopbackBindAddr(SocketAddr),
    InvalidConnectTimeout(String),
    ConnectTimeoutOutOfBounds(u64),
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

        Ok(Config {
            bind_addr,
            database_url,
            database_connect_timeout: Duration::from_millis(timeout_ms),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn source(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_unset() {
        let config = Config::from_source(source(&[])).unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.database_url, None);
        assert_eq!(config.database_connect_timeout, Duration::from_millis(2000));
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
}

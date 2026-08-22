use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::{Config, RawPayloadKey, SessionSecret};

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub database_connect_timeout: Duration,
    pub session_secret: SessionSecret,
    pub session_ttl: Duration,
    pub session_cookie_secure: bool,
    pub session_cookie_domain: Option<String>,
    pub cors_allowed_origin: Option<String>,
    pub raw_payload_key: RawPayloadKey,
}

impl AppState {
    /// `connect_lazy` does not open a connection; it only validates the URL
    /// shape. The first real connection attempt happens on first use (the
    /// readiness check), bounded by `database_connect_timeout`.
    pub fn new(config: &Config) -> Result<Self, sqlx::Error> {
        let db = match config.database_url.as_deref() {
            Some(url) => Some(
                PgPoolOptions::new()
                    .acquire_timeout(config.database_connect_timeout)
                    .connect_lazy(url)?,
            ),
            None => None,
        };

        Ok(Self {
            db,
            database_connect_timeout: config.database_connect_timeout,
            session_secret: config.session_secret.clone(),
            session_ttl: config.session_ttl,
            session_cookie_secure: config.session_cookie_secure,
            session_cookie_domain: config.session_cookie_domain.clone(),
            cors_allowed_origin: config.cors_allowed_origin.clone(),
            raw_payload_key: config.raw_payload_key.clone(),
        })
    }

    /// Test-support constructor: builds `AppState` from an already-connected
    /// pool (e.g. one whose credentials were swapped to `crm_app`) and a
    /// `Config`, so a future new field only needs updating here instead of
    /// at every integration-test struct literal (docs/specs/SLICE_002.md
    /// §14a).
    pub fn for_tests(pool: PgPool, config: &Config) -> Self {
        Self {
            db: Some(pool),
            database_connect_timeout: config.database_connect_timeout,
            session_secret: config.session_secret.clone(),
            session_ttl: config.session_ttl,
            session_cookie_secure: config.session_cookie_secure,
            session_cookie_domain: config.session_cookie_domain.clone(),
            cors_allowed_origin: config.cors_allowed_origin.clone(),
            raw_payload_key: config.raw_payload_key.clone(),
        }
    }
}

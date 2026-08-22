use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::{Config, RawPayloadKey, RealtimeTokenSecret, SessionSecret};
use crate::realtime::{CentrifugoTransport, Publisher};

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
    pub realtime_token_secret: RealtimeTokenSecret,
    pub realtime_token_ttl: Duration,
    pub invitation_ttl: Duration,
    pub publisher: Publisher,
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

        let publisher = Publisher::Centrifugo(CentrifugoTransport::new(
            config.centrifugo_api_url.clone(),
            &config.centrifugo_api_key,
        ));

        Ok(Self {
            db,
            database_connect_timeout: config.database_connect_timeout,
            session_secret: config.session_secret.clone(),
            session_ttl: config.session_ttl,
            session_cookie_secure: config.session_cookie_secure,
            session_cookie_domain: config.session_cookie_domain.clone(),
            cors_allowed_origin: config.cors_allowed_origin.clone(),
            raw_payload_key: config.raw_payload_key.clone(),
            realtime_token_secret: config.realtime_token_secret.clone(),
            realtime_token_ttl: config.realtime_token_ttl,
            invitation_ttl: config.invitation_ttl,
            publisher,
        })
    }

    /// Test-support constructor: builds `AppState` from an already-connected
    /// pool (e.g. one whose credentials were swapped to `crm_app`), a
    /// `Config`, and an explicit `Publisher` (almost always
    /// `Publisher::recording()` — a real `Publisher::Centrifugo` is built
    /// directly by the handful of tests that need one), so a future new
    /// field only needs updating here instead of at every integration-test
    /// struct literal (docs/specs/SLICE_002.md §14a).
    pub fn for_tests(pool: PgPool, config: &Config, publisher: Publisher) -> Self {
        Self {
            db: Some(pool),
            database_connect_timeout: config.database_connect_timeout,
            session_secret: config.session_secret.clone(),
            session_ttl: config.session_ttl,
            session_cookie_secure: config.session_cookie_secure,
            session_cookie_domain: config.session_cookie_domain.clone(),
            cors_allowed_origin: config.cors_allowed_origin.clone(),
            raw_payload_key: config.raw_payload_key.clone(),
            realtime_token_secret: config.realtime_token_secret.clone(),
            realtime_token_ttl: config.realtime_token_ttl,
            invitation_ttl: config.invitation_ttl,
            publisher,
        }
    }
}

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub database_connect_timeout: Duration,
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
        })
    }
}

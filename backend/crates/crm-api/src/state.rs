use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::{
    Config, InboundEmailSecret, IntakeMailConfig, RawPayloadKey, RealtimeTokenSecret, SessionSecret,
};
use crate::domain::migration::reader::{FubReader, HttpFubReader};
use crate::operator::OperatorRuntime;
use crate::realtime::{CentrifugoTransport, Publisher};
use crate::telephony::Telephony;

#[derive(Clone)]
pub struct AppState {
    pub import_release_path: Option<std::path::PathBuf>,
    pub import_release: Option<Arc<crate::auth::workspace::ReleaseReadiness>>,
    pub snapshot_policy: crate::domain::migration::snapshot::SnapshotPolicy,
    pub db: Option<PgPool>,
    pub workspace_read_slots: Arc<tokio::sync::Semaphore>,
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
    /// `None` when no provider is configured: `POST /api/operator/turns`
    /// answers 503 `operator_disabled` and nothing else changes
    /// (docs/specs/SLICE_005.md §9).
    pub operator: Option<Arc<OperatorRuntime>>,
    /// `None` when calling is disabled (`LIVEKIT_API_KEY` unset): the
    /// call routes answer 503 `telephony_disabled`; reads (`GET
    /// /api/calls/{id}`, history) keep working (docs/specs/SLICE_006.md
    /// §3, §9).
    pub telephony: Option<Arc<Telephony>>,
    /// Slice 007a: renders Organization intake addresses.
    pub intake_mail: IntakeMailConfig,
    /// `None` when `CRM_INBOUND_EMAIL_SECRET` is unset: `POST
    /// /inbound/email` answers 401 `unauthenticated` for every request
    /// (docs/specs/SLICE_007b.md §6).
    pub inbound_email_secret: Option<InboundEmailSecret>,
    /// Production-only HTTP reader; tests replace this through the explicit
    /// app-builder seam, never through a request or environment switch.
    pub migration_reader: Arc<dyn FubReader>,
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

        let operator = OperatorRuntime::from_config(config).map(Arc::new);
        match &operator {
            Some(runtime) => tracing::info!(
                provider = runtime.service.provider().name(),
                model = runtime.service.provider().model(),
                "operator enabled"
            ),
            None => tracing::info!("operator disabled: GROQ_API_KEY is not set"),
        }

        let telephony = Telephony::from_config(&config.telephony).map(Arc::new);
        match &telephony {
            Some(telephony) => {
                tracing::info!(provider = telephony.provider_name, "telephony enabled")
            }
            None => tracing::info!("telephony disabled"),
        }

        let migration_reader = Arc::new(
            HttpFubReader::new(
                config.fub_system.name.clone(),
                config.fub_system.key.clone(),
            )
            .map_err(|_| sqlx::Error::Protocol("FUB reader configuration invalid".into()))?,
        );
        Ok(Self {
            import_release_path: None,
            import_release: None,
            snapshot_policy: config.snapshot_policy.clone(),
            workspace_read_slots: Arc::new(tokio::sync::Semaphore::new(
                db.as_ref().map_or(1, |p| {
                    (p.options().get_max_connections() / 2).max(1) as usize
                }),
            )),
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
            operator,
            telephony,
            intake_mail: config.intake_mail.clone(),
            inbound_email_secret: config.inbound_email_secret.clone(),
            migration_reader,
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
            import_release_path: None,
            import_release: None,
            snapshot_policy: config.snapshot_policy.clone(),
            workspace_read_slots: Arc::new(tokio::sync::Semaphore::new(
                (pool.options().get_max_connections() / 2).max(1) as usize,
            )),
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
            intake_mail: config.intake_mail.clone(),
            inbound_email_secret: config.inbound_email_secret.clone(),
            migration_reader: Arc::new(
                HttpFubReader::new(None, None).expect("static FUB reader configuration"),
            ),
            publisher,
            operator: None,
            telephony: None,
        }
    }

    /// Test-only injection seam for synthetic FUB fixtures. No route or
    /// runtime configuration can select a fake reader.
    pub fn with_migration_reader(mut self, reader: Arc<dyn FubReader>) -> Self {
        self.migration_reader = reader;
        self
    }

    /// Test-support: attach an Operator runtime (almost always one built
    /// over a `ScriptedProvider`).
    pub fn with_operator(mut self, runtime: OperatorRuntime) -> Self {
        self.operator = Some(Arc::new(runtime));
        self
    }

    /// Test-support: attach a telephony runtime (almost always one built
    /// over a `ScriptedProvider`).
    pub fn with_telephony(mut self, telephony: Arc<Telephony>) -> Self {
        self.telephony = Some(telephony);
        self
    }
}

impl AppState {
    /// Read only server-owned evidence. A replaced operator report becomes usable
    /// without restarting the API, and stale evidence never advertises Confirm.
    pub async fn current_import_release(
        &self,
    ) -> Option<Arc<crate::auth::workspace::ReleaseReadiness>> {
        let pool = self.db.as_ref()?;
        if let Some(path) = &self.import_release_path {
            return crate::auth::workspace::ReleaseReadiness::load_report(pool, path)
                .await
                .ok()
                .map(Arc::new);
        }
        let release = self.import_release.as_ref()?;
        release
            .require_current(&mut *pool.acquire().await.ok()?)
            .await
            .ok()?;
        Some(release.clone())
    }
}

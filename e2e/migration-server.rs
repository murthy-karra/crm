//! Disposable E2E executable: normal router/commands/workers, synthetic FubReader only.
//! Built as an example with existing test-support; never shipped in crm-api.
use async_trait::async_trait;
use crm_api::{
    config::Config,
    domain::migration::{
        history_capture_source, import_worker,
        reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
        snapshot_source, snapshot_worker, worker,
    },
    state::AppState,
};
use serde_json::Value;
use std::sync::Arc;

struct FixtureReader {
    client: reqwest::Client,
}
impl FixtureReader {
    async fn get(&self, key: &str, path: &str) -> Result<Capture, ReaderError> {
        if key != "synthetic-migration-e2e" {
            return Err(ReaderError::InvalidCredential);
        }
        let response = self
            .client
            .get(format!("http://mocks:9000/fub/v1/{path}"))
            .send()
            .await
            .map_err(|_| ReaderError::Unavailable)?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|_| ReaderError::Unavailable)?
            .to_vec();
        if status != 200 {
            return Err(ReaderError::Unavailable);
        }
        Ok(Capture {
            status,
            body,
            truncated: false,
            source_version: Some("synthetic-fub-e2e-v1".into()),
        })
    }
}
#[async_trait]
impl FubReader for FixtureReader {
    async fn identity(&self, key: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        let capture = self.get(key, "identity").await?;
        Ok((
            crm_api::domain::migration::reader::parse_identity(&capture.body)?,
            capture.body,
        ))
    }
    async fn probe(&self, key: &str, probe: Probe) -> Result<ProbeResult, ReaderError> {
        let (path, collection) = match probe {
            Probe::PeopleExcludingTrash => ("people?limit=1&includeTrash=false", "people"),
            Probe::PeopleIncludingTrash => ("people?limit=1&includeTrash=true", "people"),
            Probe::Users => ("users?limit=1", "users"),
            Probe::Stages => ("stages?limit=1", "stages"),
            Probe::CustomFields => ("customFields?limit=1", "customfields"),
        };
        let capture = self.get(key, path).await?;
        let value: Value =
            serde_json::from_slice(&capture.body).map_err(|_| ReaderError::MalformedResponse)?;
        let count = value[collection]
            .as_array()
            .ok_or(ReaderError::MalformedResponse)?
            .len();
        Ok(ProbeResult {
            status: capture.status,
            reported_total: value["_metadata"]["total"].as_u64().map(|v| v.to_string()),
            retrieved_count: i32::try_from(count).map_err(|_| ReaderError::MalformedResponse)?,
            continuation: !value["_metadata"]["next"].is_null(),
            body: capture.body,
            source_version: capture.source_version,
        })
    }
    async fn snapshot(
        &self,
        key: &str,
        request: &snapshot_source::Request,
    ) -> Result<Capture, ReaderError> {
        self.get(key, &request.path()?).await
    }
    async fn history(
        &self,
        key: &str,
        request: &history_capture_source::Request,
    ) -> Result<Capture, ReaderError> {
        self.get(key, &request.path()?).await
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    crm_api::telemetry::init();
    assert_eq!(std::env::var("E2E_FAMILY").as_deref(), Ok("migration"));
    let config = Config::from_env()?;
    let reader = Arc::new(FixtureReader {
        client: reqwest::Client::new(),
    });
    let mut state = AppState::new(&config)?.with_migration_reader(reader.clone());
    // Same explicit synthetic release-readiness fixture as import_qa. This does
    // not certify a deployed mixed-version fleet; business workspace gates remain real.
    state.import_release = Some(Arc::new(
        crm_api::auth::workspace::ReleaseReadiness::for_tests(),
    ));
    let pool = state
        .db
        .as_ref()
        .expect("isolated application database")
        .clone();
    let _assessment = worker::spawn(pool.clone(), config.raw_payload_key.clone(), reader.clone());
    let _snapshot = snapshot_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        reader,
        state.snapshot_policy.clone(),
    );
    let _import = import_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        state.snapshot_policy.clone(),
    );
    let _metadata = crm_api::domain::migration::metadata_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        state.snapshot_policy.clone(),
    );
    let _activity = crm_api::domain::migration::activity_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        state.snapshot_policy.clone(),
    );
    let background = state.clone();
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            tick.tick().await;
            let pool = background.db.as_ref().unwrap();
            let key = &background.raw_payload_key;
            let policy = &background.snapshot_policy;
            let release = background.current_import_release().await;
            let _ = crm_api::domain::migration::history_capture_worker::run_once(
                pool,
                key,
                background.migration_reader.as_ref(),
                policy,
                release.as_deref(),
            )
            .await;
            let _ = crm_api::domain::migration::history_import_worker::run_once(
                pool,
                key,
                policy,
                release.as_deref(),
            )
            .await;
            for _ in 0..32 {
                match crm_api::domain::migration::core_change_worker::run_once(
                    pool,
                    key,
                    policy,
                    release.as_deref(),
                )
                .await
                {
                    Ok(true) => {}
                    _ => break,
                }
            }
            let execute = match client.get("http://mocks:9000/fub/control").send().await {
                Ok(response) => response
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v["executeRefresh"].as_bool())
                    .unwrap_or(false),
                Err(_) => false,
            };
            for _ in 0..32 {
                use crm_api::domain::migration::family_refresh::{execution, preparation_worker};
                let _ = preparation_worker::run_once(pool, key, policy).await;
                if execute {
                    let _ = execution::run_once(pool, key, policy, release.as_deref()).await;
                }
            }
        }
    });
    let app = crm_api::build_app(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

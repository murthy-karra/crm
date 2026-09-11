//! Synthetic-only real-API browser QA for Slice 010c.
//!
//! Build explicitly with `cargo run -p crm-api --example import_qa
//! --features test-support -- /absolute/private/qa.env /absolute/private/control.json`.
//! The private env file must explicitly contain DATABASE_URL and
//! MIGRATION_DATABASE_URL for loopback `crm_slice010c_qa`, using crm_app and
//! crm_migrator respectively, plus the ordinary session/raw-payload/realtime
//! secrets and CRM_DEV_SEED_PASSWORD. There is no ambient-environment fallback.
//! The coordinator creates the disposable database; this program never drops or
//! creates a database. Bootstrap is the existing typed platform-admin command.
//! Then run the existing scripts/seed_dev.py against http://127.0.0.1:3012.
//!
//! Serve the production web/dist build separately with Vite preview on 5182,
//! CRM_WEB_API_PROXY_TARGET=http://127.0.0.1:3012 and VITE_API_BASE_URL=/api.
//! No fixture endpoint, production configuration switch, external integration
//! client, or live FUB request is used by this executable.
//!
//! Private control-file shape (unknown fields are rejected):
//! {"mode":"normal","delay_ms":250,"note_detail_gap":true}
//! Modes: normal, pause_people, unavailable, invalid_identity. Change this local
//! file atomically to pause/resume a source scenario; use the normal API/UI retry
//! action to resume durable work. Changing fixture controls itself starts no work.
//! The always-synthetic connection input accepted by this reader is
//! `synthetic-snapshot`; all other inputs fail credential validation.
//! Complete assessment, snapshot capture and preview through normal migration
//! routes, then use the normal import mapping/confirm/review routes. Source
//! calls remain synthetic; the import worker has no FubReader/provider input.
//! The existing 010b fixture file/records are preserved. This example appends
//! only two 010c cases: 104 is a named contactless Person at captured stage 12
//! (explicit create mapping) and source user 8 (explicit unassigned mapping).
//! Person 105 has sourceUrl provenance above 2 MiB, plus explicit primary
//! contacts that occur after equivalent earlier values in the source arrays.
//! Primary selection must precede deduplication; shared contacts still create
//! separate People. Source fields are text evidence, never URLs to fetch.
//! The five-Person source page stays below the qualified 4 MiB raw-page limit.
//! A private sibling `control.stats.json` records only source_reader_calls.
//! Compare it immediately before import preparation/confirmation and afterward
//! to prove the import did not call the synthetic source reader. The file is
//! replaced atomically; it contains no credentials, identifiers or payloads.
//! Storage ceilings use the two normal CRM_FUB_SNAPSHOT_*_CEILING_BYTES names
//! from the explicit private env file; restart the example to change policy.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use crm_api::{
    config::Config,
    domain::{
        admin::commands::{grant_platform_admin, GrantPlatformAdmin},
        migration::{
            import_worker,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot_source::{Request, Stream},
            snapshot_worker, worker,
        },
    },
    realtime::Publisher,
    state::AppState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool, Row};

type HarnessError = Box<dyn std::error::Error>;
const QA_DATABASE: &str = "crm_slice010c_qa";
const FIXTURE: &str = include_str!("fixtures/snapshot_qa.json");

/// Append deterministic 010c-only cases without mutating any 010b record or
/// its metadata. In particular, the original source IDs and trash flags keep
/// their meanings. Original stage.peopleCount values are preserved source
/// metadata, not recomputed collection totals; list _metadata is authoritative.
fn import_fixture() -> Result<Value, HarnessError> {
    let mut fixture: Value = serde_json::from_str(FIXTURE)?;
    let people = fixture["people"]
        .as_array_mut()
        .ok_or("Synthetic People fixture must be an array")?;
    if people.len() != 3
        || people.iter().map(|p| p["id"].as_u64()).collect::<Vec<_>>()
            != vec![Some(101), Some(102), Some(103)]
    {
        return Err("Original synthetic People fixture scope changed".into());
    }
    people.push(json!({
        "id":104,
        "firstName":"Synthetic",
        "lastName":"Contactless",
        "stage":"Synthetic Unmapped Stage",
        "assignedUserId":8,
        "emails":[],
        "phones":[],
        "qaCase":"010c_contactless_stage_creation_explicit_unassigned"
    }));
    // Exactly 2,687,014 UTF-8 bytes in this string, and 2,818,088 bytes
    // when encoded as its canonical JSON string. Quoted text, a backslash,
    // newline, Unicode and escaped HTML recur through every field segment.
    let source_url = format!(
        "https://synthetic.invalid/never-fetch#{}",
        "Synthetic José 🏡 <p>&lt;script&gt;never execute&lt;/script&gt;</p> \"quoted\" \\\n"
            .repeat(32_768)
    );
    people.push(json!({
        "id":105,
        "firstName":"Synthetic",
        "lastName":"Large Provenance",
        "stage":"Lead",
        "assignedUserId":7,
        "sourceUrl":source_url,
        "emails":[
            {"value":"overlap@people.invalid","type":"home","isPrimary":0},
            {"value":" OVERLAP@people.invalid ","type":"work","isPrimary":1,
             "qaContact":"explicit_primary_before_deduplication"},
            {"value":"secondary@people.invalid","type":"other","isPrimary":0}
        ],
        "phones":[
            {"value":"+1 (202) 555-0100","type":"office","isPrimary":0},
            {"value":"+12025550100","type":"mobile","isPrimary":1,
             "qaContact":"explicit_primary_before_deduplication"},
            {"value":"+12025550105","type":"other","isPrimary":0}
        ],
        "qaCase":"010c_large_segments_primary_before_deduplication"
    }));
    let page = json!({
        "_metadata":{"collection":"people","offset":0,"limit":100,"total":people.len()},
        "people":people
    });
    if serde_json::to_vec(&page)?.len() >= 4 * 1024 * 1024 {
        return Err("Synthetic People page exceeds its qualified raw-page bound".into());
    }
    Ok(fixture)
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Normal,
    PausePeople,
    Unavailable,
    InvalidIdentity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    mode: Mode,
    delay_ms: u64,
    note_detail_gap: bool,
}

struct FixtureReader {
    control_path: PathBuf,
    fixture: Value,
    source_reader_calls: Mutex<u64>,
}

impl FixtureReader {
    fn control(&self) -> Result<Control, ReaderError> {
        let bytes = std::fs::read(&self.control_path).map_err(|_| ReaderError::Unavailable)?;
        if bytes.len() > 1024 {
            return Err(ReaderError::Unavailable);
        }
        let control: Control =
            serde_json::from_slice(&bytes).map_err(|_| ReaderError::Unavailable)?;
        if control.delay_ms > 5_000 {
            return Err(ReaderError::Unavailable);
        }
        Ok(control)
    }

    fn write_stats(&self, calls: u64) -> Result<(), ReaderError> {
        let stats_path = self.control_path.with_extension("stats.json");
        let temporary = self.control_path.with_extension("stats.tmp");
        let bytes = serde_json::to_vec(&json!({"source_reader_calls": calls}))
            .map_err(|_| ReaderError::Unavailable)?;
        std::fs::write(&temporary, bytes).map_err(|_| ReaderError::Unavailable)?;
        std::fs::rename(temporary, stats_path).map_err(|_| ReaderError::Unavailable)
    }

    async fn before_request(&self, key: &str) -> Result<Control, ReaderError> {
        {
            let mut calls = self
                .source_reader_calls
                .lock()
                .map_err(|_| ReaderError::Unavailable)?;
            *calls = calls.checked_add(1).ok_or(ReaderError::Unavailable)?;
            self.write_stats(*calls)?;
        }
        if key != "synthetic-snapshot" {
            return Err(ReaderError::InvalidCredential);
        }
        let control = self.control()?;
        tokio::time::sleep(Duration::from_millis(control.delay_ms)).await;
        Ok(control)
    }

    fn capture(&self, status: u16, value: &Value) -> Result<Capture, ReaderError> {
        Ok(Capture {
            status,
            body: serde_json::to_vec(value).map_err(|_| ReaderError::MalformedResponse)?,
            truncated: false,
            // This is fixture provenance, not a claimed running FUB API version.
            source_version: Some("synthetic-010b-qa".into()),
        })
    }
}

#[async_trait]
impl FubReader for FixtureReader {
    async fn identity(&self, key: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        let control = self.before_request(key).await?;
        if control.mode == Mode::InvalidIdentity {
            return Err(ReaderError::InvalidCredential);
        }
        Ok((
            Identity {
                account_id: 101,
                account_domain: Some("Synthetic Snapshot Realty".into()),
                user_id: Some(7),
                display_name: Some("Synthetic Admin".into()),
            },
            self.capture(200, &self.fixture["identity"])?.body,
        ))
    }

    async fn probe(&self, key: &str, probe: Probe) -> Result<ProbeResult, ReaderError> {
        let control = self.before_request(key).await?;
        if control.mode == Mode::Unavailable {
            return Err(ReaderError::Unavailable);
        }
        let collection = match probe {
            Probe::PeopleExcludingTrash | Probe::PeopleIncludingTrash => "people",
            Probe::Users => "users",
            Probe::Stages => "stages",
            Probe::CustomFields => "customfields",
        };
        let mut records = self.fixture[collection]
            .as_array()
            .ok_or(ReaderError::MalformedResponse)?
            .clone();
        if probe == Probe::PeopleExcludingTrash {
            records.retain(|record| record["isTrash"] != true);
        }
        let total = records.len();
        let returned = records.first().cloned().into_iter().collect::<Vec<_>>();
        let body = self
            .capture(
                200,
                &json!({"_metadata":{"collection":collection,"offset":0,"limit":1,"total":total},collection:returned}),
            )?
            .body;
        Ok(ProbeResult {
            status: 200,
            body,
            reported_total: Some(total.to_string()),
            retrieved_count: i32::from(total > 0),
            continuation: total > 1,
            source_version: Some("synthetic-010b-qa".into()),
        })
    }

    async fn snapshot(&self, key: &str, request: &Request) -> Result<Capture, ReaderError> {
        let control = self.before_request(key).await?;
        if control.mode == Mode::Unavailable {
            return Err(ReaderError::Unavailable);
        }
        if control.mode == Mode::PausePeople && request.stream == Stream::People {
            return Err(ReaderError::AccessDenied.with_capture(
                self.capture(403, &json!({"error":"Synthetic collection denial"}))?,
            ));
        }
        if request.stream == Stream::NoteDetail {
            let id = request
                .source_id
                .as_deref()
                .ok_or(ReaderError::MalformedResponse)?;
            if id == "302" && control.note_detail_gap {
                return self.capture(404, &json!({"error":"Synthetic restricted note detail"}));
            }
            let note = self.fixture["note_details"]
                .get(id)
                .ok_or(ReaderError::MalformedResponse)?;
            return self.capture(200, note);
        }
        let (fixture_key, collection) = match request.stream {
            Stream::Users => ("users", "users"),
            Stream::Stages => ("stages", "stages"),
            Stream::CustomFields => ("customfields", "customfields"),
            Stream::People => ("people", "people"),
            Stream::Notes => ("notes", "notes"),
            Stream::TasksOpen => ("tasks_open", "tasks"),
            Stream::TasksCompleted => ("tasks_completed", "tasks"),
            Stream::NoteDetail => return Err(ReaderError::MalformedResponse),
        };
        // Fixtures model the explicitly qualified offset profile. No source URL
        // or opaque continuation is ever interpreted by this test reader.
        if request.cursor.next.is_some() {
            return Err(ReaderError::MalformedResponse);
        }
        let records = self.fixture[fixture_key]
            .as_array()
            .ok_or(ReaderError::MalformedResponse)?;
        let offset =
            usize::try_from(request.cursor.offset).map_err(|_| ReaderError::MalformedResponse)?;
        let page = records.iter().skip(offset).take(100).collect::<Vec<_>>();
        self.capture(
            200,
            &json!({"_metadata":{"collection":collection,"offset":offset,"limit":100,"total":records.len()},collection:page}),
        )
    }
}

fn database_options(value: &str, role: &str) -> Result<PgConnectOptions, HarnessError> {
    let options = PgConnectOptions::from_str(value)
        .map_err(|_| "Invalid private QA database configuration")?;
    let host = options.get_host();
    let loopback = host == "localhost"
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !loopback || options.get_database() != Some(QA_DATABASE) || options.get_username() != role {
        return Err("Refusing database outside the explicit isolated QA scope".into());
    }
    Ok(options)
}

async fn verify_connected_scope(pool: &PgPool, role: &str) -> Result<(), HarnessError> {
    let row = sqlx::query("SELECT current_database() AS database, current_user AS role")
        .fetch_one(pool)
        .await?;
    if row.try_get::<String, _>("database")? != QA_DATABASE
        || row.try_get::<String, _>("role")? != role
    {
        return Err("Connected database does not match the isolated QA scope".into());
    }
    Ok(())
}

fn private_env(path: &Path) -> Result<HashMap<String, String>, HarnessError> {
    if !path.is_absolute() {
        return Err("An absolute private QA env-file path is required".into());
    }
    dotenvy::from_path_iter(path)
        .map_err(|_| "Could not read private QA env file")?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|_| "Could not parse private QA env file".into())
}

#[tokio::main]
async fn main() -> Result<(), HarnessError> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "Usage: import_qa /absolute/private/qa.env /absolute/private/control.json".into(),
        );
    }
    let env = private_env(Path::new(&args[0]))?;
    let control_path = PathBuf::from(&args[1]);
    if !control_path.is_absolute() {
        return Err("An absolute private QA control-file path is required".into());
    }
    let reader = Arc::new(FixtureReader {
        control_path,
        fixture: import_fixture()?,
        source_reader_calls: Mutex::new(0),
    });
    reader
        .control()
        .map_err(|_| "Invalid private QA control file")?;
    reader
        .write_stats(0)
        .map_err(|_| "Could not initialize private QA reader stats")?;

    // Validate both scopes before connecting or applying any migration. These
    // names never fall back to ambient developer credentials or crm_dev.
    let app_options = database_options(
        env.get("DATABASE_URL")
            .ok_or("QA DATABASE_URL is required")?,
        "crm_app",
    )?;
    let migrator_options = database_options(
        env.get("MIGRATION_DATABASE_URL")
            .ok_or("QA MIGRATION_DATABASE_URL is required")?,
        "crm_migrator",
    )?;
    let config = Config::from_source(|name| match name {
        "CRM_API_BIND_ADDR" => Some("127.0.0.1:3012".into()),
        "CRM_CORS_ALLOWED_ORIGIN" => Some("http://127.0.0.1:5182".into()),
        "CRM_SESSION_COOKIE_SECURE" => Some("false".into()),
        "DATABASE_URL"
        | "CRM_SESSION_SECRET"
        | "CRM_RAW_PAYLOAD_KEY"
        | "CENTRIFUGO_HTTP_API_KEY"
        | "CENTRIFUGO_TOKEN_HMAC_SECRET"
        | "CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES"
        | "CRM_FUB_SNAPSHOT_ORG_CEILING_BYTES" => env.get(name).cloned(),
        _ => None,
    })
    .map_err(|_| "Invalid private QA application configuration")?;
    let seed_password = env
        .get("CRM_DEV_SEED_PASSWORD")
        .filter(|value| !value.is_empty())
        .ok_or("QA CRM_DEV_SEED_PASSWORD is required")?
        .clone();

    // Bind before making writes, so a mistaken occupied port cannot leave a
    // half-started QA setup or accidentally attach to an existing API.
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let migrator = PgPool::connect_with(migrator_options).await?;
    verify_connected_scope(&migrator, "crm_migrator").await?;
    sqlx::migrate::Migrator::new(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations"
    )))
    .await?
    .run(&migrator)
    .await?;
    grant_platform_admin(
        &migrator,
        GrantPlatformAdmin {
            email: "owner@platform.test".into(),
            display_name: "Synthetic QA Platform Owner".into(),
            password: seed_password,
        },
    )
    .await?;
    migrator.close().await;

    let pool = PgPool::connect_with(app_options).await?;
    verify_connected_scope(&pool, "crm_app").await?;
    let mut state = AppState::for_tests(pool.clone(), &config, Publisher::recording())
        .with_migration_reader(reader.clone());
    // This compile-fenced synthetic executable is not a released artifact.
    // Use server-owned test readiness, never a tenant field or production flag.
    state.import_release = Some(Arc::new(
        crm_api::auth::workspace::ReleaseReadiness::for_tests(),
    ));
    let assessment_worker =
        worker::spawn(pool.clone(), config.raw_payload_key.clone(), reader.clone());
    let snapshot_worker = snapshot_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        reader,
        config.snapshot_policy.clone(),
    );
    let import_worker = import_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    println!("Synthetic-only QA API listening on 127.0.0.1:3012; database crm_slice010c_qa");
    let result = axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    assessment_worker.abort();
    snapshot_worker.abort();
    import_worker.abort();
    pool.close().await;
    result?;
    Ok(())
}

//! Synthetic-only real-API browser QA for Slice 010f2.
//!
//! Explicit build: cargo build -p crm-api --example activity_import_qa
//! --features test-support --locked. Pass exactly two absolute private paths:
//! activity_import_qa /absolute/private/qa.env /absolute/private/control.json.
//! Both database URLs must name crm_010f2_qa on loopback55432 and their exact
//! crm_app/crm_migrator roles. The executable binds API3017 before migrations or
//! typed platform bootstrap; it never creates/drops databases or reads ambient
//! credentials/application configuration. The shared telemetry initializer uses
//! only its normal logging settings; launch with RUST_LOG=info,sqlx=warn.
//! Centrifugo must be http://127.0.0.1:18082/api; production Web5187.
//! Create synthetic Organizations and users through ordinary API invitations,
//! using the private seed password. Do not also run seed_dev.py with overlapping
//! fixture emails. The private env needs explicit values for activity_import_qa;
//! no provider configuration or production readiness overrides are accepted.
//!
//! This fixture reader never makes HTTP requests. Connect using the synthetic
//! source input `synthetic-snapshot`; all source strings/URLs are inert evidence.
//! The normal retained snapshot, preview and confirmed People import precede the
//! child. Optional metadata uses its ordinary independent worker and commands.
//! No SQL fixture changes bypass workspace, note/task permits or authority.
//!
//! Control: {"mode":"normal","delay_ms":0,"note_detail_gap":true,"activity_units":0}.
//! Modes normal/pause_people/pause_custom_fields/unavailable/invalid_identity
//! affect the source reader only. Omitted/null activity_units uses production
//! cadence (32 units/2 seconds); an integer grants that many completed units in
//! this process, one per tick. Raising it resumes scheduling, not paused imports:
//! those still require explicit Retry through the API. Controls are checked before
//! every unit. A unit already running finishes normally. A restart resets counters.
//! Private control.stats.json reports source_reader_calls; activity-stats.json
//! reports completed_activity_units (including transitions/pauses, not row count).
//! Compare source counters around activity planning/import and before restarts.
//! Private control.delivery-stats.json records publication/disconnect counts only,
//! preserving evidence that activity work emits no operational notifications.
//!
//! Data includes supported HTML and subject text, source-only replies/reactions,
//! exact plain Unicode, unsafe/hidden/oversize notes, negative-detail404, explicit
//! historical/current/missing roles, both due-date DST boundaries, conflicting
//! date/time fields, undated tasks, unknown kinds, missing completion timestamps,
//! source-only recurrence and updatedBy independent of completion authorship.
//! Fifty full-size notes on one Person exercise multiple native browser pages;
//! separate DB fixtures carry the required501-full-size-note concentration.
//! Sentinels are synthetic customer content and must never appear in API logs.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::IpAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use crm_api::{
    config::{Config, RawPayloadKey},
    domain::{
        admin::commands::{grant_platform_admin, GrantPlatformAdmin},
        migration::{
            activity_worker, import_worker, metadata_worker,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::SnapshotPolicy,
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
const QA_DATABASE: &str = "crm_010f2_qa";
const QA_DATABASE_PORT: u16 = 55432;
const QA_CENTRIFUGO_URL: &str = "http://127.0.0.1:18082/api";
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const FIXTURE: &str = include_str!("fixtures/snapshot_qa.json");
fn fixture_body(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

fn activity_fixture() -> Result<Value, HarnessError> {
    let mut fixture: Value = serde_json::from_str(FIXTURE)?;
    fixture["fixture_notice"] = json!("Entirely synthetic010f2 QA; not a captured FUB account.");
    let mut notes = Vec::new();
    let mut details = serde_json::Map::new();
    let mut add_note =
        |id: u64, person: u64, subject: &str, body: String, html: bool, author: Option<u64>| {
            let detail = json!({"id":id,"personId":person,"subject":subject,"body":body,
            "isHtml":html,"createdById":author,"created":"2026-09-01T12:00:00.123456Z",
            "updated":null,"showContent":true,"type":"Note"});
            // List bodies intentionally differ, proving detail qualification rather
            // than a successful-looking list fallback.
            notes.push(json!({"id":id,"personId":person,"subject":subject,
            "body":"SYNTHETIC_LIST_MUST_NOT_EXECUTE","createdById":author,
            "isHtml":false,"created":"2026-09-01T12:00:00.123456Z"}));
            details.insert(id.to_string(), detail);
        };
    add_note(301,101,"Readable HTML with replies", r#"<p>SYNTHETIC_ACTIVITY_BODY_SENTINEL Hello <strong>José</strong> 🏡.</p><ul><li>First item</li><li>Second item</li></ul><p><a href="https://synthetic.invalid/never-fetch">Source link</a></p>"#.into(),true,Some(7));
    add_note(
        302,
        102,
        "Restricted detail",
        "This exact source should stay unavailable after the synthetic404.".into(),
        false,
        Some(8),
    );
    add_note(
        303,
        101,
        "Plain subject",
        "Exact plain text\nSecond line — José 🏡\n".into(),
        false,
        Some(8),
    );
    add_note(304,101,"Unsupported image", r#"<p>SYNTHETIC_ACTIVITY_REJECTED_SENTINEL<img src="https://synthetic.invalid/never-fetch" onerror="window.syntheticExecuted=true"></p>"#.into(),true,Some(7));
    add_note(
        305,
        101,
        "Hidden source content",
        "SYNTHETIC_HIDDEN_BODY_SENTINEL".into(),
        false,
        Some(7),
    );
    add_note(
        306,
        101,
        "Oversize preserved source",
        "長".repeat(10_001),
        false,
        Some(7),
    );
    add_note(
        307,
        102,
        "Unknown historical source author",
        "Keep the source author evidence and explicitly choose mapping or unmapped.".into(),
        false,
        Some(999),
    );
    add_note(
        308,
        102,
        "Missing historical author",
        "Preserve the absent source actor explicitly.".into(),
        false,
        None,
    );
    for id in 1_000..1_050 {
        add_note(id, 101, "", "🏡".repeat(10_000), false, Some(7));
    }
    details.get_mut("301").ok_or("Missing synthetic detail")?["replies"] = json!([
        {"id":401,"refId":301,"refType":"Note","body":"SYNTHETIC_SOURCE_ONLY_REPLY",
         "createdById":8,"created":"2026-09-02T12:00:00Z"}]);
    details.get_mut("301").ok_or("Missing synthetic detail")?["reactions"] = json!({"👍":[7]});
    details.get_mut("305").ok_or("Missing synthetic detail")?["showContent"] = json!(false);
    fixture["notes"] = json!(notes);
    fixture["note_details"] = Value::Object(details);
    let make_task = |id: u64, person: u64, name: &str, kind: &str| {
        json!({
        "id":id,"personId":person,"name":name,"type":kind,"isCompleted":false,
        "createdById":7,"assignedUserId":7,"created":"2026-03-01T12:00:00Z","updated":null})
    };
    let mut tasks = vec![
        make_task(501, 101, "Spring DST date-only task", "Call"),
        make_task(502, 103, "Parent-held Person task", "Call"),
        make_task(504, 101, "Fall DST date-only task", "Call"),
        make_task(505, 101, "Conflicting date and instant", "Email"),
        make_task(506, 101, "Deliberately undated task", "Follow Up"),
        make_task(507, 101, "Naive instant held", "Call"),
        make_task(508, 102, "Explicit unknown task kind", "Appointment"),
        make_task(509, 101, &"X".repeat(501), "Call"),
        make_task(512, 102, "Unassigned with historical creator", "Text"),
    ];
    tasks[0]["dueDate"] = json!("2026-03-08");
    tasks[2]["dueDate"] = json!("2026-11-01");
    tasks[3]["dueDate"] = json!("2026-09-01");
    tasks[3]["dueDateTime"] = json!("2026-09-01T02:00:00Z");
    tasks[4]["recurrence"] = json!({"days":[1,3]});
    tasks[5]["dueDateTime"] = json!("2026-09-16T09:00:00");
    tasks[8]["assignedUserId"] = Value::Null;
    tasks[8]["createdById"] = json!(8);
    fixture["tasks_open"] = json!(tasks);
    let mut completed = vec![
        make_task(503, 101, "Completed task with exact timestamp", "Email"),
        make_task(510, 101, "Missing completion timestamp held", "Email"),
        make_task(511, 102, "Editor is not completion actor", "Text"),
    ];
    for task in &mut completed {
        task["isCompleted"] = json!(true);
    }
    completed[0]["dueDate"] = json!("2026-09-01");
    completed[0]["dueDateTime"] = json!("2026-09-01T17:00:00Z");
    completed[0]["completed"] = json!("2026-09-01T16:00:00.123456Z");
    completed[0]["updated"] = json!("2026-09-01T16:00:00.123456Z");
    completed[2]["completed"] = json!("2026-09-02T16:00:00Z");
    completed[2]["createdById"] = json!(8);
    completed[2]["updatedById"] = json!(7);
    fixture["tasks_completed"] = json!(completed);
    for key in [
        "people",
        "users",
        "stages",
        "customfields",
        "notes",
        "tasks_open",
        "tasks_completed",
    ] {
        if fixture_body(&fixture[key])?.len() > MAX_CAPTURE_BYTES {
            return Err("Synthetic collection exceeds capture bound".into());
        }
    }
    for detail in fixture["note_details"]
        .as_object()
        .ok_or("Missing notes")?
        .values()
    {
        if fixture_body(detail)?.len() > MAX_CAPTURE_BYTES {
            return Err("Synthetic detail exceeds capture bound".into());
        }
    }
    Ok(fixture)
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Normal,
    PausePeople,
    PauseCustomFields,
    Unavailable,
    InvalidIdentity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    mode: Mode,
    delay_ms: u64,
    note_detail_gap: bool,
    #[serde(default)]
    activity_units: Option<u64>,
}

struct FixtureReader {
    control_path: PathBuf,
    fixture: Value,
    source_reader_calls: Mutex<u64>,
}

impl FixtureReader {
    fn control(&self) -> Result<Control, ReaderError> {
        let mut bytes = Vec::new();
        std::fs::File::open(&self.control_path)
            .and_then(|file| file.take(1025).read_to_end(&mut bytes))
            .map_err(|_| ReaderError::Unavailable)?;
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

    fn write_activity_stats(&self, completed: u64) -> Result<(), ReaderError> {
        let stats_path = self.control_path.with_extension("activity-stats.json");
        let temporary = self.control_path.with_extension("activity-stats.tmp");
        let bytes = serde_json::to_vec(&json!({"completed_activity_units": completed}))
            .map_err(|_| ReaderError::Unavailable)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| ReaderError::Unavailable)?;
        file.write_all(&bytes)
            .map_err(|_| ReaderError::Unavailable)?;
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
        let body = fixture_body(value).map_err(|_| ReaderError::MalformedResponse)?;
        if body.len() > MAX_CAPTURE_BYTES {
            return Err(ReaderError::MalformedResponse);
        }
        Ok(Capture {
            status,
            body,
            truncated: false,
            // This is fixture provenance, not a claimed running FUB API version.
            source_version: Some("synthetic-010f2-qa".into()),
        })
    }
}

// Only this compile-fenced example changes scheduling. Every admitted unit runs
// the real retained-only worker, with its normal authorization and transaction
// rules. No source-reader call or native table mutation is introduced here.
fn spawn_activity_worker(
    pool: PgPool,
    key: RawPayloadKey,
    policy: SnapshotPolicy,
    reader: Arc<FixtureReader>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut completed = 0_u64;
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Ok(control) = reader.control() else {
                continue;
            };
            // If publishing a previous completion failed, recover that exact
            // counter before another unit can run. Never reset a spent grant.
            if reader.write_activity_stats(completed).is_err() {
                continue;
            }
            let batch = if control.activity_units.is_some() {
                1
            } else {
                32
            };
            for index in 0..batch {
                let Ok(current) = reader.control() else {
                    break;
                };
                if current
                    .activity_units
                    .is_some_and(|grant| completed >= grant || index > 0)
                {
                    break;
                }
                let Some(next) = completed.checked_add(1) else {
                    break;
                };
                match activity_worker::run_once(&pool, &key, &policy).await {
                    Ok(true) => {
                        completed = next;
                        if reader.write_activity_stats(completed).is_err() {
                            break;
                        }
                    }
                    Ok(false) => break,
                    Err(_) => {
                        tracing::warn!("Synthetic QA activity unit returned an error");
                        break;
                    }
                }
            }
        }
    })
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
            source_version: Some("synthetic-010f2-qa".into()),
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
        if control.mode == Mode::PauseCustomFields && request.stream == Stream::CustomFields {
            return Err(ReaderError::AccessDenied.with_capture(
                self.capture(403, &json!({"error":"Synthetic customfields denial"}))?,
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
    if !loopback
        || options.get_port() != QA_DATABASE_PORT
        || options.get_database() != Some(QA_DATABASE)
        || options.get_username() != role
    {
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
    let metadata = std::fs::metadata(path).map_err(|_| "Could not inspect private QA env file")?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("QA env file must be private to its owner".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "Could not read private QA env file")?;
    // dotenvy's interpolation prefers ambient env over earlier file values.
    // Reject it rather than mutating global env in this multithreaded process.
    if bytes.contains(&b'$') {
        return Err("Private QA env values must be explicit literals without dollar signs".into());
    }
    dotenvy::from_read_iter(bytes.as_slice())
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|_| "Could not parse private QA env file".into())
}

#[tokio::main]
async fn main() -> Result<(), HarnessError> {
    crm_api::telemetry::init();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "Usage: activity_import_qa /absolute/private/qa.env /absolute/private/control.json"
                .into(),
        );
    }
    let env = private_env(Path::new(&args[0]))?;
    let control_path = PathBuf::from(&args[1]);
    if !control_path.is_absolute() {
        return Err("An absolute private QA control-file path is required".into());
    }
    let reader = Arc::new(FixtureReader {
        control_path,
        fixture: activity_fixture()?,
        source_reader_calls: Mutex::new(0),
    });
    reader
        .control()
        .map_err(|_| "Invalid private QA control file")?;
    reader
        .write_stats(0)
        .map_err(|_| "Could not initialize private QA reader stats")?;
    reader
        .write_activity_stats(0)
        .map_err(|_| "Could not initialize private QA activity stats")?;

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
    if env.get("CRM_CENTRIFUGO_API_URL").map(String::as_str) != Some(QA_CENTRIFUGO_URL) {
        return Err("QA realtime must name the explicit isolated loopback service".into());
    }
    let config = Config::from_source(|name| match name {
        "CRM_API_BIND_ADDR" => Some("127.0.0.1:3017".into()),
        "CRM_CORS_ALLOWED_ORIGIN" => Some("http://127.0.0.1:5187".into()),
        "CRM_SESSION_COOKIE_SECURE" => Some("false".into()),
        "DATABASE_URL"
        | "CRM_SESSION_SECRET"
        | "CRM_RAW_PAYLOAD_KEY"
        | "CENTRIFUGO_HTTP_API_KEY"
        | "CENTRIFUGO_TOKEN_HMAC_SECRET"
        | "CRM_CENTRIFUGO_API_URL"
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
    let publisher = Publisher::recording();
    let Publisher::Recording(publications, disconnects) = publisher.clone() else {
        return Err("Synthetic QA must use the recording publisher".into());
    };
    let delivery_stats = reader.control_path.with_extension("delivery-stats.json");
    let delivery_temporary = reader.control_path.with_extension("delivery-stats.tmp");
    let delivery_monitor = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let counts = json!({
                "publications":publications.lock().await.len(),
                "disconnects":disconnects.lock().await.len()
            });
            let saved = (|| -> Result<(), HarnessError> {
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&delivery_temporary)?;
                file.write_all(&serde_json::to_vec(&counts)?)?;
                std::fs::rename(&delivery_temporary, &delivery_stats)?;
                Ok(())
            })();
            if saved.is_err() {
                tracing::warn!("Synthetic QA delivery count could not be persisted");
            }
        }
    });
    let mut state =
        AppState::for_tests(pool.clone(), &config, publisher).with_migration_reader(reader.clone());
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
        reader.clone(),
        config.snapshot_policy.clone(),
    );
    let import_worker = import_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let metadata_worker = metadata_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let activity_worker = spawn_activity_worker(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
        reader,
    );
    println!("Synthetic-only QA API listening on 127.0.0.1:3017; database crm_010f2_qa");
    let result = axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    assessment_worker.abort();
    snapshot_worker.abort();
    import_worker.abort();
    metadata_worker.abort();
    activity_worker.abort();
    delivery_monitor.abort();
    pool.close().await;
    result?;
    Ok(())
}

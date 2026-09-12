//! Synthetic-only real-API browser QA for Slice 010f1.
//!
//! Build explicitly with `cargo build -p crm-api --example metadata_import_qa
//! --features test-support --locked`, then pass exactly two absolute private paths:
//! `metadata_import_qa /absolute/private/qa.env /absolute/private/control.json`.
//! The coordinator creates `crm_010f1_qa` on loopback port 55431. Both database
//! URLs must name that database and the exact crm_app/crm_migrator roles. This
//! executable never creates or drops databases and never reads ambient env.
//! It binds API port 3016 before applying migrations or the existing typed
//! platform-admin bootstrap. Seed Organizations/members using scripts/seed_dev.py
//! with CRM_DEMO_API_URL=http://127.0.0.1:3016 and the private seed password.
//!
//! The explicit private env file must provide DATABASE_URL, MIGRATION_DATABASE_URL,
//! CRM_SESSION_SECRET, CRM_RAW_PAYLOAD_KEY, CENTRIFUGO_HTTP_API_KEY,
//! CENTRIFUGO_TOKEN_HMAC_SECRET, CRM_CENTRIFUGO_API_URL and CRM_DEV_SEED_PASSWORD.
//! Its Centrifugo API URL must be http://127.0.0.1:18081/api. The ordinary snapshot
//! ceiling variables may be supplied; restart to change policy. All provider,
//! inbound-email and production release-report settings are excluded. The only
//! readiness fixture is the existing compile-time test-support capability.
//! Env values must be explicit literals; dollar signs/variable interpolation are
//! rejected so dotenv parsing cannot consult the ambient process environment.
//!
//! Serve a fresh production Web build on 5186 with VITE_API_BASE_URL=/api,
//! CRM_WEB_API_PROXY_TARGET=http://127.0.0.1:3016 and
//! CRM_WEB_REALTIME_PROXY_TARGET=http://127.0.0.1:18081. The recording publisher
//! does not dispatch operational events. The QA source reader never makes HTTP
//! requests; source strings/URLs are inert evidence, not addresses to fetch.
//!
//! Private control shape: {"mode":"normal","delay_ms":250,"note_detail_gap":true,
//! "metadata_units":0}.
//! Modes: normal, pause_people, pause_custom_fields, unavailable, invalid_identity.
//! Replace the control atomically, then use the normal UI/API Retry action.
//! Omitted/null metadata_units uses the production cadence: at most 32 units per
//! two-second tick. An integer is an absolute grant for this process: zero pauses
//! metadata dispatch; increase it to allow that many completed units in total.
//! Bounded mode dispatches at most one unit per tick. Controls are checked before
//! each unit, and invalid/unreadable controls prevent further metadata dispatch.
//! A unit already in progress finishes normally; lowering a grant never undoes it.
//! The synthetic reader accepts only the fixture connection input documented in
//! before_request. No fixture route, data-write bypass or source-reader switch
//! exists in the production API. Complete retained capture, preview and the 010c
//! parent through normal routes before planning this metadata child.
//!
//! The fixture covers exact field machine names, all four native types, literal
//! None/N/A/null choices, explicit null/missing, trim/alias disclosure, invalid
//! source shapes/precision/recurrence, duplicate choices, an oversized machine key,
//! a 21-tag set, a parent-held Trash Person and separate overlapping People.
//! The "Null And Missing" Person has an exact numeric source ID of 128 nines,
//! emitted as raw digits and checked by the existing lossless source parser.
//! Its Buyer occurrence and 1,024-character tag exercise long alias inspection.
//! Person 105's declared text field contains >2 MiB of escaped/Unicode evidence:
//! it must be held under the native 500-character cap and remain fully inspectable.
//! Other valid metadata is still independently importable. Every source response
//! is checked against the 4-MiB decoded capture ceiling.
//!
//! Differing preexisting Person values are deliberately a DB negative fixture,
//! not a browser claim: ordinary writes cannot seed them after the review hold.
//! This executable never weakens that hold or injects a native conflict via SQL.
//! Existing tag/field/option catalogs can be created through ordinary APIs before
//! the People import to exercise explicit map_existing choices during review.
//! A private sibling control.stats.json contains only source_reader_calls; compare
//! it before and after child preparation/execution, and before every restart
//! (startup resets the counter). It never contains credentials or source content.
//! The separate private control.metadata-stats.json contains only
//! completed_metadata_units. It counts run_once's Ok(true), including phase
//! transitions and committed pauses, not catalog rows or People. Ok(false) and
//! returned errors do not count. This counter also resets on process startup;
//! inspect the ordinary UI/API state to determine what a completed unit did.

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
            import_worker, metadata_worker,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::SnapshotPolicy,
            snapshot_source::{self, Request, Stream},
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
const QA_DATABASE: &str = "crm_010f1_qa";
const QA_DATABASE_PORT: u16 = 55431;
const QA_CENTRIFUGO_URL: &str = "http://127.0.0.1:18081/api";
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const FIXTURE: &str = include_str!("fixtures/snapshot_qa.json");
const LONG_SOURCE_ID_MARKER: &str = "__synthetic_source_id_128__";

// Default serde_json numbers cannot carry this source ID without rounding.
// Keep the private fixture marker in Value, then replace only its exact id token
// with raw decimal digits. These emitted bytes go through the normal lossless
// source parser; no numeric ID is coerced through f64 or a native-sized integer.
fn fixture_body(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    Ok(serde_json::to_string(value)?
        .replace(
            &format!("\"id\":\"{LONG_SOURCE_ID_MARKER}\""),
            &format!("\"id\":{}", "9".repeat(128)),
        )
        .into_bytes())
}

/// Copy the committed synthetic source, preserving its original identity,
/// supporting users/stages and unrelated notes/tasks. Changes here are explicitly
/// 010f1 fixture data, never a purported live FUB observation.
fn metadata_fixture() -> Result<Value, HarnessError> {
    let mut fixture: Value = serde_json::from_str(FIXTURE)?;
    let fields = fixture["customfields"]
        .as_array_mut()
        .ok_or("Synthetic customfields fixture must be an array")?;
    if fields
        .iter()
        .map(|field| field["id"].as_u64())
        .collect::<Vec<_>>()
        != vec![Some(21), Some(22), Some(23)]
    {
        return Err("Original synthetic customfields fixture scope changed".into());
    }
    fields[0]["choices"] = json!(["North", "South", "None", "N/A", "null"]);
    fields.extend([
        json!({"id":24,"name":"customSyntheticBuyerNote","label":"Synthetic Buyer Note","type":"text","orderWeight":4,"hideIfEmpty":true}),
        json!({"id":25,"name":"customSyntheticBudget","label":"Synthetic Budget","type":"number","orderWeight":5}),
        json!({"id":26,"name":"customSyntheticMoveDate","label":"Synthetic Move Date","type":"date","isRecurring":false,"orderWeight":6}),
        json!({"id":27,"name":"customSyntheticUnused","label":"Synthetic Unused Field","type":"text","orderWeight":7}),
        json!({"id":28,"name":"customSyntheticAmbiguousChoice","label":"Synthetic Ambiguous Choice","type":"dropdown","choices":["Hot","hot"],"orderWeight":8}),
        json!({"id":29,"name":"customSyntheticLongEvidence","label":"Synthetic Long Evidence","type":"text","orderWeight":9}),
    ]);
    let oversized_key = format!("customSynthetic{}", "長".repeat(800));
    fields.push(json!({"id":30,"name":oversized_key,"label":"Synthetic Oversized Machine Key","type":"text","orderWeight":10}));
    let people = fixture["people"]
        .as_array_mut()
        .ok_or("Synthetic People fixture must be an array")?;
    if people
        .iter()
        .map(|person| person["id"].as_u64())
        .collect::<Vec<_>>()
        != vec![Some(101), Some(102), Some(103)]
    {
        return Err("Original synthetic People fixture scope changed".into());
    }
    // The original 101/102 contacts still overlap. Neither contact equality nor
    // a shared tag/field authorizes merging those two imported People.
    people[0]["tags"] = json!([
        "Synthetic embedded tag",
        "  Buyer  ",
        "BUYER",
        "Shared Household",
        "  Buyer  ",
        17,
        null
    ]);
    people[0]["customSyntheticPreferredArea"] = json!("None");
    people[0]["customSyntheticBuyerNote"] = json!(" \tSynthetic buyer note\r\n");
    people[0]["customSyntheticBudget"] = json!(500_000);
    people[0]["customSyntheticMoveDate"] = json!("2026-10-15");
    people[0]["customSyntheticAmbiguousChoice"] = json!("Hot");
    people[0][oversized_key.as_str()] = json!("Preserve oversized source key exactly");
    people[0]["qaCase"] = json!("literal_none_trimmed_text_aliases_invalid_tag_elements");

    people[1]["tags"] = json!(["Shared Household", "Investor"]);
    people[1]["customSyntheticPreferredArea"] = json!("N/A");
    people[1]["customSyntheticBuyerNote"] = json!("");
    people[1]["customSyntheticBudget"] = json!("500000");
    people[1]["customSyntheticMoveDate"] = json!("2026-10-15T12:00:00Z");
    people[1]["qaCase"] = json!("literal_na_held_empty_text_numeric_string_timestamp");

    people[2]["tags"] = json!(["Only On Parent-Held Person"]);
    people[2]["customSyntheticPreferredArea"] = json!("South");
    people[2]["qaCase"] = json!("parent_trash_excluded_never_resurrected");

    let overflow_tags = (1..=21)
        .map(|index| format!("Overflow {index:02}"))
        .collect::<Vec<_>>();
    people.push(json!({
        "id":104,"firstName":"Synthetic","lastName":"Tag Set Overflow",
        "stage":"Lead","assignedUserId":7,"emails":[],"phones":[],
        "tags":overflow_tags,"customSyntheticPreferredArea":"null",
        "customSyntheticBuyerNote":null,"customSyntheticBudget":true,
        "qaCase":"hold_all_21_new_tag_links_literal_null_choice"
    }));
    // This exact deterministic string is 2,687,014 UTF-8 bytes; canonical JSON
    // string escaping makes it 2,818,088 bytes. Never coerce it into a valid native
    // text value or copy a whole raw page per metadata value.
    let long_evidence = format!(
        "https://synthetic.invalid/never-fetch#{}",
        "Synthetic José 🏡 <p>&lt;script&gt;never execute&lt;/script&gt;</p> \"quoted\" \\\n"
            .repeat(32_768)
    );
    if long_evidence.len() <= 2 * 1024 * 1024 {
        return Err("Synthetic metadata evidence must exceed two MiB".into());
    }
    people.push(json!({
        "id":105,"firstName":"Synthetic","lastName":"Large Metadata Evidence",
        "stage":"Lead","assignedUserId":7,"sourceUrl":"https://synthetic.invalid/never-fetch",
        "emails":[{"value":"overlap@people.invalid","type":"home","isPrimary":1}],
        "phones":[{"value":"+12025550100","type":"mobile","isPrimary":1}],
        "tags":["Buyer","Shared Household"],"customSyntheticPreferredArea":"Unknown Option",
        "customSyntheticBudget":1.23456,"customSyntheticMoveDate":"1899-12-31",
        "customSyntheticLongEvidence":long_evidence,
        "customSyntheticUndeclared":{"preserveMe":true},
        "qaCase":"exact_large_held_text_precision_range_unknown_choice_and_key"
    }));
    people.push(json!({
        "id":LONG_SOURCE_ID_MARKER,"firstName":"Synthetic","lastName":"Null And Missing",
        "stage":"Lead","assignedUserId":7,"emails":[],"phones":[],"tags":["Buyer","x".repeat(1024)],
        "customSyntheticPreferredArea":null,"customSyntheticBuyerNote":"Independent valid value",
        "customSyntheticBudget":null,"customSyntheticMoveDate":"2200-12-31",
        "qaCase":"long_source_id_tag_aliases_preserve_custom_null_and_absent_fields"
    }));
    for collection in ["people", "customfields"] {
        let records = fixture[collection]
            .as_array()
            .ok_or("Missing synthetic collection")?;
        let page = json!({"_metadata":{"collection":collection,"offset":0,"limit":100,"total":records.len()},collection:records});
        let body = fixture_body(&page)?;
        if records.len() > 100 || body.len() > MAX_CAPTURE_BYTES {
            return Err("Synthetic metadata fixture exceeds its qualified capture bound".into());
        }
        if collection == "people" {
            let parsed = snapshot_source::parse(
                &Request {
                    stream: Stream::People,
                    cursor: Default::default(),
                    source_id: None,
                },
                &body,
            )
            .map_err(|_| "Synthetic People fixture failed lossless source parsing")?;
            if parsed.records.len() != 6
                || parsed.records[5].source_id.as_deref() != Some("9".repeat(128).as_str())
            {
                return Err("Synthetic 128-digit numeric source ID was not preserved".into());
            }
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
    metadata_units: Option<u64>,
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

    fn write_metadata_stats(&self, completed: u64) -> Result<(), ReaderError> {
        let stats_path = self.control_path.with_extension("metadata-stats.json");
        let temporary = self.control_path.with_extension("metadata-stats.tmp");
        let bytes = serde_json::to_vec(&json!({"completed_metadata_units": completed}))
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
            source_version: Some("synthetic-010f1-qa".into()),
        })
    }
}

// Only this compile-fenced example changes scheduling. Every admitted unit runs
// the real retained-only worker, with its normal authorization and transaction
// rules. No source-reader call or native table mutation is introduced here.
fn spawn_metadata_worker(
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
            if reader.write_metadata_stats(completed).is_err() {
                continue;
            }
            let batch = if control.metadata_units.is_some() {
                1
            } else {
                32
            };
            for index in 0..batch {
                let Ok(current) = reader.control() else {
                    break;
                };
                if current
                    .metadata_units
                    .is_some_and(|grant| completed >= grant || index > 0)
                {
                    break;
                }
                let Some(next) = completed.checked_add(1) else {
                    break;
                };
                match metadata_worker::run_once(&pool, &key, &policy).await {
                    Ok(true) => {
                        completed = next;
                        if reader.write_metadata_stats(completed).is_err() {
                            break;
                        }
                    }
                    Ok(false) => break,
                    Err(_) => {
                        tracing::warn!("Synthetic QA metadata unit returned an error");
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
            source_version: Some("synthetic-010f1-qa".into()),
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
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "Usage: metadata_import_qa /absolute/private/qa.env /absolute/private/control.json"
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
        fixture: metadata_fixture()?,
        source_reader_calls: Mutex::new(0),
    });
    reader
        .control()
        .map_err(|_| "Invalid private QA control file")?;
    reader
        .write_stats(0)
        .map_err(|_| "Could not initialize private QA reader stats")?;
    reader
        .write_metadata_stats(0)
        .map_err(|_| "Could not initialize private QA metadata stats")?;

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
        "CRM_API_BIND_ADDR" => Some("127.0.0.1:3016".into()),
        "CRM_CORS_ALLOWED_ORIGIN" => Some("http://127.0.0.1:5186".into()),
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
        reader.clone(),
        config.snapshot_policy.clone(),
    );
    let import_worker = import_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let metadata_worker = spawn_metadata_worker(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
        reader,
    );
    println!("Synthetic-only QA API listening on 127.0.0.1:3016; database crm_010f1_qa");
    let result = axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    assessment_worker.abort();
    snapshot_worker.abort();
    import_worker.abort();
    metadata_worker.abort();
    pool.close().await;
    result?;
    Ok(())
}

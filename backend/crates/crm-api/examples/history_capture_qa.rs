//! Constructed-source, real-API browser QA for Slice 010d1; never a FUB capture.
//!
//! Build explicitly with `cargo build -p crm-api --example history_capture_qa
//! --features test-support --locked`. Run with the two absolute private paths
//! `/private/tmp/crm-010d1-qa-5hqgjwkd/qa.env` and that directory's `control.json`.
//! The coordinator owns database/service creation. This executable never creates
//! or drops a database, loads ambient credentials, uses an HTTP FUB reader or
//! sends source HTTP requests. AppState::for_tests constructs a disabled reader
//! before this fixture replaces it. It binds API13010 before migrations/bootstrap. PG must be
//! loopback55432/crm_010d1_qa with crm_app/crm_migrator roles; Web15173 and
//! Centrifugo18082 are the only configured endpoints. Launch with
//! RUST_LOG=info,sqlx=warn; telemetry otherwise reads its normal logging settings.
//!
//! Prepare People through the ordinary connection/snapshot/preview/import APIs
//! using `synthetic-snapshot`. Optional sibling imports also use normal commands.
//! Do not combine invitation setup with overlapping seed_dev.py fixture users.
//! History grants call the real worker; no control writes migration/native rows.
//!
//! Initial private control example:
//! {"epoch":"initial","scenario":"happy","mode":"normal","delay_ms":0,
//!  "history_units":0,"note_detail_gap":true,"failure_stream":"events",
//!  "retry_failures":2,"retry_after_seconds":2}
//! Unknown fields fail closed. Modes: normal, overlap, denied, retry, unavailable,
//! invalid_identity, changed_account, changed_user, delayed, empty, pause_people.
//! A request freezes its control snapshot before delay. Denied/retry target only
//! failure_stream; retry fails the first retry_failures requests for each exact
//! scenario/path in this process. Use a new scenario to begin a new retry case.
//! Changed account/user affect identity only. Overlap returns event pages1-100
//! and100-199 at total200 with a changed id100; this MUST pause reconciliation.
//!
//! history_units is a cumulative grant of completed real-worker units, not rows
//! or source requests. null restores normal bounded cadence; a number grants at
//! most one unit per tick. Changing controls never resumes a paused run: use its
//! API Retry. Start/restart requires grant0 and a new epoch. A started unit may
//! finish after controls change; the domain worker owns fencing and admission.
//!
//! Normal history is201 events,101 calls,101 texts: exactly7 collection pages,
//! separate from identity/core requests. Events use offset then opaque token;
//! calls use offsets; texts use a token. Content/URLs are inert synthetic evidence.
//! control.stats.json plus source-<epoch>.json contain bounded request start/end,
//! method/path, operation, status, bytes and SHA256, never credentials/bodies.
//! Requests are recorded before delay so in-flight cancellation is observable.
//! Recorder capacity2048 fails closed; an epoch file is never silently replaced
//! by a new process. control.history-stats.json counts completed worker units;
//! control.delivery-stats.json counts publications and session disconnects.
//! Snapshot setup legitimately reads note details. Assert history-only GET scope
//! after that setup; retained history reads/proposals must add no source calls.
//! Reader injection does not test production HTTP headers/pacing/redirects: those
//! require the production reader's separate loopback transport tests.

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
    auth::workspace::ReleaseReadiness,
    config::{Config, RawPayloadKey},
    domain::{
        admin::commands::{grant_platform_admin, GrantPlatformAdmin},
        migration::{
            activity_worker, history_capture_source as history_source, history_capture_worker,
            import_worker, metadata_worker,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::SnapshotPolicy,
            snapshot_source::{Request, Stream},
            snapshot_worker, worker,
        },
    },
    realtime::Publisher,
    state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgConnectOptions, PgPool, Row};

type HarnessError = Box<dyn std::error::Error>;
const QA_DIRECTORY: &str = "/private/tmp/crm-010d1-qa-5hqgjwkd";
const QA_DATABASE: &str = "crm_010d1_qa";
const QA_DATABASE_PORT: u16 = 55432;
const QA_CENTRIFUGO_URL: &str = "http://127.0.0.1:18082/api";
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORDED_REQUESTS: usize = 2048;
const MAX_STATS_BYTES: usize = 4 * 1024 * 1024;
const SOURCE_VERSION: &str = "synthetic-010d1-qa";
const EVENTS_TOKEN: &str = "synthetic/events?position=200&literal=+#";
const TEXTS_TOKEN: &str = "synthetic/texts?position=100&literal=+#";
const FIXTURE: &str = include_str!("fixtures/snapshot_qa.json");

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Normal,
    Overlap,
    Denied,
    Retry,
    Unavailable,
    InvalidIdentity,
    ChangedAccount,
    ChangedUser,
    Delayed,
    Empty,
    PausePeople,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    epoch: String,
    scenario: String,
    mode: Mode,
    delay_ms: u64,
    history_units: Option<u64>,
    note_detail_gap: bool,
    failure_stream: history_source::Stream,
    retry_failures: u32,
    retry_after_seconds: u64,
}

fn safe_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

fn private_regular_file(path: &Path) -> Result<(), HarnessError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "Private QA file is missing")?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("QA files must be regular private files, not symlinks".into());
    }
    Ok(())
}

fn read_control(path: &Path) -> Result<Control, ReaderError> {
    private_regular_file(path).map_err(|_| ReaderError::Unavailable)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(4097).read_to_end(&mut bytes))
        .map_err(|_| ReaderError::Unavailable)?;
    if bytes.len() > 4096 {
        return Err(ReaderError::Unavailable);
    }
    let control: Control = serde_json::from_slice(&bytes).map_err(|_| ReaderError::Unavailable)?;
    if !safe_label(&control.epoch)
        || !safe_label(&control.scenario)
        || control.delay_ms > 5000
        || control.retry_failures > 3
        || !(1..=30).contains(&control.retry_after_seconds)
        || control.history_units.is_some_and(|grant| grant > 100_000)
    {
        return Err(ReaderError::Unavailable);
    }
    Ok(control)
}

fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), ReaderError> {
    if path.parent() != Some(Path::new(QA_DIRECTORY)) {
        return Err(ReaderError::Unavailable);
    }
    let bytes = serde_json::to_vec(value).map_err(|_| ReaderError::Unavailable)?;
    if bytes.len() > MAX_STATS_BYTES {
        return Err(ReaderError::Unavailable);
    }
    let temporary = path.with_extension("tmp");
    if temporary.exists() {
        private_regular_file(&temporary).map_err(|_| ReaderError::Unavailable)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| ReaderError::Unavailable)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| ReaderError::Unavailable)?;
    file.write_all(&bytes)
        .map_err(|_| ReaderError::Unavailable)?;
    std::fs::rename(temporary, path).map_err(|_| ReaderError::Unavailable)
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Identity,
    CoreProbe,
    CoreSnapshot,
    History,
}

#[derive(Serialize)]
struct RecordedRequest {
    sequence: u64,
    scenario: String,
    mode: Mode,
    operation: Operation,
    method: &'static str,
    path: String,
    started_at: String,
    completed_at: Option<String>,
    status: Option<u16>,
    response_bytes: Option<usize>,
    response_sha256: Option<String>,
    outcome: &'static str,
}

#[derive(Serialize)]
struct SourceStats {
    fixture: &'static str,
    epoch: String,
    source_reader_calls: u64,
    identity_reader_calls: u64,
    core_reader_calls: u64,
    history_reader_calls: u64,
    real_source_http_requests: u64,
    requests: Vec<RecordedRequest>,
}

struct FixtureReader {
    control_path: PathBuf,
    epoch: String,
    fixture: Value,
    history: [Vec<Value>; 3],
    stats: Mutex<SourceStats>,
}

impl FixtureReader {
    fn control(&self) -> Result<Control, ReaderError> {
        let control = read_control(&self.control_path)?;
        if control.epoch != self.epoch {
            return Err(ReaderError::Unavailable);
        }
        Ok(control)
    }

    fn write_stats(&self, stats: &SourceStats) -> Result<(), ReaderError> {
        write_private_json(&self.control_path.with_extension("stats.json"), stats)?;
        write_private_json(
            &Path::new(QA_DIRECTORY).join(format!("source-{}.json", self.epoch)),
            stats,
        )
    }

    fn begin(
        &self,
        control: &Control,
        operation: Operation,
        path: String,
    ) -> Result<(usize, u32), ReaderError> {
        if path.len() > 1024 || path.chars().any(char::is_control) {
            return Err(ReaderError::MalformedResponse);
        }
        let mut stats = self.stats.lock().map_err(|_| ReaderError::Unavailable)?;
        if stats.requests.len() >= MAX_RECORDED_REQUESTS {
            return Err(ReaderError::Unavailable);
        }
        let attempt = stats
            .requests
            .iter()
            .filter(|record| {
                record.scenario == control.scenario
                    && record.mode == control.mode
                    && record.operation == operation
                    && record.path == path
            })
            .count() as u32
            + 1;
        let index = stats.requests.len();
        stats.source_reader_calls += 1;
        match operation {
            Operation::Identity => stats.identity_reader_calls += 1,
            Operation::History => stats.history_reader_calls += 1,
            Operation::CoreProbe | Operation::CoreSnapshot => stats.core_reader_calls += 1,
        }
        stats.requests.push(RecordedRequest {
            sequence: index as u64 + 1,
            scenario: control.scenario.clone(),
            mode: control.mode,
            operation,
            method: "GET",
            path,
            started_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            status: None,
            response_bytes: None,
            response_sha256: None,
            outcome: "in_flight",
        });
        self.write_stats(&stats)?;
        Ok((index, attempt))
    }

    fn finish(
        &self,
        index: usize,
        result: Result<Capture, ReaderError>,
    ) -> Result<Capture, ReaderError> {
        let (outcome, capture) = match &result {
            Ok(capture) => ("ok", Some(capture)),
            Err(ReaderError::Captured { error, capture }) => (error.code(), Some(capture)),
            Err(error) => (error.code(), None),
        };
        let mut stats = self.stats.lock().map_err(|_| ReaderError::Unavailable)?;
        let row = stats
            .requests
            .get_mut(index)
            .ok_or(ReaderError::Unavailable)?;
        row.completed_at = Some(chrono::Utc::now().to_rfc3339());
        row.outcome = outcome;
        if let Some(capture) = capture {
            row.status = Some(capture.status);
            row.response_bytes = Some(capture.body.len());
            row.response_sha256 = Some(
                Sha256::digest(&capture.body)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            );
        }
        self.write_stats(&stats)?;
        result
    }

    async fn delay(control: &Control) {
        let delay = if control.mode == Mode::Delayed {
            control.delay_ms.max(1000)
        } else {
            control.delay_ms
        };
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    fn credential(key: &str) -> Result<(), ReaderError> {
        if key == "synthetic-snapshot" {
            Ok(())
        } else {
            Err(ReaderError::InvalidCredential)
        }
    }

    fn capture(&self, status: u16, value: &Value) -> Result<Capture, ReaderError> {
        let body = serde_json::to_vec(value).map_err(|_| ReaderError::MalformedResponse)?;
        if body.len() > MAX_CAPTURE_BYTES {
            return Err(ReaderError::ResponseTooLarge);
        }
        Ok(Capture {
            status,
            body,
            truncated: false,
            source_version: Some(SOURCE_VERSION.into()),
        })
    }

    fn source_failure(&self, status: u16, error: ReaderError) -> Result<Capture, ReaderError> {
        Err(error.with_capture(self.capture(status, &json!({"error":"Synthetic source failure"}))?))
    }

    fn history_page(
        &self,
        request: &history_source::Request,
        control: &Control,
    ) -> Result<Capture, ReaderError> {
        let slot = match request.stream {
            history_source::Stream::Events => 0,
            history_source::Stream::Calls => 1,
            history_source::Stream::TextMessages => 2,
        };
        let offset =
            usize::try_from(request.cursor.offset).map_err(|_| ReaderError::MalformedResponse)?;
        let overlap = control.mode == Mode::Overlap && slot == 0;
        let total = if control.mode == Mode::Empty {
            0
        } else if overlap {
            200
        } else {
            self.history[slot].len()
        };
        let expected_token = match (slot, offset, overlap) {
            (0, 200, false) => Some(EVENTS_TOKEN),
            (2, 100, _) => Some(TEXTS_TOKEN),
            _ => None,
        };
        if request.cursor.next.as_deref() != expected_token
            || (total == 0 && offset != 0)
            || (total != 0 && (offset >= total || offset % 100 != 0))
        {
            return Err(ReaderError::MalformedResponse);
        }
        let mut page = if total == 0 {
            Vec::new()
        } else {
            let start = if overlap && offset == 100 { 99 } else { offset };
            self.history[slot]
                .iter()
                .skip(start)
                .take((total - offset).min(100))
                .cloned()
                .collect::<Vec<_>>()
        };
        if overlap && offset == 100 {
            page[0]["message"] = json!("SYNTHETIC_HISTORY_CONFLICT_BODY_SENTINEL");
            page[0]["personId"] = json!(102);
        }
        let next = match (slot, offset, overlap, total) {
            (0, 100, false, 201) => Some(EVENTS_TOKEN),
            (2, 0, _, 101) => Some(TEXTS_TOKEN),
            _ => None,
        };
        let collection = request.stream.collection();
        self.capture(200, &json!({
            "_metadata":{"collection":collection,"offset":offset,"limit":100,"total":total,
                "next":next,"nextLink":if next.is_some() { Some("https://synthetic.invalid/never-follow") } else { None }},
            collection:page,
        }))
    }
}

fn history_fixtures() -> [Vec<Value>; 3] {
    std::array::from_fn(|slot| {
        let total = if slot == 0 { 201 } else { 101 };
        (0..total).map(|index| {
            let mut row = json!({
                "id":index + 1,"personId":101,"created":"2020-09-01T12:00:00Z",
                "updated":"2020-09-02T12:00:00Z","createdById":7,"updatedById":8,
                "unknownPreserved":{"literal":"SYNTHETIC_HISTORY_UNKNOWN_SENTINEL","sequence":index},
            });
            match slot {
                0 => {
                    row["type"] = json!("Property Inquiry");
                    row["source"] = json!("Synthetic Source");
                    row["message"] = json!("SYNTHETIC_HISTORY_EVENT_BODY_SENTINEL");
                    row["property"] = json!({"url":"https://synthetic.invalid/never-fetch","street":"Synthetic Address Sentinel"});
                }
                1 => {
                    row["userId"] = json!(7);
                    row["outcome"] = json!("Source-only outcome");
                    row["isIncoming"] = json!(index % 2 == 0);
                    row["duration"] = json!(63);
                    row["note"] = json!("SYNTHETIC_HISTORY_CALL_BODY_SENTINEL");
                    row["recordingUrl"] = json!("https://synthetic.invalid/never-fetch-recording");
                }
                _ => {
                    row["userId"] = json!(7);
                    row["status"] = json!("Source-only status");
                    row["sent"] = json!("2020-09-01T12:01:00Z");
                    row["isIncoming"] = json!(index % 2 == 0);
                    row["message"] = json!("SYNTHETIC_HISTORY_TEXT_BODY_SENTINEL <img src='https://synthetic.invalid/never-fetch' onerror='window.syntheticExecuted=true'>");
                    row["fromNumber"] = json!("+12025550100");
                    row["toNumber"] = json!("+12025550101");
                    row["media"] = json!([{ "url":"https://synthetic.invalid/never-fetch-media" }]);
                    if index == 0 {
                        row["groupTextId"] = json!(42);
                        row["participants"] = json!([
                            {"type":"person","personId":101,"sender":true},
                            {"type":"person","personId":102,"sender":false},
                            {"type":"relationship","relationshipId":17,"personId":0},
                        ]);
                    }
                }
            }
            // Fixed special cases do not alter valid, unique record IDs.
            match index {
                1 => row["personId"] = json!(102),
                2 => row["personId"] = json!(103), // held by the core parent
                3 => row["personId"] = json!(999),
                4 => row["personId"] = json!(0),
                5 => { row.as_object_mut().expect("constructed object").remove("personId"); }
                6 => { row["created"] = json!("unknown historical time"); row["updated"] = Value::Null; }
                7 => row["showContent"] = json!(false),
                8 => row[match slot { 0 => "type", 1 => "outcome", _ => "status" }] = json!("長".repeat(100)),
                _ => {}
            }
            if index == total - 1 { row["id"] = json!(9_007_199_254_740_993_u64); }
            row
        }).collect()
    })
}

#[async_trait]
impl FubReader for FixtureReader {
    async fn identity(&self, key: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        let control = self.control()?;
        let (index, _) = self.begin(&control, Operation::Identity, "identity".into())?;
        Self::delay(&control).await;
        let account_id = if control.mode == Mode::ChangedAccount {
            202
        } else {
            101
        };
        let user_id = if control.mode == Mode::ChangedUser {
            8
        } else {
            7
        };
        let result = (|| {
            Self::credential(key)?;
            if control.mode == Mode::InvalidIdentity {
                return self.source_failure(401, ReaderError::InvalidCredential);
            }
            if control.mode == Mode::Unavailable {
                return Err(ReaderError::Unavailable);
            }
            self.capture(
                200,
                &json!({"account":{"id":account_id,"domain":"Synthetic Snapshot Realty"},
                "user":{"id":user_id,"name":"Synthetic Admin"}}),
            )
        })();
        let capture = self.finish(index, result)?;
        Ok((
            Identity {
                account_id,
                account_domain: Some("Synthetic Snapshot Realty".into()),
                user_id: Some(user_id),
                display_name: Some("Synthetic Admin".into()),
            },
            capture.body,
        ))
    }

    async fn probe(&self, key: &str, probe: Probe) -> Result<ProbeResult, ReaderError> {
        let control = self.control()?;
        let (collection, path) = match probe {
            Probe::PeopleExcludingTrash => {
                ("people", "people?limit=1&fields=id&includeTrash=false")
            }
            Probe::PeopleIncludingTrash => ("people", "people?limit=1&fields=id&includeTrash=true"),
            Probe::Users => ("users", "users?limit=1"),
            Probe::Stages => ("stages", "stages?limit=1"),
            Probe::CustomFields => ("customfields", "customFields?limit=1"),
        };
        let (index, _) = self.begin(&control, Operation::CoreProbe, path.into())?;
        Self::delay(&control).await;
        let mut total = 0;
        let result = (|| {
            Self::credential(key)?;
            if control.mode == Mode::Unavailable {
                return Err(ReaderError::Unavailable);
            }
            let records = self.fixture[collection]
                .as_array()
                .ok_or(ReaderError::MalformedResponse)?;
            let selected = records
                .iter()
                .filter(|row| probe != Probe::PeopleExcludingTrash || row["isTrash"] != true)
                .collect::<Vec<_>>();
            total = selected.len();
            self.capture(200, &json!({"_metadata":{"collection":collection,"offset":0,"limit":1,"total":total},collection:selected.into_iter().take(1).collect::<Vec<_>>()}))
        })();
        let capture = self.finish(index, result)?;
        Ok(ProbeResult {
            status: capture.status,
            body: capture.body,
            reported_total: Some(total.to_string()),
            retrieved_count: i32::from(total > 0),
            continuation: total > 1,
            source_version: capture.source_version,
        })
    }

    async fn snapshot(&self, key: &str, request: &Request) -> Result<Capture, ReaderError> {
        let control = self.control()?;
        let (index, _) = self.begin(&control, Operation::CoreSnapshot, request.path()?)?;
        Self::delay(&control).await;
        let result = (|| {
            Self::credential(key)?;
            if control.mode == Mode::Unavailable {
                return Err(ReaderError::Unavailable);
            }
            if control.mode == Mode::PausePeople && request.stream == Stream::People {
                return self.source_failure(403, ReaderError::AccessDenied);
            }
            if request.stream == Stream::NoteDetail {
                let id = request
                    .source_id
                    .as_deref()
                    .ok_or(ReaderError::MalformedResponse)?;
                if id == "302" && control.note_detail_gap {
                    return self.capture(404, &json!({"error":"Synthetic restricted note detail"}));
                }
                return self.capture(
                    200,
                    self.fixture["note_details"]
                        .get(id)
                        .ok_or(ReaderError::MalformedResponse)?,
                );
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
            if request.cursor.next.is_some() {
                return Err(ReaderError::MalformedResponse);
            }
            let records = self.fixture[fixture_key]
                .as_array()
                .ok_or(ReaderError::MalformedResponse)?;
            let offset = usize::try_from(request.cursor.offset)
                .map_err(|_| ReaderError::MalformedResponse)?;
            let page = records.iter().skip(offset).take(100).collect::<Vec<_>>();
            self.capture(200, &json!({"_metadata":{"collection":collection,"offset":offset,"limit":100,"total":records.len()},collection:page}))
        })();
        self.finish(index, result)
    }

    async fn history(
        &self,
        key: &str,
        request: &history_source::Request,
    ) -> Result<Capture, ReaderError> {
        let control = self.control()?;
        let (index, attempt) = self.begin(&control, Operation::History, request.path()?)?;
        Self::delay(&control).await;
        let result = (|| {
            Self::credential(key)?;
            if control.mode == Mode::Unavailable {
                return Err(ReaderError::Unavailable);
            }
            if request.stream == control.failure_stream {
                if control.mode == Mode::Denied {
                    return self.source_failure(403, ReaderError::AccessDenied);
                }
                if control.mode == Mode::Retry && attempt <= control.retry_failures {
                    return self.source_failure(
                        429,
                        ReaderError::RateLimited(Some(control.retry_after_seconds)),
                    );
                }
            }
            self.history_page(request, &control)
        })();
        self.finish(index, result)
    }
}

fn spawn_history_worker(
    pool: PgPool,
    key: RawPayloadKey,
    policy: SnapshotPolicy,
    reader: Arc<FixtureReader>,
    release: Arc<ReleaseReadiness>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut completed = 0_u64;
        let mut errors = 0_u64;
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Ok(control) = reader.control() else {
                continue;
            };
            let stats_path = reader.control_path.with_extension("history-stats.json");
            if write_private_json(&stats_path, &json!({"epoch":reader.epoch,"completed_history_units":completed,"worker_errors":errors})).is_err() { continue; }
            let batch = if control.history_units.is_some() {
                1
            } else {
                32
            };
            for index in 0..batch {
                let Ok(current) = reader.control() else {
                    break;
                };
                if current
                    .history_units
                    .is_some_and(|grant| completed >= grant || index > 0)
                {
                    break;
                }
                let Some(next) = completed.checked_add(1) else {
                    break;
                };
                match history_capture_worker::run_once(
                    &pool,
                    &key,
                    reader.as_ref(),
                    &policy,
                    Some(release.as_ref()),
                )
                .await
                {
                    Ok(true) => completed = next,
                    Ok(false) => break,
                    Err(_) => {
                        errors = errors.saturating_add(1);
                        tracing::warn!("Synthetic QA history unit returned an error");
                        break;
                    }
                }
                if write_private_json(&stats_path, &json!({"epoch":reader.epoch,"completed_history_units":completed,"worker_errors":errors})).is_err() { break; }
            }
        }
    })
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
    private_regular_file(path)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(65_537).read_to_end(&mut bytes))
        .map_err(|_| "Could not read private QA env file")?;
    if bytes.len() > 65_536 || bytes.contains(&b'$') {
        return Err(
            "Private QA env must contain bounded explicit literals without interpolation".into(),
        );
    }
    dotenvy::from_read_iter(bytes.as_slice())
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|_| "Could not parse private QA env file".into())
}

#[tokio::main]
async fn main() -> Result<(), HarnessError> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let qa_dir = Path::new(QA_DIRECTORY);
    if args.len() != 2
        || Path::new(&args[0]) != qa_dir.join("qa.env")
        || Path::new(&args[1]) != qa_dir.join("control.json")
    {
        return Err("Pass the exact owned 010d1 private qa.env and control.json paths".into());
    }
    let dir =
        std::fs::symlink_metadata(qa_dir).map_err(|_| "Owned private QA directory is missing")?;
    if !dir.is_dir() || dir.permissions().mode() & 0o077 != 0 {
        return Err("Owned QA directory must be private and not a symlink".into());
    }
    let env = private_env(Path::new(&args[0]))?;
    let control_path = PathBuf::from(&args[1]);
    let control = read_control(&control_path).map_err(|_| "Invalid private QA control")?;
    if control.history_units != Some(0) {
        return Err("Startup requires history_units=0; grant work explicitly after startup".into());
    }
    let epoch_path = qa_dir.join(format!("source-{}.json", control.epoch));
    if epoch_path.exists() {
        return Err(
            "This recorder epoch already exists; choose a new epoch before restarting".into(),
        );
    }
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
        return Err("QA realtime must name the exact isolated loopback service".into());
    }
    let config = Config::from_source(|name| match name {
        "CRM_API_BIND_ADDR" => Some("127.0.0.1:13010".into()),
        "CRM_CORS_ALLOWED_ORIGIN" => Some("http://127.0.0.1:15173".into()),
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
    let reader = Arc::new(FixtureReader {
        control_path,
        epoch: control.epoch.clone(),
        fixture: serde_json::from_str(FIXTURE)?,
        history: history_fixtures(),
        stats: Mutex::new(SourceStats {
            fixture: SOURCE_VERSION,
            epoch: control.epoch,
            source_reader_calls: 0,
            identity_reader_calls: 0,
            core_reader_calls: 0,
            history_reader_calls: 0,
            real_source_http_requests: 0,
            requests: Vec::new(),
        }),
    });
    // Binding precedes all database writes and recorder initialization. This
    // prevents a second process from overwriting the active process's evidence.
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    {
        let stats = reader
            .stats
            .lock()
            .map_err(|_| "QA recorder is unavailable")?;
        reader
            .write_stats(&stats)
            .map_err(|_| "Cannot initialize private source recorder")?;
    }
    write_private_json(
        &reader.control_path.with_extension("history-stats.json"),
        &json!({"epoch":reader.epoch,"completed_history_units":0,"worker_errors":0}),
    )
    .map_err(|_| "Cannot initialize private worker recorder")?;
    crm_api::telemetry::init();
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
            display_name: "Synthetic History QA Platform Owner".into(),
            password: seed_password,
        },
    )
    .await?;
    migrator.close().await;
    let pool = PgPool::connect_with(app_options).await?;
    verify_connected_scope(&pool, "crm_app").await?;
    let publisher = Publisher::recording();
    let Publisher::Recording(publications, disconnects) = publisher.clone() else {
        return Err("Synthetic QA requires the recording publisher".into());
    };
    let delivery_path = reader.control_path.with_extension("delivery-stats.json");
    let delivery_monitor = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let counts = json!({"publications":publications.lock().await.len(),"disconnects":disconnects.lock().await.len()});
            if write_private_json(&delivery_path, &counts).is_err() {
                tracing::warn!("Synthetic QA delivery count could not be persisted");
            }
        }
    });
    let release = Arc::new(ReleaseReadiness::for_tests());
    let mut state =
        AppState::for_tests(pool.clone(), &config, publisher).with_migration_reader(reader.clone());
    state.import_release = Some(release.clone());
    let assessment = worker::spawn(pool.clone(), config.raw_payload_key.clone(), reader.clone());
    let snapshot = snapshot_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        reader.clone(),
        config.snapshot_policy.clone(),
    );
    let imports = import_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let metadata = metadata_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let activity = activity_worker::spawn(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
    );
    let history = spawn_history_worker(
        pool.clone(),
        config.raw_payload_key.clone(),
        config.snapshot_policy.clone(),
        reader,
        release,
    );
    println!("Synthetic-only QA API listening on 127.0.0.1:13010; database crm_010d1_qa");
    let result = axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    for task in [
        assessment,
        snapshot,
        imports,
        metadata,
        activity,
        history,
        delivery_monitor,
    ] {
        task.abort();
    }
    pool.close().await;
    result?;
    Ok(())
}

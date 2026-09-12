//! Opt-in Slice 019b paired authenticated HTTP performance harness.
//!
//! Both applications use the current request/auth/router stack. The frozen
//! arm differs only through a server-scoped task-local static-SQL adapter.

use crate::db_import_contact_perf::driver;
#[path = "fixtures/statements_6bad52a/mod.rs"]
mod frozen;
#[path = "fixtures/live_statement_explain_019b.rs"]
mod live_explain;

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    Router,
};
use chrono::{TimeZone, Utc};
use crm_api::{
    config::Config,
    domain::person::filter_test_support::{self, FilterStatementFamily, FilterStatementOverrides},
    realtime::Publisher,
    state::AppState,
};
use driver::{
    AttemptContext, AttemptIds, AttemptOutcome, AttemptPhase, ClientFailure, ClientFailureKind,
    CompletedHttpResponse, HttpBodyResult, QueryArm, TimingCapture, WaveCapture,
};
use reqwest::header::{COOKIE, SET_COOKIE};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::Digest;
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use uuid::Uuid;

const PEOPLE: i64 = 25_000;
const SERIAL_WARMUPS: usize = 5;
const SERIAL_SAMPLES: usize = 40;
const TODAY_WARMUP_WAVES: usize = 2;
const TODAY_SAMPLES_WAVES: usize = 8;
const TODAY_CONCURRENCY: usize = 5;

#[derive(Clone)]
struct SavedSource {
    id: Uuid,
    revision: i64,
}
#[derive(Clone)]
struct Fixture {
    // Keep identifiers and credentials in memory only. Artifacts expose the
    // manifest hash/counts, never this operational fixture state.
    organization_id: Uuid,
    viewer_id: Uuid,
    viewer_email: String,
    password: String,
    sources: Vec<SavedSource>,
    custom_fields: live_explain::ExplainFieldIds,
    now: chrono::DateTime<Utc>,
    manifest: FixtureManifest,
}

#[derive(Clone)]
struct Session {
    client: reqwest::Client,
    cookie: String,
}
struct Server {
    base_url: String,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl Server {
    async fn start(app: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind perf loopback");
        let address: SocketAddr = listener.local_addr().expect("loopback address");
        let (stop, shutdown) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown.await;
                })
                .await
                .expect("serve perf loopback");
        });
        Self {
            base_url: format!("http://{address}"),
            stop: Some(stop),
            task,
        }
    }
    async fn close(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.await.expect("join perf server");
    }
}

async fn scope_arm(
    State(overrides): State<FilterStatementOverrides>,
    request: Request,
    next: Next,
) -> Response {
    filter_test_support::scope(overrides, next.run(request)).await
}
fn app(
    pool: PgPool,
    config: &Config,
    now: chrono::DateTime<Utc>,
    overrides: FilterStatementOverrides,
) -> Router {
    let state = AppState::for_tests(pool, config, Publisher::recording());
    crm_api::build_app_with_today_router_and_perf_collector(
        state,
        crm_api::routes::today::router_with_test_clock(now),
        crm_api::domain::today::test_support::HttpPerfCollector::default(),
    )
    .layer(middleware::from_fn_with_state(overrides, scope_arm))
}
async fn login(base_url: &str, email: &str, password: &str) -> Session {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base_url}/api/session"))
        .json(&json!({"email":email,"password":password}))
        .send()
        .await
        .expect("authenticate fixture viewer");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "fixture login status"
    );
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .expect("session cookie")
        .to_owned();
    let _: Value = response.json().await.expect("session JSON");
    Session { client, cookie }
}

async fn configure_sources(session: &Session, base_url: &str, sources: &[SavedSource]) {
    let listed = session
        .client
        .get(format!("{base_url}/api/today/sources"))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("list today sources");
    assert_eq!(listed.status(), reqwest::StatusCode::OK);
    for source in sources {
        let response = session
            .client
            .put(format!("{base_url}/api/today/sources/{}", source.id))
            .header(COOKIE, &session.cookie)
            .json(&json!({"expected_list_revision": source.revision}))
            .send()
            .await
            .expect("enable today source");
        assert_eq!(response.status(), reqwest::StatusCode::OK, "enable source");
    }
}

#[derive(Clone, Serialize)]
struct AttemptMeta {
    body_sha256: String,
}

#[derive(Clone, Serialize)]
struct FixtureManifest {
    people: i64,
    members: usize,
    definitions: usize,
    values_per_person: usize,
    builtins: usize,
    call_only: usize,
    source_candidates: usize,
    shape_sha256: String,
}
fn classify_today_body(status: u16, value: Option<&Value>) -> HttpBodyResult {
    if status == 503 {
        return HttpBodyResult::Unavailable;
    }
    let Some(value) = value else {
        return HttpBodyResult::InvalidEnvelope;
    };
    let sources = value.get("sources");
    let source_status = sources
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str);
    let source_issues_empty = sources
        .and_then(|v| v.get("issues"))
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    let system_issues_empty = sources
        .and_then(|v| v.get("system_feed_issues"))
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    let items = value.get("items").and_then(Value::as_array);
    match (status, source_status, items) {
        (200, Some("complete"), Some(items))
            if items.len() <= 200 && source_issues_empty && system_issues_empty =>
        {
            HttpBodyResult::Complete
        }
        (200, Some("partial"), Some(_)) => HttpBodyResult::Partial,
        (200, Some("unavailable"), Some(_)) => HttpBodyResult::Unavailable,
        _ => HttpBodyResult::InvalidEnvelope,
    }
}

fn classify_response(path: &str, status: u16, value: Option<&Value>) -> HttpBodyResult {
    if path == "/api/today" {
        return classify_today_body(status, value);
    }
    let Some(value) = value else {
        return HttpBodyResult::InvalidEnvelope;
    };
    if path.starts_with("/api/people?") {
        let people = value.get("people").and_then(Value::as_array);
        let truncated = value.get("truncated").and_then(Value::as_bool);
        return match (status, people, truncated) {
            (200, Some(people), Some(_)) if people.len() <= 500 => HttpBodyResult::Complete,
            _ => HttpBodyResult::InvalidEnvelope,
        };
    }
    if path.starts_with("/api/saved-lists/") && path.contains("/count?") {
        return match (
            status,
            value.get("list_id").and_then(Value::as_str),
            value.get("revision").and_then(Value::as_i64),
            value.get("count").and_then(Value::as_i64),
            value.get("truncated").and_then(Value::as_bool),
        ) {
            (200, Some(_), Some(_), Some(count), Some(_)) if (0..=500).contains(&count) => {
                HttpBodyResult::Complete
            }
            _ => HttpBodyResult::InvalidEnvelope,
        };
    }
    HttpBodyResult::InvalidEnvelope
}

async fn assert_complete_response(response: reqwest::Response, path: &str, label: &str) {
    let status = response.status().as_u16();
    let bytes = response.bytes().await.expect("preflight response body");
    let value = serde_json::from_slice::<Value>(&bytes).ok();
    assert_eq!(
        classify_response(path, status, value.as_ref()),
        HttpBodyResult::Complete,
        "preflight complete envelope: {label}"
    );
}

async fn request(
    session: Session,
    base_url: String,
    path: String,
) -> Result<CompletedHttpResponse<AttemptMeta>, ClientFailure<AttemptMeta>> {
    let started = Instant::now();
    let response = match session
        .client
        .get(format!("{base_url}{path}"))
        .header(COOKIE, session.cookie)
        .send()
        .await
    {
        Ok(v) => v,
        Err(_) => {
            return Err(ClientFailure {
                request_completed_at: Instant::now(),
                kind: ClientFailureKind::Transport,
                metadata: AttemptMeta {
                    body_sha256: String::new(),
                },
            })
        }
    };
    let status = response.status().as_u16();
    let bytes = match response.bytes().await {
        Ok(v) => v,
        Err(_) => {
            return Err(ClientFailure {
                request_completed_at: Instant::now(),
                kind: ClientFailureKind::BodyRead,
                metadata: AttemptMeta {
                    body_sha256: String::new(),
                },
            })
        }
    };
    let value = serde_json::from_slice::<Value>(&bytes).ok();
    Ok(CompletedHttpResponse {
        request_completed_at: started + started.elapsed(),
        status,
        result: classify_response(&path, status, value.as_ref()),
        whole_source_evaluations: 0,
        metadata: AttemptMeta {
            body_sha256: sha2::Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
    })
}
fn exact_durations(waves: &[WaveCapture<AttemptMeta>]) -> Vec<Duration> {
    waves
        .iter()
        .flat_map(|w| w.attempts.iter())
        .filter_map(|a| match a.timing {
            TimingCapture::Exact(v) => Some(v),
            _ => None,
        })
        .collect()
}
fn complete_hashes(waves: &[WaveCapture<AttemptMeta>]) -> Vec<String> {
    waves
        .iter()
        .flat_map(|w| w.attempts.iter())
        .map(|a| match &a.outcome {
            AttemptOutcome::Response(r) => {
                assert_eq!(r.status, 200);
                assert_eq!(r.result, HttpBodyResult::Complete);
                r.metadata.body_sha256.clone()
            }
            _ => panic!("a performance sample did not complete"),
        })
        .collect()
}
#[derive(Serialize)]
struct SeriesArtifact {
    case: String,
    arm: String,
    // Raw captures retain statuses, body hashes, timing kinds and failures;
    // percentiles below are derived only from the measured Exact captures.
    serial_warmups: Vec<WaveCapture<AttemptMeta>>,
    serial: Vec<WaveCapture<AttemptMeta>>,
    concurrency_warmups: Vec<WaveCapture<AttemptMeta>>,
    concurrency: Vec<WaveCapture<AttemptMeta>>,
    serial_p95_ns: u128,
    concurrency_p95_ns: Option<u128>,
}

fn serial_p95(artifact: &SeriesArtifact) -> Duration {
    driver::nearest_rank_percentile(&exact_durations(&artifact.serial), 0.95)
        .expect("complete serial timing set")
}
fn concurrent_p95(artifact: &SeriesArtifact) -> Option<Duration> {
    let values = exact_durations(&artifact.concurrency);
    (!values.is_empty()).then(|| {
        driver::nearest_rank_percentile(&values, 0.95).expect("complete concurrent timing set")
    })
}
fn assert_complete_artifact(artifact: &SeriesArtifact, today: bool) {
    let serial = exact_durations(&artifact.serial);
    assert_eq!(
        serial.len(),
        SERIAL_SAMPLES,
        "all serial requests completed"
    );
    let concurrent = exact_durations(&artifact.concurrency);
    if today {
        assert_eq!(
            concurrent.len(),
            TODAY_SAMPLES_WAVES * TODAY_CONCURRENCY,
            "all concurrency-5 requests completed"
        );
    }
    // This inspects every recorded outcome after artifact write; no failed
    // response is silently omitted from a percentile or parity comparison.
    let _ = complete_hashes(&artifact.serial);
    let _ = complete_hashes(&artifact.concurrency);
}
fn relative_budget(old: Duration) -> Duration {
    // Preserve sub-millisecond precision. The approved paired allowance is
    // the larger of 10% of the frozen p95 and 25ms, never their sum.
    old + std::cmp::max(old.mul_f64(0.10), Duration::from_millis(25))
}
async fn series(
    ids: &mut AttemptIds,
    arm: QueryArm,
    session: Session,
    base_url: String,
    path: String,
    today: bool,
    name: String,
) -> SeriesArtifact {
    let call = Arc::new(move |_context: AttemptContext| {
        request(session.clone(), base_url.clone(), path.clone())
    });
    let serial_warmups = driver::run_serial(
        ids,
        arm,
        AttemptPhase::Warmup,
        0,
        SERIAL_WARMUPS,
        call.clone(),
    )
    .await
    .expect("serial warmups");
    let serial = driver::run_serial(
        ids,
        arm,
        AttemptPhase::Measured,
        SERIAL_WARMUPS,
        SERIAL_SAMPLES,
        call.clone(),
    )
    .await
    .expect("serial samples");
    let mut concurrency_warmups = Vec::new();
    let mut concurrency = Vec::new();
    if today {
        for wave in 0..TODAY_WARMUP_WAVES {
            concurrency_warmups.push(
                driver::run_wave(
                    ids,
                    arm,
                    AttemptPhase::Warmup,
                    1_000 + wave,
                    TODAY_CONCURRENCY,
                    call.clone(),
                )
                .await
                .expect("today warmup wave"),
            );
        }
        for wave in 0..TODAY_SAMPLES_WAVES {
            concurrency.push(
                driver::run_wave(
                    ids,
                    arm,
                    AttemptPhase::Measured,
                    2_000 + wave,
                    TODAY_CONCURRENCY,
                    call.clone(),
                )
                .await
                .expect("today measured wave"),
            );
        }
    }
    let serial_p95_ns = driver::nearest_rank_percentile(&exact_durations(&serial), 0.95)
        .expect("serial p95")
        .as_nanos();
    let concurrency_p95_ns = (!concurrency.is_empty()).then(|| {
        driver::nearest_rank_percentile(&exact_durations(&concurrency), 0.95)
            .expect("concurrent p95")
            .as_nanos()
    });
    SeriesArtifact {
        case: name,
        arm: match arm {
            QueryArm::FrozenOriginal => "frozen".to_owned(),
            QueryArm::Final => "live".to_owned(),
        },
        serial_warmups,
        serial,
        concurrency_warmups,
        concurrency,
        serial_p95_ns,
        concurrency_p95_ns,
    }
}

async fn clear_sources(session: &Session, base_url: &str, sources: &[SavedSource]) {
    // The actor configuration persists in the fixture database between arms.
    // Make zero-source cases explicit instead of inheriting prior case state.
    for source in sources {
        let response = session
            .client
            .delete(format!("{base_url}/api/today/sources/{}", source.id))
            .header(COOKIE, &session.cookie)
            .send()
            .await
            .expect("disable source");
        assert_eq!(response.status(), reqwest::StatusCode::OK, "disable source");
    }
}

const REQUIRED: [FilterStatementFamily; 14] = [
    FilterStatementFamily::SummaryCreatedDesc,
    FilterStatementFamily::SummaryCreatedAsc,
    FilterStatementFamily::SummaryNameAsc,
    FilterStatementFamily::SummaryNameDesc,
    FilterStatementFamily::SummaryStageAsc,
    FilterStatementFamily::SummaryStageDesc,
    FilterStatementFamily::SummaryAssigneeAsc,
    FilterStatementFamily::SummaryAssigneeDesc,
    FilterStatementFamily::Count,
    FilterStatementFamily::SourceMembership,
    FilterStatementFamily::SourceCandidates,
    FilterStatementFamily::PersonState,
    FilterStatementFamily::CallMembership,
    FilterStatementFamily::CallOnly,
];

async fn actual_preflight(
    migrator_pool: &PgPool,
    config: &Config,
    fixture: &Fixture,
    arm: QueryArm,
) -> (
    BTreeMap<FilterStatementFamily, usize>,
    BTreeMap<FilterStatementFamily, usize>,
) {
    // This is deliberately outside the timing protocol. It proves the current
    // router/auth flow reaches all fourteen live/frozen execution identities.
    let overrides = match arm {
        QueryArm::FrozenOriginal => FilterStatementOverrides::frozen(frozen::adapter()),
        QueryArm::Final => FilterStatementOverrides::live(),
    };
    let pool = crate::common::connect_as_app(migrator_pool).await;
    let server = Server::start(app(pool.clone(), config, fixture.now, overrides.clone())).await;
    let session = login(&server.base_url, &fixture.viewer_email, &fixture.password).await;
    clear_sources(&session, &server.base_url, &fixture.sources).await;
    let people = [
        "created.desc",
        "created.asc",
        "name.asc",
        "name.desc",
        "stage.asc",
        "stage.desc",
        "assignee.asc",
        "assignee.desc",
    ];
    for sort in people {
        let response = session
            .client
            .get(format!(
                "{}/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort={sort}",
                server.base_url
            ))
            .header(COOKIE, &session.cookie)
            .send()
            .await
            .expect("preflight people");
        assert_complete_response(
            response,
            &format!(
                "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort={sort}"
            ),
            "people",
        )
        .await;
    }
    let count = session
        .client
        .get(format!(
            "{}/api/saved-lists/{}/count?revision={}",
            server.base_url, fixture.sources[0].id, fixture.sources[0].revision
        ))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("preflight count");
    assert_complete_response(
        count,
        &format!(
            "/api/saved-lists/{}/count?revision={}",
            fixture.sources[0].id, fixture.sources[0].revision
        ),
        "saved-list count",
    )
    .await;
    let zero = session
        .client
        .get(format!("{}/api/today", server.base_url))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("preflight zero today");
    assert_complete_response(zero, "/api/today", "today zero sources").await;
    configure_sources(&session, &server.base_url, &fixture.sources).await;
    let five = session
        .client
        .get(format!("{}/api/today", server.base_url))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("preflight five today");
    assert_complete_response(five, "/api/today", "today five sources").await;
    let hits = overrides.hits();
    server.close().await;
    pool.close().await;
    match arm {
        QueryArm::FrozenOriginal => assert!(
            hits.0.is_empty(),
            "frozen preflight used live SQL: {:?}",
            hits.0
        ),
        QueryArm::Final => assert!(
            hits.1.is_empty(),
            "live preflight used frozen SQL: {:?}",
            hits.1
        ),
    };
    let actual = if arm == QueryArm::FrozenOriginal {
        &hits.1
    } else {
        &hits.0
    };
    assert!(
        REQUIRED
            .into_iter()
            .all(|family| actual.get(&family).copied().unwrap_or_default() > 0),
        "incomplete actual preflight: {actual:?}"
    );
    hits
}

async fn create_fixture(pool: &PgPool) -> Fixture {
    let now = Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap();
    let org = crate::common::create_org(pool, "Slice 019b performance primary").await;
    crate::common::seed_stages(pool, org).await;
    let password = "slice-019b-performance-password".to_owned();
    let viewer_email = "viewer.slice019b.perf@example.test".to_owned();
    let viewer =
        crate::common::create_user(pool, &viewer_email, "Slice 019b viewer", &password).await;
    crate::common::add_membership_with(
        pool,
        org,
        viewer,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    for ordinal in 1..50 {
        let email = format!("member-{ordinal}.slice019b.perf@example.test");
        let member =
            crate::common::create_user(pool, &email, "Fixture member", "fixture-member-password")
                .await;
        crate::common::add_membership_with(
            pool,
            org,
            member,
            crm_api::domain::admin::Role::Member,
            crm_api::domain::admin::MembershipStatus::Active,
        )
        .await;
    }
    let foreign_org = crate::common::create_org(pool, "Slice 019b performance foreign").await;
    crate::common::seed_stages(pool, foreign_org).await;
    let foreign_creator = crate::common::create_user(
        pool,
        "foreign-creator.slice019b.perf@example.test",
        "Foreign fixture creator",
        "fixture-member-password",
    )
    .await;
    crate::common::add_membership_with(
        pool,
        foreign_org,
        foreign_creator,
        crm_api::domain::admin::Role::Member,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let app = crate::common::connect_as_app(pool).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(&app)
    .await
    .expect("primary stage");
    sqlx::query("INSERT INTO person (organization_id,stage_id,assigned_user_id,first_name,last_name,created_at) SELECT $1,$2,$3,'Perf',gs::text,$4 FROM generate_series(1,25000) gs").bind(org).bind(stage).bind(viewer).bind(Utc.with_ymd_and_hms(2026,8,1,0,0,0).unwrap()).execute(&app).await.expect("seed 25k people");
    let foreign_stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(foreign_org)
    .fetch_one(&app)
    .await
    .expect("foreign stage");
    sqlx::query(
        "INSERT INTO person (organization_id,stage_id) SELECT $1,$2 FROM generate_series(1,100)",
    )
    .bind(foreign_org)
    .bind(foreign_stage)
    .execute(&app)
    .await
    .expect("foreign people");
    // Keep second-organization definition/value rows in the same cardinality
    // class so tenant predicates appear in live plans with foreign data.
    let foreign_field = Uuid::new_v4();
    sqlx::query("INSERT INTO custom_field (id,organization_id,label,field_type,position,created_by_user_id) VALUES ($1,$2,'Foreign performance field','text',0,$3)")
        .bind(foreign_field).bind(foreign_org).bind(foreign_creator).execute(&app).await.expect("foreign custom definition");
    sqlx::query("INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,text_value,origin,correlation_id) SELECT $1,id,$2,'text','foreign-value','migration',gen_random_uuid() FROM person WHERE organization_id=$1 LIMIT 100")
        .bind(foreign_org).bind(foreign_field).execute(&app).await.expect("foreign custom values");
    // Five distinct stage+tag definitions overlap substantially and each
    // retains more than 101 non-built-in candidates after the first prefix.
    let mut tags = Vec::new();
    for ordinal in 0..5_i64 {
        let tag: Uuid = sqlx::query_scalar(
            "INSERT INTO tag (organization_id,name,created_by_user_id) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind(org)
        .bind(format!("Performance overlap {ordinal}"))
        .bind(viewer)
        .fetch_one(&app)
        .await
        .expect("performance tag");
        sqlx::query(
            "INSERT INTO person_tag (organization_id,person_id,tag_id,added_by_user_id) \
             SELECT $1,id,$2,$3 FROM person WHERE organization_id=$1 \
             ORDER BY id OFFSET $4 LIMIT 1_500",
        )
        .bind(org)
        .bind(tag)
        .bind(viewer)
        .bind(300_i64 + ordinal * 100)
        .execute(&app)
        .await
        .expect("overlapping performance tags");
        tags.push(tag);
    }
    let fields = (0..50).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
    // The first five definitions provide the all-slot live EXPLAIN matrix;
    // the remaining 45 preserve the 50-definition production shape.
    for (position, field) in fields.iter().enumerate() {
        let kind = if position < 5 {
            ["text", "text", "number", "date", "choice"][position]
        } else {
            ["text", "number", "date", "choice"][position % 4]
        };
        sqlx::query("INSERT INTO custom_field (id,organization_id,label,field_type,position,created_by_user_id) VALUES ($1,$2,$3,$4,$5,$6)").bind(field).bind(org).bind(format!("Performance field {position}")).bind(kind).bind(position as i32).bind(viewer).execute(&app).await.expect("custom definition");
    }
    let option = Uuid::new_v4();
    sqlx::query("INSERT INTO custom_field_option (id,organization_id,field_id,label,position) VALUES ($1,$2,$3,'Warm',0)").bind(option).bind(org).bind(fields[4]).execute(&app).await.expect("custom option");
    for (field, kind) in fields
        .iter()
        .take(5)
        .zip(["text", "text", "number", "date", "choice"])
    {
        let text = match kind {
            // Field 0 gets a selective positive; field 1 has real negative
            // values so `not_contains` is exercised rather than a null slot.
            "text" if *field == fields[1] => "INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,text_value,origin,correlation_id) SELECT $1,id,$2,'text',CASE WHEN substring(id::text,1,1) IN ('0','1') THEN 'acceptable-value' ELSE 'excluded-value' END,'migration',gen_random_uuid() FROM person WHERE organization_id=$1",
            "text" => "INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,text_value,origin,correlation_id) SELECT $1,id,$2,'text',CASE WHEN substring(id::text,1,1) IN ('0','1') THEN 'selective-value' ELSE 'historical-value' END,'migration',gen_random_uuid() FROM person WHERE organization_id=$1",
            "number" => "INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,number_value,origin,correlation_id) SELECT $1,id,$2,'number',CASE WHEN substring(id::text,1,1) IN ('0','1') THEN 500000 ELSE 120000 END,'migration',gen_random_uuid() FROM person WHERE organization_id=$1",
            "date" => "INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,date_value,origin,correlation_id) SELECT $1,id,$2,'date',CASE WHEN substring(id::text,1,1) IN ('0','1') THEN '2020-01-01'::date ELSE '2010-01-01'::date END,'migration',gen_random_uuid() FROM person WHERE organization_id=$1",
            "choice" => "INSERT INTO person_custom_field_value (organization_id,person_id,field_id,field_type,option_id,origin,correlation_id) SELECT $1,id,$2,'choice',$3,'migration',gen_random_uuid() FROM person WHERE organization_id=$1",
            _ => unreachable!(),
        };
        let mut query = sqlx::query(text).bind(org).bind(field);
        if kind == "choice" {
            query = query.bind(option);
        }
        query.execute(&app).await.expect("custom values");
    }
    let mut sources = Vec::new();
    for (ordinal, tag) in tags.into_iter().enumerate() {
        let filter: crm_api::domain::person::filter::FilterDefinition =
            serde_json::from_value(json!({
                "version":1,
                "clauses":[
                    {"kind":"stage","stage_ids":[stage.to_string()]},
                    {"kind":"tags","tag_ids":[tag.to_string()]}
                ]
            }))
            .expect("overlapping stage/tag list filter");
        let name = format!("Slice 019b performance source {ordinal}");
        let outcome = crate::common::saved_lists::create_list(
            &app,
            org,
            viewer,
            Uuid::new_v4(),
            crm_api::domain::saved_list::SavedListScope::Personal,
            &name,
            filter,
        )
        .await;
        sources.push(SavedSource {
            id: outcome.list.id.as_uuid(),
            revision: outcome.list.revision,
        });
    }
    let people: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 ORDER BY id LIMIT 100")
            .bind(org)
            .fetch_all(&app)
            .await
            .expect("fixture people");
    // Deterministic Today book: 40 inquiry-only, 40 reply-only, 10 both,
    // and 10 call-only. Calls are inserted before a later fresh inquiry so
    // their generated contact root remains older than that inquiry.
    for person in people.iter().take(5).chain(people.iter().skip(90).take(10)) {
        crate::common::today_system_feed::insert_call(
            &app,
            org,
            *person,
            viewer,
            now - chrono::Duration::hours(3),
        )
        .await;
    }
    for person in people
        .iter()
        .skip(40)
        .take(40)
        .chain(people.iter().skip(90).take(10))
    {
        sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,received_at) VALUES ($1,$2,gen_random_uuid(),'slice019b_perf',$3)")
            .bind(org).bind(person).bind(now - chrono::Duration::days(30)).execute(&app).await.expect("old inquiry");
    }
    for person in people.iter().skip(40).take(50) {
        crate::common::today_system_feed::insert_correspondence(
            &app,
            org,
            *person,
            viewer,
            "outbound",
            now - chrono::Duration::days(2),
        )
        .await;
        crate::common::today_system_feed::insert_correspondence(
            &app,
            org,
            *person,
            viewer,
            "inbound",
            now - chrono::Duration::hours(1),
        )
        .await;
    }
    // Correspondence updates directional timestamps but not `last_contact`.
    // These forty reply-only People need a historical contact fact so the
    // unanswered-inquiry arm cannot also match them.
    for person in people.iter().skip(40).take(40) {
        sqlx::query(
            "INSERT INTO contact_attempted \
             (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, channel, outcome) \
             VALUES ($1, 'system', 'migration', $2, gen_random_uuid(), $3, 'email', 'reached')",
        )
        .bind(org)
        .bind(now - chrono::Duration::days(2))
        .bind(person)
        .execute(&app)
        .await
        .expect("reply-only historical contact");
    }
    for person in people
        .iter()
        .take(40)
        .chain(people.iter().skip(80).take(10))
    {
        sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,received_at) VALUES ($1,$2,gen_random_uuid(),'slice019b_perf',$3)")
            .bind(org).bind(person).bind(now - chrono::Duration::hours(2)).execute(&app).await.expect("fresh inquiry");
    }
    let builtin_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person WHERE organization_id = $1 AND id = ANY($2::uuid[])",
    )
    .bind(org)
    .bind(&people)
    .fetch_one(&app)
    .await
    .expect("verify built-in candidate envelope");
    assert_eq!(
        builtin_count, 100,
        "fixture retains the 100 built-in candidates"
    );
    assert_eq!(sources.len(), 5, "fixture retains five overlapping sources");
    let shape = format!("people={PEOPLE};members=50;definitions=50;values_per_person=5;builtins=100;call_only=10;source_candidates=100");
    Fixture {
        organization_id: org,
        viewer_id: viewer,
        viewer_email,
        password,
        sources,
        custom_fields: live_explain::ExplainFieldIds {
            fields: [fields[0], fields[1], fields[2], fields[3], fields[4]],
            choice_option: option,
        },
        now,
        manifest: FixtureManifest {
            people: PEOPLE,
            members: 50,
            definitions: 50,
            values_per_person: 5,
            builtins: 100,
            call_only: 10,
            source_candidates: 100,
            shape_sha256: sha2::Sha256::digest(shape.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
    }
}

#[derive(Serialize)]
struct PairArtifact {
    frozen: SeriesArtifact,
    live: SeriesArtifact,
}

#[derive(Serialize)]
struct ComparisonArtifact {
    case: String,
    serial_old_p95_ns: u128,
    serial_new_p95_ns: u128,
    serial_budget_ns: u128,
    serial_hashes_equal: bool,
    concurrency_old_p95_ns: Option<u128>,
    concurrency_new_p95_ns: Option<u128>,
    concurrency_budget_ns: Option<u128>,
    concurrency_hashes_equal: bool,
}

fn completed_hashes(waves: &[WaveCapture<AttemptMeta>]) -> Option<Vec<String>> {
    waves
        .iter()
        .flat_map(|wave| wave.attempts.iter())
        .map(|attempt| match &attempt.outcome {
            AttemptOutcome::Response(response)
                if response.status == 200 && response.result == HttpBodyResult::Complete =>
            {
                Some(response.metadata.body_sha256.clone())
            }
            _ => None,
        })
        .collect()
}

#[sqlx::test]
#[ignore = "opt-in Slice 019b paired authenticated performance evidence"]
async fn slice_019b_authenticated_request_performance(migrator_pool: PgPool) {
    let fixture = create_fixture(&migrator_pool).await;
    let config = crate::common::test_config();
    let output = std::path::PathBuf::from(
        std::env::var_os("CRM_019B_PERF_OUTPUT").expect("CRM_019B_PERF_OUTPUT"),
    );
    std::fs::create_dir_all(&output).expect("artifact directory");

    // First prove the actual fixed-arm router execution is complete. The
    // preflight is intentionally untimed and has separate counters, so a
    // long measured loop can never manufacture coverage evidence afterwards.
    let frozen_preflight =
        actual_preflight(&migrator_pool, &config, &fixture, QueryArm::FrozenOriginal).await;
    let live_preflight = actual_preflight(&migrator_pool, &config, &fixture, QueryArm::Final).await;

    let plans = live_explain::capture_live_statement_plans(
        &migrator_pool,
        fixture.organization_id,
        fixture.viewer_id,
        fixture.now,
        fixture.custom_fields,
    )
    .await
    .expect("capture current live EXPLAIN plans");
    std::fs::write(
        output.join("plans.json"),
        serde_json::to_vec_pretty(&plans).expect("serialize plans"),
    )
    .expect("write plans");
    assert_eq!(
        plans.len(),
        18,
        "14 matrices plus 4 bounded custom-field reads"
    );

    let cases = [
        ("people_created_desc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=created.desc", false, false),
        ("people_created_asc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=created.asc", false, false),
        ("people_name_asc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=name.asc", false, false),
        ("people_name_desc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=name.desc", false, false),
        ("people_stage_asc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=stage.asc", false, false),
        ("people_stage_desc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=stage.desc", false, false),
        ("people_assignee_asc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=assignee.asc", false, false),
        ("people_assignee_desc", "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=assignee.desc", false, false),
        ("saved_list_count", "", false, false),
        ("today_zero_sources", "/api/today", true, false),
        ("today_five_sources", "/api/today", true, true),
    ];
    let mut ids = AttemptIds::default();
    let mut artifacts = Vec::<PairArtifact>::new();
    let mut measured_cross_arm = Vec::<(String, bool, bool)>::new();

    for (index, (name, path, today, five_sources)) in cases.into_iter().enumerate() {
        let arms = if index % 2 == 0 {
            [QueryArm::FrozenOriginal, QueryArm::Final]
        } else {
            [QueryArm::Final, QueryArm::FrozenOriginal]
        };
        let mut pair = BTreeMap::new();
        for arm in arms {
            let overrides = match arm {
                QueryArm::FrozenOriginal => FilterStatementOverrides::frozen(frozen::adapter()),
                QueryArm::Final => FilterStatementOverrides::live(),
            };
            let pool = crate::common::connect_as_app(&migrator_pool).await;
            let server =
                Server::start(app(pool.clone(), &config, fixture.now, overrides.clone())).await;
            let session = login(&server.base_url, &fixture.viewer_email, &fixture.password).await;
            if five_sources {
                configure_sources(&session, &server.base_url, &fixture.sources).await;
            } else {
                clear_sources(&session, &server.base_url, &fixture.sources).await;
            }
            let request_path = if name == "saved_list_count" {
                format!(
                    "/api/saved-lists/{}/count?revision={}",
                    fixture.sources[0].id, fixture.sources[0].revision
                )
            } else {
                path.to_owned()
            };
            let evidence = series(
                &mut ids,
                arm,
                session,
                server.base_url.clone(),
                request_path,
                today,
                name.to_owned(),
            )
            .await;
            let hits = overrides.hits();
            measured_cross_arm.push((
                format!("{name}:{}", evidence.arm),
                !hits.0.is_empty(),
                !hits.1.is_empty(),
            ));
            pair.insert(evidence.arm.clone(), evidence);
            server.close().await;
            pool.close().await;
        }
        artifacts.push(PairArtifact {
            frozen: pair.remove("frozen").expect("frozen pair"),
            live: pair.remove("live").expect("live pair"),
        });
    }

    // Retain complete raw records (including failures and timing kinds) and
    // plans/protocol before evaluating any parity or relative p95 assertion.
    let protocol = json!({
        "protocol":"slice-019b-authenticated-request-v1",
        "fixture": fixture.manifest,
        "serial":{"warmups":SERIAL_WARMUPS,"samples":SERIAL_SAMPLES},
        "today":{"concurrency":TODAY_CONCURRENCY,"warmup_waves":TODAY_WARMUP_WAVES,"measured_waves":TODAY_SAMPLES_WAVES},
        "preflight":{"frozen_families":frozen_preflight.1.len(),"live_families":live_preflight.0.len()}
    });
    std::fs::write(
        output.join("protocol.json"),
        serde_json::to_vec_pretty(&protocol).expect("serialize protocol"),
    )
    .expect("write protocol");
    std::fs::write(
        output.join("raw-timings.json"),
        serde_json::to_vec_pretty(&artifacts).expect("serialize retained timing records"),
    )
    .expect("write retained timing records");
    std::fs::write(
        output.join("measured-cross-arm.json"),
        serde_json::to_vec_pretty(&measured_cross_arm).expect("serialize arm checks"),
    )
    .expect("write arm checks");

    let comparisons = cases
        .iter()
        .zip(&artifacts)
        .map(|((name, _, _, _), pair)| {
            let serial_old = serial_p95(&pair.frozen);
            let serial_new = serial_p95(&pair.live);
            let concurrency_old = concurrent_p95(&pair.frozen);
            let concurrency_new = concurrent_p95(&pair.live);
            ComparisonArtifact {
                case: (*name).to_owned(),
                serial_old_p95_ns: serial_old.as_nanos(),
                serial_new_p95_ns: serial_new.as_nanos(),
                serial_budget_ns: relative_budget(serial_old).as_nanos(),
                serial_hashes_equal: completed_hashes(&pair.frozen.serial)
                    == completed_hashes(&pair.live.serial),
                concurrency_old_p95_ns: concurrency_old.map(|value| value.as_nanos()),
                concurrency_new_p95_ns: concurrency_new.map(|value| value.as_nanos()),
                concurrency_budget_ns: concurrency_old
                    .map(relative_budget)
                    .map(|value| value.as_nanos()),
                concurrency_hashes_equal: completed_hashes(&pair.frozen.concurrency)
                    == completed_hashes(&pair.live.concurrency),
            }
        })
        .collect::<Vec<_>>();
    std::fs::write(
        output.join("comparisons.json"),
        serde_json::to_vec_pretty(&comparisons).expect("serialize comparisons"),
    )
    .expect("write comparisons");

    // Only after every pair and all raw artifacts are durable do acceptance
    // gates inspect statuses/hashes or apply paired relative p95 budgets.
    for ((name, _, today, _), pair) in cases.into_iter().zip(&artifacts) {
        assert_complete_artifact(&pair.frozen, today);
        assert_complete_artifact(&pair.live, today);
        assert_eq!(
            complete_hashes(&pair.frozen.serial),
            complete_hashes(&pair.live.serial),
            "serial payload parity for {name}"
        );
        assert_eq!(
            complete_hashes(&pair.frozen.concurrency),
            complete_hashes(&pair.live.concurrency),
            "concurrency payload parity for {name}"
        );
        let old = serial_p95(&pair.frozen);
        let new = serial_p95(&pair.live);
        assert!(
            new <= relative_budget(old),
            "serial p95 regression for {name}: {new:?} > {:?}",
            relative_budget(old)
        );
        if let (Some(old), Some(new)) = (concurrent_p95(&pair.frozen), concurrent_p95(&pair.live)) {
            assert!(
                new <= relative_budget(old),
                "concurrency-5 p95 regression for {name}: {new:?} > {:?}",
                relative_budget(old)
            );
        }
    }
    for (case, saw_live, saw_frozen) in measured_cross_arm {
        if case.ends_with(":frozen") {
            assert!(
                !saw_live && saw_frozen,
                "frozen measured arm execution identity: {case}"
            );
        } else {
            assert!(
                saw_live && !saw_frozen,
                "live measured arm execution identity: {case}"
            );
        }
    }
}

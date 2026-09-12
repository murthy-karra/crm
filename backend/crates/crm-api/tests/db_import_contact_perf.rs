//! Opt-in Slice 010c paired authenticated HTTP performance harness.
//!
//! Both applications use the current request/auth/router/workspace guard stack.
//! The frozen c6c5930 arm differs only through the existing task-local SQL adapter.
//! Task-only fallback is current in both arms; its independent correctness/plan
//! gates belong to the primary implementation tests. No source/provider runs.
//!
//! Run only with an explicitly isolated SQLx test master (never crm_dev),
//! CRM_DB_APP_PASSWORD loaded privately and absolute CRM_010C_PERF_OUTPUT.
//! cargo test -p crm-api --test all --features perf-harness --locked
//! db_import_contact_perf::slice_010c_contact_order_performance
//! -- --ignored --exact --nocapture --test-threads=1

// These paired readers use only the serial subset of the shared Today driver.
#[allow(dead_code)]
#[path = "fixtures/today_http_perf_driver.rs"]
pub(crate) mod driver;
#[path = "fixtures/import_contact_c6c5930/mod.rs"]
mod frozen;

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
    viewer_email: String,
    password: String,
    sources: Vec<SavedSource>,
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
    contacts_per_person: usize,
    shared_phone_members: i64,
    task_only: usize,
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
            });
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
            });
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
    serial_p95_ns: Option<u128>,
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
    // Preserve an all-timeout/failure series long enough to write its raw
    // artifact; the acceptance gate rejects missing complete measurements.
    let serial_p95_ns = driver::nearest_rank_percentile(&exact_durations(&serial), 0.95)
        .map(|duration| duration.as_nanos());
    let concurrency_p95_ns = driver::nearest_rank_percentile(&exact_durations(&concurrency), 0.95)
        .map(|duration| duration.as_nanos());
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

const REQUIRED: [FilterStatementFamily; 6] = [
    FilterStatementFamily::SummaryCreatedDesc,
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
    // router/auth flow reaches all six paired live/frozen execution identities.
    let overrides = match arm {
        QueryArm::FrozenOriginal => FilterStatementOverrides::frozen(frozen::adapter()),
        QueryArm::Final => FilterStatementOverrides::live(),
    };
    let pool = crate::common::connect_as_app(migrator_pool).await;
    let server = Server::start(app(pool.clone(), config, fixture.now, overrides.clone())).await;
    let session = login(&server.base_url, &fixture.viewer_email, &fixture.password).await;
    clear_sources(&session, &server.base_url, &fixture.sources).await;
    for sort in ["created.desc"] {
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
    let zero = session
        .client
        .get(format!("{}/api/today", server.base_url))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("preflight zero today");
    let status = zero.status().as_u16();
    let zero: Value = zero.json().await.expect("zero-source preflight body");
    assert_eq!(
        classify_today_body(status, Some(&zero)),
        HttpBodyResult::Complete
    );
    let due_only = zero["items"]
        .as_array()
        .expect("Today items")
        .iter()
        .filter(|item| {
            item["latest_inquiry"].is_null()
                && item["reasons"].as_array().is_some_and(|reasons| {
                    reasons
                        .iter()
                        .any(|reason| reason["code"] == "task_overdue")
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        due_only.len(),
        10,
        "all ten current task-only fallback rows participate"
    );
    assert!(due_only
        .iter()
        .all(|item| item["person"]["primary_phone"] == "+15550000000"));
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
    let org = crate::common::create_org(pool, "Slice 010c performance primary").await;
    crate::common::seed_stages(pool, org).await;
    let password = "slice-010c-performance-password".to_owned();
    let viewer_email = "viewer.slice010c.perf@example.test".to_owned();
    let viewer =
        crate::common::create_user(pool, &viewer_email, "Slice 010c viewer", &password).await;
    crate::common::add_membership_with(
        pool,
        org,
        viewer,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    for ordinal in 1..50 {
        let email = format!("member-{ordinal}.slice010c.perf@example.test");
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
    let foreign_org = crate::common::create_org(pool, "Slice 010c performance foreign").await;
    crate::common::seed_stages(pool, foreign_org).await;
    let foreign_creator = crate::common::create_user(
        pool,
        "foreign-creator.slice010c.perf@example.test",
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
    // Synthetic scale rows are fixture setup, never an import fidelity claim.
    // Four contacts per Person: unique first email, shared second email,
    // shared first phone and unique second phone. NULL import_order plus strict
    // timestamps make every baseline/final primary-contact selection identical.
    sqlx::query(
        r#"INSERT INTO contact_method
        (organization_id,person_id,kind,value,normalized_value,created_at)
        SELECT p.organization_id,p.id,c.kind,c.value,c.value,$2 + c.ordinal * interval '1 second'
        FROM person p CROSS JOIN LATERAL (VALUES
          ('email','first-' || p.id::text || '@example.test',1),
          ('email','shared-contact@example.test',2),
          ('phone','+15550000000',3),
          ('phone','+1555' || lpad(p.last_name,7,'0'),4)
        ) c(kind,value,ordinal) WHERE p.organization_id=$1"#,
    )
    .bind(org)
    .bind(now - chrono::Duration::days(40))
    .execute(&app)
    .await
    .expect("100k ordinary contacts");
    // Foreign contacts share the lookup key, exposing any missing tenant bound.
    sqlx::query("INSERT INTO contact_method (organization_id,person_id,kind,value,normalized_value) SELECT $1,id,'phone','+15550000000','+15550000000' FROM person WHERE organization_id=$1")
        .bind(foreign_org).execute(&app).await.expect("foreign shared contacts");
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
        let name = format!("Slice 010c performance source {ordinal}");
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
        sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,received_at) VALUES ($1,$2,gen_random_uuid(),'slice010c_perf',$3)")
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
        sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,received_at) VALUES ($1,$2,gen_random_uuid(),'slice010c_perf',$3)")
            .bind(org).bind(person).bind(now - chrono::Duration::hours(2)).execute(&app).await.expect("fresh inquiry");
    }
    sqlx::query(r#"INSERT INTO task
        (organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id)
        SELECT $1,id,'Synthetic due task','follow_up',$3,$2,$2,'migration',gen_random_uuid()
        FROM person WHERE organization_id=$1 ORDER BY id OFFSET 200 LIMIT 10"#)
        .bind(org).bind(viewer).bind(now - chrono::Duration::hours(1))
        .execute(&app).await.expect("ten due-task-only fixture rows");
    let contact_shape: (i64,i64,i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE import_order IS NOT NULL),count(*) FILTER (WHERE kind='phone' AND normalized_value='+15550000000') FROM contact_method WHERE organization_id=$1")
        .bind(org).fetch_one(&app).await.expect("contact fixture shape");
    assert_eq!(contact_shape, (PEOPLE * 4, 0, PEOPLE));
    for table in ["person", "contact_method", "person_tag", "inquiry", "task"] {
        // The names are a fixed fixture-only allowlist, never input or model SQL.
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(pool)
            .await
            .expect("analyze isolated synthetic fixture");
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
    let shape = format!(
        "people={PEOPLE};members=50;contacts_per_person=4;shared_phone_members=25000;task_only=10;builtins=100;call_only=10;source_candidates=100"
    );
    app.close().await;
    Fixture {
        organization_id: org,
        viewer_email,
        password,
        sources,
        now,
        manifest: FixtureManifest {
            people: PEOPLE,
            members: 50,
            contacts_per_person: 4,
            shared_phone_members: PEOPLE,
            task_only: 10,
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

fn verify_baseline_manifest() {
    let manifest: Value = serde_json::from_str(include_str!(
        "fixtures/import_contact_c6c5930/manifest.json"
    ))
    .expect("frozen baseline manifest");
    assert!(manifest["baseline_commit"]
        .as_str()
        .unwrap()
        .starts_with("c6c5930"));
    let statements = [
        (
            "sql/call_membership.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/call_membership.sql").as_slice(),
        ),
        (
            "sql/call_only.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/call_only.sql").as_slice(),
        ),
        (
            "sql/count_filtered_matches.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/count_filtered_matches.sql")
                .as_slice(),
        ),
        (
            "sql/filtered_summaries.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/filtered_summaries.sql").as_slice(),
        ),
        (
            "sql/filtered_summaries_assignee_asc.sql",
            include_bytes!(
                "fixtures/import_contact_c6c5930/sql/filtered_summaries_assignee_asc.sql"
            )
            .as_slice(),
        ),
        (
            "sql/filtered_summaries_assignee_desc.sql",
            include_bytes!(
                "fixtures/import_contact_c6c5930/sql/filtered_summaries_assignee_desc.sql"
            )
            .as_slice(),
        ),
        (
            "sql/filtered_summaries_created_asc.sql",
            include_bytes!(
                "fixtures/import_contact_c6c5930/sql/filtered_summaries_created_asc.sql"
            )
            .as_slice(),
        ),
        (
            "sql/filtered_summaries_name_asc.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/filtered_summaries_name_asc.sql")
                .as_slice(),
        ),
        (
            "sql/filtered_summaries_name_desc.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/filtered_summaries_name_desc.sql")
                .as_slice(),
        ),
        (
            "sql/filtered_summaries_stage_asc.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/filtered_summaries_stage_asc.sql")
                .as_slice(),
        ),
        (
            "sql/filtered_summaries_stage_desc.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/filtered_summaries_stage_desc.sql")
                .as_slice(),
        ),
        (
            "sql/person_state.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/person_state.sql").as_slice(),
        ),
        (
            "sql/source_candidates.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/source_candidates.sql").as_slice(),
        ),
        (
            "sql/source_membership.sql",
            include_bytes!("fixtures/import_contact_c6c5930/sql/source_membership.sql").as_slice(),
        ),
    ];
    let records = manifest["statements"]
        .as_array()
        .expect("statement records");
    assert_eq!(records.len(), statements.len());
    for (path, bytes) in statements {
        let record = records
            .iter()
            .find(|record| record["file"] == path)
            .expect("frozen statement record");
        assert_eq!(record["bytes"], bytes.len());
        assert_eq!(
            record["sha256"],
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "baseline SQL bytes: {path}"
        );
    }
}

#[test]
fn frozen_contact_sql_matches_c6c5930_manifest() {
    verify_baseline_manifest();
}

const PEOPLE_PATH: &str =
    "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=created.desc";

async fn preflight_people(session: &Session, server: &Server) -> Value {
    let response = session
        .client
        .get(format!("{}{PEOPLE_PATH}", server.base_url))
        .header(COOKIE, &session.cookie)
        .send()
        .await
        .expect("negative-gate request");
    let status = response.status().as_u16();
    let value: Value = response.json().await.expect("negative-gate JSON");
    assert_eq!(
        classify_response(PEOPLE_PATH, status, Some(&value)),
        HttpBodyResult::Complete
    );
    value
}

async fn negative_contact_order_gate(pool: &PgPool, config: &Config, fixture: &Fixture) {
    let app_pool = crate::common::connect_as_app(pool).await;
    let frozen_server = Server::start(app(
        app_pool.clone(),
        config,
        fixture.now,
        FilterStatementOverrides::frozen(frozen::adapter()),
    ))
    .await;
    let live_server = Server::start(app(
        app_pool.clone(),
        config,
        fixture.now,
        FilterStatementOverrides::live(),
    ))
    .await;
    let frozen_session = login(
        &frozen_server.base_url,
        &fixture.viewer_email,
        &fixture.password,
    )
    .await;
    let live_session = login(
        &live_server.base_url,
        &fixture.viewer_email,
        &fixture.password,
    )
    .await;
    let original = preflight_people(&frozen_session, &frozen_server).await;
    assert_eq!(
        original,
        preflight_people(&live_session, &live_server).await,
        "ordinary contact full-response parity before negative gate"
    );
    let person: Uuid = original["people"][0]["id"]
        .as_str()
        .expect("visible fixture Person")
        .parse()
        .unwrap();
    let scope: i64 =
        sqlx::query_scalar("SELECT count(*) FROM person WHERE id=$1 AND organization_id=$2")
            .bind(person)
            .bind(fixture.organization_id)
            .fetch_one(pool)
            .await
            .expect("negative fixture scope");
    assert_eq!(scope, 1);
    // Deliberately use the new imported priority once, outside all measured
    // requests. If both arms silently execute live SQL this gate must fail.
    sqlx::query("UPDATE contact_method c SET import_order=(SELECT count(*) FROM contact_method c2 WHERE c2.person_id=c.person_id AND c2.kind=c.kind AND c2.created_at>c.created_at) WHERE c.organization_id=$1 AND c.person_id=$2 AND c.kind='email'")
        .bind(fixture.organization_id).bind(person).execute(pool).await.expect("negative-gate imported order");
    let frozen_reordered = preflight_people(&frozen_session, &frozen_server).await;
    let live_reordered = preflight_people(&live_session, &live_server).await;
    sqlx::query(
        "UPDATE contact_method SET import_order=NULL WHERE organization_id=$1 AND person_id=$2",
    )
    .bind(fixture.organization_id)
    .bind(person)
    .execute(pool)
    .await
    .expect("restore ordinary contacts before timing");
    let restored = preflight_people(&live_session, &live_server).await;
    frozen_server.close().await;
    live_server.close().await;
    app_pool.close().await;
    assert_eq!(
        frozen_reordered, original,
        "frozen SQL must ignore the new import order"
    );
    assert_ne!(
        live_reordered, original,
        "negative gate must detect the intended reader difference"
    );
    assert_eq!(
        live_reordered["people"][0]["primary_email"],
        "shared-contact@example.test"
    );
    assert_eq!(
        restored, original,
        "ordinary-contact baseline restored before all samples"
    );
}

#[sqlx::test]
#[ignore = "opt-in Slice 010c paired authenticated performance evidence"]
async fn slice_010c_contact_order_performance(migrator_pool: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var_os("CRM_010C_PERF_OUTPUT").expect("CRM_010C_PERF_OUTPUT"),
    );
    assert!(
        output.is_absolute(),
        "private absolute artifact path required"
    );
    verify_baseline_manifest();
    std::fs::create_dir_all(&output).expect("artifact directory");
    let fixture = create_fixture(&migrator_pool).await;
    let config = crate::common::test_config();
    negative_contact_order_gate(&migrator_pool, &config, &fixture).await;

    // First prove the actual fixed-arm router execution is complete. The
    // preflight is intentionally untimed and has separate counters, so a
    // long measured loop can never manufacture coverage evidence afterwards.
    let frozen_preflight =
        actual_preflight(&migrator_pool, &config, &fixture, QueryArm::FrozenOriginal).await;
    let live_preflight = actual_preflight(&migrator_pool, &config, &fixture, QueryArm::Final).await;

    let cases = [
        (
            "people_created_desc",
            "/api/people?filter=%7B%22version%22%3A1%2C%22clauses%22%3A%5B%5D%7D&sort=created.desc",
            false,
            false,
        ),
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
            let request_path = path.to_owned();
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
    // protocol before evaluating any parity or relative p95 assertion.
    let protocol = json!({
        "protocol":"slice-010c-contact-reader-pair-v1",
        "baseline": serde_json::from_str::<Value>(include_str!("fixtures/import_contact_c6c5930/manifest.json")).expect("baseline manifest"),
        "scope":"Reader-only comparison: workspace guard and task-only fallback shared by both arms; other changed reader correctness/plans are separately verified",
        "negative_gate":"Reversed import_order changed the live primary email, frozen baseline stayed stable; NULL restored before timing",
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

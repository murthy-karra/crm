//! Slice 011d §8 Phase B performance evidence. Reuses the Slice 011c
//! Phase B fixture (`today_http_perf_fixture.rs`, unmodified — the same
//! 50,000-Person history-rich database, the same four viewer books, the
//! same five source definitions) and driver primitives
//! (`today_http_perf_driver.rs` — `run_wave`/`run_serial`,
//! `aggregate_attempts`, nearest-rank percentiles, the 1,250/2,500/4,500 ms
//! request caps and 450 ms whole-source cap) — never a reduced fixture or
//! relaxed budget.
//!
//! This file adds its own lighter-weight dispatch/evidence types rather
//! than the 011c archive's full telemetry-coverage/sentinel apparatus
//! (disclosed as a deliberate scope reduction in the archive README): the
//! 011d regression claim rests on `Legacy` still being the exact same
//! compiled-in statement it always was (proven separately by the
//! byte-identical equivalence suite in `db_today_feed_equivalence.rs`),
//! not on reconstructing a frozen historical module.
//!
//! Four parts, run sequentially against the SAME seeded database:
//! (1) STEP 6 UPDATE: the paired zero-source `Legacy`-vs-`Feeds`
//!     regression is no longer runnable live — `TodayProvider::Legacy`
//!     and its seam (`router_with_test_clock_and_provider`,
//!     `query_owned_at_with_provider`) were deleted once the DB-level
//!     equivalence suite proved `Legacy` byte-identical to the frozen
//!     `tests/fixtures/today_f51bff8/` fixture. `regression` below is now
//!     a hardcoded carry-forward of the ONE comparison actually captured
//!     (docs/design/perf/slice-011d-2026-09-07/run.json), not a live run;
//! (2) the 011c matrix at concurrency 1/10/20 (four concentrated source
//!     modes, three supplemental books at zero/five sources, and the
//!     independent five-source concurrency-20 repeat) against `Feeds`;
//! (3) three new 011d cases (feed A customized with a stage clause, feed A
//!     disabled, feed C disabled) plus preview p95, all serial (cap
//!     1,250 ms) on the concentrated book;
//! (4) EXPLAIN (ANALYZE, BUFFERS) of `person_state.sql` on the vacuumed
//!     concentrated book, with and without `SET LOCAL enable_mergejoin =
//!     off`.

#[path = "common/mod.rs"]
mod common;

#[path = "fixtures/statements_b45b04f/mod.rs"]
mod frozen;

#[path = "fixtures/today_http_perf_driver.rs"]
mod today_http_perf_driver;

#[path = "fixtures/today_http_perf_fixture.rs"]
mod today_http_perf_fixture;

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::today::system_feeds::commands::{
    self, PreviewTodaySystemFeed, SetTodaySystemFeedEnabled, UpdateTodaySystemFeed,
};
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::domain::today::test_support::{
    HttpPerfCollector, HttpPerfTelemetry, PoolAcquisitionOutcome,
};
use crm_api::ids::{CorrelationId, OrganizationId, StageId, UserId};
use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use reqwest::header::{COOKIE, SET_COOKIE};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use today_http_perf_driver::{
    run_serial, run_wave, AttemptContext, AttemptIds, AttemptOutcome, AttemptPhase, ClientFailure,
    ClientFailureKind, CompletedHttpResponse, HttpBodyResult, QueryArm, TimingCapture, WaveCapture,
    CRITICAL_SAMPLE_SHAPE, INDEPENDENT_REPEAT_CONCURRENCY, INDEPENDENT_REPEAT_WAVES,
    REQUEST_P95_CONCURRENCY_1, REQUEST_P95_CONCURRENCY_10, REQUEST_P95_CONCURRENCY_20,
    SUPPLEMENTAL_SAMPLE_SHAPE, WARMUP_CONCURRENCY, WARMUP_WAVES, WHOLE_SOURCE_P95_LIMIT,
};
use today_http_perf_fixture::{PerfSourceMode, TodayHttpPerfCase, TodayHttpPerfFixture};

type DispatchFuture = Pin<
    Box<
        dyn std::future::Future<Output = Result<CompletedHttpResponse<Meta>, ClientFailure<Meta>>>
            + Send,
    >,
>;

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

// --- Minimal loopback server / auth (mirrors db_today_http_perf.rs) --------

struct LoopbackServer {
    base_url: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl LoopbackServer {
    async fn start(app: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind isolated loopback listener");
        let address: SocketAddr = listener
            .local_addr()
            .expect("read loopback listener address");
        let (shutdown, shutdown_signal) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_signal.await;
                })
                .await
                .expect("isolated loopback application serves until shutdown");
        });
        Self {
            base_url: format!("http://{address}"),
            shutdown: Some(shutdown),
            task,
        }
    }

    async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        (&mut self.task).await.expect("loopback server task");
    }
}

#[derive(Clone)]
struct AuthenticatedSession {
    client: reqwest::Client,
    cookie: String,
}

async fn authenticate(base_url: &str, email: &str, password: &str) -> AuthenticatedSession {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base_url}/api/session"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("fixture login reaches loopback router");
    assert_eq!(
        response.status().as_u16(),
        200,
        "fixture login must succeed"
    );
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::to_owned)
        .expect("fixture login returns a session cookie");
    let _ = response.bytes().await;
    AuthenticatedSession { client, cookie }
}

// --- Dispatch metadata --------------------------------------------------

/// Safe per-attempt metadata: only bounded durations, counts and outcome
/// classifications, never a filter, name, or Person value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Meta {
    whole_source_durations_ms: Vec<u64>,
    feed_pool_wait_ms: Option<u64>,
    feed_pool_timed_out: bool,
}

fn meta_from(telemetry: HttpPerfTelemetry) -> Meta {
    Meta {
        whole_source_durations_ms: telemetry
            .source_evaluations
            .iter()
            .map(|e| e.duration.as_millis() as u64)
            .collect(),
        feed_pool_wait_ms: telemetry
            .feed_pool_acquisitions
            .first()
            .map(|a| a.duration.as_millis() as u64),
        feed_pool_timed_out: telemetry
            .feed_pool_acquisitions
            .iter()
            .any(|a| a.outcome == PoolAcquisitionOutcome::TimedOut),
    }
}

fn classify_today_body(status: u16, value: Option<&Value>) -> HttpBodyResult {
    if status == 503 {
        return HttpBodyResult::Unavailable;
    }
    let Some(value) = value else {
        return HttpBodyResult::InvalidEnvelope;
    };
    let source_status = value
        .get("sources")
        .and_then(|s| s.get("status"))
        .and_then(Value::as_str);
    match (status, source_status, value.get("items").is_some()) {
        (200, Some("complete"), true) => HttpBodyResult::Complete,
        (200, Some("partial"), true) => HttpBodyResult::Partial,
        (200, Some("unavailable"), true) => HttpBodyResult::Unavailable,
        _ => HttpBodyResult::InvalidEnvelope,
    }
}

#[allow(clippy::result_large_err)]
async fn dispatch_get(
    session: AuthenticatedSession,
    url: String,
    collector: HttpPerfCollector,
    capture_id: u64,
) -> Result<CompletedHttpResponse<Meta>, ClientFailure<Meta>> {
    collector.begin_capture(capture_id);
    let response = session
        .client
        .get(&url)
        .header(COOKIE, &session.cookie)
        .header("x-crm-perf-capture", capture_id.to_string())
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            let request_completed_at = Instant::now();
            let telemetry = collector
                .drain_after_client_failure(capture_id, Duration::from_secs(2))
                .await;
            return Err(ClientFailure {
                kind: if error.is_timeout() {
                    ClientFailureKind::RequestTimeout
                } else {
                    ClientFailureKind::Transport
                },
                request_completed_at,
                metadata: meta_from(telemetry),
            });
        }
    };
    let status = response.status().as_u16();
    let bytes = response.bytes().await;
    let request_completed_at = Instant::now();
    let Ok(bytes) = bytes else {
        let telemetry = collector
            .drain_after_client_failure(capture_id, Duration::from_secs(2))
            .await;
        return Err(ClientFailure {
            kind: ClientFailureKind::BodyRead,
            request_completed_at,
            metadata: meta_from(telemetry),
        });
    };
    let telemetry = collector.take(capture_id);
    let value: Option<Value> = serde_json::from_slice(&bytes).ok();
    let result = classify_today_body(status, value.as_ref());
    let metadata = meta_from(telemetry);
    Ok(CompletedHttpResponse {
        status,
        result,
        whole_source_evaluations: metadata.whole_source_durations_ms.len(),
        request_completed_at,
        metadata,
    })
}

// --- Percentile / p95-cap helpers ----------------------------------------

fn request_durations(waves: &[WaveCapture<Meta>]) -> Vec<Duration> {
    waves
        .iter()
        .flat_map(|w| w.attempts.iter())
        .filter_map(|a| match a.timing {
            TimingCapture::Exact(d) | TimingCapture::JoinObservedUpperBound(d) => Some(d),
            TimingCapture::Unavailable => None,
        })
        .collect()
}

fn all_normal(waves: &[WaveCapture<Meta>]) -> bool {
    waves.iter().flat_map(|w| w.attempts.iter()).all(|a| {
        matches!(
            &a.outcome,
            AttemptOutcome::Response(r) if r.status == 200 && r.result == HttpBodyResult::Complete
        )
    })
}

fn whole_source_durations(waves: &[WaveCapture<Meta>]) -> Vec<Duration> {
    waves
        .iter()
        .flat_map(|w| w.attempts.iter())
        .filter_map(|a| a.outcome.metadata())
        .flat_map(|m| {
            m.whole_source_durations_ms
                .iter()
                .map(|ms| Duration::from_millis(*ms))
        })
        .collect()
}

fn feed_pool_wait_durations(waves: &[WaveCapture<Meta>]) -> Vec<Duration> {
    waves
        .iter()
        .flat_map(|w| w.attempts.iter())
        .filter_map(|a| a.outcome.metadata())
        .filter_map(|m| m.feed_pool_wait_ms.map(Duration::from_millis))
        .collect()
}

fn p95(durations: &[Duration]) -> Option<Duration> {
    today_http_perf_driver::nearest_rank_percentile(durations, 0.95)
}

fn ms(d: Option<Duration>) -> Option<u64> {
    d.map(|d| d.as_millis() as u64)
}

#[derive(Debug, Clone, Serialize)]
struct CaseReport {
    label: String,
    serial_attempts: usize,
    serial_p95_ms: Option<u64>,
    serial_cap_ms: u64,
    serial_ok: bool,
    c10_p95_ms: Option<u64>,
    c10_cap_ms: Option<u64>,
    c10_ok: bool,
    c20_p95_ms: Option<u64>,
    c20_cap_ms: Option<u64>,
    c20_ok: bool,
    whole_source_p95_ms: Option<u64>,
    whole_source_max_ms: Option<u64>,
    whole_source_ok: bool,
    feed_pool_wait_p95_ms: Option<u64>,
    feed_pool_wait_max_ms: Option<u64>,
    feed_pool_wait_margin_ms: Option<i64>,
    all_normal: bool,
}

/// Runs `WARMUP_WAVES` x `WARMUP_CONCURRENCY` (discarded from percentiles)
/// then the declared serial/10/20 sample shape, returning the measured
/// waves plus a computed `CaseReport`. Generic over the concrete dispatch
/// closure type `run_wave`/`run_serial` require (never a `dyn Fn` trait
/// object, which cannot satisfy their `Sized` bound).
async fn run_case<F>(
    label: &str,
    ids: &mut AttemptIds,
    arm: QueryArm,
    serial_attempts: usize,
    waves_at_10: usize,
    waves_at_20: usize,
    request: Arc<F>,
) -> (Vec<WaveCapture<Meta>>, CaseReport)
where
    F: Fn(AttemptContext) -> DispatchFuture + Send + Sync + 'static,
{
    for wave in 0..WARMUP_WAVES {
        let _ = run_wave(
            ids,
            arm,
            AttemptPhase::Warmup,
            wave,
            WARMUP_CONCURRENCY,
            request.clone(),
        )
        .await
        .expect("warm-up wave dispatches");
    }

    let serial = if serial_attempts > 0 {
        run_serial(
            ids,
            arm,
            AttemptPhase::Measured,
            0,
            serial_attempts,
            request.clone(),
        )
        .await
        .expect("serial samples dispatch")
    } else {
        Vec::new()
    };

    let mut c10_waves = Vec::new();
    for wave in 0..waves_at_10 {
        let w = run_wave(
            ids,
            arm,
            AttemptPhase::Measured,
            1000 + wave,
            10,
            request.clone(),
        )
        .await
        .expect("concurrency-10 wave dispatches");
        c10_waves.push(w);
    }

    let mut c20_waves = Vec::new();
    for wave in 0..waves_at_20 {
        let w = run_wave(
            ids,
            arm,
            AttemptPhase::Measured,
            2000 + wave,
            20,
            request.clone(),
        )
        .await
        .expect("concurrency-20 wave dispatches");
        c20_waves.push(w);
    }

    let mut measured = Vec::new();
    measured.extend(serial.iter().cloned());
    measured.extend(c10_waves.iter().cloned());
    measured.extend(c20_waves.iter().cloned());

    let serial_p95 = p95(&request_durations(&serial));
    let c10_p95 = (waves_at_10 > 0)
        .then(|| p95(&request_durations(&c10_waves)))
        .flatten();
    let c20_p95 = (waves_at_20 > 0)
        .then(|| p95(&request_durations(&c20_waves)))
        .flatten();
    let ws = whole_source_durations(&measured);
    let ws_p95 = p95(&ws);
    let ws_max = ws.iter().max().copied();
    let pool = feed_pool_wait_durations(&measured);
    let pool_p95 = p95(&pool);
    let pool_max = pool.iter().max().copied();

    let report = CaseReport {
        label: label.to_string(),
        serial_attempts,
        serial_p95_ms: ms(serial_p95),
        serial_cap_ms: REQUEST_P95_CONCURRENCY_1.as_millis() as u64,
        serial_ok: serial_p95
            .map(|d| d <= REQUEST_P95_CONCURRENCY_1)
            .unwrap_or(true),
        c10_p95_ms: ms(c10_p95),
        c10_cap_ms: (waves_at_10 > 0).then_some(REQUEST_P95_CONCURRENCY_10.as_millis() as u64),
        c10_ok: c10_p95
            .map(|d| d <= REQUEST_P95_CONCURRENCY_10)
            .unwrap_or(true),
        c20_p95_ms: ms(c20_p95),
        c20_cap_ms: (waves_at_20 > 0).then_some(REQUEST_P95_CONCURRENCY_20.as_millis() as u64),
        c20_ok: c20_p95
            .map(|d| d <= REQUEST_P95_CONCURRENCY_20)
            .unwrap_or(true),
        whole_source_p95_ms: ms(ws_p95),
        whole_source_max_ms: ms(ws_max),
        whole_source_ok: ws_max.map(|d| d <= WHOLE_SOURCE_P95_LIMIT).unwrap_or(true),
        feed_pool_wait_p95_ms: ms(pool_p95),
        feed_pool_wait_max_ms: ms(pool_max),
        feed_pool_wait_margin_ms: pool_max
            .map(|d| (Duration::from_secs(2).as_millis() as i64) - (d.as_millis() as i64)),
        all_normal: all_normal(&measured),
    };
    (measured, report)
}

fn dispatch_factory(
    session: AuthenticatedSession,
    base_url: String,
    collector: HttpPerfCollector,
    capture_offset: u64,
) -> Arc<impl Fn(AttemptContext) -> DispatchFuture + Send + Sync + 'static> {
    Arc::new(move |ctx: AttemptContext| {
        let session = session.clone();
        let base_url = base_url.clone();
        let collector = collector.clone();
        let capture_id = ctx.id.0 + capture_offset;
        Box::pin(dispatch_get(
            session,
            format!("{base_url}/api/today"),
            collector,
            capture_id,
        )) as DispatchFuture
    })
}

/// A viewer's org/user id by its known fixture email, plus a promotion to
/// Admin (system-feed commands require it). Fixture setup, not a measured
/// request.
async fn org_and_admin(pool: &PgPool, email: &str) -> (Uuid, Uuid) {
    let row: (Uuid, Uuid) = sqlx::query_as(
        "SELECT om.organization_id, au.id FROM app_user au \
         JOIN organization_membership om ON om.user_id = au.id \
         WHERE au.email = $1",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .expect("fixture viewer has exactly one membership");
    sqlx::query("UPDATE organization_membership SET role = 'admin' WHERE user_id = $1")
        .bind(row.1)
        .execute(pool)
        .await
        .expect("promote fixture viewer to admin for feed commands");
    (row.0, row.1)
}

#[derive(Debug, Serialize)]
struct RegressionReport {
    case: String,
    legacy_p95_ms: Option<u64>,
    feeds_p95_ms: Option<u64>,
    allowed_ms: u64,
    within_allowed: bool,
    payload_equal_excluding_sources: bool,
}

#[derive(Debug, Serialize)]
struct PlanReport {
    label: String,
    plan_text: String,
}

#[derive(Debug, Serialize)]
struct FixtureCountsReport {
    people: usize,
    inquiries: usize,
    contact_facts: usize,
    contact_corrections: usize,
    inbound_records: usize,
}

impl From<today_http_perf_fixture::TodayHttpPerfCounts> for FixtureCountsReport {
    fn from(c: today_http_perf_fixture::TodayHttpPerfCounts) -> Self {
        Self {
            people: c.people,
            inquiries: c.inquiries,
            contact_facts: c.contact_facts,
            contact_corrections: c.contact_corrections,
            inbound_records: c.inbound_records,
        }
    }
}

#[derive(Debug, Serialize)]
struct Evidence {
    fixture_counts: FixtureCountsReport,
    regression: RegressionReport,
    matrix: Vec<CaseReport>,
    independent_repeat: CaseReport,
    new_cases: Vec<CaseReport>,
    explain_plans: Vec<PlanReport>,
}

#[sqlx::test]
#[ignore = "Phase B only: opt-in, isolated database, real HTTP load"]
async fn slice_011d_authenticated_http_performance_harness(migrator_pool: PgPool) {
    let fixture = TodayHttpPerfFixture::create(migrator_pool).await;
    let fixture_counts = fixture.counts();
    let mut ids = AttemptIds::default();

    let setup_pool = fixture.arm_pool().await;
    let concentrated_email = "slice-011c-concentrated@example.invalid";
    let (organization_id, admin_id) = org_and_admin(&setup_pool, concentrated_email).await;
    setup_pool.close().await;

    // ==== Part 1: paired zero-source regression, retained historical =====
    // `TodayProvider::Legacy` and its seam (`router_with_test_clock_and_
    // provider`, `query_owned_at_with_provider`) were deleted in Slice
    // 011d step 6, once the DB-level equivalence suite
    // (`db_today_feed_equivalence.rs`) proved `Legacy`'s compiled-in
    // statement byte-identical to a frozen fixture of the pre-011d source
    // (`tests/fixtures/today_f51bff8/`). Re-running a live Legacy-vs-Feeds
    // HTTP pairing is no longer possible (there is nothing left to serve
    // `Legacy` through), so this retained artifact carries forward the
    // ONE regression comparison actually captured, byte-for-byte, from
    // docs/design/perf/slice-011d-2026-09-07/run.json (also reproduced in
    // that archive's README Part 1 table) rather than fabricating a new
    // number or silently dropping the row from `Evidence`.
    let regression = RegressionReport {
        case: "concentrated_zero_source_serial".to_string(),
        legacy_p95_ms: Some(204),
        feeds_p95_ms: Some(177),
        allowed_ms: 229,
        within_allowed: true,
        payload_equal_excluding_sources: true,
    };

    // ==== Part 2: the 011c matrix at 1/10/20 concurrency, Feeds =========
    let mut matrix = Vec::new();
    let critical_modes = [
        (PerfSourceMode::Zero, "concentrated_zero"),
        (PerfSourceMode::OneDense, "concentrated_one_dense"),
        (PerfSourceMode::OneAbsence, "concentrated_one_absence"),
        (PerfSourceMode::FiveOverlap, "concentrated_five_overlap"),
    ];
    for (mode, label) in critical_modes {
        let pool = fixture.arm_pool().await;
        let collector = HttpPerfCollector::default();
        let router = crm_api::build_app_with_today_router_and_perf_collector(
            AppState::for_tests(pool.clone(), fixture.config(), Publisher::recording()),
            crm_api::routes::today::router_with_test_clock(fixture.fixed_clock()),
            collector.clone(),
        );
        let server = LoopbackServer::start(router).await;
        let viewer = fixture.viewer(TodayHttpPerfCase::Concentrated);
        let session = authenticate(&server.base_url, &viewer.email, &viewer.password).await;
        fixture
            .assert_sources_via_http(
                &server.base_url,
                &session.cookie,
                TodayHttpPerfCase::Concentrated,
                mode,
            )
            .await;
        let request = dispatch_factory(session, server.base_url.clone(), collector, 20_000_000);
        let (_waves, report) = run_case(
            label,
            &mut ids,
            QueryArm::Final,
            CRITICAL_SAMPLE_SHAPE.serial_attempts,
            CRITICAL_SAMPLE_SHAPE.waves_at_10,
            CRITICAL_SAMPLE_SHAPE.waves_at_20,
            request,
        )
        .await;
        matrix.push(report);
        server.stop().await;
        pool.close().await;
    }

    let supplemental_books = [
        (TodayHttpPerfCase::Typical, "typical"),
        (TodayHttpPerfCase::PartialBuiltins, "partial_builtins"),
        (TodayHttpPerfCase::EmptyBuiltins, "empty_builtins"),
    ];
    for (case, book_label) in supplemental_books {
        for (mode, mode_label) in [
            (PerfSourceMode::Zero, "zero"),
            (PerfSourceMode::FiveOverlap, "five_overlap"),
        ] {
            let pool = fixture.arm_pool().await;
            let collector = HttpPerfCollector::default();
            let router = crm_api::build_app_with_today_router_and_perf_collector(
                AppState::for_tests(pool.clone(), fixture.config(), Publisher::recording()),
                crm_api::routes::today::router_with_test_clock(fixture.fixed_clock()),
                collector.clone(),
            );
            let server = LoopbackServer::start(router).await;
            let viewer = fixture.viewer(case);
            let session = authenticate(&server.base_url, &viewer.email, &viewer.password).await;
            fixture
                .assert_sources_via_http(&server.base_url, &session.cookie, case, mode)
                .await;
            let request = dispatch_factory(session, server.base_url.clone(), collector, 30_000_000);
            let (_waves, report) = run_case(
                &format!("{book_label}_{mode_label}"),
                &mut ids,
                QueryArm::Final,
                SUPPLEMENTAL_SAMPLE_SHAPE.serial_attempts,
                SUPPLEMENTAL_SAMPLE_SHAPE.waves_at_10,
                SUPPLEMENTAL_SAMPLE_SHAPE.waves_at_20,
                request,
            )
            .await;
            matrix.push(report);
            server.stop().await;
            pool.close().await;
        }
    }

    // Independent five-source concentrated concurrency-20 repeat.
    let independent_repeat = {
        let pool = fixture.arm_pool().await;
        let collector = HttpPerfCollector::default();
        let router = crm_api::build_app_with_today_router_and_perf_collector(
            AppState::for_tests(pool.clone(), fixture.config(), Publisher::recording()),
            crm_api::routes::today::router_with_test_clock(fixture.fixed_clock()),
            collector.clone(),
        );
        let server = LoopbackServer::start(router).await;
        let viewer = fixture.viewer(TodayHttpPerfCase::Concentrated);
        let session = authenticate(&server.base_url, &viewer.email, &viewer.password).await;
        fixture
            .assert_sources_via_http(
                &server.base_url,
                &session.cookie,
                TodayHttpPerfCase::Concentrated,
                PerfSourceMode::FiveOverlap,
            )
            .await;
        let request = dispatch_factory(session, server.base_url.clone(), collector, 40_000_000);
        let (_waves, report) = run_case(
            "concentrated_five_overlap_independent_repeat",
            &mut ids,
            QueryArm::Final,
            0,
            0,
            INDEPENDENT_REPEAT_WAVES,
            request,
        )
        .await;
        assert_eq!(
            INDEPENDENT_REPEAT_CONCURRENCY, 20,
            "declared repeat concurrency"
        );
        server.stop().await;
        pool.close().await;
        report
    };

    // ==== Part 3: new 011d cases + preview p95, concentrated zero-source =
    let mut new_cases = Vec::new();
    let app_pool = fixture.arm_pool().await;
    let stage_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position")
            .bind(organization_id)
            .fetch_all(&app_pool)
            .await
            .expect("fixture organization stages");
    let concentrated_stage = stage_ids[0];

    // Case: feed A (unanswered_inquiry) customized with an extra stage
    // clause matching the fixture's single populated stage — the candidate
    // set is unchanged (every fixture Person shares this stage), so this
    // measures the added predicate's evaluation COST, not a narrower result.
    // Every command's returned `outcome.feed.revision` feeds the next
    // command's `expected_revision` — never a hardcoded guess.
    let outcome = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                    Clause::Stage(StageClause {
                        stage_ids: vec![StageId::new(concentrated_stage)],
                    }),
                ],
            },
            fresh_within_hours: Some(24),
        },
    )
    .await
    .expect("customize feed A for the perf case");
    new_cases.push(run_new_case(&fixture, &mut ids, "feed_a_customized_stage_clause").await);
    let outcome = commands::revert_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        commands::RevertTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: outcome.feed.revision,
        },
    )
    .await
    .expect("revert feed A after the customized case");

    // Case: feed A disabled.
    let outcome = commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: outcome.feed.revision,
            enabled: false,
        },
    )
    .await
    .expect("disable feed A for the perf case");
    new_cases.push(run_new_case(&fixture, &mut ids, "feed_a_disabled").await);
    commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: outcome.feed.revision,
            enabled: true,
        },
    )
    .await
    .expect("re-enable feed A");

    // Case: feed C (call_outcome_needed) disabled.
    let outcome = commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: 1,
            enabled: false,
        },
    )
    .await
    .expect("disable feed C for the perf case");
    new_cases.push(run_new_case(&fixture, &mut ids, "feed_c_disabled").await);
    commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: outcome.feed.revision,
            enabled: true,
        },
    )
    .await
    .expect("re-enable feed C");

    // Case: preview p95 (POST .../today-feeds/unanswered_inquiry/preview),
    // 8 serial samples, cap 1,250 ms.
    let preview_durations = {
        let mut durations = Vec::with_capacity(8);
        for _ in 0..8 {
            let started = Instant::now();
            let outcome = commands::preview_today_system_feed(
                &app_pool,
                &command_context(organization_id, admin_id),
                PreviewTodaySystemFeed {
                    feed_key: FeedKey::UnansweredInquiry,
                    filter: FilterDefinition {
                        version: 1,
                        clauses: vec![
                            Clause::AssignedTo(AssignedToClause {
                                assignees: vec![Assignee::Me],
                            }),
                            Clause::AwaitingResponse(BoolClause { value: true }),
                        ],
                    },
                    fresh_within_hours: Some(24),
                    subject: UserId::new(admin_id),
                },
            )
            .await;
            outcome.expect("preview succeeds for the perf case");
            durations.push(started.elapsed());
        }
        durations
    };
    let preview_p95 = p95(&preview_durations);
    let preview_report = CaseReport {
        label: "preview_unanswered_inquiry_serial".to_string(),
        serial_attempts: preview_durations.len(),
        serial_p95_ms: ms(preview_p95),
        serial_cap_ms: REQUEST_P95_CONCURRENCY_1.as_millis() as u64,
        serial_ok: preview_p95
            .map(|d| d <= REQUEST_P95_CONCURRENCY_1)
            .unwrap_or(true),
        c10_p95_ms: None,
        c10_cap_ms: None,
        c10_ok: true,
        c20_p95_ms: None,
        c20_cap_ms: None,
        c20_ok: true,
        whole_source_p95_ms: None,
        whole_source_max_ms: None,
        whole_source_ok: true,
        feed_pool_wait_p95_ms: None,
        feed_pool_wait_max_ms: None,
        feed_pool_wait_margin_ms: None,
        all_normal: true,
    };
    new_cases.push(preview_report);
    app_pool.close().await;

    // ==== Part 4: EXPLAIN (ANALYZE, BUFFERS) of person_state.sql, with ===
    // and without SET LOCAL enable_mergejoin = off, on the vacuumed
    // concentrated book.
    let explain_plans = {
        let pool = fixture.arm_pool().await;
        for relation in [
            "person",
            "inquiry",
            "contact_attempted",
            "correspondence_captured",
            "contact_method",
            "call",
        ] {
            sqlx::query(&format!("VACUUM (ANALYZE) {relation}"))
                .execute(&pool)
                .await
                .expect("vacuum analyze fills the visibility map before EXPLAIN");
        }

        let mut tx = pool.begin().await.expect("explain transaction");
        let viewer = fixture.viewer(TodayHttpPerfCase::Concentrated);
        let (_org, viewer_id): (Uuid, Uuid) = sqlx::query_as(
            "SELECT om.organization_id, au.id FROM app_user au \
             JOIN organization_membership om ON om.user_id = au.id WHERE au.email = $1",
        )
        .bind(&viewer.email)
        .fetch_one(&pool)
        .await
        .expect("resolve concentrated viewer id");

        let query_text = std::fs::read_to_string(
            "crates/crm-app/src/domain/today/system_feeds/sql/person_state.sql",
        )
        .expect("read person_state.sql for EXPLAIN");
        let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS) {query_text}");

        let default_plan = run_explain(&mut tx, &explain_sql, organization_id, viewer_id).await;
        sqlx::query("ROLLBACK TO SAVEPOINT explain_default")
            .execute(&mut *tx)
            .await
            .unwrap_or(sqlx::postgres::PgQueryResult::default());

        sqlx::query("SET LOCAL enable_mergejoin = off")
            .execute(&mut *tx)
            .await
            .expect("set enable_mergejoin off");
        let mergejoin_off_plan =
            run_explain(&mut tx, &explain_sql, organization_id, viewer_id).await;

        let _ = tx.rollback().await;
        pool.close().await;
        vec![
            PlanReport {
                label: "person_state_default".to_string(),
                plan_text: default_plan,
            },
            PlanReport {
                label: "person_state_enable_mergejoin_off".to_string(),
                plan_text: mergejoin_off_plan,
            },
        ]
    };

    fixture.cleanup().await;

    let evidence = Evidence {
        fixture_counts: fixture_counts.into(),
        regression,
        matrix,
        independent_repeat,
        new_cases,
        explain_plans,
    };
    println!("SLICE_011D_PERF_EVIDENCE_JSON_START");
    println!("{}", serde_json::to_string_pretty(&evidence).unwrap());
    println!("SLICE_011D_PERF_EVIDENCE_JSON_END");
}

async fn run_new_case(
    fixture: &TodayHttpPerfFixture,
    ids: &mut AttemptIds,
    label: &str,
) -> CaseReport {
    let pool = fixture.arm_pool().await;
    let collector = HttpPerfCollector::default();
    let router = crm_api::build_app_with_today_router_and_perf_collector(
        AppState::for_tests(pool.clone(), fixture.config(), Publisher::recording()),
        crm_api::routes::today::router_with_test_clock(fixture.fixed_clock()),
        collector.clone(),
    );
    let server = LoopbackServer::start(router).await;
    let viewer = fixture.viewer(TodayHttpPerfCase::Concentrated);
    let session = authenticate(&server.base_url, &viewer.email, &viewer.password).await;
    fixture
        .assert_sources_via_http(
            &server.base_url,
            &session.cookie,
            TodayHttpPerfCase::Concentrated,
            PerfSourceMode::Zero,
        )
        .await;
    let capture_offset = 50_000_000 + (label.len() as u64) * 1_000;
    let request = dispatch_factory(session, server.base_url.clone(), collector, capture_offset);
    let (_waves, report) = run_case(label, ids, QueryArm::Final, 8, 0, 0, request).await;
    server.stop().await;
    pool.close().await;
    report
}

async fn run_explain(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    explain_sql: &str,
    organization_id: Uuid,
    viewer_id: Uuid,
) -> String {
    sqlx::query("SAVEPOINT explain_default")
        .execute(&mut **tx)
        .await
        .expect("savepoint");
    // person_state.sql binds 51 params: $1 organization_id, $2-23 feed A
    // matrix, $24-45 feed B matrix, $46 now, $47 viewer, $48/49 enabled
    // flags, $50/51 fresh windows (hours). The canonical definition for
    // both feeds (anchor + assigned_to me) exercises the SAME statement
    // production evaluation runs for an unmodified Organization.
    let now = chrono::Utc::now();
    let rows: Vec<(String,)> = sqlx::query_as(explain_sql)
        .bind(organization_id)
        .bind(None::<Vec<Uuid>>) // $2 stage_ids (feed A)
        .bind(Some(vec![viewer_id])) // $3 assigned_user_ids (feed A)
        .bind(false) // $4 assigned_include_unassigned
        .bind(None::<Vec<String>>) // $5 sources
        .bind(None::<i32>) // $6 created_within_days
        .bind(None::<i32>) // $7 created_not_within_days
        .bind(None::<bool>) // $8 created_never
        .bind(None::<i32>) // $9 last_inquiry_within_days
        .bind(None::<i32>) // $10 last_inquiry_not_within_days
        .bind(None::<bool>) // $11 last_inquiry_never
        .bind(None::<i32>) // $12 last_contact_within_days
        .bind(None::<i32>) // $13 last_contact_not_within_days
        .bind(None::<bool>) // $14 last_contact_never
        .bind(None::<i32>) // $15 last_inbound_within_days
        .bind(None::<i32>) // $16 last_inbound_not_within_days
        .bind(None::<bool>) // $17 last_inbound_never
        .bind(None::<bool>) // $18 has_replied
        .bind(None::<bool>) // $19 has_phone
        .bind(None::<bool>) // $20 has_email
        .bind(Some(true)) // $21 awaiting_response (feed A anchor)
        .bind(None::<bool>) // $22 client_replied_unanswered
        .bind(None::<bool>) // $23 awaiting_call_outcome
        .bind(None::<Vec<Uuid>>) // $24 stage_ids (feed B)
        .bind(Some(vec![viewer_id])) // $25 assigned_user_ids (feed B)
        .bind(false) // $26 assigned_include_unassigned
        .bind(None::<Vec<String>>) // $27 sources
        .bind(None::<i32>) // $28
        .bind(None::<i32>) // $29
        .bind(None::<bool>) // $30
        .bind(None::<i32>) // $31
        .bind(None::<i32>) // $32
        .bind(None::<bool>) // $33
        .bind(None::<i32>) // $34
        .bind(None::<i32>) // $35
        .bind(None::<bool>) // $36
        .bind(None::<i32>) // $37
        .bind(None::<i32>) // $38
        .bind(None::<bool>) // $39
        .bind(None::<bool>) // $40 has_replied
        .bind(None::<bool>) // $41 has_phone
        .bind(None::<bool>) // $42 has_email
        .bind(None::<bool>) // $43 awaiting_response
        .bind(Some(true)) // $44 client_replied_unanswered (feed B anchor)
        .bind(None::<bool>) // $45 awaiting_call_outcome
        .bind(now) // $46 now
        .bind(viewer_id) // $47 viewer
        .bind(true) // $48 feed A enabled
        .bind(true) // $49 feed B enabled
        .bind(24_i32) // $50 fresh hours A
        .bind(24_i32) // $51 fresh hours B
        .fetch_all(&mut **tx)
        .await
        .expect("EXPLAIN ANALYZE person_state.sql");
    rows.into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

// --- D-050 plan-shape evidence: call_only.sql, call_membership.sql, ------
// filtered_summaries.sql (no new benchmark run — EXPLAIN-only, reusing
// the same fixture builder, never re-measuring HTTP performance). -------

#[sqlx::test]
#[ignore = "Phase B only: opt-in, isolated database, real HTTP load"]
async fn d050_plan_shape_evidence_call_statements_and_filtered_summaries(migrator_pool: PgPool) {
    let fixture = TodayHttpPerfFixture::create(migrator_pool).await;
    let pool = fixture.arm_pool().await;
    let viewer = fixture.viewer(TodayHttpPerfCase::Concentrated);
    let (organization_id, viewer_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT om.organization_id, au.id FROM app_user au \
         JOIN organization_membership om ON om.user_id = au.id WHERE au.email = $1",
    )
    .bind(&viewer.email)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Give the `call` table real volume for the concentrated viewer (the
    // fixture otherwise never inserts any Call) so the plan reflects a
    // realistic caller_user_id lookup, not a scan of an empty table.
    let call_person_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM person WHERE organization_id = $1 AND assigned_user_id = $2 LIMIT 1000",
    )
    .bind(organization_id)
    .bind(viewer_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    for person_id in &call_person_ids {
        let call_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO call \
                (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
                 correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
             VALUES \
                ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
                 'scripted', 'd050-fixture', $7, $7)",
        )
        .bind(call_id)
        .bind(organization_id)
        .bind(person_id)
        .bind(Uuid::new_v4())
        .bind(viewer_id)
        .bind(Uuid::new_v4())
        .bind(fixture.fixed_clock() - chrono::Duration::hours(2))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_attempted \
                (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
                 causation_id, person_id, channel, outcome) \
             VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, 'call', 'reached')",
        )
        .bind(organization_id)
        .bind(fixture.fixed_clock() - chrono::Duration::hours(2))
        .bind(Uuid::new_v4())
        .bind(call_id)
        .bind(person_id)
        .execute(&pool)
        .await
        .unwrap();
    }

    for relation in [
        "person",
        "inquiry",
        "contact_attempted",
        "call",
        "correspondence_captured",
        "contact_method",
    ] {
        sqlx::query(&format!("VACUUM (ANALYZE) {relation}"))
            .execute(&pool)
            .await
            .unwrap();
    }

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL jit = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL enable_mergejoin = off")
        .execute(&mut *tx)
        .await
        .unwrap();

    // call_membership.sql: canonical (no extra clauses), retained_ids =
    // the first 200 called Persons (as if person-state already retained
    // them) — exercises the JOIN against a real retained set.
    let retained: Vec<Uuid> = call_person_ids.iter().take(200).copied().collect();
    let call_membership_sql = std::fs::read_to_string(
        "crates/crm-app/src/domain/today/system_feeds/sql/call_membership.sql",
    )
    .unwrap();
    let explain_membership = format!("EXPLAIN (ANALYZE, BUFFERS) {call_membership_sql}");
    let rows: Vec<(String,)> = sqlx::query_as(&explain_membership)
        .bind(organization_id)
        .bind(None::<Vec<Uuid>>) // $2 stage_ids
        .bind(None::<Vec<Uuid>>) // $3 assigned_user_ids
        .bind(None::<bool>) // $4 assigned_include_unassigned
        .bind(None::<Vec<String>>) // $5 sources
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $6-8 created
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $9-11 last_inquiry
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $12-14 last_contact
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $15-17 last_inbound
        .bind(None::<bool>) // $18 has_replied
        .bind(None::<bool>) // $19 has_phone
        .bind(None::<bool>) // $20 has_email
        .bind(None::<bool>) // $21 awaiting_response
        .bind(None::<bool>) // $22 client_replied_unanswered
        .bind(Some(true)) // $23 awaiting_call_outcome (anchor)
        .bind(viewer_id) // $24 viewer
        .bind(&retained) // $25 retained_ids
        .bind(fixture.fixed_clock()) // $26 clock
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let membership_plan = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");

    // call_only.sql: canonical, retained_ids = the SAME 200 (so the other
    // 800 called-but-not-retained Persons are the call-only candidates).
    let call_only_sql =
        std::fs::read_to_string("crates/crm-app/src/domain/today/system_feeds/sql/call_only.sql")
            .unwrap();
    let explain_only = format!("EXPLAIN (ANALYZE, BUFFERS) {call_only_sql}");
    let rows: Vec<(String,)> = sqlx::query_as(&explain_only)
        .bind(organization_id)
        .bind(None::<Vec<Uuid>>)
        .bind(None::<Vec<Uuid>>)
        .bind(None::<bool>)
        .bind(None::<Vec<String>>)
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>)
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>)
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>)
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>)
        .bind(None::<bool>)
        .bind(None::<bool>)
        .bind(None::<bool>)
        .bind(None::<bool>)
        .bind(None::<bool>)
        .bind(Some(true))
        .bind(viewer_id)
        .bind(&retained)
        .bind(201_i64) // limit
        .bind(fixture.fixed_clock())
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let call_only_plan = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");

    // filtered_summaries.sql: the pre-existing statement, three NEW
    // clauses ABSENT (NULL) — proving they add no cost when unused.
    let filtered_summaries_sql =
        std::fs::read_to_string("crates/crm-app/src/domain/person/sql/filtered_summaries.sql")
            .unwrap();
    let explain_filtered = format!("EXPLAIN (ANALYZE, BUFFERS) {filtered_summaries_sql}");
    let rows: Vec<(String,)> = sqlx::query_as(&explain_filtered)
        .bind(organization_id)
        .bind(None::<Vec<Uuid>>) // $2 stage_ids
        .bind(Some(vec![viewer_id])) // $3 assigned_user_ids
        .bind(false) // $4 assigned_include_unassigned
        .bind(None::<Vec<String>>) // $5 sources
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $6-8
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $9-11
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $12-14
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>) // $15-17
        .bind(None::<bool>) // $18
        .bind(None::<bool>) // $19
        .bind(None::<bool>) // $20
        .bind(None::<chrono::DateTime<chrono::Utc>>) // $21 reference_now
        .bind(None::<bool>) // $22 awaiting_response (ABSENT)
        .bind(None::<bool>) // $23 client_replied_unanswered (ABSENT)
        .bind(None::<bool>) // $24 awaiting_call_outcome (ABSENT)
        .bind(viewer_id) // $25 viewer_id
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let filtered_summaries_plan = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");

    tx.rollback().await.unwrap();
    fixture.cleanup().await;

    println!("D050_PLAN_CALL_MEMBERSHIP_START");
    println!("{membership_plan}");
    println!("D050_PLAN_CALL_MEMBERSHIP_END");
    println!("D050_PLAN_CALL_ONLY_START");
    println!("{call_only_plan}");
    println!("D050_PLAN_CALL_ONLY_END");
    println!("D050_PLAN_FILTERED_SUMMARIES_START");
    println!("{filtered_summaries_plan}");
    println!("D050_PLAN_FILTERED_SUMMARIES_END");
}

// --- Slice 012 step 5 performance evidence (docs/specs/SLICE_012.md §7) ---
//
// A purpose-built, smaller harness (disclosed scope reduction, mirroring
// the Slice 011e archive's own precedent): one Organization, 25,000
// People (D-050's envelope ceiling), seeded directly via the batch-SQL
// technique lifted from `today_http_perf_fixture.rs::seed_people_and_history`
// (scaled down, same shape: history-rich, skewed assignment, corrections,
// waiting queues, inbound correspondence) — not the full 011c/011d Phase B
// HTTP-concurrency apparatus, which this slice's gate does not need
// (D-050: "no slice spends more than one benchmark run on performance
// unless the paired regression fails"). Scratch/ephemeral database only
// (the `#[sqlx::test]` throwaway), never `crm_dev`; no servers on
// 3000/5173 (the one Today HTTP trend sample below runs an in-process
// `axum::serve` on an OS-assigned ephemeral port, torn down at the end of
// the function, exactly like `today_http_perf_driver`'s own server spawn).

mod slice_012_perf {
    use std::time::{Duration, Instant};

    use chrono::{DateTime, TimeZone, Utc};
    use sqlx::PgPool;
    use uuid::Uuid;

    use crm_api::domain::person::filter::PersonFilterParams;
    use crm_api::domain::person::queries as person_queries;
    use crm_api::domain::person::visibility::PersonVisibilityScope;
    use crm_api::domain::today::sources as today_sources;
    use crm_api::domain::today::system_feeds::evaluate as system_feeds_evaluate;
    use crm_api::domain::today::system_feeds::{
        canonical_default, canonical_fresh_within_hours, FeedKey, ResolvedFeed,
    };
    use crm_api::ids::OrganizationId;

    use super::frozen;
    use super::today_http_perf_driver::nearest_rank_percentile;

    pub const PEOPLE: i64 = 25_000;

    pub fn fixed_clock() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 8, 12, 0, 0).unwrap()
    }

    /// Batch-SQL history seed, lifted (and scaled to 25k) from
    /// `today_http_perf_fixture.rs::seed_people_and_history`: the same
    /// shape (skewed assignment, historical + waiting inquiries,
    /// corrected contact attempts, inbound correspondence), one
    /// transaction. Returns the elapsed wall time — the trigger write-cost
    /// figure (report only, spec §7): every `inquiry`/`contact_attempted`/
    /// `correspondence_captured` row inserted here now also fires its
    /// `person_touch_*` trigger, whereas the 011c archive's equivalent
    /// seed (docs/design/perf/slice-011c-...) predates Slice 012 and paid
    /// no such per-row cost.
    pub async fn seed(
        pool: &PgPool,
        organization_id: Uuid,
        stage_id: Uuid,
        assignee: Uuid,
        caller: Uuid,
    ) -> Duration {
        let now = fixed_clock();
        let start = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "CREATE TEMP TABLE s012_people (id uuid PRIMARY KEY, ordinal integer NOT NULL UNIQUE) ON COMMIT DROP",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO s012_people (id, ordinal) \
             SELECT md5('slice-012-perf-person-' || value::text)::uuid, value \
             FROM generate_series(1, $1) AS value",
        )
        .bind(PEOPLE)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO person (id, organization_id, first_name, last_name, stage_id, assigned_user_id, created_at, updated_at) \
             SELECT id, $1, 'Synthetic', lpad(ordinal::text, 5, '0'), $2, \
               CASE WHEN ordinal % 3 = 0 THEN $3 ELSE NULL END, \
               $4 - ((ordinal % 365) * interval '1 day'), \
               $4 - ((ordinal % 365) * interval '1 day') \
             FROM s012_people",
        )
        .bind(organization_id)
        .bind(stage_id)
        .bind(assignee)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        // ~80% of People get an old, answered inquiry.
        sqlx::query(
            "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, received_at, created_at) \
             SELECT md5('slice-012-perf-inquiry-old-' || ordinal::text)::uuid, $1, id, \
               md5('slice-012-perf-raw-' || ordinal::text)::uuid, \
               CASE WHEN ordinal % 10 < 8 THEN 'zillow' ELSE 'website' END, \
               'fixture-' || ordinal::text, $2 - interval '30 days' - ((ordinal % 20) * interval '1 hour'), $2 \
             FROM s012_people WHERE ordinal % 5 <> 0",
        )
        .bind(organization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        // ~4% of People (the assignee's book) get a fresh, unanswered
        // (waiting) inquiry.
        sqlx::query(
            "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, received_at, created_at) \
             SELECT md5('slice-012-perf-inquiry-waiting-' || ordinal::text)::uuid, $1, id, \
               md5('slice-012-perf-raw-waiting-' || ordinal::text)::uuid, 'zillow', \
               'waiting-' || ordinal::text, $2 - interval '2 hours', $2 \
             FROM s012_people WHERE ordinal % 3 = 0 AND ordinal % 25 = 0",
        )
        .bind(organization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        // ~80% of the answered-inquiry People get a contact attempt.
        sqlx::query(
            "INSERT INTO contact_attempted \
               (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome) \
             SELECT md5('slice-012-perf-contact-root-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
               $2 - ((ordinal % 20 + 1) * interval '1 day'), $2, \
               md5('slice-012-perf-contact-correlation-' || ordinal::text)::uuid, NULL, NULL, id, 'call', 'no_answer' \
             FROM s012_people WHERE ordinal % 5 <> 0 AND ordinal % 4 <> 0",
        )
        .bind(organization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        // A correction chain on ~6% of those attempts.
        sqlx::query(
            "INSERT INTO contact_attempted \
               (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome) \
             SELECT md5('slice-012-perf-contact-correction-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
               $2 - ((ordinal % 20 + 1) * interval '1 day'), $2, \
               md5('slice-012-perf-contact-correction-correlation-' || ordinal::text)::uuid, NULL, \
               md5('slice-012-perf-contact-root-' || ordinal::text)::uuid, id, 'call', 'reached' \
             FROM s012_people WHERE ordinal % 5 <> 0 AND ordinal % 4 <> 0 AND ordinal % 17 = 0",
        )
        .bind(organization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        // Inbound correspondence for ~14% of People.
        sqlx::query(
            "INSERT INTO correspondence_raw (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed) \
             SELECT md5('slice-012-perf-raw-cc-' || ordinal::text)::uuid, $1, $2 - interval '12 hours', \
               decode('00', 'hex'), decode('00', 'hex'), \
               decode(md5('slice-012-perf-hmac-a-' || ordinal::text) || md5('slice-012-perf-hmac-b-' || ordinal::text), 'hex'), \
               1, true \
             FROM s012_people WHERE ordinal % 7 = 0",
        )
        .bind(organization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO correspondence_captured \
               (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, agent_user_id, direction, message_id, thread_key, via, correspondence_raw_id, backdated) \
             SELECT md5('slice-012-perf-cc-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
               $2 - interval '12 hours', $2, \
               md5('slice-012-perf-cc-correlation-' || ordinal::text)::uuid, NULL, NULL, id, $3, 'inbound', \
               'fixture-' || ordinal::text || '@example.invalid', NULL, 'cc', \
               md5('slice-012-perf-raw-cc-' || ordinal::text)::uuid, false \
             FROM s012_people WHERE ordinal % 7 = 0",
        )
        .bind(organization_id)
        .bind(now)
        .bind(caller)
        .execute(&mut *tx)
        .await
        .unwrap();
        // A handful of ended calls for the caller, so awaiting_call_outcome
        // and person_state's call chain have real volume too.
        for i in 1..=200i64 {
            let call_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO call (id, organization_id, person_id, contact_method_id, caller_user_id, origin, correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
                 SELECT $1, $2, id, $3, $4, 'web_session', $5, 'ended', 'agent_hangup', 'scripted', 'slice-012-perf', $6, $6 \
                 FROM s012_people WHERE ordinal = $7",
            )
            .bind(call_id)
            .bind(organization_id)
            .bind(Uuid::new_v4())
            .bind(caller)
            .bind(Uuid::new_v4())
            .bind(now - chrono::Duration::hours(3))
            .bind(i)
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO contact_attempted (organization_id, actor_kind, origin, occurred_at, correlation_id, causation_id, person_id, channel, outcome) \
                 SELECT $1, 'system', 'fixture', $2, $3, $4, id, 'call', 'reached' FROM s012_people WHERE ordinal = $5",
            )
            .bind(organization_id)
            .bind(now - chrono::Duration::hours(3))
            .bind(Uuid::new_v4())
            .bind(call_id)
            .bind(i)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        start.elapsed()
    }

    pub async fn vacuum_analyze(pool: &PgPool) {
        for relation in [
            "person",
            "inquiry",
            "contact_attempted",
            "call",
            "correspondence_captured",
            "contact_method",
        ] {
            sqlx::query(&format!("VACUUM (ANALYZE) {relation}"))
                .execute(pool)
                .await
                .unwrap();
        }
    }

    const SAMPLES: usize = 15;
    const WARMUP: usize = 3;

    fn p95(mut samples: Vec<Duration>) -> Duration {
        samples.drain(0..WARMUP);
        nearest_rank_percentile(&samples, 0.95).unwrap()
    }

    fn allowed(baseline: Duration) -> Duration {
        std::cmp::max(Duration::from_millis(25), baseline / 10)
    }

    /// Runs `f` `SAMPLES` times against a fresh connection each time,
    /// returning (durations, last result).
    async fn timed<T, F, Fut>(pool: &PgPool, mut f: F) -> (Vec<Duration>, T)
    where
        F: FnMut(sqlx::pool::PoolConnection<sqlx::Postgres>) -> Fut,
        Fut: std::future::Future<Output = (sqlx::pool::PoolConnection<sqlx::Postgres>, T)>,
    {
        let mut durations = Vec::with_capacity(SAMPLES);
        let mut last = None;
        for _ in 0..SAMPLES {
            let conn = pool.acquire().await.unwrap();
            let start = Instant::now();
            let (_conn, result) = f(conn).await;
            durations.push(start.elapsed());
            last = Some(result);
        }
        (durations, last.unwrap())
    }

    pub struct RegressionRow {
        pub statement: &'static str,
        pub binding: &'static str,
        pub frozen_p95: Duration,
        pub live_p95: Duration,
        pub allowed: Duration,
        pub within_allowed: bool,
        pub payload_equal: bool,
    }

    pub async fn gate1(
        app_pool: &PgPool,
        organization_id: Uuid,
        viewer_id: Uuid,
    ) -> Vec<RegressionRow> {
        let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
        let mut rows = Vec::new();

        let params_never = PersonFilterParams {
            last_contact_never: Some(true),
            viewer_id,
            ..PersonFilterParams::default()
        };
        let params_not_within_7 = PersonFilterParams {
            last_contact_not_within_days: Some(7),
            viewer_id,
            ..PersonFilterParams::default()
        };

        for (binding, params) in [
            ("last_contact never", &params_never),
            ("last_contact not_within_days 7", &params_not_within_7),
        ] {
            // filtered_summaries (default sort).
            let (frozen_d, frozen_r) = timed(app_pool, |mut conn| async move {
                let r = frozen::person_sql::filtered_summaries(&mut conn, &scope, params, None)
                    .await
                    .unwrap();
                (conn, r)
            })
            .await;
            let (live_d, live_r) = timed(app_pool, |mut conn| async move {
                let r = person_queries::filtered_summaries(&mut conn, &scope, params)
                    .await
                    .unwrap();
                (conn, r)
            })
            .await;
            let frozen_ids: Vec<Uuid> = frozen_r.0.iter().map(|p| p.id.0).collect();
            let live_ids: Vec<Uuid> = live_r.0.iter().map(|p| p.id.0).collect();
            let frozen_p95 = p95(frozen_d);
            let live_p95 = p95(live_d);
            rows.push(RegressionRow {
                statement: "filtered_summaries",
                binding,
                frozen_p95,
                live_p95,
                allowed: allowed(frozen_p95),
                within_allowed: live_p95 <= allowed(frozen_p95),
                payload_equal: frozen_ids == live_ids && frozen_r.1 == live_r.1,
            });

            // count_filtered_matches.
            let (frozen_d, frozen_r) = timed(app_pool, |mut conn| async move {
                let r = frozen::person_sql::count_filtered_matches(&mut conn, &scope, params)
                    .await
                    .unwrap();
                (conn, r)
            })
            .await;
            let (live_d, live_r) = timed(app_pool, |mut conn| async move {
                let r = person_queries::count_filtered_matches(&mut conn, &scope, params)
                    .await
                    .unwrap();
                (conn, r)
            })
            .await;
            let frozen_p95 = p95(frozen_d);
            let live_p95 = p95(live_d);
            rows.push(RegressionRow {
                statement: "count_filtered_matches",
                binding,
                frozen_p95,
                live_p95,
                allowed: allowed(frozen_p95),
                within_allowed: live_p95 <= allowed(frozen_p95),
                payload_equal: frozen_r == live_r,
            });

            // source_candidates.
            let now = fixed_clock();
            let builtin_ids: Vec<Uuid> = Vec::new();
            let org = OrganizationId::new(organization_id);
            let (frozen_d, frozen_r) = timed(app_pool, |mut conn| {
                let builtin_ids = &builtin_ids;
                async move {
                    let r = frozen::today_sql::source_candidates(
                        &mut conn,
                        org,
                        params,
                        now,
                        builtin_ids,
                        false,
                        50,
                    )
                    .await
                    .unwrap();
                    (conn, r)
                }
            })
            .await;
            let (live_d, live_r) = timed(app_pool, |mut conn| {
                let builtin_ids = &builtin_ids;
                async move {
                    let r = today_sources::source_candidates(
                        &mut conn,
                        org,
                        params,
                        now,
                        builtin_ids,
                        false,
                        50,
                    )
                    .await
                    .unwrap();
                    (conn, r)
                }
            })
            .await;
            let frozen_ids: Vec<Uuid> = frozen_r.iter().map(|c| c.person.id.0).collect();
            let live_ids: Vec<Uuid> = live_r.iter().map(|c| c.person.id.0).collect();
            let frozen_p95 = p95(frozen_d);
            let live_p95 = p95(live_d);
            rows.push(RegressionRow {
                statement: "source_candidates",
                binding,
                frozen_p95,
                live_p95,
                allowed: allowed(frozen_p95),
                within_allowed: live_p95 <= allowed(frozen_p95),
                payload_equal: frozen_ids == live_ids,
            });
        }

        // person_state with the canonical feed definitions bound.
        let feed_a_filter = canonical_default(FeedKey::UnansweredInquiry);
        let feed_b_filter = canonical_default(FeedKey::ClientReplied);
        let feed_a = ResolvedFeed {
            feed_key: FeedKey::UnansweredInquiry,
            enabled: true,
            filter: feed_a_filter.clone(),
            fresh_within_hours: canonical_fresh_within_hours(FeedKey::UnansweredInquiry),
            is_default: true,
            fallback: false,
            revision: 1,
            updated_at: Utc::now(),
            updated_by_user_id: None,
        };
        let feed_b = ResolvedFeed {
            feed_key: FeedKey::ClientReplied,
            enabled: true,
            filter: feed_b_filter.clone(),
            fresh_within_hours: canonical_fresh_within_hours(FeedKey::ClientReplied),
            is_default: true,
            fallback: false,
            revision: 1,
            updated_at: Utc::now(),
            updated_by_user_id: None,
        };
        let viewer = crm_api::ids::UserId::new(viewer_id);
        let org = OrganizationId::new(organization_id);
        let now = fixed_clock();
        let params_a = feed_a_filter.to_query_params(viewer);
        let params_b = feed_b_filter.to_query_params(viewer);
        let fresh_a = canonical_fresh_within_hours(FeedKey::UnansweredInquiry).unwrap_or(24);
        let fresh_b = canonical_fresh_within_hours(FeedKey::ClientReplied).unwrap_or(24);

        let (frozen_d, frozen_r) = timed(app_pool, |mut conn| {
            let params_a = &params_a;
            let params_b = &params_b;
            async move {
                let r = frozen::system_feeds_sql::person_state_candidates(
                    &mut conn, org, viewer, now, params_a, true, fresh_a, params_b, true, fresh_b,
                )
                .await
                .unwrap();
                (conn, r)
            }
        })
        .await;
        let (live_d, live_r) = timed(app_pool, |mut conn| {
            let feed_a = &feed_a;
            let feed_b = &feed_b;
            async move {
                let r = system_feeds_evaluate::person_state_candidates(
                    &mut conn, org, viewer, now, feed_a, feed_b,
                )
                .await
                .unwrap();
                (conn, r)
            }
        })
        .await;
        let frozen_ids: Vec<Uuid> = frozen_r.0.iter().map(|c| c.person.id.0).collect();
        let live_ids: Vec<Uuid> = live_r.0.iter().map(|c| c.person.id.0).collect();
        let frozen_p95 = p95(frozen_d);
        let live_p95 = p95(live_d);
        rows.push(RegressionRow {
            statement: "person_state",
            binding: "canonical feed definitions",
            frozen_p95,
            live_p95,
            allowed: allowed(frozen_p95),
            within_allowed: live_p95 <= allowed(frozen_p95),
            payload_equal: frozen_ids == live_ids && frozen_r.1 == live_r.1,
        });

        rows
    }
}

#[sqlx::test]
#[ignore = "Phase B only: opt-in, isolated database, real HTTP load"]
async fn slice_012_performance_evidence(migrator_pool: PgPool) {
    use slice_012_perf as s012;

    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (organization_id, viewer_id) = org_and_admin_named(
        &migrator_pool,
        "viewer@slice-012-perf.test",
        "Slice 012 Perf Realty",
    )
    .await;
    let stage_id = {
        let row: (Uuid,) = sqlx::query_as(
            "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1",
        )
        .bind(organization_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        row.0
    };

    let seed_duration = s012::seed(
        &migrator_pool,
        organization_id,
        stage_id,
        viewer_id,
        viewer_id,
    )
    .await;
    s012::vacuum_analyze(&migrator_pool).await;

    println!("SLICE_012_SEED_DURATION_MS {}", seed_duration.as_millis());
    println!("SLICE_012_PEOPLE {}", s012::PEOPLE);

    // Gate 1: paired regression.
    let rows = s012::gate1(&app_pool, organization_id, viewer_id).await;
    println!("SLICE_012_GATE1_START");
    for row in &rows {
        println!(
            "{}\t{}\tfrozen_p95_ms={}\tlive_p95_ms={}\tallowed_ms={}\twithin_allowed={}\tpayload_equal={}",
            row.statement,
            row.binding,
            row.frozen_p95.as_millis(),
            row.live_p95.as_millis(),
            row.allowed.as_millis(),
            row.within_allowed,
            row.payload_equal,
        );
        assert!(
            row.payload_equal,
            "{} ({}): frozen and live payloads must be equal",
            row.statement, row.binding
        );
        assert!(
            row.within_allowed,
            "{} ({}): live p95 {:?} exceeds allowed {:?} (frozen p95 {:?})",
            row.statement, row.binding, row.live_p95, row.allowed, row.frozen_p95
        );
    }
    println!("SLICE_012_GATE1_END");

    // Gate 2: EXPLAIN (ANALYZE, BUFFERS) through PREPARE/EXECUTE of the
    // exact .sqlx text, for filtered_summaries (never / not_within_days 7)
    // and person_state (canonical). Absolute, `CARGO_MANIFEST_DIR`-rooted
    // paths (not a `cwd`-relative guess): cargo's own test-runner
    // convention sets a test binary's process `cwd` to the PACKAGE
    // directory (`crates/crm-api/`), not the workspace root, which a bare
    // `"crates/crm-app/..."` relative path silently assumes.
    let filtered_summaries_sql = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../crm-app/src/domain/person/sql/filtered_summaries.sql"
    ))
    .unwrap();
    let person_state_sql = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../crm-app/src/domain/today/system_feeds/sql/person_state.sql"
    ))
    .unwrap();

    let mut tx = migrator_pool.begin().await.unwrap();
    sqlx::query(&format!("PREPARE s012_fs AS {filtered_summaries_sql}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(&format!("PREPARE s012_ps AS {person_state_sql}"))
        .execute(&mut *tx)
        .await
        .unwrap();

    // filtered_summaries: last_contact never.
    let sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS) EXECUTE s012_fs(\
         '{organization_id}', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, \
         NULL, NULL, true, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{viewer_id}', NULL, NULL)"
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql).fetch_all(&mut *tx).await.unwrap();
    let plan_never = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");

    // filtered_summaries: last_contact not_within_days 7.
    let sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS) EXECUTE s012_fs(\
         '{organization_id}', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, \
         NULL, 7, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{viewer_id}', NULL, NULL)"
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql).fetch_all(&mut *tx).await.unwrap();
    let plan_not_within_7 = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");

    // person_state: canonical feed definitions (assigned_to me +
    // awaiting_response for feed A, assigned_to me + client_replied for
    // feed B), enabled, 24h freshness both.
    let sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS) EXECUTE s012_ps(\
         '{organization_id}', \
         NULL, ARRAY['{viewer_id}']::uuid[], false, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, true, NULL, NULL, \
         NULL, ARRAY['{viewer_id}']::uuid[], false, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, true, NULL, \
         '2026-09-08 12:00:00+00', '{viewer_id}', true, true, 24, 24, NULL, NULL, NULL, NULL)"
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql).fetch_all(&mut *tx).await.unwrap();
    let plan_person_state = rows
        .into_iter()
        .map(|(l,)| l)
        .collect::<Vec<_>>()
        .join("\n");
    tx.rollback().await.unwrap();

    println!("SLICE_012_PLAN_FILTERED_SUMMARIES_NEVER_START");
    println!("{plan_never}");
    println!("SLICE_012_PLAN_FILTERED_SUMMARIES_NEVER_END");
    println!("SLICE_012_PLAN_FILTERED_SUMMARIES_NOT_WITHIN_7_START");
    println!("{plan_not_within_7}");
    println!("SLICE_012_PLAN_FILTERED_SUMMARIES_NOT_WITHIN_7_END");
    println!("SLICE_012_PLAN_PERSON_STATE_START");
    println!("{plan_person_state}");
    println!("SLICE_012_PLAN_PERSON_STATE_END");

    // Backfill block duration on this populated (25k-Person) fixture.
    const MIGRATION: &str = include_str!("../migrations/20260910000001_person_last_activity.sql");
    let block = {
        const BEGIN: &str = "-- BEGIN PERSON_LAST_ACTIVITY_BACKFILL";
        const END: &str = "-- END PERSON_LAST_ACTIVITY_BACKFILL";
        let start = MIGRATION.find(BEGIN).unwrap() + BEGIN.len();
        let rest = &MIGRATION[start..];
        let end = rest.find(END).unwrap();
        rest[..end].trim().to_string()
    };
    sqlx::query(
        "UPDATE person SET last_inquiry_at = NULL, last_contact_at = NULL, \
         last_inbound_at = NULL, last_outbound_at = NULL WHERE organization_id = $1",
    )
    .bind(organization_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let backfill_start = Instant::now();
    sqlx::query(&block).execute(&migrator_pool).await.unwrap();
    let backfill_duration = backfill_start.elapsed();
    println!(
        "SLICE_012_BACKFILL_DURATION_MS {} (over {} People)",
        backfill_duration.as_millis(),
        s012::PEOPLE
    );

    // Today HTTP serial p95 trend (report only, against the archived
    // 011d Feeds/concentrated/zero-source/serial figure of 177 ms at
    // 50,000 People — docs/design/perf/slice-011d-2026-09-07/README.md
    // Part 1).
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(
        &router,
        "viewer@slice-012-perf.test",
        "correct horse battery staple",
    )
    .await;
    let mut http_durations = Vec::with_capacity(SLICE_012_HTTP_SAMPLES);
    for _ in 0..SLICE_012_HTTP_SAMPLES {
        let start = Instant::now();
        let response = crate::common::get_with_cookie(&router, "/api/today", &cookie).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        http_durations.push(start.elapsed());
    }
    let http_p95 = today_http_perf_driver::nearest_rank_percentile(&http_durations, 0.95).unwrap();
    println!(
        "SLICE_012_TODAY_HTTP_SERIAL_P95_MS {} (011d Feeds concentrated/zero-source/serial trend: 177 ms at 50k People)",
        http_p95.as_millis()
    );
}

const SLICE_012_HTTP_SAMPLES: usize = 12;

async fn org_and_admin_named(pool: &PgPool, email: &str, org_name: &str) -> (Uuid, Uuid) {
    let (organization_id, user_id) = crate::common::create_org_with_stages_and_member(
        pool,
        org_name,
        email,
        "Slice 012 Perf Viewer",
        "correct horse battery staple",
    )
    .await;
    (organization_id, user_id)
}

//! Phase B authenticated HTTP performance harness for Slice 011c.
//!
//! This ignored test is deliberately separate from the pure workload driver.
//! The driver owns barrier scheduling, sample accounting, and normal-completion
//! gates; this file owns the isolated real-HTTP application wiring, complete
//! response-body classification, and the safe raw evidence shape. The primary
//! implementation lane owns fixture construction, database lifecycle, test
//! registration, and execution.

#[path = "common/mod.rs"]
mod common;

#[path = "fixtures/today_http_perf_driver.rs"]
mod today_http_perf_driver;

#[path = "fixtures/today_http_perf_fixture.rs"]
mod today_http_perf_fixture;

#[path = "fixtures/today_9d62e86/mod.rs"]
mod today_9d62e86;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use crm_api::auth::AuthContext;
use crm_api::config::Config;
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::today::test_support::{
    HttpPerfCaptureTerminal as CapturedHttpPerfCaptureTerminal, HttpPerfCollector,
    HttpPerfTelemetry, PoolAcquisition, PoolAcquisitionOutcome as CapturedPoolAcquisitionOutcome,
    SourceEnumeration, SourceEnumerationOutcome as CapturedSourceEnumerationOutcome,
    SourceEvaluation, SourceEvaluationOutcome as CapturedSourceEvaluationOutcome,
};
use crm_api::error::ApiError;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use futures_util::FutureExt;
use reqwest::header::{COOKIE, SET_COOKIE};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use today_http_perf_driver::{
    AttemptId, AttemptIds, AttemptOutcome, AttemptPhase, AttemptRecord, FinalMatrixCase,
    NormalCompletionGate, NormalCompletionReport, QueryArm, SampleShape, TimingSummary,
};
use today_http_perf_fixture::{PerfSourceMode, TodayHttpPerfCase};

/// A static series label retained on every wrapper around a driver wave. It
/// is intentionally independent from the case: the independent repeat must
/// not be merged into the ordinary final matrix merely because both exercise
/// the concentrated five-source configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum HttpPerformanceSeries {
    FinalMatrix,
    IndependentConcentratedRepeat,
    PairedOriginalZeroSource,
}

/// Static, non-identifying case labels are kept outside per-response metadata
/// so a failed join remains attributable in the retained raw evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum HttpPerformanceCase {
    ConcentratedZeroSources,
    ConcentratedOneDenseSource,
    ConcentratedOneAbsenceSource,
    ConcentratedFiveOverlappingSources,
    TypicalZeroSources,
    TypicalFiveOverlappingSources,
    PartialBuiltinsZeroSources,
    PartialBuiltinsFiveOverlappingSources,
    EmptyBuiltinsZeroSources,
    EmptyBuiltinsFiveOverlappingSources,
}

impl From<FinalMatrixCase> for HttpPerformanceCase {
    fn from(case: FinalMatrixCase) -> Self {
        match case {
            FinalMatrixCase::ConcentratedZeroSources => Self::ConcentratedZeroSources,
            FinalMatrixCase::ConcentratedOneDenseSource => Self::ConcentratedOneDenseSource,
            FinalMatrixCase::ConcentratedOneAbsenceSource => Self::ConcentratedOneAbsenceSource,
            FinalMatrixCase::ConcentratedFiveOverlappingSources => {
                Self::ConcentratedFiveOverlappingSources
            }
            FinalMatrixCase::TypicalZeroSources => Self::TypicalZeroSources,
            FinalMatrixCase::TypicalFiveOverlappingSources => Self::TypicalFiveOverlappingSources,
            FinalMatrixCase::PartialBuiltinsZeroSources => Self::PartialBuiltinsZeroSources,
            FinalMatrixCase::PartialBuiltinsFiveOverlappingSources => {
                Self::PartialBuiltinsFiveOverlappingSources
            }
            FinalMatrixCase::EmptyBuiltinsZeroSources => Self::EmptyBuiltinsZeroSources,
            FinalMatrixCase::EmptyBuiltinsFiveOverlappingSources => {
                Self::EmptyBuiltinsFiveOverlappingSources
            }
        }
    }
}

fn fixture_case(case: HttpPerformanceCase) -> TodayHttpPerfCase {
    match case {
        HttpPerformanceCase::ConcentratedZeroSources
        | HttpPerformanceCase::ConcentratedOneDenseSource
        | HttpPerformanceCase::ConcentratedOneAbsenceSource
        | HttpPerformanceCase::ConcentratedFiveOverlappingSources => {
            TodayHttpPerfCase::Concentrated
        }
        HttpPerformanceCase::TypicalZeroSources
        | HttpPerformanceCase::TypicalFiveOverlappingSources => TodayHttpPerfCase::Typical,
        HttpPerformanceCase::PartialBuiltinsZeroSources
        | HttpPerformanceCase::PartialBuiltinsFiveOverlappingSources => {
            TodayHttpPerfCase::PartialBuiltins
        }
        HttpPerformanceCase::EmptyBuiltinsZeroSources
        | HttpPerformanceCase::EmptyBuiltinsFiveOverlappingSources => {
            TodayHttpPerfCase::EmptyBuiltins
        }
    }
}

fn source_mode(plan: SeriesPlan) -> PerfSourceMode {
    if plan.arm == QueryArm::FrozenOriginal {
        return PerfSourceMode::Zero;
    }
    match plan.case {
        HttpPerformanceCase::ConcentratedZeroSources
        | HttpPerformanceCase::TypicalZeroSources
        | HttpPerformanceCase::PartialBuiltinsZeroSources
        | HttpPerformanceCase::EmptyBuiltinsZeroSources => PerfSourceMode::Zero,
        HttpPerformanceCase::ConcentratedOneDenseSource => PerfSourceMode::OneDense,
        HttpPerformanceCase::ConcentratedOneAbsenceSource => PerfSourceMode::OneAbsence,
        HttpPerformanceCase::ConcentratedFiveOverlappingSources
        | HttpPerformanceCase::TypicalFiveOverlappingSources
        | HttpPerformanceCase::PartialBuiltinsFiveOverlappingSources
        | HttpPerformanceCase::EmptyBuiltinsFiveOverlappingSources => PerfSourceMode::FiveOverlap,
    }
}

/// The source envelope is read only after reqwest has consumed the full body.
/// This is distinct from the driver's normal-completion result: the latter
/// gates HTTP 200 plus `Complete`, while this field records the exact safe
/// response classification retained for every response status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum FullBodyClassification {
    FinalComplete,
    FinalPartial,
    FinalUnavailable,
    FrozenOriginalComplete,
    ErrorEnvelope,
    InvalidJson,
    InvalidTodayEnvelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum TodaySourceStatus {
    Complete,
    Partial,
    Unavailable,
}

/// The application collector records pool acquisition at each real request
/// boundary. Keeping unsuccessful acquisition events makes the evidence
/// honest about waits that end in timeout or another unavailable outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum PoolAcquisitionOutcome {
    Acquired,
    TimedOut,
    Failed,
}

/// Terminal capture state is retained for client failures so a bounded late
/// telemetry drain can never masquerade as a response whose complete body was
/// consumed. It contains no request or database detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaptureTerminal {
    CompleteBody,
    ClientFailureQuiescent,
    ClientFailureDrainTimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PoolAcquisitionEvent {
    outcome: PoolAcquisitionOutcome,
    duration: Duration,
}

/// Every configured source evaluation is retained, including a timeout or
/// failed validation/query. Names, IDs, filter values and database errors are
/// deliberately absent; the position/count/order are sufficient to check the
/// declared five-source matrix and timing budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum WholeSourceOutcome {
    Complete,
    InvalidFilter,
    Unavailable,
    TimedOut,
}

/// Filter values never leave the request-local collector. The fixed v1 kind
/// vocabulary is enough to prove the intended source shapes and that useful
/// safe telemetry survives the sentinel checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum StaticFilterKind {
    Stage,
    AssignedTo,
    Source,
    Created,
    LastInquiry,
    LastContact,
    LastInbound,
    HasReplied,
    HasPhone,
    HasEmail,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WholeSourceEvent {
    outcome: WholeSourceOutcome,
    duration: Duration,
    filter_kinds: Vec<StaticFilterKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum SourceEnumerationOutcome {
    Complete,
    Unavailable,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SourceEnumerationEvent {
    outcome: SourceEnumerationOutcome,
    duration: Duration,
}

/// Safe fields attached to every fully accounted HTTP attempt. There is no
/// URL, session cookie, password, Organization/user/list identifier, filter,
/// response body, or database error in this record. `body_sha256` covers the
/// complete response bytes; `normalized_today_sha256` removes only the final
/// source envelope and supports zero-source frozen-original parity checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HttpAttemptMetadata {
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    body_classification: Option<FullBodyClassification>,
    body_sha256: Option<String>,
    normalized_today_sha256: Option<String>,
    /// Used only for in-process exact parity comparison. It is deliberately
    /// skipped from the retained raw artifact so Person/list content cannot
    /// leak merely because a benchmark ran.
    #[serde(skip)]
    normalized_today_body: Option<Value>,
    source_status: Option<TodaySourceStatus>,
    capture_terminal: CaptureTerminal,
    initial_capture_terminal: Option<CaptureTerminal>,
    /// Process-local association used only to reconcile an initial bounded
    /// client-failure drain after the arm server has quiesced. Capture IDs
    /// never enter the retained artifact.
    #[serde(skip)]
    pending_client_failure_capture_id: Option<u64>,
    source_enumeration: Vec<SourceEnumerationEvent>,
    whole_source_evaluation_count: usize,
    source_evaluations: Vec<WholeSourceEvent>,
    authentication_pool_acquisitions: Vec<PoolAcquisitionEvent>,
    feed_pool_acquisitions: Vec<PoolAcquisitionEvent>,
}

/// Authentication happens before each retained measured series and remains a
/// first-class part of the evidence. It stays out of the request-wave driver
/// because a single real login supplies the actual reqwest session cookie for
/// all of that series' feed attempts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AuthenticationEvidence {
    status: Option<u16>,
    body_classification: AuthenticationBodyClassification,
    body_sha256: Option<String>,
    pool_acquisitions: Vec<PoolAcquisitionEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum AuthenticationBodyClassification {
    NotAttempted,
    CompleteSession,
    ErrorEnvelope,
    InvalidJson,
    InvalidSessionEnvelope,
    TransportFailure,
    BodyReadFailure,
}

/// Configuration is a real authenticated HTTP setup step before each series.
/// Its safe status is retained so a setup panic/failure cannot discard earlier
/// measured waves or be mistaken for an omitted series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum SourceConfigurationOutcome {
    Complete,
    DefinitionCountMismatch,
    CommandFailed,
}

/// A raw wave wrapper deliberately duplicates the static case/series labels
/// beside every `WaveCapture`; the independent recount tool can then retain
/// failed joins and still group the complete predeclared matrix correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RawWave<M> {
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    wave: today_http_perf_driver::WaveCapture<M>,
}

/// The only body material retained beyond the request itself is its digest.
/// `serde_json::Value` uses deterministic map ordering when serialized, so
/// the normalized digest is stable after removal of the one declared final
/// response addition (`sources`).
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClassifiedBody {
    classification: FullBodyClassification,
    driver_result: today_http_perf_driver::HttpBodyResult,
    body_sha256: String,
    normalized_today_sha256: Option<String>,
    normalized_today_body: Option<Value>,
    source_status: Option<TodaySourceStatus>,
}

fn is_today_payload(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("generated_at").is_some_and(Value::is_string)
        && object.get("items").is_some_and(Value::is_array)
        && object.get("truncated").is_some_and(Value::is_boolean)
}

/// Classify only after full response-body consumption. A final response has
/// the declared source envelope; the frozen original arm deliberately does
/// not. An error response remains visible as an error envelope even though it
/// fails the driver's normal 200/complete gate.
fn classify_full_body(arm: QueryArm, body: &[u8]) -> ClassifiedBody {
    let body_sha256 = sha256_hex(body);
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return ClassifiedBody {
            classification: FullBodyClassification::InvalidJson,
            driver_result: today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
            body_sha256,
            normalized_today_sha256: None,
            normalized_today_body: None,
            source_status: None,
        };
    };

    if value
        .as_object()
        .and_then(|object| object.get("error"))
        .is_some_and(Value::is_string)
    {
        return ClassifiedBody {
            classification: FullBodyClassification::ErrorEnvelope,
            driver_result: today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
            body_sha256,
            normalized_today_sha256: None,
            normalized_today_body: None,
            source_status: None,
        };
    }

    if !is_today_payload(&value) {
        return ClassifiedBody {
            classification: FullBodyClassification::InvalidTodayEnvelope,
            driver_result: today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
            body_sha256,
            normalized_today_sha256: None,
            normalized_today_body: None,
            source_status: None,
        };
    }

    let object = value
        .as_object_mut()
        .expect("is_today_payload already established an object");
    let (classification, driver_result, source_status) = match arm {
        QueryArm::FrozenOriginal => {
            if object.contains_key("sources") {
                (
                    FullBodyClassification::InvalidTodayEnvelope,
                    today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
                    None,
                )
            } else {
                (
                    FullBodyClassification::FrozenOriginalComplete,
                    today_http_perf_driver::HttpBodyResult::Complete,
                    None,
                )
            }
        }
        QueryArm::Final => match object.remove("sources") {
            Some(Value::Object(sources)) if sources.get("issues").is_some_and(Value::is_array) => {
                let issues_empty = sources
                    .get("issues")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty);
                match sources.get("status").and_then(Value::as_str) {
                    Some("complete") if issues_empty => (
                        FullBodyClassification::FinalComplete,
                        today_http_perf_driver::HttpBodyResult::Complete,
                        Some(TodaySourceStatus::Complete),
                    ),
                    // A `complete` source envelope with issues is internally
                    // contradictory. Preserve the body hash but never let it
                    // satisfy the driver's normal-complete gate.
                    Some("complete") => (
                        FullBodyClassification::InvalidTodayEnvelope,
                        today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
                        None,
                    ),
                    Some("partial") => (
                        FullBodyClassification::FinalPartial,
                        today_http_perf_driver::HttpBodyResult::Partial,
                        Some(TodaySourceStatus::Partial),
                    ),
                    Some("unavailable") => (
                        FullBodyClassification::FinalUnavailable,
                        today_http_perf_driver::HttpBodyResult::Unavailable,
                        Some(TodaySourceStatus::Unavailable),
                    ),
                    _ => (
                        FullBodyClassification::InvalidTodayEnvelope,
                        today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
                        None,
                    ),
                }
            }
            _ => (
                FullBodyClassification::InvalidTodayEnvelope,
                today_http_perf_driver::HttpBodyResult::InvalidEnvelope,
                None,
            ),
        },
    };

    let normalized_today_sha256 = serde_json::to_vec(&value)
        .ok()
        .map(|normalized| sha256_hex(&normalized));
    ClassifiedBody {
        classification,
        driver_result,
        body_sha256,
        normalized_today_sha256,
        normalized_today_body: Some(value),
        source_status,
    }
}

fn pool_acquisition_event(event: PoolAcquisition) -> PoolAcquisitionEvent {
    PoolAcquisitionEvent {
        outcome: match event.outcome {
            CapturedPoolAcquisitionOutcome::Acquired => PoolAcquisitionOutcome::Acquired,
            CapturedPoolAcquisitionOutcome::TimedOut => PoolAcquisitionOutcome::TimedOut,
            CapturedPoolAcquisitionOutcome::Failed => PoolAcquisitionOutcome::Failed,
        },
        duration: event.duration,
    }
}

fn source_enumeration_event(event: SourceEnumeration) -> SourceEnumerationEvent {
    SourceEnumerationEvent {
        outcome: match event.outcome {
            CapturedSourceEnumerationOutcome::Complete => SourceEnumerationOutcome::Complete,
            CapturedSourceEnumerationOutcome::Unavailable => SourceEnumerationOutcome::Unavailable,
            CapturedSourceEnumerationOutcome::TimedOut => SourceEnumerationOutcome::TimedOut,
        },
        duration: event.duration,
    }
}

fn static_filter_kind(kind: &'static str) -> StaticFilterKind {
    match kind {
        "stage" => StaticFilterKind::Stage,
        "assigned_to" => StaticFilterKind::AssignedTo,
        "source" => StaticFilterKind::Source,
        "created" => StaticFilterKind::Created,
        "last_inquiry" => StaticFilterKind::LastInquiry,
        "last_contact" => StaticFilterKind::LastContact,
        "last_inbound" => StaticFilterKind::LastInbound,
        "has_replied" => StaticFilterKind::HasReplied,
        "has_phone" => StaticFilterKind::HasPhone,
        "has_email" => StaticFilterKind::HasEmail,
        _ => StaticFilterKind::Unknown,
    }
}

fn source_evaluation_event(event: SourceEvaluation) -> WholeSourceEvent {
    WholeSourceEvent {
        outcome: match event.outcome {
            CapturedSourceEvaluationOutcome::Complete => WholeSourceOutcome::Complete,
            CapturedSourceEvaluationOutcome::InvalidFilter => WholeSourceOutcome::InvalidFilter,
            CapturedSourceEvaluationOutcome::Unavailable => WholeSourceOutcome::Unavailable,
            CapturedSourceEvaluationOutcome::TimedOut => WholeSourceOutcome::TimedOut,
        },
        duration: event.duration,
        filter_kinds: event
            .filter_kinds
            .into_iter()
            .map(static_filter_kind)
            .collect(),
    }
}

fn capture_terminal(terminal: CapturedHttpPerfCaptureTerminal) -> CaptureTerminal {
    match terminal {
        CapturedHttpPerfCaptureTerminal::CompleteBody => CaptureTerminal::CompleteBody,
        CapturedHttpPerfCaptureTerminal::ClientFailureQuiescent => {
            CaptureTerminal::ClientFailureQuiescent
        }
        CapturedHttpPerfCaptureTerminal::ClientFailureDrainTimedOut => {
            CaptureTerminal::ClientFailureDrainTimedOut
        }
    }
}

fn attempt_metadata(
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    classified_body: Option<ClassifiedBody>,
    telemetry: HttpPerfTelemetry,
) -> HttpAttemptMetadata {
    let (
        body_classification,
        body_sha256,
        normalized_today_sha256,
        normalized_today_body,
        source_status,
    ) = match classified_body {
        Some(body) => (
            Some(body.classification),
            Some(body.body_sha256),
            body.normalized_today_sha256,
            body.normalized_today_body,
            body.source_status,
        ),
        None => (None, None, None, None, None),
    };
    let mut metadata = HttpAttemptMetadata {
        case,
        series,
        body_classification,
        body_sha256,
        normalized_today_sha256,
        normalized_today_body,
        source_status,
        capture_terminal: CaptureTerminal::CompleteBody,
        initial_capture_terminal: None,
        pending_client_failure_capture_id: None,
        source_enumeration: Vec::new(),
        whole_source_evaluation_count: 0,
        source_evaluations: Vec::new(),
        authentication_pool_acquisitions: Vec::new(),
        feed_pool_acquisitions: Vec::new(),
    };
    apply_telemetry(&mut metadata, telemetry);
    metadata
}

fn apply_telemetry(metadata: &mut HttpAttemptMetadata, telemetry: HttpPerfTelemetry) {
    metadata.capture_terminal = capture_terminal(telemetry.terminal);
    metadata.source_enumeration = telemetry
        .source_enumeration
        .into_iter()
        .map(source_enumeration_event)
        .collect();
    metadata.source_evaluations = telemetry
        .source_evaluations
        .into_iter()
        .map(source_evaluation_event)
        .collect();
    metadata.whole_source_evaluation_count = metadata.source_evaluations.len();
    metadata.authentication_pool_acquisitions = telemetry
        .authentication_pool_acquisitions
        .into_iter()
        .map(pool_acquisition_event)
        .collect();
    metadata.feed_pool_acquisitions = telemetry
        .feed_pool_acquisitions
        .into_iter()
        .map(pool_acquisition_event)
        .collect();
}

fn retain_pending_late_capture(metadata: &mut HttpAttemptMetadata, capture_id: u64) {
    metadata.initial_capture_terminal = Some(metadata.capture_terminal);
    if metadata.capture_terminal == CaptureTerminal::ClientFailureDrainTimedOut {
        metadata.pending_client_failure_capture_id = Some(capture_id);
    }
}

/// The test-only frozen adapter deliberately replaces only `/api/today`.
/// `build_app_with_today_router` supplies all normal session/auth/request
/// middleware, so the baseline goes through the same real HTTP stack as the
/// final arm. The fixed clock is captured in this closure at router creation;
/// no client request can choose it.
fn frozen_original_today_router(now: DateTime<Utc>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/today",
            get(move |state: State<AppState>, auth: AuthContext| {
                let now = now;
                async move { frozen_original_today_at(state, auth, now).await }
            }),
        )
        // The frozen arm replaces only the list GET. Source setup remains on
        // the normal authenticated command/read routes before every series.
        .merge(crm_api::routes::today::source_control_router())
}

/// Build each arm with its own ten-connection application pool and the same
/// test-only app stack. The caller starts/stops these routers sequentially;
/// a final and frozen-original pool are never combined in one load run.
fn final_arm_router(
    app_pool: PgPool,
    config: &Config,
    now: DateTime<Utc>,
    collector: HttpPerfCollector,
) -> Router {
    let state = AppState::for_tests(app_pool, config, Publisher::recording());
    crm_api::build_app_with_today_router_and_perf_collector(
        state,
        crm_api::routes::today::router_with_test_clock(now),
        collector,
    )
}

fn frozen_original_arm_router(
    app_pool: PgPool,
    config: &Config,
    now: DateTime<Utc>,
    collector: HttpPerfCollector,
) -> Router {
    let state = AppState::for_tests(app_pool, config, Publisher::recording());
    crm_api::build_app_with_today_router_and_perf_collector(
        state,
        frozen_original_today_router(now),
        collector,
    )
}

async fn frozen_original_today_at(
    State(state): State<AppState>,
    auth: AuthContext,
    now: DateTime<Utc>,
) -> Result<Json<today_9d62e86::TodayList>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let acquisition_started = Instant::now();
    let mut connection = match pool.acquire().await {
        Ok(connection) => {
            crm_api::domain::today::test_support::record_feed_pool_acquisition(
                CapturedPoolAcquisitionOutcome::Acquired,
                acquisition_started.elapsed(),
            );
            connection
        }
        Err(error) => {
            crm_api::domain::today::test_support::record_feed_pool_acquisition(
                if matches!(&error, sqlx::Error::PoolTimedOut) {
                    CapturedPoolAcquisitionOutcome::TimedOut
                } else {
                    CapturedPoolAcquisitionOutcome::Failed
                },
                acquisition_started.elapsed(),
            );
            return Err(ApiError::Unavailable);
        }
    };
    let scope = PersonVisibilityScope::from_auth(&auth);
    let list = today_9d62e86::query(&mut connection, &scope, auth.actor_user_id, now)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    Ok(Json(list))
}

/// A real loopback listener keeps request dispatch, session middleware,
/// connection pooling, serialization, reqwest transport and full-body reads
/// inside the measured path. Each arm gets its own listener/application pool
/// and is shut down before the other one starts.
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
        (&mut self.task)
            .await
            .expect("isolated loopback server task");
    }
}

impl Drop for LoopbackServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

/// Request capture IDs are numerical, independent from driver attempt IDs,
/// and never serialized. This lets a real login and every concurrent GET
/// retrieve only its own server-side safe telemetry after body consumption.
#[derive(Clone)]
struct CaptureIds(Arc<AtomicU64>);

impl Default for CaptureIds {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(1)))
    }
}

impl CaptureIds {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

/// Driver attempt IDs are preassigned before a wave starts, so the GET
/// capture identity is equally preassigned and remains recoverable even if a
/// spawned request task later joins as cancelled or panicked. Login captures
/// occupy the small monotonic range; this disjoint offset is never serialized.
const GET_CAPTURE_ID_OFFSET: u64 = 1_000_000;

fn capture_id_for_attempt(context: today_http_perf_driver::AttemptContext) -> u64 {
    context
        .id
        .0
        .checked_add(GET_CAPTURE_ID_OFFSET)
        .expect("declared Phase B attempt IDs stay below capture offset headroom")
}

#[derive(Clone)]
struct AuthenticatedSession {
    client: reqwest::Client,
    cookie: String,
}

/// Login is a real HTTP request against the isolated router. The returned
/// `Set-Cookie` value is retained only in memory and sent by reqwest on the
/// subsequent GETs; it never enters the raw evidence artifact or assertion
/// output.
async fn authenticate_session(
    client: reqwest::Client,
    base_url: &str,
    email: &str,
    password: &str,
    collector: &HttpPerfCollector,
    capture_ids: &CaptureIds,
) -> (Option<AuthenticatedSession>, AuthenticationEvidence) {
    let capture_id = capture_ids.next();
    collector.begin_capture(capture_id);
    let response = client
        .post(format!("{base_url}/api/session"))
        .header("x-crm-perf-capture", capture_id.to_string())
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await;
    let evidence_from_transport_failure = || AuthenticationEvidence {
        status: None,
        body_classification: AuthenticationBodyClassification::TransportFailure,
        body_sha256: None,
        pool_acquisitions: collector
            .take(capture_id)
            .authentication_pool_acquisitions
            .into_iter()
            .map(pool_acquisition_event)
            .collect(),
    };
    let Ok(response) = response else {
        return (None, evidence_from_transport_failure());
    };
    let status = response.status().as_u16();
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::to_owned);
    let bytes = response.bytes().await;
    let telemetry = collector.take(capture_id);
    let pool_acquisitions = telemetry
        .authentication_pool_acquisitions
        .into_iter()
        .map(pool_acquisition_event)
        .collect();
    let Ok(bytes) = bytes else {
        return (
            None,
            AuthenticationEvidence {
                status: Some(status),
                body_classification: AuthenticationBodyClassification::BodyReadFailure,
                body_sha256: None,
                pool_acquisitions,
            },
        );
    };
    let body_sha256 = Some(sha256_hex(&bytes));
    let classification = match serde_json::from_slice::<Value>(&bytes) {
        Ok(value)
            if status == today_http_perf_driver::HTTP_OK
                && cookie.is_some()
                && value.get("user").is_some_and(Value::is_object)
                && value.get("organization").is_some()
                && value.get("platform_admin").is_some_and(Value::is_boolean) =>
        {
            AuthenticationBodyClassification::CompleteSession
        }
        Ok(value) if value.get("error").is_some_and(Value::is_string) => {
            AuthenticationBodyClassification::ErrorEnvelope
        }
        Ok(_) => AuthenticationBodyClassification::InvalidSessionEnvelope,
        Err(_) => AuthenticationBodyClassification::InvalidJson,
    };
    let evidence = AuthenticationEvidence {
        status: Some(status),
        body_classification: classification,
        body_sha256,
        pool_acquisitions,
    };
    let session =
        (classification == AuthenticationBodyClassification::CompleteSession).then(|| {
            AuthenticatedSession {
                client,
                cookie: cookie.expect("complete session classification requires a cookie"),
            }
        });
    (session, evidence)
}

// The Err carries the complete safe attempt record by design; boxing would add nothing.
#[allow(clippy::result_large_err)]
async fn dispatch_today(
    session: AuthenticatedSession,
    base_url: String,
    arm: QueryArm,
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    collector: HttpPerfCollector,
    capture_id: u64,
) -> Result<
    today_http_perf_driver::CompletedHttpResponse<HttpAttemptMetadata>,
    today_http_perf_driver::ClientFailure<HttpAttemptMetadata>,
> {
    collector.begin_capture(capture_id);
    let response = session
        .client
        .get(format!("{base_url}/api/today"))
        .header(COOKIE, session.cookie)
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
            let mut metadata = attempt_metadata(case, series, None, telemetry);
            retain_pending_late_capture(&mut metadata, capture_id);
            return Err(today_http_perf_driver::ClientFailure {
                kind: if error.is_timeout() {
                    today_http_perf_driver::ClientFailureKind::RequestTimeout
                } else {
                    today_http_perf_driver::ClientFailureKind::Transport
                },
                request_completed_at,
                metadata,
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
        return Err(today_http_perf_driver::ClientFailure {
            kind: today_http_perf_driver::ClientFailureKind::BodyRead,
            request_completed_at,
            metadata: {
                let mut metadata = attempt_metadata(case, series, None, telemetry);
                retain_pending_late_capture(&mut metadata, capture_id);
                metadata
            },
        });
    };
    let telemetry = collector.take(capture_id);
    let classified = classify_full_body(arm, &bytes);
    let metadata = attempt_metadata(case, series, Some(classified.clone()), telemetry);
    Ok(today_http_perf_driver::CompletedHttpResponse {
        status,
        result: classified.driver_result,
        whole_source_evaluations: metadata.whole_source_evaluation_count,
        request_completed_at,
        metadata,
    })
}

/// A complete retained series. The warm-up and measured wave vectors are
/// separate by construction, which prevents an acceptance percentile from
/// accidentally including the 900 warm-up requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SeriesEvidence {
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    arm: QueryArm,
    authentication: AuthenticationEvidence,
    source_configuration: SourceConfigurationOutcome,
    warmups: Vec<RawWave<HttpAttemptMetadata>>,
    measured: Vec<RawWave<HttpAttemptMetadata>>,
    summary: SeriesSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LoadShapeSummary {
    request_timing: TimingSummary,
    request_p95_limit: Option<Duration>,
    p95_within_limit: bool,
}

/// This report is written alongside raw waves. It does not discard failures:
/// normal completion, missing/upper-bound timing, and p95 limits stay visible
/// even when the series has already failed another acceptance condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SeriesSummary {
    normal_completion: NormalCompletionReport,
    telemetry_coverage: TelemetryCoverage,
    warmup_normal_completion: NormalCompletionReport,
    warmup_telemetry_coverage: TelemetryCoverage,
    request_by_concurrency: BTreeMap<usize, LoadShapeSummary>,
    whole_source_timing: TimingSummary,
    whole_source_p95_within_limit: bool,
    whole_source_max_within_budget: bool,
    source_enumeration_timing: TimingSummary,
    source_enumeration_max_within_budget: bool,
    authentication_pool_acquisition_timing: TimingSummary,
    authentication_pool_headroom_to_two_seconds: Option<Duration>,
    feed_pool_acquisition_timing: TimingSummary,
    feed_pool_headroom_to_two_seconds: Option<Duration>,
}

/// Presence/count checks complement timing summaries. A missing collector
/// event is never turned into a fast-looking zero; it is retained and makes
/// the normal series ineligible for acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TelemetryCoverage {
    expected_get_attempts: usize,
    attempts_with_metadata: usize,
    attempts_with_authentication_pool_acquisition: usize,
    attempts_with_feed_pool_acquisition: usize,
    expected_source_enumeration_events: usize,
    recorded_source_enumeration_events: usize,
    expected_whole_source_events: usize,
    recorded_whole_source_events: usize,
    typed_violations: Vec<TelemetryViolation>,
}

/// Every event remains in the raw attempt metadata. These safe, typed
/// violations make an apparently complete HTTP body ineligible when the
/// request-local collector says a pool or source operation actually failed,
/// was duplicated, or was not the declared source shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum TelemetryViolation {
    MissingMetadata {
        attempt: AttemptId,
    },
    AuthenticationPoolAcquisitionCount {
        attempt: AttemptId,
        actual: usize,
    },
    AuthenticationPoolAcquisitionOutcome {
        attempt: AttemptId,
        outcome: PoolAcquisitionOutcome,
    },
    FeedPoolAcquisitionCount {
        attempt: AttemptId,
        actual: usize,
    },
    FeedPoolAcquisitionOutcome {
        attempt: AttemptId,
        outcome: PoolAcquisitionOutcome,
    },
    SourceEnumerationCount {
        attempt: AttemptId,
        expected: usize,
        actual: usize,
    },
    SourceEnumerationOutcome {
        attempt: AttemptId,
        outcome: SourceEnumerationOutcome,
    },
    WholeSourceEvaluationCount {
        attempt: AttemptId,
        expected: usize,
        actual: usize,
    },
    WholeSourceEvaluationOutcome {
        attempt: AttemptId,
        outcome: WholeSourceOutcome,
    },
    WholeSourceWithoutStaticKinds {
        attempt: AttemptId,
    },
    UnknownStaticFilterKind {
        attempt: AttemptId,
    },
    IncompleteCaptureTerminal {
        attempt: AttemptId,
        terminal: CaptureTerminal,
    },
}

/// Static execution plan for one of the predeclared cases. The independent
/// repeat uses its own series and shape, even though it shares a case label
/// with a final-matrix row.
#[derive(Debug, Clone, Copy)]
struct SeriesPlan {
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    arm: QueryArm,
    shape: SampleShape,
    expected_sources_per_attempt: usize,
}

impl SeriesPlan {
    const fn final_matrix(case: FinalMatrixCase) -> Self {
        Self {
            case: match case {
                FinalMatrixCase::ConcentratedZeroSources => {
                    HttpPerformanceCase::ConcentratedZeroSources
                }
                FinalMatrixCase::ConcentratedOneDenseSource => {
                    HttpPerformanceCase::ConcentratedOneDenseSource
                }
                FinalMatrixCase::ConcentratedOneAbsenceSource => {
                    HttpPerformanceCase::ConcentratedOneAbsenceSource
                }
                FinalMatrixCase::ConcentratedFiveOverlappingSources => {
                    HttpPerformanceCase::ConcentratedFiveOverlappingSources
                }
                FinalMatrixCase::TypicalZeroSources => HttpPerformanceCase::TypicalZeroSources,
                FinalMatrixCase::TypicalFiveOverlappingSources => {
                    HttpPerformanceCase::TypicalFiveOverlappingSources
                }
                FinalMatrixCase::PartialBuiltinsZeroSources => {
                    HttpPerformanceCase::PartialBuiltinsZeroSources
                }
                FinalMatrixCase::PartialBuiltinsFiveOverlappingSources => {
                    HttpPerformanceCase::PartialBuiltinsFiveOverlappingSources
                }
                FinalMatrixCase::EmptyBuiltinsZeroSources => {
                    HttpPerformanceCase::EmptyBuiltinsZeroSources
                }
                FinalMatrixCase::EmptyBuiltinsFiveOverlappingSources => {
                    HttpPerformanceCase::EmptyBuiltinsFiveOverlappingSources
                }
            },
            series: HttpPerformanceSeries::FinalMatrix,
            arm: QueryArm::Final,
            shape: case.sample_shape(),
            expected_sources_per_attempt: case.expected_sources_per_attempt(),
        }
    }

    const fn independent_concentrated_repeat() -> Self {
        Self {
            case: HttpPerformanceCase::ConcentratedFiveOverlappingSources,
            series: HttpPerformanceSeries::IndependentConcentratedRepeat,
            arm: QueryArm::Final,
            shape: SampleShape::new(0, 0, 2),
            expected_sources_per_attempt: 5,
        }
    }

    const fn paired_original_zero_source(case: HttpPerformanceCase, shape: SampleShape) -> Self {
        Self {
            case,
            series: HttpPerformanceSeries::PairedOriginalZeroSource,
            arm: QueryArm::FrozenOriginal,
            shape,
            expected_sources_per_attempt: 0,
        }
    }

    const fn expected_measured_attempts(self) -> usize {
        self.shape.attempts()
    }

    const fn expected_whole_source_evaluations(self) -> usize {
        self.expected_measured_attempts() * self.expected_sources_per_attempt
    }
}

fn final_matrix_plans() -> Vec<SeriesPlan> {
    FinalMatrixCase::CRITICAL
        .into_iter()
        .chain(FinalMatrixCase::SUPPLEMENTAL)
        .map(SeriesPlan::final_matrix)
        .collect()
}

fn paired_original_zero_source_plans() -> [SeriesPlan; 4] {
    [
        SeriesPlan::paired_original_zero_source(
            HttpPerformanceCase::ConcentratedZeroSources,
            today_http_perf_driver::CRITICAL_SAMPLE_SHAPE,
        ),
        SeriesPlan::paired_original_zero_source(
            HttpPerformanceCase::TypicalZeroSources,
            today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
        ),
        SeriesPlan::paired_original_zero_source(
            HttpPerformanceCase::PartialBuiltinsZeroSources,
            today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
        ),
        SeriesPlan::paired_original_zero_source(
            HttpPerformanceCase::EmptyBuiltinsZeroSources,
            today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
        ),
    ]
}

/// The matrix is stated as code as well as prose. Any accidental row removal,
/// shape change or source-count change fails before an execution can label a
/// reduced workload as the approved Phase B run.
fn assert_predeclared_matrix() {
    let final_plans = final_matrix_plans();
    assert_eq!(
        final_plans.len(),
        10,
        "the final matrix has every declared case"
    );
    let final_measured = final_plans
        .iter()
        .map(|plan| plan.expected_measured_attempts())
        .sum::<usize>()
        + SeriesPlan::independent_concentrated_repeat().expected_measured_attempts();
    assert_eq!(
        final_measured,
        today_http_perf_driver::FINAL_ARM_MEASURED_ATTEMPTS,
        "the final matrix plus independent repeat retains 522 measured requests",
    );
    let final_source_evaluations = final_plans
        .iter()
        .map(|plan| plan.expected_whole_source_evaluations())
        .sum::<usize>()
        + SeriesPlan::independent_concentrated_repeat().expected_whole_source_evaluations();
    assert_eq!(
        final_source_evaluations, 1_201,
        "the final matrix plus independent repeat retains every expected source evaluation",
    );
    let original_measured = paired_original_zero_source_plans()
        .into_iter()
        .map(|plan| plan.expected_measured_attempts())
        .sum::<usize>();
    assert_eq!(
        original_measured,
        today_http_perf_driver::PAIRED_ORIGINAL_MEASURED_ATTEMPTS,
        "all four paired frozen-original zero-source series retain their matching shapes",
    );
    assert_eq!(
        (final_plans.len() + 1 + paired_original_zero_source_plans().len())
            * today_http_perf_driver::WARMUP_ATTEMPTS_PER_CASE,
        900,
        "all fifteen series retain six ten-request warm-up waves",
    );
}

/// Run one complete declared series. Authentication is intentionally passed
/// in after the real login because it has its own capture and a single actual
/// reqwest session cookie then drives every warm-up and measured GET.
async fn run_series<F, Fut>(
    ids: &mut AttemptIds,
    plan: SeriesPlan,
    authentication: AuthenticationEvidence,
    source_configuration: SourceConfigurationOutcome,
    request: Arc<F>,
) -> Result<SeriesEvidence, today_http_perf_driver::DriverError>
where
    F: Fn(today_http_perf_driver::AttemptContext) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<
            Output = Result<
                today_http_perf_driver::CompletedHttpResponse<HttpAttemptMetadata>,
                today_http_perf_driver::ClientFailure<HttpAttemptMetadata>,
            >,
        > + Send
        + 'static,
{
    let mut warmups = Vec::with_capacity(today_http_perf_driver::WARMUP_WAVES);
    for wave in 0..today_http_perf_driver::WARMUP_WAVES {
        warmups.push(RawWave {
            case: plan.case,
            series: plan.series,
            wave: today_http_perf_driver::run_wave(
                ids,
                plan.arm,
                AttemptPhase::Warmup,
                wave,
                today_http_perf_driver::WARMUP_CONCURRENCY,
                request.clone(),
            )
            .await?,
        });
    }

    let mut measured = Vec::with_capacity(
        plan.shape.serial_attempts + plan.shape.waves_at_10 + plan.shape.waves_at_20,
    );
    let first_measured_wave = today_http_perf_driver::WARMUP_WAVES;
    for wave in today_http_perf_driver::run_serial(
        ids,
        plan.arm,
        AttemptPhase::Measured,
        first_measured_wave,
        plan.shape.serial_attempts,
        request.clone(),
    )
    .await?
    {
        measured.push(RawWave {
            case: plan.case,
            series: plan.series,
            wave,
        });
    }

    let mut next_wave = first_measured_wave + plan.shape.serial_attempts;
    for _ in 0..plan.shape.waves_at_10 {
        measured.push(RawWave {
            case: plan.case,
            series: plan.series,
            wave: today_http_perf_driver::run_wave(
                ids,
                plan.arm,
                AttemptPhase::Measured,
                next_wave,
                10,
                request.clone(),
            )
            .await?,
        });
        next_wave += 1;
    }
    for _ in 0..plan.shape.waves_at_20 {
        measured.push(RawWave {
            case: plan.case,
            series: plan.series,
            wave: today_http_perf_driver::run_wave(
                ids,
                plan.arm,
                AttemptPhase::Measured,
                next_wave,
                20,
                request.clone(),
            )
            .await?,
        });
        next_wave += 1;
    }

    let summary = summarize_series(&warmups, &measured, plan);
    Ok(SeriesEvidence {
        case: plan.case,
        series: plan.series,
        arm: plan.arm,
        authentication,
        source_configuration,
        warmups,
        measured,
        summary,
    })
}

fn measured_attempts(
    waves: &[RawWave<HttpAttemptMetadata>],
) -> Vec<&AttemptRecord<HttpAttemptMetadata>> {
    waves
        .iter()
        .flat_map(|wrapped| wrapped.wave.attempts.iter())
        .collect()
}

fn metadata<'a>(
    attempts: impl IntoIterator<Item = &'a AttemptRecord<HttpAttemptMetadata>>,
) -> impl Iterator<Item = &'a HttpAttemptMetadata> {
    attempts
        .into_iter()
        .filter_map(|attempt| attempt.outcome.metadata())
}

fn safe_duration_summary(durations: impl IntoIterator<Item = Duration>) -> TimingSummary {
    today_http_perf_driver::summarize_timings(
        durations
            .into_iter()
            .map(today_http_perf_driver::TimingCapture::Exact),
    )
}

fn max_headroom(summary: &TimingSummary) -> Option<Duration> {
    summary
        .max
        .and_then(|observed| today_http_perf_driver::ACQUISITION_BUDGET.checked_sub(observed))
}

fn completion_report(
    attempts: &[&AttemptRecord<HttpAttemptMetadata>],
    expected_attempts: usize,
    expected_whole_source_evaluations: usize,
) -> NormalCompletionReport {
    today_http_perf_driver::check_normal_completion(
        &attempts.iter().copied().cloned().collect::<Vec<_>>(),
        NormalCompletionGate {
            expected_attempts,
            expected_whole_source_evaluations: Some(expected_whole_source_evaluations),
        },
    )
}

fn telemetry_coverage(
    attempts: &[&AttemptRecord<HttpAttemptMetadata>],
    plan: SeriesPlan,
    expected_attempts: usize,
) -> TelemetryCoverage {
    let expected_source_enumeration_per_attempt = usize::from(plan.arm == QueryArm::Final);
    let mut attempts_with_metadata = 0;
    let mut attempts_with_authentication_pool_acquisition = 0;
    let mut attempts_with_feed_pool_acquisition = 0;
    let mut recorded_source_enumeration_events = 0;
    let mut recorded_whole_source_events = 0;
    let mut typed_violations = Vec::new();

    for attempt in attempts {
        let Some(metadata) = attempt.outcome.metadata() else {
            typed_violations.push(TelemetryViolation::MissingMetadata {
                attempt: attempt.context.id,
            });
            continue;
        };
        attempts_with_metadata += 1;

        if metadata.capture_terminal != CaptureTerminal::CompleteBody {
            typed_violations.push(TelemetryViolation::IncompleteCaptureTerminal {
                attempt: attempt.context.id,
                terminal: metadata.capture_terminal,
            });
        }

        let authentication_count = metadata.authentication_pool_acquisitions.len();
        if authentication_count > 0 {
            attempts_with_authentication_pool_acquisition += 1;
        }
        if authentication_count != 1 {
            typed_violations.push(TelemetryViolation::AuthenticationPoolAcquisitionCount {
                attempt: attempt.context.id,
                actual: authentication_count,
            });
        }
        for event in &metadata.authentication_pool_acquisitions {
            if event.outcome != PoolAcquisitionOutcome::Acquired {
                typed_violations.push(TelemetryViolation::AuthenticationPoolAcquisitionOutcome {
                    attempt: attempt.context.id,
                    outcome: event.outcome,
                });
            }
        }

        let feed_count = metadata.feed_pool_acquisitions.len();
        if feed_count > 0 {
            attempts_with_feed_pool_acquisition += 1;
        }
        if feed_count != 1 {
            typed_violations.push(TelemetryViolation::FeedPoolAcquisitionCount {
                attempt: attempt.context.id,
                actual: feed_count,
            });
        }
        for event in &metadata.feed_pool_acquisitions {
            if event.outcome != PoolAcquisitionOutcome::Acquired {
                typed_violations.push(TelemetryViolation::FeedPoolAcquisitionOutcome {
                    attempt: attempt.context.id,
                    outcome: event.outcome,
                });
            }
        }

        let enumeration_count = metadata.source_enumeration.len();
        recorded_source_enumeration_events += enumeration_count;
        if enumeration_count != expected_source_enumeration_per_attempt {
            typed_violations.push(TelemetryViolation::SourceEnumerationCount {
                attempt: attempt.context.id,
                expected: expected_source_enumeration_per_attempt,
                actual: enumeration_count,
            });
        }
        for event in &metadata.source_enumeration {
            if event.outcome != SourceEnumerationOutcome::Complete {
                typed_violations.push(TelemetryViolation::SourceEnumerationOutcome {
                    attempt: attempt.context.id,
                    outcome: event.outcome,
                });
            }
        }

        let source_count = metadata.source_evaluations.len();
        recorded_whole_source_events += source_count;
        if source_count != plan.expected_sources_per_attempt
            || metadata.whole_source_evaluation_count != source_count
        {
            typed_violations.push(TelemetryViolation::WholeSourceEvaluationCount {
                attempt: attempt.context.id,
                expected: plan.expected_sources_per_attempt,
                actual: source_count,
            });
        }
        for event in &metadata.source_evaluations {
            if event.outcome != WholeSourceOutcome::Complete {
                typed_violations.push(TelemetryViolation::WholeSourceEvaluationOutcome {
                    attempt: attempt.context.id,
                    outcome: event.outcome,
                });
            }
            if event.filter_kinds.is_empty() {
                typed_violations.push(TelemetryViolation::WholeSourceWithoutStaticKinds {
                    attempt: attempt.context.id,
                });
            }
            if event.filter_kinds.contains(&StaticFilterKind::Unknown) {
                typed_violations.push(TelemetryViolation::UnknownStaticFilterKind {
                    attempt: attempt.context.id,
                });
            }
        }
    }

    TelemetryCoverage {
        expected_get_attempts: expected_attempts,
        attempts_with_metadata,
        attempts_with_authentication_pool_acquisition,
        attempts_with_feed_pool_acquisition,
        expected_source_enumeration_events: expected_attempts
            * expected_source_enumeration_per_attempt,
        recorded_source_enumeration_events,
        expected_whole_source_events: expected_attempts * plan.expected_sources_per_attempt,
        recorded_whole_source_events,
        typed_violations,
    }
}

fn summarize_series(
    warmups: &[RawWave<HttpAttemptMetadata>],
    measured: &[RawWave<HttpAttemptMetadata>],
    plan: SeriesPlan,
) -> SeriesSummary {
    let attempts = measured_attempts(measured);
    let normal_completion = today_http_perf_driver::check_normal_completion(
        &attempts.iter().copied().cloned().collect::<Vec<_>>(),
        NormalCompletionGate {
            expected_attempts: plan.expected_measured_attempts(),
            expected_whole_source_evaluations: Some(plan.expected_whole_source_evaluations()),
        },
    );

    let mut grouped = BTreeMap::<usize, Vec<_>>::new();
    for wave in measured {
        grouped
            .entry(wave.wave.concurrency)
            .or_default()
            .extend(wave.wave.attempts.iter().map(|attempt| attempt.timing));
    }
    let request_by_concurrency = grouped
        .into_iter()
        .map(|(concurrency, timings)| {
            let request_timing = today_http_perf_driver::summarize_timings(timings);
            let request_p95_limit = today_http_perf_driver::request_p95_limit(concurrency);
            let p95_within_limit = request_p95_limit
                .zip(request_timing.p95)
                .is_some_and(|(limit, p95)| p95 <= limit);
            (
                concurrency,
                LoadShapeSummary {
                    request_timing,
                    request_p95_limit,
                    p95_within_limit,
                },
            )
        })
        .collect();

    let whole_source_timing =
        safe_duration_summary(metadata(attempts.iter().copied()).flat_map(|metadata| {
            metadata
                .source_evaluations
                .iter()
                .map(|event| event.duration)
        }));
    let source_enumeration_timing =
        safe_duration_summary(metadata(attempts.iter().copied()).flat_map(|metadata| {
            metadata
                .source_enumeration
                .iter()
                .map(|event| event.duration)
        }));
    let authentication_pool_acquisition_timing =
        safe_duration_summary(metadata(attempts.iter().copied()).flat_map(|metadata| {
            metadata
                .authentication_pool_acquisitions
                .iter()
                .map(|event| event.duration)
        }));
    let feed_pool_acquisition_timing =
        safe_duration_summary(metadata(attempts.iter().copied()).flat_map(|metadata| {
            metadata
                .feed_pool_acquisitions
                .iter()
                .map(|event| event.duration)
        }));
    let measured_telemetry_coverage =
        telemetry_coverage(&attempts, plan, plan.expected_measured_attempts());
    let warmup_attempts = measured_attempts(warmups);
    let warmup_normal_completion = completion_report(
        &warmup_attempts,
        today_http_perf_driver::WARMUP_ATTEMPTS_PER_CASE,
        today_http_perf_driver::WARMUP_ATTEMPTS_PER_CASE * plan.expected_sources_per_attempt,
    );
    let warmup_telemetry_coverage = telemetry_coverage(
        &warmup_attempts,
        plan,
        today_http_perf_driver::WARMUP_ATTEMPTS_PER_CASE,
    );

    SeriesSummary {
        normal_completion,
        telemetry_coverage: measured_telemetry_coverage,
        warmup_normal_completion,
        warmup_telemetry_coverage,
        request_by_concurrency,
        whole_source_p95_within_limit: whole_source_timing
            .p95
            .is_none_or(|p95| p95 < today_http_perf_driver::WHOLE_SOURCE_P95_LIMIT),
        whole_source_max_within_budget: whole_source_timing
            .max
            .is_none_or(|maximum| maximum <= today_http_perf_driver::WHOLE_SOURCE_BUDGET),
        whole_source_timing,
        source_enumeration_max_within_budget: source_enumeration_timing
            .max
            .is_none_or(|maximum| maximum <= today_http_perf_driver::SOURCE_ENUMERATION_BUDGET),
        source_enumeration_timing,
        authentication_pool_headroom_to_two_seconds: max_headroom(
            &authentication_pool_acquisition_timing,
        ),
        authentication_pool_acquisition_timing,
        feed_pool_headroom_to_two_seconds: max_headroom(&feed_pool_acquisition_timing),
        feed_pool_acquisition_timing,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ZeroSourceConcurrencyParity {
    original_p95: Option<Duration>,
    final_p95: Option<Duration>,
    allowed_final_p95: Option<Duration>,
    p95_within_regression_limit: bool,
}

/// The frozen and final arms run as separate application/pool lifetimes, but
/// share one fixed server clock and fixture. This report compares the exact
/// in-process normalized JSON values (items, reasons, order and truncation),
/// while raw evidence carries only their SHA-256 digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ZeroSourceParityReport {
    case: HttpPerformanceCase,
    comparison_present: bool,
    original_payloads_complete_and_stable: bool,
    final_payloads_complete_and_stable: bool,
    exact_normalized_payload_parity: bool,
    normalized_hash_parity: bool,
    p95_by_concurrency: BTreeMap<usize, ZeroSourceConcurrencyParity>,
}

fn normalized_bodies(series: &SeriesEvidence) -> Vec<&Value> {
    series
        .measured
        .iter()
        .flat_map(|wrapped| wrapped.wave.attempts.iter())
        .filter_map(|attempt| attempt.outcome.metadata())
        .filter_map(|metadata| metadata.normalized_today_body.as_ref())
        .collect()
}

fn normalized_hashes(series: &SeriesEvidence) -> Vec<&str> {
    series
        .measured
        .iter()
        .flat_map(|wrapped| wrapped.wave.attempts.iter())
        .filter_map(|attempt| attempt.outcome.metadata())
        .filter_map(|metadata| metadata.normalized_today_sha256.as_deref())
        .collect()
}

fn stable_payloads(payloads: &[&Value], expected: usize) -> bool {
    let Some(first) = payloads.first() else {
        return false;
    };
    payloads.len() == expected && payloads.iter().all(|payload| *payload == *first)
}

fn stable_hashes(hashes: &[&str], expected: usize) -> bool {
    let Some(first) = hashes.first() else {
        return false;
    };
    hashes.len() == expected && hashes.iter().all(|hash| *hash == *first)
}

fn paired_zero_source_parity(
    original: &SeriesEvidence,
    final_arm: &SeriesEvidence,
) -> ZeroSourceParityReport {
    assert_eq!(
        original.arm,
        QueryArm::FrozenOriginal,
        "parity original arm must use the frozen query adapter",
    );
    assert_eq!(
        final_arm.arm,
        QueryArm::Final,
        "parity final arm must use the source-aware HTTP router",
    );
    assert_eq!(
        original.case, final_arm.case,
        "only corresponding zero-source cases may be paired",
    );

    let expected = original.summary.normal_completion.expected_attempts;
    let original_payloads = normalized_bodies(original);
    let final_payloads = normalized_bodies(final_arm);
    let original_hashes = normalized_hashes(original);
    let final_hashes = normalized_hashes(final_arm);
    let original_stable = stable_payloads(&original_payloads, expected);
    let final_stable = stable_payloads(&final_payloads, expected);
    let exact_normalized_payload_parity =
        original_stable && final_stable && original_payloads.first() == final_payloads.first();
    let normalized_hash_parity = stable_hashes(&original_hashes, expected)
        && stable_hashes(&final_hashes, expected)
        && original_hashes.first() == final_hashes.first();

    let mut p95_by_concurrency = BTreeMap::new();
    for concurrency in [1, 10, 20] {
        let original_p95 = original
            .summary
            .request_by_concurrency
            .get(&concurrency)
            .and_then(|summary| summary.request_timing.p95);
        let final_p95 = final_arm
            .summary
            .request_by_concurrency
            .get(&concurrency)
            .and_then(|summary| summary.request_timing.p95);
        let allowed_final_p95 = original_p95
            .map(|baseline| baseline + std::cmp::max(Duration::from_millis(25), baseline / 10));
        let p95_within_regression_limit = final_p95
            .zip(allowed_final_p95)
            .is_some_and(|(actual, allowed)| actual <= allowed);
        p95_by_concurrency.insert(
            concurrency,
            ZeroSourceConcurrencyParity {
                original_p95,
                final_p95,
                allowed_final_p95,
                p95_within_regression_limit,
            },
        );
    }

    ZeroSourceParityReport {
        case: original.case,
        comparison_present: true,
        original_payloads_complete_and_stable: original_stable,
        final_payloads_complete_and_stable: final_stable,
        exact_normalized_payload_parity,
        normalized_hash_parity,
        p95_by_concurrency,
    }
}

fn missing_zero_source_parity(case: HttpPerformanceCase) -> ZeroSourceParityReport {
    let p95_by_concurrency = [1, 10, 20]
        .into_iter()
        .map(|concurrency| {
            (
                concurrency,
                ZeroSourceConcurrencyParity {
                    original_p95: None,
                    final_p95: None,
                    allowed_final_p95: None,
                    p95_within_regression_limit: false,
                },
            )
        })
        .collect();
    ZeroSourceParityReport {
        case,
        comparison_present: false,
        original_payloads_complete_and_stable: false,
        final_payloads_complete_and_stable: false,
        exact_normalized_payload_parity: false,
        normalized_hash_parity: false,
        p95_by_concurrency,
    }
}

fn paired_zero_source_reports(series: &[SeriesEvidence]) -> Vec<ZeroSourceParityReport> {
    [
        HttpPerformanceCase::ConcentratedZeroSources,
        HttpPerformanceCase::TypicalZeroSources,
        HttpPerformanceCase::PartialBuiltinsZeroSources,
        HttpPerformanceCase::EmptyBuiltinsZeroSources,
    ]
    .into_iter()
    .map(|case| {
        let original = series.iter().find(|evidence| {
            evidence.case == case
                && evidence.series == HttpPerformanceSeries::PairedOriginalZeroSource
        });
        let final_arm = series.iter().find(|evidence| {
            evidence.case == case && evidence.series == HttpPerformanceSeries::FinalMatrix
        });
        match (original, final_arm) {
            (Some(original), Some(final_arm)) => paired_zero_source_parity(original, final_arm),
            _ => missing_zero_source_parity(case),
        }
    })
    .collect()
}

/// Exact fixture facts are recorded once per run. They deliberately contain
/// only synthetic cardinalities, the fixed server-side clock, and immutable
/// build/source hashes supplied by the primary-owned fixture setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FixtureEvidence {
    people: usize,
    inquiries: usize,
    contact_facts: usize,
    contact_corrections: usize,
    inbound_records: usize,
    fixed_clock: DateTime<Utc>,
    source_hash: String,
    build_hash: String,
}

/// A join failure has no caller metadata slot in the pure scheduling driver.
/// This separately retained record reconciles its preassigned capture ID after
/// the loopback server has stopped, keeping safe late events and terminal
/// status linked to the static case/series/attempt context without persisting
/// the capture ID itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LateCaptureReconciliation {
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    arm: QueryArm,
    phase: AttemptPhase,
    wave: usize,
    attempt: AttemptId,
    capture_terminal: CaptureTerminal,
    source_enumeration: Vec<SourceEnumerationEvent>,
    whole_source_evaluation_count: usize,
    source_evaluations: Vec<WholeSourceEvent>,
    authentication_pool_acquisitions: Vec<PoolAcquisitionEvent>,
    feed_pool_acquisitions: Vec<PoolAcquisitionEvent>,
}

fn late_capture_reconciliation(
    case: HttpPerformanceCase,
    series: HttpPerformanceSeries,
    context: today_http_perf_driver::AttemptContext,
    telemetry: HttpPerfTelemetry,
) -> LateCaptureReconciliation {
    let source_enumeration = telemetry
        .source_enumeration
        .into_iter()
        .map(source_enumeration_event)
        .collect();
    let source_evaluations = telemetry
        .source_evaluations
        .into_iter()
        .map(source_evaluation_event)
        .collect::<Vec<_>>();
    LateCaptureReconciliation {
        case,
        series,
        arm: context.arm,
        phase: context.phase,
        wave: context.wave,
        attempt: context.id,
        capture_terminal: capture_terminal(telemetry.terminal),
        source_enumeration,
        whole_source_evaluation_count: source_evaluations.len(),
        source_evaluations,
        authentication_pool_acquisitions: telemetry
            .authentication_pool_acquisitions
            .into_iter()
            .map(pool_acquisition_event)
            .collect(),
        feed_pool_acquisitions: telemetry
            .feed_pool_acquisitions
            .into_iter()
            .map(pool_acquisition_event)
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PhaseBEvidence {
    protocol: &'static str,
    fixture: FixtureEvidence,
    series: Vec<SeriesEvidence>,
    late_capture_reconciliations: Vec<LateCaptureReconciliation>,
    paired_zero_source_parity: Vec<ZeroSourceParityReport>,
    sentinel_safety: SentinelSafetyReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
struct SentinelSafetyReport {
    forbidden_fixture_sentinels_absent: bool,
    required_safe_telemetry_fields_present: bool,
}

/// A failed run is an evidence artifact, never an automatically overwritten
/// temporary file. The coordinator may copy the resulting path into the
/// reviewed archive after it has verified that the JSON contains no sentinel
/// fixture names or criteria.
fn write_phase_b_evidence(evidence: &PhaseBEvidence) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/slice-011c-http");
    std::fs::create_dir_all(&directory).expect("create Phase B retained-evidence directory");
    let path = directory.join(format!("run-{timestamp_nanos}.json"));
    let bytes = serde_json::to_vec_pretty(evidence).expect("serialize safe Phase B evidence");
    std::fs::write(&path, bytes).expect("retain Phase B raw evidence");
    path
}

fn coverage_is_complete(coverage: &TelemetryCoverage) -> bool {
    coverage.attempts_with_metadata == coverage.expected_get_attempts
        && coverage.attempts_with_authentication_pool_acquisition == coverage.expected_get_attempts
        && coverage.attempts_with_feed_pool_acquisition == coverage.expected_get_attempts
        && coverage.recorded_source_enumeration_events
            == coverage.expected_source_enumeration_events
        && coverage.recorded_whole_source_events == coverage.expected_whole_source_events
        && coverage.typed_violations.is_empty()
}

fn normal_series_passes(series: &SeriesEvidence) -> bool {
    let summary = &series.summary;
    let meets_absolute_request_p95 = series.arm != QueryArm::Final
        || summary
            .request_by_concurrency
            .values()
            .all(|shape| shape.p95_within_limit);
    series.authentication.status == Some(today_http_perf_driver::HTTP_OK)
        && series.authentication.body_classification
            == AuthenticationBodyClassification::CompleteSession
        && series.source_configuration == SourceConfigurationOutcome::Complete
        && summary.normal_completion.passed
        && coverage_is_complete(&summary.telemetry_coverage)
        && summary.warmup_normal_completion.passed
        && coverage_is_complete(&summary.warmup_telemetry_coverage)
        && meets_absolute_request_p95
        && summary.whole_source_p95_within_limit
        && summary.whole_source_max_within_budget
        && summary.source_enumeration_max_within_budget
}

/// A failed real login still has to be retained as a declared series. It has
/// no invented GET outcomes: the empty retained waves make the missing
/// requests and their expected telemetry explicit in the normal-completion
/// and coverage reports.
fn unauthenticated_series(
    plan: SeriesPlan,
    authentication: AuthenticationEvidence,
) -> SeriesEvidence {
    SeriesEvidence {
        case: plan.case,
        series: plan.series,
        arm: plan.arm,
        authentication,
        source_configuration: SourceConfigurationOutcome::CommandFailed,
        warmups: Vec::new(),
        measured: Vec::new(),
        summary: summarize_series(&[], &[], plan),
    }
}

fn setup_failed_series(
    plan: SeriesPlan,
    authentication: AuthenticationEvidence,
    source_configuration: SourceConfigurationOutcome,
) -> SeriesEvidence {
    SeriesEvidence {
        case: plan.case,
        series: plan.series,
        arm: plan.arm,
        authentication,
        source_configuration,
        warmups: Vec::new(),
        measured: Vec::new(),
        summary: summarize_series(&[], &[], plan),
    }
}

fn not_attempted_authentication() -> AuthenticationEvidence {
    AuthenticationEvidence {
        status: None,
        body_classification: AuthenticationBodyClassification::NotAttempted,
        body_sha256: None,
        pool_acquisitions: Vec::new(),
    }
}

/// Logs in once for each declared series, applies the approved source
/// configuration through its real HTTP command path, then runs all retained
/// warm-up and measured GET attempts through one isolated arm server. A login
/// failure is retained and aborts the arm rather than substituting a retry or
/// pretending its absent GETs occurred.
async fn run_arm_series(
    fixture: &today_http_perf_fixture::TodayHttpPerfFixture,
    server: &LoopbackServer,
    plans: impl IntoIterator<Item = SeriesPlan>,
    ids: &mut AttemptIds,
    capture_ids: &CaptureIds,
    collector: &HttpPerfCollector,
) -> (Vec<SeriesEvidence>, bool) {
    let mut evidence = Vec::new();
    for plan in plans {
        if fixture
            .sources(fixture_case(plan.case), source_mode(plan))
            .len()
            != plan.expected_sources_per_attempt
        {
            evidence.push(setup_failed_series(
                plan,
                not_attempted_authentication(),
                SourceConfigurationOutcome::DefinitionCountMismatch,
            ));
            return (evidence, false);
        }
        let viewer = fixture.viewer(fixture_case(plan.case));
        let (session, authentication) = authenticate_session(
            reqwest::Client::new(),
            &server.base_url,
            &viewer.email,
            &viewer.password,
            collector,
            capture_ids,
        )
        .await;
        let Some(session) = session else {
            evidence.push(unauthenticated_series(plan, authentication));
            return (evidence, false);
        };

        let source_configuration = std::panic::AssertUnwindSafe(fixture.assert_sources_via_http(
            &server.base_url,
            &session.cookie,
            fixture_case(plan.case),
            source_mode(plan),
        ))
        .catch_unwind()
        .await;
        if source_configuration.is_err() {
            evidence.push(setup_failed_series(
                plan,
                authentication,
                SourceConfigurationOutcome::CommandFailed,
            ));
            return (evidence, false);
        }

        let base_url = server.base_url.clone();
        let session_for_requests = session.clone();
        let collector = collector.clone();
        let request = Arc::new(move |context: today_http_perf_driver::AttemptContext| {
            let session = session_for_requests.clone();
            let base_url = base_url.clone();
            let collector = collector.clone();
            let capture_id = capture_id_for_attempt(context);
            async move {
                dispatch_today(
                    session,
                    base_url,
                    plan.arm,
                    plan.case,
                    plan.series,
                    collector,
                    capture_id,
                )
                .await
            }
        });
        let series = run_series(
            ids,
            plan,
            authentication,
            SourceConfigurationOutcome::Complete,
            request,
        )
        .await
        .expect("declared Phase B driver wave shape is valid");
        evidence.push(series);
    }
    (evidence, true)
}

fn plan_from_series(series: &SeriesEvidence) -> SeriesPlan {
    match series.series {
        HttpPerformanceSeries::FinalMatrix => {
            let case = match series.case {
                HttpPerformanceCase::ConcentratedZeroSources => {
                    FinalMatrixCase::ConcentratedZeroSources
                }
                HttpPerformanceCase::ConcentratedOneDenseSource => {
                    FinalMatrixCase::ConcentratedOneDenseSource
                }
                HttpPerformanceCase::ConcentratedOneAbsenceSource => {
                    FinalMatrixCase::ConcentratedOneAbsenceSource
                }
                HttpPerformanceCase::ConcentratedFiveOverlappingSources => {
                    FinalMatrixCase::ConcentratedFiveOverlappingSources
                }
                HttpPerformanceCase::TypicalZeroSources => FinalMatrixCase::TypicalZeroSources,
                HttpPerformanceCase::TypicalFiveOverlappingSources => {
                    FinalMatrixCase::TypicalFiveOverlappingSources
                }
                HttpPerformanceCase::PartialBuiltinsZeroSources => {
                    FinalMatrixCase::PartialBuiltinsZeroSources
                }
                HttpPerformanceCase::PartialBuiltinsFiveOverlappingSources => {
                    FinalMatrixCase::PartialBuiltinsFiveOverlappingSources
                }
                HttpPerformanceCase::EmptyBuiltinsZeroSources => {
                    FinalMatrixCase::EmptyBuiltinsZeroSources
                }
                HttpPerformanceCase::EmptyBuiltinsFiveOverlappingSources => {
                    FinalMatrixCase::EmptyBuiltinsFiveOverlappingSources
                }
            };
            SeriesPlan::final_matrix(case)
        }
        HttpPerformanceSeries::IndependentConcentratedRepeat => {
            SeriesPlan::independent_concentrated_repeat()
        }
        HttpPerformanceSeries::PairedOriginalZeroSource => match series.case {
            HttpPerformanceCase::ConcentratedZeroSources => {
                SeriesPlan::paired_original_zero_source(
                    HttpPerformanceCase::ConcentratedZeroSources,
                    today_http_perf_driver::CRITICAL_SAMPLE_SHAPE,
                )
            }
            HttpPerformanceCase::TypicalZeroSources => SeriesPlan::paired_original_zero_source(
                HttpPerformanceCase::TypicalZeroSources,
                today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
            ),
            HttpPerformanceCase::PartialBuiltinsZeroSources => {
                SeriesPlan::paired_original_zero_source(
                    HttpPerformanceCase::PartialBuiltinsZeroSources,
                    today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
                )
            }
            HttpPerformanceCase::EmptyBuiltinsZeroSources => {
                SeriesPlan::paired_original_zero_source(
                    HttpPerformanceCase::EmptyBuiltinsZeroSources,
                    today_http_perf_driver::SUPPLEMENTAL_SAMPLE_SHAPE,
                )
            }
            _ => panic!("only declared zero-source cases can use the frozen-original series"),
        },
    }
}

/// Reconcile any bounded client drain only after the arm's loopback server has
/// gracefully stopped. This work is deliberately post-timing: the driver has
/// already recorded each response/client-failure completion boundary. A join
/// failure has no metadata slot, so it receives a separate safe record linked
/// by its preassigned attempt ID and enclosing static case/series wrapper.
async fn reconcile_after_server_stop(
    series: &mut [SeriesEvidence],
    collector: &HttpPerfCollector,
) -> Vec<LateCaptureReconciliation> {
    let mut reconciliations = Vec::new();
    for series_evidence in series {
        for wrapped in series_evidence
            .warmups
            .iter_mut()
            .chain(series_evidence.measured.iter_mut())
        {
            for attempt in &mut wrapped.wave.attempts {
                match &mut attempt.outcome {
                    AttemptOutcome::ClientFailure(failure) => {
                        let Some(capture_id) =
                            failure.metadata.pending_client_failure_capture_id.take()
                        else {
                            continue;
                        };
                        let telemetry = collector
                            .drain_after_client_failure(capture_id, Duration::from_secs(2))
                            .await;
                        apply_telemetry(&mut failure.metadata, telemetry);
                    }
                    AttemptOutcome::JoinFailure(_) => {
                        let telemetry = collector
                            .drain_after_client_failure(
                                capture_id_for_attempt(attempt.context),
                                Duration::from_secs(2),
                            )
                            .await;
                        reconciliations.push(late_capture_reconciliation(
                            wrapped.case,
                            wrapped.series,
                            attempt.context,
                            telemetry,
                        ));
                    }
                    AttemptOutcome::Response(_) => {}
                }
            }
        }
        let plan = plan_from_series(series_evidence);
        series_evidence.summary =
            summarize_series(&series_evidence.warmups, &series_evidence.measured, plan);
    }
    reconciliations
}

fn paired_parity_passes(report: &ZeroSourceParityReport) -> bool {
    report.comparison_present
        && report.original_payloads_complete_and_stable
        && report.final_payloads_complete_and_stable
        && report.exact_normalized_payload_parity
        && report.normalized_hash_parity
        && report
            .p95_by_concurrency
            .values()
            .all(|shape| shape.p95_within_regression_limit)
}

fn fixture_evidence(fixture: &today_http_perf_fixture::TodayHttpPerfFixture) -> FixtureEvidence {
    let counts = fixture.counts();
    assert_eq!(counts.people, 50_000, "Phase B fixture People count");
    assert_eq!(counts.inquiries, 46_064, "Phase B fixture inquiry count");
    assert_eq!(
        counts.contact_facts, 52_306,
        "Phase B fixture contact-fact count"
    );
    assert_eq!(
        counts.contact_corrections, 2_906,
        "Phase B fixture contact-correction count"
    );
    assert_eq!(
        counts.inbound_records, 2_843,
        "Phase B fixture inbound-history count"
    );
    FixtureEvidence {
        people: counts.people,
        inquiries: counts.inquiries,
        contact_facts: counts.contact_facts,
        contact_corrections: counts.contact_corrections,
        inbound_records: counts.inbound_records,
        fixed_clock: fixture.fixed_clock(),
        source_hash: fixture.source_hash().to_owned(),
        build_hash: fixture.build_hash().to_owned(),
    }
}

fn attempt_count(series: &SeriesEvidence, phase: AttemptPhase) -> usize {
    let waves = match phase {
        AttemptPhase::Warmup => &series.warmups,
        AttemptPhase::Measured => &series.measured,
    };
    waves
        .iter()
        .map(|wrapped| wrapped.wave.attempts.len())
        .sum()
}

fn all_attempt_ids_are_unique(series: &[SeriesEvidence]) -> bool {
    let mut ids = std::collections::BTreeSet::new();
    series
        .iter()
        .flat_map(|series| series.warmups.iter().chain(series.measured.iter()))
        .flat_map(|wrapped| wrapped.wave.attempts.iter())
        .all(|attempt| ids.insert(attempt.context.id))
}

/// The retained artifact must have structural safe fields as well as the
/// request-local source telemetry needed for sentinel review. The fixture uses
/// these strings in Person values, email/session data, and saved source names;
/// their absence proves raw evidence did not accidentally capture a body,
/// credential, or source definition.
fn sentinel_safety_report(evidence: &PhaseBEvidence) -> SentinelSafetyReport {
    let serialized =
        serde_json::to_string(evidence).expect("serialize Phase B evidence for sentinel check");
    let forbidden_fixture_sentinels_absent = [
        "Synthetic",
        "fixture-",
        "example.invalid",
        "slice-011c-http-fixture-password",
    ]
    .into_iter()
    .all(|sentinel| !serialized.contains(sentinel));
    let required_safe_telemetry_fields_present = [
        "filter_kinds",
        "authentication_pool_acquisitions",
        "feed_pool_acquisitions",
        "source_evaluations",
        "capture_terminal",
    ]
    .into_iter()
    .all(|required_safe_field| serialized.contains(required_safe_field));
    SentinelSafetyReport {
        forbidden_fixture_sentinels_absent,
        required_safe_telemetry_fields_present,
    }
}

#[sqlx::test]
#[ignore = "Phase B only: requires an isolated database and explicit optimized-build hash"]
async fn slice_011c_authenticated_http_performance_harness(migrator_pool: PgPool) {
    assert_predeclared_matrix();
    let fixture = today_http_perf_fixture::TodayHttpPerfFixture::create(migrator_pool).await;
    let fixture_record = fixture_evidence(&fixture);
    let mut attempt_ids = AttemptIds::default();
    let capture_ids = CaptureIds::default();
    let mut series = Vec::with_capacity(15);
    let mut late_capture_reconciliations = Vec::new();

    // The frozen original completes before a distinct final-arm pool is even
    // created. This preserves the protocol's sequential capacity boundary.
    let original_collector = HttpPerfCollector::default();
    let original_pool = fixture.arm_pool().await;
    let original_server = LoopbackServer::start(frozen_original_arm_router(
        original_pool.clone(),
        fixture.config(),
        fixture.fixed_clock(),
        original_collector.clone(),
    ))
    .await;
    let (mut original_series, original_dispatched) = run_arm_series(
        &fixture,
        &original_server,
        paired_original_zero_source_plans(),
        &mut attempt_ids,
        &capture_ids,
        &original_collector,
    )
    .await;
    original_server.stop().await;
    original_pool.close().await;
    late_capture_reconciliations
        .extend(reconcile_after_server_stop(&mut original_series, &original_collector).await);
    series.append(&mut original_series);

    let final_collector = HttpPerfCollector::default();
    let final_pool = fixture.arm_pool().await;
    let final_server = LoopbackServer::start(final_arm_router(
        final_pool.clone(),
        fixture.config(),
        fixture.fixed_clock(),
        final_collector.clone(),
    ))
    .await;
    let mut final_plans = final_matrix_plans();
    final_plans.push(SeriesPlan::independent_concentrated_repeat());
    let (mut final_series, final_dispatched) = run_arm_series(
        &fixture,
        &final_server,
        final_plans,
        &mut attempt_ids,
        &capture_ids,
        &final_collector,
    )
    .await;
    final_server.stop().await;
    final_pool.close().await;
    late_capture_reconciliations
        .extend(reconcile_after_server_stop(&mut final_series, &final_collector).await);
    series.append(&mut final_series);

    let paired_zero_source_parity = paired_zero_source_reports(&series);
    let mut evidence = PhaseBEvidence {
        protocol: "slice-011c-authenticated-http-v1",
        fixture: fixture_record,
        series,
        late_capture_reconciliations,
        paired_zero_source_parity,
        sentinel_safety: SentinelSafetyReport::default(),
    };
    evidence.sentinel_safety = sentinel_safety_report(&evidence);
    let artifact = write_phase_b_evidence(&evidence);

    let final_measured_attempts = evidence
        .series
        .iter()
        .filter(|series| series.arm == QueryArm::Final)
        .map(|series| attempt_count(series, AttemptPhase::Measured))
        .sum::<usize>();
    let original_measured_attempts = evidence
        .series
        .iter()
        .filter(|series| series.arm == QueryArm::FrozenOriginal)
        .map(|series| attempt_count(series, AttemptPhase::Measured))
        .sum::<usize>();
    let warmup_attempts = evidence
        .series
        .iter()
        .map(|series| attempt_count(series, AttemptPhase::Warmup))
        .sum::<usize>();
    let final_whole_source_evaluations = evidence
        .series
        .iter()
        .filter(|series| series.arm == QueryArm::Final)
        .flat_map(|series| series.measured.iter())
        .flat_map(|wrapped| wrapped.wave.attempts.iter())
        .filter_map(|attempt| attempt.outcome.metadata())
        .map(|metadata| metadata.whole_source_evaluation_count)
        .sum::<usize>();
    let accepted = original_dispatched
        && final_dispatched
        && evidence.series.len() == 15
        && final_measured_attempts == 522
        && original_measured_attempts == 173
        && warmup_attempts == 900
        && final_whole_source_evaluations == 1_201
        && all_attempt_ids_are_unique(&evidence.series)
        && evidence.sentinel_safety.forbidden_fixture_sentinels_absent
        && evidence
            .sentinel_safety
            .required_safe_telemetry_fields_present
        && evidence.series.iter().all(normal_series_passes)
        && evidence.paired_zero_source_parity.len() == 4
        && evidence
            .paired_zero_source_parity
            .iter()
            .all(paired_parity_passes);

    fixture.cleanup().await;
    assert!(
        accepted,
        "Phase B HTTP acceptance failed; retained safe evidence at {}",
        artifact.display(),
    );
}

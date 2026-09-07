//! Pure attempt scheduling and aggregation for Slice 011c's Phase B HTTP
//! verification. This fixture deliberately has no router, database, URL,
//! authentication, credential, or request-body knowledge. A later harness
//! supplies a closure that dispatches one already-authenticated request and
//! resolves only after consuming its complete body.
//!
//! The protocol is declared in
//! `docs/design/perf/slice-011c-http-2026-09-06/PROTOCOL.md`. It requires
//! deterministic preassigned attempt IDs, retained warm-ups, barrier-aligned
//! waves, raw outcomes (including client and join failures), and nearest-rank
//! percentiles over all timing observations. This module preserves those
//! facts without printing response content or connection details.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::{sync::Barrier, task::JoinHandle};

/// The Phase B HTTP protocol's six ten-request warm-up waves. Warm-up records
/// must be retained separately and never blended into measured percentiles.
pub const WARMUP_WAVES: usize = 6;
pub const WARMUP_CONCURRENCY: usize = 10;
pub const WARMUP_ATTEMPTS_PER_CASE: usize = WARMUP_WAVES * WARMUP_CONCURRENCY;

/// The concentrated critical matrix has four cases (zero, one dense, one
/// absence, and five overlapping sources). Each has 8 serial samples and two
/// waves at both concurrency 10 and 20: 68 attempts per case / 272 total.
pub const CRITICAL_CASES: usize = 4;
pub const CRITICAL_SAMPLE_SHAPE: SampleShape = SampleShape::new(8, 2, 2);
pub const CRITICAL_MEASURED_ATTEMPTS: usize = CRITICAL_CASES * CRITICAL_SAMPLE_SHAPE.attempts();

/// The supplemental matrix has six cases (zero/five sources across typical,
/// partially filled, and empty built-in queues). Each has 5 serial samples
/// and one wave at both concurrency 10 and 20: 35 attempts per case / 210.
pub const SUPPLEMENTAL_CASES: usize = 6;
pub const SUPPLEMENTAL_SAMPLE_SHAPE: SampleShape = SampleShape::new(5, 1, 1);
pub const SUPPLEMENTAL_MEASURED_ATTEMPTS: usize =
    SUPPLEMENTAL_CASES * SUPPLEMENTAL_SAMPLE_SHAPE.attempts();

/// The independent critical five-source repeat comprises two more
/// concurrency-20 waves: 40 requests and 200 whole-source evaluations.
pub const INDEPENDENT_REPEAT_WAVES: usize = 2;
pub const INDEPENDENT_REPEAT_CONCURRENCY: usize = 20;
pub const INDEPENDENT_REPEAT_ATTEMPTS: usize =
    INDEPENDENT_REPEAT_WAVES * INDEPENDENT_REPEAT_CONCURRENCY;

/// The final arm includes the 272 critical samples, 210 supplemental samples,
/// and 40-request independent repeat. Paired frozen-original zero-source
/// series are recorded separately with their matching sample shapes.
pub const FINAL_ARM_MEASURED_ATTEMPTS: usize =
    CRITICAL_MEASURED_ATTEMPTS + SUPPLEMENTAL_MEASURED_ATTEMPTS + INDEPENDENT_REPEAT_ATTEMPTS;

/// The four frozen-original zero-source comparisons use the matching final
/// arm shape: concentrated has the critical 68 samples, and typical, partial,
/// and empty books each have 35 supplemental samples. They are intentionally
/// separate from the final-arm total because the two arms run sequentially.
pub const PAIRED_ORIGINAL_MEASURED_ATTEMPTS: usize =
    CRITICAL_SAMPLE_SHAPE.attempts() + 3 * SUPPLEMENTAL_SAMPLE_SHAPE.attempts();

/// Normal-case acceptance limits from Slice 011c §8 / the Phase B protocol.
pub const HTTP_OK: u16 = 200;
pub const REQUEST_P95_CONCURRENCY_1: Duration = Duration::from_millis(1_250);
pub const REQUEST_P95_CONCURRENCY_10: Duration = Duration::from_millis(2_500);
pub const REQUEST_P95_CONCURRENCY_20: Duration = Duration::from_millis(4_500);
pub const WHOLE_SOURCE_P95_LIMIT: Duration = Duration::from_millis(450);
pub const SOURCE_ENUMERATION_BUDGET: Duration = Duration::from_millis(250);
pub const WHOLE_SOURCE_BUDGET: Duration = Duration::from_millis(500);
pub const ACQUISITION_BUDGET: Duration = Duration::from_secs(2);

/// Serial and concurrent measured samples for a matrix case. The protocol
/// uses only concurrency 1, 10, and 20, keeping any alternate load shape out
/// of the acceptance run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SampleShape {
    pub serial_attempts: usize,
    pub waves_at_10: usize,
    pub waves_at_20: usize,
}

impl SampleShape {
    pub const fn new(serial_attempts: usize, waves_at_10: usize, waves_at_20: usize) -> Self {
        Self {
            serial_attempts,
            waves_at_10,
            waves_at_20,
        }
    }

    pub const fn attempts(self) -> usize {
        self.serial_attempts + self.waves_at_10 * 10 + self.waves_at_20 * 20
    }
}

/// Named matrix rows prevent a harness from quietly substituting one book or
/// source configuration for another. They contain only protocol categories,
/// never tenant, URL, filter JSON, Person, or authentication data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalMatrixCase {
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

impl FinalMatrixCase {
    pub const CRITICAL: [Self; CRITICAL_CASES] = [
        Self::ConcentratedZeroSources,
        Self::ConcentratedOneDenseSource,
        Self::ConcentratedOneAbsenceSource,
        Self::ConcentratedFiveOverlappingSources,
    ];

    pub const SUPPLEMENTAL: [Self; SUPPLEMENTAL_CASES] = [
        Self::TypicalZeroSources,
        Self::TypicalFiveOverlappingSources,
        Self::PartialBuiltinsZeroSources,
        Self::PartialBuiltinsFiveOverlappingSources,
        Self::EmptyBuiltinsZeroSources,
        Self::EmptyBuiltinsFiveOverlappingSources,
    ];

    pub const fn sample_shape(self) -> SampleShape {
        match self {
            Self::ConcentratedZeroSources
            | Self::ConcentratedOneDenseSource
            | Self::ConcentratedOneAbsenceSource
            | Self::ConcentratedFiveOverlappingSources => CRITICAL_SAMPLE_SHAPE,
            Self::TypicalZeroSources
            | Self::TypicalFiveOverlappingSources
            | Self::PartialBuiltinsZeroSources
            | Self::PartialBuiltinsFiveOverlappingSources
            | Self::EmptyBuiltinsZeroSources
            | Self::EmptyBuiltinsFiveOverlappingSources => SUPPLEMENTAL_SAMPLE_SHAPE,
        }
    }

    pub const fn expected_sources_per_attempt(self) -> usize {
        match self {
            Self::ConcentratedZeroSources
            | Self::TypicalZeroSources
            | Self::PartialBuiltinsZeroSources
            | Self::EmptyBuiltinsZeroSources => 0,
            Self::ConcentratedOneDenseSource | Self::ConcentratedOneAbsenceSource => 1,
            Self::ConcentratedFiveOverlappingSources
            | Self::TypicalFiveOverlappingSources
            | Self::PartialBuiltinsFiveOverlappingSources
            | Self::EmptyBuiltinsFiveOverlappingSources => 5,
        }
    }
}

/// The query path under comparison. Frozen original and final requests must
/// be run sequentially, never against a combined application pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryArm {
    FrozenOriginal,
    Final,
}

/// Warm-ups are retained as raw observations but excluded from sample
/// percentiles. Measured attempts form the acceptance aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Warmup,
    Measured,
}

/// A monotonically allocated identifier. `AttemptIds` reserves all IDs for a
/// wave before spawning work, so output remains stable even when completions
/// arrive in a different order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct AttemptId(pub u64);

/// Per-attempt public scheduling information. A harness should capture only
/// static case metadata itself; this context intentionally has no request
/// URL, cookie, authorization header, body, filter, or tenant identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AttemptContext {
    pub id: AttemptId,
    pub arm: QueryArm,
    pub phase: AttemptPhase,
    pub wave: usize,
    pub slot: usize,
}

/// Allocates deterministic numeric attempt IDs. Keep one allocator for the
/// complete retained run so an attempt can be correlated across raw artifacts
/// without using any customer or authentication identifier.
#[derive(Debug, Clone, Copy)]
pub struct AttemptIds {
    next: u64,
}

impl Default for AttemptIds {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl AttemptIds {
    fn reserve(&mut self, count: usize) -> Result<Vec<AttemptId>, DriverError> {
        let count = u64::try_from(count).map_err(|_| DriverError::AttemptIdExhausted)?;
        let after = self
            .next
            .checked_add(count)
            .ok_or(DriverError::AttemptIdExhausted)?;
        let ids = (0..count)
            .map(|offset| AttemptId(self.next + offset))
            .collect();
        self.next = after;
        Ok(ids)
    }
}

/// The harness must return a response only after it has read the complete HTTP
/// body and classified it. `metadata` is caller-selected and must contain only
/// safe timing/count/static-kind fields suitable for the final retained
/// evidence; this helper never formats or prints it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletedHttpResponse<M> {
    /// Captured immediately after the complete response body is available,
    /// before hashing, parsing, cloning, or evidence aggregation. The field
    /// stays out of raw JSON because `Instant` has only process-local meaning.
    #[serde(skip)]
    pub request_completed_at: Instant,
    pub status: u16,
    pub result: HttpBodyResult,
    pub whole_source_evaluations: usize,
    pub metadata: M,
}

/// Safe response classifications for the full body. A normal benchmark sample
/// passes only with `status == HTTP_OK` and `Complete`; partial/unavailable,
/// malformed/incomplete envelopes, and every other status remain retained and
/// fail the normal-completion gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpBodyResult {
    Complete,
    Partial,
    Unavailable,
    InvalidEnvelope,
}

/// A client-side failure returned by the request closure. The concrete
/// transport/parser error stays outside committed evidence; the harness maps
/// it to this safe category and supplies safe metadata such as elapsed
/// acquisition/source timing counts when available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientFailure<M> {
    /// Captured when request dispatch or full-body consumption settles, before
    /// any late server-telemetry drain or failure classification work.
    #[serde(skip)]
    pub request_completed_at: Instant,
    pub kind: ClientFailureKind,
    pub metadata: M,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientFailureKind {
    Transport,
    RequestTimeout,
    BodyRead,
}

/// A task panic/cancellation is represented independently of client failures;
/// it can never disappear merely because `JoinHandle::await` returned an
/// error. No panic payload or runtime error text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JoinFailure {
    pub cancelled: bool,
    pub panicked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum AttemptOutcome<M> {
    Response(CompletedHttpResponse<M>),
    ClientFailure(ClientFailure<M>),
    JoinFailure(JoinFailure),
}

impl<M> AttemptOutcome<M> {
    pub fn metadata(&self) -> Option<&M> {
        match self {
            Self::Response(response) => Some(&response.metadata),
            Self::ClientFailure(failure) => Some(&failure.metadata),
            Self::JoinFailure(_) => None,
        }
    }
}

/// Request timing is exact for a future that returned normally (including a
/// classified client failure). A panicked/cancelled task receives the elapsed
/// time observed at join as a documented upper bound, rather than being
/// silently omitted. An unavailable timing is explicitly counted in summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "duration")]
pub enum TimingCapture {
    Exact(Duration),
    JoinObservedUpperBound(Duration),
    Unavailable,
}

/// One raw attempt retained in ID order, independent of task completion order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttemptRecord<M> {
    pub context: AttemptContext,
    pub timing: TimingCapture,
    pub outcome: AttemptOutcome<M>,
}

/// One barrier-aligned wave. Warm-up and measured waves are separate values so
/// callers cannot accidentally calculate acceptance percentiles over both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveCapture<M> {
    pub arm: QueryArm,
    pub phase: AttemptPhase,
    pub wave: usize,
    pub concurrency: usize,
    pub attempts: Vec<AttemptRecord<M>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    EmptyWave,
    AttemptIdExhausted,
}

struct CompletedTask<M> {
    context: AttemptContext,
    duration: Duration,
    outcome: Result<CompletedHttpResponse<M>, ClientFailure<M>>,
}

/// Dispatches a single concurrent barrier wave. Every ID is reserved before
/// any task starts; every spawned task yields an `AttemptRecord`, including a
/// failed join. The supplied closure must perform all HTTP work through full
/// body consumption before resolving.
pub async fn run_wave<M, F, Fut>(
    ids: &mut AttemptIds,
    arm: QueryArm,
    phase: AttemptPhase,
    wave: usize,
    concurrency: usize,
    request: Arc<F>,
) -> Result<WaveCapture<M>, DriverError>
where
    M: Send + 'static,
    F: Fn(AttemptContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompletedHttpResponse<M>, ClientFailure<M>>> + Send + 'static,
{
    if concurrency == 0 {
        return Err(DriverError::EmptyWave);
    }

    let contexts = ids
        .reserve(concurrency)?
        .into_iter()
        .enumerate()
        .map(|(slot, id)| AttemptContext {
            id,
            arm,
            phase,
            wave,
            slot,
        })
        .collect::<Vec<_>>();
    let gate = Arc::new(Barrier::new(concurrency));
    let starts = Arc::new(Mutex::new(BTreeMap::<AttemptId, Instant>::new()));
    let mut tasks: Vec<(AttemptContext, JoinHandle<CompletedTask<M>>)> =
        Vec::with_capacity(concurrency);

    for context in contexts {
        let task_context = context;
        let gate = gate.clone();
        let starts = starts.clone();
        let request = request.clone();
        let handle = tokio::spawn(async move {
            gate.wait().await;
            let started = Instant::now();
            starts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(task_context.id, started);
            let outcome = request(task_context).await;
            let completed_at = match &outcome {
                Ok(response) => response.request_completed_at,
                Err(failure) => failure.request_completed_at,
            };
            CompletedTask {
                context: task_context,
                duration: completed_at.saturating_duration_since(started),
                outcome,
            }
        });
        tasks.push((context, handle));
    }

    let mut attempts = Vec::with_capacity(concurrency);
    for (context, task) in tasks {
        let record = match task.await {
            Ok(CompletedTask {
                context,
                duration,
                outcome,
            }) => AttemptRecord {
                context,
                timing: TimingCapture::Exact(duration),
                outcome: match outcome {
                    Ok(response) => AttemptOutcome::Response(response),
                    Err(failure) => AttemptOutcome::ClientFailure(failure),
                },
            },
            Err(error) => {
                let observed_since_start = starts
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&context.id)
                    .copied()
                    .map(|started| TimingCapture::JoinObservedUpperBound(started.elapsed()))
                    .unwrap_or(TimingCapture::Unavailable);
                AttemptRecord {
                    context,
                    timing: observed_since_start,
                    outcome: AttemptOutcome::JoinFailure(JoinFailure {
                        cancelled: error.is_cancelled(),
                        panicked: error.is_panic(),
                    }),
                }
            }
        };
        attempts.push(record);
    }
    attempts.sort_by_key(|attempt| attempt.context.id);

    Ok(WaveCapture {
        arm,
        phase,
        wave,
        concurrency,
        attempts,
    })
}

/// Executes serial samples as distinct one-request barrier waves. This retains
/// the same dispatch/timing machinery as concurrent samples while ensuring
/// each next request begins only after the previous one is fully accounted for.
pub async fn run_serial<M, F, Fut>(
    ids: &mut AttemptIds,
    arm: QueryArm,
    phase: AttemptPhase,
    first_wave: usize,
    attempts: usize,
    request: Arc<F>,
) -> Result<Vec<WaveCapture<M>>, DriverError>
where
    M: Send + 'static,
    F: Fn(AttemptContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompletedHttpResponse<M>, ClientFailure<M>>> + Send + 'static,
{
    let mut serial = Vec::with_capacity(attempts);
    for offset in 0..attempts {
        serial.push(run_wave(ids, arm, phase, first_wave + offset, 1, request.clone()).await?);
    }
    Ok(serial)
}

/// Nearest-rank percentile required by the Phase B protocol: for `n` sorted
/// observations and percentile `p`, select rank `ceil(p * n)` (one-based).
/// Invalid/empty inputs return `None` so a harness cannot invent a value.
pub fn nearest_rank_percentile(values: &[Duration], percentile: f64) -> Option<Duration> {
    if values.is_empty() || !percentile.is_finite() || percentile <= 0.0 || percentile > 1.0 {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let rank = ((percentile * values.len() as f64).ceil() as usize).max(1);
    values.get(rank - 1).copied()
}

/// A percentile summary exposes unavailable and upper-bound timing captures;
/// consumers cannot present p95 as if a join failure had simply vanished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimingSummary {
    pub observations: usize,
    pub exact_observations: usize,
    pub join_upper_bound_observations: usize,
    pub unavailable_observations: usize,
    pub p50: Option<Duration>,
    pub p95: Option<Duration>,
    pub max: Option<Duration>,
}

pub fn summarize_timings<I>(timings: I) -> TimingSummary
where
    I: IntoIterator<Item = TimingCapture>,
{
    let mut observations = 0;
    let mut exact_observations = 0;
    let mut join_upper_bound_observations = 0;
    let mut unavailable_observations = 0;
    let mut values = Vec::new();

    for timing in timings {
        observations += 1;
        match timing {
            TimingCapture::Exact(duration) => {
                exact_observations += 1;
                values.push(duration);
            }
            TimingCapture::JoinObservedUpperBound(duration) => {
                join_upper_bound_observations += 1;
                values.push(duration);
            }
            TimingCapture::Unavailable => unavailable_observations += 1,
        }
    }

    let max = values.iter().copied().max();
    TimingSummary {
        observations,
        exact_observations,
        join_upper_bound_observations,
        unavailable_observations,
        p50: nearest_rank_percentile(&values, 0.50),
        p95: nearest_rank_percentile(&values, 0.95),
        max,
    }
}

/// Counts and timing summaries derived from all retained attempts. The caller
/// supplies a safe duration extractor for metadata, for example per-source or
/// authentication/feed-acquisition durations. This module deliberately does
/// not know metadata's schema and never formats it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttemptAggregate {
    pub attempts: usize,
    pub responses: usize,
    pub client_failures: usize,
    pub join_failures: usize,
    pub complete_responses: usize,
    pub status_counts: BTreeMap<u16, usize>,
    pub result_counts: BTreeMap<HttpBodyResult, usize>,
    pub client_failure_counts: BTreeMap<ClientFailureKind, usize>,
    pub whole_source_evaluations: usize,
    pub request_timing: TimingSummary,
    pub caller_metadata_timing: TimingSummary,
}

pub fn aggregate_attempts<M, F, I>(
    attempts: &[AttemptRecord<M>],
    safe_durations: F,
) -> AttemptAggregate
where
    F: Fn(&M) -> I,
    I: IntoIterator<Item = Duration>,
{
    let mut responses = 0;
    let mut client_failures = 0;
    let mut join_failures = 0;
    let mut complete_responses = 0;
    let mut status_counts = BTreeMap::new();
    let mut result_counts = BTreeMap::new();
    let mut client_failure_counts = BTreeMap::new();
    let mut whole_source_evaluations = 0;
    let mut metadata_durations = Vec::new();

    for attempt in attempts {
        if let Some(metadata) = attempt.outcome.metadata() {
            metadata_durations.extend(safe_durations(metadata));
        }
        match &attempt.outcome {
            AttemptOutcome::Response(response) => {
                responses += 1;
                whole_source_evaluations += response.whole_source_evaluations;
                *status_counts.entry(response.status).or_default() += 1;
                *result_counts.entry(response.result).or_default() += 1;
                if response.status == HTTP_OK && response.result == HttpBodyResult::Complete {
                    complete_responses += 1;
                }
            }
            AttemptOutcome::ClientFailure(failure) => {
                client_failures += 1;
                *client_failure_counts.entry(failure.kind).or_default() += 1;
            }
            AttemptOutcome::JoinFailure(_) => join_failures += 1,
        }
    }

    AttemptAggregate {
        attempts: attempts.len(),
        responses,
        client_failures,
        join_failures,
        complete_responses,
        status_counts,
        result_counts,
        client_failure_counts,
        whole_source_evaluations,
        request_timing: summarize_timings(attempts.iter().map(|attempt| attempt.timing)),
        caller_metadata_timing: summarize_timings(
            metadata_durations.into_iter().map(TimingCapture::Exact),
        ),
    }
}

/// Expected values for a normal Phase B series. The source-evaluation count is
/// optional because zero-source and paired-original samples both legitimately
/// have zero source work; use `Some(0)` when that zero is itself an assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NormalCompletionGate {
    pub expected_attempts: usize,
    pub expected_whole_source_evaluations: Option<usize>,
}

/// Every defect that prevents a normal case from being called complete. The
/// report holds all violations rather than stopping at the first, preserving
/// every raw attempt for final evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormalCompletionReport {
    pub passed: bool,
    pub expected_attempts: usize,
    pub recorded_attempts: usize,
    pub normal_complete_attempts: usize,
    pub whole_source_evaluations: usize,
    pub violations: Vec<NormalCompletionViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NormalCompletionViolation {
    AttemptCount { expected: usize, actual: usize },
    DuplicateAttemptId { id: AttemptId },
    NonCompleteAttempt { id: AttemptId },
    WholeSourceEvaluationCount { expected: usize, actual: usize },
}

/// Performs the protocol's explicit normal-case completion check: each
/// expected attempt must be present exactly once and return HTTP 200 with a
/// fully consumed, complete envelope. Partial/unavailable/503/client/join
/// outcomes therefore fail rather than being excluded from p95 calculation.
pub fn check_normal_completion<M>(
    attempts: &[AttemptRecord<M>],
    gate: NormalCompletionGate,
) -> NormalCompletionReport {
    let aggregate = aggregate_attempts(attempts, |_| std::iter::empty::<Duration>());
    let mut violations = Vec::new();
    if attempts.len() != gate.expected_attempts {
        violations.push(NormalCompletionViolation::AttemptCount {
            expected: gate.expected_attempts,
            actual: attempts.len(),
        });
    }

    let mut ids = BTreeSet::new();
    for attempt in attempts {
        if !ids.insert(attempt.context.id) {
            violations.push(NormalCompletionViolation::DuplicateAttemptId {
                id: attempt.context.id,
            });
        }
        let normal_complete = matches!(
            &attempt.outcome,
            AttemptOutcome::Response(CompletedHttpResponse {
                status: HTTP_OK,
                result: HttpBodyResult::Complete,
                ..
            })
        );
        if !normal_complete {
            violations.push(NormalCompletionViolation::NonCompleteAttempt {
                id: attempt.context.id,
            });
        }
    }
    if let Some(expected) = gate.expected_whole_source_evaluations {
        if aggregate.whole_source_evaluations != expected {
            violations.push(NormalCompletionViolation::WholeSourceEvaluationCount {
                expected,
                actual: aggregate.whole_source_evaluations,
            });
        }
    }

    NormalCompletionReport {
        passed: violations.is_empty(),
        expected_attempts: gate.expected_attempts,
        recorded_attempts: attempts.len(),
        normal_complete_attempts: aggregate.complete_responses,
        whole_source_evaluations: aggregate.whole_source_evaluations,
        violations,
    }
}

/// Returns the request-p95 threshold for the protocol's allowed concurrency
/// levels. Callers receive `None` for a non-protocol load shape instead of an
/// invented limit.
pub const fn request_p95_limit(concurrency: usize) -> Option<Duration> {
    match concurrency {
        1 => Some(REQUEST_P95_CONCURRENCY_1),
        10 => Some(REQUEST_P95_CONCURRENCY_10),
        20 => Some(REQUEST_P95_CONCURRENCY_20),
        _ => None,
    }
}

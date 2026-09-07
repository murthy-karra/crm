//! Test-feature-only checkpoints for controlled Today query failure tests.
//! The hook is task-local: parallel tests cannot alter another query and no
//! HTTP input, environment switch, or production build can enable it.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodayQueryPhase {
    AfterBuiltins,
    AfterMetadata,
    SourceAfterSavepoint,
    SourceAfterMembership,
    BeforeSourceRelease,
    RecoveryAfterRollback,
    BeforeFinalCommit,
    /// docs/specs/SLICE_011d.md §5 step 4: the call feed's own checkpoints,
    /// mirroring the list-source `SourceAfter*`/`BeforeSourceRelease` triad.
    /// The call feed has no list id, so it always passes `source_id: None`
    /// — including at the shared `RecoveryAfterRollback` and
    /// `BeforeFinalCommit` phases it reuses, which a hook can therefore
    /// still target specifically by matching `source_id.is_none()` (every
    /// list source always passes `Some(id)` there).
    CallFeedAfterSavepoint,
    CallFeedAfterMembership,
    BeforeCallFeedRelease,
}

pub type HookFuture<'a> = Pin<Box<dyn Future<Output = Result<(), sqlx::Error>> + Send + 'a>>;

pub trait TodayQueryHook: Send + Sync {
    fn checkpoint<'a>(
        &'a self,
        phase: TodayQueryPhase,
        source_id: Option<Uuid>,
        deadline: Option<Instant>,
        connection: &'a mut PgConnection,
    ) -> HookFuture<'a>;
}

#[derive(Clone)]
pub struct TodayQueryHooks {
    hook: Arc<dyn TodayQueryHook>,
}

impl TodayQueryHooks {
    pub fn new(hook: Arc<dyn TodayQueryHook>) -> Self {
        Self { hook }
    }
}

tokio::task_local! {
    static ACTIVE_HOOKS: TodayQueryHooks;
}

pub async fn scope<F>(hooks: TodayQueryHooks, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_HOOKS.scope(hooks, future).await
}

pub async fn checkpoint(
    phase: TodayQueryPhase,
    source_id: Option<Uuid>,
    deadline: Option<Instant>,
    connection: &mut PgConnection,
) -> Result<(), sqlx::Error> {
    let hooks = ACTIVE_HOOKS.try_with(|hooks| hooks.clone()).ok();
    if let Some(hooks) = hooks {
        hooks
            .hook
            .checkpoint(phase, source_id, deadline, connection)
            .await?;
    }
    Ok(())
}

/// Test-only, request-scoped collection for the authenticated Phase B HTTP
/// harness. It holds no customer content: capture IDs are numeric, pool/source
/// events contain only bounded durations and typed outcomes, and filter kinds
/// are the fixed v1 vocabulary.
#[derive(Clone, Default)]
pub struct HttpPerfCollector {
    captures: Arc<Mutex<HashMap<u64, Arc<HttpPerfCapture>>>>,
}

/// The way a request-scoped capture became observable to the harness. A
/// client-side failure is never silently treated as a complete request: the
/// collector either waited for the server scope to quiesce or retained the
/// capture until its bounded drain elapsed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HttpPerfCaptureTerminal {
    #[default]
    CompleteBody,
    ClientFailureQuiescent,
    ClientFailureDrainTimedOut,
}

#[derive(Debug, Clone, Default)]
pub struct HttpPerfTelemetry {
    pub terminal: HttpPerfCaptureTerminal,
    pub authentication_pool_acquisitions: Vec<PoolAcquisition>,
    pub feed_pool_acquisitions: Vec<PoolAcquisition>,
    pub source_enumeration: Vec<SourceEnumeration>,
    pub source_evaluations: Vec<SourceEvaluation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolAcquisitionOutcome {
    Acquired,
    TimedOut,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolAcquisition {
    pub outcome: PoolAcquisitionOutcome,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEnumerationOutcome {
    Complete,
    Unavailable,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceEnumeration {
    pub outcome: SourceEnumerationOutcome,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEvaluationOutcome {
    Complete,
    InvalidFilter,
    Unavailable,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvaluation {
    pub outcome: SourceEvaluationOutcome,
    pub duration: Duration,
    pub filter_kinds: Vec<&'static str>,
}

struct HttpPerfCapture {
    telemetry: Mutex<HttpPerfTelemetry>,
    active_scopes: AtomicUsize,
    ever_active: AtomicBool,
}

impl Default for HttpPerfCapture {
    fn default() -> Self {
        Self {
            telemetry: Mutex::new(HttpPerfTelemetry::default()),
            active_scopes: AtomicUsize::new(0),
            ever_active: AtomicBool::new(false),
        }
    }
}

struct PerfCaptureScopeGuard(Arc<HttpPerfCapture>);

impl Drop for PerfCaptureScopeGuard {
    fn drop(&mut self) {
        self.0.active_scopes.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
struct ActivePerfCapture {
    capture: Arc<HttpPerfCapture>,
}

tokio::task_local! {
    static ACTIVE_PERF_CAPTURE: ActivePerfCapture;
}

impl HttpPerfCollector {
    /// Allocates the safe capture before the client starts I/O. A failed
    /// client can therefore keep the entry alive while a server task that
    /// already received the request finishes recording its outcome.
    pub fn begin_capture(&self, capture_id: u64) {
        let _ = self.capture(capture_id);
    }

    pub async fn scope<F>(&self, capture_id: u64, future: F) -> F::Output
    where
        F: Future,
    {
        let capture = self.capture(capture_id);
        // Publish an active scope before declaring the capture eligible for
        // quiescence. A client-failure drain seeing `ever_active` must never
        // observe a transient zero count and detach this still-starting
        // server request from its capture.
        capture.active_scopes.fetch_add(1, Ordering::AcqRel);
        capture.ever_active.store(true, Ordering::Release);
        let guard = PerfCaptureScopeGuard(capture.clone());
        ACTIVE_PERF_CAPTURE
            .scope(ActivePerfCapture { capture }, async move {
                let _guard = guard;
                future.await
            })
            .await
    }

    /// Removes all captured safe events for an attempt. The HTTP harness calls
    /// this only after it has received the complete response body.
    pub fn take(&self, capture_id: u64) -> HttpPerfTelemetry {
        let capture = self
            .captures
            .lock()
            .expect("HTTP perf collector lock")
            .remove(&capture_id)
            .unwrap_or_default();
        let mut telemetry = capture
            .telemetry
            .lock()
            .expect("HTTP perf capture lock")
            .clone();
        telemetry.terminal = HttpPerfCaptureTerminal::CompleteBody;
        telemetry
    }

    /// Retains a capture after a client transport/body failure until the
    /// server request scope has ended, or `limit` expires. This bounded drain
    /// is deliberately outside the driver's HTTP timing boundary. A timed-out
    /// capture stays registered so a late server event cannot recreate an
    /// uncorrelated entry; its returned terminal state makes the incomplete
    /// observation explicit in raw evidence.
    pub async fn drain_after_client_failure(
        &self,
        capture_id: u64,
        limit: Duration,
    ) -> HttpPerfTelemetry {
        let capture = self.capture(capture_id);
        let deadline = tokio::time::Instant::now() + limit;

        loop {
            if capture.ever_active.load(Ordering::Acquire)
                && capture.active_scopes.load(Ordering::Acquire) == 0
            {
                return self.remove_with_terminal(
                    capture_id,
                    &capture,
                    HttpPerfCaptureTerminal::ClientFailureQuiescent,
                );
            }

            if tokio::time::Instant::now() >= deadline {
                return Self::snapshot_with_terminal(
                    &capture,
                    HttpPerfCaptureTerminal::ClientFailureDrainTimedOut,
                );
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            // A server scope may start after the client has already failed
            // its write/read. Polling is bounded and avoids relying on a
            // notification registration race for that transition.
            tokio::time::sleep(remaining.min(Duration::from_millis(10))).await;
        }
    }

    fn capture(&self, capture_id: u64) -> Arc<HttpPerfCapture> {
        let mut captures = self.captures.lock().expect("HTTP perf collector lock");
        captures
            .entry(capture_id)
            .or_insert_with(|| Arc::new(HttpPerfCapture::default()))
            .clone()
    }

    fn remove_with_terminal(
        &self,
        capture_id: u64,
        expected: &Arc<HttpPerfCapture>,
        terminal: HttpPerfCaptureTerminal,
    ) -> HttpPerfTelemetry {
        let capture = {
            let mut captures = self.captures.lock().expect("HTTP perf collector lock");
            match captures.get(&capture_id) {
                Some(actual) if Arc::ptr_eq(actual, expected) => captures
                    .remove(&capture_id)
                    .expect("matching capture remains present"),
                _ => expected.clone(),
            }
        };
        Self::snapshot_with_terminal(&capture, terminal)
    }

    fn snapshot_with_terminal(
        capture: &HttpPerfCapture,
        terminal: HttpPerfCaptureTerminal,
    ) -> HttpPerfTelemetry {
        let mut telemetry = capture
            .telemetry
            .lock()
            .expect("HTTP perf capture lock")
            .clone();
        telemetry.terminal = terminal;
        telemetry
    }

    fn record(capture: &HttpPerfCapture, apply: impl FnOnce(&mut HttpPerfTelemetry)) {
        apply(&mut capture.telemetry.lock().expect("HTTP perf capture lock"));
    }
}

fn with_active_capture(apply: impl FnOnce(&HttpPerfCapture)) {
    let _ = ACTIVE_PERF_CAPTURE.try_with(|active| apply(&active.capture));
}

pub fn record_authentication_pool_acquisition(outcome: PoolAcquisitionOutcome, duration: Duration) {
    with_active_capture(|capture| {
        HttpPerfCollector::record(capture, |telemetry| {
            telemetry
                .authentication_pool_acquisitions
                .push(PoolAcquisition { outcome, duration });
        });
    });
}

pub fn record_feed_pool_acquisition(outcome: PoolAcquisitionOutcome, duration: Duration) {
    with_active_capture(|capture| {
        HttpPerfCollector::record(capture, |telemetry| {
            telemetry
                .feed_pool_acquisitions
                .push(PoolAcquisition { outcome, duration });
        });
    });
}

pub fn record_source_enumeration(outcome: SourceEnumerationOutcome, duration: Duration) {
    with_active_capture(|capture| {
        HttpPerfCollector::record(capture, |telemetry| {
            telemetry
                .source_enumeration
                .push(SourceEnumeration { outcome, duration });
        });
    });
}

pub fn record_source_evaluation(
    outcome: SourceEvaluationOutcome,
    duration: Duration,
    filter_kinds: Vec<&'static str>,
) {
    with_active_capture(|capture| {
        HttpPerfCollector::record(capture, |telemetry| {
            telemetry.source_evaluations.push(SourceEvaluation {
                outcome,
                duration,
                filter_kinds,
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::{
        record_authentication_pool_acquisition, record_feed_pool_acquisition,
        HttpPerfCaptureTerminal, HttpPerfCollector, PoolAcquisitionOutcome,
    };

    #[tokio::test]
    async fn client_failure_drain_retains_events_until_the_server_scope_quiesces() {
        let collector = HttpPerfCollector::default();
        collector.begin_capture(41);
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let server_collector = collector.clone();
        let server = tokio::spawn(async move {
            server_collector
                .scope(41, async move {
                    record_authentication_pool_acquisition(
                        PoolAcquisitionOutcome::Acquired,
                        Duration::from_millis(2),
                    );
                    entered_tx
                        .send(())
                        .expect("drain test sees active server scope");
                    release_rx.await.expect("release active server scope");
                    record_feed_pool_acquisition(
                        PoolAcquisitionOutcome::Acquired,
                        Duration::from_millis(3),
                    );
                })
                .await;
        });
        entered_rx.await.expect("server scope entered");

        let drain_collector = collector.clone();
        let drain = tokio::spawn(async move {
            drain_collector
                .drain_after_client_failure(41, Duration::from_millis(200))
                .await
        });
        tokio::task::yield_now().await;
        assert!(
            !drain.is_finished(),
            "a client failure cannot detach telemetry while its server scope remains active"
        );
        release_tx.send(()).expect("release server scope");
        server.await.expect("server scope joins");
        let telemetry = drain.await.expect("drain joins");
        assert_eq!(
            telemetry.terminal,
            HttpPerfCaptureTerminal::ClientFailureQuiescent
        );
        assert_eq!(telemetry.authentication_pool_acquisitions.len(), 1);
        assert_eq!(telemetry.feed_pool_acquisitions.len(), 1);
    }

    #[tokio::test]
    async fn timed_out_drain_keeps_the_capture_for_a_late_server_scope() {
        let collector = HttpPerfCollector::default();
        collector.begin_capture(42);
        let initial = collector
            .drain_after_client_failure(42, Duration::from_millis(1))
            .await;
        assert_eq!(
            initial.terminal,
            HttpPerfCaptureTerminal::ClientFailureDrainTimedOut
        );

        collector
            .scope(42, async {
                record_feed_pool_acquisition(
                    PoolAcquisitionOutcome::Acquired,
                    Duration::from_millis(4),
                );
            })
            .await;
        let late = collector.take(42);
        assert_eq!(late.terminal, HttpPerfCaptureTerminal::CompleteBody);
        assert_eq!(late.feed_pool_acquisitions.len(), 1);
    }
}

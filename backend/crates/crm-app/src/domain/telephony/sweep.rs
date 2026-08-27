//! The sweep (docs/specs/SLICE_006.md §3, §9): a 30 s `tokio` interval,
//! in-process, one pod (§12 defers multi-pod coordination), started from
//! `run()` only when telephony is enabled — never by the test router.
//! Per-state horizons: `placing` older than `10 s + 30 s` →
//! `failed{expired}`; `ringing` older than `ring_timeout + 30 s` →
//! `failed{expired}`; `answered` older than `max_call + 60 s` →
//! `ended{reconciled}`. Every finalisation goes through `settle` (the one
//! write path), then a best-effort `provider.hangup`. No room-existence
//! query. `run_once` is the unit the DB-backed tests drive directly.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::domain::telephony::settle::settle;
use crate::domain::telephony::transitions::Signal;
use crate::domain::telephony::CallStatus;
use crate::ids::{CallId, OrganizationId};
use crate::realtime::Publisher;
use crate::telephony::{Telephony, DEFAULT_AGENT_JOIN_TIMEOUT};

pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);
/// Slack added to the `placing` and `ringing` horizons.
pub const EXPIRY_GRACE: Duration = Duration::from_secs(30);
/// Slack added to the `answered` horizon.
pub const RECONCILE_GRACE: Duration = Duration::from_secs(60);

/// What one pass did — the `finalized` span field and a test observable.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    /// Candidates the horizon query returned.
    pub candidates: usize,
    /// Calls moved to a terminal state by this pass.
    pub finalized: usize,
    /// Best-effort room deletes that failed (already finalised).
    pub hangup_failures: usize,
}

struct Candidate {
    organization_id: Uuid,
    id: Uuid,
    status: String,
}

/// Spawns the periodic sweep. The first pass runs after one interval, not
/// immediately, so a restart does not race calls that are mid-dial.
pub fn spawn(pool: PgPool, publisher: Publisher, telephony: Arc<Telephony>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = run_once(&pool, &publisher, &telephony, Utc::now()).await {
                tracing::warn!(error = %err, "call sweep failed");
            }
        }
    })
}

/// One pass at `now`. Finalises every active call past its horizon.
#[tracing::instrument(name = "call.sweep", skip_all, fields(finalized = tracing::field::Empty))]
pub async fn run_once(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Telephony,
    now: DateTime<Utc>,
) -> Result<SweepReport, sqlx::Error> {
    let limits = &telephony.limits;
    let placing_before = now - to_chrono(DEFAULT_AGENT_JOIN_TIMEOUT + EXPIRY_GRACE);
    let ringing_before = now - to_chrono(limits.ring_timeout + EXPIRY_GRACE);
    let answered_before = now - to_chrono(limits.max_call + RECONCILE_GRACE);

    // The partial unique index (`status IN (...)`) serves this scan.
    let candidates = sqlx::query_as!(
        Candidate,
        r#"SELECT organization_id, id, status FROM call
           WHERE (status = 'placing' AND placed_at < $1)
              OR (status = 'ringing' AND ringing_at < $2)
              OR (status = 'answered' AND answered_at < $3)
           ORDER BY placed_at"#,
        placing_before,
        ringing_before,
        answered_before,
    )
    .fetch_all(pool)
    .await?;

    let mut report = SweepReport {
        candidates: candidates.len(),
        ..SweepReport::default()
    };

    for candidate in candidates {
        let signal = match CallStatus::decode(&candidate.status) {
            Some(CallStatus::Placing | CallStatus::Ringing) => Signal::Expired,
            Some(CallStatus::Answered) => Signal::Reconciled,
            _ => continue,
        };
        // `settle` re-reads the locked status: a call that moved on since
        // the scan is a no-op here.
        let outcome = match settle(
            pool,
            publisher,
            OrganizationId::new(candidate.organization_id),
            CallId::new(candidate.id),
            &signal,
            now,
        )
        .await
        {
            Ok(Some(outcome)) if !outcome.is_noop() => outcome,
            Ok(_) => continue,
            Err(err) => {
                tracing::warn!(call_id = %candidate.id, error = %err, "sweep settle failed");
                continue;
            }
        };
        report.finalized += 1;
        tracing::info!(
            call_id = %candidate.id,
            organization_id = %candidate.organization_id,
            from = %candidate.status,
            transition = crate::domain::telephony::settle::transition_tag(&outcome.transition),
            "sweep finalized a call"
        );
        let room = Telephony::room_for(candidate.id);
        if let Err(err) = telephony.provider.hangup(&room).await {
            report.hangup_failures += 1;
            tracing::warn!(call_id = %candidate.id, error_kind = err.kind(), "sweep room delete failed");
        }
    }

    tracing::Span::current().record("finalized", report.finalized);
    Ok(report)
}

fn to_chrono(d: Duration) -> chrono::Duration {
    chrono::Duration::from_std(d).unwrap_or(chrono::Duration::MAX)
}

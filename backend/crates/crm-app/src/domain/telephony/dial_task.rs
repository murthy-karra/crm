//! The dial task (docs/specs/SLICE_006.md §3), spawned by `dial_call`
//! under the request's span: wait for the agent to be in the room, move
//! the call to `ringing`, dial the PSTN leg through the provider, and
//! settle whatever comes back. Bounded by `agent_join_timeout +
//! ring_timeout + DIAL_SETTLE_GRACE`. Reads the number from
//! `contact_method.normalized_value` at dial time and never logs it.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::Instrument;
use uuid::Uuid;

use crate::domain::telephony::queries;
use crate::domain::telephony::settle::{settle, SettleOutcome};
use crate::domain::telephony::transitions::Signal;
use crate::domain::telephony::CallStatus;
use crate::ids::{OrganizationId, UserId};
use crate::realtime::Publisher;
use crate::telephony::livekit::ADMIN_CALL_TIMEOUT;
use crate::telephony::{
    DialOutcome, DialRequest, PhoneNumber, ProviderError, Telephony, DIAL_SETTLE_GRACE,
};

/// Everything the task needs, captured at `dial_call` time — the seam the
/// LiveKit provider (Lane A step 4) drops into without touching this file.
pub struct DialTask {
    pub pool: PgPool,
    pub publisher: Publisher,
    pub telephony: Arc<Telephony>,
    pub organization_id: OrganizationId,
    pub call_id: Uuid,
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
    pub caller_user_id: UserId,
}

/// How the task ended — the `outcome` span field and a test observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialTaskOutcome {
    Answered,
    /// Answered, then the callee was already gone at the re-check.
    AnsweredRemoteLeft,
    DialFailed,
    AgentNotJoined,
    ProviderError,
    /// The call was no longer `placing` when the task got to it (the
    /// caller hung up first); nothing written.
    Superseded,
    /// The call row vanished (cannot happen via the application path).
    UnknownCall,
}

impl DialTaskOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            DialTaskOutcome::Answered => "answered",
            DialTaskOutcome::AnsweredRemoteLeft => "answered_remote_left",
            DialTaskOutcome::DialFailed => "dial_failed",
            DialTaskOutcome::AgentNotJoined => "agent_not_joined",
            DialTaskOutcome::ProviderError => "provider_error",
            DialTaskOutcome::Superseded => "superseded",
            DialTaskOutcome::UnknownCall => "unknown_call",
        }
    }
}

impl DialTask {
    /// Spawns the task on the current runtime, instrumented with the
    /// calling span so `call_id`/`correlation_id` reach every line it
    /// logs (docs/specs/SLICE_006.md §3).
    pub fn spawn(self) -> JoinHandle<DialTaskOutcome> {
        tokio::spawn(self.run().instrument(tracing::Span::current()))
    }

    /// The true sum of the inner bounds: the join wait plus its last
    /// in-flight presence call, the pre-dial presence re-check, the dial
    /// (`ring_timeout + DIAL_SETTLE_GRACE`), the post-answer re-check, and
    /// one more grace for the settles. The outer timeout is a safety net
    /// only; it can no longer fire while an inner step is still within
    /// its own bound.
    pub fn total_budget(&self) -> Duration {
        let limits = &self.telephony.limits;
        limits.agent_join_timeout
            + ADMIN_CALL_TIMEOUT * 3
            + limits.ring_timeout
            + DIAL_SETTLE_GRACE * 2
    }

    /// Runs to completion; the outer budget turns a hung provider into
    /// `failed{provider_error}` rather than a leaked task.
    pub async fn run(self) -> DialTaskOutcome {
        let budget = self.total_budget();
        let room = Telephony::room_for(self.call_id);
        let outcome = match tokio::time::timeout(budget, self.run_inner(&room)).await {
            Ok(outcome) => outcome,
            Err(_elapsed) => {
                tracing::warn!(call_id = %self.call_id, "dial task exceeded its budget");
                self.settle_terminal(&Signal::ProviderError, &room).await;
                DialTaskOutcome::ProviderError
            }
        };
        tracing::info!(call_id = %self.call_id, outcome = outcome.as_str(), "dial task finished");
        outcome
    }

    async fn run_inner(&self, room: &str) -> DialTaskOutcome {
        let limits = &self.telephony.limits;
        let provider = &self.telephony.provider;
        let agent = Telephony::agent_identity(self.caller_user_id.as_uuid());

        // 1. Wait ≤ agent_join_timeout for the browser to be in the room.
        let deadline = Instant::now() + limits.agent_join_timeout;
        loop {
            match provider.participant_present(room, &agent).await {
                Ok(true) => break,
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(call_id = %self.call_id, error_kind = err.kind(), "presence check failed");
                    self.settle_terminal(&Signal::ProviderError, room).await;
                    return DialTaskOutcome::ProviderError;
                }
            }
            if Instant::now() >= deadline {
                self.settle_terminal(&Signal::AgentNotJoined, room).await;
                return DialTaskOutcome::AgentNotJoined;
            }
            tokio::time::sleep(limits.presence_poll_interval).await;
        }

        // 2. The number, read now and never logged.
        let mut conn = match self.pool.acquire().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(call_id = %self.call_id, error = %err, "dial task: pool acquire failed");
                self.settle_terminal(&Signal::ProviderError, room).await;
                return DialTaskOutcome::ProviderError;
            }
        };
        let number = queries::phone_contact_method_normalized(
            &mut conn,
            self.organization_id,
            self.person_id,
            self.contact_method_id,
        )
        .await;
        drop(conn);
        let number = match number {
            Ok(Some(number)) => PhoneNumber::new(number),
            Ok(None) => {
                tracing::warn!(call_id = %self.call_id, "dial task: contact method no longer exists");
                self.settle_terminal(&Signal::ProviderError, room).await;
                return DialTaskOutcome::ProviderError;
            }
            Err(err) => {
                tracing::warn!(call_id = %self.call_id, error = %err, "dial task: contact method read failed");
                self.settle_terminal(&Signal::ProviderError, room).await;
                return DialTaskOutcome::ProviderError;
            }
        };

        // 3. One last presence check while still `placing`, immediately
        // before the leg is dialed: an agent that left in the meantime
        // settles `failed{agent_not_joined}` (no attempt) rather than
        // ringing a callee into an empty room.
        match provider.participant_present(room, &agent).await {
            Ok(true) => {}
            Ok(false) => {
                self.settle_terminal(&Signal::AgentNotJoined, room).await;
                return DialTaskOutcome::AgentNotJoined;
            }
            Err(err) => {
                tracing::warn!(call_id = %self.call_id, error_kind = err.kind(), "pre-dial presence check failed");
                self.settle_terminal(&Signal::ProviderError, room).await;
                return DialTaskOutcome::ProviderError;
            }
        }

        // placing → ringing. A no-op means the caller already hung up.
        match self.settle_signal(&Signal::Dialing).await {
            Some(outcome) if !outcome.is_noop() => {}
            Some(_) => return DialTaskOutcome::Superseded,
            None => return DialTaskOutcome::UnknownCall,
        }

        // 4. Dial. `participant_identity = "sip:<call_id>"`.
        let request = DialRequest {
            room: room.to_string(),
            to_number: number,
            participant_identity: Telephony::sip_identity(self.call_id),
            ring_timeout: limits.ring_timeout,
            max_call: limits.max_call,
        };
        let dial_budget = limits.ring_timeout + DIAL_SETTLE_GRACE;
        let result = match tokio::time::timeout(dial_budget, provider.dial(request)).await {
            Ok(result) => result,
            Err(_elapsed) => Err(ProviderError::Timeout),
        };

        match result {
            Ok(DialOutcome::Answered { call_ref }) => {
                tracing::Span::current().record("sip_status_class", "2xx");
                match self.settle_signal(&Signal::Answered { call_ref }).await {
                    Some(outcome) if !outcome.is_noop() => {}
                    // Hung up while the callee was answering: the terminal
                    // state is already recorded; make sure the room is gone.
                    Some(_) => {
                        self.hangup_best_effort(room).await;
                        return DialTaskOutcome::Superseded;
                    }
                    None => return DialTaskOutcome::UnknownCall,
                }
                // 5. One re-check: a sub-second answer-and-hangup race.
                let sip = Telephony::sip_identity(self.call_id);
                match provider.participant_present(room, &sip).await {
                    Ok(true) => DialTaskOutcome::Answered,
                    Ok(false) => {
                        self.settle_terminal(&Signal::RemoteLeft, room).await;
                        DialTaskOutcome::AnsweredRemoteLeft
                    }
                    Err(err) => {
                        tracing::warn!(call_id = %self.call_id, error_kind = err.kind(), "post-answer presence check failed");
                        DialTaskOutcome::Answered
                    }
                }
            }
            Ok(DialOutcome::Failed(failure)) => {
                if let Some(class) = failure.status_class() {
                    tracing::Span::current().record("sip_status_class", class);
                }
                let signal = Signal::from_sip_failure(failure);
                self.settle_terminal(&signal, room).await;
                if matches!(signal, Signal::ProviderError) {
                    DialTaskOutcome::ProviderError
                } else {
                    DialTaskOutcome::DialFailed
                }
            }
            Err(err) => {
                tracing::warn!(call_id = %self.call_id, error_kind = err.kind(), "dial failed at the provider");
                self.settle_terminal(&Signal::ProviderError, room).await;
                DialTaskOutcome::ProviderError
            }
        }
    }

    async fn settle_signal(&self, signal: &Signal) -> Option<SettleOutcome> {
        match settle(
            &self.pool,
            &self.publisher,
            self.organization_id,
            self.call_id,
            signal,
            Utc::now(),
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                tracing::error!(call_id = %self.call_id, signal = signal.kind(), error = %err, "settle failed");
                None
            }
        }
    }

    /// Settles a terminal signal, then deletes the room best-effort so
    /// the browser sees the disconnect (docs/specs/SLICE_006.md §9) — but
    /// never the room of a live `answered` call: when the settle was a
    /// no-op against `answered` (or failed, so the state is unknown) the
    /// room is left alone for `hangup`/the webhook/the sweep.
    async fn settle_terminal(&self, signal: &Signal, room: &str) {
        let outcome = self.settle_signal(signal).await;
        let safe_to_delete = match &outcome {
            Some(outcome) => !outcome.is_noop() || outcome.call.status != CallStatus::Answered,
            None => false,
        };
        if safe_to_delete {
            self.hangup_best_effort(room).await;
        }
    }

    async fn hangup_best_effort(&self, room: &str) {
        if let Err(err) = self.telephony.provider.hangup(room).await {
            tracing::warn!(call_id = %self.call_id, error_kind = err.kind(), "room delete failed");
        }
    }
}

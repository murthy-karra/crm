//! The `crm-api` side of the Operator (docs/specs/SLICE_005.md §4, §7,
//! §8; D-028 §5): the runtime held in `AppState` (service + concurrency
//! guards), the `ToolBackend` adapter, the explanation builder, and the
//! PII-free ledger writer (D-029).

pub mod backend;
pub mod explain;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::config::Config;
use crate::domain::envelope::{ActorKind, Origin};
use crm_operator::{
    GroqConfig, GroqProvider, InferenceProvider, Limits, OperatorContext, OperatorService,
    ScreenRoute, TurnOutput,
};

pub use backend::SqlxToolBackend;

/// What `AppState.operator` holds when a provider is configured.
pub struct OperatorRuntime {
    pub service: OperatorService,
    semaphore: Arc<Semaphore>,
    in_flight: Mutex<HashSet<Uuid>>,
    /// `start_call` proposal lifetime (docs/specs/SLICE_006b.md §2),
    /// threaded to `SqlxToolBackend` at turn time.
    proposal_ttl: Duration,
}

/// Both concurrency entries, released by RAII (docs/specs/SLICE_005.md
/// §7): the server-wide permit and the per-user in-flight marker. Held by
/// the spawned turn task, so a client disconnect cannot leak either.
pub struct TurnSlot {
    _permit: OwnedSemaphorePermit,
    in_flight: InFlightRelease,
}

struct InFlightRelease {
    runtime: Arc<OperatorRuntime>,
    user_id: Uuid,
}

impl Drop for InFlightRelease {
    fn drop(&mut self) {
        self.runtime
            .in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.user_id);
    }
}

impl TurnSlot {
    /// Kept so the slot is observably alive for the task's whole lifetime.
    pub fn user_id(&self) -> Uuid {
        self.in_flight.user_id
    }
}

impl std::fmt::Debug for TurnSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnSlot")
            .field("user_id", &self.in_flight.user_id)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    /// Server cap reached or this user already has a turn in flight.
    Busy,
}

impl OperatorRuntime {
    pub fn new(service: OperatorService, max_concurrent: usize) -> Self {
        Self::with_proposal_ttl(service, max_concurrent, Duration::from_secs(120))
    }

    pub fn with_proposal_ttl(
        service: OperatorService,
        max_concurrent: usize,
        proposal_ttl: Duration,
    ) -> Self {
        Self {
            service,
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            in_flight: Mutex::new(HashSet::new()),
            proposal_ttl,
        }
    }

    /// `None` when `GROQ_API_KEY` is unset (docs/specs/SLICE_005.md §9).
    pub fn from_config(config: &Config) -> Option<Self> {
        let api_key = config.groq_api_key.clone()?;
        let provider = GroqProvider::new(GroqConfig {
            base_url: config.operator.base_url.clone(),
            model: config.operator.model.clone(),
            api_key,
            call_timeout: config.operator.call_timeout,
            connect_timeout: crm_operator::providers::groq::DEFAULT_CONNECT_TIMEOUT,
        });
        let limits = Limits {
            turn_timeout: config.operator.turn_timeout,
            ..Limits::default()
        };
        Some(Self::with_proposal_ttl(
            OperatorService::new(Arc::new(provider), limits),
            config.operator.max_concurrent,
            config.operator.proposal_ttl,
        ))
    }

    /// Test-support: a runtime over any provider with explicit limits.
    pub fn with_provider(
        provider: Arc<dyn InferenceProvider>,
        limits: Limits,
        max_concurrent: usize,
    ) -> Self {
        Self::new(OperatorService::new(provider, limits), max_concurrent)
    }

    pub fn turn_timeout(&self) -> Duration {
        self.service.limits().turn_timeout
    }

    pub fn proposal_ttl(&self) -> Duration {
        self.proposal_ttl
    }

    /// Acquires both guards or fails fast with `Busy` — never waits
    /// (docs/specs/SLICE_005.md §7, §9).
    pub fn try_acquire(self: &Arc<Self>, user_id: Uuid) -> Result<TurnSlot, SlotError> {
        let permit = self
            .semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| SlotError::Busy)?;
        {
            let mut set = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
            if !set.insert(user_id) {
                return Err(SlotError::Busy);
            }
        }
        Ok(TurnSlot {
            _permit: permit,
            in_flight: InFlightRelease {
                runtime: Arc::clone(self),
                user_id,
            },
        })
    }

    /// Observability for tests: how many permits remain.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn is_in_flight(&self, user_id: Uuid) -> bool {
        self.in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&user_id)
    }
}

/// What the ledger records about one turn besides the `TurnOutput`
/// (docs/specs/SLICE_005.md §2). No message, reply, argument, search
/// string, or history text (D-029).
pub struct TurnRecord<'a> {
    pub ctx: &'a OperatorContext,
    pub output: &'a TurnOutput,
    pub provider: &'a str,
    pub model: &'a str,
    pub completed_at: DateTime<Utc>,
    pub context_route: ScreenRoute,
}

fn route_str(route: ScreenRoute) -> &'static str {
    match route {
        ScreenRoute::Today => "today",
        ScreenRoute::Person => "person",
        ScreenRoute::People => "people",
        ScreenRoute::Other => "other",
    }
}

fn clamp_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// One `operator_turn` row plus N `operator_tool_call` rows in one
/// transaction, written after the turn from the same `TurnOutput` the
/// response is built from. Rejected (429) requests never reach here.
pub async fn record_turn(pool: &PgPool, record: TurnRecord<'_>) -> Result<(), sqlx::Error> {
    let ctx = record.ctx;
    let output = record.output;
    let mut tx = pool.begin().await?;

    let tool_call_count = i32::try_from(output.tool_calls.len())
        .map_err(|_| sqlx::Error::Protocol("tool_call_count overflow".into()))?;

    sqlx::query!(
        r#"INSERT INTO operator_turn
            (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id, corrects_id, completed_at, outcome,
             provider, model, prompt_tokens, completion_tokens, model_call_count,
             tool_call_count, context_route)
           VALUES ($1, $2, $3, $4, NULL, $5, $6, $1, NULL, NULL, $7, $8, $9, $10, $11, $12, $13, $14, $15)"#,
        ctx.turn_id,
        ctx.organization_id,
        ActorKind::User.as_str(),
        ctx.actor_user_id,
        Origin::Operator.as_str(),
        ctx.now,
        record.completed_at,
        output.outcome.as_str(),
        record.provider,
        record.model,
        output.usage.prompt_tokens.map(clamp_i32),
        output.usage.completion_tokens.map(clamp_i32),
        clamp_i32(output.model_call_count),
        tool_call_count,
        route_str(record.context_route),
    )
    .execute(&mut *tx)
    .await?;

    for (index, call) in output.tool_calls.iter().enumerate() {
        let seq = i16::try_from(index).map_err(|_| sqlx::Error::Protocol("seq overflow".into()))?;
        sqlx::query!(
            r#"INSERT INTO operator_tool_call (turn_id, seq, tool_name, outcome, duration_ms, person_ids)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            ctx.turn_id,
            seq,
            call.name,
            call.outcome.as_str(),
            clamp_i32(call.duration_ms),
            &call.person_ids,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crm_operator::ScriptedProvider;

    fn runtime(max_concurrent: usize) -> Arc<OperatorRuntime> {
        Arc::new(OperatorRuntime::with_provider(
            Arc::new(ScriptedProvider::new(vec![])),
            Limits::default(),
            max_concurrent,
        ))
    }

    #[test]
    fn same_user_cannot_hold_two_slots_and_release_is_raii() {
        let rt = runtime(4);
        let alice = Uuid::new_v4();
        let slot = rt.try_acquire(alice).unwrap();
        assert_eq!(slot.user_id(), alice);
        assert_eq!(rt.try_acquire(alice).unwrap_err(), SlotError::Busy);
        assert!(rt.is_in_flight(alice));
        assert_eq!(rt.available_permits(), 3);
        drop(slot);
        assert!(!rt.is_in_flight(alice));
        assert_eq!(rt.available_permits(), 4);
        assert!(rt.try_acquire(alice).is_ok());
    }

    #[test]
    fn semaphore_full_is_busy_and_does_not_mark_the_user() {
        let rt = runtime(1);
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        let _slot = rt.try_acquire(alice).unwrap();
        assert_eq!(rt.try_acquire(bob).unwrap_err(), SlotError::Busy);
        assert!(!rt.is_in_flight(bob));
    }

    #[test]
    fn a_rejected_same_user_attempt_does_not_consume_a_permit() {
        let rt = runtime(2);
        let alice = Uuid::new_v4();
        let _slot = rt.try_acquire(alice).unwrap();
        assert_eq!(rt.try_acquire(alice).unwrap_err(), SlotError::Busy);
        assert_eq!(rt.available_permits(), 1);
    }
}

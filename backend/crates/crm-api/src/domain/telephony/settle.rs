//! `settle` — the **one** write path for every call signal
//! (docs/specs/SLICE_006.md §2, §3): commands, the dial task, the webhook,
//! and the sweep all come through here. It takes `SELECT … FOR UPDATE` on
//! the `call` row and applies the transition to the *locked* status, which
//! is what makes "first signal wins, the other is a no-op" true across
//! concurrent sources. In the same transaction it writes the D-031
//! `contact_attempted` and, on a terminal transition, the `call_completed`
//! fact; the publications it returns go out only after commit.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::envelope::{ActorKind, FactEnvelope, Origin};
use crate::domain::facts::{self, CallCompletedFact, ContactAttemptedFact};
use crate::domain::telephony::queries::{self, CallRow};
use crate::domain::telephony::transitions::{apply, Signal, Transition};
use crate::domain::telephony::CallStatus;
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

/// What one `settle_in_tx` decided and what to publish after commit.
#[derive(Debug)]
pub struct SettleOutcome {
    /// The row as it is after the transition (unchanged on `NoOp`).
    pub call: CallRow,
    pub transition: Transition,
    /// `call.changed`, then `person.changed{contact_attempted}` when an
    /// attempt was written. Empty on `NoOp`.
    pub publications: Vec<Publication>,
}

impl SettleOutcome {
    pub fn is_noop(&self) -> bool {
        self.transition.is_noop()
    }
}

/// Applies `signal` to the locked call inside `tx`. `Ok(None)` when no
/// such call exists in `organization_id`. Does not commit.
pub async fn settle_in_tx(
    tx: &mut PgConnection,
    organization_id: Uuid,
    call_id: Uuid,
    signal: &Signal,
    now: DateTime<Utc>,
) -> Result<Option<SettleOutcome>, sqlx::Error> {
    let Some(mut call) = queries::lock_call(tx, organization_id, call_id).await? else {
        return Ok(None);
    };

    let transition = apply(call.status, signal);
    let Some(next_status) = transition.status() else {
        return Ok(Some(SettleOutcome {
            call,
            transition,
            publications: Vec::new(),
        }));
    };

    // --- UPDATE call ------------------------------------------------------
    match &transition {
        Transition::NoOp => unreachable!("status() is None for NoOp"),
        Transition::Ringing => {
            call.ringing_at = Some(now);
        }
        Transition::Answered { call_ref } => {
            call.answered_at = Some(now);
            call.provider_call_ref = call_ref.clone();
        }
        Transition::Failed { reason, .. } => {
            call.failure_reason = Some(*reason);
            call.ended_at = Some(now);
        }
        Transition::Ended { reason } => {
            call.end_reason = Some(*reason);
            call.ended_at = Some(now);
        }
    }
    call.status = next_status;

    let status = call.status.as_str();
    let failure_reason = call.failure_reason.map(|r| r.as_str());
    let end_reason = call.end_reason.map(|r| r.as_str());
    sqlx::query!(
        r#"UPDATE call
           SET status = $3, failure_reason = $4, end_reason = $5, provider_call_ref = $6,
               ringing_at = $7, answered_at = $8, ended_at = $9, updated_at = $10
           WHERE organization_id = $1 AND id = $2"#,
        organization_id,
        call.id,
        status,
        failure_reason,
        end_reason,
        call.provider_call_ref,
        call.ringing_at,
        call.answered_at,
        call.ended_at,
        now,
    )
    .execute(&mut *tx)
    .await?;

    // --- facts ------------------------------------------------------------
    let origin = Origin::decode(&call.origin).ok_or_else(|| {
        sqlx::Error::Decode(format!("call.origin: unknown value {:?}", call.origin).into())
    })?;
    let envelope = FactEnvelope {
        organization_id: call.organization_id,
        actor_kind: ActorKind::User,
        actor_user_id: Some(call.caller_user_id),
        on_behalf_of_user_id: None,
        origin,
        occurred_at: now,
        correlation_id: call.correlation_id,
        causation_id: Some(call.id),
    };

    let mut publications = vec![Publication::for_event(RealtimeEvent::call_changed(
        call.organization_id,
        now,
        call.correlation_id,
        call.id,
        call.person_id,
    ))];

    if let Some(outcome) = transition.attempt() {
        facts::insert_contact_attempted(
            tx,
            &envelope,
            ContactAttemptedFact {
                person_id: call.person_id,
                channel: "call",
                outcome: outcome.as_str(),
                corrects_id: None,
                recorded_at: None,
            },
        )
        .await?;
        publications.push(Publication::for_event(RealtimeEvent::person_changed(
            call.organization_id,
            now,
            call.correlation_id,
            call.person_id,
            PersonChange::ContactAttempted,
        )));
    }

    if transition.is_terminal() {
        let outcome = match call.status {
            CallStatus::Ended => "reached",
            CallStatus::Failed => call
                .failure_reason
                .expect("a failed call has a failure_reason")
                .as_str(),
            CallStatus::Placing | CallStatus::Ringing | CallStatus::Answered => {
                unreachable!("terminal transition to a non-terminal status")
            }
        };
        facts::insert_call_completed(
            tx,
            &envelope,
            CallCompletedFact {
                call_id: call.id,
                person_id: call.person_id,
                contact_method_id: call.contact_method_id,
                outcome,
                answered_at: call.answered_at,
                ended_at: now,
                talk_seconds: call.talk_seconds(),
            },
        )
        .await?;
    }

    Ok(Some(SettleOutcome {
        call,
        transition,
        publications,
    }))
}

/// One transaction: lock, apply, write, commit; then publish. `Ok(None)`
/// when the call does not exist in `organization_id`. The span records
/// the signal and the resulting transition (ids and tags only).
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %organization_id,
        call_id = %call_id,
        signal = signal.kind(),
        transition = tracing::field::Empty,
    )
)]
pub async fn settle(
    pool: &PgPool,
    publisher: &Publisher,
    organization_id: Uuid,
    call_id: Uuid,
    signal: &Signal,
    now: DateTime<Utc>,
) -> Result<Option<SettleOutcome>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let outcome = settle_in_tx(&mut tx, organization_id, call_id, signal, now).await?;
    let Some(outcome) = outcome else {
        tx.rollback().await?;
        tracing::Span::current().record("transition", "unknown_call");
        return Ok(None);
    };
    tx.commit().await?;

    tracing::Span::current().record("transition", transition_tag(&outcome.transition));
    for publication in &outcome.publications {
        publisher.publish_after_commit(publication.clone()).await;
    }
    Ok(Some(outcome))
}

pub fn transition_tag(transition: &Transition) -> &'static str {
    match transition {
        Transition::NoOp => "noop",
        Transition::Ringing => "ringing",
        Transition::Answered { .. } => "answered",
        Transition::Failed { reason, .. } => reason.as_str(),
        Transition::Ended { reason } => reason.as_str(),
    }
}

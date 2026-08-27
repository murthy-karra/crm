//! `correct_call_outcome` (docs/specs/SLICE_006c.md §3, §5a; D-032,
//! D-033): the caller's statement of how a call went, written as a
//! **new** `contact_attempted` row whose `corrects_id` points at the
//! call's head attempt. The original is never touched (D-015). Step
//! order is frozen: call lock first, then the head lookup, so concurrent
//! saves serialize on the call row and the later one chains onto the
//! earlier (or no-ops). The agent's choice is always written when the
//! head is the automatic root (`corrects_id IS NULL`), even if it equals
//! the system's observation: the automatic row is evidence, not an
//! outcome (D-033). `changed: false` only when the head is already an
//! agent choice with the same outcome.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::commands::{CallError, ContactChannel, ContactOutcome};
use crate::domain::envelope::{Actor, CommandContext, FactEnvelope};
use crate::domain::facts::{self, ContactAttemptedFact};
use crate::domain::telephony::queries as call_queries;
use crate::ids::{CallId, OrganizationId, PersonId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

/// The five values `POST /api/calls/{id}/outcome` accepts
/// (docs/specs/SLICE_006c.md §2, §5). `sent` is not a call outcome and is
/// a serde rejection (400), never a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallOutcomeCorrection {
    Reached,
    LeftMessage,
    NoAnswer,
    Busy,
    WrongNumber,
}

impl CallOutcomeCorrection {
    pub fn as_outcome(self) -> ContactOutcome {
        match self {
            CallOutcomeCorrection::Reached => ContactOutcome::Reached,
            CallOutcomeCorrection::LeftMessage => ContactOutcome::LeftMessage,
            CallOutcomeCorrection::NoAnswer => ContactOutcome::NoAnswer,
            CallOutcomeCorrection::Busy => ContactOutcome::Busy,
            CallOutcomeCorrection::WrongNumber => ContactOutcome::WrongNumber,
        }
    }
}

pub struct CorrectCallOutcome {
    pub call_id: CallId,
    pub outcome: CallOutcomeCorrection,
}

/// The `attempt` of the 200 body (docs/specs/SLICE_006c.md §5): the head
/// attempt after the command — the new correction row, or the unchanged
/// head when `changed: false`. A new type; `ContactAttemptRef` is
/// unchanged.
#[derive(Debug, Clone, Serialize)]
pub struct CorrectedAttemptRef {
    pub id: Uuid,
    pub channel: ContactChannel,
    pub outcome: ContactOutcome,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub corrects_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorrectionResult {
    pub attempt: CorrectedAttemptRef,
    pub changed: bool,
}

struct HeadRow {
    id: Uuid,
    channel: String,
    outcome: String,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    corrects_id: Option<Uuid>,
}

impl TryFrom<HeadRow> for CorrectedAttemptRef {
    type Error = CallError;

    /// A CHECK-allowed value the application does not know is `Corrupt`
    /// (fail closed, never panic), as `CallRow::try_from`.
    fn try_from(row: HeadRow) -> Result<Self, CallError> {
        let channel = ContactChannel::decode(&row.channel).ok_or(CallError::Corrupt)?;
        let outcome = ContactOutcome::decode(&row.outcome).ok_or(CallError::Corrupt)?;
        Ok(CorrectedAttemptRef {
            id: row.id,
            channel,
            outcome,
            occurred_at: row.occurred_at,
            recorded_at: row.recorded_at,
            corrects_id: row.corrects_id,
        })
    }
}

/// The call's **head attempt** (docs/specs/SLICE_006c.md §2, §3): the
/// `contact_attempted` row with `causation_id = call.id` that has no
/// corrector. Organization- and Person-scoped; `ORDER BY recorded_at DESC`
/// is belt-and-braces — the partial unique index keeps chains linear, so
/// at most one row qualifies.
async fn head_attempt(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    call_id: CallId,
    person_id: PersonId,
) -> Result<Option<HeadRow>, sqlx::Error> {
    sqlx::query_as!(
        HeadRow,
        r#"SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at, ca.recorded_at, ca.corrects_id
           FROM contact_attempted ca
           WHERE ca.organization_id = $1 AND ca.causation_id = $2 AND ca.person_id = $3
             AND NOT EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id = ca.id)
           ORDER BY ca.recorded_at DESC
           LIMIT 1"#,
        organization_id.0,
        // `causation_id` is `Option<Uuid>` on the fact side (the cross-
        // fact-table union) but the column here just names the call —
        // unwrap explicitly (hardening chunk N4).
        call_id.as_uuid(),
        person_id.0,
    )
    .fetch_optional(conn)
    .await
}

/// `23505` on `contact_attempted_corrects_once` (docs/specs/SLICE_006c.md
/// §3): defensive only — this command holds the call lock.
fn is_corrects_once_conflict(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("23505")
                && db.constraint() == Some("contact_attempted_corrects_once")
        }
        _ => false,
    }
}

/// `correct_call_outcome` (docs/specs/SLICE_006c.md §3, §9). The span's
/// `correlation_id` is the **call's** (as on the fact and the event);
/// `contact_outcome` is the chosen value and `outcome` the result tag
/// (`corrected` / `unchanged` / error kind). Ids and tags only.
#[tracing::instrument(
    name = "call.outcome",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = tracing::field::Empty,
        call_id = %cmd.call_id,
        person_id = tracing::field::Empty,
        contact_outcome = cmd.outcome.as_outcome().as_str(),
        outcome = tracing::field::Empty,
        changed = tracing::field::Empty,
    )
)]
pub async fn correct_call_outcome(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CorrectCallOutcome,
) -> Result<CorrectionResult, CallError> {
    let result = correct_call_outcome_attempt(pool, publisher, ctx, cmd).await;
    let span = tracing::Span::current();
    match &result {
        Ok(r) => {
            span.record("outcome", if r.changed { "corrected" } else { "unchanged" });
            span.record("changed", r.changed);
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "correct_call_outcome failed");
            span.record("outcome", err.kind());
        }
    }
    result
}

async fn correct_call_outcome_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CorrectCallOutcome,
) -> Result<CorrectionResult, CallError> {
    let requested = cmd.outcome.as_outcome();
    let mut tx = pool.begin().await?;

    // 1. Lock the call (foreign/nonexistent → byte-identical 404).
    let call = call_queries::lock_call(&mut tx, ctx.organization_id, cmd.call_id)
        .await?
        .ok_or(CallError::CallNotFound)?;
    let span = tracing::Span::current();
    span.record(
        "correlation_id",
        tracing::field::display(call.correlation_id),
    );
    span.record("person_id", tracing::field::display(call.person_id));
    // 2. Caller only (the span already carries the call's ids).
    if call.caller_user_id != ctx.actor_user_id {
        return Err(CallError::Forbidden);
    }
    // 3. Terminal only.
    if !call.status.is_terminal() {
        return Err(CallError::InvalidCallState);
    }
    // 4. Head attempt.
    let head = head_attempt(&mut tx, ctx.organization_id, call.id, call.person_id)
        .await?
        .ok_or(CallError::NoContactAttempt)?;
    let head = CorrectedAttemptRef::try_from(head)?;
    // 5. Unchanged only when the head is already an agent choice with the
    //    same outcome. When the head is the automatic root the agent's
    //    choice is always written, even if it equals the observation —
    //    the system's row is evidence, not an outcome (D-033), and the
    //    "outcome needed" Today tier clears only on an agent row.
    if head.corrects_id.is_some() && head.outcome == requested {
        tx.rollback().await?;
        return Ok(CorrectionResult {
            attempt: head,
            changed: false,
        });
    }

    // 6. The correction row (SLICE_006c §2): the head's `occurred_at` and
    //    channel, the call's correlation/causation, the correcting user as
    //    actor, the command's origin, and a `recorded_at` taken *after*
    //    the lock so it is strictly later than the head's.
    let recorded_at: DateTime<Utc> = sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(&mut *tx)
        .await?;
    let envelope = FactEnvelope {
        organization_id: ctx.organization_id,
        actor: Actor::User(ctx.actor_user_id),
        on_behalf_of_user_id: None,
        origin: ctx.origin,
        occurred_at: head.occurred_at,
        correlation_id: call.correlation_id,
        // `causation_id` stays `Option<Uuid>` (envelope.rs's cross-fact-
        // table union) while `call.id` is `CallId` — unwrap explicitly at
        // this one crossing (hardening chunk N4, mirroring settle.rs).
        causation_id: Some(call.id.as_uuid()),
    };
    let fact_id = facts::insert_contact_attempted(
        &mut tx,
        &envelope,
        ContactAttemptedFact {
            person_id: call.person_id,
            channel: head.channel,
            outcome: requested,
            corrects_id: Some(head.id),
            recorded_at: Some(recorded_at),
        },
    )
    .await
    .map_err(|err| {
        if is_corrects_once_conflict(&err) {
            CallError::CorrectionConflict
        } else {
            CallError::from(err)
        }
    })?;

    // 7. Commit, then publish with the call's correlation id.
    tx.commit().await?;
    publisher
        .publish_after_commit(Publication::for_event(RealtimeEvent::person_changed(
            ctx.organization_id,
            recorded_at,
            call.correlation_id,
            call.person_id,
            PersonChange::ContactAttempted,
        )))
        .await;

    Ok(CorrectionResult {
        attempt: CorrectedAttemptRef {
            id: fact_id,
            channel: head.channel,
            outcome: requested,
            occurred_at: head.occurred_at,
            recorded_at,
            corrects_id: Some(head.id),
        },
        changed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::CallOutcomeCorrection;

    #[test]
    fn accepts_exactly_the_five_call_outcomes() {
        for (json, expected) in [
            ("\"reached\"", CallOutcomeCorrection::Reached),
            ("\"left_message\"", CallOutcomeCorrection::LeftMessage),
            ("\"no_answer\"", CallOutcomeCorrection::NoAnswer),
            ("\"busy\"", CallOutcomeCorrection::Busy),
            ("\"wrong_number\"", CallOutcomeCorrection::WrongNumber),
        ] {
            let parsed: CallOutcomeCorrection = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
            assert_eq!(
                serde_json::to_string(&parsed.as_outcome()).unwrap(),
                json,
                "the stored outcome uses the same wire value"
            );
        }
    }

    #[test]
    fn rejects_sent_and_unknown_values() {
        for json in [
            "\"sent\"",
            "\"voicemail\"",
            "\"Reached\"",
            "\"\"",
            "1",
            "null",
        ] {
            assert!(
                serde_json::from_str::<CallOutcomeCorrection>(json).is_err(),
                "{json}"
            );
        }
    }
}

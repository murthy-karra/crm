//! Typed inserts for the five fact tables: the four D-015 §8 tables
//! (docs/specs/SLICE_002.md §2, §4) plus `contact_attempted`, the fifth
//! (docs/specs/SLICE_003.md §2, D-022). Each returns the inserted fact's
//! id — needed so `assignment_changed.causation_id` can be set to the
//! `routing_decision.id` on intake.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::envelope::FactEnvelope;

pub struct InquiryReceivedFact<'a> {
    pub inquiry_id: Uuid,
    pub person_id: Uuid,
    pub raw_payload_id: Uuid,
    pub content_hmac: &'a [u8],
    pub source: &'a str,
    pub person_created: bool,
    pub matched_by: Option<&'a str>,
}

pub async fn insert_inquiry_received(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InquiryReceivedFact<'_>,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO inquiry_received
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             inquiry_id, person_id, raw_payload_id, content_hmac, source, person_created, matched_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.inquiry_id,
        fact.person_id,
        fact.raw_payload_id,
        fact.content_hmac,
        fact.source,
        fact.person_created,
        fact.matched_by,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct RoutingDecisionFact<'a> {
    pub inquiry_id: Uuid,
    pub person_id: Uuid,
    pub strategy: &'a str,
    pub assignee_user_id: Option<Uuid>,
}

pub async fn insert_routing_decision(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: RoutingDecisionFact<'_>,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO routing_decision
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             inquiry_id, person_id, strategy, assignee_user_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.inquiry_id,
        fact.person_id,
        fact.strategy,
        fact.assignee_user_id,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct AssignmentChangedFact {
    pub person_id: Uuid,
    pub from_user_id: Option<Uuid>,
    pub to_user_id: Option<Uuid>,
    pub reason: &'static str,
}

pub async fn insert_assignment_changed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: AssignmentChangedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO assignment_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, from_user_id, to_user_id, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.person_id,
        fact.from_user_id,
        fact.to_user_id,
        fact.reason,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct ContactAttemptedFact<'a> {
    pub person_id: Uuid,
    pub channel: &'a str,
    pub outcome: &'a str,
}

/// The fifth typed fact table (docs/specs/SLICE_003.md §2, D-022): a
/// contact attempt is a real-world event with historical meaning, written
/// by `LogContactAttempt`.
pub async fn insert_contact_attempted(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: ContactAttemptedFact<'_>,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, channel, outcome)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.person_id,
        fact.channel,
        fact.outcome,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct StageChangedFact {
    pub person_id: Uuid,
    pub from_stage_id: Option<Uuid>,
    pub to_stage_id: Uuid,
    pub reason: &'static str,
}

// --- Slice 004 admin facts (docs/specs/SLICE_004.md §2) -------------------

pub async fn insert_organization_created(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO organization_created
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct InvitationIssuedFact {
    pub invitation_id: Uuid,
    pub role: &'static str,
    pub superseded_invitation_id: Option<Uuid>,
}

pub async fn insert_invitation_issued(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InvitationIssuedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO invitation_issued
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             invitation_id, role, superseded_invitation_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.invitation_id,
        fact.role,
        fact.superseded_invitation_id,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct InvitationResolvedFact {
    pub invitation_id: Uuid,
    pub outcome: &'static str,
}

pub async fn insert_invitation_resolved(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InvitationResolvedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO invitation_resolved
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             invitation_id, outcome)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.invitation_id,
        fact.outcome,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct MembershipChangedFact {
    pub user_id: Uuid,
    pub from_role: Option<&'static str>,
    pub to_role: &'static str,
    pub from_status: Option<&'static str>,
    pub to_status: &'static str,
    pub reason: &'static str,
}

pub async fn insert_membership_changed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: MembershipChangedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO membership_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             user_id, from_role, to_role, from_status, to_status, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.user_id,
        fact.from_role,
        fact.to_role,
        fact.from_status,
        fact.to_status,
        fact.reason,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub async fn insert_stage_changed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: StageChangedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO stage_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, from_stage_id, to_stage_id, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.person_id,
        fact.from_stage_id,
        fact.to_stage_id,
        fact.reason,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

// --- Slice 006 (docs/specs/SLICE_006.md §2) --------------------------------

pub struct CallCompletedFact<'a> {
    pub call_id: Uuid,
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
    /// `reached` for every answered call, else the `failure_reason`.
    pub outcome: &'a str,
    pub answered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: chrono::DateTime<chrono::Utc>,
    pub talk_seconds: Option<i32>,
}

/// The completed-call fact (AGENTS.md §4.6; docs/specs/SLICE_006.md §2):
/// written once by `settle` on every terminal transition, PII-free
/// (`contact_method_id` is a bare id). `envelope.occurred_at` is the
/// call's `ended_at`.
pub async fn insert_call_completed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: CallCompletedFact<'_>,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor_kind.as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO call_completed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             call_id, person_id, contact_method_id, outcome, answered_at, ended_at, talk_seconds)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
           RETURNING id"#,
        envelope.organization_id,
        actor_kind,
        envelope.actor_user_id,
        envelope.on_behalf_of_user_id,
        origin,
        envelope.occurred_at,
        envelope.correlation_id,
        envelope.causation_id,
        fact.call_id,
        fact.person_id,
        fact.contact_method_id,
        fact.outcome,
        fact.answered_at,
        fact.ended_at,
        fact.talk_seconds,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

//! Typed inserts for the five fact tables: the four D-015 §8 tables
//! (docs/specs/SLICE_002.md §2, §4) plus `contact_attempted`, the fifth
//! (docs/specs/SLICE_003.md §2, D-022). Each returns the inserted fact's
//! id — needed so `assignment_changed.causation_id` can be set to the
//! `routing_decision.id` on intake.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::admin::{MembershipStatus, Role};
use crate::domain::commands::{ContactChannel, ContactOutcome, RoutingStrategy};
use crate::domain::contact::ContactKind;
use crate::domain::envelope::FactEnvelope;
use crate::ids::{
    CallId, ContactMethodId, InquiryId, InvitationId, PersonId, RawPayloadId, StageId, UserId,
};

pub struct InquiryReceivedFact<'a> {
    pub inquiry_id: InquiryId,
    pub person_id: PersonId,
    pub raw_payload_id: RawPayloadId,
    pub content_hmac: &'a [u8],
    pub source: &'a str,
    pub person_created: bool,
    pub matched_by: Option<ContactKind>,
}

pub async fn insert_inquiry_received(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InquiryReceivedFact<'_>,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO inquiry_received
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             inquiry_id, person_id, raw_payload_id, content_hmac, source, person_created, matched_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.inquiry_id.0,
        fact.person_id.0,
        fact.raw_payload_id.0,
        fact.content_hmac,
        fact.source,
        fact.person_created,
        fact.matched_by.map(|kind| kind.as_str()),
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct RoutingDecisionFact {
    pub inquiry_id: InquiryId,
    pub person_id: PersonId,
    pub strategy: RoutingStrategy,
    pub assignee_user_id: Option<UserId>,
}

pub async fn insert_routing_decision(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: RoutingDecisionFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO routing_decision
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             inquiry_id, person_id, strategy, assignee_user_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.inquiry_id.0,
        fact.person_id.0,
        fact.strategy.as_str(),
        fact.assignee_user_id.map(|id| id.0),
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

/// Why a Person's assignment changed (hardening chunk S2 micro-enum): the
/// two values `AssignmentChangedFact.reason` is ever constructed with —
/// `receive_inquiry.rs`'s intake routing outcome, or an explicit
/// `AssignPerson` (docs/specs/SLICE_002.md §4). No DB `CHECK` constrains
/// this column (`assignment_changed.reason TEXT NOT NULL`, unconstrained),
/// and nothing reads it back into Rust (`person::queries`'s history read
/// keeps the raw column string for its JSON `detail` blob), so this type
/// covers only the construction side — `as_str()`, no decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentReason {
    Intake,
    Manual,
}

impl AssignmentReason {
    pub fn as_str(self) -> &'static str {
        match self {
            AssignmentReason::Intake => "intake",
            AssignmentReason::Manual => "manual",
        }
    }
}

pub struct AssignmentChangedFact {
    pub person_id: PersonId,
    pub from_user_id: Option<UserId>,
    pub to_user_id: Option<UserId>,
    pub reason: AssignmentReason,
}

pub async fn insert_assignment_changed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: AssignmentChangedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO assignment_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, from_user_id, to_user_id, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.person_id.0,
        fact.from_user_id.map(|id| id.0),
        fact.to_user_id.map(|id| id.0),
        fact.reason.as_str(),
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct ContactAttemptedFact {
    pub person_id: PersonId,
    pub channel: ContactChannel,
    pub outcome: ContactOutcome,
    /// The row this one supersedes (docs/specs/SLICE_006c.md §2): set only
    /// by `correct_call_outcome`; `None` for every original attempt.
    pub corrects_id: Option<Uuid>,
    /// `None` → the column default (`now()`, transaction start).
    /// `correct_call_outcome` passes `clock_timestamp()` taken after the
    /// call lock so a correction's `recorded_at` is strictly later than
    /// its head's (SLICE_006c §2).
    pub recorded_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The fifth typed fact table (docs/specs/SLICE_003.md §2, D-022): a
/// contact attempt is a real-world event with historical meaning, written
/// by `LogContactAttempt`, by `settle` (D-031), and — as a correction row
/// with `corrects_id` — by `correct_call_outcome` (SLICE_006c §2).
pub async fn insert_contact_attempted(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: ContactAttemptedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, channel, outcome, corrects_id, recorded_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,COALESCE($13, now()))
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.person_id.0,
        fact.channel.as_str(),
        fact.outcome.as_str(),
        fact.corrects_id,
        fact.recorded_at,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

/// Why a Person's stage changed (hardening chunk S2 micro-enum): mirrors
/// [`AssignmentReason`] exactly — same two values, same "no DB `CHECK`,
/// nothing reads it back" rationale — but kept as a separate type rather
/// than shared: `stage_changed` and `assignment_changed` are independent
/// columns on independent tables, and this codebase's established
/// precedent (`CallOutcomeCorrection` vs `ContactOutcome`,
/// `commands/correct_call_outcome.rs`) is to keep structurally-identical
/// but conceptually-distinct vocabularies as separate types rather than
/// share one, even when today's value sets coincide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageChangeReason {
    Intake,
    Manual,
}

impl StageChangeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StageChangeReason::Intake => "intake",
            StageChangeReason::Manual => "manual",
        }
    }
}

pub struct StageChangedFact {
    pub person_id: PersonId,
    pub from_stage_id: Option<StageId>,
    pub to_stage_id: StageId,
    pub reason: StageChangeReason,
}

// --- Slice 004 admin facts (docs/specs/SLICE_004.md §2) -------------------

pub async fn insert_organization_created(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO organization_created
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct InvitationIssuedFact {
    pub invitation_id: InvitationId,
    pub role: Role,
    pub superseded_invitation_id: Option<InvitationId>,
}

pub async fn insert_invitation_issued(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InvitationIssuedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO invitation_issued
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             invitation_id, role, superseded_invitation_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.invitation_id.0,
        fact.role.as_str(),
        fact.superseded_invitation_id.map(|id| id.0),
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

pub struct InvitationResolvedFact {
    pub invitation_id: InvitationId,
    pub outcome: &'static str,
}

pub async fn insert_invitation_resolved(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: InvitationResolvedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO invitation_resolved
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             invitation_id, outcome)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.invitation_id.0,
        fact.outcome,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

/// Why a membership changed (hardening chunk S2 micro-enum): the DB
/// `CHECK` on `membership_changed.reason` (migration 20260823000001) also
/// allows `'bootstrap'`, but no application code ever constructs it — the
/// genesis/bootstrap path this is presumably reserved for does not exist
/// yet (grepped: no caller anywhere in the workspace passes `"bootstrap"`
/// to this fact). Per this chunk's discipline (read the actual values
/// USED, not the full CHECK superset — mirrors S1's fail-closed `parse`
/// covering exactly the constructible set), this enum has five variants,
/// not six; add `Bootstrap` when a real caller needs it, the same way
/// `RoutingStrategy` grew `OrganizationDefault`/`Unassigned` when
/// SLICE_007c added their call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipChangeReason {
    /// `AcceptInvitation` (docs/specs/SLICE_004.md §4).
    Invitation,
    /// `ChangeMemberRole` to `Admin`.
    Promote,
    /// `ChangeMemberRole` to `Member`.
    Demote,
    /// `SetMemberStatus` to `Inactive`.
    Deactivate,
    /// `SetMemberStatus` to `Active`.
    Reactivate,
}

impl MembershipChangeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            MembershipChangeReason::Invitation => "invitation",
            MembershipChangeReason::Promote => "promote",
            MembershipChangeReason::Demote => "demote",
            MembershipChangeReason::Deactivate => "deactivate",
            MembershipChangeReason::Reactivate => "reactivate",
        }
    }
}

pub struct MembershipChangedFact {
    pub user_id: UserId,
    pub from_role: Option<Role>,
    pub to_role: Role,
    pub from_status: Option<MembershipStatus>,
    pub to_status: MembershipStatus,
    pub reason: MembershipChangeReason,
}

pub async fn insert_membership_changed(
    tx: &mut PgConnection,
    envelope: &FactEnvelope,
    fact: MembershipChangedFact,
) -> Result<Uuid, sqlx::Error> {
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO membership_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             user_id, from_role, to_role, from_status, to_status, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.user_id.0,
        fact.from_role.map(|r| r.as_str()),
        fact.to_role.as_str(),
        fact.from_status.map(|s| s.as_str()),
        fact.to_status.as_str(),
        fact.reason.as_str(),
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
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO stage_changed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             person_id, from_stage_id, to_stage_id, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.person_id.0,
        fact.from_stage_id.map(|id| id.0),
        fact.to_stage_id.0,
        fact.reason.as_str(),
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

// --- Slice 006 (docs/specs/SLICE_006.md §2) --------------------------------

pub struct CallCompletedFact<'a> {
    pub call_id: CallId,
    pub person_id: PersonId,
    pub contact_method_id: ContactMethodId,
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
    let actor_kind = envelope.actor.kind().as_str();
    let origin = envelope.origin.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO call_completed
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, causation_id,
             call_id, person_id, contact_method_id, outcome, answered_at, ended_at, talk_seconds)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
           RETURNING id"#,
        envelope.organization_id.0,
        actor_kind,
        envelope.actor.user_id().map(|id| id.0),
        envelope.on_behalf_of_user_id.map(|id| id.0),
        origin,
        envelope.occurred_at,
        envelope.correlation_id.0,
        envelope.causation_id,
        fact.call_id.0,
        fact.person_id.0,
        fact.contact_method_id.0,
        fact.outcome,
        fact.answered_at,
        fact.ended_at,
        fact.talk_seconds,
    )
    .fetch_one(tx)
    .await?;
    Ok(row.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These three micro-enums (hardening chunk S2) are construction-only
    /// — nothing decodes `assignment_changed`/`stage_changed`/
    /// `membership_changed`'s `reason` column back into Rust (the history
    /// read in `person::queries` keeps the raw column string for its JSON
    /// `detail` blob) — so there is no round trip to pin, only that
    /// `as_str()` still yields exactly the strings the ledger/history rows
    /// have always stored.
    #[test]
    fn assignment_reason_as_str_matches_the_column_vocabulary() {
        assert_eq!(AssignmentReason::Intake.as_str(), "intake");
        assert_eq!(AssignmentReason::Manual.as_str(), "manual");
    }

    #[test]
    fn stage_change_reason_as_str_matches_the_column_vocabulary() {
        assert_eq!(StageChangeReason::Intake.as_str(), "intake");
        assert_eq!(StageChangeReason::Manual.as_str(), "manual");
    }

    #[test]
    fn membership_change_reason_as_str_matches_the_column_vocabulary() {
        assert_eq!(MembershipChangeReason::Invitation.as_str(), "invitation");
        assert_eq!(MembershipChangeReason::Promote.as_str(), "promote");
        assert_eq!(MembershipChangeReason::Demote.as_str(), "demote");
        assert_eq!(MembershipChangeReason::Deactivate.as_str(), "deactivate");
        assert_eq!(MembershipChangeReason::Reactivate.as_str(), "reactivate");
    }
}

//! Email lead intake (docs/plans/SLICE_007_LADDER.md). Rung 007a: the
//! Organization intake address value type only — nothing receives mail.
//! Rung 007b: Phase-A-only inbound email endpoint (receive module).
//! Rung 007c: `IntakeActor` — the system-actor path through
//! `receive_inquiry` (docs/specs/SLICE_007c.md §4). Rung 007d: the
//! pinned-format email module and Phase B on the inbound endpoint
//! (docs/specs/SLICE_007d.md §4).

pub mod address;
pub mod email;
pub mod extraction;
pub mod receive;
pub mod rotate;
pub mod workbench;

pub use address::IntakeAddress;
pub use receive::{receive_inbound_email, InboundEmailOutcome, ReceiveInboundEmailError};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::envelope::{ActorKind, CommandContext, FactEnvelope, Origin};
use crate::ids::{OrganizationId, UserId};

/// The actor behind a `receive_inquiry` call (docs/specs/SLICE_007c.md
/// §4): either an authenticated user's trusted session context, or no
/// human actor at all — unattended intake routed by the Organization's
/// configured default. `organization_id`/`correlation_id` come from the
/// server either way, never from client input (AGENTS.md §4.2); a
/// `System` actor's `organization_id` is resolved by its trusted caller
/// (tests/CLI resolve it server-side; 007d will derive it from the
/// recipient token).
pub enum IntakeActor {
    User(CommandContext),
    System {
        organization_id: OrganizationId,
        origin: Origin,
        correlation_id: Uuid,
        /// The human whose action caused this unattended execution
        /// (docs/specs/SLICE_007e.md §4: the retrying admin). Delivery
        /// paths pass `None`.
        on_behalf_of_user_id: Option<UserId>,
    },
}

impl IntakeActor {
    pub fn organization_id(&self) -> OrganizationId {
        match self {
            IntakeActor::User(ctx) => ctx.organization_id,
            IntakeActor::System {
                organization_id, ..
            } => *organization_id,
        }
    }

    pub fn origin(&self) -> Origin {
        match self {
            IntakeActor::User(ctx) => ctx.origin,
            IntakeActor::System { origin, .. } => *origin,
        }
    }

    pub fn correlation_id(&self) -> Uuid {
        match self {
            IntakeActor::User(ctx) => ctx.correlation_id,
            IntakeActor::System { correlation_id, .. } => *correlation_id,
        }
    }

    pub fn actor_kind(&self) -> ActorKind {
        match self {
            IntakeActor::User(_) => ActorKind::User,
            IntakeActor::System { .. } => ActorKind::System,
        }
    }

    /// The authenticated user id behind a `User` actor — `None` for
    /// `System` (the point of the variant). Used by the routing matrix's
    /// `actor_default` branch (docs/specs/SLICE_007c.md §4).
    pub fn user_actor_id(&self) -> Option<UserId> {
        match self {
            IntakeActor::User(ctx) => Some(ctx.actor_user_id),
            IntakeActor::System { .. } => None,
        }
    }

    pub fn envelope(&self, occurred_at: DateTime<Utc>) -> FactEnvelope {
        match self {
            IntakeActor::User(ctx) => FactEnvelope::for_command(ctx, occurred_at),
            IntakeActor::System {
                organization_id,
                origin,
                correlation_id,
                on_behalf_of_user_id,
            } => FactEnvelope::for_system(
                *organization_id,
                *origin,
                occurred_at,
                *correlation_id,
                *on_behalf_of_user_id,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_actor_accessors_and_envelope() {
        let organization_id = OrganizationId::new(Uuid::new_v4());
        let correlation_id = Uuid::new_v4();
        let occurred_at = Utc::now();
        let actor = IntakeActor::System {
            organization_id,
            origin: Origin::Cli,
            correlation_id,
            on_behalf_of_user_id: None,
        };

        assert_eq!(actor.organization_id(), organization_id);
        assert_eq!(actor.origin(), Origin::Cli);
        assert_eq!(actor.correlation_id(), correlation_id);
        assert_eq!(actor.actor_kind(), ActorKind::System);
        assert_eq!(actor.user_actor_id(), None);

        let envelope = actor.envelope(occurred_at);
        assert_eq!(envelope.organization_id, organization_id);
        assert_eq!(envelope.actor.kind(), ActorKind::System);
        assert_eq!(envelope.actor.user_id(), None);
        assert_eq!(envelope.on_behalf_of_user_id, None);
        assert_eq!(envelope.origin, Origin::Cli);
        assert_eq!(envelope.occurred_at, occurred_at);
        assert_eq!(envelope.correlation_id, correlation_id);
        assert_eq!(envelope.causation_id, None);
    }

    #[test]
    fn system_actor_on_behalf_of_reaches_the_envelope() {
        // SLICE_007e §4: the workbench retry records the acting admin in
        // on_behalf_of_user_id while staying a System actor.
        let admin = UserId::new(Uuid::new_v4());
        let actor = IntakeActor::System {
            organization_id: OrganizationId::new(Uuid::new_v4()),
            origin: Origin::WebSession,
            correlation_id: Uuid::new_v4(),
            on_behalf_of_user_id: Some(admin),
        };
        let envelope = actor.envelope(Utc::now());
        assert_eq!(envelope.actor.kind(), ActorKind::System);
        assert_eq!(envelope.actor.user_id(), None);
        assert_eq!(envelope.on_behalf_of_user_id, Some(admin));
        assert_eq!(envelope.origin, Origin::WebSession);
    }

    #[test]
    fn user_actor_accessors_and_envelope() {
        let ctx = CommandContext {
            organization_id: OrganizationId::new(Uuid::new_v4()),
            actor_user_id: UserId::new(Uuid::new_v4()),
            origin: Origin::WebSession,
            correlation_id: Uuid::new_v4(),
        };
        let actor = IntakeActor::User(ctx.clone());

        assert_eq!(actor.organization_id(), ctx.organization_id);
        assert_eq!(actor.origin(), Origin::WebSession);
        assert_eq!(actor.correlation_id(), ctx.correlation_id);
        assert_eq!(actor.actor_kind(), ActorKind::User);
        assert_eq!(actor.user_actor_id(), Some(ctx.actor_user_id));

        let occurred_at = Utc::now();
        let envelope = actor.envelope(occurred_at);
        assert_eq!(envelope.actor.kind(), ActorKind::User);
        assert_eq!(envelope.actor.user_id(), Some(ctx.actor_user_id));
    }
}

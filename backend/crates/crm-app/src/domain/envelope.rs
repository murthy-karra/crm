//! `CommandContext`, `FactEnvelope`, `Origin`, `ActorKind`, `Actor`
//! (docs/specs/SLICE_002.md §4).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::ids::{CorrelationId, OrganizationId, TurnId, UserId};

/// `actor_kind` on every fact row. This slice only ever writes `User` (a
/// future webhook adapter writes `System` — spec §5's "actor_kind =
/// 'system'").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    User,
    System,
}

impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::System => "system",
        }
    }
}

/// The actor behind a fact: an authenticated user, or no human actor at all
/// (docs/specs/SLICE_007c.md §4). Replaces the `actor_kind` +
/// `actor_user_id: Option<UserId>` pair `FactEnvelope` used to carry as
/// separate pub fields: `(System, Some(user))` and `(User, None)` were both
/// representable and only caught, at insert time, by the DB CHECK
/// `(actor_kind = 'user') = (actor_user_id IS NOT NULL)` (migration
/// `20260821000004`, kept as defense-in-depth). With `Actor` those
/// combinations have no representation at all (the `SenderTrust`/
/// `IntakeActor` "no capacity for it" house style). `kind()`/`user_id()`
/// project back onto the DB vocabulary (`ActorKind`) and the raw id, so
/// every insert site keeps binding exactly what it bound before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User(UserId),
    System,
}

impl Actor {
    pub fn kind(self) -> ActorKind {
        match self {
            Actor::User(_) => ActorKind::User,
            Actor::System => ActorKind::System,
        }
    }

    pub fn user_id(self) -> Option<UserId> {
        match self {
            Actor::User(id) => Some(id),
            Actor::System => None,
        }
    }
}

/// `origin` on every fact row and on `raw_payload`. This slice only ever
/// writes `WebSession`; the other variants are named here so future slices
/// extend, not redefine, the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    WebSession,
    Webhook,
    Operator,
    Migration,
    /// The platform-admin surface acting on an Organization it does not
    /// belong to (docs/specs/SLICE_004.md §4).
    Platform,
    /// The `crm-admin` CLI (docs/specs/SLICE_004.md §11).
    Cli,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::WebSession => "web_session",
            Origin::Webhook => "webhook",
            Origin::Operator => "operator",
            Origin::Migration => "migration",
            Origin::Platform => "platform",
            Origin::Cli => "cli",
        }
    }

    /// Decodes an `origin` read back from a row (e.g. `call.origin`, so a
    /// call-derived fact carries the call's own origin — `web_session` in
    /// Slice 006, `operator` in 006b). `None` for an unknown value: a read
    /// path fails closed, never panics.
    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "web_session" => Some(Origin::WebSession),
            "webhook" => Some(Origin::Webhook),
            "operator" => Some(Origin::Operator),
            "migration" => Some(Origin::Migration),
            "platform" => Some(Origin::Platform),
            "cli" => Some(Origin::Cli),
            _ => None,
        }
    }
}

/// Trusted context for a business-mutation command: Organization, actor,
/// and correlation id all come from the server, never from client input
/// (AGENTS.md §4.2). `correlation_id` is fresh per command execution and is
/// never the request id — the request span logs both (docs/specs/SLICE_002.md
/// §4). One declared amendment (docs/specs/SLICE_006b.md §3): an
/// Operator-confirmed command reuses the Operator **turn id** as its
/// correlation id, chaining turn → call → facts for the audit trail.
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub organization_id: OrganizationId,
    pub actor_user_id: UserId,
    pub origin: Origin,
    pub correlation_id: CorrelationId,
}

impl CommandContext {
    pub fn from_auth(auth: &AuthContext) -> Self {
        Self {
            organization_id: auth.active_organization_id,
            actor_user_id: auth.actor_user_id,
            origin: Origin::WebSession,
            correlation_id: CorrelationId::new(Uuid::new_v4()),
        }
    }

    /// An Operator-proposed command the user confirmed in the UI
    /// (docs/specs/SLICE_006b.md §3): same trusted session identity as
    /// `from_auth`, origin `Operator`, and the Operator turn id as the
    /// correlation id (the declared amendment above). `turn_id` and
    /// `correlation_id` are distinct id types (hardening chunk N4) — this
    /// is the one place they cross, and it is done visibly, never by an
    /// implicit `From`/`Into`.
    pub fn for_operator(auth: &AuthContext, turn_id: TurnId) -> Self {
        Self {
            organization_id: auth.active_organization_id,
            actor_user_id: auth.actor_user_id,
            origin: Origin::Operator,
            correlation_id: CorrelationId::new(turn_id.as_uuid()),
        }
    }
}

/// The envelope fields common to every fact-table insert
/// (docs/specs/SLICE_002.md §2). `occurred_at` is supplied per call site:
/// intake facts use the request's `received_at`; assign/stage commands use
/// `Utc::now()` (spec §4).
///
/// `causation_id` semantics per fact: `assignment_changed.causation_id` =
/// the `routing_decision.id` on intake (SLICE_002 §2);
/// `contact_attempted.causation_id` = the `call.id` when written by or
/// about a call — the automatic attempt (D-031, docs/specs/SLICE_006.md
/// §2) and any correction of it (`corrects_id` set, docs/specs/SLICE_006c.md
/// §2; the correction keeps `causation_id = call.id` so "all facts for this
/// call" stays one lookup) — and NULL when logged by hand;
/// `call_completed.causation_id` = the `call.id` likewise. Every call-derived fact keeps the caller as the
/// actor (`Actor::User(caller_user_id)`), the call's `origin`, and the
/// call's `correlation_id` — the provider merely reports.
#[derive(Debug, Clone)]
pub struct FactEnvelope {
    pub organization_id: OrganizationId,
    pub actor: Actor,
    pub on_behalf_of_user_id: Option<UserId>,
    pub origin: Origin,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: CorrelationId,
    /// The cross-fact-table union described above — deliberately NOT typed
    /// with an id newtype (hardening chunk N4 scope note): it names a row
    /// in whichever fact table is semantically the cause, not one fixed
    /// domain entity.
    pub causation_id: Option<Uuid>,
}

impl FactEnvelope {
    /// Envelope for a fact caused directly by an authenticated user's
    /// command (`Actor::User`; `on_behalf_of_user_id` always NULL this
    /// slice).
    pub fn for_command(ctx: &CommandContext, occurred_at: DateTime<Utc>) -> Self {
        Self {
            organization_id: ctx.organization_id,
            actor: Actor::User(ctx.actor_user_id),
            on_behalf_of_user_id: None,
            origin: ctx.origin,
            occurred_at,
            correlation_id: ctx.correlation_id,
            causation_id: None,
        }
    }

    /// Sets `causation_id` (e.g. intake's `assignment_changed.causation_id`
    /// = the `routing_decision.id`, docs/specs/SLICE_002.md §2).
    pub fn with_causation(mut self, causation_id: Uuid) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Envelope for a fact caused by unattended (no human actor) intake
    /// (docs/specs/SLICE_007c.md §4): `Actor::System` — no capacity to also
    /// carry a user id, unlike the old `actor_kind`/`actor_user_id` pair,
    /// which relied on the `(actor_kind = 'user') = (actor_user_id IS NOT
    /// NULL)` CHECK (migration `20260821000004`, kept as defense-in-depth)
    /// to reject `(System, Some(user))` at insert time. `origin` is a
    /// parameter: the CLI walkthrough passes `Origin::Cli`; 007d passes
    /// `Origin::Webhook`. `on_behalf_of_user_id` (D-015 §2's
    /// "on-whose-behalf" field, docs/specs/SLICE_007e.md §4): the human
    /// whose action caused this unattended execution — e.g. the admin who
    /// clicked Try again. Delivery paths pass `None`. Declared additive
    /// extension of the SLICE_007c §4 signature (SLICE_007e approval).
    pub fn for_system(
        organization_id: OrganizationId,
        origin: Origin,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        on_behalf_of_user_id: Option<UserId>,
    ) -> Self {
        Self {
            organization_id,
            actor: Actor::System,
            on_behalf_of_user_id,
            origin,
            occurred_at,
            correlation_id,
            causation_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Actor::kind()`/`user_id()` must project each variant back onto
    /// exactly the `ActorKind` + `Option<UserId>` pair the fact tables'
    /// `actor_kind`/`actor_user_id` columns store — the mapping every
    /// `facts.rs` insert fn and `rotate.rs` rely on.
    #[test]
    fn actor_accessors_round_trip_both_variants() {
        let user_id = UserId::new(Uuid::new_v4());

        let user = Actor::User(user_id);
        assert_eq!(user.kind(), ActorKind::User);
        assert_eq!(user.user_id(), Some(user_id));

        let system = Actor::System;
        assert_eq!(system.kind(), ActorKind::System);
        assert_eq!(system.user_id(), None);
    }
}

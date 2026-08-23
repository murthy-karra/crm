//! `CommandContext`, `FactEnvelope`, `Origin`, `ActorKind`
//! (docs/specs/SLICE_002.md §4).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::auth::AuthContext;

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
/// §4).
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub organization_id: Uuid,
    pub actor_user_id: Uuid,
    pub origin: Origin,
    pub correlation_id: Uuid,
}

impl CommandContext {
    pub fn from_auth(auth: &AuthContext) -> Self {
        Self {
            organization_id: auth.active_organization_id,
            actor_user_id: auth.actor_user_id,
            origin: Origin::WebSession,
            correlation_id: Uuid::new_v4(),
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
/// `contact_attempted.causation_id` = the `call.id` when the attempt was
/// written automatically by a call (D-031, docs/specs/SLICE_006.md §2) and
/// NULL when logged by hand; `call_completed.causation_id` = the
/// `call.id` likewise. Every call-derived fact keeps the caller as the
/// actor (`actor_kind = 'user'`, `actor_user_id = caller_user_id`), the
/// call's `origin`, and the call's `correlation_id` — the provider merely
/// reports.
#[derive(Debug, Clone)]
pub struct FactEnvelope {
    pub organization_id: Uuid,
    pub actor_kind: ActorKind,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_of_user_id: Option<Uuid>,
    pub origin: Origin,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
}

impl FactEnvelope {
    /// Envelope for a fact caused directly by an authenticated user's
    /// command (`actor_kind = 'user'`; `on_behalf_of_user_id` always NULL
    /// this slice).
    pub fn for_command(ctx: &CommandContext, occurred_at: DateTime<Utc>) -> Self {
        Self {
            organization_id: ctx.organization_id,
            actor_kind: ActorKind::User,
            actor_user_id: Some(ctx.actor_user_id),
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
}

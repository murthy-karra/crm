//! Slice 004 administration domain: platform admin, invitations, and
//! Organization membership roles/status (docs/specs/SLICE_004.md §4).
//! Layout mirrors `domain/commands` — one file per command, plus shared
//! read models in `queries`.

pub mod commands;
pub mod queries;
pub mod validation;

pub use commands::{
    AcceptInvitation, AcceptInvitationOutcome, AdminCommandError, ChangeMemberRole,
    CreateOrganization, GrantPlatformAdmin, IssueInvitation, IssueInvitationOutcome,
    RevokeInvitation, SetLocalPassword, SetMemberStatus,
};

use serde::Serialize;
use uuid::Uuid;

use crate::domain::envelope::Origin;

/// The actor and origin behind an admin command — deliberately lighter
/// than `domain::envelope::CommandContext` (docs/specs/SLICE_002.md §4):
/// several Slice 004 commands (`CreateOrganization` especially) don't yet
/// have an Organization id when the command starts, so each command builds
/// its own `CommandContext`/`FactEnvelope` once it knows one, generating a
/// fresh `correlation_id` itself, exactly as `CommandContext::from_auth`
/// does (docs/specs/SLICE_004.md §4).
#[derive(Debug, Clone, Copy)]
pub struct AdminActor {
    pub actor_user_id: Uuid,
    pub origin: Origin,
}

/// `organization_membership.role` (docs/specs/SLICE_004.md §2). "Admin" is
/// the only authorization fact this slice adds inside an Organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            _ => None,
        }
    }
}

/// `organization_membership.status` (D-027; docs/specs/SLICE_004.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Inactive,
}

impl MembershipStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MembershipStatus::Active => "active",
            MembershipStatus::Inactive => "inactive",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(MembershipStatus::Active),
            "inactive" => Some(MembershipStatus::Inactive),
            _ => None,
        }
    }
}

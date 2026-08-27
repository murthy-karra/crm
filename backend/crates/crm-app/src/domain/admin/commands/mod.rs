//! Slice 004 typed application commands (docs/specs/SLICE_004.md §4). Each
//! runs in one transaction, writes its fact(s) via `domain::facts`, and
//! (per §6) publishes no realtime event.

pub mod accept_invitation;
pub mod change_member_role;
pub mod create_organization;
pub mod grant_platform_admin;
pub mod issue_invitation;
pub mod revoke_invitation;
pub mod set_local_password;
pub mod set_member_status;
pub mod token;

pub use accept_invitation::{accept_invitation, AcceptInvitation, AcceptInvitationOutcome};
pub use change_member_role::{change_member_role, ChangeMemberRole};
pub use create_organization::{create_organization, CreateOrganization};
pub use grant_platform_admin::{grant_platform_admin, GrantPlatformAdmin};
pub use issue_invitation::{issue_invitation, IssueInvitation, IssueInvitationOutcome};
pub use revoke_invitation::{revoke_invitation, RevokeInvitation};
pub use set_local_password::{set_local_password, SetLocalPassword};
pub use set_member_status::{set_member_status, SetMemberStatus};

#[derive(Debug)]
pub enum AdminCommandError {
    NotFound,
    OrganizationNameTaken,
    InvalidEmail,
    AlreadyMember,
    InvitationUsed,
    InvitationExpired,
    InvitationNotAcceptable,
    WeakPassword,
    /// Display name outside 1–120 chars after trim, or any other
    /// request-shape validation failure that maps to `400
    /// malformed_request` (docs/specs/SLICE_004.md §5).
    MalformedRequest,
    LastAdmin,
    Crypto,
    /// Slice 007a: nine intake-slug candidates all taken (create_organization).
    IntakeSlugExhausted,
    /// Data read back from our own database didn't match an expected
    /// shape — fail closed rather than panic, matching
    /// `domain::commands::CommandError::Corrupt`.
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for AdminCommandError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            // A row-boundary decode failure (decode_role/decode_status
            // manufacture these) is corrupt data, not an unavailable
            // database — 500, never a retryable-looking 503 (hardening
            // chunk S2, completing the Decode -> Corrupt mapping across
            // all three error types; CallError precedent).
            sqlx::Error::Decode(_) => AdminCommandError::Corrupt,
            other => AdminCommandError::Database(other),
        }
    }
}

impl AdminCommandError {
    /// A stable, PII-free tag for logging (docs/specs/SLICE_002.md §8
    /// convention, applied here): never the variant's `Display`/`Debug`
    /// text.
    pub fn kind(&self) -> &'static str {
        match self {
            AdminCommandError::NotFound => "not_found",
            AdminCommandError::OrganizationNameTaken => "organization_name_taken",
            AdminCommandError::InvalidEmail => "invalid_email",
            AdminCommandError::AlreadyMember => "already_member",
            AdminCommandError::InvitationUsed => "invitation_used",
            AdminCommandError::InvitationExpired => "invitation_expired",
            AdminCommandError::InvitationNotAcceptable => "invitation_not_acceptable",
            AdminCommandError::WeakPassword => "weak_password",
            AdminCommandError::MalformedRequest => "malformed_request",
            AdminCommandError::LastAdmin => "last_admin",
            AdminCommandError::Crypto => "crypto",
            AdminCommandError::IntakeSlugExhausted => "intake_slug_exhausted",
            AdminCommandError::Corrupt => "corrupt",
            AdminCommandError::Database(_) => "database",
        }
    }
}

impl std::fmt::Display for AdminCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind())
    }
}

impl std::error::Error for AdminCommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hardening chunk S2: a decode failure is corrupt data (500), never
    /// a retryable-looking database error (503) — same pin as
    /// CommandError's and WorkbenchError's.
    #[test]
    fn decode_error_maps_to_corrupt() {
        let err: AdminCommandError = sqlx::Error::Decode("bad enum value".into()).into();
        assert!(matches!(err, AdminCommandError::Corrupt));
        let err: AdminCommandError = sqlx::Error::RowNotFound.into();
        assert!(matches!(err, AdminCommandError::Database(_)));
    }
}

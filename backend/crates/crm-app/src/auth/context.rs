//! `AuthContext` itself is application-layer data (domain's
//! `CommandContext::from_auth` and person visibility consume it); the
//! Axum extractors that *build* it live in `auth::extractors`
//! (docs/specs/SLICE_006a.md §4).

use uuid::Uuid;

use crate::domain::admin::Role;
use crate::ids::OrganizationId;

/// The trusted actor and active Organization for this request, derived
/// entirely server-side from the session cookie. Handlers take this as a
/// parameter and never see the cookie; an Organization ID never enters a
/// query from client input (AGENTS.md §4.2).
///
/// Requires an active Organization (docs/specs/SLICE_004.md §3): a
/// platform-only session (no Organization) gets 401 `unauthenticated` here,
/// so every existing tenant route fails closed without modification.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub actor_user_id: Uuid,
    pub actor_email: String,
    pub actor_display_name: String,
    pub active_organization_id: OrganizationId,
    pub active_organization_name: String,
    pub role: Role,
}

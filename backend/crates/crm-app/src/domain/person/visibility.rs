//! D-005 Person visibility. Every Person-visibility read (people list,
//! person detail, that Person's inquiries and history) takes
//! `&PersonVisibilityScope`, not a raw `Uuid`, so an exhaustive `match`
//! forces any future variant through every such query site
//! (docs/specs/SLICE_002.md §4). Stage and unresolved-queue reads take
//! `organization_id` from `AuthContext` directly instead — they are
//! Organization data, not Person visibility, and must not become
//! team-scoped if a Team variant ever arrives.

use crate::auth::AuthContext;
use crate::ids::OrganizationId;

/// The only implemented variant is Organization-wide visibility
/// (AGENTS.md §4.4, D-005).
#[derive(Debug, Clone, Copy)]
pub enum PersonVisibilityScope {
    Organization(OrganizationId),
}

impl PersonVisibilityScope {
    pub fn from_auth(auth: &AuthContext) -> Self {
        PersonVisibilityScope::Organization(auth.active_organization_id)
    }

    pub fn organization_id(&self) -> OrganizationId {
        match self {
            PersonVisibilityScope::Organization(id) => *id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn organization_id_returns_the_wrapped_id() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let scope = PersonVisibilityScope::Organization(org_id);
        assert_eq!(scope.organization_id(), org_id);
    }
}

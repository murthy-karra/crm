//! Hardening chunk N1 (docs/design/type-safety-hardening.md, chunk 2; its
//! "sqlx strategy" section is binding): the Organization tenant key gets
//! its own type. Every crm-app and crm-api site that carries an
//! Organization id takes [`OrganizationId`] instead of a bare `Uuid`, so
//! swapping it with any other id (user, person, payload, call, …) is a
//! compile error rather than a tenant-isolation bug waiting for a runtime
//! test to catch it (AGENTS.md §4.3 — Organization is the tenant
//! boundary). `query!`/`query_as!` sites keep binding `.0`: the SQL text
//! and the tracked `.sqlx` cache stay byte-identical, and the wire shape
//! is unchanged because the type is `#[serde(transparent)]`. This is the
//! first id newtype in the ladder; later chunks (`UserId`, `PersonId`, …)
//! reuse this module and its pattern.

use std::fmt;

use uuid::Uuid;

/// The Organization tenant key. `#[repr(transparent)]` and
/// `#[serde(transparent)]` mean this is exactly a `Uuid` on the wire and in
/// memory — no JSON shape change, no extra allocation, no new `sqlx::Type`
/// (binds still go through `.0`, so this type never needs one).
///
/// The friction is deliberate: no `From<Uuid>`/`Into<Uuid>` conversion
/// exists beyond [`OrganizationId::new`]/[`OrganizationId::as_uuid`], both
/// visible at the call site instead of happening implicitly. A bare
/// `Uuid` — this Organization's own id included — no longer satisfies a
/// parameter typed `OrganizationId`, and neither does any other id this
/// ladder eventually types:
///
/// ```compile_fail,E0308
/// # use crm_app::ids::OrganizationId;
/// # use uuid::Uuid;
/// fn scoped_to(_organization_id: OrganizationId) {}
///
/// let person_id: Uuid = Uuid::new_v4(); // any other bare id, equally
/// scoped_to(person_id); // does not compile: `Uuid` is not `OrganizationId`
/// ```
///
/// which without this type is exactly the swap that compiled before this
/// chunk:
///
/// ```compile_fail,E0308
/// # use crm_app::ids::OrganizationId;
/// # use uuid::Uuid;
/// fn scoped_to(_organization_id: OrganizationId, _person_id: Uuid) {}
///
/// let organization_id = OrganizationId::new(Uuid::new_v4());
/// let person_id = Uuid::new_v4();
/// scoped_to(person_id, organization_id); // arguments transposed
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct OrganizationId(pub Uuid);

impl OrganizationId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%org`) or any `format!("{organization_id}")` call site left over from
/// the bare-`Uuid` era renders byte-identically.
impl fmt::Display for OrganizationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `OrganizationId(<uuid>)` tuple
/// form. An id is not PII (AGENTS.md §9 lists what must never be logged;
/// ids are not on that list), so there is nothing to redact — but a stray
/// `{:?}` in a log line or an `.expect()` message should still read as the
/// plain UUID a human can search for, not Rust's tuple-struct wrapper
/// noise.
impl fmt::Debug for OrganizationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(OrganizationId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let org = OrganizationId::new(Uuid::new_v4());
        assert_eq!(format!("{org:?}"), format!("{org}"));
        assert!(!format!("{org:?}").contains("OrganizationId"));
    }

    /// The wire-compat invariant: a UUID string serialized/deserialized
    /// through `OrganizationId` is byte-identical to the bare `Uuid` (hard
    /// invariant 2 — no wire changes).
    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let org = OrganizationId::new(id);

        let org_json = serde_json::to_string(&org).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(org_json, uuid_json);

        let round_tripped: OrganizationId = serde_json::from_str(&org_json).unwrap();
        assert_eq!(round_tripped, org);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(OrganizationId::new(id).as_uuid(), id);
    }
}

/// The User identity key (hardening chunk N2,
/// docs/design/type-safety-hardening.md chunk 3): the id most frequently
/// adjacent to [`OrganizationId`] — `actor_user_id`, `assigned_user_id`,
/// `on_behalf_of_user_id`, and the like — so it is the swap this chunk
/// closes. Same shape as `OrganizationId` for the same reasons:
/// `#[repr(transparent)]` and `#[serde(transparent)]` mean this is exactly a
/// `Uuid` on the wire and in memory, `query!`/`query!` binds still go
/// through `.0`, and no `From<Uuid>`/`Into<Uuid>` conversion exists beyond
/// [`UserId::new`]/[`UserId::as_uuid`], both visible at the call site.
///
/// Both halves of the org/user adjacency are typed as of this chunk, so the
/// transposition that used to compile now does not, in either argument
/// order:
///
/// ```compile_fail,E0308
/// # use crm_app::ids::{OrganizationId, UserId};
/// # use uuid::Uuid;
/// fn scoped_to(_organization_id: OrganizationId, _user_id: UserId) {}
///
/// let organization_id = OrganizationId::new(Uuid::new_v4());
/// let user_id = UserId::new(Uuid::new_v4());
/// scoped_to(user_id, organization_id); // arguments transposed
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct UserId(pub Uuid);

impl UserId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%actor_id`) or any `format!("{user_id}")` call site left over from the
/// bare-`Uuid` era renders byte-identically.
impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `UserId(<uuid>)` tuple form —
/// same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod user_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(UserId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let user = UserId::new(Uuid::new_v4());
        assert_eq!(format!("{user:?}"), format!("{user}"));
        assert!(!format!("{user:?}").contains("UserId"));
    }

    /// The wire-compat invariant: a UUID string serialized/deserialized
    /// through `UserId` is byte-identical to the bare `Uuid` (hard invariant
    /// 2 — no wire changes).
    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let user = UserId::new(id);

        let user_json = serde_json::to_string(&user).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(user_json, uuid_json);

        let round_tripped: UserId = serde_json::from_str(&user_json).unwrap();
        assert_eq!(round_tripped, user);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(UserId::new(id).as_uuid(), id);
    }
}

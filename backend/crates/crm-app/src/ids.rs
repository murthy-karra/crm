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

/// The Person's own identity key (hardening chunk N3,
/// docs/design/type-safety-hardening.md chunk 5): the id every intake,
/// assignment, stage-change, telephony, and Operator call site ultimately
/// resolves to. Same shape as `OrganizationId`/`UserId` for the same
/// reasons: `#[repr(transparent)]` and `#[serde(transparent)]` mean this is
/// exactly a `Uuid` on the wire and in memory, `query!`/`query_as!` binds
/// still go through `.0`, and no `From<Uuid>`/`Into<Uuid>` conversion
/// exists beyond [`PersonId::new`]/[`PersonId::as_uuid`], both visible at
/// the call site.
///
/// Closes the worst adjacency the N3 survey found —
/// `insert_person(tx, org, first, last, stage_id, assigned_user_id)` no
/// longer lets a caller transpose the new Person's stage with its assignee,
/// because both are now distinct types (`StageId` vs `UserId`) rather than
/// adjacent bare `Uuid`s — and a `PersonId` is no more interchangeable with
/// its most frequent neighbor, `InquiryId` (e.g. `NewInquiry { person_id,
/// raw_payload_id, .. }`), than with any other id in this ladder. This is
/// the one representative cross-confusion pinned for the whole N3 cluster
/// (the mechanism itself was already proven by `OrganizationId`/`UserId`;
/// the other three N3 types below do not repeat it):
///
/// ```compile_fail,E0308
/// # use crm_app::ids::{InquiryId, PersonId};
/// # use uuid::Uuid;
/// fn for_inquiry(_inquiry_id: InquiryId) {}
///
/// let person_id = PersonId::new(Uuid::new_v4());
/// for_inquiry(person_id); // does not compile: `PersonId` is not `InquiryId`
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct PersonId(pub Uuid);

impl PersonId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%person_id`) or any `format!("{person_id}")` call site left over from
/// the bare-`Uuid` era renders byte-identically.
impl fmt::Display for PersonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `PersonId(<uuid>)` tuple form
/// — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for PersonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod person_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(PersonId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let person = PersonId::new(Uuid::new_v4());
        assert_eq!(format!("{person:?}"), format!("{person}"));
        assert!(!format!("{person:?}").contains("PersonId"));
    }

    /// The wire-compat invariant: a UUID string serialized/deserialized
    /// through `PersonId` is byte-identical to the bare `Uuid` (hard
    /// invariant 2 — no wire changes).
    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let person = PersonId::new(id);

        let person_json = serde_json::to_string(&person).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(person_json, uuid_json);

        let round_tripped: PersonId = serde_json::from_str(&person_json).unwrap();
        assert_eq!(round_tripped, person);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(PersonId::new(id).as_uuid(), id);
    }
}

/// The Inquiry's own identity key (hardening chunk N3): one Person may have
/// many Inquiries over time (AGENTS.md §4.5), and this is the id that
/// distinguishes them — `ReceiveInquiryOutcome::Resolved.inquiry_id`,
/// `NewInquiry`'s `person_id`/`raw_payload_id` neighbors, and the
/// `resolved_outcome_for_inquiry`/`mark_resolved(id, org, inquiry_id)`
/// payload-vs-inquiry pair the N3 survey flagged. Same transparent,
/// no-implicit-conversion shape as every other id in this module.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct InquiryId(pub Uuid);

impl InquiryId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%inquiry_id`) or any `format!("{inquiry_id}")` call site left over
/// from the bare-`Uuid` era renders byte-identically.
impl fmt::Display for InquiryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `InquiryId(<uuid>)` tuple form
/// — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for InquiryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod inquiry_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(InquiryId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let inquiry = InquiryId::new(Uuid::new_v4());
        assert_eq!(format!("{inquiry:?}"), format!("{inquiry}"));
        assert!(!format!("{inquiry:?}").contains("InquiryId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let inquiry = InquiryId::new(id);

        let inquiry_json = serde_json::to_string(&inquiry).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(inquiry_json, uuid_json);

        let round_tripped: InquiryId = serde_json::from_str(&inquiry_json).unwrap();
        assert_eq!(round_tripped, inquiry);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(InquiryId::new(id).as_uuid(), id);
    }
}

/// The `raw_payload` row's own identity key (hardening chunk N3): the
/// payload half of the crypto AAD tenant binding —
/// `crypto::seal`/`open(key, organization_id, raw_payload_id, ..)` (the org
/// half was typed in N1; the bytes `associated_data` produces are
/// unchanged — only the Rust call site can no longer transpose this
/// argument with `organization_id`, `inquiry_id`, or `person_id`) — and the
/// `mark_resolved`/`mark_unresolved`/`lock_for_processing` `(id, org)`
/// pairs the N3 survey flagged. Also the axum `Path` id at
/// `crm-api/src/routes/intake.rs`'s `/api/intake/unresolved/{id}` routes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct RawPayloadId(pub Uuid);

impl RawPayloadId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%raw_payload_id`) or any `format!("{raw_payload_id}")` call site left
/// over from the bare-`Uuid` era renders byte-identically.
impl fmt::Display for RawPayloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `RawPayloadId(<uuid>)` tuple
/// form — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for RawPayloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod raw_payload_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(RawPayloadId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let raw_payload = RawPayloadId::new(Uuid::new_v4());
        assert_eq!(format!("{raw_payload:?}"), format!("{raw_payload}"));
        assert!(!format!("{raw_payload:?}").contains("RawPayloadId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let raw_payload = RawPayloadId::new(id);

        let raw_payload_json = serde_json::to_string(&raw_payload).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(raw_payload_json, uuid_json);

        let round_tripped: RawPayloadId = serde_json::from_str(&raw_payload_json).unwrap();
        assert_eq!(round_tripped, raw_payload);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(RawPayloadId::new(id).as_uuid(), id);
    }
}

/// The Stage's own identity key (hardening chunk N3, D-019): a
/// per-Organization list, not a fixed enum. Closes
/// `stage::exists(stage_id, organization_id)` and `ChangePersonStage {
/// person_id, stage_id }`'s person/stage adjacency — both were bare `Uuid`
/// before this chunk, so a swapped argument order compiled.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct StageId(pub Uuid);

impl StageId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%stage_id`) or any `format!("{stage_id}")` call site left over from
/// the bare-`Uuid` era renders byte-identically.
impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `StageId(<uuid>)` tuple form —
/// same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod stage_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(StageId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let stage = StageId::new(Uuid::new_v4());
        assert_eq!(format!("{stage:?}"), format!("{stage}"));
        assert!(!format!("{stage:?}").contains("StageId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let stage = StageId::new(id);

        let stage_json = serde_json::to_string(&stage).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(stage_json, uuid_json);

        let round_tripped: StageId = serde_json::from_str(&stage_json).unwrap();
        assert_eq!(round_tripped, stage);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(StageId::new(id).as_uuid(), id);
    }
}

/// The `call` aggregate's own identity key (hardening chunk N4,
/// docs/design/type-safety-hardening.md chunk 6): closes `CallRow.id`/
/// `NewCall.id`/`DialTask.call_id` and the `settle(org, call_id)`/
/// `RealtimeEvent::call_changed(org, occurred_at, correlation_id, call_id,
/// person_id)` adjacencies — every one of that signature's four ids is now
/// a distinct type. Same transparent, no-implicit-conversion shape as
/// every other id in this module; the mechanism (compile_fail doctests on
/// `OrganizationId`/`UserId`/`PersonId`) is already proven, so this chunk
/// adds no new one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct CallId(pub Uuid);

impl CallId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%call_id`) or any `format!("{call_id}")` call site left over from the
/// bare-`Uuid` era renders byte-identically.
impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `CallId(<uuid>)` tuple form —
/// same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod call_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(CallId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let call = CallId::new(Uuid::new_v4());
        assert_eq!(format!("{call:?}"), format!("{call}"));
        assert!(!format!("{call:?}").contains("CallId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let call = CallId::new(id);

        let call_json = serde_json::to_string(&call).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(call_json, uuid_json);

        let round_tripped: CallId = serde_json::from_str(&call_json).unwrap();
        assert_eq!(round_tripped, call);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(CallId::new(id).as_uuid(), id);
    }
}

/// A Person's `contact_method` row identity (hardening chunk N4): the id
/// the call cluster reads (`phone_contact_method_exists`/
/// `_normalized(org, person_id, contact_method_id)`) and stores
/// (`CallRow`/`NewCall`/`DialTask`/`CallCompletedFact`/`CallView`'s
/// `contact_method_id`). The general contact-method listing used for
/// search/display (`domain/person/queries.rs`'s `ContactMethodItem`, the
/// V1 lane's territory) is untouched by this chunk and stays bare `Uuid`;
/// this type covers only the call-cluster sites named above.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct ContactMethodId(pub Uuid);

impl ContactMethodId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%contact_method_id`) or any `format!("{contact_method_id}")` call
/// site left over from the bare-`Uuid` era renders byte-identically.
impl fmt::Display for ContactMethodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `ContactMethodId(<uuid>)`
/// tuple form — same rationale as `OrganizationId`'s `Debug` (ids are not
/// PII).
impl fmt::Debug for ContactMethodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod contact_method_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(ContactMethodId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let cm = ContactMethodId::new(Uuid::new_v4());
        assert_eq!(format!("{cm:?}"), format!("{cm}"));
        assert!(!format!("{cm:?}").contains("ContactMethodId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let cm = ContactMethodId::new(id);

        let cm_json = serde_json::to_string(&cm).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(cm_json, uuid_json);

        let round_tripped: ContactMethodId = serde_json::from_str(&cm_json).unwrap();
        assert_eq!(round_tripped, cm);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(ContactMethodId::new(id).as_uuid(), id);
    }
}

/// An Operator `start_call` proposal's identity (hardening chunk N4,
/// docs/specs/SLICE_006b.md): the `operator_proposal` row id threaded
/// through `POST /api/operator/proposals/{id}/confirm` and
/// `finalize_failed(proposal_id, call_id)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct ProposalId(pub Uuid);

impl ProposalId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%proposal_id`) or any `format!("{proposal_id}")` call site left over
/// from the bare-`Uuid` era renders byte-identically.
impl fmt::Display for ProposalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `ProposalId(<uuid>)` tuple
/// form — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for ProposalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod proposal_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(ProposalId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let proposal = ProposalId::new(Uuid::new_v4());
        assert_eq!(format!("{proposal:?}"), format!("{proposal}"));
        assert!(!format!("{proposal:?}").contains("ProposalId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let proposal = ProposalId::new(id);

        let proposal_json = serde_json::to_string(&proposal).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(proposal_json, uuid_json);

        let round_tripped: ProposalId = serde_json::from_str(&proposal_json).unwrap();
        assert_eq!(round_tripped, proposal);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(ProposalId::new(id).as_uuid(), id);
    }
}

/// The correlation id every fact and realtime event carries (hardening
/// chunk N4): `FactEnvelope.correlation_id`/`CommandContext.correlation_id`
/// and the `RealtimeEvent` constructors' `correlation_id` param. Distinct
/// from `causation_id`, which STAYS `Option<Uuid>` by design (a cross-fact-
/// table union — see `domain/envelope.rs`'s docs — not a single domain
/// entity's id, so it does not belong in this ladder). One declared
/// exception crosses into this type explicitly rather than implicitly: an
/// Operator-confirmed command reuses the Operator turn id as its
/// correlation id (docs/specs/SLICE_006b.md §3) — see
/// `CommandContext::for_operator`, which performs that conversion visibly
/// (`CorrelationId::new(turn_id.as_uuid())`), never by an implicit
/// `From`/`Into`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct CorrelationId(pub Uuid);

impl CorrelationId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%correlation_id`, or `correlation_id = %ctx.correlation_id`) or any
/// `format!("{correlation_id}")` call site left over from the bare-`Uuid`
/// era renders byte-identically.
impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `CorrelationId(<uuid>)` tuple
/// form — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod correlation_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(CorrelationId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let corr = CorrelationId::new(Uuid::new_v4());
        assert_eq!(format!("{corr:?}"), format!("{corr}"));
        assert!(!format!("{corr:?}").contains("CorrelationId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let corr = CorrelationId::new(id);

        let corr_json = serde_json::to_string(&corr).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(corr_json, uuid_json);

        let round_tripped: CorrelationId = serde_json::from_str(&corr_json).unwrap();
        assert_eq!(round_tripped, corr);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(CorrelationId::new(id).as_uuid(), id);
    }
}

/// The `invitation` row's own identity key (hardening chunk N4): closes
/// `mark_invitation_accepted(invitation_id, accepted_user_id)`'s
/// adjacency, plus the rest of the admin-invitation query/fact layer
/// (`InvitationRow`/`InvitationView`/`InvitationIssuedFact`/
/// `InvitationResolvedFact`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct InvitationId(pub Uuid);

impl InvitationId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%invitation_id`) or any `format!("{invitation_id}")` call site left
/// over from the bare-`Uuid` era renders byte-identically.
impl fmt::Display for InvitationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `InvitationId(<uuid>)` tuple
/// form — same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for InvitationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod invitation_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(InvitationId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let invitation = InvitationId::new(Uuid::new_v4());
        assert_eq!(format!("{invitation:?}"), format!("{invitation}"));
        assert!(!format!("{invitation:?}").contains("InvitationId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let invitation = InvitationId::new(id);

        let invitation_json = serde_json::to_string(&invitation).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(invitation_json, uuid_json);

        let round_tripped: InvitationId = serde_json::from_str(&invitation_json).unwrap();
        assert_eq!(round_tripped, invitation);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(InvitationId::new(id).as_uuid(), id);
    }
}

/// An Operator turn's identity (hardening chunk N4): `operator_turn`'s own
/// id, threaded through `OperatorContext.turn_id` (crm-operator; stays
/// bare `Uuid` at the D-028 §5 crate fence) into crm-api's `TurnResponse`
/// and `CommandContext::for_operator`. One declared, explicit crossing
/// exists from this type into [`CorrelationId`] — never the reverse, and
/// never implicit — because an Operator-confirmed command's correlation id
/// IS the turn id that produced it (docs/specs/SLICE_006b.md §3): see
/// `CommandContext::for_operator`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct TurnId(pub Uuid);

impl TurnId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Delegates to the inner `Uuid`'s `Display`, so a tracing span field
/// (`%turn_id`) or any `format!("{turn_id}")` call site left over from the
/// bare-`Uuid` era renders byte-identically.
impl fmt::Display for TurnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Matches `Display` rather than the derived `TurnId(<uuid>)` tuple form —
/// same rationale as `OrganizationId`'s `Debug` (ids are not PII).
impl fmt::Debug for TurnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod turn_id_tests {
    use super::*;

    #[test]
    fn display_matches_the_inner_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(TurnId::new(id).to_string(), id.to_string());
    }

    #[test]
    fn debug_matches_display_not_the_derived_tuple_form() {
        let turn = TurnId::new(Uuid::new_v4());
        assert_eq!(format!("{turn:?}"), format!("{turn}"));
        assert!(!format!("{turn:?}").contains("TurnId"));
    }

    #[test]
    fn serde_round_trip_is_transparent_with_bare_uuid() {
        let id = Uuid::new_v4();
        let turn = TurnId::new(id);

        let turn_json = serde_json::to_string(&turn).unwrap();
        let uuid_json = serde_json::to_string(&id).unwrap();
        assert_eq!(turn_json, uuid_json);

        let round_tripped: TurnId = serde_json::from_str(&turn_json).unwrap();
        assert_eq!(round_tripped, turn);
    }

    #[test]
    fn new_and_as_uuid_round_trip() {
        let id = Uuid::new_v4();
        assert_eq!(TurnId::new(id).as_uuid(), id);
    }
}

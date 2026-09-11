//! Ad-hoc People filter vocabulary (docs/specs/SLICE_011a.md §4; D-043).
//!
//! The wire shape, `FilterDefinition`, is the SAME typed model 011b will
//! persist as a saved list and 011c/d will evaluate as a Today feed — one
//! vocabulary for ad-hoc filtering, saved lists, and Today's feeds (§4a).
//! Nothing in this module touches the database except [`validate_references`]
//! (org-scoped id existence checks); everything else is pure and unit
//! tested here.
//!
//! ## `deny_unknown_fields` on a tagged enum (§4a)
//!
//! Serde's derive does not reliably honor `#[serde(deny_unknown_fields)]`
//! for a newtype variant of an internally tagged enum: the wrapped type
//! ends up seeing the whole object, tag field included, and a tag/field
//! interaction has historically been unreliable across serde versions. To
//! avoid depending on that, [`Clause`] and every nested choice type
//! ([`Assignee`], [`AgeSpec`]) implement `Deserialize`/`Serialize` by hand.
//! Unit test 1 below pins the resulting behavior (reject unknown
//! kind/op/field), not the mechanism.
//!
//! ## Duplicate JSON keys fail closed, even below the top level (§4b)
//!
//! The obvious hand-rolled approach — read the whole object into a
//! `serde_json::Value` and inspect it — is a trap: `Value`'s own object
//! representation is a plain map, so a duplicate key silently
//! last-wins-collapses during that read, before any of this module's own
//! logic ever runs (`{"kind":"bogus","kind":"stage",...}` would decode as
//! `"stage"`). Every manual `Deserialize` here therefore drives
//! `deserializer.deserialize_map(...)` directly, walking `MapAccess`
//! key-by-key and rejecting a repeat immediately (the
//! `read_object_rejecting_duplicate_keys` helper below, and `Assignee`'s
//! own map visitor for its object form) — the same guarantee serde's own
//! struct-derive gives the top-level `FilterDefinition`/per-clause structs
//! for free (duplicate struct fields already fail closed there; only the
//! hand-written types needed this explicitly).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use serde::de::{Error as DeError, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::PgConnection;

use crate::domain::custom_field::{self, FieldType};
use crate::domain::person::queries as person_queries;
use crate::domain::stage;
use crate::domain::tag;
use crate::ids::{CustomFieldId, CustomFieldOptionId, OrganizationId, StageId, TagId, UserId};

/// A JSON value parsed with duplicate-object-key rejection applied AT
/// EVERY NESTING LEVEL (§4b) — the key difference from
/// `serde_json::Value::deserialize`, which silently last-wins-collapses a
/// duplicate key no matter how deep it is nested (an early version of this
/// module used that directly at each hand-written `Deserialize` impl and
/// only caught duplicates at THAT type's own top level: a duplicate `"op"`
/// nested inside a clause's `"age"` object, or a duplicate `"user_id"`
/// nested inside one element of an `"assignees"` array, both slipped
/// through, because the *containing* read had already flattened that
/// nested object into a plain `serde_json::Value` via the naive path
/// before the nested type's own check ever ran). `DupSafeValue` fixes this
/// once, recursively, and converts losslessly into an ordinary
/// `serde_json::Value` — by construction duplicate-free — for the rest of
/// this module's dispatch logic (`serde_json::from_value` on the per-kind
/// structs) to keep using unchanged.
struct DupSafeValue(serde_json::Value);

impl<'de> Deserialize<'de> for DupSafeValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DupSafeVisitor;

        impl<'de> Visitor<'de> for DupSafeVisitor {
            type Value = DupSafeValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "any JSON value with no duplicate object keys at any level"
                )
            }

            fn visit_bool<E: DeError>(self, v: bool) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::Bool(v)))
            }
            fn visit_i64<E: DeError>(self, v: i64) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::from(v)))
            }
            fn visit_u64<E: DeError>(self, v: u64) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::from(v)))
            }
            fn visit_f64<E: DeError>(self, v: f64) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(
                    serde_json::Number::from_f64(v)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null),
                ))
            }
            fn visit_str<E: DeError>(self, v: &str) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::String(v.to_string())))
            }
            fn visit_string<E: DeError>(self, v: String) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::String(v)))
            }
            fn visit_unit<E: DeError>(self) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::Null))
            }
            fn visit_none<E: DeError>(self) -> Result<DupSafeValue, E> {
                Ok(DupSafeValue(serde_json::Value::Null))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<DupSafeValue, D::Error> {
                DupSafeValue::deserialize(d)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<DupSafeValue, A::Error> {
                let mut out = Vec::new();
                while let Some(DupSafeValue(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(DupSafeValue(serde_json::Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<DupSafeValue, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(DeError::custom(format!("duplicate key: {key:?}")));
                    }
                    let DupSafeValue(value) = map.next_value()?;
                    out.insert(key, value);
                }
                Ok(DupSafeValue(serde_json::Value::Object(out)))
            }
        }

        deserializer.deserialize_any(DupSafeVisitor)
    }
}

/// Reads a JSON object into a `serde_json::Map`, rejecting any duplicate
/// key at ANY nesting level within it (§4b, via [`DupSafeValue`]). Used by
/// every hand-written `Deserialize` below in place of
/// `serde_json::Value::deserialize`.
fn read_object_rejecting_duplicate_keys<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<serde_json::Map<String, serde_json::Value>, D::Error> {
    match DupSafeValue::deserialize(deserializer)?.0 {
        serde_json::Value::Object(obj) => Ok(obj),
        _ => Err(DeError::custom("expected a JSON object")),
    }
}

/// Whether `s` is a UUID in canonical lowercase-hyphenated form
/// (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, hex digits only) — §4b:
/// `urn:uuid:...`, `{braced}`, simple (no hyphens), and uppercase forms are
/// all rejected at decode, canonical filters only (011b persists this type;
/// a re-serialized alternate form would not round-trip byte-stable).
/// Deliberately stricter than `uuid::Uuid`'s own `FromStr`, which accepts
/// all of those forms — checked BEFORE parsing, not after, so a lenient
/// parse can never paper over a non-canonical wire string.
fn is_canonical_uuid_string(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| match i {
        8 | 13 | 18 | 23 => b == b'-',
        _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
    })
}

fn parse_canonical_uuid(s: &str) -> Result<uuid::Uuid, String> {
    if !is_canonical_uuid_string(s) {
        return Err(format!(
            "uuid must be canonical lowercase-hyphenated form: {s:?}"
        ));
    }
    uuid::Uuid::parse_str(s).map_err(|_| format!("invalid uuid: {s:?}"))
}

// --- Structural caps (§4b) -------------------------------------------------

pub const MAX_CLAUSES: usize = 20;
pub const MAX_VALUES: usize = 50;
pub const MIN_DAYS: i64 = 1;
pub const MAX_DAYS: i64 = 3650;

/// The four failure classes a filter can produce (§4b, §5a, §7). `Malformed`
/// always maps to HTTP 400 `malformed_request`; `InvalidStage`/
/// `InvalidAssignee` map to the existing non-leaking 422 codes; `Database`
/// maps to `503 unavailable` (review R2) — a transient DB failure mid
/// `validate_references` must never be reported as "this filter is
/// invalid" (a 422), which would additionally cause the web client's
/// URL-origin degrade path to erase a perfectly valid filter on a
/// transient hiccup. Not `PartialEq`/`Copy` (matches `CommandError`'s and
/// `AdminCommandError`'s precedent): `sqlx::Error` implements neither;
/// tests compare via `matches!` instead of `assert_eq!`.
#[derive(Debug)]
pub enum FilterError {
    Malformed,
    InvalidStage,
    InvalidAssignee,
    /// docs/specs/SLICE_011e.md §4b: a `tags`/`not_tags` value does not
    /// exist in the active Organization (`tag::exists`) — non-leaking,
    /// byte-identical for a nonexistent or cross-Organization id, exactly
    /// like `InvalidStage`/`InvalidAssignee`.
    InvalidTag,
    /// A custom field is missing, foreign, archived, or has a different
    /// type from the submitted typed clause. Deliberately one non-leaking
    /// envelope for all four cases.
    InvalidField,
    /// A choice option is missing, foreign, or belongs to another field.
    /// Archived options remain valid by specification.
    InvalidOption,
    Database(sqlx::Error),
}

// --- Age axis -------------------------------------------------------------

/// `{"op": "within_days", "days": N}` | `{"op": "not_within_days", "days": N}`
/// | `{"op": "never"}` (§4a). Manually (de)serialized — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeSpec {
    WithinDays(i64),
    NotWithinDays(i64),
    Never,
}

impl AgeSpec {
    fn kind_label(&self) -> &'static str {
        match self {
            AgeSpec::WithinDays(_) => "within_days",
            AgeSpec::NotWithinDays(_) => "not_within_days",
            AgeSpec::Never => "never",
        }
    }
}

impl Serialize for AgeSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("op", self.kind_label())?;
        match self {
            AgeSpec::WithinDays(days) | AgeSpec::NotWithinDays(days) => {
                map.serialize_entry("days", days)?;
            }
            AgeSpec::Never => {}
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for AgeSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let obj = read_object_rejecting_duplicate_keys(deserializer)?;
        let op = obj
            .get("op")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DeError::custom("age spec missing string field \"op\""))?;
        match op {
            "within_days" | "not_within_days" => {
                let extra: Vec<&String> = obj
                    .keys()
                    .filter(|k| k.as_str() != "op" && k.as_str() != "days")
                    .collect();
                if !extra.is_empty() {
                    return Err(DeError::custom(format!(
                        "unknown field(s) in age spec: {extra:?}"
                    )));
                }
                let days = obj
                    .get("days")
                    .ok_or_else(|| DeError::custom("age spec missing field \"days\""))?;
                let days = days
                    .as_i64()
                    .ok_or_else(|| DeError::custom("age spec \"days\" must be an integer"))?;
                if op == "within_days" {
                    Ok(AgeSpec::WithinDays(days))
                } else {
                    Ok(AgeSpec::NotWithinDays(days))
                }
            }
            "never" => {
                let extra: Vec<&String> = obj.keys().filter(|k| k.as_str() != "op").collect();
                if !extra.is_empty() {
                    return Err(DeError::custom(format!(
                        "unknown field(s) in age spec: {extra:?}"
                    )));
                }
                Ok(AgeSpec::Never)
            }
            other => Err(DeError::custom(format!("unknown age op: {other:?}"))),
        }
    }
}

// --- Assignee ---------------------------------------------------------------

/// `"me"` | `"unassigned"` | `{"user_id": "<uuid>"}` (§4a, §4c). Manually
/// (de)serialized — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Assignee {
    Me,
    Unassigned,
    User(UserId),
}

impl Serialize for Assignee {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Assignee::Me => serializer.serialize_str("me"),
            Assignee::Unassigned => serializer.serialize_str("unassigned"),
            Assignee::User(id) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("user_id", &id.0)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Assignee {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AssigneeVisitor;

        impl<'de> Visitor<'de> for AssigneeVisitor {
            type Value = Assignee;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "\"me\", \"unassigned\", or {{\"user_id\": <uuid>}}")
            }

            fn visit_str<E: DeError>(self, v: &str) -> Result<Assignee, E> {
                match v {
                    "me" => Ok(Assignee::Me),
                    "unassigned" => Ok(Assignee::Unassigned),
                    other => Err(DeError::custom(format!(
                        "unknown assignee token: {other:?}"
                    ))),
                }
            }

            // Streamed key-by-key (not via `serde_json::Value`) so a
            // duplicate "user_id" key fails closed rather than
            // last-wins-collapsing (§4b) — see the module docs.
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Assignee, A::Error> {
                let mut user_id: Option<String> = None;
                let mut seen = HashSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(DeError::custom(format!("duplicate key: {key:?}")));
                    }
                    if key == "user_id" {
                        user_id = Some(map.next_value()?);
                    } else {
                        let _ignored: serde_json::Value = map.next_value()?;
                        return Err(DeError::custom(format!(
                            "unknown field(s) in assignee: [{key:?}]"
                        )));
                    }
                }
                let user_id = user_id
                    .ok_or_else(|| DeError::custom("assignee object missing \"user_id\""))?;
                // §4b: canonical lowercase-hyphenated uuid only — checked
                // before parsing, not after (see
                // `is_canonical_uuid_string`'s doc comment).
                let user_id = parse_canonical_uuid(&user_id).map_err(DeError::custom)?;
                Ok(Assignee::User(UserId::new(user_id)))
            }
        }

        deserializer.deserialize_any(AssigneeVisitor)
    }
}

// --- Per-clause payload structs (each deny_unknown_fields) ------------------

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageClause {
    pub stage_ids: Vec<StageId>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignedToClause {
    pub assignees: Vec<Assignee>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceClause {
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgeClause {
    pub age: AgeSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoolClause {
    pub value: bool,
}

/// `{"tag_ids": [uuid, ...]}` — shared payload for `tags` (any-of) and
/// `not_tags` (none-of) (docs/specs/SLICE_011e.md §4a). One clause struct
/// shared by both kinds, like `StageClause` is shared by nothing but is
/// structurally validated identically (§4b: non-empty, ≤ 50, no
/// duplicates).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TagIdsClause {
    pub tag_ids: Vec<TagId>,
}

/// Typed custom-field predicates. These deliberately do not use a generic
/// JSON payload: the fixed vocabulary is what keeps every SQL bind static.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomTextClause {
    pub field_id: CustomFieldId,
    pub test: TextTest,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomNumberClause {
    pub field_id: CustomFieldId,
    pub test: NumberTest,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomDateClause {
    pub field_id: CustomFieldId,
    pub test: DateTest,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomChoiceClause {
    pub field_id: CustomFieldId,
    pub test: ChoiceTest,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextTest {
    IsSet,
    IsNotSet,
    Contains { text: String },
    NotContains { text: String },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum NumberTest {
    IsSet,
    IsNotSet,
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<String>,
    },
    NotRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<String>,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum DateTest {
    IsSet,
    IsNotSet,
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<NaiveDate>,
    },
    NotRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<NaiveDate>,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceTest {
    IsSet,
    IsNotSet,
    AnyOf {
        option_ids: Vec<CustomFieldOptionId>,
    },
    NoneOf {
        option_ids: Vec<CustomFieldOptionId>,
    },
}

macro_rules! redacted_filter_debug {
    ($($type:ty),+ $(,)?) => {$(
        impl fmt::Debug for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(stringify!($type))
            }
        }
    )+};
}

redacted_filter_debug!(
    CustomTextClause,
    CustomNumberClause,
    CustomDateClause,
    CustomChoiceClause,
    TextTest,
    NumberTest,
    DateTest,
    ChoiceTest,
);

// --- Clause -----------------------------------------------------------------

/// One filter clause (§4a). Manually (de)serialized on the `"kind"`
/// discriminator — see the module docs.
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Stage(StageClause),
    AssignedTo(AssignedToClause),
    Source(SourceClause),
    Created(AgeClause),
    LastInquiry(AgeClause),
    LastContact(AgeClause),
    LastInbound(AgeClause),
    HasReplied(BoolClause),
    HasPhone(BoolClause),
    HasEmail(BoolClause),
    /// docs/specs/SLICE_011d.md §2: derived, viewer-independent. True when
    /// the Person has at least one inquiry after the effective last contact
    /// attempt by any member (or has inquiries and no attempt at all).
    AwaitingResponse(BoolClause),
    /// docs/specs/SLICE_011d.md §2: derived, viewer-independent. True when
    /// the latest inbound correspondence is later than both the effective
    /// last contact attempt and the latest outbound correspondence.
    ClientRepliedUnanswered(BoolClause),
    /// docs/specs/SLICE_011d.md §2: derived, VIEWER-relative (the viewer is
    /// the caller). True when a call of the viewer's to the Person is
    /// `ended`/`failed` with a non-null `ended_at` and its root automatic
    /// contact attempt has no correction.
    AwaitingCallOutcome(BoolClause),
    /// docs/specs/SLICE_011e.md §4: the Person carries at least one of the
    /// listed tags (`EXISTS`, OR semantics like every value array).
    Tags(TagIdsClause),
    /// docs/specs/SLICE_011e.md §4: the Person carries none of the listed
    /// tags (`NOT EXISTS`); an untagged Person matches. Exact complement of
    /// `Tags` with the same ids.
    NotTags(TagIdsClause),
    CustomText(CustomTextClause),
    CustomNumber(CustomNumberClause),
    CustomDate(CustomDateClause),
    CustomChoice(CustomChoiceClause),
}

impl Clause {
    /// The wire `"kind"` token — also the static clause-kind vocabulary
    /// used for the `filter_kinds` span field (§7) and duplicate-kind
    /// detection (§4a).
    pub fn kind_label(&self) -> &'static str {
        match self {
            Clause::Stage(_) => "stage",
            Clause::AssignedTo(_) => "assigned_to",
            Clause::Source(_) => "source",
            Clause::Created(_) => "created",
            Clause::LastInquiry(_) => "last_inquiry",
            Clause::LastContact(_) => "last_contact",
            Clause::LastInbound(_) => "last_inbound",
            Clause::HasReplied(_) => "has_replied",
            Clause::HasPhone(_) => "has_phone",
            Clause::HasEmail(_) => "has_email",
            Clause::AwaitingResponse(_) => "awaiting_response",
            Clause::ClientRepliedUnanswered(_) => "client_replied_unanswered",
            Clause::AwaitingCallOutcome(_) => "awaiting_call_outcome",
            Clause::Tags(_) => "tags",
            Clause::NotTags(_) => "not_tags",
            Clause::CustomText(_) => "custom_text",
            Clause::CustomNumber(_) => "custom_number",
            Clause::CustomDate(_) => "custom_date",
            Clause::CustomChoice(_) => "custom_choice",
        }
    }
}

impl Serialize for Clause {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Re-serialize the per-kind payload to a Value, splice in "kind",
        // and emit the merged object — keeps this in lockstep with the
        // per-clause structs' own `Serialize` derives instead of
        // duplicating their field lists here.
        fn merged<T: Serialize>(kind: &str, payload: &T) -> serde_json::Value {
            let mut obj = match serde_json::to_value(payload) {
                Ok(serde_json::Value::Object(obj)) => obj,
                _ => serde_json::Map::new(),
            };
            obj.insert(
                "kind".to_string(),
                serde_json::Value::String(kind.to_string()),
            );
            serde_json::Value::Object(obj)
        }
        let value = match self {
            Clause::Stage(c) => merged("stage", c),
            Clause::AssignedTo(c) => merged("assigned_to", c),
            Clause::Source(c) => merged("source", c),
            Clause::Created(c) => merged("created", c),
            Clause::LastInquiry(c) => merged("last_inquiry", c),
            Clause::LastContact(c) => merged("last_contact", c),
            Clause::LastInbound(c) => merged("last_inbound", c),
            Clause::HasReplied(c) => merged("has_replied", c),
            Clause::HasPhone(c) => merged("has_phone", c),
            Clause::HasEmail(c) => merged("has_email", c),
            Clause::AwaitingResponse(c) => merged("awaiting_response", c),
            Clause::ClientRepliedUnanswered(c) => merged("client_replied_unanswered", c),
            Clause::AwaitingCallOutcome(c) => merged("awaiting_call_outcome", c),
            Clause::Tags(c) => merged("tags", c),
            Clause::NotTags(c) => merged("not_tags", c),
            Clause::CustomText(c) => merged("custom_text", c),
            Clause::CustomNumber(c) => merged("custom_number", c),
            Clause::CustomDate(c) => merged("custom_date", c),
            Clause::CustomChoice(c) => merged("custom_choice", c),
        };
        value.serialize(serializer)
    }
}

/// §4b: every string in `remaining[field]` (when present and an array) must
/// already look like a canonical uuid — checked BEFORE handing `remaining`
/// to `StageClause`'s normal derive, whose `StageId`/`Uuid` deserialize is
/// deliberately left as-is (shared with every other route on the wire;
/// tightening it globally would be an unrelated, out-of-scope contract
/// change) — this pre-check is what actually enforces canonical-only for
/// `stage_ids` without touching `ids.rs`. A non-string array element is
/// left for the subsequent real parse to reject with its own type error.
fn require_canonical_uuid_strings<E: DeError>(
    remaining: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<(), E> {
    if let Some(arr) = remaining.get(field).and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                if !is_canonical_uuid_string(s) {
                    return Err(DeError::custom(format!(
                        "{field} must be canonical lowercase-hyphenated uuids: {s:?}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_custom_test_shape<E: DeError>(
    kind: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), E> {
    let test = object
        .get("test")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| DeError::custom("custom field test must be an object"))?;
    let op = test
        .get("op")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| DeError::custom("custom field test needs string op"))?;
    let operands: &[&str] = match (kind, op) {
        (_, "is_set" | "is_not_set") => &[],
        ("custom_text", "contains" | "not_contains") => &["text"],
        ("custom_number" | "custom_date", "range" | "not_range") => &["min", "max"],
        ("custom_choice", "any_of" | "none_of") => &["option_ids"],
        _ => return Err(DeError::custom("unknown custom field test operation")),
    };
    if test
        .keys()
        .any(|key| key != "op" && !operands.contains(&key.as_str()))
    {
        return Err(DeError::custom("unknown custom field test operand"));
    }
    for operand in operands {
        if let Some(value) = test.get(*operand) {
            if value.is_null() {
                return Err(DeError::custom("custom field test operands cannot be null"));
            }
        }
    }
    if kind == "custom_date" && matches!(op, "range" | "not_range") {
        for operand in ["min", "max"] {
            if let Some(value) = test.get(operand) {
                let raw = value
                    .as_str()
                    .ok_or_else(|| DeError::custom("date bounds must be strings"))?;
                if raw.len() != 10
                    || !raw.as_bytes().iter().enumerate().all(|(index, byte)| {
                        matches!(index, 4 | 7) && *byte == b'-'
                            || !matches!(index, 4 | 7) && byte.is_ascii_digit()
                    })
                {
                    return Err(DeError::custom("date bounds must be canonical YYYY-MM-DD"));
                }
            }
        }
    }
    if operands == ["text"] && !test.contains_key("text") {
        return Err(DeError::custom("text test needs text"));
    }
    if operands == ["option_ids"] && !test.contains_key("option_ids") {
        return Err(DeError::custom("choice test needs option_ids"));
    }
    Ok(())
}

impl<'de> Deserialize<'de> for Clause {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut obj = read_object_rejecting_duplicate_keys(deserializer)?;
        let kind = obj
            .remove("kind")
            .ok_or_else(|| DeError::custom("clause missing string field \"kind\""))?;
        let kind = kind
            .as_str()
            .ok_or_else(|| DeError::custom("clause \"kind\" must be a string"))?
            .to_string();
        if kind == "stage" {
            require_canonical_uuid_strings(&obj, "stage_ids")?;
        } else if kind == "tags" || kind == "not_tags" {
            require_canonical_uuid_strings(&obj, "tag_ids")?;
        } else if matches!(
            kind.as_str(),
            "custom_text" | "custom_number" | "custom_date" | "custom_choice"
        ) {
            validate_custom_test_shape(&kind, &obj)?;
            if let Some(field_id) = obj.get("field_id").and_then(|value| value.as_str()) {
                if !is_canonical_uuid_string(field_id) {
                    return Err(DeError::custom(
                        "field_id must be canonical lowercase-hyphenated uuid",
                    ));
                }
            }
            if let Some(test) = obj.get("test").and_then(|value| value.as_object()) {
                require_canonical_uuid_strings(test, "option_ids")?;
            }
        }
        let remaining = serde_json::Value::Object(obj);
        fn decode<T: serde::de::DeserializeOwned, E: DeError>(
            value: serde_json::Value,
        ) -> Result<T, E> {
            serde_json::from_value(value).map_err(|err| E::custom(err.to_string()))
        }
        match kind.as_str() {
            "stage" => Ok(Clause::Stage(decode(remaining)?)),
            "assigned_to" => Ok(Clause::AssignedTo(decode(remaining)?)),
            "source" => Ok(Clause::Source(decode(remaining)?)),
            "created" => Ok(Clause::Created(decode(remaining)?)),
            "last_inquiry" => Ok(Clause::LastInquiry(decode(remaining)?)),
            "last_contact" => Ok(Clause::LastContact(decode(remaining)?)),
            "last_inbound" => Ok(Clause::LastInbound(decode(remaining)?)),
            "has_replied" => Ok(Clause::HasReplied(decode(remaining)?)),
            "has_phone" => Ok(Clause::HasPhone(decode(remaining)?)),
            "has_email" => Ok(Clause::HasEmail(decode(remaining)?)),
            "awaiting_response" => Ok(Clause::AwaitingResponse(decode(remaining)?)),
            "client_replied_unanswered" => Ok(Clause::ClientRepliedUnanswered(decode(remaining)?)),
            "awaiting_call_outcome" => Ok(Clause::AwaitingCallOutcome(decode(remaining)?)),
            "tags" => Ok(Clause::Tags(decode(remaining)?)),
            "not_tags" => Ok(Clause::NotTags(decode(remaining)?)),
            "custom_text" => Ok(Clause::CustomText(decode(remaining)?)),
            "custom_number" => Ok(Clause::CustomNumber(decode(remaining)?)),
            "custom_date" => Ok(Clause::CustomDate(decode(remaining)?)),
            "custom_choice" => Ok(Clause::CustomChoice(decode(remaining)?)),
            other => Err(DeError::custom(format!("unknown clause kind: {other:?}"))),
        }
    }
}

// --- FilterDefinition ---------------------------------------------------

fn deserialize_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let v = u32::deserialize(deserializer)?;
    if v != 1 {
        return Err(DeError::custom(format!("unsupported filter version: {v}")));
    }
    Ok(v)
}

/// The typed, versioned filter (§4). This same type is what 011b persists;
/// fail-closed decode IS the ladder's "unknown-clause-fails-closed on
/// read" (§4a).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilterDefinition {
    #[serde(deserialize_with = "deserialize_version")]
    pub version: u32,
    pub clauses: Vec<Clause>,
}

fn validate_source(raw: &str) -> bool {
    // Deliberately NOT `Source::parse` (which trims/lowercases): §4b
    // requires the wire value to already be canonical, verbatim — a
    // divergence stated in the spec and disclosed in the implementation
    // report.
    !raw.is_empty()
        && raw.len() <= 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn validate_string_values(values: &[String]) -> Result<(), FilterError> {
    if values.is_empty() || values.len() > MAX_VALUES {
        return Err(FilterError::Malformed);
    }
    let mut seen = HashSet::new();
    for v in values {
        if !seen.insert(v.as_str()) {
            return Err(FilterError::Malformed);
        }
    }
    Ok(())
}

fn validate_age(age: &AgeSpec, allow_never: bool) -> Result<(), FilterError> {
    match age {
        AgeSpec::WithinDays(days) | AgeSpec::NotWithinDays(days) => {
            if *days < MIN_DAYS || *days > MAX_DAYS {
                return Err(FilterError::Malformed);
            }
            Ok(())
        }
        AgeSpec::Never => {
            if allow_never {
                Ok(())
            } else {
                Err(FilterError::Malformed)
            }
        }
    }
}

fn custom_field_id(clause: &Clause) -> Option<CustomFieldId> {
    match clause {
        Clause::CustomText(c) => Some(c.field_id),
        Clause::CustomNumber(c) => Some(c.field_id),
        Clause::CustomDate(c) => Some(c.field_id),
        Clause::CustomChoice(c) => Some(c.field_id),
        _ => None,
    }
}

fn custom_field_type(clause: &Clause) -> Option<FieldType> {
    match clause {
        Clause::CustomText(_) => Some(FieldType::Text),
        Clause::CustomNumber(_) => Some(FieldType::Number),
        Clause::CustomDate(_) => Some(FieldType::Date),
        Clause::CustomChoice(_) => Some(FieldType::Choice),
        _ => None,
    }
}

fn choice_option_ids(clause: &Clause) -> Option<&[CustomFieldOptionId]> {
    match clause {
        Clause::CustomChoice(CustomChoiceClause {
            test: ChoiceTest::AnyOf { option_ids } | ChoiceTest::NoneOf { option_ids },
            ..
        }) => Some(option_ids),
        _ => None,
    }
}

async fn validate_custom_reference(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    clause: &Clause,
) -> Result<(), FilterError> {
    let (Some(field_id), Some(expected_type)) =
        (custom_field_id(clause), custom_field_type(clause))
    else {
        return Ok(());
    };
    let actual = custom_field::live_field_type_for_filter(conn, organization_id, field_id)
        .await
        .map_err(FilterError::Database)?;
    if actual != Some(expected_type) {
        return Err(FilterError::InvalidField);
    }
    if let Some(option_ids) = choice_option_ids(clause) {
        let valid = custom_field::option_ids_belong_to_field_for_filter(
            conn,
            organization_id,
            field_id,
            option_ids,
        )
        .await
        .map_err(FilterError::Database)?;
        if !valid {
            return Err(FilterError::InvalidOption);
        }
    }
    Ok(())
}

async fn validate_custom_reference_until(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    clause: &Clause,
    deadline: Instant,
) -> Result<(), FilterError> {
    let (Some(field_id), Some(expected_type)) =
        (custom_field_id(clause), custom_field_type(clause))
    else {
        return Ok(());
    };
    set_statement_timeout_until(conn, deadline).await?;
    let actual = custom_field::live_field_type_for_filter(conn, organization_id, field_id)
        .await
        .map_err(FilterError::Database)?;
    if actual != Some(expected_type) {
        return Err(FilterError::InvalidField);
    }
    if let Some(option_ids) = choice_option_ids(clause) {
        set_statement_timeout_until(conn, deadline).await?;
        let valid = custom_field::option_ids_belong_to_field_for_filter(
            conn,
            organization_id,
            field_id,
            option_ids,
        )
        .await
        .map_err(FilterError::Database)?;
        if !valid {
            return Err(FilterError::InvalidOption);
        }
    }
    Ok(())
}

fn decimal_to_scaled(raw: &str) -> Option<i128> {
    custom_field::validate_number_pattern(raw).ok()?;
    let negative = raw.starts_with('-');
    let unsigned = raw.strip_prefix('-').unwrap_or(raw);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let whole = whole.parse::<i128>().ok()?;
    let fraction = format!("{fraction:0<4}").parse::<i128>().ok()?;
    let scaled = whole.checked_mul(10_000)?.checked_add(fraction)?;
    Some(if negative { -scaled } else { scaled })
}

fn validate_text_test(test: &TextTest) -> Result<(), FilterError> {
    let value = match test {
        TextTest::Contains { text } | TextTest::NotContains { text } => text,
        TextTest::IsSet | TextTest::IsNotSet => return Ok(()),
    };
    let normalized =
        custom_field::validate_text_value(value).map_err(|_| FilterError::Malformed)?;
    if normalized != *value {
        return Err(FilterError::Malformed);
    }
    Ok(())
}

fn validate_number_test(test: &NumberTest) -> Result<(), FilterError> {
    let (min, max) = match test {
        NumberTest::Range { min, max } | NumberTest::NotRange { min, max } => (min, max),
        NumberTest::IsSet | NumberTest::IsNotSet => return Ok(()),
    };
    let min_scaled = match min.as_deref() {
        Some(value) => Some(decimal_to_scaled(value).ok_or(FilterError::Malformed)?),
        None => None,
    };
    let max_scaled = match max.as_deref() {
        Some(value) => Some(decimal_to_scaled(value).ok_or(FilterError::Malformed)?),
        None => None,
    };
    match (min_scaled, max_scaled) {
        (None, None) => Err(FilterError::Malformed),
        (Some(min), Some(max)) if min > max => Err(FilterError::Malformed),
        _ => Ok(()),
    }
}

fn validate_date_test(test: &DateTest) -> Result<(), FilterError> {
    let (min, max) = match test {
        DateTest::Range { min, max } | DateTest::NotRange { min, max } => (min, max),
        DateTest::IsSet | DateTest::IsNotSet => return Ok(()),
    };
    for date in [min, max].into_iter().flatten() {
        custom_field::validate_date_range(*date).map_err(|_| FilterError::Malformed)?;
    }
    match (min, max) {
        (None, None) => Err(FilterError::Malformed),
        (Some(min), Some(max)) if min > max => Err(FilterError::Malformed),
        _ => Ok(()),
    }
}

fn validate_choice_test(test: &ChoiceTest) -> Result<(), FilterError> {
    let ids = match test {
        ChoiceTest::AnyOf { option_ids } | ChoiceTest::NoneOf { option_ids } => option_ids,
        ChoiceTest::IsSet | ChoiceTest::IsNotSet => return Ok(()),
    };
    if ids.is_empty()
        || ids.len() > MAX_VALUES
        || ids.iter().map(|id| id.0).collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(FilterError::Malformed);
    }
    Ok(())
}

impl FilterDefinition {
    /// IDs named by custom clauses, in wire order. Description loaders use
    /// this to keep metadata reads bounded to the definition being shown.
    pub fn custom_field_ids(&self) -> Vec<CustomFieldId> {
        self.clauses.iter().filter_map(custom_field_id).collect()
    }
    /// Pure structural validation (§4b): caps, duplicate kinds/values,
    /// empty arrays, `days` bounds, `never` forbidden on `created`,
    /// non-canonical `source` values. Ordering = clause order, first
    /// failure wins. Never touches the database — org-scoped id checks
    /// are [`validate_references`], run only after this succeeds.
    pub fn validate(&self) -> Result<(), FilterError> {
        if self.clauses.len() > MAX_CLAUSES {
            return Err(FilterError::Malformed);
        }
        let mut seen_kinds = HashSet::new();
        let mut custom_fields = HashSet::new();
        let mut custom_count = 0usize;
        for clause in &self.clauses {
            if custom_field_id(clause).is_none() && !seen_kinds.insert(clause.kind_label()) {
                return Err(FilterError::Malformed);
            }
            if let Some(field_id) = custom_field_id(clause) {
                custom_count += 1;
                if custom_count > 5 || !custom_fields.insert(field_id.0) {
                    return Err(FilterError::Malformed);
                }
            }
            match clause {
                Clause::Stage(c) => {
                    if c.stage_ids.is_empty() || c.stage_ids.len() > MAX_VALUES {
                        return Err(FilterError::Malformed);
                    }
                    let mut seen = HashSet::new();
                    for id in &c.stage_ids {
                        if !seen.insert(id.0) {
                            return Err(FilterError::Malformed);
                        }
                    }
                }
                Clause::AssignedTo(c) => {
                    if c.assignees.is_empty() || c.assignees.len() > MAX_VALUES {
                        return Err(FilterError::Malformed);
                    }
                    let mut seen = HashSet::new();
                    for a in &c.assignees {
                        if !seen.insert(*a) {
                            return Err(FilterError::Malformed);
                        }
                    }
                }
                Clause::Source(c) => {
                    validate_string_values(&c.sources)?;
                    for s in &c.sources {
                        if !validate_source(s) {
                            return Err(FilterError::Malformed);
                        }
                    }
                }
                Clause::Created(c) => validate_age(&c.age, false)?,
                Clause::LastInquiry(c) | Clause::LastContact(c) | Clause::LastInbound(c) => {
                    validate_age(&c.age, true)?
                }
                Clause::HasReplied(_) | Clause::HasPhone(_) | Clause::HasEmail(_) => {}
                Clause::AwaitingResponse(_)
                | Clause::ClientRepliedUnanswered(_)
                | Clause::AwaitingCallOutcome(_) => {}
                Clause::Tags(c) | Clause::NotTags(c) => {
                    if c.tag_ids.is_empty() || c.tag_ids.len() > MAX_VALUES {
                        return Err(FilterError::Malformed);
                    }
                    let mut seen = HashSet::new();
                    for id in &c.tag_ids {
                        if !seen.insert(id.0) {
                            return Err(FilterError::Malformed);
                        }
                    }
                }
                Clause::CustomText(c) => validate_text_test(&c.test)?,
                Clause::CustomNumber(c) => validate_number_test(&c.test)?,
                Clause::CustomDate(c) => validate_date_test(&c.test)?,
                Clause::CustomChoice(c) => validate_choice_test(&c.test)?,
            }
        }
        Ok(())
    }

    /// The comma-joined static clause-kind vocabulary, in clause order —
    /// the `filter_kinds` span field (§7). Empty string for `clauses: []`.
    pub fn kinds_field(&self) -> String {
        self.clauses
            .iter()
            .map(Clause::kind_label)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Org-scoped reference validation (§4b), run only after
    /// [`validate`](Self::validate) succeeds: every `stage_id` must exist
    /// in `organization_id` (`invalid_stage`); every concrete assignee
    /// `user_id` must be an org member, any status (D-027) — `me` needs no
    /// check (§4c). Ordering = clause order, first failure wins. A `sqlx`
    /// failure while checking is `FilterError::Database` (review R2), NEVER
    /// silently reported as the id itself being invalid — DB down means
    /// `503 unavailable`, per §7, exactly like every other read path in
    /// this codebase.
    pub async fn validate_references(
        &self,
        conn: &mut PgConnection,
        organization_id: OrganizationId,
    ) -> Result<(), FilterError> {
        for clause in &self.clauses {
            match clause {
                Clause::Stage(c) => {
                    for id in &c.stage_ids {
                        let exists = stage::exists(conn, *id, organization_id)
                            .await
                            .map_err(FilterError::Database)?;
                        if !exists {
                            return Err(FilterError::InvalidStage);
                        }
                    }
                }
                Clause::AssignedTo(c) => {
                    for a in &c.assignees {
                        if let Assignee::User(user_id) = a {
                            let is_member = person_queries::is_organization_member(
                                conn,
                                organization_id,
                                *user_id,
                            )
                            .await
                            .map_err(FilterError::Database)?;
                            if !is_member {
                                return Err(FilterError::InvalidAssignee);
                            }
                        }
                    }
                }
                Clause::Tags(c) | Clause::NotTags(c) => {
                    for id in &c.tag_ids {
                        let exists = tag::exists(conn, organization_id, *id)
                            .await
                            .map_err(tag_error_to_database)?;
                        if !exists {
                            return Err(FilterError::InvalidTag);
                        }
                    }
                }
                Clause::CustomText(_)
                | Clause::CustomNumber(_)
                | Clause::CustomDate(_)
                | Clause::CustomChoice(_) => {
                    validate_custom_reference(conn, organization_id, clause).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The Today source evaluator validates each source inside one absolute
    /// wall-clock allowance. Re-arm PostgreSQL's transaction-local statement
    /// timeout before every reference probe so a late probe receives only the
    /// source's remaining time, never a fresh full-source budget.
    pub async fn validate_references_until(
        &self,
        conn: &mut PgConnection,
        organization_id: OrganizationId,
        deadline: Instant,
    ) -> Result<(), FilterError> {
        for clause in &self.clauses {
            match clause {
                Clause::Stage(c) => {
                    for id in &c.stage_ids {
                        set_statement_timeout_until(conn, deadline).await?;
                        let exists = stage::exists(conn, *id, organization_id)
                            .await
                            .map_err(FilterError::Database)?;
                        if !exists {
                            return Err(FilterError::InvalidStage);
                        }
                    }
                }
                Clause::AssignedTo(c) => {
                    for a in &c.assignees {
                        if let Assignee::User(user_id) = a {
                            set_statement_timeout_until(conn, deadline).await?;
                            let is_member = person_queries::is_organization_member(
                                conn,
                                organization_id,
                                *user_id,
                            )
                            .await
                            .map_err(FilterError::Database)?;
                            if !is_member {
                                return Err(FilterError::InvalidAssignee);
                            }
                        }
                    }
                }
                Clause::Tags(c) | Clause::NotTags(c) => {
                    for id in &c.tag_ids {
                        set_statement_timeout_until(conn, deadline).await?;
                        let exists = tag::exists(conn, organization_id, *id)
                            .await
                            .map_err(tag_error_to_database)?;
                        if !exists {
                            return Err(FilterError::InvalidTag);
                        }
                    }
                }
                Clause::CustomText(_)
                | Clause::CustomNumber(_)
                | Clause::CustomDate(_)
                | Clause::CustomChoice(_) => {
                    validate_custom_reference_until(conn, organization_id, clause, deadline)
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// `tag::exists` returns `Result<bool, tag::TagError>` (the module's shared
/// command/read error shape) rather than a bare `sqlx::Error` like
/// `stage::exists`; `TagError::Database` is the only variant a simple
/// existence probe can actually produce, but the type still requires this
/// mapping to be total.
fn tag_error_to_database(err: tag::TagError) -> FilterError {
    match err {
        tag::TagError::Database(error) => FilterError::Database(error),
        _ => FilterError::Database(sqlx::Error::Decode(
            "tag::exists returned an unexpected non-database TagError".into(),
        )),
    }
}

async fn set_statement_timeout_until(
    conn: &mut PgConnection,
    deadline: Instant,
) -> Result<(), FilterError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| FilterError::Database(sqlx::Error::PoolTimedOut))?;
    // Keep the Today source wall-clock deadline authoritative while giving
    // PostgreSQL enough time to report a statement cancellation before its
    // caller's outer timeout begins savepoint recovery.
    let milliseconds = remaining
        .saturating_sub(Duration::from_millis(10))
        .as_millis()
        .clamp(1, 500)
        .to_string();
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(milliseconds)
        .execute(conn)
        .await
        .map_err(FilterError::Database)?;
    Ok(())
}

// --- SQL binding params (person/queries.rs's `filtered_summaries`) --------

/// The NULL-guarded bind parameters `filtered_summaries` executes against
/// (§4e). Built from a validated [`FilterDefinition`] plus the caller's own
/// id, which resolves `me` server-side — the SQL never sees the token "me"
/// (§4c).
#[derive(Clone)]
pub struct PersonFilterParams {
    pub stage_ids: Option<Vec<uuid::Uuid>>,
    pub assigned_user_ids: Option<Vec<uuid::Uuid>>,
    pub assigned_include_unassigned: Option<bool>,
    pub sources: Option<Vec<String>>,
    pub created_within_days: Option<i32>,
    pub created_not_within_days: Option<i32>,
    pub created_never: Option<bool>,
    pub last_inquiry_within_days: Option<i32>,
    pub last_inquiry_not_within_days: Option<i32>,
    pub last_inquiry_never: Option<bool>,
    pub last_contact_within_days: Option<i32>,
    pub last_contact_not_within_days: Option<i32>,
    pub last_contact_never: Option<bool>,
    pub last_inbound_within_days: Option<i32>,
    pub last_inbound_not_within_days: Option<i32>,
    pub last_inbound_never: Option<bool>,
    pub has_replied: Option<bool>,
    pub has_phone: Option<bool>,
    pub has_email: Option<bool>,
    /// docs/specs/SLICE_011d.md §2.
    pub awaiting_response: Option<bool>,
    /// docs/specs/SLICE_011d.md §2.
    pub client_replied_unanswered: Option<bool>,
    /// docs/specs/SLICE_011d.md §2.
    pub awaiting_call_outcome: Option<bool>,
    /// docs/specs/SLICE_011e.md §4: `tags` (any-of).
    pub tag_ids_any: Option<Vec<uuid::Uuid>>,
    /// docs/specs/SLICE_011e.md §4: `not_tags` (none-of).
    pub tag_ids_none: Option<Vec<uuid::Uuid>>,
    /// Exactly five submitted custom clauses in wire order. SQL consumers
    /// expand these into fixed nine-bind matrices; absent slots bind NULL.
    pub custom_slots: [Option<CustomFilterSlot>; 5],
    /// The bound viewer id, used ONLY by the `awaiting_call_outcome` probe
    /// (§2: "for the first time, a bound viewer id"). Always set by
    /// [`to_query_params`](FilterDefinition::to_query_params) regardless of
    /// which clauses are present; every eleven-statement predicate that
    /// reads it is itself NULL-guarded on `awaiting_call_outcome`, so an
    /// absent clause never lets an unrelated nil/default value leak into a
    /// query result — this field is inert whenever that clause is absent.
    pub viewer_id: uuid::Uuid,
}

impl Default for PersonFilterParams {
    fn default() -> Self {
        Self {
            stage_ids: None,
            assigned_user_ids: None,
            assigned_include_unassigned: None,
            sources: None,
            created_within_days: None,
            created_not_within_days: None,
            created_never: None,
            last_inquiry_within_days: None,
            last_inquiry_not_within_days: None,
            last_inquiry_never: None,
            last_contact_within_days: None,
            last_contact_not_within_days: None,
            last_contact_never: None,
            last_inbound_within_days: None,
            last_inbound_not_within_days: None,
            last_inbound_never: None,
            has_replied: None,
            has_phone: None,
            has_email: None,
            awaiting_response: None,
            client_replied_unanswered: None,
            awaiting_call_outcome: None,
            tag_ids_any: None,
            tag_ids_none: None,
            custom_slots: std::array::from_fn(|_| None),
            viewer_id: uuid::Uuid::nil(),
        }
    }
}

impl PersonFilterParams {
    pub fn custom_slot(&self, index: usize) -> Option<&CustomFilterSlot> {
        self.custom_slots.get(index).and_then(Option::as_ref)
    }
}

/// One static custom-field predicate slot. Operand-bearing values are never
/// exposed through `Debug`, traces, or error text.
#[derive(Clone)]
pub struct CustomFilterSlot {
    pub field_id: uuid::Uuid,
    pub field_type: &'static str,
    pub operation: &'static str,
    pub text_operand: Option<String>,
    pub number_min: Option<String>,
    pub number_max: Option<String>,
    pub date_min: Option<NaiveDate>,
    pub date_max: Option<NaiveDate>,
    pub option_ids: Option<Vec<uuid::Uuid>>,
}

impl fmt::Debug for CustomFilterSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomFilterSlot")
            .field("field_id", &self.field_id)
            .field("field_type", &self.field_type)
            .field("operation", &self.operation)
            .field("has_text_operand", &self.text_operand.is_some())
            .field("has_number_min", &self.number_min.is_some())
            .field("has_number_max", &self.number_max.is_some())
            .field("has_date_min", &self.date_min.is_some())
            .field("has_date_max", &self.date_max.is_some())
            .field("option_count", &self.option_ids.as_ref().map(Vec::len))
            .finish()
    }
}

impl fmt::Debug for PersonFilterParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PersonFilterParams")
            .field("stage_count", &self.stage_ids.as_ref().map(Vec::len))
            .field("custom_slots", &self.custom_slots)
            .field("viewer_id", &self.viewer_id)
            .finish_non_exhaustive()
    }
}

fn custom_slot(clause: &Clause) -> Option<CustomFilterSlot> {
    let slot = |field_id: CustomFieldId,
                field_type: FieldType,
                operation,
                text_operand,
                number_min,
                number_max,
                date_min,
                date_max,
                option_ids| {
        CustomFilterSlot {
            field_id: field_id.0,
            field_type: field_type.as_str(),
            operation,
            text_operand,
            number_min,
            number_max,
            date_min,
            date_max,
            option_ids,
        }
    };
    match clause {
        Clause::CustomText(c) => match &c.test {
            TextTest::IsSet => Some(slot(
                c.field_id,
                FieldType::Text,
                "is_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            TextTest::IsNotSet => Some(slot(
                c.field_id,
                FieldType::Text,
                "is_not_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            TextTest::Contains { text } => Some(slot(
                c.field_id,
                FieldType::Text,
                "contains",
                Some(text.clone()),
                None,
                None,
                None,
                None,
                None,
            )),
            TextTest::NotContains { text } => Some(slot(
                c.field_id,
                FieldType::Text,
                "not_contains",
                Some(text.clone()),
                None,
                None,
                None,
                None,
                None,
            )),
        },
        Clause::CustomNumber(c) => match &c.test {
            NumberTest::IsSet => Some(slot(
                c.field_id,
                FieldType::Number,
                "is_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            NumberTest::IsNotSet => Some(slot(
                c.field_id,
                FieldType::Number,
                "is_not_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            NumberTest::Range { min, max } => Some(slot(
                c.field_id,
                FieldType::Number,
                "range",
                None,
                min.clone(),
                max.clone(),
                None,
                None,
                None,
            )),
            NumberTest::NotRange { min, max } => Some(slot(
                c.field_id,
                FieldType::Number,
                "not_range",
                None,
                min.clone(),
                max.clone(),
                None,
                None,
                None,
            )),
        },
        Clause::CustomDate(c) => match &c.test {
            DateTest::IsSet => Some(slot(
                c.field_id,
                FieldType::Date,
                "is_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            DateTest::IsNotSet => Some(slot(
                c.field_id,
                FieldType::Date,
                "is_not_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            DateTest::Range { min, max } => Some(slot(
                c.field_id,
                FieldType::Date,
                "range",
                None,
                None,
                None,
                *min,
                *max,
                None,
            )),
            DateTest::NotRange { min, max } => Some(slot(
                c.field_id,
                FieldType::Date,
                "not_range",
                None,
                None,
                None,
                *min,
                *max,
                None,
            )),
        },
        Clause::CustomChoice(c) => match &c.test {
            ChoiceTest::IsSet => Some(slot(
                c.field_id,
                FieldType::Choice,
                "is_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            ChoiceTest::IsNotSet => Some(slot(
                c.field_id,
                FieldType::Choice,
                "is_not_set",
                None,
                None,
                None,
                None,
                None,
                None,
            )),
            ChoiceTest::AnyOf { option_ids } => Some(slot(
                c.field_id,
                FieldType::Choice,
                "any_of",
                None,
                None,
                None,
                None,
                None,
                Some(option_ids.iter().map(|id| id.0).collect()),
            )),
            ChoiceTest::NoneOf { option_ids } => Some(slot(
                c.field_id,
                FieldType::Choice,
                "none_of",
                None,
                None,
                None,
                None,
                None,
                Some(option_ids.iter().map(|id| id.0).collect()),
            )),
        },
        _ => None,
    }
}

/// A lossless `i32` conversion for a `days` value that has already been
/// through [`FilterDefinition::validate`] (bounded `1..=3650`) — never
/// reachable out of range TODAY, but `to_query_params` takes no `Result`
/// and 011b will add a second, saved-list caller of this same conversion
/// that may not re-run `validate` immediately before it. `as i32` would
/// silently wrap an out-of-range `i64` (including into a NEGATIVE number,
/// which `make_interval(days => …)` would then treat as a valid but
/// nonsensical cutoff — wrong, not a crash, and easy to miss). This fails
/// loud in debug/test builds (`debug_assert!`, matching the coordinator's
/// review note) and degrades safely in release rather than wrapping.
fn clamp_days_to_i32(days: i64) -> i32 {
    debug_assert!(
        (MIN_DAYS..=MAX_DAYS).contains(&days),
        "bind_age called with an out-of-range day count ({days}) — validate() must run first"
    );
    i32::try_from(days).unwrap_or(i32::MAX)
}

fn bind_age(spec: &AgeSpec) -> (Option<i32>, Option<i32>, Option<bool>) {
    match spec {
        AgeSpec::WithinDays(days) => (Some(clamp_days_to_i32(*days)), None, None),
        AgeSpec::NotWithinDays(days) => (None, Some(clamp_days_to_i32(*days)), None),
        AgeSpec::Never => (None, None, Some(true)),
    }
}

impl FilterDefinition {
    /// Converts a validated filter into bound SQL parameters, resolving
    /// `me` to `viewer` (§4c: appended to the bound user array AFTER
    /// validation, never a wire value reaching SQL as a token). Must only
    /// be called after both [`validate`](Self::validate) and
    /// [`validate_references`](Self::validate_references) have succeeded.
    pub fn to_query_params(&self, viewer: UserId) -> PersonFilterParams {
        let mut params = PersonFilterParams {
            viewer_id: viewer.0,
            ..PersonFilterParams::default()
        };
        let mut custom_slot_index = 0usize;
        for clause in &self.clauses {
            if let Some(slot) = custom_slot(clause) {
                // Structural validation enforces the five-slot cap. Keep a
                // defensive bound here so an accidentally unvalidated call
                // cannot panic or displace an existing predicate.
                if let Some(target) = params.custom_slots.get_mut(custom_slot_index) {
                    *target = Some(slot);
                    custom_slot_index += 1;
                }
                continue;
            }
            match clause {
                Clause::Stage(c) => {
                    params.stage_ids = Some(c.stage_ids.iter().map(|id| id.0).collect());
                }
                Clause::AssignedTo(c) => {
                    let mut include_unassigned = false;
                    let mut ids = Vec::new();
                    for a in &c.assignees {
                        match a {
                            Assignee::Me => ids.push(viewer.0),
                            Assignee::Unassigned => include_unassigned = true,
                            Assignee::User(id) => ids.push(id.0),
                        }
                    }
                    params.assigned_user_ids = Some(ids);
                    params.assigned_include_unassigned = Some(include_unassigned);
                }
                Clause::Source(c) => {
                    params.sources = Some(c.sources.clone());
                }
                Clause::Created(c) => {
                    let (w, nw, n) = bind_age(&c.age);
                    params.created_within_days = w;
                    params.created_not_within_days = nw;
                    params.created_never = n;
                }
                Clause::LastInquiry(c) => {
                    let (w, nw, n) = bind_age(&c.age);
                    params.last_inquiry_within_days = w;
                    params.last_inquiry_not_within_days = nw;
                    params.last_inquiry_never = n;
                }
                Clause::LastContact(c) => {
                    let (w, nw, n) = bind_age(&c.age);
                    params.last_contact_within_days = w;
                    params.last_contact_not_within_days = nw;
                    params.last_contact_never = n;
                }
                Clause::LastInbound(c) => {
                    let (w, nw, n) = bind_age(&c.age);
                    params.last_inbound_within_days = w;
                    params.last_inbound_not_within_days = nw;
                    params.last_inbound_never = n;
                }
                Clause::HasReplied(c) => params.has_replied = Some(c.value),
                Clause::HasPhone(c) => params.has_phone = Some(c.value),
                Clause::HasEmail(c) => params.has_email = Some(c.value),
                Clause::AwaitingResponse(c) => params.awaiting_response = Some(c.value),
                Clause::ClientRepliedUnanswered(c) => {
                    params.client_replied_unanswered = Some(c.value)
                }
                Clause::AwaitingCallOutcome(c) => params.awaiting_call_outcome = Some(c.value),
                Clause::Tags(c) => {
                    params.tag_ids_any = Some(c.tag_ids.iter().map(|id| id.0).collect());
                }
                Clause::NotTags(c) => {
                    params.tag_ids_none = Some(c.tag_ids.iter().map(|id| id.0).collect());
                }
                Clause::CustomText(_)
                | Clause::CustomNumber(_)
                | Clause::CustomDate(_)
                | Clause::CustomChoice(_) => unreachable!("custom clauses are handled above"),
            }
        }
        params
    }
}

// --- describe() (§4d) -------------------------------------------------------

/// Pre-resolved name maps for [`FilterDefinition::describe`], kept DB-free
/// and unit-testable by taking names rather than looking them up (§4d).
#[derive(Clone, Default)]
pub struct FilterNames {
    pub stage_names: std::collections::HashMap<StageId, String>,
    pub user_names: std::collections::HashMap<UserId, String>,
    /// docs/specs/SLICE_011e.md §4d.
    pub tag_names: std::collections::HashMap<TagId, String>,
    pub custom_field_names: HashMap<CustomFieldId, String>,
    pub custom_option_names: HashMap<(CustomFieldId, CustomFieldOptionId), String>,
}

impl fmt::Debug for FilterNames {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FilterNames")
            .field("stage_name_count", &self.stage_names.len())
            .field("user_name_count", &self.user_names.len())
            .field("tag_name_count", &self.tag_names.len())
            .field("custom_field_name_count", &self.custom_field_names.len())
            .field("custom_option_name_count", &self.custom_option_names.len())
            .finish()
    }
}

fn join_or(items: Vec<String>) -> String {
    items.join(" or ")
}

fn age_label(axis: &str, never_phrase: &str, age: &AgeSpec) -> String {
    match age {
        AgeSpec::WithinDays(days) => format!("{axis} within the last {days} days"),
        AgeSpec::NotWithinDays(days) => {
            format!("{axis} not within the last {days} days (or never)")
        }
        AgeSpec::Never => never_phrase.to_string(),
    }
}

fn custom_field_label(names: &FilterNames, field_id: CustomFieldId) -> String {
    names
        .custom_field_names
        .get(&field_id)
        .cloned()
        .unwrap_or_else(|| "an unavailable custom field".to_string())
}

fn custom_presence_label(field: String, is_set: bool) -> String {
    if is_set {
        format!("{field} is set")
    } else {
        format!("{field} is empty")
    }
}

impl FilterDefinition {
    /// One human-readable line per clause, in clause order (§4d). Ships
    /// unit-tested but unwired to any HTTP surface this slice (§4d, §10o).
    pub fn describe(&self, names: &FilterNames) -> Vec<String> {
        self.clauses
            .iter()
            .map(|clause| match clause {
                Clause::Stage(c) => {
                    let labels = c
                        .stage_ids
                        .iter()
                        .map(|id| {
                            names
                                .stage_names
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| "an unknown stage".to_string())
                        })
                        .collect();
                    format!("Stage is {}", join_or(labels))
                }
                Clause::AssignedTo(c) => {
                    let labels = c
                        .assignees
                        .iter()
                        .map(|a| match a {
                            Assignee::Me => "me".to_string(),
                            Assignee::Unassigned => "unassigned".to_string(),
                            Assignee::User(id) => names
                                .user_names
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| "an unknown person".to_string()),
                        })
                        .collect();
                    format!("Assigned to {}", join_or(labels))
                }
                Clause::Source(c) => format!("Source is {}", join_or(c.sources.clone())),
                Clause::Created(c) => age_label("Created", "Created never (unreachable)", &c.age),
                Clause::LastInquiry(c) => age_label("Last inquiry", "Never inquired", &c.age),
                Clause::LastContact(c) => age_label("Last contact", "Never contacted", &c.age),
                Clause::LastInbound(c) => age_label(
                    "Last inbound message",
                    "Never received an inbound message",
                    &c.age,
                ),
                Clause::HasReplied(c) => {
                    if c.value {
                        "Has replied".to_string()
                    } else {
                        "Has not replied".to_string()
                    }
                }
                Clause::HasPhone(c) => {
                    if c.value {
                        "Has a phone number".to_string()
                    } else {
                        "No phone number".to_string()
                    }
                }
                Clause::HasEmail(c) => {
                    if c.value {
                        "Has an email address".to_string()
                    } else {
                        "No email address".to_string()
                    }
                }
                Clause::AwaitingResponse(c) => {
                    if c.value {
                        "Awaiting a response".to_string()
                    } else {
                        "Not awaiting a response".to_string()
                    }
                }
                Clause::ClientRepliedUnanswered(c) => {
                    if c.value {
                        "Client replied, unanswered".to_string()
                    } else {
                        "No unanswered client reply".to_string()
                    }
                }
                Clause::AwaitingCallOutcome(c) => {
                    if c.value {
                        "A call of mine needs an outcome".to_string()
                    } else {
                        "No call of mine needs an outcome".to_string()
                    }
                }
                Clause::Tags(c) => {
                    let labels = c
                        .tag_ids
                        .iter()
                        .map(|id| {
                            names
                                .tag_names
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| "an unknown tag".to_string())
                        })
                        .collect();
                    format!("Tagged {}", join_or(labels))
                }
                Clause::NotTags(c) => {
                    let labels = c
                        .tag_ids
                        .iter()
                        .map(|id| {
                            names
                                .tag_names
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| "an unknown tag".to_string())
                        })
                        .collect();
                    format!("Not tagged {}", join_or(labels))
                }
                Clause::CustomText(c) => match &c.test {
                    TextTest::IsSet => {
                        custom_presence_label(custom_field_label(names, c.field_id), true)
                    }
                    TextTest::IsNotSet => {
                        custom_presence_label(custom_field_label(names, c.field_id), false)
                    }
                    TextTest::Contains { text } => {
                        format!("{} contains {text}", custom_field_label(names, c.field_id))
                    }
                    TextTest::NotContains { text } => format!(
                        "{} does not contain {text} (or empty)",
                        custom_field_label(names, c.field_id)
                    ),
                },
                Clause::CustomNumber(c) => {
                    describe_custom_range(&custom_field_label(names, c.field_id), &c.test)
                }
                Clause::CustomDate(c) => {
                    describe_custom_date_range(&custom_field_label(names, c.field_id), &c.test)
                }
                Clause::CustomChoice(c) => match &c.test {
                    ChoiceTest::IsSet => {
                        custom_presence_label(custom_field_label(names, c.field_id), true)
                    }
                    ChoiceTest::IsNotSet => {
                        custom_presence_label(custom_field_label(names, c.field_id), false)
                    }
                    ChoiceTest::AnyOf { option_ids } | ChoiceTest::NoneOf { option_ids } => {
                        let choices = option_ids
                            .iter()
                            .map(|id| {
                                names
                                    .custom_option_names
                                    .get(&(c.field_id, *id))
                                    .cloned()
                                    .unwrap_or_else(|| "an unavailable option".to_string())
                            })
                            .collect();
                        if matches!(c.test, ChoiceTest::AnyOf { .. }) {
                            format!(
                                "{} is {}",
                                custom_field_label(names, c.field_id),
                                join_or(choices)
                            )
                        } else {
                            format!(
                                "{} is not {} (or empty)",
                                custom_field_label(names, c.field_id),
                                join_or(choices)
                            )
                        }
                    }
                },
            })
            .collect()
    }
}

fn range_phrase(min: Option<&str>, max: Option<&str>) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!("between {min} and {max}"),
        (Some(min), None) => format!("at least {min}"),
        (None, Some(max)) => format!("at most {max}"),
        (None, None) => "in an invalid range".to_string(),
    }
}

fn describe_custom_range(field: &str, test: &NumberTest) -> String {
    match test {
        NumberTest::IsSet => custom_presence_label(field.to_string(), true),
        NumberTest::IsNotSet => custom_presence_label(field.to_string(), false),
        NumberTest::Range { min, max } => format!(
            "{field} is {}",
            range_phrase(min.as_deref(), max.as_deref())
        ),
        NumberTest::NotRange { min, max } => format!(
            "{field} is not {} (or empty)",
            range_phrase(min.as_deref(), max.as_deref())
        ),
    }
}

fn describe_custom_date_range(field: &str, test: &DateTest) -> String {
    let value = |date: &NaiveDate| date.format("%Y-%m-%d").to_string();
    match test {
        DateTest::IsSet => custom_presence_label(field.to_string(), true),
        DateTest::IsNotSet => custom_presence_label(field.to_string(), false),
        DateTest::Range { min, max } => format!(
            "{field} is {}",
            range_phrase(
                min.as_ref().map(value).as_deref(),
                max.as_ref().map(value).as_deref()
            )
        ),
        DateTest::NotRange { min, max } => format!(
            "{field} is not {} (or empty)",
            range_phrase(
                min.as_ref().map(value).as_deref(),
                max.as_ref().map(value).as_deref()
            )
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn parse(json: &str) -> Result<FilterDefinition, serde_json::Error> {
        serde_json::from_str(json)
    }

    // --- Unit test 1: wire round-trip + fail-closed decode --------------

    #[test]
    fn round_trips_every_clause_kind_and_age_op() {
        let stage_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let json = serde_json::json!({
            "version": 1,
            "clauses": [
                {"kind": "stage", "stage_ids": [stage_id]},
                {"kind": "assigned_to", "assignees": ["me", "unassigned", {"user_id": user_id}]},
                {"kind": "source", "sources": ["zillow", "website"]},
                {"kind": "created", "age": {"op": "within_days", "days": 30}},
                {"kind": "last_inquiry", "age": {"op": "not_within_days", "days": 7}},
                {"kind": "last_contact", "age": {"op": "never"}},
                {"kind": "last_inbound", "age": {"op": "within_days", "days": 14}},
                {"kind": "has_replied", "value": true},
                {"kind": "has_phone", "value": true},
                {"kind": "has_email", "value": false},
            ]
        })
        .to_string();

        let parsed: FilterDefinition = parse(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.clauses.len(), 10);
        assert_eq!(
            parsed.clauses[1],
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![
                    Assignee::Me,
                    Assignee::Unassigned,
                    Assignee::User(UserId::new(user_id))
                ]
            })
        );
        assert_eq!(
            parsed.clauses[5],
            Clause::LastContact(AgeClause {
                age: AgeSpec::Never
            })
        );

        // Round-trip: serialize back, re-parse, compare.
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: FilterDefinition = parse(&serialized).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn unknown_kind_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"bogus","x":1}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_age_op_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"created","age":{"op":"bogus","days":5}}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_version_fails_closed() {
        let json = r#"{"version":2,"clauses":[]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn custom_field_clauses_round_trip_and_bind_in_wire_order() {
        let text = Uuid::new_v4();
        let number = Uuid::new_v4();
        let choice = Uuid::new_v4();
        let option = Uuid::new_v4();
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"custom_text","field_id":"{text}","test":{{"op":"not_contains","text":"A%_\\\\B"}}}},{{"kind":"custom_number","field_id":"{number}","test":{{"op":"range","min":"-1.2500","max":"12.50"}}}},{{"kind":"custom_choice","field_id":"{choice}","test":{{"op":"none_of","option_ids":["{option}"]}}}}]}}"#
        );
        let filter = parse(&json).unwrap();
        filter.validate().unwrap();
        let params = filter.to_query_params(UserId::new(Uuid::new_v4()));
        assert_eq!(params.custom_slot(0).unwrap().field_id, text);
        assert_eq!(params.custom_slot(0).unwrap().operation, "not_contains");
        assert_eq!(
            params.custom_slot(1).unwrap().number_max.as_deref(),
            Some("12.50")
        );
        assert_eq!(
            params.custom_slot(2).unwrap().option_ids.as_ref().unwrap(),
            &vec![option]
        );
        assert!(params.custom_slot(3).is_none());
        assert_eq!(
            parse(&serde_json::to_string(&filter).unwrap()).unwrap(),
            filter
        );
    }

    #[test]
    fn custom_field_structural_limits_and_invalid_operands_fail_closed() {
        let field = Uuid::new_v4();
        for test in [
            r#"{"op":"range"}"#,
            r#"{"op":"range","min":"12.00001"}"#,
            r#"{"op":"range","min":"2","max":"1"}"#,
            r#"{"op":"contains","text":" trimmed "}"#,
            r#"{"op":"is_set","text":"nope"}"#,
        ] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"custom_number","field_id":"{field}","test":{test}}}]}}"#
            );
            let parsed = parse(&json);
            assert!(
                parsed.is_err() || parsed.unwrap().validate().is_err(),
                "{test}"
            );
        }
        let repeated = format!(
            r#"{{"version":1,"clauses":[{{"kind":"custom_text","field_id":"{field}","test":{{"op":"is_set"}}}},{{"kind":"custom_date","field_id":"{field}","test":{{"op":"is_not_set"}}}}]}}"#
        );
        assert!(parse(&repeated).unwrap().validate().is_err());
    }

    #[test]
    fn custom_wire_shape_rejects_null_duplicate_and_noncanonical_operands() {
        let field = Uuid::new_v4();
        let option = Uuid::new_v4();
        for raw_test in [
            r#"{"op":"is_set","text":null}"#,
            r#"{"op":"contains","text":null}"#,
            r#"{"op":"range","min":null,"max":null}"#,
            r#"{"op":"any_of","option_ids":[]}"#,
            r#"{"op":"any_of","option_ids":["not-a-uuid"]}"#,
            r#"{"op":"contains","text":"line\nbreak"}"#,
        ] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"custom_text","field_id":"{field}","test":{raw_test}}}]}}"#
            );
            assert!(
                parse(&json).is_err() || parse(&json).unwrap().validate().is_err(),
                "{raw_test}"
            );
        }
        let duplicate_op = format!(
            r#"{{"version":1,"clauses":[{{"kind":"custom_choice","field_id":"{field}","test":{{"op":"any_of","op":"none_of","option_ids":["{option}"]}}}}]}}"#
        );
        assert!(parse(&duplicate_op).is_err());
    }

    #[test]
    fn custom_slot_and_choice_caps_are_exact() {
        let field_ids: Vec<_> = (0..6).map(|_| Uuid::new_v4()).collect();
        let five = FilterDefinition {
            version: 1,
            clauses: field_ids[..5]
                .iter()
                .map(|id| {
                    Clause::CustomText(CustomTextClause {
                        field_id: CustomFieldId::new(*id),
                        test: TextTest::IsSet,
                    })
                })
                .collect(),
        };
        assert!(five.validate().is_ok());
        let six = FilterDefinition {
            version: 1,
            clauses: field_ids
                .iter()
                .map(|id| {
                    Clause::CustomText(CustomTextClause {
                        field_id: CustomFieldId::new(*id),
                        test: TextTest::IsSet,
                    })
                })
                .collect(),
        };
        assert!(matches!(six.validate(), Err(FilterError::Malformed)));

        for count in [0, 50, 51] {
            let options = (0..count)
                .map(|_| CustomFieldOptionId::new(Uuid::new_v4()))
                .collect();
            let filter = FilterDefinition {
                version: 1,
                clauses: vec![Clause::CustomChoice(CustomChoiceClause {
                    field_id: CustomFieldId::new(Uuid::new_v4()),
                    test: ChoiceTest::AnyOf {
                        option_ids: options,
                    },
                })],
            };
            assert_eq!(filter.validate().is_ok(), count == 50, "count {count}");
        }
    }

    #[test]
    fn custom_descriptions_resolve_bounded_names_and_keep_negative_absence_explicit() {
        let field = CustomFieldId::new(Uuid::new_v4());
        let option = CustomFieldOptionId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::CustomText(CustomTextClause {
                    field_id: field,
                    test: TextTest::NotContains {
                        text: "literal".to_string(),
                    },
                }),
                Clause::CustomChoice(CustomChoiceClause {
                    field_id: field,
                    test: ChoiceTest::NoneOf {
                        option_ids: vec![option],
                    },
                }),
            ],
        };
        let mut names = FilterNames::default();
        names
            .custom_field_names
            .insert(field, "Referrer".to_string());
        names
            .custom_option_names
            .insert((field, option), "Past client".to_string());
        assert_eq!(
            filter.describe(&names),
            vec![
                "Referrer does not contain literal (or empty)",
                "Referrer is not Past client (or empty)",
            ]
        );
    }

    #[test]
    fn unknown_assignee_token_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"assigned_to","assignees":["bob"]}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_field_at_top_level_fails_closed() {
        let json = r#"{"version":1,"clauses":[],"extra":true}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_field_inside_a_clause_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"stage","stage_ids":["00000000-0000-0000-0000-000000000001"],"extra":1}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_field_inside_an_age_spec_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"created","age":{"op":"within_days","days":5,"extra":1}}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn never_with_extra_days_field_fails_closed() {
        let json =
            r#"{"version":1,"clauses":[{"kind":"last_contact","age":{"op":"never","days":5}}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn unknown_field_inside_an_assignee_object_fails_closed() {
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"assigned_to","assignees":[{{"user_id":"{}","extra":1}}]}}]}}"#,
            Uuid::new_v4()
        );
        assert!(parse(&json).is_err());
    }

    #[test]
    fn missing_kind_fails_closed() {
        let json =
            r#"{"version":1,"clauses":[{"stage_ids":["00000000-0000-0000-0000-000000000001"]}]}"#;
        assert!(parse(json).is_err());
    }

    // --- Unit test 2: validation matrix ----------------------------------

    fn stage_clause(n: usize) -> Clause {
        Clause::Stage(StageClause {
            stage_ids: (0..n).map(|_| StageId::new(Uuid::new_v4())).collect(),
        })
    }

    #[test]
    fn more_than_twenty_clauses_is_malformed() {
        // `clauses.len() > MAX_CLAUSES` is checked before the per-clause
        // loop (M12 cosmetic fix: the previous version of this test
        // believed duplicate-kind detection would "trip first" and worked
        // around that with an alternating has_replied/has_phone vec — but
        // the length cap is unconditional and runs first regardless of
        // content, so 21 identical clauses already exercises exactly the
        // cap, nothing else).
        let clauses: Vec<Clause> = (0..21).map(|_| stage_clause(1)).collect();
        let filter = FilterDefinition {
            version: 1,
            clauses,
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn more_than_fifty_values_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![stage_clause(51)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn empty_value_array_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![stage_clause(0)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn duplicate_value_within_an_array_is_malformed() {
        let id = StageId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Stage(StageClause {
                stage_ids: vec![id, id],
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn duplicate_clause_kind_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![stage_clause(1), stage_clause(1)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn days_zero_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(0),
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn days_3651_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(3651),
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn days_bounds_are_inclusive() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(1),
            })],
        };
        assert!(filter.validate().is_ok());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(3650),
            })],
        };
        assert!(filter.validate().is_ok());
    }

    #[test]
    fn never_on_created_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Created(AgeClause {
                age: AgeSpec::Never,
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn never_is_allowed_on_the_other_three_age_axes() {
        for clause in [
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::Never,
            }),
        ] {
            let filter = FilterDefinition {
                version: 1,
                clauses: vec![clause],
            };
            assert!(filter.validate().is_ok());
        }
    }

    #[test]
    fn malformed_source_shape_is_rejected_at_decode_not_validate() {
        // A source with disallowed characters at all is fine to represent
        // in the type (it's just a String); rejection happens in validate().
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec!["not a valid source!".to_string()],
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn non_canonical_source_with_whitespace_is_rejected_never_normalized() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec![" Zillow ".to_string()],
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn non_canonical_source_with_uppercase_is_rejected_never_normalized() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec!["ZILLOW".to_string()],
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn well_formed_but_stale_source_passes_structural_validation() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec!["a_source_nobody_used_in_months".to_string()],
            })],
        };
        assert!(filter.validate().is_ok());
    }

    #[test]
    fn empty_clauses_is_valid() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![],
        };
        assert!(filter.validate().is_ok());
    }

    // --- Unit test 3: describe() -----------------------------------------

    #[test]
    fn describe_stage_joins_multiple_names_and_placeholders_unknown_ids() {
        let known = StageId::new(Uuid::new_v4());
        let unknown = StageId::new(Uuid::new_v4());
        let mut names = FilterNames::default();
        names.stage_names.insert(known, "Lead".to_string());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Stage(StageClause {
                stage_ids: vec![known, unknown],
            })],
        };
        assert_eq!(
            filter.describe(&names),
            vec!["Stage is Lead or an unknown stage".to_string()]
        );
    }

    #[test]
    fn describe_stage_two_names() {
        let lead = StageId::new(Uuid::new_v4());
        let hot = StageId::new(Uuid::new_v4());
        let mut names = FilterNames::default();
        names.stage_names.insert(lead, "Lead".to_string());
        names.stage_names.insert(hot, "Hot Prospect".to_string());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Stage(StageClause {
                stage_ids: vec![lead, hot],
            })],
        };
        assert_eq!(
            filter.describe(&names),
            vec!["Stage is Lead or Hot Prospect".to_string()]
        );
    }

    #[test]
    fn describe_assigned_to_me_and_unassigned() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::Unassigned],
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Assigned to me or unassigned".to_string()]
        );
    }

    #[test]
    fn describe_assigned_to_unknown_user_placeholder() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::User(UserId::new(Uuid::new_v4()))],
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Assigned to an unknown person".to_string()]
        );
    }

    #[test]
    fn describe_source_joins() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec!["zillow".to_string(), "website".to_string()],
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Source is zillow or website".to_string()]
        );
    }

    #[test]
    fn describe_created_within_days() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Created(AgeClause {
                age: AgeSpec::WithinDays(30),
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Created within the last 30 days".to_string()]
        );
    }

    #[test]
    fn describe_last_contact_not_within_days_carries_or_never_suffix() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastContact(AgeClause {
                age: AgeSpec::NotWithinDays(7),
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Last contact not within the last 7 days (or never)".to_string()]
        );
    }

    #[test]
    fn describe_last_contact_never() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastContact(AgeClause {
                age: AgeSpec::Never,
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Never contacted".to_string()]
        );
    }

    #[test]
    fn describe_last_inquiry_never() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::Never,
            })],
        };
        assert_eq!(
            filter.describe(&FilterNames::default()),
            vec!["Never inquired".to_string()]
        );
    }

    #[test]
    fn describe_last_inbound_within_and_never() {
        let within = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInbound(AgeClause {
                age: AgeSpec::WithinDays(14),
            })],
        };
        assert_eq!(
            within.describe(&FilterNames::default()),
            vec!["Last inbound message within the last 14 days".to_string()]
        );
        let never = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInbound(AgeClause {
                age: AgeSpec::Never,
            })],
        };
        assert_eq!(
            never.describe(&FilterNames::default()),
            vec!["Never received an inbound message".to_string()]
        );
    }

    #[test]
    fn describe_has_replied_true_and_false() {
        let t = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasReplied(BoolClause { value: true })],
        };
        assert_eq!(
            t.describe(&FilterNames::default()),
            vec!["Has replied".to_string()]
        );
        let f = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasReplied(BoolClause { value: false })],
        };
        assert_eq!(
            f.describe(&FilterNames::default()),
            vec!["Has not replied".to_string()]
        );
    }

    #[test]
    fn describe_has_phone_true_and_false() {
        let t = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasPhone(BoolClause { value: true })],
        };
        assert_eq!(
            t.describe(&FilterNames::default()),
            vec!["Has a phone number".to_string()]
        );
        let f = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasPhone(BoolClause { value: false })],
        };
        assert_eq!(
            f.describe(&FilterNames::default()),
            vec!["No phone number".to_string()]
        );
    }

    #[test]
    fn describe_has_email_true_and_false() {
        let t = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasEmail(BoolClause { value: true })],
        };
        assert_eq!(
            t.describe(&FilterNames::default()),
            vec!["Has an email address".to_string()]
        );
        let f = FilterDefinition {
            version: 1,
            clauses: vec![Clause::HasEmail(BoolClause { value: false })],
        };
        assert_eq!(
            f.describe(&FilterNames::default()),
            vec!["No email address".to_string()]
        );
    }

    // --- to_query_params ---------------------------------------------------

    #[test]
    fn to_query_params_resolves_me_and_binds_unassigned_as_empty_nonnull_array() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Unassigned],
            })],
        };
        let viewer = UserId::new(Uuid::new_v4());
        let params = filter.to_query_params(viewer);
        assert_eq!(params.assigned_user_ids, Some(vec![]));
        assert_eq!(params.assigned_include_unassigned, Some(true));
    }

    #[test]
    fn to_query_params_appends_me_to_the_bound_user_array() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            })],
        };
        let viewer = UserId::new(Uuid::new_v4());
        let params = filter.to_query_params(viewer);
        assert_eq!(params.assigned_user_ids, Some(vec![viewer.0]));
        assert_eq!(params.assigned_include_unassigned, Some(false));
    }

    #[test]
    fn to_query_params_leaves_axis_params_null_when_clause_absent() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![],
        };
        let params = filter.to_query_params(UserId::new(Uuid::new_v4()));
        assert_eq!(params.stage_ids, None);
        assert_eq!(params.assigned_user_ids, None);
        assert_eq!(params.assigned_include_unassigned, None);
        assert_eq!(params.sources, None);
        assert_eq!(params.has_replied, None);
    }

    #[test]
    fn kinds_field_is_comma_joined_in_clause_order() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::HasReplied(BoolClause { value: true }),
                Clause::HasPhone(BoolClause { value: true }),
            ],
        };
        assert_eq!(filter.kinds_field(), "has_replied,has_phone");
    }

    // --- R3: duplicate JSON keys fail closed, even below the top level ---
    //
    // `json!{}` builds via a Rust map literal and can't itself express a
    // textual duplicate key, so these use raw string literals — the exact
    // wire bytes a client could actually send.

    #[test]
    fn duplicate_kind_key_fails_closed() {
        let stage_id = Uuid::new_v4();
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"bogus","kind":"stage","stage_ids":["{stage_id}"]}}]}}"#
        );
        assert!(
            parse(&json).is_err(),
            "a duplicate \"kind\" key must never let an unknown kind hide behind a later valid one"
        );
    }

    #[test]
    fn duplicate_stage_ids_key_fails_closed() {
        let stage_id = Uuid::new_v4();
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"stage","stage_ids":["{stage_id}"],"stage_ids":["{stage_id}"]}}]}}"#
        );
        assert!(parse(&json).is_err());
    }

    #[test]
    fn duplicate_op_key_in_age_spec_fails_closed() {
        let json = r#"{"version":1,"clauses":[{"kind":"created","age":{"op":"never","op":"within_days","days":5}}]}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn duplicate_user_id_key_in_assignee_object_fails_closed() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"assigned_to","assignees":[{{"user_id":"{a}","user_id":"{b}"}}]}}]}}"#
        );
        assert!(parse(&json).is_err());
    }

    // --- R3: canonical-uuid-only wire values (amended §4b) ---------------

    #[test]
    fn non_canonical_stage_id_forms_are_rejected_canonical_accepted() {
        let id = Uuid::new_v4();
        let canonical = id.to_string();
        let simple = id.simple().to_string();
        let braced = format!("{{{canonical}}}");
        let urn = format!("urn:uuid:{canonical}");
        let upper = canonical.to_uppercase();

        for bad in [simple, braced, urn, upper] {
            let json =
                format!(r#"{{"version":1,"clauses":[{{"kind":"stage","stage_ids":["{bad}"]}}]}}"#);
            assert!(parse(&json).is_err(), "expected rejection of {bad:?}");
        }

        let good = format!(
            r#"{{"version":1,"clauses":[{{"kind":"stage","stage_ids":["{canonical}"]}}]}}"#
        );
        assert!(parse(&good).is_ok(), "canonical form must be accepted");
    }

    #[test]
    fn non_canonical_assignee_user_id_forms_are_rejected_canonical_accepted() {
        let id = Uuid::new_v4();
        let canonical = id.to_string();
        let upper = canonical.to_uppercase();
        let simple = id.simple().to_string();
        let braced = format!("{{{canonical}}}");
        let urn = format!("urn:uuid:{canonical}");

        for bad in [upper, simple, braced, urn] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"assigned_to","assignees":[{{"user_id":"{bad}"}}]}}]}}"#
            );
            assert!(parse(&json).is_err(), "expected rejection of {bad:?}");
        }

        let good = format!(
            r#"{{"version":1,"clauses":[{{"kind":"assigned_to","assignees":[{{"user_id":"{canonical}"}}]}}]}}"#
        );
        assert!(parse(&good).is_ok());
    }

    // --- M6: parser robustness (currently-sound, now pinned) --------------

    #[test]
    fn deeply_nested_json_is_a_decode_error_not_a_panic() {
        // ~200 levels of nested arrays where a clause object is expected —
        // serde_json's own recursion guard must reject this (never panic)
        // regardless of what this module's manual Deserialize impls do.
        let nested = "[".repeat(200) + &"]".repeat(200);
        let json = format!(r#"{{"version":1,"clauses":{nested}}}"#);
        assert!(parse(&json).is_err());
    }

    fn parse_and_validate(json: &str) -> Result<(), String> {
        let filter: FilterDefinition = serde_json::from_str(json).map_err(|e| e.to_string())?;
        filter.validate().map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    #[test]
    fn non_integer_days_values_are_rejected_end_to_end() {
        // Disclosed nuance (see the implementation report): 7.5/7.0/1e2 are
        // all lexed as JSON floats and rejected at DECODE
        // (`Number::as_i64()` is `None` for a float-lexed literal,
        // regardless of value); "-0" has no decimal point or exponent, so
        // it lexes as the plain integer 0 and decodes fine — it is instead
        // caught by validate()'s `days >= 1` bound. Both are decode+validate
        // pipeline failures either way (400 `malformed_request` either
        // way), which is what this end-to-end helper actually pins.
        for days_literal in ["7.5", "7.0", "1e2", "-0"] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"last_inquiry","age":{{"op":"within_days","days":{days_literal}}}}}]}}"#
            );
            assert!(
                parse_and_validate(&json).is_err(),
                "days: {days_literal} must be rejected by decode+validate"
            );
        }
    }

    #[test]
    fn i64_max_and_a_value_above_i32_range_are_rejected_by_validate() {
        // Pins that the `as-i32` cast `bind_age` used to do is unreachable
        // on any VALIDATED filter — both values decode fine (still valid
        // i64 integers) and are caught by the day-count bound, not by
        // integer-width overflow.
        for days_literal in [i64::MAX.to_string(), "4294967296".to_string()] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"last_inquiry","age":{{"op":"within_days","days":{days_literal}}}}}]}}"#
            );
            let filter: FilterDefinition =
                parse(&json).expect("decodes fine — still a valid i64 integer");
            assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
        }
    }

    #[test]
    #[should_panic(expected = "out-of-range day count")]
    fn bind_age_debug_asserts_on_an_out_of_range_days_value_that_bypassed_validate() {
        // 011b's future second caller of `to_query_params` is the scenario
        // this guards: a days value that reaches `bind_age` WITHOUT having
        // gone through `validate()` first must fail loudly in debug/test
        // builds rather than silently wrapping via `as i32`.
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(i64::MAX),
            })],
        };
        let _ = filter.to_query_params(UserId::new(Uuid::new_v4()));
    }

    // --- 011d: derived clause kinds (docs/specs/SLICE_011d.md §2) --------

    #[test]
    fn round_trips_the_three_derived_clause_kinds() {
        let json = serde_json::json!({
            "version": 1,
            "clauses": [
                {"kind": "awaiting_response", "value": true},
                {"kind": "client_replied_unanswered", "value": false},
                {"kind": "awaiting_call_outcome", "value": true},
            ]
        })
        .to_string();
        let parsed: FilterDefinition = parse(&json).unwrap();
        assert_eq!(parsed.clauses.len(), 3);
        assert_eq!(
            parsed.clauses[0],
            Clause::AwaitingResponse(BoolClause { value: true })
        );
        assert_eq!(
            parsed.clauses[1],
            Clause::ClientRepliedUnanswered(BoolClause { value: false })
        );
        assert_eq!(
            parsed.clauses[2],
            Clause::AwaitingCallOutcome(BoolClause { value: true })
        );
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: FilterDefinition = parse(&serialized).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn derived_clause_kind_labels() {
        assert_eq!(
            Clause::AwaitingResponse(BoolClause { value: true }).kind_label(),
            "awaiting_response"
        );
        assert_eq!(
            Clause::ClientRepliedUnanswered(BoolClause { value: true }).kind_label(),
            "client_replied_unanswered"
        );
        assert_eq!(
            Clause::AwaitingCallOutcome(BoolClause { value: true }).kind_label(),
            "awaiting_call_outcome"
        );
    }

    #[test]
    fn derived_clauses_validate_with_no_extra_rules() {
        for clause in [
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::ClientRepliedUnanswered(BoolClause { value: false }),
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
        ] {
            let filter = FilterDefinition {
                version: 1,
                clauses: vec![clause],
            };
            assert!(filter.validate().is_ok());
        }
    }

    #[test]
    fn derived_clauses_cannot_duplicate_kind_with_each_other_axis() {
        // A duplicate of ITS OWN kind is still malformed, same as every
        // other clause kind (§4b one-per-kind rule) — this is not a special
        // case, just a pin that the new kinds went through the same
        // `seen_kinds` check.
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AwaitingResponse(BoolClause { value: true }),
                Clause::AwaitingResponse(BoolClause { value: false }),
            ],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn describe_derived_clauses_true_and_false() {
        let names = FilterNames::default();
        let cases: [(Clause, &str); 6] = [
            (
                Clause::AwaitingResponse(BoolClause { value: true }),
                "Awaiting a response",
            ),
            (
                Clause::AwaitingResponse(BoolClause { value: false }),
                "Not awaiting a response",
            ),
            (
                Clause::ClientRepliedUnanswered(BoolClause { value: true }),
                "Client replied, unanswered",
            ),
            (
                Clause::ClientRepliedUnanswered(BoolClause { value: false }),
                "No unanswered client reply",
            ),
            (
                Clause::AwaitingCallOutcome(BoolClause { value: true }),
                "A call of mine needs an outcome",
            ),
            (
                Clause::AwaitingCallOutcome(BoolClause { value: false }),
                "No call of mine needs an outcome",
            ),
        ];
        for (clause, expected) in cases {
            let filter = FilterDefinition {
                version: 1,
                clauses: vec![clause],
            };
            assert_eq!(filter.describe(&names), vec![expected.to_string()]);
        }
    }

    #[test]
    fn to_query_params_binds_the_three_derived_axes_and_always_binds_viewer_id() {
        let viewer = UserId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AwaitingResponse(BoolClause { value: true }),
                Clause::AwaitingCallOutcome(BoolClause { value: true }),
            ],
        };
        let params = filter.to_query_params(viewer);
        assert_eq!(params.awaiting_response, Some(true));
        assert_eq!(params.client_replied_unanswered, None);
        assert_eq!(params.awaiting_call_outcome, Some(true));
        assert_eq!(params.viewer_id, viewer.0);

        // Absent clauses: viewer_id is STILL bound (§2 — "always set",
        // inert because every reader of it is itself NULL-guarded on
        // awaiting_call_outcome).
        let empty = FilterDefinition {
            version: 1,
            clauses: vec![],
        };
        let params = empty.to_query_params(viewer);
        assert_eq!(params.awaiting_response, None);
        assert_eq!(params.client_replied_unanswered, None);
        assert_eq!(params.awaiting_call_outcome, None);
        assert_eq!(params.viewer_id, viewer.0);
    }

    // --- M10: "me" alongside the viewer's own explicit {"user_id"} -------

    #[test]
    fn me_alongside_the_viewers_own_explicit_user_id_is_valid_and_binds_twice() {
        let viewer = UserId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::User(viewer)],
            })],
        };
        assert!(
            filter.validate().is_ok(),
            "not a duplicate at the type level — Me != User(id) until resolved"
        );
        let params = filter.to_query_params(viewer);
        assert_eq!(
            params.assigned_user_ids,
            Some(vec![viewer.0, viewer.0]),
            "harmless: `= ANY(...)` ignores duplicate array entries"
        );
    }

    // --- 011e: `tags` / `not_tags` (docs/specs/SLICE_011e.md §4) ---------

    #[test]
    fn round_trips_tags_and_not_tags() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let json = serde_json::json!({
            "version": 1,
            "clauses": [
                {"kind": "tags", "tag_ids": [a, b]},
                {"kind": "not_tags", "tag_ids": [a]},
            ]
        })
        .to_string();
        let parsed: FilterDefinition = parse(&json).unwrap();
        assert_eq!(parsed.clauses.len(), 2);
        assert_eq!(
            parsed.clauses[0],
            Clause::Tags(TagIdsClause {
                tag_ids: vec![TagId::new(a), TagId::new(b)]
            })
        );
        assert_eq!(
            parsed.clauses[1],
            Clause::NotTags(TagIdsClause {
                tag_ids: vec![TagId::new(a)]
            })
        );
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: FilterDefinition = parse(&serialized).unwrap();
        assert_eq!(parsed, reparsed);
        // Review round 1, tester L5: `PartialEq` alone would not catch a
        // serializer regression that reordered fields or changed a tag
        // clause's wire shape without changing its parsed meaning (e.g. a
        // `Vec` vs `BTreeSet` swap that still parses to the same
        // `Clause::Tags`/`NotTags` value) -- re-serializing the reparsed
        // value must produce byte-for-byte the same JSON as the first
        // round-trip.
        assert_eq!(
            serialized,
            serde_json::to_string(&reparsed).unwrap(),
            "re-serializing the round-tripped value must be byte-stable"
        );
    }

    #[test]
    fn tags_and_not_tags_kind_labels() {
        assert_eq!(
            Clause::Tags(TagIdsClause {
                tag_ids: vec![TagId::new(Uuid::new_v4())]
            })
            .kind_label(),
            "tags"
        );
        assert_eq!(
            Clause::NotTags(TagIdsClause {
                tag_ids: vec![TagId::new(Uuid::new_v4())]
            })
            .kind_label(),
            "not_tags"
        );
    }

    #[test]
    fn non_canonical_tag_id_fails_closed_at_decode() {
        let json = r#"{"version":1,"clauses":[{"kind":"tags","tag_ids":["00000000-0000-0000-0000-00000000000A"]}]}"#;
        assert!(parse(json).is_err(), "uppercase hex must be rejected");
        let json = r#"{"version":1,"clauses":[{"kind":"not_tags","tag_ids":["{00000000-0000-0000-0000-000000000001}"]}]}"#;
        assert!(parse(json).is_err(), "braced form must be rejected");
    }

    #[test]
    fn unknown_field_inside_a_tags_clause_fails_closed() {
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"tags","tag_ids":["{}"],"extra":1}}]}}"#,
            Uuid::new_v4()
        );
        assert!(parse(&json).is_err());
    }

    fn tags_clause(n: usize) -> Clause {
        Clause::Tags(TagIdsClause {
            tag_ids: (0..n).map(|_| TagId::new(Uuid::new_v4())).collect(),
        })
    }

    #[test]
    fn empty_tag_ids_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![tags_clause(0)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn more_than_fifty_tag_ids_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![tags_clause(51)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn duplicate_tag_id_within_tags_is_malformed() {
        let id = TagId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Tags(TagIdsClause {
                tag_ids: vec![id, id],
            })],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn duplicate_tags_clause_kind_is_malformed() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![tags_clause(1), tags_clause(1)],
        };
        assert!(matches!(filter.validate(), Err(FilterError::Malformed)));
    }

    #[test]
    fn tags_and_not_tags_together_is_valid_not_a_duplicate_kind() {
        let id = TagId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::Tags(TagIdsClause { tag_ids: vec![id] }),
                Clause::NotTags(TagIdsClause { tag_ids: vec![id] }),
            ],
        };
        assert!(filter.validate().is_ok());
    }

    #[test]
    fn describe_tags_joins_multiple_names_and_placeholders_unknown_ids() {
        let known = TagId::new(Uuid::new_v4());
        let unknown = TagId::new(Uuid::new_v4());
        let mut names = FilterNames::default();
        names.tag_names.insert(known, "Investor".to_string());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::Tags(TagIdsClause {
                tag_ids: vec![known, unknown],
            })],
        };
        assert_eq!(
            filter.describe(&names),
            vec!["Tagged Investor or an unknown tag".to_string()]
        );
    }

    #[test]
    fn describe_not_tags_single_name() {
        let known = TagId::new(Uuid::new_v4());
        let mut names = FilterNames::default();
        names.tag_names.insert(known, "Investor".to_string());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![Clause::NotTags(TagIdsClause {
                tag_ids: vec![known],
            })],
        };
        assert_eq!(
            filter.describe(&names),
            vec!["Not tagged Investor".to_string()]
        );
    }

    #[test]
    fn to_query_params_binds_tag_axes_and_leaves_them_null_when_absent() {
        let a = TagId::new(Uuid::new_v4());
        let b = TagId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::Tags(TagIdsClause {
                    tag_ids: vec![a, b],
                }),
                Clause::NotTags(TagIdsClause { tag_ids: vec![a] }),
            ],
        };
        let params = filter.to_query_params(UserId::new(Uuid::new_v4()));
        assert_eq!(params.tag_ids_any, Some(vec![a.0, b.0]));
        assert_eq!(params.tag_ids_none, Some(vec![a.0]));

        let empty = FilterDefinition {
            version: 1,
            clauses: vec![],
        };
        let params = empty.to_query_params(UserId::new(Uuid::new_v4()));
        assert_eq!(params.tag_ids_any, None);
        assert_eq!(params.tag_ids_none, None);
    }

    #[test]
    fn kinds_field_includes_tags_and_not_tags() {
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                tags_clause(1),
                Clause::NotTags(TagIdsClause {
                    tag_ids: vec![TagId::new(Uuid::new_v4())],
                }),
            ],
        };
        assert_eq!(filter.kinds_field(), "tags,not_tags");
    }

    #[test]
    fn one_sided_custom_ranges_round_trip_without_null_operands() {
        let number = CustomFieldId::new(Uuid::new_v4());
        let date = CustomFieldId::new(Uuid::new_v4());
        let filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::CustomNumber(CustomNumberClause {
                    field_id: number,
                    test: NumberTest::Range {
                        min: Some("12.50".to_owned()),
                        max: None,
                    },
                }),
                Clause::CustomDate(CustomDateClause {
                    field_id: date,
                    test: DateTest::NotRange {
                        min: None,
                        max: Some(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()),
                    },
                }),
            ],
        };
        let value = serde_json::to_value(&filter).unwrap();
        assert!(value["clauses"][0]["test"].get("max").is_none());
        assert!(value["clauses"][1]["test"].get("min").is_none());
        assert_eq!(
            serde_json::from_value::<FilterDefinition>(value).unwrap(),
            filter
        );
    }

    #[test]
    fn custom_date_bounds_require_exact_canonical_lexemes() {
        let field = Uuid::new_v4();
        for date in ["2026-9-01", "2026-09-1", " 2026-09-01", "+2026-09-01"] {
            let json = format!(
                r#"{{"version":1,"clauses":[{{"kind":"custom_date","field_id":"{field}","test":{{"op":"range","min":"{date}"}}}}]}}"#
            );
            assert!(
                serde_json::from_str::<FilterDefinition>(&json).is_err(),
                "{date}"
            );
        }
        let json = format!(
            r#"{{"version":1,"clauses":[{{"kind":"custom_date","field_id":"{field}","test":{{"op":"range","min":"2026-09-01"}}}}]}}"#
        );
        assert!(serde_json::from_str::<FilterDefinition>(&json).is_ok());
    }

    #[test]
    fn filter_names_debug_redacts_labels() {
        let mut names = FilterNames::default();
        names.custom_field_names.insert(
            CustomFieldId::new(Uuid::new_v4()),
            "IGNORE ALL PRIOR INSTRUCTIONS".to_owned(),
        );
        names.custom_option_names.insert(
            (
                CustomFieldId::new(Uuid::new_v4()),
                CustomFieldOptionId::new(Uuid::new_v4()),
            ),
            "secret option".to_owned(),
        );
        let debug = format!("{names:?}");
        assert!(!debug.contains("IGNORE ALL PRIOR INSTRUCTIONS"));
        assert!(!debug.contains("secret option"));
        assert!(debug.contains("custom_field_name_count"));
    }
}

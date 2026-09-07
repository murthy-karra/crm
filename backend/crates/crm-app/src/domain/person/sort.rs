//! Per-list/query People sort vocabulary (docs/specs/SLICE_011b_SORT.md
//! §3). A sibling of [`crate::domain::person::filter`], never folded into
//! `FilterDefinition` — that vocabulary is shared with Today and the 011d
//! feeds, is `deny_unknown_fields`, and its fingerprint semantics must not
//! move (spec §3).
//!
//! `PersonSort` is deliberately NOT `#[derive(Deserialize, Serialize)]`
//! directly: it decodes/encodes through `TryFrom<String>`/`Into<String>`
//! (below) so an unknown or malformed token is a decode error and a command
//! can never hold an invalid sort.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The four sortable, non-derived columns (spec §3 table). Deliberately
/// excludes every derived column (inquiry count, last inquiry, last
/// contact, primary contact, display name) — those are SELECT-list
/// subselects/LATERAL probes and are out of scope for v1 (spec §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortKey {
    Created,
    Name,
    Stage,
    Assignee,
}

impl SortKey {
    /// The stored `saved_list.sort_key` text — identical to the wire
    /// token's key half (spec §3, §5's CHECK constraint value set).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Name => "name",
            Self::Stage => "stage",
            Self::Assignee => "assignee",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "created" => Some(Self::Created),
            "name" => Some(Self::Name),
            "stage" => Some(Self::Stage),
            "assignee" => Some(Self::Assignee),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    /// The stored `saved_list.sort_direction` text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "asc" => Some(Self::Asc),
            "desc" => Some(Self::Desc),
            _ => None,
        }
    }
}

/// A `key`/`direction` pair. The wire/storage form is the lowercase dotted
/// token `"<key>.<direction>"` (e.g. `"name.asc"`) — exactly the eight
/// combinations below, no others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PersonSort {
    pub key: SortKey,
    pub direction: SortDirection,
}

/// An unknown, malformed, or wrong-case sort token (spec §3, §11.1:
/// `NAME.asc`, `name`, `name.`, `name.up`, `inquiry_count.asc`, empty and
/// whitespace are all rejected — lowercase only, no derived columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortParseError;

impl fmt::Display for SortParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid person sort token")
    }
}

impl std::error::Error for SortParseError {}

impl PersonSort {
    /// `created.desc` — today's implicit order, unchanged by this slice
    /// when no sort is requested (spec §3, §4).
    pub const DEFAULT: PersonSort = PersonSort {
        key: SortKey::Created,
        direction: SortDirection::Desc,
    };

    /// Parses the exact lowercase dotted wire/storage token. No trimming,
    /// no case-folding — the token must already be canonical (mirrors
    /// `filter.rs`'s `validate_source`'s "already canonical" discipline).
    pub fn parse(token: &str) -> Result<Self, SortParseError> {
        let mut parts = token.split('.');
        let (Some(key_part), Some(direction_part), None) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err(SortParseError);
        };
        let key = match key_part {
            "created" => SortKey::Created,
            "name" => SortKey::Name,
            "stage" => SortKey::Stage,
            "assignee" => SortKey::Assignee,
            _ => return Err(SortParseError),
        };
        let direction = match direction_part {
            "asc" => SortDirection::Asc,
            "desc" => SortDirection::Desc,
            _ => return Err(SortParseError),
        };
        Ok(PersonSort { key, direction })
    }

    /// The canonical lowercase dotted wire/storage token.
    pub fn token(self) -> &'static str {
        match (self.key, self.direction) {
            (SortKey::Created, SortDirection::Asc) => "created.asc",
            (SortKey::Created, SortDirection::Desc) => "created.desc",
            (SortKey::Name, SortDirection::Asc) => "name.asc",
            (SortKey::Name, SortDirection::Desc) => "name.desc",
            (SortKey::Stage, SortDirection::Asc) => "stage.asc",
            (SortKey::Stage, SortDirection::Desc) => "stage.desc",
            (SortKey::Assignee, SortDirection::Asc) => "assignee.asc",
            (SortKey::Assignee, SortDirection::Desc) => "assignee.desc",
        }
    }

    /// `None` for [`DEFAULT`](Self::DEFAULT), `Some(self)` otherwise — the
    /// normalization every persistence/fingerprint/query-dispatch rule
    /// applies before storage or execution (spec §4, §5).
    pub fn normalized(self) -> Option<Self> {
        if self == Self::DEFAULT {
            None
        } else {
            Some(self)
        }
    }

    /// The `(sort_key, sort_direction)` pair a NORMALIZED sort (i.e.
    /// already run through [`normalized`](Self::normalized)) is stored as —
    /// `(None, None)` for the default order, else both columns set
    /// (docs/specs/SLICE_011b_SORT.md §5).
    pub fn storage_columns(sort: Option<Self>) -> (Option<&'static str>, Option<&'static str>) {
        match sort {
            None => (None, None),
            Some(sort) => (Some(sort.key.as_str()), Some(sort.direction.as_str())),
        }
    }

    /// Decodes a stored `saved_list.(sort_key, sort_direction)` column pair
    /// (docs/specs/SLICE_011b_SORT.md §5). The CHECK constraints block every
    /// other value for a normal write, but a binary must still fail closed
    /// on a pair it cannot read — possible only under migration or binary
    /// skew (spec §5) — exactly like an unknown filter clause, never as a
    /// silent default order.
    pub fn decode_stored(sort_key: Option<&str>, sort_direction: Option<&str>) -> SortDecodeResult {
        match (sort_key, sort_direction) {
            (None, None) => SortDecodeResult::Default,
            (Some(key), Some(direction)) => {
                match (SortKey::from_db(key), SortDirection::from_db(direction)) {
                    (Some(key), Some(direction)) => {
                        SortDecodeResult::Sort(PersonSort { key, direction })
                    }
                    _ => SortDecodeResult::Unreadable,
                }
            }
            // A half pair is blocked by `saved_list_sort_pair_check`; this
            // arm is defensive only (unreachable under the current schema).
            _ => SortDecodeResult::Unreadable,
        }
    }
}

/// The outcome of decoding a stored sort column pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDecodeResult {
    /// A `NULL` pair — the default order (`created.desc`).
    Default,
    /// A recognized, non-default stored sort.
    Sort(PersonSort),
    /// A non-`NULL` pair this binary cannot read. Fails closed exactly like
    /// an unknown filter clause (spec §5).
    Unreadable,
}

impl TryFrom<String> for PersonSort {
    type Error = SortParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<PersonSort> for String {
    fn from(value: PersonSort) -> Self {
        value.token().to_string()
    }
}

impl<'de> Deserialize<'de> for PersonSort {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        PersonSort::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl Serialize for PersonSort {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.token())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_TOKENS: [&str; 8] = [
        "created.asc",
        "created.desc",
        "name.asc",
        "name.desc",
        "stage.asc",
        "stage.desc",
        "assignee.asc",
        "assignee.desc",
    ];

    /// §11.1: all eight tokens round-trip through `parse`/`token`.
    #[test]
    fn all_eight_tokens_round_trip() {
        for token in ALL_TOKENS {
            let sort = PersonSort::parse(token).unwrap_or_else(|_| panic!("{token} must parse"));
            assert_eq!(sort.token(), token, "token round-trip for {token}");
        }
    }

    /// §11.1: all eight tokens round-trip through `TryFrom<String>`/
    /// `Into<String>` — the serde path.
    #[test]
    fn all_eight_tokens_round_trip_through_string_conversions() {
        for token in ALL_TOKENS {
            let sort = PersonSort::try_from(token.to_string())
                .unwrap_or_else(|_| panic!("{token} must convert"));
            let back: String = sort.into();
            assert_eq!(back, token);
        }
    }

    /// §11.1: rejected malformed/wrong-case/derived-column/empty/whitespace
    /// tokens — every one is a decode error, never a silent fallback.
    #[test]
    fn malformed_tokens_are_rejected() {
        for bad in [
            "NAME.asc",
            "Name.Asc",
            "name",
            "name.",
            ".asc",
            "name.up",
            "name.ascending",
            "inquiry_count.asc",
            "last_inquiry.asc",
            "last_contact.desc",
            "primary_contact.asc",
            "display_name.asc",
            "",
            " ",
            "created.desc.extra",
            "created .desc",
            "created. desc",
            " created.desc",
            "created.desc ",
        ] {
            assert!(PersonSort::parse(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    /// §11.1: `created.desc` normalizes to `None`; every other token
    /// normalizes to `Some(self)`.
    #[test]
    fn created_desc_normalizes_to_none_every_other_token_to_some_self() {
        assert_eq!(PersonSort::DEFAULT.normalized(), None);
        assert_eq!(
            PersonSort::parse("created.desc").unwrap().normalized(),
            None
        );
        for token in ALL_TOKENS {
            if token == "created.desc" {
                continue;
            }
            let sort = PersonSort::parse(token).unwrap();
            assert_eq!(
                sort.normalized(),
                Some(sort),
                "{token} must normalize to itself"
            );
        }
    }

    /// Serde: valid token decodes; invalid token is a decode error, not a
    /// silent default (mirrors `FilterDefinition`'s decode-error discipline
    /// — an invalid sort can never reach a typed command).
    #[test]
    fn serde_round_trips_and_rejects_invalid_json_strings() {
        let sort = PersonSort::parse("name.asc").unwrap();
        let json = serde_json::to_string(&sort).unwrap();
        assert_eq!(json, "\"name.asc\"");
        let decoded: PersonSort = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, sort);

        assert!(serde_json::from_str::<PersonSort>("\"NAME.asc\"").is_err());
        assert!(serde_json::from_str::<PersonSort>("\"\"").is_err());
        assert!(serde_json::from_str::<PersonSort>("null").is_err());
        assert!(serde_json::from_str::<PersonSort>("42").is_err());
    }

    /// §5: storage-column round trip for every normalized sort, and the
    /// `NULL, NULL` default pair.
    #[test]
    fn storage_columns_round_trip_through_decode_stored() {
        assert_eq!(PersonSort::storage_columns(None), (None, None));
        assert_eq!(
            PersonSort::decode_stored(None, None),
            SortDecodeResult::Default
        );
        for token in ALL_TOKENS {
            if token == "created.desc" {
                continue;
            }
            let sort = PersonSort::parse(token).unwrap();
            let (key, direction) = PersonSort::storage_columns(Some(sort));
            assert_eq!(
                PersonSort::decode_stored(key, direction),
                SortDecodeResult::Sort(sort),
                "{token} must decode back to itself"
            );
        }
    }

    /// §5: a stored pair this binary cannot read fails closed — a
    /// recognized key with a garbage direction, a garbage key with a
    /// recognized direction, and an unrecognized pair altogether. A half
    /// pair (one `NULL`, one not) is blocked by the DB's own
    /// `saved_list_sort_pair_check`, so this only exercises the two-non-NULL
    /// unrecognized case this binary must still defend against under
    /// migration/binary skew.
    #[test]
    fn unrecognized_stored_pairs_are_unreadable() {
        assert_eq!(
            PersonSort::decode_stored(Some("name"), Some("sideways")),
            SortDecodeResult::Unreadable
        );
        assert_eq!(
            PersonSort::decode_stored(Some("inquiry_count"), Some("asc")),
            SortDecodeResult::Unreadable
        );
        assert_eq!(
            PersonSort::decode_stored(Some("bogus"), Some("bogus")),
            SortDecodeResult::Unreadable
        );
    }
}

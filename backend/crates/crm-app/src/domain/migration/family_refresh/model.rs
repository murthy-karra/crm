//! D-092 closed policy model. Source-bearing values deliberately have no Debug.
//! A comparison is not write authority: persistence revalidates these proofs
//! under the workspace, family, identity and native-record locks at commit.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const ENGINE: &str = "fub-family-refresh-v1";
pub const PAGE_MAX: u16 = 50;
pub const RESPONSE_BYTES: usize = 512 * 1024;
pub const FIELD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Metadata,
    Activity,
    History,
}
impl Family {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Activity => "activity",
            Self::History => "history",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Metadata,
    Note,
    Task,
    Event,
    Call,
    Text,
}
impl Kind {
    pub const fn family(self) -> Family {
        match self {
            Self::Metadata => Family::Metadata,
            Self::Note | Self::Task => Family::Activity,
            Self::Event | Self::Call | Self::Text => Family::History,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hold {
    FirstCoverageRequired,
    BaselineUnproven,
    SourceNotObserved,
    SourceUnavailable,
    SourceConflict,
    SourceNotNewer,
    UnsupportedSource,
    TargetErased,
    IdentityMismatch,
    LocalChange,
    StaleHead,
    MappingRequired,
    TargetUnavailable,
    UnitTooLarge,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Immutable original/admitted result, or prior family-refresh result.
    pub result_id: Uuid,
    pub person_id: Uuid,
    pub target_id: Uuid,
    pub revision: i64,
    pub native: Value,
    /// True only for a positively proven migration-created link/value/record.
    /// Recognizing an equal existing value does not acquire its ownership.
    pub owned: bool,
    pub head_id: Option<Uuid>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Current {
    pub person_id: Uuid,
    pub target_id: Uuid,
    pub revision: i64,
    pub native: Value,
    pub deleted: bool,
    pub head_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Insert,
    Update,
    AlreadyCurrent,
    Held(Hold),
}

/// Qualified new identities may insert after initial family coverage. Existing
/// unowned or erased identities can never be adopted by value equality.
pub fn compare_native(
    baseline: Option<&Baseline>,
    current: Option<&Current>,
    desired: &Value,
    identity_consumed: bool,
    first_coverage: bool,
) -> Outcome {
    if !first_coverage {
        return Outcome::Held(Hold::FirstCoverageRequired);
    }
    let Some(b) = baseline else {
        return if identity_consumed || current.is_some() {
            Outcome::Held(Hold::BaselineUnproven)
        } else {
            Outcome::Insert
        };
    };
    let Some(c) = current else {
        return Outcome::Held(Hold::TargetErased);
    };
    if c.deleted {
        return Outcome::Held(Hold::TargetErased);
    }
    if b.person_id != c.person_id || b.target_id != c.target_id {
        return Outcome::Held(Hold::IdentityMismatch);
    }
    if b.head_id != c.head_id {
        return Outcome::Held(Hold::StaleHead);
    }
    // Check the revision before value equality. An edit-and-revert is still a
    // local mutation, even when it happens to equal the source's new value.
    if b.revision != c.revision || b.native != c.native {
        return Outcome::Held(Hold::LocalChange);
    }
    if c.native == *desired {
        Outcome::AlreadyCurrent
    } else if b.owned {
        Outcome::Update
    } else {
        Outcome::Held(Hold::BaselineUnproven)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagState {
    Complete,
    Missing,
    Null,
    Invalid,
}
/// All source aliases must be absent from a qualified complete tag list. An
/// already-present local tag, deleted target or unknown list is never removable.
pub fn removable_tag(
    state: TagState,
    aliases_still_present: bool,
    baseline: &Baseline,
    current: &Current,
) -> bool {
    state == TagState::Complete
        && !aliases_still_present
        && baseline.owned
        && compare_native(Some(baseline), Some(current), &Value::Null, true, true)
            == Outcome::Update
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryChange {
    New,
    AlreadyCurrent,
    Correction,
    Held(Hold),
}
/// Full canonical source equality matters even if displayed metadata is equal.
/// Target changes and same-capture variants are never fix-forward corrections.
pub fn compare_history(
    existing: Option<(&[u8], Uuid)>,
    desired_semantic: &[u8],
    desired_person: Uuid,
    source_is_newer: bool,
    has_variants: bool,
    erased: bool,
) -> HistoryChange {
    if erased {
        return HistoryChange::Held(Hold::TargetErased);
    }
    if has_variants {
        return HistoryChange::Held(Hold::SourceConflict);
    }
    if !source_is_newer {
        return HistoryChange::Held(Hold::SourceNotNewer);
    }
    let Some((semantic, person)) = existing else {
        return HistoryChange::New;
    };
    if person != desired_person {
        return HistoryChange::Held(Hold::IdentityMismatch);
    }
    if semantic == desired_semantic {
        HistoryChange::AlreadyCurrent
    } else {
        HistoryChange::Correction
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    #[serde(with = "decimal_count")]
    pub units: u64,
    #[serde(with = "decimal_count")]
    pub inserts: u64,
    #[serde(with = "decimal_count")]
    pub updates: u64,
    #[serde(with = "decimal_count")]
    pub already_current: u64,
    #[serde(with = "decimal_count")]
    pub held: u64,
    #[serde(with = "decimal_count")]
    pub excluded: u64,
    #[serde(with = "decimal_count")]
    pub tag_removals: u64,
    #[serde(with = "decimal_count")]
    pub field_clears: u64,
    #[serde(with = "decimal_count")]
    pub task_completions: u64,
    #[serde(with = "decimal_count")]
    pub task_reopens: u64,
    #[serde(with = "decimal_count")]
    pub history_corrections: u64,
    #[serde(with = "decimal_count")]
    pub source_only: u64,
}
impl Counts {
    /// Add one bounded unit without wrapping counts or losing wire precision.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        macro_rules! sum {
            ($($field:ident),+ $(,)?) => {
                Some(Self { $($field: self.$field.checked_add(other.$field).filter(|v| *v <= i64::MAX as u64)?,)+ })
            };
        }
        sum!(
            units,
            inserts,
            updates,
            already_current,
            held,
            excluded,
            tag_removals,
            field_clears,
            task_completions,
            task_reopens,
            history_corrections,
            source_only
        )
    }
    pub fn useful(&self) -> bool {
        self.inserts > 0
            || self.updates > 0
            || self.already_current > 0
            || self.history_corrections > 0
    }
    pub fn reconciles(&self) -> bool {
        [
            self.inserts,
            self.updates,
            self.already_current,
            self.held,
            self.excluded,
            self.history_corrections,
        ]
        .into_iter()
        .try_fold(0_u64, |total, count| total.checked_add(count))
            == Some(self.units)
    }
}

// Counts cross JavaScript clients without loss of precision. Numeric JSON and
// noncanonical forms are rejected rather than silently rounded/coerced.
mod decimal_count {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        if *value > i64::MAX as u64 {
            return Err(serde::ser::Error::custom("count out of range"));
        }
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 19
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(serde::de::Error::custom("invalid decimal count"));
        }
        value
            .parse::<i64>()
            .map(|value| value as u64)
            .map_err(|_| serde::de::Error::custom("count out of range"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn records() -> (Baseline, Current) {
        let b = Baseline {
            result_id: Uuid::new_v4(),
            person_id: Uuid::new_v4(),
            target_id: Uuid::new_v4(),
            revision: 1,
            native: json!({"value":500}),
            owned: true,
            head_id: None,
        };
        let c = Current {
            person_id: b.person_id,
            target_id: b.target_id,
            revision: b.revision,
            native: b.native.clone(),
            deleted: false,
            head_id: None,
        };
        (b, c)
    }
    #[test]
    fn edit_and_revert_and_equal_source_cannot_erase_local_mutation() {
        let (b, mut c) = records();
        c.revision += 1;
        assert_eq!(
            compare_native(Some(&b), Some(&c), &c.native, true, true),
            Outcome::Held(Hold::LocalChange)
        );
    }
    #[test]
    fn qualified_updates_require_positive_ownership_and_exact_baseline() {
        let (mut b, mut c) = records();
        let desired = json!({"value":550});
        assert_eq!(
            compare_native(Some(&b), Some(&c), &desired, true, true),
            Outcome::Update
        );
        b.owned = false;
        assert_eq!(
            compare_native(Some(&b), Some(&c), &desired, true, true),
            Outcome::Held(Hold::BaselineUnproven)
        );
        assert_eq!(
            compare_native(Some(&b), Some(&c), &c.native, true, true),
            Outcome::AlreadyCurrent
        );
        c.native = json!({"value":525});
        assert_eq!(
            compare_native(Some(&b), Some(&c), &desired, true, true),
            Outcome::Held(Hold::LocalChange)
        );
    }
    #[test]
    fn consumed_or_deleted_identity_never_becomes_new() {
        let (b, mut c) = records();
        assert_eq!(
            compare_native(None, None, &json!({}), true, true),
            Outcome::Held(Hold::BaselineUnproven)
        );
        assert_eq!(
            compare_native(Some(&b), None, &json!({}), true, true),
            Outcome::Held(Hold::TargetErased)
        );
        c.deleted = true;
        assert_eq!(
            compare_native(Some(&b), Some(&c), &json!({}), true, true),
            Outcome::Held(Hold::TargetErased)
        );
        assert_eq!(
            compare_native(None, None, &json!({}), false, true),
            Outcome::Insert
        );
        assert_eq!(
            compare_native(None, None, &json!({}), false, false),
            Outcome::Held(Hold::FirstCoverageRequired)
        );
    }
    #[test]
    fn newer_heads_and_other_people_cannot_be_reversed_by_remainder() {
        let (b, mut c) = records();
        c.head_id = Some(Uuid::new_v4());
        assert_eq!(
            compare_native(Some(&b), Some(&c), &json!({}), true, true),
            Outcome::Held(Hold::StaleHead)
        );
        c.person_id = Uuid::new_v4();
        assert_eq!(
            compare_native(Some(&b), Some(&c), &json!({}), true, true),
            Outcome::Held(Hold::IdentityMismatch)
        );
    }
    #[test]
    fn tag_absence_needs_complete_list_all_aliases_and_owned_unchanged_link() {
        let (mut b, c) = records();
        assert!(removable_tag(TagState::Complete, false, &b, &c));
        for state in [TagState::Missing, TagState::Null, TagState::Invalid] {
            assert!(!removable_tag(state, false, &b, &c));
        }
        assert!(!removable_tag(TagState::Complete, true, &b, &c));
        b.owned = false;
        assert!(!removable_tag(TagState::Complete, false, &b, &c));
    }
    #[test]
    fn history_versions_require_later_unambiguous_same_person_source() {
        let p = Uuid::new_v4();
        let old = [1; 32];
        let new = [2; 32];
        assert_eq!(
            compare_history(Some((&old, p)), &new, p, true, false, false),
            HistoryChange::Correction
        );
        assert_eq!(
            compare_history(Some((&old, p)), &old, p, true, false, false),
            HistoryChange::AlreadyCurrent
        );
        assert_eq!(
            compare_history(Some((&old, p)), &new, p, false, false, false),
            HistoryChange::Held(Hold::SourceNotNewer)
        );
        assert_eq!(
            compare_history(Some((&old, p)), &new, p, true, true, false),
            HistoryChange::Held(Hold::SourceConflict)
        );
        assert_eq!(
            compare_history(Some((&old, p)), &new, Uuid::new_v4(), true, false, false),
            HistoryChange::Held(Hold::IdentityMismatch)
        );
        assert_eq!(
            compare_history(None, &new, p, true, false, true),
            HistoryChange::Held(Hold::TargetErased)
        );
    }
    #[test]
    fn accumulating_counts_checks_every_action_and_wire_overflow() {
        let a = Counts {
            units: 1,
            inserts: 1,
            ..Counts::default()
        };
        let b = Counts {
            units: 1,
            history_corrections: 1,
            ..Counts::default()
        };
        let sum = a.checked_add(&b).unwrap();
        assert_eq!(sum.units, 2);
        assert_eq!(sum.inserts, 1);
        assert_eq!(sum.history_corrections, 1);
        assert!(sum.reconciles());
        assert!(Counts {
            units: i64::MAX as u64,
            ..Counts::default()
        }
        .checked_add(&a)
        .is_none());
        assert!(Counts {
            source_only: u64::MAX,
            ..Counts::default()
        }
        .checked_add(&a)
        .is_none());
    }
    #[test]
    fn count_wire_contract_is_exact_and_strict() {
        let counts = Counts {
            units: 9_007_199_254_740_993,
            held: 9_007_199_254_740_993,
            ..Counts::default()
        };
        let value = serde_json::to_value(&counts).unwrap();
        assert_eq!(value["units"], "9007199254740993");
        assert!(serde_json::from_value::<Counts>(value.clone())
            .unwrap()
            .reconciles());
        for bad in [
            json!(1),
            json!("01"),
            json!("-1"),
            json!("1e3"),
            json!("9223372036854775808"),
        ] {
            let mut altered = value.clone();
            altered["units"] = bad;
            assert!(serde_json::from_value::<Counts>(altered).is_err());
        }
        let mut altered = value;
        altered["unknown"] = json!("0");
        assert!(serde_json::from_value::<Counts>(altered).is_err());
        assert!(!Counts {
            units: 1,
            inserts: u64::MAX,
            updates: 2,
            ..Counts::default()
        }
        .reconciles());
    }
}

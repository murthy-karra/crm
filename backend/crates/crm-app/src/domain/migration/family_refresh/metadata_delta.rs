//! One atomic Person metadata proposal. Source qualification, catalog resolution
//! and immutable baseline reconstruction happen before this comparison; database
//! execution must recheck the same state/revision and catalog under its locks.
use super::model::{Counts, Hold};
use crate::domain::custom_field::{
    validate_date_range, validate_number_pattern, validate_text_value, CustomFieldValue, FieldType,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

type Alias = [u8; 32];
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub tags: BTreeSet<Uuid>,
    pub fields: BTreeMap<Uuid, CustomFieldValue>,
}
impl State {
    pub(super) fn matches(&self, other: &Self) -> bool {
        self.tags == other.tags
            && self.fields.len() == other.fields.len()
            && self.fields.iter().all(|(id, value)| {
                other
                    .fields
                    .get(id)
                    .is_some_and(|other| equal(value, other))
            })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub person: Uuid,
    pub revision: i64,
    pub head: Option<Uuid>,
    /// Complete native metadata, including unowned links and field values.
    pub state: State,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Ownership {
    /// Positive immutable insertion proof, plus every source alias supporting
    /// the link. Already-present native links never enter this map.
    pub tags: BTreeMap<Uuid, BTreeSet<Alias>>,
    pub fields: BTreeSet<Uuid>,
}
#[derive(Serialize, Deserialize)]
pub enum Tags {
    Missing,
    Null,
    Complete(Vec<(Alias, Uuid)>),
    Held(Hold),
}
#[derive(Serialize, Deserialize)]
pub enum FieldIntent {
    Missing,
    /// A source null without a qualified clear meaning is retained as unknown.
    UnknownNull,
    QualifiedClear,
    Set(CustomFieldValue),
    Held(Hold),
}
#[derive(Serialize, Deserialize)]
pub struct Source {
    pub tags: Tags,
    /// Vector preserves duplicate target claims so they cannot silently collapse
    /// into a map's last-writer-wins value.
    pub fields: Vec<(Uuid, FieldIntent)>,
}
pub struct Field {
    pub kind: FieldType,
    pub live_options: BTreeSet<Uuid>,
}
pub struct Catalog {
    pub live_tags: BTreeSet<Uuid>,
    pub live_fields: BTreeMap<Uuid, Field>,
}
#[derive(Serialize, Deserialize)]
pub enum Change {
    AddTag(Uuid),
    RemoveTag(Uuid),
    SetField(Uuid, CustomFieldValue),
    ClearField(Uuid),
}
#[derive(Serialize, Deserialize)]
pub enum Gap {
    MissingTags,
    NullTags,
    MissingField(Uuid),
    UnqualifiedNull(Uuid),
}
#[derive(Serialize, Deserialize)]
pub struct Proposal {
    pub person: Uuid,
    pub expected_revision: i64,
    pub expected_head: Option<Uuid>,
    pub changes: Vec<Change>,
    pub after: State,
    pub ownership: Ownership,
    pub counts: Counts,
    pub gaps: Vec<Gap>,
}
fn equal(a: &CustomFieldValue, b: &CustomFieldValue) -> bool {
    match (a, b) {
        (CustomFieldValue::Text(a), CustomFieldValue::Text(b))
        | (CustomFieldValue::Number(a), CustomFieldValue::Number(b)) => a == b,
        (CustomFieldValue::Date(a), CustomFieldValue::Date(b)) => a == b,
        (CustomFieldValue::Choice(a), CustomFieldValue::Choice(b)) => a == b,
        _ => false,
    }
}
fn validate(value: &CustomFieldValue, field: &Field) -> Result<(), Hold> {
    if value.field_type() != field.kind {
        return Err(Hold::MappingRequired);
    }
    let valid = match value {
        CustomFieldValue::Text(s) => validate_text_value(s).is_ok_and(|native| native == *s),
        CustomFieldValue::Number(s) => validate_number_pattern(s).is_ok(),
        CustomFieldValue::Date(d) => validate_date_range(*d).is_ok(),
        CustomFieldValue::Choice(id) => field.live_options.contains(&id.0),
    };
    valid.then_some(()).ok_or(Hold::UnsupportedSource)
}

/// Missing/erased baselines, any local edit (including edit-and-revert), or an
/// invalid sub-operation holds the entire Person. The caller publishes a hold
/// with zero destructive-action counts, never the partially built local result.
pub fn propose(
    baseline: Option<&Snapshot>,
    current: Option<&Snapshot>,
    ownership: &Ownership,
    source: &Source,
    catalog: &Catalog,
    first_coverage: bool,
) -> Result<Proposal, Hold> {
    if !first_coverage {
        return Err(Hold::FirstCoverageRequired);
    }
    let baseline = baseline.ok_or(Hold::BaselineUnproven)?;
    let current = current.ok_or(Hold::TargetErased)?;
    if baseline.person != current.person {
        return Err(Hold::IdentityMismatch);
    }
    if baseline.head != current.head {
        return Err(Hold::StaleHead);
    }
    if baseline.revision <= 0 || current.revision <= 0 {
        return Err(Hold::BaselineUnproven);
    }
    if baseline.revision != current.revision || !baseline.state.matches(&current.state) {
        return Err(Hold::LocalChange);
    }
    if ownership
        .tags
        .iter()
        .any(|(tag, aliases)| !baseline.state.tags.contains(tag) || aliases.is_empty())
        || ownership
            .fields
            .iter()
            .any(|field| !baseline.state.fields.contains_key(field))
    {
        return Err(Hold::BaselineUnproven);
    }
    let mut alias_owners = BTreeMap::new();
    for (tag, aliases) in &ownership.tags {
        for alias in aliases {
            if alias_owners
                .insert(*alias, *tag)
                .is_some_and(|old| old != *tag)
            {
                return Err(Hold::BaselineUnproven);
            }
        }
    }
    let mut observed = false;
    let mut result = Proposal {
        person: current.person,
        expected_revision: current.revision,
        expected_head: current.head,
        changes: vec![],
        gaps: vec![],
        after: current.state.clone(),
        ownership: ownership.clone(),
        counts: Counts {
            units: 1,
            ..Default::default()
        },
    };
    match &source.tags {
        Tags::Held(reason) => return Err(*reason),
        Tags::Missing => result.gaps.push(Gap::MissingTags),
        Tags::Null => result.gaps.push(Gap::NullTags),
        Tags::Complete(occurrences) => {
            observed = true;
            let mut aliases = BTreeMap::new();
            for (alias, tag) in occurrences {
                if aliases.insert(*alias, *tag).is_some_and(|old| old != *tag) {
                    return Err(Hold::SourceConflict);
                }
            }
            if aliases.values().any(|tag| !catalog.live_tags.contains(tag)) {
                return Err(Hold::TargetUnavailable);
            }
            let mut targets: BTreeMap<Uuid, BTreeSet<Alias>> = BTreeMap::new();
            for (alias, target) in &aliases {
                targets.entry(*target).or_default().insert(*alias);
            }
            for (tag, support) in &ownership.tags {
                // An existing source claim cannot be reassigned to another
                // catalog target merely by replacing its mapping in a refresh.
                if support
                    .iter()
                    .any(|alias| aliases.get(alias).is_some_and(|target| target != tag))
                {
                    return Err(Hold::MappingRequired);
                }
                if !targets.contains_key(tag)
                    && support.iter().all(|alias| !aliases.contains_key(alias))
                {
                    if !catalog.live_tags.contains(tag) {
                        return Err(Hold::TargetUnavailable);
                    }
                    result.after.tags.remove(tag);
                    result.ownership.tags.remove(tag);
                    result.changes.push(Change::RemoveTag(*tag));
                    result.counts.tag_removals += 1;
                }
            }
            for (tag, support) in targets {
                if result.after.tags.insert(tag) {
                    result.changes.push(Change::AddTag(tag));
                    result.ownership.tags.insert(tag, support);
                } else if let Some(owned) = result.ownership.tags.get_mut(&tag) {
                    // Remember all supporting aliases. Mere observation never
                    // acquires an already-present native link.
                    owned.extend(support);
                }
            }
            if result
                .changes
                .iter()
                .any(|c| matches!(c, Change::AddTag(_)))
                && result.after.tags.len() > crate::domain::tag::PERSON_TAG_LIMIT
            {
                return Err(Hold::UnitTooLarge);
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (id, intent) in &source.fields {
        if !seen.insert(*id) {
            return Err(Hold::SourceConflict);
        }
        match intent {
            FieldIntent::Held(reason) => return Err(*reason),
            FieldIntent::Missing => result.gaps.push(Gap::MissingField(*id)),
            FieldIntent::UnknownNull => result.gaps.push(Gap::UnqualifiedNull(*id)),
            FieldIntent::QualifiedClear => {
                observed = true;
                if !catalog.live_fields.contains_key(id) {
                    return Err(Hold::TargetUnavailable);
                }
                if current.state.fields.contains_key(id) {
                    if !ownership.fields.contains(id) {
                        return Err(Hold::BaselineUnproven);
                    }
                    result.after.fields.remove(id);
                    result.ownership.fields.remove(id);
                    result.changes.push(Change::ClearField(*id));
                    result.counts.field_clears += 1;
                }
            }
            FieldIntent::Set(value) => {
                observed = true;
                let field = catalog.live_fields.get(id).ok_or(Hold::TargetUnavailable)?;
                validate(value, field)?;
                if let Some(old) = current.state.fields.get(id) {
                    if equal(old, value) {
                        continue;
                    }
                    if !ownership.fields.contains(id) {
                        return Err(Hold::BaselineUnproven);
                    }
                }
                result.after.fields.insert(*id, value.clone());
                result.ownership.fields.insert(*id);
                result.changes.push(Change::SetField(*id, value.clone()));
            }
        }
    }
    if !observed {
        return Err(Hold::SourceNotObserved);
    }
    if result.changes.is_empty() {
        result.counts.already_current = 1;
    } else {
        result.counts.updates = 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Snapshot, Ownership, Catalog, Uuid, Uuid) {
        let owned = Uuid::new_v4();
        let local = Uuid::new_v4();
        let field = Uuid::new_v4();
        (
            Snapshot {
                person: Uuid::new_v4(),
                revision: 3,
                head: None,
                state: State {
                    tags: BTreeSet::from([owned, local]),
                    fields: BTreeMap::from([(field, CustomFieldValue::Text("before".into()))]),
                },
            },
            Ownership {
                tags: BTreeMap::from([(owned, BTreeSet::from([[1; 32], [2; 32]]))]),
                fields: BTreeSet::from([field]),
            },
            Catalog {
                live_tags: BTreeSet::from([owned, local]),
                live_fields: BTreeMap::from([(
                    field,
                    Field {
                        kind: FieldType::Text,
                        live_options: BTreeSet::new(),
                    },
                )]),
            },
            owned,
            field,
        )
    }
    fn empty() -> Source {
        Source {
            tags: Tags::Complete(vec![]),
            fields: vec![],
        }
    }
    #[test]
    fn removal_needs_all_aliases_absent_and_preserves_unowned_links() {
        let (b, ownership, catalog, owned, _) = fixture();
        let source = Source {
            tags: Tags::Complete(vec![([2; 32], owned)]),
            fields: vec![],
        };
        let kept = propose(Some(&b), Some(&b), &ownership, &source, &catalog, true).unwrap();
        assert!(kept.changes.is_empty());
        assert_eq!(kept.counts.already_current, 1);
        let removed = propose(Some(&b), Some(&b), &ownership, &empty(), &catalog, true).unwrap();
        assert_eq!(removed.after.tags.len(), 1);
        assert!(!removed.after.tags.contains(&owned));
        assert_eq!(removed.counts.tag_removals, 1);
        assert!(removed.counts.reconciles());
        assert!(removed.ownership.tags.is_empty());
        let serialized = serde_json::to_vec(&source).unwrap();
        let decoded: Source = serde_json::from_slice(&serialized).unwrap();
        assert!(propose(Some(&b), Some(&b), &ownership, &decoded, &catalog, true).is_ok());
        let encoded = serde_json::to_vec(&removed).unwrap();
        let decoded: Proposal = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.counts.tag_removals, 1);
    }
    #[test]
    fn equal_native_values_and_links_never_acquire_ownership() {
        let (b, _, catalog, _, field) = fixture();
        let source = Source {
            tags: Tags::Complete(
                b.state
                    .tags
                    .iter()
                    .enumerate()
                    .map(|(i, t)| ([i as u8; 32], *t))
                    .collect(),
            ),
            fields: vec![(
                field,
                FieldIntent::Set(CustomFieldValue::Text("before".into())),
            )],
        };
        let p = propose(
            Some(&b),
            Some(&b),
            &Ownership::default(),
            &source,
            &catalog,
            true,
        )
        .unwrap();
        assert!(p.ownership.tags.is_empty() && p.ownership.fields.is_empty());
        assert!(p.changes.is_empty());
        let clear = Source {
            tags: Tags::Missing,
            fields: vec![(field, FieldIntent::QualifiedClear)],
        };
        assert!(matches!(
            propose(
                Some(&b),
                Some(&b),
                &Ownership::default(),
                &clear,
                &catalog,
                true
            ),
            Err(Hold::BaselineUnproven)
        ));
    }
    #[test]
    fn local_edit_or_aba_holds_the_whole_person_even_if_source_matches() {
        let (b, o, c, _, field) = fixture();
        let mut current = b.clone();
        current.revision += 2;
        assert!(matches!(
            propose(Some(&b), Some(&current), &o, &empty(), &c, true),
            Err(Hold::LocalChange)
        ));
        current = b.clone();
        current
            .state
            .fields
            .insert(field, CustomFieldValue::Text("local".into()));
        let source = Source {
            tags: Tags::Complete(vec![]),
            fields: vec![(
                field,
                FieldIntent::Set(CustomFieldValue::Text("local".into())),
            )],
        };
        assert!(matches!(
            propose(Some(&b), Some(&current), &o, &source, &c, true),
            Err(Hold::LocalChange)
        ));
        assert_eq!(
            b.state.tags.len(),
            2,
            "no partial tag removals escaped a hold"
        );
        current = b.clone();
        current.head = Some(Uuid::new_v4());
        assert!(matches!(
            propose(Some(&b), Some(&current), &o, &empty(), &c, true),
            Err(Hold::StaleHead)
        ));
    }
    #[test]
    fn unknown_null_missing_and_empty_text_are_never_implicit_clears() {
        let (b, o, c, owned, field) = fixture();
        for intent in [FieldIntent::Missing, FieldIntent::UnknownNull] {
            let source = Source {
                tags: Tags::Complete(vec![([1; 32], owned)]),
                fields: vec![(field, intent)],
            };
            let p = propose(Some(&b), Some(&b), &o, &source, &c, true).unwrap();
            assert_eq!(p.counts.field_clears, 0);
            assert_eq!(p.gaps.len(), 1);
            assert!(p.after.fields.contains_key(&field));
        }
        let empty_text = Source {
            tags: Tags::Missing,
            fields: vec![(
                field,
                FieldIntent::Set(CustomFieldValue::Text(String::new())),
            )],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &empty_text, &c, true),
            Err(Hold::UnsupportedSource)
        ));
        let clear = Source {
            tags: Tags::Null,
            fields: vec![(field, FieldIntent::QualifiedClear)],
        };
        let p = propose(Some(&b), Some(&b), &o, &clear, &c, true).unwrap();
        assert!(p.after.tags.contains(&owned));
        assert!(!p.after.fields.contains_key(&field));
        assert_eq!(p.counts.field_clears, 1);
        assert_eq!(p.counts.updates, 1);
        assert_eq!(p.counts.tag_removals, 0);
        let unknown = Source {
            tags: Tags::Missing,
            fields: vec![(field, FieldIntent::Missing)],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &unknown, &c, true),
            Err(Hold::SourceNotObserved)
        ));
    }
    #[test]
    fn capacity_uses_final_set_and_applied_additions_get_positive_ownership() {
        let (mut b, mut o, mut c, owned, _) = fixture();
        while b.state.tags.len() < crate::domain::tag::PERSON_TAG_LIMIT {
            b.state.tags.insert(Uuid::new_v4());
        }
        c.live_tags = b.state.tags.clone();
        let added = Uuid::new_v4();
        c.live_tags.insert(added);
        let source = Source {
            tags: Tags::Complete(vec![([3; 32], added)]),
            fields: vec![],
        };
        let p = propose(Some(&b), Some(&b), &o, &source, &c, true).unwrap();
        assert_eq!(p.after.tags.len(), crate::domain::tag::PERSON_TAG_LIMIT);
        assert_eq!(p.counts.tag_removals, 1);
        assert!(p.ownership.tags.contains_key(&added));
        o.tags.remove(&owned);
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &source, &c, true),
            Err(Hold::UnitTooLarge)
        ));
    }
    #[test]
    fn conflicting_field_targets_and_moved_aliases_hold_without_partial_changes() {
        let (b, o, c, owned, field) = fixture();
        let duplicate = Source {
            tags: Tags::Complete(vec![]),
            fields: vec![
                (field, FieldIntent::QualifiedClear),
                (field, FieldIntent::Missing),
            ],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &duplicate, &c, true),
            Err(Hold::SourceConflict)
        ));
        let local = *b.state.tags.iter().find(|id| **id != owned).unwrap();
        let conflicted = Source {
            tags: Tags::Complete(vec![([1; 32], owned), ([1; 32], local)]),
            fields: vec![],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &conflicted, &c, true),
            Err(Hold::SourceConflict)
        ));
        let moved = Source {
            tags: Tags::Complete(vec![([1; 32], local)]),
            fields: vec![],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &moved, &c, true),
            Err(Hold::MappingRequired)
        ));
    }
    #[test]
    fn missing_coverage_baseline_and_erased_person_cannot_be_reconstructed_from_current() {
        let (b, o, c, _, _) = fixture();
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &empty(), &c, false),
            Err(Hold::FirstCoverageRequired)
        ));
        assert!(matches!(
            propose(None, Some(&b), &o, &empty(), &c, true),
            Err(Hold::BaselineUnproven)
        ));
        assert!(matches!(
            propose(Some(&b), None, &o, &empty(), &c, true),
            Err(Hold::TargetErased)
        ));
        let mut invalid = b.clone();
        invalid.revision = 0;
        assert!(matches!(
            propose(Some(&invalid), Some(&invalid), &o, &empty(), &c, true),
            Err(Hold::BaselineUnproven)
        ));
        let mut foreign = b.clone();
        foreign.person = Uuid::new_v4();
        assert!(matches!(
            propose(Some(&b), Some(&foreign), &o, &empty(), &c, true),
            Err(Hold::IdentityMismatch)
        ));
    }
    #[test]
    fn catalog_type_option_and_native_limits_are_rechecked_before_proposal() {
        let (b, o, mut c, _, field) = fixture();
        for value in [
            CustomFieldValue::Text("x".repeat(501)),
            CustomFieldValue::Number("1".into()),
        ] {
            let source = Source {
                tags: Tags::Missing,
                fields: vec![(field, FieldIntent::Set(value))],
            };
            assert!(propose(Some(&b), Some(&b), &o, &source, &c, true).is_err());
        }
        let choice = Uuid::new_v4();
        c.live_fields.insert(
            choice,
            Field {
                kind: FieldType::Choice,
                live_options: BTreeSet::new(),
            },
        );
        let source = Source {
            tags: Tags::Missing,
            fields: vec![(
                choice,
                FieldIntent::Set(CustomFieldValue::Choice(
                    crate::ids::CustomFieldOptionId::new(Uuid::new_v4()),
                )),
            )],
        };
        assert!(matches!(
            propose(Some(&b), Some(&b), &o, &source, &c, true),
            Err(Hold::UnsupportedSource)
        ));
    }
}

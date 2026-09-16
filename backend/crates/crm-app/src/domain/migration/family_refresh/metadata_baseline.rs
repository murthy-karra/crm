//! Encrypted, transaction-local metadata after-state. Current rows never create
//! ownership: only successful INSERT receipts can do that.
use super::{
    metadata_delta::{Ownership, Snapshot, State},
    model::Hold,
};
use crate::{domain::migration::MigrationError, ids::OrganizationId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct AfterState {
    version: u8,
    organization: Uuid,
    import: Uuid,
    manifest: Uuid,
    snapshot: Snapshot,
    ownership: Ownership,
}
/// Server-owned insertion receipts, assembled after the native operation settles.
pub(crate) struct Receipt<'a> {
    pub kind: &'a str,
    pub source_key: &'a [u8],
    pub target: Option<Uuid>,
    pub outcome: &'a str,
}
fn ownership(state: &State, receipts: &[Receipt<'_>]) -> Result<Ownership, MigrationError> {
    let mut owned = Ownership::default();
    let mut aliases = BTreeMap::<Uuid, BTreeSet<[u8; 32]>>::new();
    let mut targets = BTreeMap::new();
    for r in receipts {
        if !matches!(r.outcome, "applied" | "already_present") {
            continue;
        }
        let target = r.target.ok_or(MigrationError::Crypto)?;
        let alias: [u8; 32] = r
            .source_key
            .try_into()
            .map_err(|_| MigrationError::Crypto)?;
        if targets
            .insert((r.kind, alias), target)
            .is_some_and(|old| old != target)
        {
            return Err(MigrationError::Crypto);
        }
        match r.kind {
            "tag_link" if state.tags.contains(&target) => {
                aliases.entry(target).or_default().insert(alias);
                if r.outcome == "applied" {
                    owned.tags.entry(target).or_default();
                }
            }
            "value" if state.fields.contains_key(&target) => {
                if r.outcome == "applied" {
                    owned.fields.insert(target);
                }
            }
            _ => return Err(MigrationError::Crypto),
        }
    }
    for (target, support) in &mut owned.tags {
        *support = aliases.remove(target).ok_or(MigrationError::Crypto)?;
    }
    Ok(owned)
}
/// Caller holds the Organization metadata namespace. Insertion transactions also
/// retain the Person revision lock acquired by the native mutation triggers.
/// Includes archived catalog values as well as live ones: hidden local data must
/// still participate in the full-state/revision comparison.
pub(crate) async fn observe(
    conn: &mut PgConnection,
    org: OrganizationId,
    person: Uuid,
) -> Result<Snapshot, MigrationError> {
    let value: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('person',p.id,'revision',p.metadata_revision,'head',NULL,'state',jsonb_build_object('tags',COALESCE((SELECT jsonb_agg(t.tag_id ORDER BY t.tag_id) FROM person_tag t WHERE t.organization_id=p.organization_id AND t.person_id=p.id),'[]'::jsonb),'fields',COALESCE((SELECT jsonb_object_agg(v.field_id,CASE v.field_type WHEN 'text' THEN jsonb_build_object('text',v.text_value) WHEN 'number' THEN jsonb_build_object('number',v.number_value::text) WHEN 'date' THEN jsonb_build_object('date',v.date_value) WHEN 'choice' THEN jsonb_build_object('option_id',v.option_id) END) FROM person_custom_field_value v WHERE v.organization_id=p.organization_id AND v.person_id=p.id),'{}'::jsonb))) FROM person p WHERE p.organization_id=$1 AND p.id=$2")
        .bind(org.0).bind(person).fetch_one(conn).await?;
    serde_json::from_value(value).map_err(|_| MigrationError::Crypto)
}
pub(crate) async fn capture(
    conn: &mut PgConnection,
    org: OrganizationId,
    import: Uuid,
    manifest: Uuid,
    person: Uuid,
    receipts: &[Receipt<'_>],
) -> Result<AfterState, MigrationError> {
    let snapshot = observe(conn, org, person).await?;
    let ownership = ownership(&snapshot.state, receipts)?;
    Ok(AfterState {
        version: 1,
        organization: org.0,
        import,
        manifest,
        snapshot,
        ownership,
    })
}
/// Relational identifiers come from the scoped successful result and manifest,
/// after the encrypted result has been authenticated. No request may supply them.
pub struct Binding {
    pub organization: OrganizationId,
    pub import: Uuid,
    pub manifest: Uuid,
    pub person: Uuid,
}
pub fn verify(
    proof: Option<&AfterState>,
    binding: &Binding,
    current: Option<&Snapshot>,
) -> Result<(Snapshot, Ownership), Hold> {
    let proof = proof.ok_or(Hold::BaselineUnproven)?;
    if proof.version != 1
        || proof.organization != binding.organization.0
        || proof.import != binding.import
        || proof.manifest != binding.manifest
        || proof.snapshot.person != binding.person
        || proof.snapshot.revision <= 0
        || proof.snapshot.head.is_some()
    {
        return Err(Hold::BaselineUnproven);
    }
    let current = current.ok_or(Hold::TargetErased)?;
    if current.person != binding.person {
        return Err(Hold::IdentityMismatch);
    }
    if current.head.is_some() {
        return Err(Hold::StaleHead);
    }
    if proof.snapshot.revision != current.revision || !proof.snapshot.state.matches(&current.state)
    {
        return Err(Hold::LocalChange);
    }
    verify_ownership(&proof.snapshot, &proof.ownership)?;
    Ok((proof.snapshot.clone(), proof.ownership.clone()))
}

pub(super) fn verify_ownership(snapshot: &Snapshot, ownership: &Ownership) -> Result<(), Hold> {
    let mut aliases = BTreeSet::new();
    if ownership.tags.iter().any(|(target, support)| {
        !snapshot.state.tags.contains(target)
            || support.is_empty()
            || support.iter().any(|alias| !aliases.insert(*alias))
    }) || ownership
        .fields
        .iter()
        .any(|id| !snapshot.state.fields.contains_key(id))
    {
        return Err(Hold::BaselineUnproven);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipts_own_only_insertions_and_keep_all_supporting_aliases() {
        let tag = Uuid::new_v4();
        let local = Uuid::new_v4();
        let state = State {
            tags: [tag, local].into(),
            ..State::default()
        };
        let receipts = [
            Receipt {
                kind: "tag_link",
                source_key: &[1; 32],
                target: Some(tag),
                outcome: "applied",
            },
            Receipt {
                kind: "tag_link",
                source_key: &[2; 32],
                target: Some(tag),
                outcome: "already_present",
            },
            Receipt {
                kind: "tag_link",
                source_key: &[3; 32],
                target: Some(local),
                outcome: "already_present",
            },
        ];
        let owned = ownership(&state, &receipts).unwrap();
        assert_eq!(owned.tags.len(), 1);
        assert_eq!(owned.tags[&tag], BTreeSet::from([[1; 32], [2; 32]]));
        let reversed: Vec<_> = receipts.into_iter().rev().collect();
        assert_eq!(ownership(&state, &reversed).unwrap().tags, owned.tags);
    }
    #[test]
    fn equal_and_held_fields_remain_unowned() {
        use crate::domain::custom_field::CustomFieldValue;
        let applied = Uuid::new_v4();
        let equal = Uuid::new_v4();
        let held = Uuid::new_v4();
        let state = State {
            fields: [applied, equal, held]
                .into_iter()
                .map(|id| (id, CustomFieldValue::Text("same".into())))
                .collect(),
            ..State::default()
        };
        let receipts = [
            Receipt {
                kind: "value",
                source_key: &[1; 32],
                target: Some(applied),
                outcome: "applied",
            },
            Receipt {
                kind: "value",
                source_key: &[2; 32],
                target: Some(equal),
                outcome: "already_present",
            },
            Receipt {
                kind: "value",
                source_key: &[3; 32],
                target: Some(held),
                outcome: "held",
            },
        ];
        assert_eq!(
            ownership(&state, &receipts).unwrap().fields,
            BTreeSet::from([applied])
        );
    }
    #[test]
    fn malformed_or_conflicting_positive_receipts_never_grant_ownership() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let state = State {
            tags: [a, b].into(),
            ..State::default()
        };
        assert!(ownership(
            &state,
            &[Receipt {
                kind: "tag_link",
                source_key: &[1; 31],
                target: Some(a),
                outcome: "applied"
            }]
        )
        .is_err());
        assert!(ownership(
            &state,
            &[
                Receipt {
                    kind: "tag_link",
                    source_key: &[1; 32],
                    target: Some(a),
                    outcome: "applied"
                },
                Receipt {
                    kind: "tag_link",
                    source_key: &[1; 32],
                    target: Some(b),
                    outcome: "already_present"
                }
            ]
        )
        .is_err());
        assert!(ownership(
            &State::default(),
            &[Receipt {
                kind: "value",
                source_key: &[1; 32],
                target: Some(a),
                outcome: "applied"
            }]
        )
        .is_err());
    }
    #[test]
    fn proof_rejects_legacy_wrong_binding_aba_and_unowned_state_changes() {
        let org = OrganizationId::new(Uuid::new_v4());
        let binding = Binding {
            organization: org,
            import: Uuid::new_v4(),
            manifest: Uuid::new_v4(),
            person: Uuid::new_v4(),
        };
        let snapshot = Snapshot {
            person: binding.person,
            revision: 4,
            head: None,
            state: State::default(),
        };
        let proof = AfterState {
            version: 1,
            organization: org.0,
            import: binding.import,
            manifest: binding.manifest,
            snapshot: snapshot.clone(),
            ownership: Ownership::default(),
        };
        let bytes = serde_json::to_vec(&proof).unwrap();
        let proof: AfterState = serde_json::from_slice(&bytes).unwrap();
        assert!(verify(Some(&proof), &binding, Some(&snapshot)).is_ok());
        assert!(matches!(
            verify(None, &binding, Some(&snapshot)),
            Err(Hold::BaselineUnproven)
        ));
        let mut current = snapshot.clone();
        current.revision += 2;
        assert!(matches!(
            verify(Some(&proof), &binding, Some(&current)),
            Err(Hold::LocalChange)
        ));
        current = snapshot.clone();
        current.state.tags.insert(Uuid::new_v4());
        assert!(matches!(
            verify(Some(&proof), &binding, Some(&current)),
            Err(Hold::LocalChange)
        ));
        let other = Binding {
            organization: OrganizationId::new(Uuid::new_v4()),
            ..binding
        };
        assert!(matches!(
            verify(Some(&proof), &other, Some(&snapshot)),
            Err(Hold::BaselineUnproven)
        ));
    }
}

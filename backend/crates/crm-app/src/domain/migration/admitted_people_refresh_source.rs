//! Deliberately small source helpers. The worker only accepts a retained record
//! whose capture, ordinal and semantic HMAC agree with the immutable index.
use super::{
    admitted_people_refresh_store::CAPTURE_LIMIT,
    crypto,
    import_source::{self, Entity, ExtractedRecord},
    snapshot_source::Stream,
    MigrationError,
};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A missing later observation is explicit non-deletion evidence. Retained
/// observations that cannot be trusted are materially different: the worker
/// must hold the Person rather than silently call damaged evidence absence.
pub enum RetainedPerson {
    Missing,
    EvidenceGap,
    Present(ExtractedRecord, Uuid, i32),
}

pub async fn retained_person(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    source_id: &str,
) -> Result<RetainedPerson, MigrationError> {
    // A source ID may have been observed on more than one retained page. Every
    // observation is evidence: equal semantic records are safe repetition,
    // while a semantic disagreement is ambiguity. Do not choose a later
    // variant merely because it sorts last.
    let rows = sqlx::query(
        "SELECT r.id,r.capture_id,r.ordinal,r.semantic_hmac,
                c.raw_byte_len,c.accepted,c.truncated,c.http_status,
                c.classification,c.representation
           FROM migration_snapshot_record r
           JOIN migration_snapshot_capture c
             ON c.id=r.capture_id AND c.snapshot_id=r.snapshot_id
            AND c.organization_id=r.organization_id
          WHERE r.snapshot_id=$1 AND r.organization_id=$2
            AND r.family='people' AND r.source_id=$3
          ORDER BY r.capture_sequence DESC,r.ordinal DESC LIMIT 51",
    )
    .bind(snapshot)
    .bind(org.0)
    .bind(source_id)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(RetainedPerson::Missing);
    }
    if rows.len() > 50 {
        return Ok(RetainedPerson::EvidenceGap);
    }

    let mut semantic: Option<Vec<u8>> = None;
    let mut raw_total = 0_i64;
    let mut selected: Option<(ExtractedRecord, Uuid, i32)> = None;
    for row in rows {
        let raw_len: i64 = row.get("raw_byte_len");
        if !row.get::<bool, _>("accepted")
            || row.get::<bool, _>("truncated")
            || !(200..300).contains(&row.get::<i32, _>("http_status"))
            || row.get::<String, _>("classification") != "success"
            || row.get::<String, _>("representation") != Stream::People.representation()
            || raw_len > CAPTURE_LIMIT
            || raw_total.saturating_add(raw_len) > CAPTURE_LIMIT
        {
            return Ok(RetainedPerson::EvidenceGap);
        }
        // Fetch and open one raw capture at a time. This validates every
        // repeated observation without materializing an unbounded capture set.
        let capture_id: Uuid = row.get("capture_id");
        let capture = sqlx::query(
            "SELECT nonce,ciphertext FROM migration_snapshot_capture
              WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3",
        )
        .bind(capture_id)
        .bind(snapshot)
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(MigrationError::Crypto)?;
        let raw = crypto::open_snapshot(
            key,
            org,
            snapshot,
            capture_id,
            "capture",
            &capture.get::<Vec<u8>, _>("nonce"),
            &capture.get::<Vec<u8>, _>("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        if raw.len() as i64 != raw_len {
            return Err(MigrationError::Crypto);
        }
        raw_total += raw_len;
        let item = import_source::extract_page(Stream::People, &raw)
            .map_err(|_| MigrationError::SourceNotEligible)?
            .get(
                usize::try_from(row.get::<i32, _>("ordinal"))
                    .map_err(|_| MigrationError::Crypto)?,
            )
            .cloned()
            .ok_or(MigrationError::SourceNotEligible)?;
        let observed = crypto::snapshot_hmac(
            key,
            org,
            &format!("semantic:{}", Stream::People.representation()),
            &item.canonical,
        );
        if item.source_id.as_deref() != Some(source_id)
            || row.get::<Vec<u8>, _>("semantic_hmac") != observed
        {
            return Err(MigrationError::Crypto);
        }
        if !same_semantic(&mut semantic, &observed) {
            return Ok(RetainedPerson::EvidenceGap);
        }
        if selected.is_none() {
            selected = Some((item, capture_id, row.get("ordinal")));
        }
    }
    Ok(selected
        .map(|(record, capture, ordinal)| RetainedPerson::Present(record, capture, ordinal))
        .unwrap_or(RetainedPerson::EvidenceGap))
}

fn same_semantic(expected: &mut Option<Vec<u8>>, observed: &[u8]) -> bool {
    match expected {
        Some(value) => value.as_slice() == observed,
        None => {
            *expected = Some(observed.to_vec());
            true
        }
    }
}

/// Presence-aware refresh overlay. `ExtractedRecord` is retained raw evidence:
/// its provenance map distinguishes an absent key from JSON null, which 010c's
/// normal import projection intentionally did not need to preserve.
/// The executable projection and the components that were deliberately left at
/// their prior baseline because the retained source omitted an instruction.
/// This metadata is stored beside the immutable plan; it is never mixed into
/// the projection used for B/C/N equality or native writes.
pub struct PeopleOverlay {
    pub projection: Value,
    pub no_instruction: Vec<&'static str>,
}

pub fn overlay_people(
    baseline: &Value,
    newer: &ExtractedRecord,
) -> Result<PeopleOverlay, MigrationError> {
    let mut next = baseline.clone();
    let mut no_instruction = Vec::new();
    let Entity::People(person) = &newer.entity else {
        return Err(MigrationError::SourceNotEligible);
    };
    for (source, target) in [("firstName", "first_name"), ("lastName", "last_name")] {
        let Some(raw) = newer.provenance.get(source) else {
            no_instruction.push(source);
            continue;
        };
        let value: Value = serde_json::from_str(raw).map_err(|_| MigrationError::Crypto)?;
        match value {
            Value::Null => next[target] = Value::Null,
            Value::String(text) => {
                if text.contains('\0') {
                    return Err(MigrationError::SourceNotEligible);
                }
                next[target] = if text.trim().is_empty() {
                    Value::Null
                } else {
                    Value::String(text)
                }
            }
            _ => return Err(MigrationError::SourceNotEligible),
        }
    }
    for (source, kind) in [("emails", "email"), ("phones", "phone")] {
        let Some(raw) = newer.provenance.get(source) else {
            no_instruction.push(source);
            continue;
        };
        let value: Value = serde_json::from_str(raw).map_err(|_| MigrationError::Crypto)?;
        if value.is_null() {
            no_instruction.push(source);
            continue;
        }
        if !value.is_array() {
            return Err(MigrationError::SourceNotEligible);
        }
        if newer.reasons.iter().any(|reason| reason.starts_with(kind)) {
            return Err(MigrationError::SourceNotEligible);
        }
        let replacement=person.contacts.iter().filter(|contact|contact.kind==kind).map(|contact|json!({"kind":contact.kind,"value":contact.value,"normalized_value":contact.normalized_value,"import_order":contact.import_order})).collect::<Vec<_>>();
        let contacts = next["contacts"]
            .as_array_mut()
            .ok_or(MigrationError::Crypto)?;
        contacts.retain(|contact| contact["kind"] != kind);
        contacts.extend(replacement);
    }
    let displayable = next["first_name"]
        .as_str()
        .is_some_and(|v| !v.trim().is_empty())
        || next["last_name"]
            .as_str()
            .is_some_and(|v| !v.trim().is_empty())
        || next["contacts"].as_array().is_some_and(|v| !v.is_empty());
    if !displayable {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(PeopleOverlay {
        projection: next,
        no_instruction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Value {
        json!({
            "first_name":"Baseline",
            "last_name":"Person",
            "stage_id":"00000000-0000-0000-0000-000000000001",
            "assigned_user_id":null,
            "contacts":[
                {"id":"00000000-0000-0000-0000-000000000010","kind":"email","value":"baseline@example.test","normalized_value":"baseline@example.test","import_order":0},
                {"id":"00000000-0000-0000-0000-000000000011","kind":"phone","value":"4155550100","normalized_value":"+14155550100","import_order":0}
            ]
        })
    }

    fn record(raw: &str) -> ExtractedRecord {
        let page = format!(r#"{{"people":[{raw}]}}"#);
        import_source::extract_page(Stream::People, page.as_bytes())
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn missing_components_preserve_baseline_and_are_recorded() {
        let b = baseline();
        let overlay = overlay_people(&b, &record(r#"{"id":1,"stage":"Lead"}"#)).unwrap();
        assert_eq!(overlay.projection, b);
        assert_eq!(
            overlay.no_instruction,
            vec!["firstName", "lastName", "emails", "phones"]
        );
    }

    #[test]
    fn null_and_whitespace_names_are_visible_clears() {
        let overlay = overlay_people(
            &baseline(),
            &record(r#"{"id":1,"firstName":null,"lastName":"  ","emails":null,"phones":null}"#),
        )
        .unwrap();
        assert_eq!(overlay.projection["first_name"], Value::Null);
        assert_eq!(overlay.projection["last_name"], Value::Null);
        assert_eq!(overlay.no_instruction, vec!["emails", "phones"]);
    }

    #[test]
    fn null_collections_preserve_and_empty_arrays_clear_only_that_kind() {
        let retained = overlay_people(
            &baseline(),
            &record(r#"{"id":1,"emails":null,"phones":null}"#),
        )
        .unwrap();
        assert_eq!(retained.projection["contacts"].as_array().unwrap().len(), 2);
        assert_eq!(
            retained.no_instruction,
            vec!["firstName", "lastName", "emails", "phones"]
        );

        let cleared = overlay_people(
            &baseline(),
            &record(r#"{"id":1,"emails":[],"phones":null}"#),
        )
        .unwrap();
        let contacts = cleared.projection["contacts"].as_array().unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0]["kind"], "phone");
        assert_eq!(contacts[0]["id"], "00000000-0000-0000-0000-000000000011");
    }

    #[test]
    fn equal_repeated_observations_are_qualified() {
        let mut expected = None;
        assert!(same_semantic(&mut expected, b"same"));
        assert!(same_semantic(&mut expected, b"same"));
    }

    #[test]
    fn conflicting_repeated_observations_are_ambiguous() {
        let mut expected = None;
        assert!(same_semantic(&mut expected, b"first"));
        assert!(!same_semantic(&mut expected, b"second"));
    }

    #[test]
    fn any_invalid_supplied_component_holds_the_whole_person() {
        assert!(matches!(
            overlay_people(
                &baseline(),
                &record(
                    r#"{"id":1,"firstName":"New","emails":[{"value":"new@example.test"},{"value":42}]}"#
                )
            ),
            Err(MigrationError::SourceNotEligible)
        ));
        assert!(matches!(
            overlay_people(&baseline(), &record(r#"{"id":1,"firstName":42}"#)),
            Err(MigrationError::SourceNotEligible)
        ));
    }
}

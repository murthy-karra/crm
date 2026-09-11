//! Pure 010c extraction from complete retained source bytes, never preview data.
//!
//! The caller qualifies capture scope/status/acceptance/representation and matches
//! each returned ordinal, source ID and canonical HMAC against stored evidence.
//! This module interprets native fields only; original top-level values remain
//! canonical JSON text so unknown fields and arbitrary-precision numbers survive.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::snapshot_source::{positive_id, JsonParser, Node, ParseError, Stream};
use crate::domain::contact::{normalize_email, normalize_phone};

const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_ITEMS: usize = 100;
const MAX_INDEXED_BYTES: usize = 2048;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ExtractedRecord {
    pub source_id: Option<String>,
    pub canonical: Vec<u8>,
    pub entity: Entity,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "family", content = "value", rename_all = "snake_case")]
pub(crate) enum Entity {
    People(PersonInput),
    Stage(StageInput),
    User(UserInput),
    Invalid,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PersonInput {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub contacts: Vec<ContactInput>,
    pub stage_label: Option<String>,
    pub assignee_key: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ContactInput {
    pub kind: String,
    pub value: String,
    pub normalized_value: String,
    pub import_order: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StageInput {
    pub label: Option<String>,
    pub can_create: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UserInput {
    pub email: Option<String>,
    pub name: Option<String>,
    pub is_pond: bool,
}

pub(crate) fn extract_page(stream: Stream, raw: &[u8]) -> Result<Vec<ExtractedRecord>, ParseError> {
    if !matches!(stream, Stream::People | Stream::Stages | Stream::Users) {
        return Err(ParseError::Malformed);
    }
    if raw.len() > MAX_RAW_BYTES {
        return Err(ParseError::DerivedTooLarge);
    }
    let root = JsonParser::parse(raw)?;
    let items = root
        .get(stream.collection())
        .and_then(Node::array)
        .ok_or(ParseError::Malformed)?;
    if items.len() > MAX_PAGE_ITEMS {
        return Err(ParseError::DerivedTooLarge);
    }
    // Source pagination is already settled in 010b. Historical request cursors
    // are not available here; never invent one to re-run the HTTP page parser.
    Ok(items
        .iter()
        .map(|node| extract_record(stream, node))
        .collect())
}

fn mark(values: &mut Vec<String>, code: &str) {
    if !values.iter().any(|v| v == code) {
        values.push(code.into());
    }
}

fn canonical(node: &Node) -> Vec<u8> {
    let mut output = Vec::new();
    node.encode(&mut output);
    output
}

fn extract_record(stream: Stream, node: &Node) -> ExtractedRecord {
    let source_id = node.get("id").and_then(positive_id);
    let mut reasons = Vec::new();
    let mut transformations = Vec::new();
    let mut provenance = BTreeMap::new();
    if source_id.is_none() {
        mark(&mut reasons, "invalid_source_id");
    }
    let entity = if let Node::Object(fields) = node {
        for (key, value) in fields {
            // Node's encoder emits UTF-8 from a validated JSON tree. Do not
            // deserialize these numbers through serde_json::Value or f64.
            provenance.insert(
                key.clone(),
                String::from_utf8(canonical(value)).expect("canonical JSON is UTF-8"),
            );
        }
        match node.get("showContent") {
            Some(Node::Bool(false)) => mark(&mut reasons, "content_inaccessible"),
            None | Some(Node::Null | Node::Bool(true)) => {}
            _ => mark(&mut transformations, "show_content_unqualified"),
        }
        match stream {
            Stream::People => Entity::People(person(node, &mut reasons, &mut transformations)),
            Stream::Stages => Entity::Stage(stage(node, &mut reasons, &mut transformations)),
            Stream::Users => Entity::User(user(node, &mut reasons)),
            _ => unreachable!("extract_page permits only the three import families"),
        }
    } else {
        mark(&mut reasons, "unsupported_record_shape");
        Entity::Invalid
    };
    reasons.sort();
    transformations.sort();
    ExtractedRecord {
        source_id,
        canonical: canonical(node),
        entity,
        reasons,
        transformations,
        provenance,
    }
}

fn optional_text(node: &Node, field: &str, reasons: &mut Vec<String>) -> Option<String> {
    match node.get(field) {
        None | Some(Node::Null) => None,
        Some(Node::String(value)) => Some(value.clone()),
        _ => {
            mark(reasons, &format!("{field}_shape_unqualified"));
            None
        }
    }
}

fn name(
    node: &Node,
    field: &str,
    reasons: &mut Vec<String>,
    transformations: &mut Vec<String>,
) -> Option<String> {
    let value = optional_text(node, field, reasons)?;
    if value.contains('\0') {
        mark(reasons, "native_text_contains_nul");
    }
    if value.trim().is_empty() {
        mark(transformations, &format!("{field}_whitespace_to_null"));
        None
    } else {
        Some(value)
    }
}

fn person(
    node: &Node,
    reasons: &mut Vec<String>,
    transformations: &mut Vec<String>,
) -> PersonInput {
    let first_name = name(node, "firstName", reasons, transformations);
    let last_name = name(node, "lastName", reasons, transformations);
    let mut contact_inputs = contacts(node, "emails", "email", reasons, transformations);
    contact_inputs.extend(contacts(node, "phones", "phone", reasons, transformations));
    if first_name.is_none() && last_name.is_none() && contact_inputs.is_empty() {
        mark(reasons, "person_has_no_displayable_identity");
    }
    let stage_label = optional_text(node, "stage", reasons)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    if stage_label.as_deref() == Some("Trash") {
        mark(reasons, "trash_person");
    }
    match node.get("isTrash") {
        Some(Node::Bool(true)) => mark(reasons, "trash_person"),
        None | Some(Node::Null | Node::Bool(false)) => {}
        _ => mark(transformations, "is_trash_unqualified"),
    }
    PersonInput {
        first_name,
        last_name,
        contacts: contact_inputs,
        stage_label,
        assignee_key: assignee(node, reasons),
    }
}

fn assignee(node: &Node, reasons: &mut Vec<String>) -> Option<String> {
    let reference = |field| match node.get(field) {
        None | Some(Node::Null) => Ok(None),
        Some(value) => positive_id(value).map(Some).ok_or(()),
    };
    match (reference("assignedUserId"), reference("assignedPondId")) {
        (Ok(Some(_)), Ok(Some(_))) => {
            mark(reasons, "assignment_conflict");
            None
        }
        (Err(()), _) | (_, Err(())) => {
            // In particular, zero is non-null but not a qualified source ID.
            mark(reasons, "assignment_unqualified");
            None
        }
        (Ok(Some(user)), Ok(None)) => Some(user),
        (Ok(None), Ok(Some(pond))) => Some(format!("pond:{pond}")),
        (Ok(None), Ok(None)) => {
            if !matches!(node.get("assignedTo"), None | Some(Node::Null)) {
                mark(reasons, "assignment_unqualified");
            }
            None
        }
    }
}

fn contacts(
    node: &Node,
    field: &str,
    kind: &str,
    reasons: &mut Vec<String>,
    transformations: &mut Vec<String>,
) -> Vec<ContactInput> {
    let items = match node.get(field) {
        None | Some(Node::Null) => return Vec::new(),
        Some(Node::Array(items)) => items,
        _ => {
            mark(reasons, &format!("{kind}_collection_unqualified"));
            return Vec::new();
        }
    };
    let mut candidates = Vec::new();
    let mut primary_count = 0;
    let mut nulls = 0;
    let mut empty = 0;
    for item in items {
        if matches!(item, Node::Null) {
            nulls += 1;
            continue;
        }
        if !matches!(item, Node::Object(_)) {
            mark(reasons, &format!("{kind}_entry_unqualified"));
            continue;
        }
        let primary = match item.get("isPrimary") {
            Some(Node::Number(value)) if value == "1e0" => true,
            Some(Node::Number(value)) if value == "0" => false,
            None | Some(Node::Null) => false,
            _ => {
                mark(transformations, &format!("{kind}_primary_unqualified"));
                false
            }
        };
        primary_count += usize::from(primary);
        let value = match item.get("value") {
            Some(Node::Null) => {
                nulls += 1;
                continue;
            }
            Some(Node::String(value)) if value.trim().is_empty() => {
                empty += 1;
                continue;
            }
            Some(Node::String(value)) => value,
            _ => {
                mark(reasons, &format!("{kind}_value_unqualified"));
                continue;
            }
        };
        if value.contains('\0') {
            mark(reasons, "native_text_contains_nul");
            continue;
        }
        let normalized = if kind == "email" {
            normalize_email(value).map(|v| v.as_str().to_owned())
        } else {
            normalize_phone(value).map(|v| v.as_str().to_owned())
        };
        let Some(normalized) = normalized else {
            mark(reasons, &format!("{kind}_not_normalizable"));
            continue;
        };
        if normalized.len() > MAX_INDEXED_BYTES {
            mark(reasons, &format!("{kind}_normalized_value_too_large"));
            continue;
        }
        candidates.push((primary, value.clone(), normalized));
    }
    if primary_count == 1 {
        if let Some(index) = candidates.iter().position(|v| v.0) {
            let primary = candidates.remove(index);
            candidates.insert(0, primary);
        } else {
            mark(transformations, &format!("{kind}_primary_unavailable"));
        }
    } else if primary_count > 1 {
        mark(transformations, &format!("{kind}_primary_ambiguous"));
    }
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    let mut duplicates = 0;
    for (_, value, normalized) in candidates {
        if !seen.insert(normalized.clone()) {
            duplicates += 1;
            continue;
        }
        output.push(ContactInput {
            kind: kind.into(),
            value,
            normalized_value: normalized,
            import_order: output.len() as i32,
        });
    }
    for (count, suffix) in [
        (nulls, "null_entries"),
        (empty, "empty_entries"),
        (duplicates, "duplicates_collapsed"),
    ] {
        if count > 0 {
            transformations.push(format!("{kind}_{suffix}:{count}"));
        }
    }
    output
}

fn stage(node: &Node, reasons: &mut Vec<String>, transformations: &mut Vec<String>) -> StageInput {
    let original = optional_text(node, "name", reasons);
    let label = original.as_ref().map(|v| v.trim().to_owned());
    if original != label {
        mark(transformations, "stage_label_trimmed");
    }
    let mut can_create = true;
    match label.as_deref() {
        None | Some("") => {
            can_create = false;
            mark(reasons, "stage_label_missing");
        }
        Some(value) => {
            // These creation-specific reasons do not invalidate qualified raw
            // evidence or forbid explicit mapping to an existing native stage.
            if value.contains('\0') {
                can_create = false;
                mark(reasons, "stage_create_native_nul");
            }
            if value.len() > MAX_INDEXED_BYTES {
                can_create = false;
                mark(reasons, "stage_create_label_too_large");
            }
            if value == "Trash" {
                can_create = false;
                mark(reasons, "trash_stage");
            }
        }
    }
    StageInput {
        label: label.filter(|v| !v.is_empty()),
        can_create,
    }
}

fn user(node: &Node, reasons: &mut Vec<String>) -> UserInput {
    UserInput {
        email: optional_text(node, "email", reasons),
        name: optional_text(node, "name", reasons),
        // The users profile has no qualified pond-record discriminator.
        // Person.assignedPondId supplies a separate namespaced reference.
        is_pond: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(stream: Stream, items: &str) -> Vec<u8> {
        format!(r#"{{"{}":[{items}]}}"#, stream.collection()).into_bytes()
    }

    fn one(stream: Stream, item: &str) -> ExtractedRecord {
        extract_page(stream, &page(stream, item)).unwrap().remove(0)
    }

    fn person_input(record: &ExtractedRecord) -> &PersonInput {
        let Entity::People(value) = &record.entity else {
            panic!("expected Person")
        };
        value
    }

    fn has(values: &[String], expected: &str) -> bool {
        values.iter().any(|v| v == expected)
    }

    #[test]
    fn import_source_exact_ids_unknown_values_and_semantics_survive() {
        let a = one(
            Stream::People,
            r#"{"id":184467440737095516160001,"firstName":"Ada","unknown":{"amount":0.12345678901234567890123456789,"large":9007199254740993},"assignedUserId":999999999999999999999,"stage":"Lead"}"#,
        );
        assert_eq!(a.source_id.as_deref(), Some("184467440737095516160001"));
        assert_eq!(
            person_input(&a).assignee_key.as_deref(),
            Some("999999999999999999999")
        );
        assert!(a.provenance["unknown"].contains("12345678901234567890123456789e-29"));
        assert!(a.provenance["unknown"].contains("9007199254740993e0"));
        let b = one(
            Stream::People,
            r#"{"stage":"Lead","assignedUserId":999999999999999999999,"unknown":{"large":9007199254740993.0,"amount":12345678901234567890123456789e-29},"firstName":"Ada","id":184467440737095516160001}"#,
        );
        assert_eq!(a.canonical, b.canonical);
        let long = "9".repeat(128);
        assert_eq!(
            one(Stream::Users, &format!(r#"{{"id":{long}}}"#)).source_id,
            Some(long)
        );
        assert!(
            one(Stream::Users, &format!(r#"{{"id":{}}}"#, "9".repeat(129)))
                .source_id
                .is_none()
        );
    }

    #[test]
    fn import_source_primary_precedes_dedup_and_people_remain_separate() {
        let item = r#"{"id":1,"firstName":"Ada","emails":[{"value":" Shared@Example.test ","isPrimary":0,"label":"Home"},{"value":"other@example.test","isPrimary":0},{"value":"SHARED@example.test","isPrimary":1,"label":"Work","unknownFlag":{"value":false}}],"phones":[{"value":"(415)555-0100","isPrimary":0},{"value":"+14155550100","isPrimary":1}]}"#;
        let a = one(Stream::People, item);
        let p = person_input(&a);
        assert_eq!(p.contacts.len(), 3);
        assert_eq!(p.contacts[0].value, "SHARED@example.test");
        assert_eq!(p.contacts[0].normalized_value, "shared@example.test");
        assert_eq!(p.contacts[0].import_order, 0);
        assert_eq!(p.contacts[1].import_order, 1);
        assert_eq!(p.contacts[2].value, "+14155550100");
        assert_eq!(p.contacts[2].import_order, 0);
        assert!(has(&a.transformations, "email_duplicates_collapsed:1"));
        assert!(a.provenance["emails"].contains("unknownFlag"));
        assert!(a.provenance["emails"].contains(" Shared@Example.test "));
        let b = one(
            Stream::People,
            r#"{"id":2,"emails":[{"value":"shared@example.test"}]}"#,
        );
        assert_eq!(person_input(&b).contacts.len(), 1);
        assert_ne!(a.source_id, b.source_id);
    }

    #[test]
    fn import_source_primary_shapes_and_empty_entries_are_explicit() {
        for marker in ["true", "\"1\"", "null"] {
            let item = format!(
                r#"{{"id":1,"emails":[{{"value":"first@x"}},{{"value":"second@x","isPrimary":{marker}}},null,{{"value":null}},{{"value":"  "}}]}}"#
            );
            let r = one(Stream::People, &item);
            assert_eq!(person_input(&r).contacts[0].value, "first@x");
            assert!(has(&r.transformations, "email_null_entries:2"));
            assert!(has(&r.transformations, "email_empty_entries:1"));
            assert_eq!(
                has(&r.transformations, "email_primary_unqualified"),
                marker != "null"
            );
        }
        let r = one(
            Stream::People,
            r#"{"id":1,"emails":[{"value":"first@x","isPrimary":1},{"value":"second@x","isPrimary":1}]}"#,
        );
        assert_eq!(person_input(&r).contacts[0].value, "first@x");
        assert!(has(&r.transformations, "email_primary_ambiguous"));
    }

    #[test]
    fn import_source_native_byte_limits_apply_after_normalization() {
        let limit = format!("{}@x", "é".repeat(1023));
        assert_eq!(limit.len(), 2048);
        let valid = one(
            Stream::People,
            &format!(
                r#"{{"id":1,"emails":[{{"value":{}}}]}}"#,
                serde_json::to_string(&limit).unwrap()
            ),
        );
        assert!(valid.reasons.is_empty());
        let too_large = one(
            Stream::People,
            &format!(
                r#"{{"id":1,"firstName":"Ada","emails":[{{"value":{}}}]}}"#,
                serde_json::to_string(&format!("{limit}a")).unwrap()
            ),
        );
        assert!(has(&too_large.reasons, "email_normalized_value_too_large"));
        let expansion = format!("{}a@x", "İ".repeat(682));
        assert!(expansion.len() < 2048);
        let r = one(
            Stream::People,
            &format!(
                r#"{{"id":1,"firstName":"Ada","emails":[{{"value":{}}}]}}"#,
                serde_json::to_string(&expansion).unwrap()
            ),
        );
        assert!(has(&r.reasons, "email_normalized_value_too_large"));
        for item in [
            r#"{"id":1,"firstName":"A\u0000da"}"#,
            r#"{"id":1,"firstName":"Ada","phones":[{"value":"415\u00005550100"}]}"#,
        ] {
            assert!(has(
                &one(Stream::People, item).reasons,
                "native_text_contains_nul"
            ));
        }
    }

    #[test]
    fn import_source_stage_creation_limits_do_not_discard_mapping_evidence() {
        for (bytes, can_create) in [(2048, true), (2049, false)] {
            let label = "x".repeat(bytes);
            let r = one(Stream::Stages, &format!(r#"{{"id":4,"name":"{label}"}}"#));
            let Entity::Stage(stage) = r.entity else {
                panic!("stage")
            };
            assert_eq!(stage.label.as_deref(), Some(label.as_str()));
            assert_eq!(stage.can_create, can_create);
            assert_eq!(has(&r.reasons, "stage_create_label_too_large"), !can_create);
        }
        let r = one(Stream::Stages, r#"{"id":4,"name":"A\u0000B"}"#);
        assert!(has(&r.reasons, "stage_create_native_nul"));
        assert_eq!(r.provenance["name"], r#""A\u0000B""#);
    }

    #[test]
    fn import_source_assignment_and_visibility_are_not_guessed() {
        for value in ["0", "\"0\"", "true", "1.5", "{}"] {
            let r = one(
                Stream::People,
                &format!(r#"{{"id":1,"firstName":"Ada","assignedUserId":{value}}}"#),
            );
            assert!(has(&r.reasons, "assignment_unqualified"));
        }
        let pond = one(
            Stream::People,
            r#"{"id":1,"firstName":"Ada","assignedPondId":1234567890123456789012345}"#,
        );
        assert_eq!(
            person_input(&pond).assignee_key.as_deref(),
            Some("pond:1234567890123456789012345")
        );
        let both = one(
            Stream::People,
            r#"{"id":1,"firstName":"Ada","assignedUserId":1,"assignedPondId":2}"#,
        );
        assert!(has(&both.reasons, "assignment_conflict"));
        assert!(has(
            &one(
                Stream::People,
                r#"{"id":1,"firstName":"Ada","assignedTo":"Source Name"}"#
            )
            .reasons,
            "assignment_unqualified"
        ));
        assert!(has(
            &one(
                Stream::People,
                r#"{"id":1,"firstName":"Ada","stage":"Trash"}"#
            )
            .reasons,
            "trash_person"
        ));
        assert!(has(
            &one(Stream::Users, r#"{"id":3,"showContent":false}"#).reasons,
            "content_inaccessible"
        ));
        let unknown = one(
            Stream::People,
            r#"{"id":1,"firstName":"Ada","showContent":"false","unknownPrivacyFlag":true}"#,
        );
        assert!(unknown.reasons.is_empty());
        assert!(has(&unknown.transformations, "show_content_unqualified"));
        assert_eq!(unknown.provenance["unknownPrivacyFlag"], "true");
    }

    #[test]
    fn import_source_explicit_trash_flag_is_held_without_truthy_coercion() {
        let trash = one(
            Stream::People,
            r#"{"id":103,"firstName":"Ada","stage":"Unfamiliar","isTrash":true}"#,
        );
        assert!(has(&trash.reasons, "trash_person"));
        assert_eq!(trash.provenance["isTrash"], "true");
        for flag in ["false", "null", "1", "\"true\"", "{}"] {
            let record = one(
                Stream::People,
                &format!(r#"{{"id":103,"firstName":"Ada","isTrash":{flag}}}"#),
            );
            assert!(!has(&record.reasons, "trash_person"));
            assert_eq!(
                has(&record.transformations, "is_trash_unqualified"),
                !matches!(flag, "false" | "null")
            );
            assert!(record.provenance.contains_key("isTrash"));
        }
    }

    #[test]
    fn import_source_names_shapes_and_contactless_people_preserve_meaning() {
        let r = one(
            Stream::People,
            r#"{"id":1,"firstName":" Ada ","lastName":"  ","emails":[],"created":null,"unknown":[]}"#,
        );
        assert_eq!(person_input(&r).first_name.as_deref(), Some(" Ada "));
        assert!(person_input(&r).last_name.is_none());
        assert!(r.reasons.is_empty());
        assert!(has(&r.transformations, "lastName_whitespace_to_null"));
        assert_eq!(r.provenance["created"], "null");
        assert!(!r.provenance.contains_key("updated"));
        assert!(has(
            &one(Stream::People, r#"{"id":1,"name":"Do not split this"}"#).reasons,
            "person_has_no_displayable_identity"
        ));
        assert!(has(
            &one(
                Stream::People,
                r#"{"id":1,"firstName":"Ada","emails":[{"value":42}]}"#
            )
            .reasons,
            "email_value_unqualified"
        ));
    }

    #[test]
    fn import_source_large_provenance_is_complete_and_manifest_serializable() {
        let url = format!(
            "https://synthetic.invalid/{}",
            "x".repeat(2 * 1024 * 1024 + 7)
        );
        let raw = format!(
            r#"{{"id":1,"firstName":"Ada","sourceUrl":{},"unknown":{{"number":1e1000000}}}}"#,
            serde_json::to_string(&url).unwrap()
        );
        let r = one(Stream::People, &raw);
        assert!(r.reasons.is_empty());
        assert_eq!(
            r.provenance["sourceUrl"],
            serde_json::to_string(&url).unwrap()
        );
        assert_eq!(r.provenance["unknown"], r#"{"number":1e1000000}"#);
        let stored = serde_json::to_vec(&r).unwrap();
        let restored: ExtractedRecord = serde_json::from_slice(&stored).unwrap();
        assert_eq!(r.canonical, restored.canonical);
        assert_eq!(r.provenance, restored.provenance);
    }

    #[test]
    fn import_source_rejects_wrong_family_duplicate_keys_and_unbounded_pages() {
        assert!(extract_page(Stream::Notes, b"{\"notes\":[]}").is_err());
        assert!(extract_page(Stream::People, b"{\"users\":[]}").is_err());
        assert_eq!(
            extract_page(
                Stream::People,
                &page(Stream::People, r#"{"id":1,"\u0069d":2}"#)
            )
            .err(),
            Some(ParseError::DuplicateKey)
        );
        assert_eq!(
            extract_page(
                Stream::People,
                &page(Stream::People, &vec![r#"{"id":1}"#; 101].join(","))
            )
            .err(),
            Some(ParseError::DerivedTooLarge)
        );
        assert_eq!(
            extract_page(Stream::People, &vec![b' '; MAX_RAW_BYTES + 1]).err(),
            Some(ParseError::DerivedTooLarge)
        );
        let r = one(Stream::People, "null");
        assert!(matches!(r.entity, Entity::Invalid));
        assert!(has(&r.reasons, "invalid_source_id"));
        assert!(has(&r.reasons, "unsupported_record_shape"));
    }
}

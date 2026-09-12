//! Pure 010f1 metadata extraction from complete retained captures.
//!
//! Capture qualification, source identity conflicts, native mappings and aggregate
//! limits belong to the caller. Source content deliberately has no Debug impl.
//! Canonical bytes are transient HMAC input; clear them before storing a Record.
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::snapshot_source::{positive_id, JsonParser, Node, ParseError, Stream};
use crate::domain::custom_field::{
    normalize_and_validate_label, validate_date_range, validate_number_pattern, validate_text_value,
};
use crate::domain::tag::normalize_and_validate_name;

const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_ITEMS: usize = 100;
const MAX_INDEXED_BYTES: usize = 2048;
const MAX_NEW_FIELD_OPTIONS: usize = 50;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    pub source_id: Option<String>,
    pub canonical: Vec<u8>,
    pub entity: Entity,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum Entity {
    Person(PersonInput),
    Field(FieldInput),
    Invalid,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PersonInput {
    pub tags_state: String,
    pub tags: Vec<TagInput>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TagInput {
    pub ordinal: u32,
    pub raw: Option<String>,
    pub label: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FieldInput {
    pub name: Option<String>,
    pub label: Option<String>,
    pub field_type: Option<String>,
    pub choices: Vec<ChoiceInput>,
    pub reasons: Vec<String>,
    pub creation_reasons: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ChoiceInput {
    pub ordinal: u32,
    pub raw: Option<String>,
    pub label: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ValueInput {
    pub disposition: String,
    pub value: Option<NativeValue>,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum NativeValue {
    Text(String),
    Number(String),
    Date(String),
    Choice(String),
}

pub(crate) fn extract_page(stream: Stream, raw: &[u8]) -> Result<Vec<Record>, ParseError> {
    if !matches!(stream, Stream::People | Stream::CustomFields) {
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
    Ok(items.iter().map(|node| record(stream, node)).collect())
}

fn mark(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|existing| existing == reason) {
        reasons.push(reason.into());
    }
}

fn canonical(node: &Node) -> Vec<u8> {
    let mut bytes = Vec::new();
    node.encode(&mut bytes);
    bytes
}

fn record(stream: Stream, node: &Node) -> Record {
    let source_id = node.get("id").and_then(positive_id);
    let mut reasons = Vec::new();
    let mut transformations = Vec::new();
    let mut provenance = BTreeMap::new();
    if source_id.is_none() {
        mark(&mut reasons, "invalid_source_id");
    }
    let entity = if let Node::Object(fields) = node {
        for (key, value) in fields {
            // The bounded parser and encoder retain exact numbers, unknown
            // fields and projection-looking objects without a Value/f64 roundtrip.
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
            Stream::People => Entity::Person(person(node, &mut transformations)),
            Stream::CustomFields => Entity::Field(field(node, &mut transformations)),
            _ => unreachable!("extract_page restricts source families"),
        }
    } else {
        mark(&mut reasons, "unsupported_record_shape");
        Entity::Invalid
    };
    reasons.sort();
    transformations.sort();
    Record {
        source_id,
        canonical: canonical(node),
        entity,
        reasons,
        transformations,
        provenance,
    }
}

fn person(node: &Node, transformations: &mut Vec<String>) -> PersonInput {
    let (state, items) = match node.get("tags") {
        None => ("not_supplied", None),
        Some(Node::Null) => ("source_null", None),
        Some(Node::Array(items)) if items.is_empty() => ("empty", Some(items)),
        Some(Node::Array(items)) => ("present", Some(items)),
        _ => ("held", None),
    };
    let tags = items
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(ordinal, item)| {
            let raw = item.string().map(str::to_owned);
            let mut reasons = Vec::new();
            let label = match raw.as_deref() {
                Some(raw) => match normalize_and_validate_name(raw) {
                    Ok(label) => {
                        if label != raw {
                            mark(transformations, "tag_label_trimmed");
                        }
                        Some(label)
                    }
                    Err(_) => {
                        mark(&mut reasons, "unsupported_tag_label");
                        None
                    }
                },
                None => {
                    mark(&mut reasons, "unsupported_tag_shape");
                    None
                }
            };
            TagInput {
                // A page has at most the parser's 100,000-node ceiling.
                ordinal: ordinal as u32,
                raw,
                label,
                reasons,
            }
        })
        .collect();
    PersonInput {
        tags_state: state.into(),
        tags,
    }
}

fn field(node: &Node, transformations: &mut Vec<String>) -> FieldInput {
    let mut reasons = Vec::new();
    let mut creation_reasons = Vec::new();
    let name = node.get("name").and_then(Node::string).map(str::to_owned);
    match name.as_deref() {
        Some(name) if !name.trim().is_empty() => {
            if name.contains('\0') {
                mark(&mut creation_reasons, "native_source_key_contains_nul");
            }
            if name.len() > MAX_INDEXED_BYTES {
                mark(&mut creation_reasons, "native_source_key_too_large");
            }
        }
        _ => mark(&mut reasons, "unsupported_field_key"),
    }
    let label =
        node.get("label").and_then(Node::string).and_then(
            |raw| match normalize_and_validate_label(raw) {
                Ok(label) => {
                    if label != raw {
                        mark(transformations, "field_label_trimmed");
                    }
                    Some(label)
                }
                Err(_) => None,
            },
        );
    if label.is_none() {
        mark(&mut creation_reasons, "unsupported_field_label");
    }
    let field_type = match node.get("type").and_then(Node::string) {
        Some("text") => Some("text".into()),
        Some("number") => Some("number".into()),
        Some("date") => Some("date".into()),
        Some("dropdown") => Some("choice".into()),
        _ => {
            mark(&mut reasons, "unsupported_field_kind");
            None
        }
    };
    match node.get("isRecurring") {
        None => mark(transformations, "recurrence_not_supplied"),
        Some(Node::Bool(false)) => {}
        Some(Node::Bool(true)) => mark(&mut reasons, "recurring_date_unsupported"),
        _ => mark(&mut reasons, "unsupported_recurring_flag"),
    }
    for (key, reason) in [
        ("orderWeight", "order_weight_not_imported"),
        ("hideIfEmpty", "hide_if_empty_not_imported"),
    ] {
        if node.get(key).is_some() {
            mark(transformations, reason);
        }
    }
    let mut choices = Vec::new();
    if field_type.as_deref() == Some("choice") {
        match node.get("choices").and_then(Node::array) {
            Some(items) if !items.is_empty() => {
                if items.len() > MAX_NEW_FIELD_OPTIONS {
                    mark(&mut creation_reasons, "unsupported_field_capacity");
                }
                let mut exact = BTreeSet::new();
                let mut folded = BTreeSet::new();
                for (ordinal, item) in items.iter().enumerate() {
                    let raw = item.string().map(str::to_owned);
                    let mut choice_reasons = Vec::new();
                    let label = if let Some(raw) = raw.as_deref() {
                        if !exact.insert(raw.to_owned()) {
                            mark(&mut reasons, "duplicate_source_choice");
                        }
                        // This detects definite source collisions. Native
                        // collation equivalence must additionally be checked
                        // under the destination namespace lock by the caller.
                        if !folded.insert(raw.trim().to_lowercase()) {
                            mark(&mut reasons, "colliding_source_choices");
                        }
                        match normalize_and_validate_label(raw) {
                            Ok(label) => {
                                if label != raw {
                                    mark(transformations, "choice_label_trimmed");
                                }
                                Some(label)
                            }
                            Err(_) => {
                                mark(&mut choice_reasons, "unsupported_option_label");
                                mark(&mut creation_reasons, "unsupported_option_label");
                                None
                            }
                        }
                    } else {
                        mark(&mut choice_reasons, "unsupported_choice_shape");
                        mark(&mut reasons, "unsupported_field_options");
                        None
                    };
                    choices.push(ChoiceInput {
                        ordinal: ordinal as u32,
                        raw,
                        label,
                        reasons: choice_reasons,
                    });
                }
            }
            _ => mark(&mut reasons, "unsupported_field_options"),
        }
    } else {
        match node.get("choices") {
            None | Some(Node::Null) => {}
            Some(Node::Array(items)) if items.is_empty() => {}
            _ => mark(&mut reasons, "unsupported_field_options"),
        }
    }
    reasons.sort();
    creation_reasons.sort();
    FieldInput {
        name,
        label,
        field_type,
        choices,
        reasons,
        creation_reasons,
    }
}

fn disposition(disposition: &str) -> ValueInput {
    ValueInput {
        disposition: disposition.into(),
        value: None,
        reasons: Vec::new(),
        transformations: Vec::new(),
    }
}

fn held(reason: &str) -> ValueInput {
    let mut result = disposition("held");
    mark(&mut result.reasons, reason);
    result
}

pub(crate) fn extract_value(record: &Record, field: &FieldInput) -> ValueInput {
    if !matches!(record.entity, Entity::Person(_)) {
        return held("source_not_person");
    }
    if !record.reasons.is_empty() {
        let mut result = disposition("held");
        result.reasons.clone_from(&record.reasons);
        return result;
    }
    let Some(name) = field.name.as_deref().filter(|name| !name.trim().is_empty()) else {
        return held("unsupported_field_key");
    };
    let Some(raw) = record.provenance.get(name) else {
        return disposition("not_supplied");
    };
    let node = match JsonParser::parse(raw.as_bytes()) {
        Ok(node) => node,
        Err(_) => return held("source_value_malformed"),
    };
    if matches!(node, Node::Null) {
        return disposition("source_null");
    }
    // Missing and null remain explicit no-op evidence even when a definition is
    // held. Every supplied non-null value depends on its qualified definition.
    if !field.reasons.is_empty() {
        let mut result = disposition("held");
        result.reasons.clone_from(&field.reasons);
        return result;
    }
    let mut result = disposition("eligible");
    result.value = Some(match (field.field_type.as_deref(), node) {
        (Some("text"), Node::String(raw)) => match validate_text_value(&raw) {
            Ok(value) => {
                if value != raw {
                    mark(&mut result.transformations, "text_value_trimmed");
                }
                NativeValue::Text(value)
            }
            Err(_) => return held("unsupported_text_value"),
        },
        (Some("number"), Node::Number(raw)) => match exact_decimal(&raw) {
            Some(value) => NativeValue::Number(value),
            None => return held("unsupported_number_precision"),
        },
        (Some("date"), Node::String(raw)) => {
            if exact_date(&raw).is_none() {
                return held("unsupported_date_value");
            }
            NativeValue::Date(raw)
        }
        (Some("choice"), Node::String(raw)) => {
            if !field
                .choices
                .iter()
                .any(|choice| choice.raw.as_deref() == Some(raw.as_str()))
            {
                return held("unsupported_choice_value");
            }
            NativeValue::Choice(raw)
        }
        (None, _) => return held("unsupported_field_kind"),
        _ => return held("unsupported_value_shape"),
    });
    result
}

fn exact_date(raw: &str) -> Option<NaiveDate> {
    if raw.len() != 10
        || !raw.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return None;
    }
    let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    validate_date_range(date).ok()?;
    Some(date)
}

/// Input is the lossless parser's coefficient/exponent form. Bound digit counts
/// before expanding anything: both a billion-digit exponent and a long source
/// coefficient fail without an allocation proportional to their numeric value.
fn exact_decimal(raw: &str) -> Option<String> {
    if raw == "0" {
        return Some("0".into());
    }
    let (coefficient, exponent) = raw.split_once('e')?;
    let unsigned = coefficient.strip_prefix('-').unwrap_or(coefficient);
    if unsigned.is_empty()
        || unsigned.len() > 19
        || !unsigned.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let exponent = exponent.parse::<i64>().ok()?;
    if !(-4..=15).contains(&exponent) {
        return None;
    }
    let point = i64::try_from(unsigned.len()).ok()?.checked_add(exponent)?;
    if point > 15 {
        return None;
    }
    let mut value = String::with_capacity(21);
    if coefficient.starts_with('-') {
        value.push('-');
    }
    if point <= 0 {
        value.push_str("0.");
        value.push_str(&"0".repeat(usize::try_from(-point).ok()?));
        value.push_str(unsigned);
    } else if point >= unsigned.len() as i64 {
        value.push_str(unsigned);
        value.push_str(&"0".repeat(usize::try_from(exponent).ok()?));
    } else {
        let point = usize::try_from(point).ok()?;
        value.push_str(&unsigned[..point]);
        value.push('.');
        value.push_str(&unsigned[point..]);
    }
    validate_number_pattern(&value).ok()?;
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn person_record(properties: &str) -> Record {
        extract_page(
            Stream::People,
            format!(r#"{{"people":[{{"id":1{properties}}}]}}"#).as_bytes(),
        )
        .unwrap()
        .remove(0)
    }

    fn field_record(value: serde_json::Value) -> Record {
        extract_page(
            Stream::CustomFields,
            &serde_json::to_vec(&json!({"customfields": [value]})).unwrap(),
        )
        .unwrap()
        .remove(0)
    }

    fn definition(value: serde_json::Value) -> FieldInput {
        let Entity::Field(field) = field_record(value).entity else {
            panic!("field fixture must produce field input");
        };
        field
    }

    fn typed_field(kind: &str) -> FieldInput {
        definition(json!({"id":2,"name":"customValue","label":"Value","type":kind}))
    }

    fn native(value: &ValueInput) -> Option<&str> {
        match value.value.as_ref()? {
            NativeValue::Text(value)
            | NativeValue::Number(value)
            | NativeValue::Date(value)
            | NativeValue::Choice(value) => Some(value),
        }
    }

    #[test]
    fn canonical_evidence_keeps_exact_numbers_unknown_fields_and_marker_objects() {
        let record = extract_page(Stream::People, br#"{"people":[{"id":18446744073709551617,"unknown":123456789012345678901234567890,"customValue":{"_lossless_number":"1e0"}}]}"#)
            .unwrap().remove(0);
        assert_eq!(record.source_id.as_deref(), Some("18446744073709551617"));
        assert_eq!(
            record.provenance["unknown"],
            "12345678901234567890123456789e1"
        );
        assert_eq!(
            record.provenance["customValue"],
            r#"{"_lossless_number":"1e0"}"#
        );
        assert_eq!(
            extract_value(&record, &typed_field("number")).disposition,
            "held"
        );
        let restored: Record =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(restored.canonical, record.canonical);
        assert_eq!(restored.provenance, record.provenance);
    }

    #[test]
    fn duplicate_keys_and_invalid_shape_fail_closed() {
        assert!(matches!(
            extract_page(
                Stream::People,
                br#"{"people":[{"id":1,"tags":[],"ta\u0067s":[]}]}"#
            ),
            Err(ParseError::DuplicateKey)
        ));
        assert!(matches!(
            extract_page(Stream::Users, br#"{"users":[]}"#),
            Err(ParseError::Malformed)
        ));
        assert!(matches!(
            extract_page(Stream::People, br#"{"people":{}}"#),
            Err(ParseError::Malformed)
        ));
        let record = extract_page(Stream::People, br#"{"people":[false]}"#)
            .unwrap()
            .remove(0);
        assert!(matches!(record.entity, Entity::Invalid));
        assert!(record
            .reasons
            .iter()
            .any(|reason| reason == "invalid_source_id"));
    }

    #[test]
    fn page_bounds_apply_before_deriving_metadata() {
        let raw = vec![b' '; MAX_RAW_BYTES + 1];
        assert!(matches!(
            extract_page(Stream::People, &raw),
            Err(ParseError::DerivedTooLarge)
        ));
        let raw = serde_json::to_vec(&json!({"people": vec![json!({"id":1}); 101]})).unwrap();
        assert!(matches!(
            extract_page(Stream::People, &raw),
            Err(ParseError::DerivedTooLarge)
        ));
        let too_long_id = "1".repeat(129);
        let record = field_record(
            json!({"id":too_long_id,"name":"customValue","label":"Value","type":"text"}),
        );
        assert!(record.source_id.is_none());
    }

    #[test]
    fn tag_collection_absence_null_empty_and_bad_shape_remain_distinct() {
        for (properties, state) in [
            ("", "not_supplied"),
            (r#", "tags":null"#, "source_null"),
            (r#", "tags":[]"#, "empty"),
            (r#", "tags":{}"#, "held"),
        ] {
            let record = person_record(properties);
            let Entity::Person(person) = record.entity else {
                panic!("person fixture");
            };
            assert_eq!(person.tags_state, state);
            assert!(person.tags.is_empty());
            assert!(record.reasons.is_empty());
        }
    }

    #[test]
    fn tag_items_preserve_ordinals_duplicates_and_independent_holds() {
        let record = person_record(r#", "tags":[" Buyer ",false,"",null,"Buyer","bad\u0000name"]"#);
        let Entity::Person(person) = record.entity else {
            panic!("person fixture");
        };
        assert_eq!(person.tags.len(), 6);
        assert_eq!(person.tags[0].raw.as_deref(), Some(" Buyer "));
        assert_eq!(person.tags[0].label.as_deref(), Some("Buyer"));
        assert_eq!(person.tags[4].ordinal, 4);
        assert_eq!(person.tags[4].label.as_deref(), Some("Buyer"));
        assert!(person.tags[1].raw.is_none());
        assert_eq!(person.tags[1].reasons, ["unsupported_tag_shape"]);
        assert_eq!(person.tags[5].reasons, ["unsupported_tag_label"]);
        assert!(record
            .transformations
            .iter()
            .any(|reason| reason == "tag_label_trimmed"));
        let tags: Vec<String> = (0..201).map(|n| format!("Tag {n}")).collect();
        let raw = serde_json::to_vec(&json!({"people":[{"id":1,"tags":tags}]})).unwrap();
        let Entity::Person(person) = extract_page(Stream::People, &raw).unwrap().remove(0).entity
        else {
            panic!("person fixture");
        };
        assert_eq!(person.tags.len(), 201);
        assert!(person.tags.iter().all(|tag| tag.reasons.is_empty()));
    }

    #[test]
    fn unrepresentable_native_key_and_label_only_prevent_creation() {
        for key in ["x".repeat(2049), "é".repeat(1025), "custom\0Value".into()] {
            let field = definition(json!({"id":2,"name":key,"label":"x".repeat(61),"type":"text"}));
            assert_eq!(field.name.as_deref(), Some(key.as_str()));
            assert!(field.reasons.is_empty());
            assert!(field
                .creation_reasons
                .iter()
                .any(|reason| reason == "unsupported_field_label"));
            let raw = serde_json::to_vec(&json!({"people":[{"id":1,(key):"retained"}]})).unwrap();
            let result = extract_value(
                &extract_page(Stream::People, &raw).unwrap().remove(0),
                &field,
            );
            assert_eq!(native(&result), Some("retained"));
        }
        let field =
            definition(json!({"id":2,"name":"x".repeat(2048),"label":"Value","type":"text"}));
        assert!(field.creation_reasons.is_empty());
        for name in [json!(null), json!(""), json!(" \t"), json!(7)] {
            let field = definition(json!({"id":2,"name":name,"label":"Value","type":"text"}));
            assert_eq!(field.reasons, ["unsupported_field_key"]);
        }
    }

    #[test]
    fn choices_retain_exact_order_and_invalid_native_labels_can_map() {
        let long = "x".repeat(61);
        let field = definition(
            json!({"id":2,"name":"customValue","label":"Value","type":"dropdown","choices":["First",long," Last "]}),
        );
        assert!(field.reasons.is_empty());
        assert_eq!(field.choices[1].raw.as_deref(), Some(long.as_str()));
        assert!(field.choices[1].label.is_none());
        assert_eq!(field.choices[2].ordinal, 2);
        assert_eq!(field.choices[2].label.as_deref(), Some("Last"));
        let record = person_record(&format!(
            ",\"customValue\":{}",
            serde_json::to_string(&long).unwrap()
        ));
        assert_eq!(native(&extract_value(&record, &field)), Some(long.as_str()));
        let many: Vec<String> = (0..51).map(|i| format!("Choice {i}")).collect();
        let field = definition(
            json!({"id":2,"name":"customValue","label":"Value","type":"dropdown","choices":many}),
        );
        assert!(field.reasons.is_empty());
        assert_eq!(field.choices.len(), 51);
        assert_eq!(field.creation_reasons, ["unsupported_field_capacity"]);
    }

    #[test]
    fn duplicate_and_case_colliding_choices_hold_the_entire_field() {
        for choices in [
            json!(["A", "A"]),
            json!(["A", "a"]),
            json!([" A ", "A"]),
            json!(["A", null]),
            json!([]),
            json!(null),
        ] {
            let field = definition(
                json!({"id":2,"name":"customValue","label":"Value","type":"dropdown","choices":choices}),
            );
            assert!(!field.reasons.is_empty());
            let result = extract_value(&person_record(r#", "customValue":"A""#), &field);
            assert_eq!(result.disposition, "held");
        }
    }

    #[test]
    fn declared_null_like_choices_are_literals_and_require_exact_matching() {
        let field = definition(
            json!({"id":2,"name":"customValue","label":"Value","type":"dropdown","choices":["None","N/A","null"]}),
        );
        for literal in ["None", "N/A", "null"] {
            let record = person_record(&format!(",\"customValue\":\"{literal}\""));
            assert_eq!(native(&extract_value(&record, &field)), Some(literal));
        }
        for raw in [r#""none""#, r#"" null ""#, r#"["None"]"#, "false"] {
            assert_eq!(
                extract_value(&person_record(&format!(",\"customValue\":{raw}")), &field)
                    .disposition,
                "held"
            );
        }
    }

    #[test]
    fn numbers_preserve_exact_value_and_reject_rounding_and_exponent_blowup() {
        let field = typed_field("number");
        for (source, expected) in [
            ("999999999999999.9999", "999999999999999.9999"),
            ("1e-4", "0.0001"),
            ("1.2300", "1.23"),
            ("1e14", "100000000000000"),
            ("-0.0000", "0"),
            ("-12.34", "-12.34"),
        ] {
            let result = extract_value(
                &person_record(&format!(",\"customValue\":{source}")),
                &field,
            );
            assert_eq!(native(&result), Some(expected));
        }
        for raw in [
            "1.00001",
            "1e15",
            "1e1000000000",
            "1e-1000000000",
            "99999999999999999999",
            r#""12.34""#,
            "true",
        ] {
            assert_eq!(
                extract_value(&person_record(&format!(",\"customValue\":{raw}")), &field)
                    .disposition,
                "held"
            );
        }
    }

    #[test]
    fn dates_require_exact_calendar_shape_and_native_range() {
        let field = typed_field("date");
        for raw in ["1900-01-01", "2200-12-31", "2024-02-29"] {
            let result = extract_value(
                &person_record(&format!(",\"customValue\":\"{raw}\"")),
                &field,
            );
            assert_eq!(native(&result), Some(raw));
        }
        for raw in [
            "1899-12-31",
            "2201-01-01",
            "2023-02-29",
            "2024-2-029",
            " 2024-01-01",
            "2024-01-01T00:00:00Z",
            "2024-01-01 ",
        ] {
            assert_eq!(
                extract_value(
                    &person_record(&format!(",\"customValue\":\"{raw}\"")),
                    &field
                )
                .disposition,
                "held"
            );
        }
    }

    #[test]
    fn recurrence_is_held_or_explicitly_omitted_without_importing_behavior() {
        for recurrence in [json!(true), json!("false"), json!(null)] {
            let field = definition(
                json!({"id":2,"name":"customValue","label":"Value","type":"date","isRecurring":recurrence}),
            );
            assert!(!field.reasons.is_empty());
            assert_eq!(
                extract_value(&person_record(r#", "customValue":"2000-01-01""#), &field)
                    .disposition,
                "held"
            );
            assert_eq!(
                extract_value(&person_record(""), &field).disposition,
                "not_supplied"
            );
            assert_eq!(
                extract_value(&person_record(r#", "customValue":null"#), &field).disposition,
                "source_null"
            );
        }
        let record = field_record(
            json!({"id":2,"name":"customValue","label":"Value","type":"date","hideIfEmpty":true,"orderWeight":3}),
        );
        assert!(record
            .transformations
            .iter()
            .any(|reason| reason == "recurrence_not_supplied"));
        assert!(record
            .transformations
            .iter()
            .any(|reason| reason == "hide_if_empty_not_imported"));
        let field = definition(
            json!({"id":2,"name":"customValue","label":"Value","type":"date","isRecurring":false}),
        );
        assert!(field.reasons.is_empty());
    }

    #[test]
    fn text_trimming_is_disclosed_without_converting_missing_null_or_empty() {
        let field = typed_field("text");
        let result = extract_value(
            &person_record(r#", "customValue":" \t value \r\n""#),
            &field,
        );
        assert_eq!(native(&result), Some("value"));
        assert_eq!(result.transformations, ["text_value_trimmed"]);
        assert_eq!(
            extract_value(&person_record(""), &field).disposition,
            "not_supplied"
        );
        assert_eq!(
            extract_value(&person_record(r#", "customValue":null"#), &field).disposition,
            "source_null"
        );
        for raw in [
            r#""""#,
            r#"" \t ""#,
            r#""bad\u0000value""#,
            r#""bad\nvalue""#,
            "7",
            "false",
            "[]",
            "{}",
        ] {
            assert_eq!(
                extract_value(&person_record(&format!(",\"customValue\":{raw}")), &field)
                    .disposition,
                "held"
            );
        }
        let result = extract_value(&person_record(r#", "customValue":"null""#), &field);
        assert_eq!(native(&result), Some("null"));
    }

    #[test]
    fn source_holds_and_unsupported_definition_shapes_do_not_become_values() {
        let field = typed_field("text");
        let mut record = person_record(r#", "customValue":"value", "showContent":false"#);
        assert_eq!(
            extract_value(&record, &field).reasons,
            ["content_inaccessible"]
        );
        record.reasons.clear();
        record.provenance.insert("customValue".into(), "{".into());
        assert_eq!(
            extract_value(&record, &field).reasons,
            ["source_value_malformed"]
        );
        let field =
            definition(json!({"id":2,"name":"customValue","label":"Value","type":"boolean"}));
        assert_eq!(field.reasons, ["unsupported_field_kind"]);
        let field = definition(
            json!({"id":2,"name":"customValue","label":"Value","type":"text","choices":["unexpected"]}),
        );
        assert_eq!(field.reasons, ["unsupported_field_options"]);
    }

    #[test]
    fn enums_use_the_frozen_tagged_wire_shape() {
        assert_eq!(
            serde_json::to_value(NativeValue::Number("12.34".into())).unwrap(),
            json!({"kind":"number","value":"12.34"})
        );
        assert_eq!(
            serde_json::to_value(Entity::Invalid).unwrap(),
            json!({"kind":"invalid"})
        );
        assert_eq!(
            serde_json::to_value(person_record("").entity).unwrap()["kind"],
            "person"
        );
    }
}

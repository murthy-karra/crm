//! Bounded review text from a retained, qualified import record.
//!
//! These values are display-only. Source values remain strings, including exact
//! canonical numeric text; callers must serialize them as JSON and clients must
//! render them as text. This module never turns source text into HTML or a URL.
//! Authorization, response-page budgets and authenticated cursor scope belong
//! to the caller. Field keys identify review fields, never grant access to them.
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::import_source::{ContactInput, Entity, ExtractedRecord};

const MAX_DISPLAY_BYTES: usize = 1024;
const MAX_DISPLAY_ENTRIES: usize = 20;
const MIN_SEGMENT_BYTES: usize = 4;
const MAX_SEGMENT_BYTES: usize = 65_536;

#[derive(Serialize)]
pub(crate) struct Segment {
    pub text: String,
    pub offset: String,
    pub full_utf8_bytes: String,
    pub next_offset: Option<String>,
}

/// At most 1024 original UTF-8 bytes AND 1024 JSON-escaped content bytes.
/// The second bound prevents control characters from expanding a display page
/// sixfold. A prefix may be shorter than 1024 bytes; its metadata says so.
fn prefix(text: &str) -> &str {
    let mut end = 0;
    let mut escaped_bytes = 0;
    for character in text.chars() {
        let width = character.len_utf8();
        let escaped_width = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{0008}' | '\u{000c}' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            _ => width,
        };
        if end + width > MAX_DISPLAY_BYTES || escaped_bytes + escaped_width > MAX_DISPLAY_BYTES {
            break;
        }
        end += width;
        escaped_bytes += escaped_width;
    }
    &text[..end]
}

fn display(text: &str, field_key: &str) -> Value {
    let value = prefix(text);
    json!({
        "value": value,
        "abbreviated": value.len() < text.len(),
        "full_utf8_bytes": text.len().to_string(),
        "field_key": field_key,
    })
}

fn optional_display(text: Option<&str>, field_key: &str) -> Value {
    text.map_or(Value::Null, |text| display(text, field_key))
}

fn source_field(key: &str) -> String {
    use std::fmt::Write;
    let mut field = String::with_capacity(71);
    field.push_str("source.");
    for byte in Sha256::digest(key.as_bytes()) {
        write!(&mut field, "{byte:02x}").expect("writing to a String cannot fail");
    }
    field
}

fn contact_display(contact: &ContactInput) -> Value {
    json!({
        "kind": prefix(&contact.kind),
        "value": prefix(&contact.value),
        "normalized_value": prefix(&contact.normalized_value),
        "import_order": contact.import_order,
    })
}

fn contacts_display(contacts: &[ContactInput]) -> Value {
    let entries: Vec<_> = contacts
        .iter()
        .take(MAX_DISPLAY_ENTRIES)
        .map(contact_display)
        .collect();
    let abbreviated = contacts.len() > MAX_DISPLAY_ENTRIES
        || contacts.iter().take(MAX_DISPLAY_ENTRIES).any(|contact| {
            [&contact.kind, &contact.value, &contact.normalized_value]
                .into_iter()
                .any(|text| prefix(text).len() < text.len())
        });
    json!({
        "entries": entries,
        "total_count": contacts.len().to_string(),
        "abbreviated": abbreviated,
        "field_key": "contacts",
    })
}

fn provenance_display(record: &ExtractedRecord) -> Value {
    let fields: Vec<_> = record
        .provenance
        .iter()
        .take(MAX_DISPLAY_ENTRIES)
        .map(|(key, text)| {
            let value = prefix(text);
            let label = prefix(key);
            json!({
                "field_key": source_field(key),
                "label": label,
                "label_abbreviated": label.len() < key.len(),
                "label_full_utf8_bytes": key.len().to_string(),
                "value": value,
                "abbreviated": value.len() < text.len(),
                "full_utf8_bytes": text.len().to_string(),
            })
        })
        .collect();
    let abbreviated = record.provenance.len() > MAX_DISPLAY_ENTRIES
        || record
            .provenance
            .iter()
            .take(MAX_DISPLAY_ENTRIES)
            .any(|(key, text)| prefix(key).len() < key.len() || prefix(text).len() < text.len());
    json!({
        "fields": fields,
        "total_count": record.provenance.len().to_string(),
        "abbreviated": abbreviated,
        "field_key": "provenance",
    })
}

pub(crate) fn summary(record: &ExtractedRecord) -> Value {
    let mut result = match &record.entity {
        Entity::People(person) => json!({
            "family": "people",
            "first_name": optional_display(person.first_name.as_deref(), "first_name"),
            "last_name": optional_display(person.last_name.as_deref(), "last_name"),
            "stage_label": optional_display(person.stage_label.as_deref(), &source_field("stage")),
            // Qualified assignee keys are at most 128 digits plus "pond:".
            // Display through the same bound even for a malformed stored input.
            "assignee_key": person.assignee_key.as_deref().map(prefix),
            "contacts": contacts_display(&person.contacts),
        }),
        Entity::Stage(stage) => json!({
            "family": "stage",
            "label": optional_display(stage.label.as_deref(), &source_field("name")),
            "can_create": stage.can_create,
        }),
        Entity::User(user) => json!({
            "family": "user",
            "name": optional_display(user.name.as_deref(), &source_field("name")),
            "email": optional_display(user.email.as_deref(), &source_field("email")),
            "is_pond": user.is_pond,
        }),
        Entity::Invalid => json!({"family": "invalid"}),
    };
    result["provenance"] = provenance_display(record);
    result
}

/// The allowlist deliberately exposes individual record fields, never raw
/// captures. Hashing original keys gives even very long keys a bounded route.
/// Canonical provenance values are serialized as strings and never re-parsed.
pub(crate) fn field_text(record: &ExtractedRecord, field: &str) -> Option<String> {
    match field {
        "first_name" => match &record.entity {
            Entity::People(person) => person.first_name.clone(),
            _ => None,
        },
        "last_name" => match &record.entity {
            Entity::People(person) => person.last_name.clone(),
            _ => None,
        },
        "contacts" => match &record.entity {
            Entity::People(person) => serde_json::to_string(&person.contacts).ok(),
            _ => None,
        },
        "provenance" => serde_json::to_string(&record.provenance).ok(),
        _ if field.len() == 71 && field.starts_with("source.") => record
            .provenance
            .iter()
            .find(|(key, _)| source_field(key) == field)
            .map(|(_, text)| text.clone()),
        _ => None,
    }
}

pub(crate) fn segment(text: &str, offset: usize, limit: usize) -> Result<Segment, &'static str> {
    if !(MIN_SEGMENT_BYTES..=MAX_SEGMENT_BYTES).contains(&limit) {
        return Err("invalid_limit");
    }
    if !text.is_char_boundary(offset) {
        return Err("invalid_offset");
    }
    let mut end = offset.saturating_add(limit).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // Every UTF-8 scalar occupies at most four bytes and the minimum limit is
    // four. A valid non-EOF offset therefore always consumes at least one scalar.
    debug_assert!(end > offset || offset == text.len());
    Ok(Segment {
        text: text[offset..end].to_owned(),
        offset: offset.to_string(),
        full_utf8_bytes: text.len().to_string(),
        next_offset: (end < text.len()).then(|| end.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::migration::{
        import_source::{extract_page, PersonInput},
        snapshot_source::Stream,
    };

    fn person(
        contacts: Vec<ContactInput>,
        provenance: BTreeMap<String, String>,
    ) -> ExtractedRecord {
        ExtractedRecord {
            source_id: Some("9007199254740993".into()),
            canonical: Vec::new(),
            entity: Entity::People(PersonInput {
                first_name: Some("Ada".into()),
                last_name: None,
                contacts,
                stage_label: None,
                assignee_key: None,
            }),
            reasons: Vec::new(),
            transformations: Vec::new(),
            provenance,
        }
    }

    fn reconstruct(text: &str, limit: usize) -> String {
        let mut output = String::new();
        let mut offset = 0;
        loop {
            let part = segment(text, offset, limit).unwrap();
            assert_eq!(part.offset, offset.to_string());
            assert_eq!(part.full_utf8_bytes, text.len().to_string());
            assert!(part.text.len() <= limit);
            output.push_str(&part.text);
            let Some(next) = part.next_offset else {
                break;
            };
            let next: usize = next.parse().unwrap();
            assert!(next > offset);
            assert_eq!(next - offset, part.text.len());
            offset = next;
        }
        output
    }

    #[test]
    fn import_display_segments_progress_and_reconstruct_exact_utf8() {
        let text = "🙂é中a\u{0000}\"\\\n𐐀".repeat(5000);
        for limit in [4, 5, 7, 1024, 65_536] {
            assert_eq!(reconstruct(&text, limit), text);
        }
        let part = segment("a🙂b", 0, 4).unwrap();
        assert_eq!(part.text, "a");
        assert_eq!(part.next_offset.as_deref(), Some("1"));
        assert_eq!(segment("a🙂b", 1, 4).unwrap().text, "🙂");
    }

    #[test]
    fn import_display_segments_reject_invalid_limits_and_offsets_and_allow_eof() {
        for limit in [0, 1, 3, 65_537, usize::MAX] {
            assert_eq!(segment("🙂", 0, limit).err(), Some("invalid_limit"));
        }
        for offset in [1, 2, 3, 5, usize::MAX] {
            assert_eq!(segment("🙂", offset, 4).err(), Some("invalid_offset"));
        }
        for text in ["", "🙂"] {
            let end = segment(text, text.len(), 4).unwrap();
            assert!(end.text.is_empty());
            assert!(end.next_offset.is_none());
            assert_eq!(end.offset, text.len().to_string());
        }
    }

    #[test]
    fn import_display_arrays_and_escaped_strings_have_a_strict_summary_bound() {
        let expansion = "\u{0000}".repeat(2000);
        let contacts = (0..100)
            .map(|import_order| ContactInput {
                kind: expansion.clone(),
                value: expansion.clone(),
                normalized_value: expansion.clone(),
                import_order,
            })
            .collect();
        let provenance = (0..100)
            .map(|index| (format!("{index:03}{expansion}"), expansion.clone()))
            .collect();
        let mut record = person(contacts, provenance);
        if let Entity::People(person) = &mut record.entity {
            person.first_name = Some(expansion.clone());
            person.last_name = Some(expansion.clone());
            person.stage_label = Some(expansion.clone());
            person.assignee_key = Some(expansion);
        }
        let display = summary(&record);
        assert!(serde_json::to_vec(&display).unwrap().len() < 128 * 1024);
        assert_eq!(display["contacts"]["entries"].as_array().unwrap().len(), 20);
        assert_eq!(display["contacts"]["total_count"], "100");
        assert_eq!(display["contacts"]["abbreviated"], true);
        assert_eq!(
            display["provenance"]["fields"].as_array().unwrap().len(),
            20
        );
        assert_eq!(display["provenance"]["total_count"], "100");
        assert_eq!(display["provenance"]["abbreviated"], true);
        let source_field = &display["provenance"]["fields"][0];
        assert_eq!(source_field["label_abbreviated"], true);
        assert_eq!(source_field["label_full_utf8_bytes"], "2003");
        let key = source_field["field_key"].as_str().unwrap();
        assert_eq!(key.len(), 71);
        assert_eq!(field_text(&record, key).unwrap().len(), 2000);
        let full_contacts: Vec<ContactInput> =
            serde_json::from_str(&field_text(&record, "contacts").unwrap()).unwrap();
        assert_eq!(full_contacts.len(), 100);
        assert_eq!(full_contacts[99].value.len(), 2000);
        let full_provenance: BTreeMap<String, String> =
            serde_json::from_str(&field_text(&record, "provenance").unwrap()).unwrap();
        assert_eq!(full_provenance, record.provenance);
    }

    #[test]
    fn import_display_clipping_is_reported_even_without_omitted_entries() {
        let long = "🙂".repeat(300);
        let record = person(
            vec![ContactInput {
                kind: "email".into(),
                value: long.clone(),
                normalized_value: "small@example.test".into(),
                import_order: 0,
            }],
            BTreeMap::from([("note".into(), long.clone())]),
        );
        let display = summary(&record);
        assert_eq!(display["contacts"]["total_count"], "1");
        assert_eq!(display["contacts"]["abbreviated"], true);
        assert_eq!(
            display["contacts"]["entries"][0]["value"]
                .as_str()
                .unwrap()
                .len(),
            1024
        );
        assert_eq!(display["provenance"]["total_count"], "1");
        assert_eq!(display["provenance"]["abbreviated"], true);
        assert_eq!(
            display["provenance"]["fields"][0]["full_utf8_bytes"],
            "1200"
        );
        assert_eq!(field_text(&record, &source_field("note")), Some(long));
        assert_eq!(display["first_name"]["abbreviated"], false);
        assert_eq!(display["first_name"]["full_utf8_bytes"], "3");
        assert!(display["last_name"].is_null());
    }

    #[test]
    fn import_display_source_numbers_and_untrusted_content_remain_exact_text() {
        let raw = br#"{"people":[{"id":9007199254740993,"firstName":"<img src=x onerror=alert(1)>","arbitrary":123456789012345678901234567890.123456789,"unknown":{"_lossless_number":"untrusted"}}]}"#;
        let record = extract_page(Stream::People, raw).unwrap().remove(0);
        let display = summary(&record);
        assert_eq!(
            display["first_name"]["value"],
            "<img src=x onerror=alert(1)>"
        );
        for key in ["id", "arbitrary", "unknown"] {
            let field = field_text(&record, &source_field(key)).unwrap();
            assert_eq!(field, record.provenance[key]);
            assert_eq!(reconstruct(&field, 4), field);
        }
        assert_eq!(
            field_text(&record, &source_field("id")).unwrap(),
            "9007199254740993e0"
        );
        let full: BTreeMap<String, String> =
            serde_json::from_str(&field_text(&record, "provenance").unwrap()).unwrap();
        assert_eq!(full["arbitrary"], record.provenance["arbitrary"]);
        assert_eq!(full["unknown"], r#"{"_lossless_number":"untrusted"}"#);
        for unknown in ["raw", "canonical", "source.id", "firstName", "source."] {
            assert!(field_text(&record, unknown).is_none());
        }
    }

    #[test]
    fn import_display_stage_and_user_fields_link_to_full_source_text() {
        for (stream, collection, core_key, source_key) in [
            (Stream::Stages, "stages", "label", "name"),
            (Stream::Users, "users", "name", "name"),
            (Stream::Users, "users", "email", "email"),
        ] {
            let long = "é".repeat(1100);
            let raw = json!({collection: [{"id": 1, source_key: long}]}).to_string();
            let record = extract_page(stream, raw.as_bytes()).unwrap().remove(0);
            let display = summary(&record);
            assert_eq!(display[core_key]["abbreviated"], true);
            assert_eq!(display[core_key]["full_utf8_bytes"], "2200");
            let field = display[core_key]["field_key"].as_str().unwrap();
            assert_eq!(field, source_field(source_key));
            assert_eq!(
                field_text(&record, field).unwrap(),
                record.provenance[source_key]
            );
            assert!(field_text(&record, "contacts").is_none());
        }
    }

    #[test]
    fn import_display_full_large_fields_survive_segmented_inspection() {
        let source = "中".repeat(700_000);
        let record = person(
            Vec::new(),
            BTreeMap::from([("large".into(), source.clone())]),
        );
        let display = summary(&record);
        assert_eq!(
            display["provenance"]["fields"][0]["full_utf8_bytes"],
            "2100000"
        );
        assert!(serde_json::to_vec(&display).unwrap().len() < 4096);
        let full = field_text(&record, &source_field("large")).unwrap();
        assert_eq!(reconstruct(&full, 65_536), source);
        let all = field_text(&record, "provenance").unwrap();
        let decoded: BTreeMap<String, String> =
            serde_json::from_str(&reconstruct(&all, 65_536)).unwrap();
        assert_eq!(decoded["large"], source);
    }
}

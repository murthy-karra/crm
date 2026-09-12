//! Closed retained plan data and bounded display; no source content is Debug.
use super::{
    metadata_source::{FieldInput, NativeValue},
    metadata_store as s, MigrationError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Dispositions {
    pub planned: i64,
    pub eligible: i64,
    pub created: i64,
    pub applied: i64,
    pub already_present: i64,
    pub held: i64,
    pub not_supplied: i64,
    pub source_null: i64,
    pub pending: i64,
}
impl Dispositions {
    pub fn add(&mut self, disposition: &str, amount: i64) {
        match disposition {
            "eligible" => self.eligible += amount,
            "created" => self.created += amount,
            "applied" => self.applied += amount,
            "already_present" => self.already_present += amount,
            "held" => self.held += amount,
            "not_supplied" => self.not_supplied += amount,
            "source_null" => self.source_null += amount,
            "pending" => self.pending += amount,
            _ => {}
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct PeopleCounts {
    pub source: i64,
    pub eligible: i64,
    pub excluded: i64,
    pub settled: i64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Counts {
    pub people: PeopleCounts,
    pub tags: Dispositions,
    pub fields: Dispositions,
    pub options: Dispositions,
    pub tag_links: Dispositions,
    pub values: Dispositions,
    pub held_count: i64,
    pub invalid_source_ids: i64,
}
impl Counts {
    pub fn family(&mut self, kind: &str) -> &mut Dispositions {
        match kind {
            "tag" => &mut self.tags,
            "field" => &mut self.fields,
            "option" => &mut self.options,
            "tag_link" => &mut self.tag_links,
            _ => &mut self.values,
        }
    }
    pub fn planned(&mut self, kind: &str, disposition: &str) {
        let c = self.family(kind);
        c.planned += 1;
        c.pending += 1;
        c.add(disposition, 1);
        if disposition == "held" {
            self.held_count += 1
        }
    }
    pub fn outcome(&mut self, kind: &str, disposition: &str) {
        let c = self.family(kind);
        c.planned += 1;
        c.add(disposition, 1);
        if disposition == "held" {
            self.held_count += 1;
        }
    }
    pub fn settled(&mut self, kind: &str, old: &str, new: &str) {
        let c = self.family(kind);
        c.pending -= 1;
        if old != new {
            c.add(old, -1);
            c.add(new, 1);
        }
        if old == "held" && new != "held" {
            self.held_count -= 1
        } else if old != "held" && new == "held" {
            self.held_count += 1
        }
    }
    pub fn load(value: Value) -> Result<Self, MigrationError> {
        serde_json::from_value(value).map_err(|_| MigrationError::Crypto)
    }
    pub fn wire(&self) -> Value {
        {
            let mut value = decimal(serde_json::to_value(self).expect("closed counters serialize"));
            value["issues"] = json!([]);
            value
        }
    }
}
pub(crate) fn decimal(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.to_string()),
        Value::Array(a) => Value::Array(a.into_iter().map(decimal).collect()),
        Value::Object(a) => Value::Object(a.into_iter().map(|(k, v)| (k, decimal(v))).collect()),
        other => other,
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Target {
    pub id: Uuid,
    pub kind: String,
    pub field_id: Option<Uuid>,
    pub label: String,
    pub field_type: Option<String>,
    pub source: Option<String>,
    pub external_key: Option<String>,
    pub position: i32,
}
impl Target {
    pub fn wire(&self) -> Value {
        json!({"id":self.id,"kind":self.kind,"field_id":self.field_id,"label":self.label,"field_type":self.field_type,"source_bound":self.source.is_some(),"archived":false})
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Destination {
    pub tags: Vec<Target>,
    pub fields: Vec<Target>,
    pub options: Vec<Target>,
}
impl Destination {
    pub fn all(&self) -> impl Iterator<Item = &Target> {
        self.tags
            .iter()
            .chain(self.fields.iter())
            .chain(self.options.iter())
    }
    pub fn by_id(&self, id: Uuid) -> Option<&Target> {
        self.all().find(|v| v.id == id)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Mapping {
    pub source: BTreeMap<String, String>,
    pub label: Option<String>,
    pub group: Option<String>,
    pub source_name: Option<String>,
    pub field: Option<FieldInput>,
    pub raw_choice: Option<String>,
    pub choice: super::metadata::Choice,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub suggestions: Vec<Target>,
    pub target: Option<Target>,
}
impl Mapping {
    pub fn create_matching_available(&self, qualified: bool) -> bool {
        qualified
            && self.label.is_some()
            && !self
                .field
                .as_ref()
                .is_some_and(|f| !f.creation_reasons.is_empty())
            && !self.reasons.iter().any(|r| {
                r.starts_with("unsupported_")
                    || matches!(
                        r.as_str(),
                        "source_integrity"
                            | "source_field_key_collision"
                            | "colliding_source_choices"
                            | "duplicate_source_choice"
                    )
            })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Operation {
    pub id: Uuid,
    pub kind: String,
    pub source_key: Vec<u8>,
    pub source_field: Option<String>,
    pub mapping_id: Option<Uuid>,
    pub target_id: Option<Uuid>,
    pub disposition: String,
    pub value: Option<NativeValue>,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub reasons: Vec<String>,
    pub operations: Vec<Operation>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResultData {
    pub source: BTreeMap<String, String>,
    pub operations: Value,
    pub reasons: Vec<String>,
}
pub(crate) fn mark(codes: &mut Vec<String>, code: &str) {
    if !codes.iter().any(|v| v == code) {
        codes.push(code.to_owned())
    }
}

fn prefix(value: &str) -> (&str, bool) {
    let mut end = 0;
    let mut escaped = 0;
    for c in value.chars() {
        let raw = c.len_utf8();
        let cost = serde_json::to_string(&c.to_string())
            .expect("text serializes")
            .len()
            - 2;
        if end + raw > 1024 || escaped + cost > 1024 {
            break;
        }
        end += raw;
        escaped += cost;
    }
    (&value[..end], end < value.len())
}
pub(crate) fn field_key(section: &str, name: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{section}.{}", s::hex(&Sha256::digest(name.as_bytes())))
}
pub(crate) fn summary(section: &str, fields: &BTreeMap<String, String>) -> Value {
    let shown=fields.iter().take(20).map(|(k,v)|{let (label,label_cut)=prefix(k);let (text,cut)=prefix(v);json!({"key":field_key(section,k),"label":label,"label_abbreviated":label_cut,"label_full_utf8_bytes":k.len().to_string(),"text":text,"full_utf8_bytes":v.len().to_string(),"abbreviated":cut})}).collect::<Vec<_>>();
    let abbreviated = fields.len() > 20
        || shown
            .iter()
            .any(|v| v["abbreviated"] == true || v["label_abbreviated"] == true);
    json!({"fields":shown,"total_fields":fields.len().to_string(),"abbreviated":abbreviated,"field_key":format!("{section}.all")})
}
pub(crate) fn operation_fields(value: &Value) -> BTreeMap<String, String> {
    match value {
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::to_string(v).expect("JSON serializes"),
                )
            })
            .collect(),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    i.to_string(),
                    serde_json::to_string(v).expect("JSON serializes"),
                )
            })
            .collect(),
        v => BTreeMap::from([("value".into(), v.to_string())]),
    }
}
pub(crate) fn field_text(
    source: &BTreeMap<String, String>,
    operations: &Value,
    field: &str,
) -> Option<String> {
    let ops = operation_fields(operations);
    if field == "all" {
        return serde_json::to_string(&json!({"source":source,"operations":ops})).ok();
    }
    for (section, fields) in [("source", source), ("operations", &ops)] {
        if field == format!("{section}.all") {
            return serde_json::to_string(fields).ok();
        }
        if let Some((_, v)) = fields.iter().find(|(k, _)| field_key(section, k) == field) {
            return Some(v.clone());
        }
    }
    None
}
pub(crate) fn segment(
    text: &str,
    offset: usize,
    limit: usize,
) -> Result<(String, usize), MigrationError> {
    if !(4..=65536).contains(&limit) || offset > text.len() || !text.is_char_boundary(offset) {
        return Err(MigrationError::InvalidInput);
    }
    let mut end = offset.saturating_add(limit).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[offset..end].to_owned(), end))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fields_are_distinct_and_exact() {
        let source = BTreeMap::from([("number".into(), "9007199254740993e0".into())]);
        let op = json!({"number":"native"});
        assert_eq!(
            field_text(&source, &op, &field_key("source", "number")).as_deref(),
            Some("9007199254740993e0")
        );
        assert_eq!(
            field_text(&source, &op, &field_key("operations", "number")).as_deref(),
            Some("\"native\"")
        );
        assert!(field_text(&source, &op, "all")
            .unwrap()
            .contains("operations"));
    }
    #[test]
    fn display_clips_collections_and_escaping() {
        let source = (0..100)
            .map(|i| (i.to_string(), "\u{0}".repeat(10000)))
            .collect();
        let v = summary("source", &source);
        assert_eq!(v["fields"].as_array().unwrap().len(), 20);
        assert_eq!(v["abbreviated"], true);
        assert!(s::bytes(&v).unwrap().len() < 128 * 1024);
    }
    #[test]
    fn segments_advance_on_scalar_boundaries() {
        assert_eq!(segment("🙂🙂x", 0, 4).unwrap(), ("🙂".into(), 4));
        assert!(segment("🙂", 1, 4).is_err());
        assert!(segment("🙂", 0, 1).is_err());
    }
}

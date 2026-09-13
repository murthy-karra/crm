//! Lossless, content-free comparison data. Source bodies never implement Debug.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    crypto,
    snapshot_source::{positive_id, JsonParser, Node, ParseError, Stream},
};
use crate::{config::RawPayloadKey, ids::OrganizationId};

pub const ENGINE: &str = "fub-core-change-v1";
pub const MAX_RAW: usize = 16 * 1024 * 1024;
pub const MAX_OBSERVATIONS: usize = 100;
pub const FAMILIES: [&str; 6] = [
    "people",
    "users",
    "stages",
    "custom_fields",
    "notes",
    "tasks",
];
pub const DISPOSITIONS: [&str; 5] = [
    "unchanged",
    "changed",
    "newly_observed",
    "not_seen_again",
    "unresolved",
];

pub struct Observation {
    pub source_id: Option<String>,
    pub semantic: [u8; 32],
    pub categories: BTreeMap<String, [u8; 32]>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub snapshot_id: Uuid,
    pub capture_id: Uuid,
    pub ordinal: i32,
    pub side: String,
    pub stream: String,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Side {
    pub semantic: [u8; 32],
    pub categories: BTreeMap<String, [u8; 32]>,
    pub count: u64,
    pub equal_repeats: u64,
    pub conflict: bool,
    pub streams: BTreeSet<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Group {
    pub components: BTreeMap<String, [Option<Side>; 2]>,
    pub reasons: BTreeSet<String>,
    pub evidence: Vec<Evidence>,
}
impl Group {
    pub fn add(
        &mut self,
        side: usize,
        representation: &str,
        observation: &Observation,
        evidence: Evidence,
        repeat: bool,
    ) {
        self.reasons.extend(observation.reasons.iter().cloned());
        let entry = &mut self
            .components
            .entry(representation.to_owned())
            .or_default()[side];
        if let Some(entry) = entry {
            entry.conflict |= entry.semantic != observation.semantic;
            entry.count += 1;
            entry.equal_repeats += u64::from(repeat);
            entry.streams.insert(evidence.stream.clone());
        } else {
            *entry = Some(Side {
                semantic: observation.semantic,
                categories: observation.categories.clone(),
                count: 1,
                equal_repeats: 0,
                conflict: false,
                streams: BTreeSet::from([evidence.stream.clone()]),
            });
        }
        if self.evidence.len() < 16 {
            self.evidence.push(evidence);
        }
    }
    pub fn observations(&self, side: usize) -> u64 {
        self.components
            .values()
            .filter_map(|v| v[side].as_ref())
            .map(|v| v.count)
            .sum()
    }
    pub fn repeats(&self) -> u64 {
        self.components
            .values()
            .flat_map(|v| v.iter().flatten())
            .map(|v| v.equal_repeats)
            .sum()
    }
    pub fn conflicted(&self) -> bool {
        self.components
            .values()
            .flat_map(|v| v.iter().flatten())
            .any(|v| v.conflict)
    }
}

pub fn extract(
    stream: Stream,
    raw: &[u8],
    key: &RawPayloadKey,
    org: OrganizationId,
) -> Result<Vec<Observation>, ParseError> {
    if raw.len() > MAX_RAW {
        return Err(ParseError::DerivedTooLarge);
    }
    let root = JsonParser::parse(raw)?;
    let items = if stream == Stream::NoteDetail {
        std::slice::from_ref(&root)
    } else {
        root.get(stream.collection())
            .and_then(Node::array)
            .ok_or(ParseError::Malformed)?
    };
    if items.len() > MAX_OBSERVATIONS {
        return Err(ParseError::DerivedTooLarge);
    }
    Ok(items
        .iter()
        .map(|node| observation(stream, node, key, org))
        .collect())
}
fn category(family: &str, key: &str) -> &'static str {
    match (family, key) {
        ("people", "emails" | "phones" | "addresses" | "firstName" | "lastName" | "name") => {
            "contact_information"
        }
        ("people", "stage" | "stageId" | "assignedUserId" | "assignedPondId") => "assignment_stage",
        ("people", "tags") => "embedded_tags",
        ("people", k) if k.starts_with("custom") => "custom_fields",
        ("custom_fields", _) => "custom_fields",
        (
            "notes",
            "body"
            | "subject"
            | "isHtml"
            | "replies"
            | "reactions"
            | "showContent"
            | "contentInaccessible"
            | "isContentAccessible",
        ) => "note_content",
        ("tasks", "isCompleted" | "completed") => "task_status",
        ("tasks", "dueDate" | "dueDateTime") => "task_due_date",
        (_, "id") => "identity",
        _ => "other_properties",
    }
}
fn observation(
    stream: Stream,
    node: &Node,
    key: &RawPayloadKey,
    org: OrganizationId,
) -> Observation {
    let mut canonical = Vec::new();
    node.encode(&mut canonical);
    let semantic = crypto::snapshot_hmac(
        key,
        org,
        &format!("semantic:{}", stream.representation()),
        &canonical,
    );
    let source_id = node.get("id").and_then(positive_id);
    let mut reasons = Vec::new();
    if source_id.is_none() {
        reasons.push("invalid_source_id".into());
    }
    let mut subsets: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    if let Node::Object(fields) = node {
        for (name, value) in fields {
            let bytes = subsets
                .entry(category(stream.family().as_str(), name))
                .or_default();
            Node::String(name.clone()).encode(bytes);
            value.encode(bytes);
        }
    } else {
        reasons.push("unsupported_record_shape".into());
    }
    if matches!(stream, Stream::Notes | Stream::NoteDetail)
        && (matches!(node.get("showContent"), Some(Node::Bool(false)))
            || matches!(node.get("contentInaccessible"), Some(Node::Bool(true)))
            || matches!(node.get("isContentAccessible"), Some(Node::Bool(false))))
    {
        reasons.push("note_content_inaccessible".into());
    }
    let categories = subsets
        .into_iter()
        .map(|(category, bytes)| {
            (
                category.to_owned(),
                crypto::snapshot_hmac(
                    key,
                    org,
                    &format!("core-change-category:{category}"),
                    &bytes,
                ),
            )
        })
        .collect();
    Observation {
        source_id,
        semantic,
        categories,
        reasons,
    }
}
pub fn unsupported(source_id: Option<String>, reason: &str) -> Observation {
    Observation {
        source_id,
        semantic: [0; 32],
        categories: BTreeMap::new(),
        reasons: vec![reason.to_owned()],
    }
}
pub fn note_request(key: &RawPayloadKey, org: OrganizationId, source_id: &str) -> [u8; 32] {
    crypto::snapshot_hmac(
        key,
        org,
        "request",
        &serde_json::to_vec(
            &json!({"stream":"note_detail","offset":0,"next":null,"source_id":source_id}),
        )
        .expect("static DTO"),
    )
}
fn stream_complete(inputs: &Value, side: usize, stream: &str, uncertain: &[String]) -> bool {
    !uncertain.iter().any(|v| v == stream)
        && inputs[if side == 0 { "baseline" } else { "newer" }]["streams"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter()
                    .any(|v| v["stream"] == stream && v["state"] == "completed")
            })
}
fn family_complete(inputs: &Value, side: usize, family: &str, uncertain: &[String]) -> bool {
    let streams: &[&str] = match family {
        "notes" => &["notes", "note_detail"],
        "tasks" => &["tasks_open", "tasks_completed"],
        "people" => &["people"],
        "users" => &["users"],
        "stages" => &["stages"],
        _ => &["custom_fields"],
    };
    streams
        .iter()
        .all(|v| stream_complete(inputs, side, v, uncertain))
}
pub fn compare(
    id: Uuid,
    family: &str,
    source_id: Option<&str>,
    group: &Group,
    inputs: &Value,
    uncertain: [&[String]; 2],
) -> Value {
    let mut reasons = group.reasons.clone();
    let mut categories = BTreeSet::new();
    let mut components = Vec::new();
    let has = [group.observations(0) > 0, group.observations(1) > 0];
    let mut primary = if !has[0] {
        "newly_observed"
    } else if !has[1] {
        "not_seen_again"
    } else {
        "unchanged"
    };
    if source_id.is_none() {
        reasons.insert("invalid_source_id".into());
    }
    if group.conflicted() {
        reasons.insert("conflicting_observations".into());
    }
    if !has[0] && !family_complete(inputs, 0, family, uncertain[0])
        || !has[1] && !family_complete(inputs, 1, family, uncertain[1])
    {
        reasons.insert("incomplete_enumeration".into());
    }
    for (representation, sides) in &group.components {
        let disposition = match (&sides[0], &sides[1]) {
            (Some(a), Some(b)) if a.conflict || b.conflict => "unresolved",
            (Some(a), Some(b)) => {
                for category in a.categories.keys().chain(b.categories.keys()) {
                    if a.categories.get(category) != b.categories.get(category) {
                        categories.insert(category.clone());
                    }
                }
                if family == "tasks" && a.streams != b.streams {
                    categories.insert("task_status".into());
                }
                if a.semantic == b.semantic && a.streams == b.streams {
                    "unchanged"
                } else {
                    "changed"
                }
            }
            (None, Some(_)) if !has[0] => "newly_observed",
            (Some(_), None) if !has[1] => "not_seen_again",
            _ => {
                reasons.insert("missing_comparable_component".into());
                "unresolved"
            }
        };
        if disposition == "changed" && primary == "unchanged" {
            primary = "changed";
        }
        components.push(json!({"representation":representation,"disposition":disposition,"baseline_observations":sides[0].as_ref().map_or(0,|v|v.count).to_string(),"newer_observations":sides[1].as_ref().map_or(0,|v|v.count).to_string()}));
    }
    if family == "notes" {
        for side in 0..2 {
            if has[side]
                && [
                    Stream::Notes.representation(),
                    Stream::NoteDetail.representation(),
                ]
                .iter()
                .any(|repr| {
                    group
                        .components
                        .get(*repr)
                        .is_none_or(|v| v[side].is_none())
                })
            {
                reasons.insert("missing_note_component".into());
            }
        }
    }
    if !reasons.is_empty() {
        primary = "unresolved";
    }
    if primary == "not_seen_again" {
        reasons.insert("absence_is_not_deletion".into());
    }
    if primary == "newly_observed" {
        reasons.insert("first_observation_is_not_creation".into());
    }
    if !has[0] || !has[1] {
        reasons.insert("effective_access_not_proven".into());
        if inputs["source_scope"] != "consistent_identity" {
            reasons.insert("source_scope_changed_or_unknown".into());
        }
    }
    json!({"id":id,"family":family,"source_id":source_id,"disposition":primary,"categories":categories,"reasons":reasons,"baseline_observations":group.observations(0).to_string(),"newer_observations":group.observations(1).to_string(),"components":components,"evidence":group.evidence,"evidence_is_exhaustive":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_unknown_properties_and_rejections() {
        let key = RawPayloadKey::new([42; 32]);
        let org = OrganizationId::new(Uuid::new_v4());
        let a = extract(
            Stream::People,
            br#"{"people":[{"id":1,"unknown":{"a":9007199254740993},"x":null}]}"#,
            &key,
            org,
        )
        .unwrap();
        let b = extract(
            Stream::People,
            br#"{"people":[{"x":null,"unknown":{"a":90071992547409930e-1},"id":1.0}]}"#,
            &key,
            org,
        )
        .unwrap();
        assert_eq!(a[0].semantic, b[0].semantic);
        let c = extract(
            Stream::People,
            br#"{"people":[{"id":1,"unknown":{"a":9007199254740992}}]}"#,
            &key,
            org,
        )
        .unwrap();
        assert_ne!(a[0].semantic, c[0].semantic);
        assert_ne!(
            a[0].categories["other_properties"],
            c[0].categories["other_properties"]
        );
        assert!(extract(
            Stream::People,
            br#"{"people":[{"id":1,"\u0069d":2}]}"#,
            &key,
            org
        )
        .is_err());
    }
}

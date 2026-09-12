//! Pure, lossless retained activity extraction and native preview (010f2).
//!
//! The caller qualifies captures, groups *complete* canonical observations by
//! representation, resolves the parent Person and explicitly maps user/type IDs.
//! No type carrying source content implements Debug. Nothing here authorizes a
//! write, chooses a source variant or interprets a note-list row as a detail row.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::snapshot_source::{positive_id, JsonParser, Node, ParseError, Stream};
use crate::domain::note::NoteBody;
use crate::domain::task::{TaskKind, TaskTitle};

pub(crate) const ENGINE: &str = "fub-activity-source-v1";
pub(crate) const HTML_PROFILE: &str = "fub-note-readable-v1/html5ever-0.39.0";
pub(crate) const TIME_PROFILE: &str = "fub-task-time-v1/chrono-tz-0.10.4";
pub(crate) const TZDB_VERSION: &str = chrono_tz::IANA_TZDB_VERSION;

const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_ITEMS: usize = 100;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    pub source_id: Option<String>,
    pub person_id: Option<String>,
    /// Full lossless bytes for representation equality/HMAC, never a projection.
    pub canonical: Vec<u8>,
    pub stream: Stream,
    pub roles: Vec<RoleRef>,
    pub source_type: Option<String>,
    pub reasons: Vec<String>,
    pub source_only: BTreeMap<String, u64>,
    /// Every original property as lossless canonical JSON, including unknowns.
    pub provenance: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RoleRef {
    pub role: String,
    pub source_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Preview {
    pub native: Option<NativeActivity>,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub source_only: BTreeMap<String, u64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum NativeActivity {
    Note {
        body: String,
        author_source_id: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    },
    Task {
        title: String,
        source_type: String,
        creator_source_id: Option<String>,
        assignee_source_id: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        due_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    },
}

pub(crate) fn extract_page(stream: Stream, raw: &[u8]) -> Result<Vec<Record>, ParseError> {
    if !matches!(
        stream,
        Stream::Users
            | Stream::Notes
            | Stream::NoteDetail
            | Stream::TasksOpen
            | Stream::TasksCompleted
    ) {
        return Err(ParseError::Malformed);
    }
    if raw.len() > MAX_RAW_BYTES {
        return Err(ParseError::DerivedTooLarge);
    }
    let root = JsonParser::parse(raw)?;
    if stream == Stream::NoteDetail {
        return Ok(vec![record(stream, &root)]);
    }
    let items = root
        .get(stream.collection())
        .and_then(Node::array)
        .ok_or(ParseError::Malformed)?;
    if items.len() > MAX_PAGE_ITEMS {
        return Err(ParseError::DerivedTooLarge);
    }
    Ok(items.iter().map(|node| record(stream, node)).collect())
}

fn mark(codes: &mut Vec<String>, code: &str) {
    if !codes.iter().any(|existing| existing == code) {
        codes.push(code.to_owned());
    }
}

fn count(counts: &mut BTreeMap<String, u64>, code: &str, amount: u64) {
    if amount > 0 {
        *counts.entry(code.to_owned()).or_default() += amount;
    }
}

fn canonical(node: &Node) -> Vec<u8> {
    let mut bytes = Vec::new();
    node.encode(&mut bytes);
    bytes
}

fn present(node: &Node) -> bool {
    match node {
        Node::Null => false,
        Node::String(value) => !value.is_empty(),
        Node::Array(items) => !items.is_empty(),
        Node::Object(fields) => !fields.is_empty(),
        // Numeric zero can mean an at-due-time reminder; never erase it.
        Node::Bool(_) | Node::Number(_) => true,
    }
}

fn source_component(node: &Node) -> u64 {
    match node {
        Node::Array(items) => items.len() as u64,
        Node::Object(fields) => u64::from(!fields.is_empty()),
        other => u64::from(present(other)),
    }
}

fn record(stream: Stream, node: &Node) -> Record {
    let mut result = Record {
        source_id: node.get("id").and_then(positive_id),
        person_id: node.get("personId").and_then(positive_id),
        canonical: canonical(node),
        stream,
        roles: Vec::new(),
        source_type: node.get("type").and_then(Node::string).map(str::to_owned),
        reasons: Vec::new(),
        source_only: BTreeMap::new(),
        provenance: BTreeMap::new(),
    };
    if result.source_id.is_none() {
        mark(&mut result.reasons, "invalid_source_id");
    }
    if stream != Stream::Users && result.person_id.is_none() {
        mark(&mut result.reasons, "invalid_person_reference");
    }
    let Node::Object(fields) = node else {
        mark(&mut result.reasons, "unsupported_record_shape");
        return result;
    };
    for (key, value) in fields {
        result.provenance.insert(
            key.clone(),
            String::from_utf8(canonical(value)).expect("canonical source JSON is UTF-8"),
        );
    }
    if stream == Stream::Users {
        return result;
    }
    match stream {
        Stream::Notes | Stream::NoteDetail => {
            role(node, "createdById", "note_author", &mut result);
        }
        Stream::TasksOpen | Stream::TasksCompleted => {
            role(node, "createdById", "task_creator", &mut result);
            role(node, "assignedUserId", "task_assignee", &mut result);
        }
        _ => unreachable!("extract_page restricts the stream"),
    }
    visibility(node, &mut result.reasons);
    for (key, value) in fields {
        let code = match key.as_str() {
            // Fully interpreted core fields; original values still retained.
            "id"
            | "personId"
            | "created"
            | "updated"
            | "createdById"
            | "showContent"
            | "contentInaccessible"
            | "isContentAccessible" => continue,
            "body" | "subject" | "isHtml"
                if matches!(stream, Stream::Notes | Stream::NoteDetail) =>
            {
                continue
            }
            "name" | "type" | "isCompleted" | "completed" | "dueDate" | "dueDateTime"
            | "assignedUserId"
                if matches!(stream, Stream::TasksOpen | Stream::TasksCompleted) =>
            {
                continue
            }
            "replies" if matches!(stream, Stream::Notes | Stream::NoteDetail) => {
                "replies_source_only"
            }
            "reactions" if matches!(stream, Stream::Notes | Stream::NoteDetail) => {
                "reactions_source_only"
            }
            "attachments" => "attachments_source_only",
            "description" => "description_source_only",
            "remindSecondsBefore" | "reminders" => "reminders_source_only",
            "externalTaskLink" | "externalCalendarId" | "externalTaskId" => {
                "external_task_reference_source_only"
            }
            "recurrence" | "isRecurring" | "recurring" | "recurrenceRule" | "actionPlanId" => {
                "recurrence_source_only"
            }
            "priority" => "priority_source_only",
            "createdBy" | "assignedTo" | "AssignedTo" => "source_attribution_labels",
            "updatedBy" | "updatedById" => "source_editor_attribution",
            "type" | "isExternal" | "systemId" | "systemName" => "source_note_metadata",
            _ => {
                // Include *every* unexpected property, even null/empty. Its
                // name/value remain encrypted exact data, not an issue token.
                count(&mut result.source_only, "unknown_properties_source_only", 1);
                continue;
            }
        };
        count(&mut result.source_only, code, source_component(value));
    }
    result.reasons.sort();
    result
}

fn role(node: &Node, key: &str, name: &str, result: &mut Record) {
    if let Some(value) = node.get(key) {
        match positive_id(value) {
            Some(source_id) => result.roles.push(RoleRef {
                role: name.to_owned(),
                source_id,
            }),
            None if !matches!(value, Node::Null) => {
                // No qualified source ID means no mapping group and no native
                // actor. The exact field stays inspectable source attribution.
                count(&mut result.source_only, "unqualified_user_reference", 1);
            }
            None => {}
        }
    }
}

fn visibility(node: &Node, reasons: &mut Vec<String>) {
    for (key, access_value) in [
        ("showContent", true),
        ("contentInaccessible", false),
        ("isContentAccessible", true),
    ] {
        match node.get(key) {
            None | Some(Node::Null) => {}
            Some(Node::Bool(value)) if *value == access_value => {}
            Some(Node::Bool(_)) => mark(reasons, "content_inaccessible"),
            _ => mark(reasons, "unqualified_visibility_metadata"),
        }
    }
    // Unknown visibility/restriction metadata cannot safely be treated as a
    // harmless decoration. It remains source-only as well as holding the note.
    for key in [
        "visibility",
        "isPrivate",
        "restricted",
        "isRestricted",
        "permissions",
        "accessible",
        "contentUnavailable",
    ] {
        if node.get(key).is_some_and(present) {
            mark(reasons, "unqualified_visibility_metadata");
        }
    }
}

fn source_node(record: &Record) -> Result<Node, ParseError> {
    let mut fields = BTreeMap::new();
    for (key, value) in &record.provenance {
        fields.insert(key.clone(), JsonParser::parse(value.as_bytes())?);
    }
    Ok(Node::Object(fields))
}

fn role_id(record: &Record, name: &str) -> Option<String> {
    record
        .roles
        .iter()
        .find(|role| role.role == name)
        .map(|role| role.source_id.clone())
}

pub(crate) fn valid_timezone(raw: &str) -> bool {
    super::activity_time::timezone(raw).is_some()
}

/// These are suggestions only: the coordinator must require explicit mapping.
pub(crate) fn suggested_kind(source_type: &str) -> Option<TaskKind> {
    match source_type {
        "Call" => Some(TaskKind::Call),
        "Email" => Some(TaskKind::Email),
        "Text" => Some(TaskKind::Text),
        "Follow Up" => Some(TaskKind::FollowUp),
        _ => None,
    }
}

pub(crate) fn preview(record: &Record, source_timezone: Option<&str>) -> Preview {
    let mut result = Preview {
        native: None,
        reasons: record.reasons.clone(),
        transformations: Vec::new(),
        source_only: record.source_only.clone(),
    };
    if record.stream == Stream::Users {
        mark(&mut result.reasons, "source_user_evidence");
        return result;
    }
    if record.stream == Stream::Notes {
        mark(&mut result.reasons, "note_list_not_executable");
        return result;
    }
    let Ok(node) = source_node(record) else {
        mark(&mut result.reasons, "source_record_malformed");
        return result;
    };
    let times = timestamps(&node, &mut result);
    match record.stream {
        Stream::NoteDetail => note_preview(record, &node, times, &mut result),
        Stream::TasksOpen | Stream::TasksCompleted => {
            task_preview(record, &node, times, source_timezone, &mut result);
        }
        _ => mark(&mut result.reasons, "unsupported_record_stream"),
    }
    if !result.reasons.is_empty() {
        result.native = None;
    }
    result.reasons.sort();
    result.transformations.sort();
    result
}

fn timestamps(node: &Node, result: &mut Preview) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let created = node
        .get("created")
        .and_then(Node::string)
        .and_then(super::activity_time::instant);
    if created.is_none() {
        mark(&mut result.reasons, "invalid_created_instant");
    }
    let updated = match node.get("updated") {
        None | Some(Node::Null) => {
            mark(
                &mut result.transformations,
                "updated_unknown_initialized_from_created",
            );
            created
        }
        Some(value) => {
            let updated = value.string().and_then(super::activity_time::instant);
            if updated.is_none() {
                mark(&mut result.reasons, "invalid_updated_instant");
            }
            updated
        }
    };
    match (created, updated) {
        (Some(created), Some(updated)) if updated >= created => Some((created, updated)),
        (Some(_), Some(_)) => {
            mark(&mut result.reasons, "updated_before_created");
            None
        }
        _ => None,
    }
}

fn note_preview(
    record: &Record,
    node: &Node,
    times: Option<(DateTime<Utc>, DateTime<Utc>)>,
    result: &mut Preview,
) {
    let subject = match node.get("subject") {
        None | Some(Node::Null) => Some(String::new()),
        Some(Node::String(value)) => {
            let normalized = value.replace("\r\n", "\n");
            let normalized = normalized.trim().to_owned();
            if normalized != *value {
                mark(&mut result.transformations, "subject_plain_text_normalized");
            }
            if normalized
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
            {
                mark(&mut result.reasons, "unsupported_note_subject");
                None
            } else {
                Some(normalized)
            }
        }
        _ => {
            mark(&mut result.reasons, "unsupported_note_subject");
            None
        }
    };
    let is_html = match node.get("isHtml") {
        Some(Node::Bool(value)) => Some(*value),
        _ => {
            mark(&mut result.reasons, "unqualified_note_format");
            None
        }
    };
    let body = match node.get("body") {
        Some(Node::String(value)) => match is_html {
            Some(false) => Some(value.clone()),
            Some(true) => match super::activity_html::convert(value) {
                Ok(converted) => {
                    mark(&mut result.transformations, "html_to_readable_text");
                    mark(
                        &mut result.transformations,
                        "html_whitespace_and_styling_changed",
                    );
                    count(
                        &mut result.source_only,
                        "html_comments_source_only",
                        converted.comments,
                    );
                    Some(converted.text)
                }
                Err(code) => {
                    mark(&mut result.reasons, code);
                    None
                }
            },
            None => None,
        },
        _ => {
            mark(&mut result.reasons, "unsupported_note_body");
            None
        }
    };
    if let (Some(subject), Some(body)) = (subject, body) {
        let combined = if subject.is_empty() {
            body
        } else if body.trim().is_empty() {
            mark(&mut result.transformations, "subject_only_note");
            format!("Subject: {subject}")
        } else {
            mark(&mut result.transformations, "subject_prepended");
            format!("Subject: {subject}\n\n{body}")
        };
        match NoteBody::parse(&combined) {
            Ok(body) => {
                if body != combined {
                    mark(&mut result.transformations, "note_body_normalized");
                }
                if let Some((created_at, updated_at)) = times {
                    result.native = Some(NativeActivity::Note {
                        body,
                        author_source_id: role_id(record, "note_author"),
                        created_at,
                        updated_at,
                    });
                }
            }
            Err(_) => mark(&mut result.reasons, "unsupported_native_note_body"),
        }
    }
}

fn task_preview(
    record: &Record,
    node: &Node,
    times: Option<(DateTime<Utc>, DateTime<Utc>)>,
    source_timezone: Option<&str>,
    result: &mut Preview,
) {
    let title = match node.get("name").and_then(Node::string) {
        Some(raw) => match TaskTitle::parse(raw) {
            Ok(title) => {
                if title != raw {
                    mark(&mut result.transformations, "task_title_trimmed");
                }
                Some(title)
            }
            Err(_) => {
                mark(&mut result.reasons, "unsupported_task_title");
                None
            }
        },
        None => {
            mark(&mut result.reasons, "unsupported_task_title");
            None
        }
    };
    let source_type = node
        .get("type")
        .and_then(Node::string)
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control));
    if source_type.is_none() {
        mark(&mut result.reasons, "unqualified_task_type");
    }
    let completed = match node.get("isCompleted") {
        Some(Node::Bool(value)) => Some(*value),
        Some(Node::Number(value)) if value == "0" => Some(false),
        Some(Node::Number(value)) if value == "1e0" => Some(true),
        _ => {
            mark(&mut result.reasons, "unqualified_completion_state");
            None
        }
    };
    if completed.is_some_and(|completed| completed != (record.stream == Stream::TasksCompleted)) {
        mark(&mut result.reasons, "completion_partition_conflict");
    }
    let completed_at = match node.get("completed") {
        None | Some(Node::Null) if completed == Some(false) => None,
        None | Some(Node::Null) => {
            mark(&mut result.reasons, "missing_completion_instant");
            None
        }
        Some(_) if completed == Some(false) => {
            mark(&mut result.reasons, "open_task_has_completion_instant");
            None
        }
        Some(value) => {
            let instant = value.string().and_then(super::activity_time::instant);
            if instant.is_none() {
                mark(&mut result.reasons, "invalid_completion_instant");
            }
            if let (Some(instant), Some((created, _))) = (instant, times) {
                if instant < created {
                    mark(&mut result.reasons, "completed_before_created");
                }
            }
            instant
        }
    };
    if completed == Some(true) {
        mark(&mut result.transformations, "completion_actor_unknown");
    }
    let due_at = task_due(node, source_timezone, result);
    if let (Some(title), Some(source_type), Some((created_at, updated_at))) =
        (title, source_type, times)
    {
        result.native = Some(NativeActivity::Task {
            title,
            source_type: source_type.to_owned(),
            creator_source_id: role_id(record, "task_creator"),
            assignee_source_id: role_id(record, "task_assignee"),
            created_at,
            updated_at,
            due_at,
            completed_at,
        });
    }
}

fn task_due(
    node: &Node,
    source_timezone: Option<&str>,
    result: &mut Preview,
) -> Option<DateTime<Utc>> {
    let date_value = node
        .get("dueDate")
        .filter(|value| !matches!(value, Node::Null));
    let timed_value = node
        .get("dueDateTime")
        .filter(|value| !matches!(value, Node::Null));
    let date = date_value
        .and_then(Node::string)
        .and_then(super::activity_time::date);
    let timed = timed_value
        .and_then(Node::string)
        .and_then(super::activity_time::instant);
    if date_value.is_some() && date.is_none() {
        mark(&mut result.reasons, "invalid_due_date");
    }
    if timed_value.is_some() && timed.is_none() {
        mark(&mut result.reasons, "invalid_due_instant");
    }
    if date_value.is_none() && timed_value.is_none() {
        mark(&mut result.transformations, "task_deliberately_undated");
        return None;
    }
    let Some(date) = date else {
        return timed;
    };
    let zone = source_timezone.and_then(super::activity_time::timezone);
    let Some(zone) = zone else {
        mark(
            &mut result.reasons,
            if source_timezone.is_some() {
                "invalid_source_timezone"
            } else {
                "source_timezone_confirmation_required"
            },
        );
        return None;
    };
    if let Some(timed) = timed {
        if timed.with_timezone(&zone).date_naive() != date {
            mark(&mut result.reasons, "due_date_instant_conflict");
            return None;
        }
        mark(
            &mut result.transformations,
            "due_date_checked_in_confirmed_timezone",
        );
        return Some(timed);
    }
    if timed_value.is_some() {
        return None;
    }
    let due = super::activity_time::end_of_day(date, zone);
    if due.is_none() {
        mark(&mut result.reasons, "unresolvable_date_boundary");
    } else {
        mark(
            &mut result.transformations,
            "date_only_end_of_day_in_confirmed_timezone",
        );
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn note() -> Value {
        json!({"id":11,"personId":1,"createdById":7,"body":"original body","subject":"",
            "isHtml":false,"created":"2026-09-01T12:00:00.123456Z","updated":null})
    }

    fn task() -> Value {
        json!({"id":21,"personId":1,"createdById":7,"assignedUserId":8,"name":"Call the client",
            "type":"Call","isCompleted":false,"created":"2026-09-01T12:00:00Z","updated":null})
    }

    fn fixture(stream: Stream, value: Value) -> Record {
        let value = match stream {
            Stream::NoteDetail => value,
            Stream::Notes => json!({"notes":[value]}),
            Stream::Users => json!({"users":[value]}),
            _ => json!({"tasks":[value]}),
        };
        extract_page(stream, &serde_json::to_vec(&value).unwrap())
            .unwrap()
            .remove(0)
    }

    fn body(result: &Preview) -> Option<&str> {
        match result.native.as_ref()? {
            NativeActivity::Note { body, .. } => Some(body),
            _ => None,
        }
    }

    fn due(result: &Preview) -> Option<DateTime<Utc>> {
        match result.native.as_ref()? {
            NativeActivity::Task { due_at, .. } => *due_at,
            _ => None,
        }
    }

    #[test]
    fn canonical_records_preserve_unknown_exact_numbers_and_never_project() {
        let raw = br#"{"id":18446744073709551617,"personId":"123456789012345678901234567890","unknown":123456789012345678901234567890.123456789,"object":{"_lossless_number":"1e0"},"body":"not clipped"}"#;
        let record = extract_page(Stream::NoteDetail, raw).unwrap().remove(0);
        assert_eq!(record.source_id.as_deref(), Some("18446744073709551617"));
        assert_eq!(
            record.person_id.as_deref(),
            Some("123456789012345678901234567890")
        );
        assert_eq!(
            record.provenance["unknown"],
            "123456789012345678901234567890123456789e-9"
        );
        assert_eq!(record.provenance["object"], r#"{"_lossless_number":"1e0"}"#);
        assert_eq!(record.source_only["unknown_properties_source_only"], 2);
        let restored: Record =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(record.canonical, restored.canonical);
        assert_eq!(record.provenance, restored.provenance);
    }

    #[test]
    fn source_users_are_lossless_evidence_without_native_person_or_mapping() {
        let record = fixture(
            Stream::Users,
            json!({"id":7,"timezone":"America/Los_Angeles","unknown":null}),
        );
        assert_eq!(record.source_id.as_deref(), Some("7"));
        assert!(record.person_id.is_none());
        assert!(record.roles.is_empty());
        assert!(record.reasons.is_empty());
        assert_eq!(record.provenance["timezone"], "\"America/Los_Angeles\"");
        assert_eq!(preview(&record, None).reasons, ["source_user_evidence"]);
    }

    #[test]
    fn duplicate_decoded_keys_depth_shape_and_page_budgets_fail_closed() {
        assert!(matches!(
            extract_page(
                Stream::NoteDetail,
                br#"{"id":1,"bo\u0064y":"one","body":"two"}"#
            ),
            Err(ParseError::DuplicateKey)
        ));
        assert!(extract_page(Stream::People, br#"{"people":[]}"#).is_err());
        assert!(extract_page(Stream::Notes, br#"{"notes":{}}"#).is_err());
        assert!(matches!(
            extract_page(Stream::NoteDetail, &vec![b' '; MAX_RAW_BYTES + 1]),
            Err(ParseError::DerivedTooLarge)
        ));
        assert!(matches!(
            extract_page(
                Stream::Notes,
                &serde_json::to_vec(&json!({"notes":vec![json!({"id":1});101]})).unwrap()
            ),
            Err(ParseError::DerivedTooLarge)
        ));
        let deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
        assert!(extract_page(Stream::NoteDetail, deep.as_bytes()).is_err());
    }

    #[test]
    fn invalid_ids_and_nonobjects_remain_individually_countable() {
        let records =
            extract_page(Stream::Notes, br#"{"notes":[false,{"id":0},{"id":"bad"}]}"#).unwrap();
        assert_eq!(records.len(), 3);
        assert!(records.iter().all(|record| record.source_id.is_none()));
        assert_ne!(records[0].canonical, records[1].canonical);
        let record = fixture(
            Stream::NoteDetail,
            json!({"id":"1".repeat(129),"personId":1}),
        );
        assert!(record.source_id.is_none());
        let record = fixture(
            Stream::NoteDetail,
            json!({"id":"1".repeat(128),"personId":1}),
        );
        assert!(record.source_id.is_some());
    }

    #[test]
    fn list_detail_and_task_partition_observations_are_never_merged_or_selected() {
        let list = fixture(Stream::Notes, note());
        let mut sparse = note();
        sparse.as_object_mut().unwrap().remove("body");
        let detail = fixture(Stream::NoteDetail, sparse);
        assert!(preview(&list, None).native.is_none());
        assert!(preview(&detail, None).native.is_none());
        let first = fixture(Stream::TasksOpen, task());
        let mut other = task();
        other["unexpected"] = json!(true);
        let second = fixture(Stream::TasksOpen, other);
        assert_ne!(first.canonical, second.canonical);
        // The canonical bytes intentionally do not smuggle in stream ordering;
        // the coordinator groups tasks across both partitions and holds conflict.
        let wrong_partition = fixture(Stream::TasksCompleted, task());
        assert_eq!(first.canonical, wrong_partition.canonical);
        assert!(preview(&wrong_partition, None)
            .reasons
            .contains(&"completion_partition_conflict".into()));
    }

    #[test]
    fn literal_plain_text_and_subject_normalization_do_not_detect_html() {
        let mut value = note();
        value["body"] = json!("  literal <b>markup</b>\r\nsecond line  ");
        let result = preview(&fixture(Stream::NoteDetail, value.clone()), None);
        assert_eq!(body(&result), Some("literal <b>markup</b>\nsecond line"));
        value["subject"] = json!("  <b>Subject</b>  ");
        value["body"] = json!("body");
        let result = preview(&fixture(Stream::NoteDetail, value), None);
        assert_eq!(body(&result), Some("Subject: <b>Subject</b>\n\nbody"));
        assert!(result
            .transformations
            .contains(&"subject_plain_text_normalized".into()));
        assert!(result.transformations.contains(&"subject_prepended".into()));
    }

    #[test]
    fn empty_body_can_be_subject_only_but_absent_or_unqualified_body_cannot() {
        let mut value = note();
        value["subject"] = json!("A subject");
        value["body"] = json!("");
        let result = preview(&fixture(Stream::NoteDetail, value.clone()), None);
        assert_eq!(body(&result), Some("Subject: A subject"));
        assert!(result.transformations.contains(&"subject_only_note".into()));
        for invalid in [Value::Null, json!(7), json!({})] {
            value["body"] = invalid;
            assert!(preview(&fixture(Stream::NoteDetail, value.clone()), None)
                .native
                .is_none());
        }
        value.as_object_mut().unwrap().remove("body");
        assert!(preview(&fixture(Stream::NoteDetail, value), None)
            .native
            .is_none());
        let mut empty = note();
        empty["body"] = json!(" \t ");
        assert!(preview(&fixture(Stream::NoteDetail, empty), None)
            .native
            .is_none());
    }

    #[test]
    fn missing_null_or_nonboolean_format_never_falls_back_to_plain_text() {
        for format in [Value::Null, json!(0), json!("false"), json!({})] {
            let mut value = note();
            value["isHtml"] = format;
            assert!(preview(&fixture(Stream::NoteDetail, value), None)
                .native
                .is_none());
        }
        let mut value = note();
        value.as_object_mut().unwrap().remove("isHtml");
        assert!(preview(&fixture(Stream::NoteDetail, value), None)
            .native
            .is_none());
    }

    #[test]
    fn inaccessible_or_contradictory_visibility_holds_even_a_present_body() {
        for (key, flag) in [
            ("showContent", json!(false)),
            ("showContent", json!("true")),
            ("contentInaccessible", json!(true)),
            ("isContentAccessible", json!(false)),
            ("visibility", json!("private")),
        ] {
            let mut value = note();
            value[key] = flag;
            assert!(preview(&fixture(Stream::NoteDetail, value), None)
                .native
                .is_none());
        }
    }

    #[test]
    fn html_conversion_keeps_original_and_separate_reply_reaction_attachment_counts() {
        let mut value = note();
        value["isHtml"] = json!(true);
        value["body"] =
            json!("<p>A &amp; B</p><ol><li>First</li><li>Second</li></ol><!-- retained -->");
        value["replies"] = json!([{"id":31,"body":"reply one"},{"id":32,"body":"reply two"}]);
        value["reactions"] = json!([{"reaction":"liked"}]);
        value["attachments"] = json!([{"url":"https://example.test/file"}]);
        let record = fixture(Stream::NoteDetail, value);
        let result = preview(&record, None);
        assert_eq!(body(&result), Some("A & B\n1. First\n2. Second"));
        assert_eq!(result.source_only["replies_source_only"], 2);
        assert_eq!(result.source_only["reactions_source_only"], 1);
        assert_eq!(result.source_only["attachments_source_only"], 1);
        assert_eq!(result.source_only["html_comments_source_only"], 1);
        assert!(record.provenance["body"].contains("&amp;"));
        assert!(record.provenance["replies"].contains("reply two"));
        assert!(!body(&result).unwrap().contains("reply"));
    }

    #[test]
    fn native_note_cap_is_unicode_characters_and_never_truncates() {
        for (body_text, allowed) in [
            ("😀".repeat(10_000), true),
            ("a".repeat(10_001), false),
            ("bad\0control".into(), false),
            ("bad\rcontrol".into(), false),
        ] {
            let mut value = note();
            value["body"] = json!(body_text);
            let result = preview(&fixture(Stream::NoteDetail, value), None);
            assert_eq!(result.native.is_some(), allowed);
            if allowed {
                assert_eq!(body(&result).unwrap().chars().count(), 10_000);
            }
        }
        let mut value = note();
        value["isHtml"] = json!(true);
        value["body"] = json!(format!("<p>{}</p>", "😀".repeat(10_000)));
        let result = preview(&fixture(Stream::NoteDetail, value.clone()), None);
        assert_eq!(body(&result).unwrap().chars().count(), 10_000);
        value["body"] = json!(format!("<p>{}</p>", "a".repeat(10_001)));
        assert!(preview(&fixture(Stream::NoteDetail, value), None)
            .native
            .is_none());
    }

    #[test]
    fn source_ids_make_role_groups_names_never_become_user_ids() {
        let mut value = task();
        value["createdById"] = Value::Null;
        value["createdBy"] = json!("Only a name");
        value["assignedUserId"] = json!(0);
        value["assignedTo"] = json!("Same name");
        value["updatedById"] = json!(999);
        let record = fixture(Stream::TasksOpen, value);
        assert!(record.roles.is_empty());
        let result = preview(&record, None);
        let Some(NativeActivity::Task {
            creator_source_id,
            assignee_source_id,
            ..
        }) = result.native
        else {
            panic!("representable task expected");
        };
        assert!(creator_source_id.is_none());
        assert!(assignee_source_id.is_none());
        assert_eq!(result.source_only["source_attribution_labels"], 2);
        assert_eq!(result.source_only["unqualified_user_reference"], 1);
    }

    #[test]
    fn task_titles_use_native_validator_without_description_flattening() {
        for (title, allowed) in [
            ("😀".repeat(500), true),
            ("a".repeat(501), false),
            ("two\nlines".into(), false),
            ("two\u{2028}lines".into(), false),
            ("\u{200b} \u{200d}".into(), false),
        ] {
            let mut value = task();
            value["name"] = json!(title);
            value["description"] = json!("never append this");
            let result = preview(&fixture(Stream::TasksOpen, value), None);
            assert_eq!(result.native.is_some(), allowed);
            assert_eq!(result.source_only["description_source_only"], 1);
        }
    }

    #[test]
    fn only_exact_documented_task_types_suggest_a_kind_without_losing_original() {
        for (source, expected) in [
            ("Call", Some(TaskKind::Call)),
            ("Email", Some(TaskKind::Email)),
            ("Text", Some(TaskKind::Text)),
            ("Follow Up", Some(TaskKind::FollowUp)),
            ("Appointment", None),
            (" Call ", None),
            ("call", None),
        ] {
            assert_eq!(suggested_kind(source), expected);
            let mut value = task();
            value["type"] = json!(source);
            let record = fixture(Stream::TasksOpen, value);
            assert_eq!(record.source_type.as_deref(), Some(source));
            assert!(preview(&record, None).native.is_some());
        }
    }

    #[test]
    fn booleans_and_exact_zero_one_complete_only_in_matching_partition() {
        for completion in [json!(false), json!(0)] {
            let mut value = task();
            value["isCompleted"] = completion;
            assert!(preview(&fixture(Stream::TasksOpen, value.clone()), None)
                .native
                .is_some());
            assert!(preview(&fixture(Stream::TasksCompleted, value), None)
                .native
                .is_none());
        }
        for completion in [json!(true), json!(1)] {
            let mut value = task();
            value["isCompleted"] = completion;
            value["completed"] = json!("2026-09-03T10:00:00Z");
            assert!(
                preview(&fixture(Stream::TasksCompleted, value.clone()), None)
                    .native
                    .is_some()
            );
            assert!(preview(&fixture(Stream::TasksOpen, value), None)
                .native
                .is_none());
        }
        for completion in [Value::Null, json!("true"), json!(2), json!(-1), json!(0.5)] {
            let mut value = task();
            value["isCompleted"] = completion;
            assert!(preview(&fixture(Stream::TasksOpen, value), None)
                .native
                .is_none());
        }
        let mut value = task();
        value.as_object_mut().unwrap().remove("isCompleted");
        assert!(preview(&fixture(Stream::TasksOpen, value), None)
            .native
            .is_none());
    }

    #[test]
    fn completion_requires_actual_instant_without_inventing_completer_from_editor() {
        let mut value = task();
        value["isCompleted"] = json!(1);
        value["updated"] = json!("2026-09-02T10:00:00Z");
        value["updatedById"] = json!(999);
        assert!(
            preview(&fixture(Stream::TasksCompleted, value.clone()), None)
                .native
                .is_none()
        );
        value["completed"] = json!("2026-09-03T10:00:00.123456Z");
        let record = fixture(Stream::TasksCompleted, value.clone());
        assert!(!record.roles.iter().any(|role| role.source_id == "999"));
        let result = preview(&record, None);
        assert!(result.native.is_some());
        assert!(result
            .transformations
            .contains(&"completion_actor_unknown".into()));
        let native = serde_json::to_value(result.native).unwrap();
        assert!(native.get("completed_by_user_id").is_none());
        for invalid in [
            json!(""),
            json!("2026-09-03T10:00:00"),
            json!("2026-08-31T10:00:00Z"),
            json!(7),
        ] {
            value["completed"] = invalid;
            assert!(
                preview(&fixture(Stream::TasksCompleted, value.clone()), None)
                    .native
                    .is_none()
            );
        }
    }

    #[test]
    fn due_absence_is_deliberately_undated_but_empty_or_malformed_is_held() {
        for fields in [json!({}), json!({"dueDate":null,"dueDateTime":null})] {
            let mut value = task();
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let result = preview(&fixture(Stream::TasksOpen, value), None);
            assert!(result.native.is_some());
            assert!(due(&result).is_none());
            assert!(result
                .transformations
                .contains(&"task_deliberately_undated".into()));
        }
        for fields in [
            json!({"dueDate":""}),
            json!({"dueDateTime":""}),
            json!({"dueDateTime":"2026-09-11T12:00:00"}),
            json!({"dueDate":7}),
        ] {
            let mut value = task();
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert!(preview(&fixture(Stream::TasksOpen, value), None)
                .native
                .is_none());
        }
    }

    #[test]
    fn date_only_requires_confirmed_zone_and_preserves_utc_end_of_day() {
        let mut value = task();
        value["dueDate"] = json!("2026-03-08");
        let record = fixture(Stream::TasksOpen, value);
        assert!(preview(&record, None)
            .reasons
            .contains(&"source_timezone_confirmation_required".into()));
        let result = preview(&record, Some("America/Los_Angeles"));
        assert_eq!(
            due(&result),
            super::super::activity_time::instant("2026-03-09T06:59:59Z")
        );
        let replanned = preview(&record, Some("America/New_York"));
        assert_eq!(
            due(&replanned),
            super::super::activity_time::instant("2026-03-09T03:59:59Z")
        );
        let frozen: Preview =
            serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
        assert_eq!(due(&frozen), due(&result));
        assert!(valid_timezone("America/Los_Angeles"));
        assert!(!valid_timezone(" browser "));
        assert!(!TZDB_VERSION.is_empty());
    }

    #[test]
    fn timed_due_is_exact_and_both_due_fields_must_agree_in_the_confirmed_zone() {
        let mut value = task();
        value["dueDateTime"] = json!("2026-09-12T01:00:00.123456Z");
        let record = fixture(Stream::TasksOpen, value.clone());
        assert_eq!(
            due(&preview(&record, None)),
            super::super::activity_time::instant("2026-09-12T01:00:00.123456Z")
        );
        value["dueDate"] = json!("2026-09-11");
        let record = fixture(Stream::TasksOpen, value);
        assert!(preview(&record, None).native.is_none());
        assert!(preview(&record, Some("America/Los_Angeles"))
            .native
            .is_some());
        let wrong = preview(&record, Some("UTC"));
        assert!(wrong.reasons.contains(&"due_date_instant_conflict".into()));
        assert!(wrong.native.is_none());
    }

    #[test]
    fn timestamp_initialization_and_precision_are_disclosed_without_import_time_defaults() {
        let record = fixture(Stream::NoteDetail, note());
        let result = preview(&record, None);
        let Some(NativeActivity::Note {
            created_at,
            updated_at,
            ..
        }) = result.native
        else {
            panic!("representable note expected");
        };
        assert_eq!(created_at, updated_at);
        assert_eq!(created_at.timestamp_subsec_micros(), 123456);
        assert!(result
            .transformations
            .contains(&"updated_unknown_initialized_from_created".into()));
        for (key, raw) in [
            ("created", "2026-09-01T12:00:00"),
            ("created", "2026-09-01T12:00:00.1234561Z"),
            ("updated", "2026-08-31T12:00:00Z"),
            ("updated", ""),
        ] {
            let mut value = note();
            value[key] = json!(raw);
            assert!(preview(&fixture(Stream::NoteDetail, value), None)
                .native
                .is_none());
        }
    }

    #[test]
    fn task_settings_and_unknown_properties_are_counted_without_native_behavior() {
        let mut value = task();
        value["description"] = json!("inspect this");
        value["remindSecondsBefore"] = json!(0);
        value["recurrence"] = json!({"days":[1,2]});
        value["externalCalendarId"] = json!("source calendar");
        value["priority"] = json!(2);
        value["newUnknown"] = Value::Null;
        let record = fixture(Stream::TasksOpen, value);
        let result = preview(&record, None);
        assert!(result.native.is_some());
        for code in [
            "description_source_only",
            "reminders_source_only",
            "recurrence_source_only",
            "external_task_reference_source_only",
            "priority_source_only",
            "unknown_properties_source_only",
        ] {
            assert_eq!(result.source_only[code], 1);
        }
        let native = serde_json::to_value(result.native).unwrap();
        assert!(native.get("description").is_none());
        assert!(native.get("recurrence").is_none());
    }
}

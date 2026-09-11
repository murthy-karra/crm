//! Frozen, GET-only `fub-core-v1` profile and bounded source interpretation.
//! Raw HTTP bytes belong to the caller's encrypted capture, even on parser errors.
//! This is public-documentation qualification, not a claim of live API fidelity.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::reader::ReaderError;
use crate::domain::contact::{normalize_email, normalize_phone};

const PAGE_SIZE: usize = 100;
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 100_000;
const MAX_TOKEN: usize = 2048;
const MAX_SOURCE_ID: usize = 128;
const MAX_CONTACT_KEYS: usize = 4096;
const MAX_PAGE_CONTACT_KEYS: usize = 16_384;
const MAX_CONTACT_BYTES: usize = 1024 * 1024;
const MAX_PROJECTION: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Users,
    Stages,
    CustomFields,
    People,
    Notes,
    Tasks,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Stages => "stages",
            Self::CustomFields => "custom_fields",
            Self::People => "people",
            Self::Notes => "notes",
            Self::Tasks => "tasks",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "users" => Some(Self::Users),
            "stages" => Some(Self::Stages),
            "custom_fields" => Some(Self::CustomFields),
            "people" => Some(Self::People),
            "notes" => Some(Self::Notes),
            "tasks" => Some(Self::Tasks),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Users,
    Stages,
    CustomFields,
    People,
    Notes,
    NoteDetail,
    TasksOpen,
    TasksCompleted,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Stages => "stages",
            Self::CustomFields => "custom_fields",
            Self::People => "people",
            Self::Notes => "notes",
            Self::NoteDetail => "note_detail",
            Self::TasksOpen => "tasks_open",
            Self::TasksCompleted => "tasks_completed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "users" => Some(Self::Users),
            "stages" => Some(Self::Stages),
            "custom_fields" => Some(Self::CustomFields),
            "people" => Some(Self::People),
            "notes" => Some(Self::Notes),
            "note_detail" => Some(Self::NoteDetail),
            "tasks_open" => Some(Self::TasksOpen),
            "tasks_completed" => Some(Self::TasksCompleted),
            _ => None,
        }
    }

    pub fn family(self) -> Family {
        match self {
            Self::Users => Family::Users,
            Self::Stages => Family::Stages,
            Self::CustomFields => Family::CustomFields,
            Self::People => Family::People,
            Self::Notes | Self::NoteDetail => Family::Notes,
            Self::TasksOpen | Self::TasksCompleted => Family::Tasks,
        }
    }

    /// Public examples do not qualify equality of note content between list
    /// and enriched detail. In v1 only their source ID links those observations;
    /// no cross-representation content fields are designated comparable. Compare
    /// full semantic variants within a representation, and retain list evidence
    /// without filling absent detail fields from the list.
    pub fn representation(self) -> &'static str {
        match self {
            Self::Users => "fub-core-v1/users/allFields,calling",
            Self::Stages => "fub-core-v1/stages/default",
            Self::CustomFields => "fub-core-v1/customFields/default",
            Self::People => "fub-core-v1/people/allFields",
            Self::Notes => "fub-core-v1/notes/list",
            Self::NoteDetail => "fub-core-v1/notes/detail/replies,reactions",
            Self::TasksOpen | Self::TasksCompleted => "fub-core-v1/tasks/default",
        }
    }

    fn collection(self) -> &'static str {
        match self.family() {
            Family::CustomFields => "customfields",
            other => other.as_str(),
        }
    }

    fn supports_next(self) -> bool {
        // People is demonstrated by the pagination guide; users has an explicit
        // next/null response shape. Other families freeze documented offset mode.
        matches!(self, Self::People | Self::Users)
    }
}

/// `offset` is the local returned-item position even when `next` is in use.
/// A cursor contains source data and deliberately does not implement Debug.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub offset: u64,
    pub next: Option<String>,
}

#[derive(Clone)]
pub struct Request {
    pub stream: Stream,
    pub cursor: Cursor,
    pub source_id: Option<String>,
}

impl Request {
    pub fn path(&self) -> Result<String, ReaderError> {
        let malformed = || ReaderError::MalformedResponse;
        if self.stream == Stream::NoteDetail {
            let id = self.source_id.as_deref().ok_or_else(malformed)?;
            if positive_id(&Node::String(id.into())).as_deref() != Some(id)
                || self.cursor != Cursor::default()
            {
                return Err(malformed());
            }
            return Ok(format!(
                "notes/{id}?includeThreadedReplies=true&includeReactions=true"
            ));
        }
        if self.source_id.is_some() {
            return Err(malformed());
        }
        let mut path = match self.stream {
            Stream::Users => "users?limit=100&fields=allFields%2Ccalling&includeDeleted=true",
            Stream::Stages => "stages?limit=100",
            Stream::CustomFields => "customFields?limit=100",
            Stream::People => {
                "people?limit=100&fields=allFields&includeTrash=true&includeUnclaimed=true"
            }
            Stream::Notes => "notes?limit=100",
            Stream::TasksOpen => "tasks?limit=100&isCompleted=false",
            Stream::TasksCompleted => "tasks?limit=100&isCompleted=true",
            Stream::NoteDetail => unreachable!(),
        }
        .to_owned();
        if let Some(token) = &self.cursor.next {
            if !self.stream.supports_next() || !valid_token(token) {
                return Err(malformed());
            }
            path.push_str("&next=");
            for byte in token.bytes() {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    path.push(char::from(byte));
                } else {
                    use std::fmt::Write;
                    write!(&mut path, "%{byte:02X}").map_err(|_| malformed())?;
                }
            }
        } else {
            path.push_str(&format!("&offset={}", self.cursor.offset));
        }
        Ok(path)
    }
}

pub struct Parsed {
    pub records: Vec<SourceRecord>,
    pub next: Option<Cursor>,
    pub reported_total: Option<String>,
}

pub struct SourceRecord {
    pub source_id: Option<String>,
    /// Stable UTF-8 encoding, with exact decimal numbers and sorted object keys.
    pub canonical: Vec<u8>,
    pub projection: Value,
    /// Transient PII: the persistence caller must immediately tenant-HMAC these.
    pub contact_keys: Vec<(String, String)>,
    pub content_gap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Malformed,
    DuplicateKey,
    TooDeep,
    DerivedTooLarge,
    UnrepresentableNumber,
    PaginationUncertain,
    PaginationUnsupported,
    NoProgress,
    DetailIdMismatch,
}

impl ParseError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed => "malformed_response",
            Self::DuplicateKey => "duplicate_json_key",
            Self::TooDeep => "source_nesting_exceeded",
            Self::DerivedTooLarge => "derived_data_too_large",
            Self::UnrepresentableNumber => "unrepresentable_number",
            Self::PaginationUncertain => "pagination_uncertain",
            Self::PaginationUnsupported => "pagination_unsupported",
            Self::NoProgress => "pagination_no_progress",
            Self::DetailIdMismatch => "source_id_mismatch",
        }
    }
}

pub fn parse(request: &Request, bytes: &[u8]) -> Result<Parsed, ParseError> {
    request.path().map_err(|_| ParseError::Malformed)?;
    if bytes.len() > MAX_BODY {
        return Err(ParseError::DerivedTooLarge);
    }
    let root = JsonParser::parse(bytes)?;
    if request.stream == Stream::NoteDetail {
        let record = source_record(request.stream, &root)?;
        if record.source_id.as_deref() != request.source_id.as_deref() {
            return Err(ParseError::DetailIdMismatch);
        }
        return Ok(Parsed {
            records: vec![record],
            next: None,
            reported_total: None,
        });
    }
    let items = root
        .get(request.stream.collection())
        .and_then(Node::array)
        .ok_or(ParseError::Malformed)?;
    if items.len() > PAGE_SIZE {
        return Err(ParseError::DerivedTooLarge);
    }
    let metadata = root
        .get("_metadata")
        .ok_or(ParseError::PaginationUncertain)?;
    if metadata.get("collection").and_then(Node::string) != Some(request.stream.collection())
        || metadata.get("limit").and_then(decimal).as_deref() != Some("100")
    {
        return Err(ParseError::PaginationUncertain);
    }
    let returned_offset = metadata.get("offset").and_then(decimal);
    if returned_offset.is_none()
        || (request.cursor.next.is_none()
            && returned_offset.as_deref() != Some(&request.cursor.offset.to_string()))
    {
        return Err(ParseError::PaginationUncertain);
    }
    let position = request
        .cursor
        .offset
        .checked_add(items.len() as u64)
        .ok_or(ParseError::DerivedTooLarge)?;
    let total = metadata.get("total").and_then(decimal);
    let total_coherent = total
        .as_deref()
        .is_some_and(|n| decimal_cmp(n, &position.to_string()) != std::cmp::Ordering::Less);
    let token = metadata.get("next");
    let next_link = metadata.get("nextLink");
    let has_link = next_link.is_some_and(|v| !matches!(v, Node::Null));
    if has_link && !matches!(token, Some(Node::String(_))) {
        return Err(ParseError::PaginationUncertain);
    }
    let next = match token {
        Some(Node::String(token)) => {
            if !request.stream.supports_next() {
                return Err(ParseError::PaginationUnsupported);
            }
            if !valid_token(token) {
                return Err(ParseError::PaginationUncertain);
            }
            if items.is_empty() || request.cursor.next.as_deref() == Some(token) {
                return Err(ParseError::NoProgress);
            }
            if total_coherent && total.as_deref() == Some(&position.to_string()) {
                return Err(ParseError::PaginationUncertain);
            }
            Some(Cursor {
                offset: position,
                next: Some(token.clone()),
            })
        }
        None | Some(Node::Null) => {
            if request.cursor.next.is_some() && !matches!(token, Some(Node::Null)) {
                return Err(ParseError::PaginationUncertain);
            }
            if !total_coherent {
                return Err(ParseError::PaginationUncertain);
            }
            if total.as_deref() == Some(&position.to_string()) {
                None
            } else if request.cursor.next.is_some() || items.is_empty() {
                return Err(ParseError::PaginationUncertain);
            } else {
                Some(Cursor {
                    offset: position,
                    next: None,
                })
            }
        }
        _ => return Err(ParseError::PaginationUncertain),
    };
    let mut records = Vec::with_capacity(items.len());
    let mut contact_count = 0;
    for node in items {
        let record = source_record(request.stream, node)?;
        contact_count += record.contact_keys.len();
        if contact_count > MAX_PAGE_CONTACT_KEYS {
            return Err(ParseError::DerivedTooLarge);
        }
        records.push(record);
    }
    Ok(Parsed {
        records,
        next,
        reported_total: if total_coherent { total } else { None },
    })
}

fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TOKEN && !value.chars().any(char::is_control)
}

fn decimal_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn decimal(node: &Node) -> Option<String> {
    match node {
        Node::String(value)
            if !value.is_empty()
                && value.len() <= MAX_SOURCE_ID
                && value.bytes().all(|c| c.is_ascii_digit()) =>
        {
            let value = value.trim_start_matches('0');
            Some(if value.is_empty() { "0" } else { value }.into())
        }
        Node::Number(number) if !number.starts_with('-') => {
            if number == "0" {
                return Some("0".into());
            }
            let (coefficient, exponent) = number.split_once('e')?;
            let exponent = exponent.parse::<usize>().ok()?;
            if coefficient.len().checked_add(exponent)? > MAX_SOURCE_ID {
                return None;
            }
            Some(format!("{coefficient}{}", "0".repeat(exponent)))
        }
        _ => None,
    }
}

fn positive_id(node: &Node) -> Option<String> {
    decimal(node).filter(|v| v != "0")
}

fn source_record(stream: Stream, node: &Node) -> Result<SourceRecord, ParseError> {
    let source_id = node.get("id").and_then(positive_id);
    let mut canonical = Vec::new();
    node.encode(&mut canonical);
    let content_gap = matches!(node.get("showContent"), Some(Node::Bool(false)));
    let mut contact_keys = BTreeSet::new();
    let mut contact_bytes = 0;
    if stream == Stream::People && source_id.is_some() {
        for (field, kind) in [("emails", "email"), ("phones", "phone")] {
            if let Some(items) = node.get(field).and_then(Node::array) {
                for item in items {
                    if let Some(value) = item.get("value").and_then(Node::string) {
                        let normalized = if kind == "email" {
                            normalize_email(value).map(|v| v.as_str().to_owned())
                        } else {
                            normalize_phone(value).map(|v| v.as_str().to_owned())
                        };
                        if let Some(normalized) = normalized {
                            if normalized.len() > 4096 {
                                return Err(ParseError::DerivedTooLarge);
                            }
                            if contact_keys.insert((kind.to_owned(), normalized.clone())) {
                                contact_bytes += kind.len() + normalized.len();
                                if contact_keys.len() > MAX_CONTACT_KEYS
                                    || contact_bytes > MAX_CONTACT_BYTES
                                {
                                    return Err(ParseError::DerivedTooLarge);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let projection = projection(stream.family(), node, source_id.is_none(), content_gap)?;
    Ok(SourceRecord {
        source_id,
        canonical,
        projection,
        contact_keys: contact_keys.into_iter().collect(),
        content_gap,
    })
}

fn selected(family: Family, key: &str) -> bool {
    if matches!(key, "id" | "created" | "updated" | "name") {
        return true;
    }
    match family {
        Family::Users => matches!(
            key,
            "firstName"
                | "lastName"
                | "email"
                | "phone"
                | "role"
                | "status"
                | "timezone"
                | "isOwner"
                | "groups"
                | "teamIds"
                | "teamLeaderOf"
                | "calling"
        ),
        Family::Stages => matches!(key, "orderWeight" | "isProtected" | "peopleCount"),
        Family::CustomFields => matches!(
            key,
            "label" | "type" | "choices" | "isRecurring" | "orderWeight"
        ),
        Family::People => {
            key.starts_with("custom")
                || matches!(
                    key,
                    "firstName"
                        | "lastName"
                        | "stage"
                        | "source"
                        | "sourceUrl"
                        | "assignedUserId"
                        | "assignedTo"
                        | "assignedPondId"
                        | "emails"
                        | "phones"
                        | "tags"
                        | "addresses"
                        | "relationships"
                        | "timeframeId"
                        | "timeframeUpdated"
                        | "timeframeStatus"
                        | "contacted"
                        | "price"
                )
        }
        Family::Notes => matches!(
            key,
            "personId"
                | "createdById"
                | "updatedById"
                | "createdBy"
                | "updatedBy"
                | "subject"
                | "body"
                | "type"
                | "isHtml"
                | "showContent"
                | "replies"
                | "reactions"
                | "actionPlanId"
                | "isExternal"
                | "systemId"
                | "systemName"
        ),
        Family::Tasks => matches!(
            key,
            "personId"
                | "type"
                | "isCompleted"
                | "completed"
                | "dueDate"
                | "dueDateTime"
                | "assignedUserId"
                | "AssignedTo"
                | "assignedTo"
                | "createdById"
                | "updatedById"
                | "createdBy"
                | "updatedBy"
                | "externalTaskLink"
                | "externalCalendarId"
                | "remindSecondsBefore"
        ),
    }
}

fn projection(
    family: Family,
    node: &Node,
    invalid_id: bool,
    content_gap: bool,
) -> Result<Value, ParseError> {
    let mut output = Map::new();
    let mut flags = BTreeSet::new();
    if node.has_projection_marker() {
        // The marker is reserved for this projection's trusted encoder. Preserve
        // a source collision as data and make it ineligible for interpretation.
        flags.insert("projection_marker_collision");
    }
    let mut omitted_fields = 0_u64;
    if let Node::Object(fields) = node {
        let mut attempted = 0;
        // Show ordinary identifiers/mapping fields before potentially numerous
        // custom fields. Limit projection attempts so a broad flat source object
        // cannot cause repeated serialization of an already-full projection.
        for custom_pass in [false, true] {
            for (key, value) in fields {
                if key.starts_with("custom") != custom_pass {
                    continue;
                }
                if selected(family, key) {
                    if key.len() > 256 || attempted >= 128 {
                        omitted_fields += 1;
                        flags.insert("projection_truncated");
                        continue;
                    }
                    attempted += 1;
                    let projected = project_value(value, 0, &mut flags);
                    output.insert(key.clone(), projected);
                    if serde_json::to_vec(&output)
                        .map_err(|_| ParseError::Malformed)?
                        .len()
                        > MAX_PROJECTION - 1024
                    {
                        output.remove(key);
                        omitted_fields += 1;
                        flags.insert("projection_truncated");
                    }
                } else {
                    omitted_fields += 1;
                }
            }
        }
    } else {
        flags.insert("invalid_record_shape");
    }
    if invalid_id {
        flags.insert("invalid_source_id");
    }
    if content_gap {
        flags.insert("content_unavailable");
    }
    output.insert(
        "_snapshot".into(),
        serde_json::json!({
            "projection_version": 1,
            "flags": flags.into_iter().collect::<Vec<_>>(),
            "omitted_fields": omitted_fields,
            "raw_capture_preserved_separately": true,
        }),
    );
    let output = Value::Object(output);
    if serde_json::to_vec(&output)
        .map_err(|_| ParseError::Malformed)?
        .len()
        > MAX_PROJECTION
    {
        return Err(ParseError::DerivedTooLarge);
    }
    Ok(output)
}

fn project_value(node: &Node, depth: usize, flags: &mut BTreeSet<&'static str>) -> Value {
    if depth > 6 {
        flags.insert("projection_truncated");
        return serde_json::json!({"_truncated": true});
    }
    match node {
        Node::Null => Value::Null,
        Node::Bool(value) => Value::Bool(*value),
        Node::String(value) => {
            let mut end = value.len().min(4096);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            if end < value.len() {
                flags.insert("projection_truncated");
            }
            Value::String(value[..end].into())
        }
        Node::Number(value) => {
            // Projection numbers retain their numeric JSON form only if a
            // serde round-trip preserves the exact decimal value. Otherwise the
            // explicit wrapper preserves digits without falsely claiming a float.
            let numeric_text = if let Some(positive) = value.strip_prefix('-') {
                decimal(&Node::Number(positive.into())).map(|n| format!("-{n}"))
            } else {
                decimal(node)
            };
            // Rust can round-trip u64 values that JavaScript JSON.parse cannot.
            // The browser-facing projection must also preserve those integers.
            let integer = value == "0"
                || value
                    .split_once('e')
                    .is_some_and(|(_, exponent)| exponent.parse::<i64>().is_ok_and(|e| e >= 0));
            let unsafe_integer = integer
                && numeric_text.as_deref().is_none_or(|text| {
                    decimal_cmp(text.trim_start_matches('-'), "9007199254740991")
                        == std::cmp::Ordering::Greater
                });
            let parsed =
                serde_json::from_str::<Value>(numeric_text.as_deref().unwrap_or(value)).ok();
            if let Some(Value::Number(number)) = &parsed {
                if !unsafe_integer
                    && canonical_number(&number.to_string()).as_deref() == Ok(value.as_str())
                {
                    return parsed.unwrap_or(Value::Null);
                }
            }
            flags.insert("lossless_number_wrapper");
            if value.len() > 4096 {
                flags.insert("projection_truncated");
                serde_json::json!({"_lossless_number": &value[..4096], "_truncated": true})
            } else {
                serde_json::json!({"_lossless_number": value})
            }
        }
        Node::Array(items) => {
            if items.len() > 50 {
                flags.insert("projection_truncated");
            }
            Value::Array(
                items
                    .iter()
                    .take(50)
                    .map(|v| project_value(v, depth + 1, flags))
                    .collect(),
            )
        }
        Node::Object(items) => {
            let mut result = Map::new();
            for (key, value) in items.iter().take(50) {
                if key.len() <= 256 {
                    result.insert(key.clone(), project_value(value, depth + 1, flags));
                } else {
                    flags.insert("projection_truncated");
                }
            }
            if items.len() > 50 {
                flags.insert("projection_truncated");
            }
            Value::Object(result)
        }
    }
}

/// Numbers store normalized coefficient/exponent text, never floating point.
enum Node {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Node>),
    Object(BTreeMap<String, Node>),
}

impl Node {
    fn has_projection_marker(&self) -> bool {
        match self {
            Self::Object(values) => {
                values.contains_key("_lossless_number")
                    || values.values().any(Self::has_projection_marker)
            }
            Self::Array(values) => values.iter().any(Self::has_projection_marker),
            _ => false,
        }
    }

    fn get(&self, key: &str) -> Option<&Self> {
        if let Self::Object(value) = self {
            value.get(key)
        } else {
            None
        }
    }
    fn string(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn array(&self) -> Option<&[Self]> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Null => out.extend_from_slice(b"null"),
            Self::Bool(true) => out.extend_from_slice(b"true"),
            Self::Bool(false) => out.extend_from_slice(b"false"),
            Self::Number(value) => out.extend_from_slice(value.as_bytes()),
            Self::String(value) => out.extend_from_slice(
                serde_json::to_string(value)
                    .expect("serializing a string cannot fail")
                    .as_bytes(),
            ),
            Self::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    item.encode(out);
                }
                out.push(b']');
            }
            Self::Object(items) => {
                out.push(b'{');
                for (i, (key, item)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    out.extend_from_slice(
                        serde_json::to_string(key)
                            .expect("serializing a string key cannot fail")
                            .as_bytes(),
                    );
                    out.push(b':');
                    item.encode(out);
                }
                out.push(b'}');
            }
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Node, ParseError> {
        let mut parser = Self {
            bytes,
            pos: 0,
            nodes: 0,
        };
        let value = parser.value(0)?;
        parser.whitespace();
        if parser.pos != bytes.len() {
            return Err(ParseError::Malformed);
        }
        Ok(value)
    }
    fn whitespace(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|c| b" \n\r\t".contains(c))
        {
            self.pos += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.whitespace();
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<Node, ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError::TooDeep);
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(ParseError::DerivedTooLarge);
        }
        self.whitespace();
        match self
            .bytes
            .get(self.pos)
            .copied()
            .ok_or(ParseError::Malformed)?
        {
            b'"' => Ok(Node::String(self.string()?)),
            b'{' => {
                self.pos += 1;
                let mut entries = BTreeMap::new();
                if self.take(b'}') {
                    return Ok(Node::Object(entries));
                }
                loop {
                    self.whitespace();
                    let key = self.string()?;
                    if !self.take(b':') {
                        return Err(ParseError::Malformed);
                    }
                    if entries.contains_key(&key) {
                        return Err(ParseError::DuplicateKey);
                    }
                    entries.insert(key, self.value(depth + 1)?);
                    if self.take(b'}') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err(ParseError::Malformed);
                    }
                }
                Ok(Node::Object(entries))
            }
            b'[' => {
                self.pos += 1;
                let mut entries = Vec::new();
                if self.take(b']') {
                    return Ok(Node::Array(entries));
                }
                loop {
                    entries.push(self.value(depth + 1)?);
                    if self.take(b']') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err(ParseError::Malformed);
                    }
                }
                Ok(Node::Array(entries))
            }
            b't' => {
                self.literal(b"true")?;
                Ok(Node::Bool(true))
            }
            b'f' => {
                self.literal(b"false")?;
                Ok(Node::Bool(false))
            }
            b'n' => {
                self.literal(b"null")?;
                Ok(Node::Null)
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(ParseError::Malformed),
        }
    }
    fn literal(&mut self, text: &[u8]) -> Result<(), ParseError> {
        if self.bytes.get(self.pos..self.pos + text.len()) != Some(text) {
            return Err(ParseError::Malformed);
        }
        self.pos += text.len();
        Ok(())
    }
    fn string(&mut self) -> Result<String, ParseError> {
        if self.bytes.get(self.pos) != Some(&b'"') {
            return Err(ParseError::Malformed);
        }
        let start = self.pos;
        self.pos += 1;
        loop {
            match self
                .bytes
                .get(self.pos)
                .copied()
                .ok_or(ParseError::Malformed)?
            {
                b'"' => {
                    self.pos += 1;
                    return serde_json::from_slice(&self.bytes[start..self.pos])
                        .map_err(|_| ParseError::Malformed);
                }
                b'\\' => {
                    self.pos += 2;
                }
                _ => self.pos += 1,
            }
        }
    }
    fn number(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(ParseError::Malformed),
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            let start = self.pos;
            self.digits();
            if self.pos == start {
                return Err(ParseError::Malformed);
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let start = self.pos;
            self.digits();
            if self.pos == start {
                return Err(ParseError::Malformed);
            }
        }
        let text =
            std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| ParseError::Malformed)?;
        Ok(Node::Number(canonical_number(text)?))
    }
    fn digits(&mut self) {
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
    }
}

// Coefficient/exponent form avoids exponent-sized allocations: 1e1000000 is
// represented in a few bytes. Overflow is classified, never rounded through f64.
fn canonical_number(text: &str) -> Result<String, ParseError> {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i64>()
                .map_err(|_| ParseError::UnrepresentableNumber)?,
        ),
        None => (text, 0),
    };
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let coefficient: String = mantissa.chars().filter(|c| *c != '.').collect();
    let coefficient = coefficient.trim_start_matches('0');
    if coefficient.is_empty() {
        return Ok("0".into());
    }
    let trimmed = coefficient.trim_end_matches('0');
    let exponent = exponent
        .checked_sub(fraction as i64)
        .and_then(|e| e.checked_add((coefficient.len() - trimmed.len()) as i64))
        .ok_or(ParseError::UnrepresentableNumber)?;
    Ok(format!(
        "{}{trimmed}e{exponent}",
        if negative { "-" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(stream: Stream) -> Request {
        Request {
            stream,
            cursor: Cursor::default(),
            source_id: None,
        }
    }
    fn page(stream: Stream, records: &str, meta: &str) -> Vec<u8> {
        format!(r#"{{"_metadata":{{"collection":"{}","limit":100,"offset":0,{meta}}},"{}":[{records}]}}"#, stream.collection(), stream.collection()).into_bytes()
    }
    fn canonical(text: &str) -> Result<Vec<u8>, ParseError> {
        let node = JsonParser::parse(text.as_bytes())?;
        let mut bytes = Vec::new();
        node.encode(&mut bytes);
        Ok(bytes)
    }

    #[test]
    fn profile_paths_and_representation_boundaries() {
        for stream in [
            Stream::Users,
            Stream::Stages,
            Stream::CustomFields,
            Stream::People,
            Stream::Notes,
            Stream::TasksOpen,
            Stream::TasksCompleted,
        ] {
            assert_eq!(Stream::parse(stream.as_str()), Some(stream));
            assert_eq!(
                Family::parse(stream.family().as_str()),
                Some(stream.family())
            );
            assert!(request(stream).path().unwrap().contains("limit=100"));
        }
        assert!(request(Stream::Users)
            .path()
            .unwrap()
            .contains("fields=allFields%2Ccalling&includeDeleted=true"));
        assert!(request(Stream::People)
            .path()
            .unwrap()
            .contains("includeTrash=true&includeUnclaimed=true"));
        assert!(request(Stream::TasksOpen)
            .path()
            .unwrap()
            .contains("isCompleted=false"));
        assert!(request(Stream::TasksCompleted)
            .path()
            .unwrap()
            .contains("isCompleted=true"));
        assert_eq!(
            Stream::TasksOpen.representation(),
            Stream::TasksCompleted.representation()
        );
        assert_ne!(
            Stream::Notes.representation(),
            Stream::NoteDetail.representation()
        );
        assert_eq!(Stream::CustomFields.collection(), "customfields");
    }

    #[test]
    fn token_is_encoded_data_and_never_an_executable_url() {
        let mut r = request(Stream::People);
        r.cursor.next = Some("https://evil.invalid/?steal=yes&a=+/#".into());
        let path = r.path().unwrap();
        assert!(path.starts_with("people?"));
        assert!(path.contains("next=https%3A%2F%2Fevil.invalid%2F%3Fsteal%3Dyes%26a%3D%2B%2F%23"));
        assert!(!path.contains("&offset="));
        r.cursor.next = Some("\nheader".into());
        assert!(r.path().is_err());
        r.cursor.next = Some("x".repeat(MAX_TOKEN + 1));
        assert!(r.path().is_err());
    }

    #[test]
    fn exhaustion_requires_coherent_metadata_not_just_short_or_empty_pages() {
        let r = request(Stream::Notes);
        let p = parse(&r, &page(Stream::Notes, r#"{"id":1}"#, r#""total":"2""#)).unwrap();
        assert_eq!(p.next.unwrap().offset, 1);
        assert!(parse(&r, &page(Stream::Notes, "", r#""total":1"#)).is_err());
        assert!(parse(&r, br#"{"notes":[]}"#).is_err());
        assert!(parse(&r, &page(Stream::Notes, "", r#""total":0"#))
            .unwrap()
            .next
            .is_none());
        assert!(parse(&r, &page(Stream::Notes, r#"{"id":1}"#, r#""total":0"#)).is_err());
        assert!(parse(
            &r,
            &page(
                Stream::Notes,
                r#"{"id":1}"#,
                r#""total":1,"nextLink":"https://evil.invalid""#
            )
        )
        .is_err());
        let mut wrong_offset = request(Stream::Notes);
        wrong_offset.cursor.offset = 1;
        assert!(parse(&wrong_offset, &page(Stream::Notes, "", r#""total":1"#)).is_err());
    }

    #[test]
    fn next_mode_is_explicit_and_loops_or_incomplete_terminals_pause() {
        let r = request(Stream::People);
        let p = parse(
            &r,
            &page(
                Stream::People,
                r#"{"id":1}"#,
                r#""total":2,"next":"opaque""#,
            ),
        )
        .unwrap();
        let mut next = request(Stream::People);
        next.cursor = p.next.unwrap();
        assert_eq!(
            parse(
                &next,
                &page(
                    Stream::People,
                    r#"{"id":2}"#,
                    r#""total":2,"next":"opaque""#
                )
            )
            .err(),
            Some(ParseError::NoProgress)
        );
        assert!(parse(&next, &page(Stream::People, r#"{"id":2}"#, r#""total":2"#)).is_err());
        assert!(parse(
            &next,
            &page(Stream::People, r#"{"id":2}"#, r#""total":2,"next":null"#)
        )
        .unwrap()
        .next
        .is_none());
        assert_eq!(
            parse(
                &request(Stream::Notes),
                &page(Stream::Notes, r#"{"id":1}"#, r#""total":2,"next":"opaque""#)
            )
            .err(),
            Some(ParseError::PaginationUnsupported)
        );
    }

    #[test]
    fn semantic_encoding_preserves_unknown_fields_arrays_and_large_numbers() {
        let a =
            canonical(r#"{"z":1.00,"large":9007199254740993,"a":[1,2],"unknown":{"é":"\u0061"}}"#)
                .unwrap();
        let b = canonical(
            r#"{ "unknown":{"\u00e9":"a"}, "a":[1e0,20e-1], "large":9007199254740993, "z":10e-1 }"#,
        )
        .unwrap();
        assert_eq!(a, b);
        assert_ne!(
            canonical(r#"{"large":9007199254740992}"#).unwrap(),
            canonical(r#"{"large":9007199254740993}"#).unwrap()
        );
        assert_ne!(canonical("[1,2]").unwrap(), canonical("[2,1]").unwrap());
        assert_eq!(canonical("-0.000e12").unwrap(), b"0");
        assert_eq!(
            canonical("0.123456789012345678901234567890").unwrap(),
            b"12345678901234567890123456789e-29"
        );
        assert_eq!(canonical("1e1000000").unwrap(), b"1e1000000");
    }

    #[test]
    fn duplicate_decoded_keys_malformed_numbers_and_bounded_recursion_are_classified() {
        assert_eq!(
            canonical(r#"{"id":1,"\u0069d":2}"#).err(),
            Some(ParseError::DuplicateKey)
        );
        assert_eq!(
            canonical(r#"{"x":{"a":1,"a":2}}"#).err(),
            Some(ParseError::DuplicateKey)
        );
        for invalid in [
            "01",
            "1.",
            "1e",
            "+1",
            "NaN",
            "[1,]",
            "{\"x\":1,}",
            "true false",
            "\"\\ud800\"",
        ] {
            assert!(canonical(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            canonical(&format!("{}0{}", "[".repeat(66), "]".repeat(66))).err(),
            Some(ParseError::TooDeep)
        );
        assert_eq!(
            canonical("1e9999999999999999999999999").err(),
            Some(ParseError::UnrepresentableNumber)
        );
        assert_eq!(
            canonical(&format!("[{}0]", "0,".repeat(MAX_NODES))).err(),
            Some(ParseError::DerivedTooLarge)
        );
    }

    #[test]
    fn ids_are_positive_lossless_decimal_strings_and_invalid_items_are_retained() {
        let p = parse(&request(Stream::People), &page(Stream::People, r#"{"id":184467440737095516160001},{"id":"00042"},{"id":0},{"id":-1},{"id":1.5},{}"#, r#""total":6"#)).unwrap();
        assert_eq!(
            p.records[0].source_id.as_deref(),
            Some("184467440737095516160001")
        );
        assert_eq!(p.records[1].source_id.as_deref(), Some("42"));
        assert!(p.records[2..].iter().all(|r| r.source_id.is_none()));
        assert!(p.records.iter().all(|r| !r.canonical.is_empty()));
        assert_eq!(p.records[1].projection["id"], "00042");
        assert_eq!(
            p.records[0].projection["id"]["_lossless_number"],
            "184467440737095516160001e0"
        );
    }

    #[test]
    fn projection_preserves_safe_integer_types_and_labels_exact_decimal_wrappers() {
        let p = parse(
            &request(Stream::People),
            &page(
                Stream::People,
                r#"{"id":42,"assignedUserId":7,"customNumber":0.12345678901234567890123456789}"#,
                r#""total":1"#,
            ),
        )
        .unwrap();
        assert_eq!(p.records[0].projection["id"].as_u64(), Some(42));
        assert_eq!(p.records[0].projection["assignedUserId"].as_u64(), Some(7));
        assert_eq!(
            p.records[0].projection["customNumber"]["_lossless_number"],
            "12345678901234567890123456789e-29"
        );
    }

    #[test]
    fn original_nested_projection_markers_are_untrusted_and_explicitly_flagged() {
        let p = parse(
            &request(Stream::People),
            &page(Stream::People, r#"{"id":42,"customNumber":{"_lossless_number":"1e0"},"unknown":[{"_lossless_number":"2e0"}]}"#, r#""total":1"#),
        ).unwrap();
        assert!(p.records[0].projection["_snapshot"]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "projection_marker_collision"));
        assert!(String::from_utf8_lossy(&p.records[0].canonical)
            .contains(r#""_lossless_number":"2e0""#));
        let ordinary = parse(
            &request(Stream::People),
            &page(
                Stream::People,
                r#"{"id":42,"customNumber":0.12345678901234567890123456789}"#,
                r#""total":1"#,
            ),
        )
        .unwrap();
        assert!(!ordinary.records[0].projection["_snapshot"]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "projection_marker_collision"));
    }

    #[test]
    fn browser_unsafe_integers_are_wrapped_even_when_rust_can_represent_them() {
        let p = parse(
            &request(Stream::People),
            &page(Stream::People, r#"{"id":9007199254740993,"assignedUserId":9007199254740991,"assignedPondId":9007199254740992,"customNegative":-9007199254740992,"customHuge":1e150}"#, r#""total":1"#),
        ).unwrap();
        let projection = &p.records[0].projection;
        assert_eq!(p.records[0].source_id.as_deref(), Some("9007199254740993"));
        assert_eq!(projection["id"]["_lossless_number"], "9007199254740993e0");
        assert_eq!(
            projection["assignedUserId"].as_u64(),
            Some(9007199254740991)
        );
        assert_eq!(
            projection["assignedPondId"]["_lossless_number"],
            "9007199254740992e0"
        );
        assert_eq!(
            projection["customNegative"]["_lossless_number"],
            "-9007199254740992e0"
        );
        assert_eq!(projection["customHuge"]["_lossless_number"], "1e150");
    }

    #[test]
    fn note_details_require_the_requested_id_and_expose_content_gaps() {
        let mut r = request(Stream::NoteDetail);
        r.source_id = Some("42".into());
        assert_eq!(
            r.path().unwrap(),
            "notes/42?includeThreadedReplies=true&includeReactions=true"
        );
        assert_eq!(
            parse(&r, br#"{"id":43}"#).err(),
            Some(ParseError::DetailIdMismatch)
        );
        let p = parse(
            &r,
            br#"{"id":42,"showContent":false,"body":"","replies":[],"reactions":{}}"#,
        )
        .unwrap();
        assert!(p.records[0].content_gap);
        assert!(p.next.is_none());
        r.source_id = Some("1/../../identity".into());
        assert!(r.path().is_err());
    }

    #[test]
    fn contacts_use_existing_normalization_and_projection_is_labeled_and_bounded() {
        let records = format!(
            r#"{{"id":1,"emails":[{{"value":" Ada@Example.COM "}},{{"value":"ada@example.com"}}],"phones":[{{"value":"(555) 555-0100"}}],"customText":"{}","unknown":"kept in canonical"}}"#,
            "🦀".repeat(20_000)
        );
        let p = parse(
            &request(Stream::People),
            &page(Stream::People, &records, r#""total":1"#),
        )
        .unwrap();
        assert_eq!(
            p.records[0].contact_keys,
            vec![
                ("email".into(), "ada@example.com".into()),
                ("phone".into(), "+15555550100".into())
            ]
        );
        assert!(serde_json::to_vec(&p.records[0].projection).unwrap().len() <= MAX_PROJECTION);
        assert!(p.records[0].projection["_snapshot"]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "projection_truncated"));
        assert!(String::from_utf8_lossy(&p.records[0].canonical).contains("kept in canonical"));
    }

    #[test]
    fn contact_and_flat_projection_amplification_have_explicit_bounds() {
        let contacts = (0..=MAX_CONTACT_KEYS)
            .map(|i| format!(r#"{{"value":"{i}@example.com"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let record = format!(r#"{{"id":1,"emails":[{contacts}]}}"#);
        assert_eq!(
            parse(
                &request(Stream::People),
                &page(Stream::People, &record, r#""total":1"#)
            )
            .err(),
            Some(ParseError::DerivedTooLarge)
        );
        let fields = (0..5000)
            .map(|i| format!(r#""custom{i}":"value""#))
            .collect::<Vec<_>>()
            .join(",");
        let record = format!(r#"{{"id":1,"stage":"Lead",{fields}}}"#);
        let parsed = parse(
            &request(Stream::People),
            &page(Stream::People, &record, r#""total":1"#),
        )
        .unwrap();
        let projection = &parsed.records[0].projection;
        assert_eq!(projection["id"], 1);
        assert_eq!(projection["stage"], "Lead");
        assert!(projection["_snapshot"]["omitted_fields"].as_u64().unwrap() > 4800);
        assert!(serde_json::to_vec(projection).unwrap().len() <= MAX_PROJECTION);
    }
}

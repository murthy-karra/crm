//! Frozen, GET-only `fub-history-v1` capture profile. The caller encrypts exact
//! response bytes even when parsing fails. Constructed fixtures qualify this
//! conservative adapter, not actual account visibility or a source snapshot.

use std::collections::BTreeSet;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::reader::ReaderError;
pub use super::snapshot_source::ParseError;
use super::snapshot_source::{positive_id, JsonParser, Node};

pub const PROFILE_VERSION: &str = "fub-history-v1";
pub const PARSER_VERSION: i32 = 1;
pub const PAGE_SIZE: usize = 100;
pub const MAX_PROJECTION_BYTES: usize = 6 * 1024;
pub const MAX_RECORD_PERSON_REFS: usize = 64;
pub const MAX_PAGE_PERSON_REFS: usize = 4096;
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_TOKEN: usize = 2048;
const MAX_LABEL: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Events,
    Calls,
    TextMessages,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Calls => "calls",
            Self::TextMessages => "text_messages",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "events" => Some(Self::Events),
            "calls" => Some(Self::Calls),
            "text_messages" => Some(Self::TextMessages),
            _ => None,
        }
    }

    pub fn representation(self) -> &'static str {
        match self {
            Self::Events => "fub-history-v1/events/default",
            Self::Calls => "fub-history-v1/calls/default",
            Self::TextMessages => "fub-history-v1/textMessages/default",
        }
    }

    pub fn collection(self) -> &'static str {
        match self {
            Self::TextMessages => "textmessages",
            other => other.as_str(),
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::TextMessages => "textMessages",
            other => other.as_str(),
        }
    }

    fn supports_next(self) -> bool {
        // Text continuation is explicitly provisional in the approved profile.
        matches!(self, Self::Events | Self::TextMessages)
    }
}

/// Contains source data: deliberately no Debug implementation. Offset remains
/// the cumulative returned position when an opaque next token is in use.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub offset: u64,
    pub next: Option<String>,
}

#[derive(Clone)]
pub struct Request {
    pub stream: Stream,
    pub cursor: Cursor,
}

impl Request {
    pub fn path(&self) -> Result<String, ReaderError> {
        let mut path = format!("{}?limit=100", self.stream.endpoint());
        if let Some(token) = &self.cursor.next {
            if !self.stream.supports_next() || !valid_token(token) || self.cursor.offset == 0 {
                return Err(ReaderError::MalformedResponse);
            }
            path.push_str("&next=");
            for byte in token.bytes() {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    path.push(char::from(byte));
                } else {
                    use std::fmt::Write;
                    write!(&mut path, "%{byte:02X}").map_err(|_| ReaderError::MalformedResponse)?;
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
    pub reported_total: String,
    /// Transient canonical record array, excluding pagination metadata. The
    /// worker must purpose-HMAC it, not persist a second plaintext body.
    pub page_digest_bytes: Vec<u8>,
}

pub struct SourceRecord {
    pub source_id: Option<String>,
    /// Complete canonical source record, including unknown fields. Transient.
    pub canonical: Vec<u8>,
    pub projection: Value,
    /// Canonical deduplicated source IDs; encrypted/indexed by the caller.
    pub person_refs: Vec<String>,
    pub primary_person_id: Option<String>,
    pub relationship_uncertain: bool,
}

/// Validate the entire retained identity response before applying the existing
/// positive account/user shape contract. Unknown fields are subject to the same
/// duplicate-key, numeric and structural limits as collection evidence.
pub fn parse_identity(bytes: &[u8]) -> Result<super::reader::Identity, ParseError> {
    if bytes.len() > MAX_BODY {
        return Err(ParseError::DerivedTooLarge);
    }
    JsonParser::parse(bytes)?;
    super::reader::parse_identity(bytes).map_err(|_| ParseError::Malformed)
}

pub fn parse(
    request: &Request,
    bytes: &[u8],
    frozen_total: Option<&str>,
) -> Result<Parsed, ParseError> {
    request.path().map_err(|_| ParseError::Malformed)?;
    if bytes.len() > MAX_BODY {
        return Err(ParseError::DerivedTooLarge);
    }
    let root = JsonParser::parse(bytes)?;
    let collection = root
        .get(request.stream.collection())
        .ok_or(ParseError::Malformed)?;
    let items = collection.array().ok_or(ParseError::Malformed)?;
    if items.len() > PAGE_SIZE {
        return Err(ParseError::DerivedTooLarge);
    }
    let metadata = root
        .get("_metadata")
        .ok_or(ParseError::PaginationUncertain)?;
    if metadata.get("collection").and_then(Node::string) != Some(request.stream.collection())
        || metadata.get("limit").and_then(decimal).as_deref() != Some("100")
        || metadata.get("offset").and_then(decimal).as_deref()
            != Some(request.cursor.offset.to_string().as_str())
    {
        return Err(ParseError::PaginationUncertain);
    }
    let total = metadata
        .get("total")
        .and_then(decimal)
        .ok_or(ParseError::PaginationUncertain)?;
    if decimal_cmp(&total, "9223372036854775807").is_gt() {
        return Err(ParseError::PaginationUncertain);
    }
    if (request.cursor.offset > 0 && frozen_total.is_none())
        || frozen_total.is_some_and(|frozen| frozen != total)
    {
        return Err(ParseError::PaginationUncertain);
    }
    let position = request
        .cursor
        .offset
        .checked_add(items.len() as u64)
        .ok_or(ParseError::DerivedTooLarge)?;
    let position_text = position.to_string();
    if decimal_cmp(&position_text, &total).is_gt() {
        return Err(ParseError::PaginationUncertain);
    }
    let token = match metadata.get("next") {
        None | Some(Node::Null) => None,
        Some(Node::String(token)) if valid_token(token) => Some(token),
        _ => return Err(ParseError::PaginationUncertain),
    };
    if token.is_some() && !request.stream.supports_next() {
        return Err(ParseError::PaginationUnsupported);
    }
    if token.is_some_and(|token| request.cursor.next.as_ref() == Some(token)) {
        return Err(ParseError::NoProgress);
    }
    let next = if position_text == total {
        if token.is_some()
            || (items.is_empty() && request.cursor.offset != 0)
            || !matches!(metadata.get("nextLink"), None | Some(Node::Null))
            || (request.cursor.next.is_some() && !matches!(metadata.get("next"), Some(Node::Null)))
        {
            return Err(ParseError::PaginationUncertain);
        }
        None
    } else {
        if items.len() != PAGE_SIZE {
            return Err(ParseError::PaginationUncertain);
        }
        if request.cursor.next.is_some() && token.is_none() {
            return Err(ParseError::PaginationUncertain);
        }
        Some(Cursor {
            offset: position,
            next: token.cloned(),
        })
    };

    let mut records = Vec::with_capacity(items.len());
    let mut page_refs = 0;
    for item in items {
        let (record, refs) = source_record(request.stream, item)?;
        page_refs += refs;
        if page_refs > MAX_PAGE_PERSON_REFS {
            return Err(ParseError::DerivedTooLarge);
        }
        records.push(record);
    }
    let mut page_digest_bytes = Vec::new();
    collection.encode(&mut page_digest_bytes);
    // Cross-page token/page fingerprints and terminal distinct-ID reconciliation
    // require durable run state and are deliberately enforced by the store.
    Ok(Parsed {
        records,
        next,
        reported_total: total,
        page_digest_bytes,
    })
}

fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TOKEN && !value.chars().any(char::is_control)
}

fn decimal(node: &Node) -> Option<String> {
    positive_id(node).or_else(|| match node {
        Node::Number(value) if value == "0" => Some("0".into()),
        Node::String(value)
            if !value.is_empty()
                && value.len() <= 128
                && value.bytes().all(|byte| byte == b'0') =>
        {
            Some("0".into())
        }
        _ => None,
    })
}

fn decimal_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[derive(Default)]
struct References {
    ids: BTreeSet<String>,
    occurrences: usize,
    uncertain: bool,
}

impl References {
    fn add(&mut self, node: &Node) -> Result<(), ParseError> {
        self.occurrences += 1;
        if self.occurrences > MAX_RECORD_PERSON_REFS {
            return Err(ParseError::DerivedTooLarge);
        }
        if let Some(id) = positive_id(node) {
            self.ids.insert(id);
        } else {
            self.uncertain = true;
        }
        Ok(())
    }
}

/// Inventory known and unexpected Person-reference keys without interpreting
/// relationships or matching phone/contact values. Unknown structures remain raw.
fn references(
    node: &Node,
    known_person_object: bool,
    refs: &mut References,
) -> Result<(), ParseError> {
    match node {
        Node::Object(fields) => {
            for (key, value) in fields {
                match key.as_str() {
                    "personId" => {
                        refs.add(value)?;
                        refs.uncertain |= !known_person_object;
                    }
                    "personIds" => {
                        refs.uncertain = true;
                        if let Some(values) = value.array() {
                            for value in values {
                                refs.add(value)?;
                            }
                        } else {
                            refs.add(value)?;
                        }
                    }
                    "relationshipId"
                        if !matches!(value, Node::Null)
                            && decimal(value).as_deref() != Some("0") =>
                    {
                        refs.uncertain = true
                    }
                    "participants" => {
                        let Some(participants) = value.array() else {
                            refs.uncertain = true;
                            references(value, false, refs)?;
                            continue;
                        };
                        if participants.len() > MAX_RECORD_PERSON_REFS {
                            return Err(ParseError::DerivedTooLarge);
                        }
                        for participant in participants {
                            let known =
                                participant.get("type").and_then(Node::string) == Some("person");
                            if !known || participant.get("personId").is_none() {
                                refs.uncertain = true;
                            }
                            references(participant, known, refs)?;
                        }
                        continue;
                    }
                    _ => {}
                }
                references(value, false, refs)?;
            }
        }
        Node::Array(values) => {
            for value in values {
                references(value, false, refs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn timestamp(node: Option<&Node>) -> Option<String> {
    DateTime::parse_from_rfc3339(node?.string()?)
        .ok()
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::AutoSi, true)
        })
}

fn label(node: Option<&Node>, truncated: &mut bool) -> Option<String> {
    let value = node?.string()?;
    // Raw source retains these verbatim; the metadata surface does not expose
    // source markup, arbitrary URLs, or control characters in display labels.
    let has_scheme = value
        .trim_start()
        .split_once(':')
        .is_some_and(|(scheme, _)| {
            scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
        });
    if value.contains(['<', '>'])
        || value.contains("://")
        || value
            .as_bytes()
            .windows(4)
            .any(|bytes| bytes.eq_ignore_ascii_case(b"www."))
        || value.trim_start().starts_with("//")
        || has_scheme
        || value.chars().any(char::is_control)
    {
        *truncated = true;
        return None;
    }
    let mut end = value.len().min(MAX_LABEL);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    *truncated |= end < value.len();
    Some(value[..end].into())
}

fn source_record(stream: Stream, node: &Node) -> Result<(SourceRecord, usize), ParseError> {
    let source_id = node.get("id").and_then(positive_id);
    let primary_person_id = node.get("personId").and_then(positive_id);
    let mut refs = References::default();
    references(node, true, &mut refs)?;
    let relationship_uncertain =
        refs.uncertain || refs.ids.len() > 1 || primary_person_id.is_none();
    let mut canonical = Vec::new();
    node.encode(&mut canonical);
    let source_users = [
        "userId",
        "createdById",
        "updatedById",
        "createdBy",
        "updatedBy",
    ]
    .into_iter()
    .filter_map(|key| node.get(key).and_then(positive_id))
    .collect::<BTreeSet<_>>();
    let created = timestamp(node.get("created"));
    let updated = timestamp(node.get("updated"));
    // Presence is the only qualified claim. A vendor flag or replacement text
    // does not establish accessibility, redaction, or complete content.
    let content = if ["message", "body", "text", "note", "description", "subject"]
        .into_iter()
        .any(|key| {
            node.get(key)
                .is_some_and(|value| !matches!(value, Node::Null))
        }) {
        "returned_in_raw"
    } else {
        "not_returned"
    };
    let mut projection = json!({
        "source_id": source_id,
        "source_person_id": primary_person_id,
        "source_user_ids": source_users,
        "source_created": created,
        "source_updated": updated,
        "source_timestamp_uncertain": created.is_none() || updated.is_none(),
        "source_kind": stream.as_str(),
        "preview_truncated": false,
        "relationship_uncertain": relationship_uncertain,
        "content_availability": content,
    });
    let mut truncated = false;
    if stream == Stream::Events {
        projection["source_event_type"] = json!(label(node.get("type"), &mut truncated));
    }
    if stream == Stream::Calls {
        projection["source_outcome"] = json!(label(node.get("outcome"), &mut truncated));
        projection["source_duration"] = json!(node.get("duration").and_then(decimal));
    }
    if matches!(stream, Stream::Calls | Stream::TextMessages) {
        projection["source_is_incoming"] = match node.get("isIncoming") {
            Some(Node::Bool(value)) => Value::Bool(*value),
            _ => Value::Null,
        };
    }
    projection["preview_truncated"] = Value::Bool(truncated);
    if serde_json::to_vec(&projection)
        .map_err(|_| ParseError::Malformed)?
        .len()
        > MAX_PROJECTION_BYTES
    {
        return Err(ParseError::DerivedTooLarge);
    }
    Ok((
        SourceRecord {
            source_id,
            canonical,
            projection,
            person_refs: refs.ids.into_iter().collect(),
            primary_person_id,
            relationship_uncertain,
        },
        refs.occurrences,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_validates_all_json_before_using_the_positive_identity_shape() {
        let valid = parse_identity(
            br#"{"account":{"id":17,"domain":"Synthetic"},"user":{"id":3,"name":"Admin"},"unknown":[true,null,1]}"#,
        )
        .unwrap();
        assert_eq!(valid.account_id, 17);
        assert_eq!(valid.user_id, Some(3));
        assert_eq!(valid.account_domain.as_deref(), Some("Synthetic"));
        assert_eq!(valid.display_name.as_deref(), Some("Admin"));
        for bytes in [
            br#"{"account":{"id":17},"\u0061ccount":{"id":99},"user":{"id":3}}"#.as_slice(),
            br#"{"account":{"id":17},"user":{"id":3},"\u0075ser":{"id":9}}"#,
            br#"{"account":{"id":99,"\u0069d":17},"user":{"id":3}}"#,
            br#"{"account":{"id":17},"user":{"id":9,"\u0069d":3}}"#,
            br#"{"account":{"id":17},"user":{"id":3},"unknown":{"x":1,"\u0078":2}}"#,
        ] {
            assert_eq!(parse_identity(bytes).err(), Some(ParseError::DuplicateKey));
        }
        for bytes in [
            br#"{"account":{"id":0},"user":{"id":3}}"#.as_slice(),
            br#"{"account":{"id":17},"user":{"id":-1}}"#,
            br#"{"account":{"id":"17"},"user":{"id":3}}"#,
            br#"{"account":{"id":17},"user":{"id":9223372036854775808}}"#,
            br#"{"account":{"id":17},"user":{"id":3}} trailing"#,
        ] {
            assert_eq!(parse_identity(bytes).err(), Some(ParseError::Malformed));
        }
        assert_eq!(
            parse_identity(
                br#"{"account":{"id":17},"user":{"id":3},"unknown":1e9223372036854775808}"#
            )
            .err(),
            Some(ParseError::UnrepresentableNumber)
        );
        let deep = format!(
            "{{\"account\":{{\"id\":17}},\"user\":{{\"id\":3}},\"unknown\":{}0{}}}",
            "[".repeat(65),
            "]".repeat(65)
        );
        assert_eq!(
            parse_identity(deep.as_bytes()).err(),
            Some(ParseError::TooDeep)
        );
        let nodes = format!(
            "{{\"account\":{{\"id\":17}},\"user\":{{\"id\":3}},\"unknown\":[{}0]}}",
            "0,".repeat(100_000)
        );
        assert_eq!(
            parse_identity(nodes.as_bytes()).err(),
            Some(ParseError::DerivedTooLarge)
        );
        assert_eq!(
            parse_identity(&vec![b' '; MAX_BODY + 1]).err(),
            Some(ParseError::DerivedTooLarge)
        );
    }

    fn request(stream: Stream) -> Request {
        Request {
            stream,
            cursor: Cursor::default(),
        }
    }

    fn page(stream: Stream, records: &str, offset: u64, total: &str, extra: &str) -> Vec<u8> {
        format!(
            "{{\"{}\":[{}],\"_metadata\":{{\"collection\":\"{}\",\"limit\":100,\"offset\":{},\"total\":{} {}}}}}",
            stream.collection(), records, stream.collection(), offset, total, extra,
        ).into_bytes()
    }

    fn records(first: u64, count: usize) -> String {
        (first..first + count as u64)
            .map(|id| format!("{{\"id\":{id},\"personId\":1}}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn error(request: &Request, bytes: &[u8], total: Option<&str>) -> ParseError {
        parse(request, bytes, total)
            .err()
            .expect("constructed invalid page must fail")
    }

    #[test]
    fn fixed_requests_have_only_profile_parameters_and_encode_opaque_tokens() {
        for (stream, path) in [
            (Stream::Events, "events?limit=100&offset=0"),
            (Stream::Calls, "calls?limit=100&offset=0"),
            (Stream::TextMessages, "textMessages?limit=100&offset=0"),
        ] {
            assert_eq!(request(stream).path().unwrap(), path);
            assert_eq!(Stream::parse(stream.as_str()), Some(stream));
        }
        assert_eq!(Stream::parse("emails"), None);
        let req = Request {
            stream: Stream::Events,
            cursor: Cursor {
                offset: 100,
                next: Some("https://evil.invalid/?fields=allFields&personId=9#雪".into()),
            },
        };
        assert_eq!(req.path().unwrap(), "events?limit=100&next=https%3A%2F%2Fevil.invalid%2F%3Ffields%3DallFields%26personId%3D9%23%E9%9B%AA");
        assert_eq!(
            Request {
                stream: Stream::Calls,
                ..req.clone()
            }
            .path(),
            Err(ReaderError::MalformedResponse)
        );
        for token in [
            "".to_owned(),
            "a\nb".into(),
            "a\u{85}b".into(),
            "x".repeat(MAX_TOKEN + 1),
        ] {
            let req = Request {
                stream: Stream::Events,
                cursor: Cursor {
                    offset: 100,
                    next: Some(token),
                },
            };
            assert_eq!(req.path(), Err(ReaderError::MalformedResponse));
        }
        assert!(serde_json::from_str::<Cursor>(
            r#"{"offset":0,"next":null,"url":"https://evil.invalid"}"#
        )
        .is_err());
    }

    #[test]
    fn offset_and_token_modes_have_explicit_terminal_evidence() {
        for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
            let empty = parse(&request(stream), &page(stream, "", 0, "0", ""), None).unwrap();
            assert!(empty.next.is_none());
            let first = parse(
                &request(stream),
                &page(stream, &records(1, 100), 0, "101", ""),
                None,
            )
            .unwrap();
            let next = first.next.unwrap();
            assert_eq!(next.offset, 100);
            assert!(next.next.is_none());
            let last = Request {
                stream,
                cursor: next,
            };
            assert!(parse(
                &last,
                &page(stream, &records(101, 1), 100, "101", ""),
                Some("101")
            )
            .unwrap()
            .next
            .is_none());
        }
        for stream in [Stream::Events, Stream::TextMessages] {
            let first = parse(
                &request(stream),
                &page(
                    stream,
                    &records(1, 100),
                    0,
                    "200",
                    r#", "next":"token", "nextLink":"https://evil.invalid""#,
                ),
                None,
            )
            .unwrap();
            let req = Request {
                stream,
                cursor: first.next.unwrap(),
            };
            assert_eq!(
                req.path().unwrap(),
                format!("{}?limit=100&next=token", stream.endpoint())
            );
            assert_eq!(
                error(
                    &req,
                    &page(stream, &records(101, 100), 100, "200", ""),
                    Some("200")
                ),
                ParseError::PaginationUncertain
            );
            assert!(parse(
                &req,
                &page(
                    stream,
                    &records(101, 100),
                    100,
                    "200",
                    r#", "next":null, "nextLink":null"#
                ),
                Some("200")
            )
            .unwrap()
            .next
            .is_none());
        }
    }

    #[test]
    fn metadata_short_pages_changed_totals_and_no_fallback_fail_closed() {
        let stream = Stream::Events;
        let req = request(stream);
        for bytes in [
            page(stream, "", 0, "1", ""),
            page(stream, &records(1, 99), 0, "100", ""),
            page(stream, &records(1, 99), 0, "100", r#", "next":"t""#),
            page(stream, &records(1, 1), 0, "0", ""),
            page(stream, &records(1, 1), 1, "1", ""),
            page(stream, "", 0, "0", r#", "nextLink":"https://evil.invalid""#),
            page(stream, &records(1, 1), 0, "1", r#", "next":"t""#),
            page(stream, "", 0, "null", ""),
            page(stream, "", 0, "-1", ""),
            page(stream, "", 0, "1.5", ""),
            page(stream, "", 0, "0", r#", "next":false"#),
        ] {
            assert_eq!(error(&req, &bytes, None), ParseError::PaginationUncertain);
        }
        assert_eq!(
            error(&req, &page(stream, &records(1, 1), 0, "1", ""), Some("2")),
            ParseError::PaginationUncertain
        );
        assert_eq!(
            error(
                &req,
                &page(stream, &records(1, 100), 0, "9223372036854775808", ""),
                None
            ),
            ParseError::PaginationUncertain
        );
        assert!(parse(
            &req,
            &page(stream, &records(1, 100), 0, "9223372036854775807", ""),
            None
        )
        .unwrap()
        .next
        .is_some());
        let token_req = Request {
            stream,
            cursor: Cursor {
                offset: 100,
                next: Some("t".into()),
            },
        };
        assert_eq!(
            error(
                &token_req,
                &page(stream, &records(101, 100), 100, "200", r#", "next":null"#),
                None
            ),
            ParseError::PaginationUncertain
        );
        assert_eq!(
            error(
                &token_req,
                &page(stream, &records(101, 100), 100, "300", r#", "next":"t""#),
                Some("300")
            ),
            ParseError::NoProgress
        );
        assert_eq!(
            error(
                &token_req,
                &page(stream, &records(101, 100), 100, "300", ""),
                Some("300")
            ),
            ParseError::PaginationUncertain
        );
        assert_eq!(
            error(
                &token_req,
                &page(stream, &records(101, 100), 0, "300", r#", "next":"u""#),
                Some("300")
            ),
            ParseError::PaginationUncertain
        );
        assert_eq!(
            error(
                &request(Stream::Calls),
                &page(Stream::Calls, &records(1, 100), 0, "200", r#", "next":"t""#),
                None
            ),
            ParseError::PaginationUnsupported
        );
        for bytes in [
            br#"{"events":[],"_metadata":{"collection":"events","limit":100,"offset":0}}"#
                .as_slice(),
            br#"{"events":[],"_metadata":{"collection":"calls","limit":100,"offset":0,"total":0}}"#,
            br#"{"events":[],"_metadata":{"collection":"events","limit":50,"offset":0,"total":0}}"#,
        ] {
            assert_eq!(error(&req, bytes, None), ParseError::PaginationUncertain);
        }
    }

    #[test]
    fn lossless_canonical_records_keep_unknown_data_and_ignore_object_order() {
        let stream = Stream::Events;
        let a = page(
            stream,
            r#"{"id":184467440737095516160001,"personId":"00042","unknown":{"b":1.00,"a":-1e150},"items":[2,1]}"#,
            0,
            "1",
            "",
        );
        let b = page(
            stream,
            r#"{"items":[2.0,1e0],"unknown":{"a":-10e149,"b":1},"personId":"00042","id":184467440737095516160001}"#,
            0,
            "1",
            r#", "next":null"#,
        );
        let a = parse(&request(stream), &a, None).unwrap();
        let b = parse(&request(stream), &b, None).unwrap();
        assert_eq!(a.records[0].canonical, b.records[0].canonical);
        assert_eq!(a.page_digest_bytes, b.page_digest_bytes);
        assert_eq!(
            a.records[0].source_id.as_deref(),
            Some("184467440737095516160001")
        );
        assert_eq!(a.records[0].primary_person_id.as_deref(), Some("42"));
        assert!(String::from_utf8_lossy(&a.records[0].canonical).contains("unknown"));
        let changed = parse(&request(stream), &page(stream, r#"{"id":184467440737095516160001,"personId":"00042","unknown":{"b":1,"a":-1e150},"items":[1,2]}"#, 0, "1", ""), None).unwrap();
        assert_ne!(a.page_digest_bytes, changed.page_digest_bytes);
        assert!(a.records[0].projection.get("unknown").is_none());
    }

    #[test]
    fn invalid_ids_and_same_page_variants_remain_observations_for_store_reconciliation() {
        let huge = "9".repeat(129);
        let body = format!(
            r#"{{"id":0}},{{"id":-1}},{{"id":1.5}},{{"id":"{huge}"}},{{}},null,{{"id":7}},{{"id":7,"note":"changed"}}"#
        );
        let parsed = parse(
            &request(Stream::Calls),
            &page(Stream::Calls, &body, 0, "8", ""),
            None,
        )
        .unwrap();
        assert_eq!(parsed.records.len(), 8);
        assert!(parsed.records[..6]
            .iter()
            .all(|record| record.source_id.is_none()));
        assert_eq!(parsed.records[6].source_id, parsed.records[7].source_id);
        assert_ne!(parsed.records[6].canonical, parsed.records[7].canonical);
        // A terminal parser candidate is NOT stream enumeration; the store must
        // retain and pause these invalid/duplicate identities before completion.
        assert!(parsed.next.is_none());
        let max_id = "9".repeat(128);
        let parsed = parse(
            &request(Stream::Calls),
            &page(Stream::Calls, &format!(r#"{{"id":{max_id}}}"#), 0, "1", ""),
            None,
        )
        .unwrap();
        assert_eq!(
            parsed.records[0].source_id.as_deref(),
            Some(max_id.as_str())
        );
    }

    #[test]
    fn duplicate_decoded_keys_input_depth_nodes_and_record_count_are_bounded() {
        let req = request(Stream::Events);
        assert_eq!(
            error(
                &req,
                &page(Stream::Events, r#"{"id":1,"\u0069d":2}"#, 0, "1", ""),
                None
            ),
            ParseError::DuplicateKey
        );
        assert_eq!(
            error(
                &req,
                &page(Stream::Events, &records(1, 101), 0, "101", ""),
                None
            ),
            ParseError::DerivedTooLarge
        );
        assert_eq!(
            error(&req, &vec![b' '; MAX_BODY + 1], None),
            ParseError::DerivedTooLarge
        );
        let deep = format!(
            r#"{{"id":1,"unknown":{}0{}}}"#,
            "[".repeat(65),
            "]".repeat(65)
        );
        assert_eq!(
            error(&req, &page(Stream::Events, &deep, 0, "1", ""), None),
            ParseError::TooDeep
        );
        let many = format!(r#"{{"id":1,"unknown":[{}]}}"#, vec!["0"; 100_000].join(","));
        assert_eq!(
            error(&req, &page(Stream::Events, &many, 0, "1", ""), None),
            ParseError::DerivedTooLarge
        );
        for record in [
            r#"{"id":1,"bad":1e999999999999999999999999}"#,
            r#"{"id":1,"bad":01}"#,
        ] {
            assert!(parse(&req, &page(Stream::Events, record, 0, "1", ""), None).is_err());
        }
    }

    #[test]
    fn group_and_unknown_relationships_keep_all_valid_references_without_contact_matching() {
        let record = r#"{"id":1,"personId":7,"participants":[{"type":"person","personId":8,"relationshipId":0,"phone":"+12025550101"},{"type":"person","personId":7},{"type":"relationship","personId":0,"relationshipId":9}],"unknown":{"personId":10},"personIds":["00011",7]}"#;
        let parsed = parse(
            &request(Stream::TextMessages),
            &page(Stream::TextMessages, record, 0, "1", ""),
            None,
        )
        .unwrap();
        let record = &parsed.records[0];
        assert_eq!(record.primary_person_id.as_deref(), Some("7"));
        assert_eq!(record.person_refs, ["10", "11", "7", "8"]);
        assert!(record.relationship_uncertain);
        let projected = serde_json::to_string(&record.projection).unwrap();
        assert!(!projected.contains("12025550101"));
        assert!(!projected.contains("relationshipId"));
        let simple = parse(&request(Stream::TextMessages), &page(Stream::TextMessages, r#"{"id":1,"personId":7,"participants":[{"type":"person","personId":7,"relationshipId":0}]}"#, 0, "1", ""), None).unwrap();
        assert!(!simple.records[0].relationship_uncertain);
    }

    #[test]
    fn relationship_derivation_caps_count_duplicates_invalid_ids_and_whole_page() {
        let participants = |count| vec![r#"{"type":"person","personId":1}"#; count].join(",");
        let record = |count| {
            format!(
                r#"{{"id":1,"personId":1,"participants":[{}]}}"#,
                participants(count)
            )
        };
        let req = request(Stream::TextMessages);
        assert!(parse(&req, &page(req.stream, &record(63), 0, "1", ""), None).is_ok());
        assert_eq!(
            error(&req, &page(req.stream, &record(64), 0, "1", ""), None),
            ParseError::DerivedTooLarge
        );
        let many = vec![record(63); 65].join(",");
        assert_eq!(
            error(&req, &page(req.stream, &many, 0, "65", ""), None),
            ParseError::DerivedTooLarge
        );
        let invalid = format!(r#"{{"id":1,"personIds":[{}]}}"#, vec!["0"; 65].join(","));
        assert_eq!(
            error(&req, &page(req.stream, &invalid, 0, "1", ""), None),
            ParseError::DerivedTooLarge
        );
    }

    #[test]
    fn projections_exclude_content_urls_markup_and_unqualified_timestamps() {
        let raw = r#"{"id":1,"personId":2,"userId":9007199254740993,"createdBy":3,"updatedById":"0004","created":"2026-09-12T01:00:00-07:00","updated":"yesterday","message":"PRIVATE_MESSAGE","note":"PRIVATE_NOTE","subject":"PRIVATE_SUBJECT","description":"PRIVATE_DESCRIPTION","phone":"PRIVATE_PHONE","userName":"PRIVATE_NAME","recordingUrl":"https://evil.invalid/audio","media":[{"url":"https://evil.invalid/media"}],"outcome":"<img src=x onerror=alert(1)>","isIncoming":false,"duration":42}"#;
        let parsed = parse(
            &request(Stream::Calls),
            &page(Stream::Calls, raw, 0, "1", ""),
            None,
        )
        .unwrap();
        let projection = &parsed.records[0].projection;
        let output = serde_json::to_string(projection).unwrap();
        assert!(!output.contains("PRIVATE_"));
        assert!(!output.contains("evil.invalid"));
        assert!(!output.contains("onerror"));
        assert_eq!(projection["source_created"], "2026-09-12T08:00:00Z");
        assert!(projection["source_updated"].is_null());
        assert_eq!(projection["source_timestamp_uncertain"], true);
        assert_eq!(
            projection["source_user_ids"],
            json!(["3", "4", "9007199254740993"])
        );
        assert_eq!(projection["source_duration"], "42");
        assert_eq!(projection["source_is_incoming"], false);
        assert_eq!(projection["content_availability"], "returned_in_raw");
        assert_eq!(projection["preview_truncated"], true);
        assert!(String::from_utf8_lossy(&parsed.records[0].canonical).contains("PRIVATE_MESSAGE"));
        for value in [
            "https://evil.invalid",
            "javascript:alert(1)",
            "data:text/html,secret",
            "WWW.evil.invalid",
            "  javascript:alert(1)",
            "//evil.invalid",
        ] {
            let record = serde_json::to_string(&json!({"id":1,"type":value})).unwrap();
            let parsed = parse(
                &request(Stream::Events),
                &page(Stream::Events, &record, 0, "1", ""),
                None,
            )
            .unwrap();
            assert!(parsed.records[0].projection["source_event_type"].is_null());
            assert_eq!(parsed.records[0].projection["preview_truncated"], true);
        }
    }

    #[test]
    fn labels_are_unicode_safe_and_worst_case_projection_keeps_envelope_headroom() {
        let large = "9".repeat(128);
        let raw = serde_json::to_string(&json!({"id":large,"personId":large,"userId":large,
            "createdById":large,"updatedById":large,"outcome":"雪".repeat(300),"duration":large,
            "message":"[This text message was sent by another system]"}))
        .unwrap();
        let parsed = parse(
            &request(Stream::Calls),
            &page(Stream::Calls, &raw, 0, "1", ""),
            None,
        )
        .unwrap();
        let projection = &parsed.records[0].projection;
        assert!(projection["source_outcome"].as_str().unwrap().len() <= MAX_LABEL);
        assert_eq!(projection["preview_truncated"], true);
        assert_eq!(projection["content_availability"], "returned_in_raw");
        assert!(serde_json::to_vec(projection).unwrap().len() <= MAX_PROJECTION_BYTES);
        let hidden = parse(
            &request(Stream::TextMessages),
            &page(
                Stream::TextMessages,
                r#"{"id":1,"showContent":false,"message":"private"}"#,
                0,
                "1",
                "",
            ),
            None,
        )
        .unwrap();
        assert_eq!(
            hidden.records[0].projection["content_availability"],
            "returned_in_raw"
        );
        let flag_only = parse(
            &request(Stream::TextMessages),
            &page(
                Stream::TextMessages,
                r#"{"id":1,"showContent":false}"#,
                0,
                "1",
                "",
            ),
            None,
        )
        .unwrap();
        assert_eq!(
            flag_only.records[0].projection["content_availability"],
            "not_returned"
        );
    }
}

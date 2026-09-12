//! New interpretation of authenticated original records. Capture v1 stays frozen.
use super::{
    history_capture_source::Stream,
    snapshot_source::{positive_id, JsonParser, Node},
    MigrationError,
};
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use serde_json::{json, Value};

pub const INTERPRETATION: &str = "fub-history-interpretation-v1";
pub const READER: &str = "fub-history-timeline-v1";
pub const DISPLAY_BYTES: usize = 4096;
pub const RECORD_RESERVATION: i64 = 32 * 1024;

pub struct Interpretation {
    pub created: Option<DateTime<Utc>>,
    pub metadata: Value,
}

fn date(value: Option<&Node>) -> Option<DateTime<Utc>> {
    let text = value?.string()?;
    if text.len() > 64 || text.ends_with("-00:00") {
        return None;
    }
    let value = DateTime::parse_from_rfc3339(text).ok()?.with_timezone(&Utc);
    if value.timestamp_subsec_nanos() >= 1_000_000_000
        || value.timestamp_subsec_nanos() % 1000 != 0
        || value.year() < -4712
    {
        return None;
    }
    Some(value)
}
fn wire_date(value: Option<DateTime<Utc>>) -> Value {
    json!(value.map(|v| v.to_rfc3339_opts(SecondsFormat::AutoSi, true)))
}
fn label(value: Option<&Node>, truncated: &mut bool) -> Option<String> {
    let text = value?.string()?;
    let scheme = text.trim_start().split_once(':').is_some_and(|(s, _)| {
        !s.is_empty()
            && s.bytes()
                .all(|v| v.is_ascii_alphanumeric() || b"+.-".contains(&v))
    });
    if text.contains(['<', '>'])
        || text.contains("://")
        || text.trim_start().starts_with("//")
        || scheme
        || text
            .as_bytes()
            .windows(4)
            .any(|v| v.eq_ignore_ascii_case(b"www."))
        || text.chars().any(char::is_control)
    {
        *truncated = true;
        return None;
    }
    let mut end = text.len().min(256);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    *truncated |= end < text.len();
    Some(text[..end].into())
}
fn duration(value: Option<&Node>) -> Option<String> {
    let Node::Number(text) = value? else {
        return None;
    };
    if text.len() > 128 || text.starts_with('-') {
        return None;
    }
    let finite = text.parse::<f64>().ok()?;
    if !finite.is_finite() || finite < 0.0 {
        return None;
    }
    Some(text.clone())
}

// Both aliases are retained evidence. Inconsistent references do not establish
// a role, and must not be resolved by whichever field happened to be checked
// first. A lone qualified alias is usable without manufacturing a local User.
fn role_id(primary: Option<&Node>, alias: Option<&Node>) -> Option<String> {
    match (primary, alias) {
        (Some(primary), Some(alias)) => {
            let primary = positive_id(primary)?;
            (positive_id(alias).as_ref() == Some(&primary)).then_some(primary)
        }
        (Some(value), None) | (None, Some(value)) => positive_id(value),
        (None, None) => None,
    }
}
pub fn interpret(
    stream: Stream,
    canonical: &[u8],
    access_user: i64,
) -> Result<Interpretation, MigrationError> {
    let node = JsonParser::parse(canonical).map_err(|_| MigrationError::Crypto)?;
    if access_user <= 0 {
        return Err(MigrationError::Crypto);
    }
    let created = date(node.get("created"));
    let mut truncated = false;
    let mut metadata = json!({
        "source_id":node.get("id").and_then(positive_id),
        "source_person_id":node.get("personId").and_then(positive_id),
        "source_access_user_id":access_user.to_string(),
        "source_attributed_user_id":node.get("userId").and_then(positive_id),
        "source_creator_user_id":role_id(node.get("createdById"),node.get("createdBy")),
        "source_editor_user_id":role_id(node.get("updatedById"),node.get("updatedBy")),
        "source_created":wire_date(created),"source_updated":wire_date(date(node.get("updated"))),
        "source_sent":if stream==Stream::TextMessages{wire_date(date(node.get("sent")))}else{Value::Null},
        "source_event_type":Value::Null,"source_note_id":Value::Null,"source_is_incoming":Value::Null,
        "source_duration":Value::Null,"source_duration_unit":Value::Null,"source_outcome":Value::Null,
        "source_status":Value::Null,
        "content_availability":if ["message","body","text","note","description","subject"].into_iter().any(|k|node.get(k).is_some_and(|v|!matches!(v,Node::Null))){"returned_in_raw"}else{"not_returned"},
        "preview_truncated":false
    });
    if stream == Stream::Events {
        metadata["source_event_type"] = json!(label(node.get("type"), &mut truncated));
        metadata["source_note_id"] = json!(node.get("noteId").and_then(positive_id));
    }
    if matches!(stream, Stream::Calls | Stream::TextMessages) {
        metadata["source_is_incoming"] = match node.get("isIncoming") {
            Some(Node::Bool(v)) => json!(v),
            _ => Value::Null,
        };
    }
    if stream == Stream::Calls {
        metadata["source_duration"] = json!(duration(node.get("duration")));
        if !metadata["source_duration"].is_null() {
            metadata["source_duration_unit"] = json!("seconds");
        }
        metadata["source_outcome"] = json!(label(node.get("outcome"), &mut truncated));
    }
    if stream == Stream::TextMessages {
        metadata["source_status"] = json!(label(node.get("status"), &mut truncated));
    }
    metadata["preview_truncated"] = json!(truncated);
    if serde_json::to_vec(&metadata)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > DISPLAY_BYTES
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(Interpretation { created, metadata })
}
pub fn fact_table(family: &str) -> Result<&'static str, MigrationError> {
    match family {
        "events" => Ok("fub_event_record_imported"),
        "calls" => Ok("fub_call_record_imported"),
        "text_messages" => Ok("fub_text_record_imported"),
        _ => Err(MigrationError::InvalidInput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parsed(stream: Stream, value: Value) -> Interpretation {
        interpret(stream, &serde_json::to_vec(&value).unwrap(), 7).unwrap()
    }
    #[test]
    fn dates_are_independent_exact_and_unknown_without_clock_fallback() {
        let p = parsed(
            Stream::TextMessages,
            json!({"created":"2026-01-02T03:04:05.123456+02:00","updated":"not a date","sent":"2026-01-02T00:00:00Z"}),
        );
        assert_eq!(p.metadata["source_created"], "2026-01-02T01:04:05.123456Z");
        assert!(p.metadata["source_updated"].is_null());
        assert!(p.created.is_some());
        for value in [
            "2026-01-01T00:00:00-00:00",
            "2026-01-01T00:00:00",
            "2016-12-31T23:59:60Z",
            "2026-01-01T00:00:00.1234567Z",
            "junk",
        ] {
            assert!(
                parsed(Stream::Events, json!({"created":value}))
                    .created
                    .is_none(),
                "{value}"
            );
        }
    }
    #[test]
    fn roles_and_source_claims_stay_separate_and_body_free() {
        let p = parsed(
            Stream::Calls,
            json!({"id":"00042","personId":101,"userId":10,"createdById":11,"updatedById":12,"duration":1.25,"outcome":"Vendor unknown outcome","isIncoming":true,"userName":"DO_NOT_COPY","phone":"DO_NOT_COPY","note":"DO_NOT_COPY","recordingUrl":"https://do-not-copy.test"}),
        );
        assert_eq!(p.metadata["source_id"], "42");
        assert_eq!(p.metadata["source_access_user_id"], "7");
        assert_eq!(p.metadata["source_attributed_user_id"], "10");
        assert_eq!(p.metadata["source_creator_user_id"], "11");
        assert_eq!(p.metadata["source_editor_user_id"], "12");
        assert_eq!(p.metadata["source_duration"], "125e-2");
        assert_eq!(p.metadata["source_duration_unit"], "seconds");
        assert!(!p.metadata.to_string().contains("DO_NOT_COPY"));
        assert!(!p.metadata.to_string().contains("do-not-copy"));
    }
    #[test]
    fn metadata_size_and_label_bounds_preserve_utf8_without_body_inference() {
        let p = parsed(
            Stream::Events,
            json!({"id":"9".repeat(128),"personId":"8".repeat(128),"userId":"7".repeat(128),"createdById":"6".repeat(128),"updatedById":"5".repeat(128),"noteId":"4".repeat(128),"type":"é".repeat(300),"showContent":false}),
        );
        assert!(serde_json::to_vec(&p.metadata).unwrap().len() <= DISPLAY_BYTES);
        assert_eq!(p.metadata["source_event_type"].as_str().unwrap().len(), 256);
        assert_eq!(p.metadata["preview_truncated"], true);
        assert_eq!(p.metadata["content_availability"], "not_returned");
        assert!(RECORD_RESERVATION > DISPLAY_BYTES as i64 + 4096);
    }

    #[test]
    fn conflicting_role_aliases_are_unknown_not_a_preferred_historical_actor() {
        let p = parsed(
            Stream::TextMessages,
            json!({"userId":19,"createdById":11,"createdBy":12,
            "updatedById":"00013","updatedBy":13}),
        );
        assert!(p.metadata["source_creator_user_id"].is_null());
        assert_eq!(p.metadata["source_editor_user_id"], "13");
        assert_eq!(p.metadata["source_attributed_user_id"], "19");
        assert_eq!(
            parsed(Stream::Events, json!({"createdBy":21})).metadata["source_creator_user_id"],
            "21"
        );
    }

    #[test]
    fn original_numbers_preserve_decimal_precision_and_reject_invalid_claims() {
        let p = interpret(
            Stream::Calls,
            br#"{"duration":9007199254740993.123456789}"#,
            7,
        )
        .unwrap();
        // Capture canonicalization changes notation without rounding the value.
        assert_eq!(
            p.metadata["source_duration"],
            "9007199254740993123456789e-9"
        );
        for value in [json!(-1), json!("1.25"), json!(true), Value::Null] {
            let p = parsed(Stream::Calls, json!({"duration":value,"isIncoming":"true"}));
            assert!(p.metadata["source_duration"].is_null());
            assert!(p.metadata["source_duration_unit"].is_null());
            assert!(p.metadata["source_is_incoming"].is_null());
        }
        assert!(interpret(Stream::Events, br#"{"id":1,"id":2}"#, 7).is_err());
        for bytes in [b"[]".as_slice(), b"null", br#""PRIVATE_SCALAR""#] {
            let held = interpret(Stream::Events, bytes, 7).unwrap();
            assert!(held.created.is_none());
            assert!(held.metadata["source_id"].is_null());
            assert!(held.metadata["source_person_id"].is_null());
            assert_eq!(held.metadata["content_availability"], "not_returned");
            assert!(!held.metadata.to_string().contains("PRIVATE_SCALAR"));
        }
        assert!(interpret(Stream::Events, b"{}", 0).is_err());
    }

    #[test]
    fn placeholders_and_source_markup_never_establish_body_access() {
        let p = parsed(
            Stream::TextMessages,
            json!({"message":"[redacted]","showContent":true,
            "status":"<script>untrusted()</script>","subject":"DO_NOT_COPY", "media":[{"url":"https://private.test"}]}),
        );
        assert_eq!(p.metadata["content_availability"], "returned_in_raw");
        assert!(p.metadata["source_status"].is_null());
        assert_eq!(p.metadata["preview_truncated"], true);
        assert!(!p.metadata.to_string().contains("DO_NOT_COPY"));
        assert!(!p.metadata.to_string().contains("private.test"));
    }
}

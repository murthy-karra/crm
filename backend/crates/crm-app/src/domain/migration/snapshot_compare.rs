//! Pure preview comparisons. These never choose an import policy or mutate a
//! source projection. Destination inputs are the report's frozen observation.
use chrono::{DateTime, NaiveDate};
use serde_json::{json, Value};

use crate::domain::{
    contact::normalize_email,
    custom_field::{
        normalize_and_validate_label, validate_date_range, validate_number_pattern,
        validate_text_value,
    },
    note::NoteBody,
    task::{TaskKind, TaskTitle},
};

fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn issue(issues: &mut Vec<String>, code: &str) {
    if !issues.iter().any(|existing| existing == code) {
        issues.push(code.to_owned());
    }
}

fn unique<'a>(mut values: impl Iterator<Item = &'a Value>) -> Option<&'a Value> {
    let first = values.next()?;
    values.next().is_none().then_some(first)
}

fn member_candidate<'a>(source: &Value, destination: &'a Value) -> Option<&'a Value> {
    let email = source["email"].as_str().and_then(normalize_email)?;
    unique(rows(destination, "members").iter().filter(|member| {
        member["email"].as_str().and_then(normalize_email).as_ref() == Some(&email)
    }))
}

fn stage_candidate<'a>(label: Option<&str>, destination: &'a Value) -> Option<&'a Value> {
    let label = label.map(str::trim).filter(|label| !label.is_empty())?;
    unique(
        rows(destination, "stages")
            .iter()
            .filter(|stage| stage["name"].as_str().map(str::trim) == Some(label)),
    )
}

fn target_type(source: &Value) -> Option<&'static str> {
    match source["type"].as_str()? {
        "text" => Some("text"),
        "number" => Some("number"),
        "date" => Some("date"),
        "dropdown" => Some("choice"),
        _ => None,
    }
}

fn source_field<'a>(machine_key: &str, destination: &'a Value) -> Option<&'a Value> {
    unique(rows(destination, "custom_fields").iter().filter(|field| {
        field["archived"] == false
            && field["source"] == "fub"
            && field["external_key"].as_str() == Some(machine_key)
    }))
}

fn field_candidate<'a>(source: &Value, destination: &'a Value) -> Option<&'a Value> {
    let kind = target_type(source)?;
    let key = source["name"].as_str().filter(|key| !key.is_empty())?;
    let field = source_field(key, destination)?;
    (field["field_type"].as_str() == Some(kind)).then_some(field)
}

/// Candidate IDs express only a unique comparison result. In particular they
/// do not authorize stage creation, invitations, Person merging or import.
pub fn candidates(family: &str, source: &Value, destination: &Value) -> Value {
    if rows(&source["_snapshot"], "flags").contains(&json!("projection_marker_collision")) {
        return json!({});
    }
    let candidate = match family {
        "users" => member_candidate(source, destination).map(|row| ("member_id", row)),
        "stages" => {
            stage_candidate(source["name"].as_str(), destination).map(|row| ("stage_id", row))
        }
        "people" => {
            stage_candidate(source["stage"].as_str(), destination).map(|row| ("stage_id", row))
        }
        "custom_fields" => field_candidate(source, destination).map(|row| ("custom_field_id", row)),
        _ => None,
    };
    candidate.map_or_else(|| json!({}), |(key, row)| json!({key: row["id"]}))
}

pub fn compare(family: &str, source: &Value, destination: &Value, issues: &mut Vec<String>) {
    if rows(&source["_snapshot"], "flags").contains(&json!("projection_marker_collision")) {
        issue(issues, "projection_marker_collision_decision");
        return;
    }
    if source["display_limited"] == true || source["truncated"] == true {
        issue(issues, "projection_limited");
    }
    for flag in rows(&source["_snapshot"], "flags") {
        match flag.as_str() {
            Some("projection_truncated") => issue(issues, "projection_limited"),
            Some("content_unavailable") => issue(issues, "content_gap"),
            Some("invalid_record_shape") => issue(issues, "unsupported_record_shape"),
            _ => {}
        }
    }
    if source["review_requires_decision"] == true {
        // The worker retained bounded variant examples. A wrapper is not a
        // source record with missing body/title fields, and has no winning value.
        issue(issues, "comparable_source_variants");
        return;
    }
    match family {
        "users" => {
            if member_candidate(source, destination).is_none() {
                issue(issues, "unresolved_active_member");
            }
        }
        "stages" => {
            if stage_candidate(source["name"].as_str(), destination).is_none() {
                issue(issues, "unresolved_stage");
            }
        }
        "people" => compare_person(source, destination, issues),
        "notes" => compare_note(source, issues),
        "tasks" => compare_task(source, issues),
        "custom_fields" => compare_field(source, destination, issues),
        _ => issue(issues, "unsupported_record_family"),
    }
}

fn compare_person(source: &Value, destination: &Value, issues: &mut Vec<String>) {
    if stage_candidate(source["stage"].as_str(), destination).is_none() {
        issue(issues, "unresolved_stage");
    }
    if positive_id(source.get("assignedPondId")) {
        issue(issues, "pond_assignment_decision");
    }
    if let Some(fields) = source.as_object() {
        for (key, value) in fields.iter().filter(|(key, _)| key.starts_with("custom")) {
            if value.is_null() {
                continue;
            }
            if let Some(field) = source_field(key, destination) {
                compare_value(value, field, issues);
            } else {
                issue(issues, "unresolved_custom_value_mapping");
            }
        }
    }
}

fn positive_id(value: Option<&Value>) -> bool {
    source_identifier(value).is_some()
}

/// The source parser allows positive identifiers larger than u64. Its explicit
/// exact-number wrappers must not turn such a reference into a missing author.
pub fn source_identifier(value: Option<&Value>) -> Option<String> {
    let digits = match value? {
        Value::Number(number) => number.as_u64()?.to_string(),
        Value::String(text) => text.clone(),
        Value::Object(object) => {
            if object.get("_truncated") == Some(&Value::Bool(true)) {
                return None;
            }
            let (coefficient, exponent) =
                object.get("_lossless_number")?.as_str()?.split_once('e')?;
            let exponent: usize = exponent.parse().ok()?;
            if coefficient.is_empty()
                || exponent > 128
                || coefficient.len().checked_add(exponent)? > 128
                || !coefficient.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let mut digits = coefficient.to_owned();
            digits.extend(std::iter::repeat_n('0', exponent));
            digits
        }
        _ => return None,
    };
    if digits.is_empty() || digits.len() > 128 || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let normalized = digits.trim_start_matches('0');
    (!normalized.is_empty()).then(|| normalized.to_owned())
}

fn meaningful(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        _ => true,
    }
}

fn compare_note(source: &Value, issues: &mut Vec<String>) {
    if source["showContent"] == false {
        issue(issues, "content_gap");
    } else {
        match source["body"].as_str() {
            Some(body) => match NoteBody::parse(body) {
                Ok(normalized) if normalized != body => {
                    issue(issues, "note_normalization_decision")
                }
                Ok(_) => {}
                Err(_) => issue(issues, "unsupported_note_body"),
            },
            None => issue(issues, "unsupported_note_body"),
        }
    }
    if source["isHtml"] == true
        || meaningful(&source["subject"])
        || meaningful(&source["replies"])
        || meaningful(&source["reactions"])
    {
        issue(issues, "note_representation_decision");
    }
    if !positive_id(source.get("createdById")) && !positive_id(source.get("userId")) {
        issue(issues, "unresolved_note_author");
    }
}

fn compare_task(source: &Value, issues: &mut Vec<String>) {
    // FUB names the title `name`; title is retained as additional source data,
    // never substituted for an absent qualified field.
    match source["name"].as_str() {
        Some(title) => match TaskTitle::parse(title) {
            Ok(normalized) if normalized != title => issue(issues, "task_normalization_decision"),
            Ok(_) => {}
            Err(_) => issue(issues, "unsupported_task_title"),
        },
        None => issue(issues, "unsupported_task_title"),
    }
    let kind = match source["type"].as_str() {
        Some("Call" | "call") => Some("call"),
        Some("Email" | "email") => Some("email"),
        Some("Text" | "text") => Some("text"),
        Some("Follow Up" | "follow_up") => Some("follow_up"),
        Some("Other" | "other") => Some("other"),
        _ => None,
    };
    if kind.and_then(TaskKind::from_db_str).is_none() {
        issue(issues, "unsupported_task_kind");
    }
    let timestamp = source["dueDateTime"].as_str();
    if timestamp.is_none_or(|text| DateTime::parse_from_rfc3339(text).is_err()) {
        issue(issues, "task_due_time_decision");
    }
    if let Some(date) = source.get("dueDate").filter(|value| !value.is_null()) {
        if date.as_str().and_then(parse_date).is_none() {
            issue(issues, "unsupported_task_due_date");
        }
    }
    if !positive_id(source.get("assignedUserId")) {
        issue(issues, "unresolved_task_assignee");
    }
    if !matches!(source.get("isCompleted"), Some(Value::Bool(_)))
        && !source["isCompleted"]
            .as_u64()
            .is_some_and(|value| value <= 1)
    {
        issue(issues, "unsupported_task_completion");
    }
}

fn compare_field(source: &Value, destination: &Value, issues: &mut Vec<String>) {
    if source["label"]
        .as_str()
        .is_none_or(|label| normalize_and_validate_label(label).is_err())
    {
        issue(issues, "unsupported_field_label");
    }
    let kind = target_type(source);
    if kind.is_none() {
        issue(issues, "unsupported_field_kind");
    }
    if source["isRecurring"] == true {
        issue(issues, "recurring_date_decision");
    } else if source
        .get("isRecurring")
        .is_some_and(|value| !value.is_null() && !value.is_boolean())
    {
        issue(issues, "unsupported_recurring_flag");
    }
    let target = field_candidate(source, destination);
    if target.is_none() {
        issue(issues, "unresolved_custom_field");
        if rows(destination, "custom_fields")
            .iter()
            .filter(|field| field["archived"] == false)
            .count()
            >= 50
        {
            issue(issues, "unsupported_field_capacity");
        }
        if source["name"]
            .as_str()
            .and_then(|key| source_field(key, destination))
            .is_some()
        {
            issue(issues, "unsupported_field_type_match");
        }
    }
    if kind == Some("choice") {
        let choices = rows(source, "choices");
        let mut normalized = Vec::new();
        if choices.is_empty() || choices.len() > 50 {
            issue(issues, "unsupported_field_options");
        }
        for choice in choices {
            let Some(label) = choice
                .as_str()
                .and_then(|text| normalize_and_validate_label(text).ok())
            else {
                issue(issues, "unsupported_field_options");
                continue;
            };
            let folded = label.to_lowercase();
            if normalized.contains(&folded) {
                issue(issues, "unsupported_field_options");
            }
            normalized.push(folded);
            if target.is_some_and(|field| {
                unique(rows(field, "options").iter().filter(|option| {
                    option["archived"] == false && option["label"].as_str() == Some(&label)
                }))
                .is_none()
            }) {
                issue(issues, "unsupported_missing_field_option");
            }
        }
    } else if meaningful(&source["choices"]) {
        issue(issues, "unsupported_field_options");
    }
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    (text.len() == 10).then_some(())?;
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn compare_value(value: &Value, field: &Value, issues: &mut Vec<String>) {
    match field["field_type"].as_str() {
        Some("text") => match value.as_str().map(validate_text_value) {
            Some(Ok(normalized)) if Some(normalized.as_str()) != value.as_str() => {
                issue(issues, "custom_value_normalization_decision")
            }
            Some(Ok(_)) => {}
            _ => issue(issues, "unsupported_text_value"),
        },
        Some("number") => {
            if number_text(value).is_none_or(|text| validate_number_pattern(&text).is_err()) {
                issue(issues, "unsupported_number_precision");
            }
        }
        Some("date") => {
            if value
                .as_str()
                .and_then(parse_date)
                .is_none_or(|date| validate_date_range(date).is_err())
            {
                issue(issues, "unsupported_date_range");
            }
        }
        Some("choice") => {
            let label = value.as_str();
            if label.is_none()
                || unique(rows(field, "options").iter().filter(|option| {
                    option["archived"] == false && option["label"].as_str() == label
                }))
                .is_none()
            {
                issue(issues, "unsupported_choice_value");
            }
        }
        _ => issue(issues, "unsupported_custom_value_kind"),
    }
}

/// Expand only a bounded, exact canonical number for the destination validator.
/// Never use f64 arithmetic or replace the retained source value with this text.
fn number_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Object(object) => {
            if object.get("_truncated") == Some(&Value::Bool(true)) {
                return None;
            }
            let canonical = object.get("_lossless_number")?.as_str()?;
            let (coefficient, exponent) = canonical.split_once('e')?;
            let exponent: i32 = exponent.parse().ok()?;
            let unsigned = coefficient.strip_prefix('-').unwrap_or(coefficient);
            if unsigned.is_empty()
                || unsigned.len() > 19
                || !unsigned.bytes().all(|byte| byte.is_ascii_digit())
                || !(-19..=15).contains(&exponent)
            {
                return None;
            }
            let point = i32::try_from(unsigned.len()).ok()? + exponent;
            if point > 15
                || (point <= 0 && -point + i32::try_from(unsigned.len()).ok()? > 4)
                || (point > 0 && exponent < -4)
            {
                return None;
            }
            let mut text = if coefficient.starts_with('-') {
                "-".to_owned()
            } else {
                String::new()
            };
            if point <= 0 {
                text.push_str("0.");
                text.extend(std::iter::repeat_n('0', (-point) as usize));
                text.push_str(unsigned);
            } else if point as usize >= unsigned.len() {
                text.push_str(unsigned);
                text.extend(std::iter::repeat_n('0', point as usize - unsigned.len()));
            } else {
                let (whole, fraction) = unsigned.split_at(point as usize);
                text.push_str(whole);
                text.push('.');
                text.push_str(fraction);
            }
            Some(text)
        }
        _ => None,
    }
}

pub fn disposition(issues: &[String]) -> &'static str {
    if issues.iter().any(|code| {
        code.ends_with("_decision")
            || matches!(
                code.as_str(),
                "comparable_source_variants"
                    | "normalized_contact_overlap"
                    | "content_gap"
                    | "projection_limited"
                    | "custom_values_require_review"
            )
    }) {
        "needs_decision"
    } else if issues.iter().any(|code| code.starts_with("unsupported_")) {
        "unsupported_value"
    } else if !issues.is_empty() {
        "unresolved_reference"
    } else {
        "reviewable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination() -> Value {
        json!({"stages":[{"id":"stage-lead","name":"Lead"}],"members":[{"id":"member-1","email":"agent@synthetic.test","name":"Agent"}],"custom_fields":[],"person_present":false})
    }

    fn issues(family: &str, source: &Value, target: &Value) -> Vec<String> {
        let mut issues = Vec::new();
        compare(family, source, target, &mut issues);
        issues
    }

    #[test]
    fn matches_require_unique_actual_keys_not_names() {
        let mut target = destination();
        let user = json!({"name":"Someone else","email":" AGENT@synthetic.test "});
        assert_eq!(candidates("users", &user, &target)["member_id"], "member-1");
        assert_eq!(
            candidates("stages", &json!({"name":" Lead "}), &target)["stage_id"],
            "stage-lead"
        );
        assert!(issues("users", &json!({"name":"Agent"}), &target)
            .contains(&"unresolved_active_member".into()));
        assert!(
            issues("stages", &json!({"name":"lead"}), &target).contains(&"unresolved_stage".into())
        );
        target["members"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"member-2","email":"agent@synthetic.test"}));
        assert_eq!(candidates("users", &user, &target), json!({}));
        assert_eq!(
            disposition(&issues("users", &user, &target)),
            "unresolved_reference"
        );
    }

    #[test]
    fn note_html_enrichment_author_and_normalization_are_separate_issues() {
        let target = destination();
        let plain =
            json!({"body":"2 < 3","isHtml":false,"createdById":7,"replies":[],"reactions":{}});
        assert!(issues("notes", &plain, &target).is_empty());
        let html = json!({"body":"hello","isHtml":true,"createdBy":"Agent","subject":"Subject","replies":[{"body":"Reply"}],"reactions":{"👍":[7]}});
        let found = issues("notes", &html, &target);
        assert!(found.contains(&"note_representation_decision".into()));
        assert!(found.contains(&"unresolved_note_author".into()));
        assert_eq!(disposition(&found), "needs_decision");
        assert!(issues(
            "notes",
            &json!({"body":"line\r\nline ","createdById":7}),
            &target
        )
        .contains(&"note_normalization_decision".into()));
        assert!(!issues("notes", &json!({"showContent":false}), &target)
            .contains(&"unsupported_note_body".into()));
    }

    #[test]
    fn task_timestamp_and_companion_date_do_not_create_false_ambiguity() {
        let target = destination();
        let mut task = json!({"name":"Call client","type":"Call","dueDate":"2026-09-11","dueDateTime":"2026-09-11T10:00:00-07:00","assignedUserId":7,"isCompleted":0});
        assert!(issues("tasks", &task, &target).is_empty());
        task["dueDateTime"] = Value::Null;
        assert!(issues("tasks", &task, &target).contains(&"task_due_time_decision".into()));
        task["dueDateTime"] = json!("2026-09-11T10:00:00");
        assert!(issues("tasks", &task, &target).contains(&"task_due_time_decision".into()));
        task["assignedUserId"] = json!(0);
        task["AssignedTo"] = json!("Agent");
        task["type"] = json!("Future kind");
        let found = issues("tasks", &task, &target);
        assert!(found.contains(&"unsupported_task_kind".into()));
        assert!(found.contains(&"unresolved_task_assignee".into()));
    }

    #[test]
    fn custom_field_options_types_and_recurring_dates_remain_explicit() {
        let mut target = destination();
        target["custom_fields"] = json!([{"id":"field-1","source":"fub","external_key":"customArea","field_type":"choice","archived":false,"options":[{"id":"option-1","label":"North","archived":false}]}]);
        let mut field =
            json!({"name":"customArea","label":"Area","type":"dropdown","choices":["North"]});
        assert!(issues("custom_fields", &field, &target).is_empty());
        assert_eq!(
            candidates("custom_fields", &field, &target)["custom_field_id"],
            "field-1"
        );
        field["choices"] = json!(["North", "South"]);
        assert!(issues("custom_fields", &field, &target)
            .contains(&"unsupported_missing_field_option".into()));
        field["choices"] = json!(["North", "north"]);
        assert!(
            issues("custom_fields", &field, &target).contains(&"unsupported_field_options".into())
        );
        field["type"] = json!("date");
        field["isRecurring"] = json!(true);
        let found = issues("custom_fields", &field, &target);
        assert!(found.contains(&"unsupported_field_type_match".into()));
        assert_eq!(disposition(&found), "needs_decision");
    }

    #[test]
    fn person_custom_values_use_real_validators_and_exact_number_wrappers() {
        let mut target = destination();
        target["custom_fields"] = json!([
            {"id":"number","source":"fub","external_key":"customAmount","field_type":"number","archived":false,"options":[]},
            {"id":"text","source":"fub","external_key":"customText","field_type":"text","archived":false,"options":[]},
            {"id":"date","source":"fub","external_key":"customDate","field_type":"date","archived":false,"options":[]}
        ]);
        let person = json!({"stage":"Lead","customAmount":{"_lossless_number":"1234567890123451234e-4"},"customText":"ok","customDate":"2026-09-11"});
        assert!(issues("people", &person, &target).is_empty());
        let invalid = json!({"stage":"Lead","customAmount":{"_lossless_number":"12345678901234512345e-5"},"customText":"line\nbreak","customDate":"2201-01-01","customUnknown":true});
        let found = issues("people", &invalid, &target);
        for code in [
            "unsupported_number_precision",
            "unsupported_text_value",
            "unsupported_date_range",
            "unresolved_custom_value_mapping",
        ] {
            assert!(found.contains(&code.into()));
        }
        assert_eq!(disposition(&found), "unsupported_value");
        assert_eq!(number_text(&json!({"_lossless_number":"1e100000"})), None);
        assert_eq!(
            number_text(&json!({"_lossless_number":"1e-4"})).as_deref(),
            Some("0.0001")
        );
    }

    #[test]
    fn projection_and_variant_markers_do_not_fabricate_missing_source_fields() {
        let target = destination();
        let source = json!({"variants":[{"body":"one"},{"body":"two"}],"display_limited":true,"review_requires_decision":true});
        let found = issues("notes", &source, &target);
        assert!(!found.contains(&"unsupported_note_body".into()));
        assert_eq!(disposition(&found), "needs_decision");
        let clipped = json!({"body":"clipped","createdById":7,"_snapshot":{"flags":["projection_truncated"]}});
        assert_eq!(
            disposition(&issues("notes", &clipped, &target)),
            "needs_decision"
        );
        let priorities = vec![
            "unsupported_text_value".into(),
            "unresolved_source_user".into(),
            "normalized_contact_overlap".into(),
        ];
        assert_eq!(disposition(&priorities), "needs_decision");
    }

    #[test]
    fn source_references_keep_large_integer_identity_and_reject_fractional_or_zero_ids() {
        assert_eq!(
            source_identifier(Some(
                &json!({"_lossless_number":"184467440737095516160001e0"})
            ))
            .as_deref(),
            Some("184467440737095516160001")
        );
        assert_eq!(
            source_identifier(Some(&json!("00042"))).as_deref(),
            Some("42")
        );
        for value in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!(""),
            json!({"_lossless_number":"1e100000"}),
            json!({"_lossless_number":"1e-1"}),
        ] {
            assert_eq!(source_identifier(Some(&value)), None);
        }
        let source = json!({"stage":"Lead","customAmount":{"_lossless_number":"1e0"},"_snapshot":{"flags":["projection_marker_collision"]}});
        assert_eq!(candidates("people", &source, &destination()), json!({}));
        assert_eq!(
            disposition(&issues("people", &source, &destination())),
            "needs_decision"
        );
    }
}

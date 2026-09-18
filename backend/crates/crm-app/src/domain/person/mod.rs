pub mod discovery;
pub mod filter;
#[cfg(feature = "test-support")]
pub mod filter_test_support;
mod history_snapshot;
pub mod model;
pub mod projection;
pub mod queries;
pub mod sort;
pub mod visibility;

pub use visibility::PersonVisibilityScope;

/// PostgreSQL's JSON encoder writes UTC timestamptz values with `+00:00`;
/// chrono's established HTTP representation uses `Z`. Normalize only known
/// timestamp fields so the purpose-built JSON read model preserves the wire
/// contract without touching user-authored strings.
pub(super) fn normalize_json_timestamps(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                normalize_json_timestamps(item);
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, field) in fields {
                if matches!(
                    key.as_str(),
                    "occurred_at"
                        | "recorded_at"
                        | "captured_at"
                        | "answered_at"
                        | "updated_at"
                        | "created_at"
                        | "received_at"
                        | "last_inquiry_at"
                        | "due_at"
                        | "completed_at"
                ) {
                    if let Some(timestamp) = field.as_str() {
                        if let Some(prefix) = timestamp.strip_suffix("+00:00") {
                            *field = serde_json::Value::String(format!("{prefix}Z"));
                            continue;
                        }
                    }
                }
                normalize_json_timestamps(field);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod timestamp_tests {
    #[test]
    fn normalizes_only_timestamp_fields() {
        let mut value = serde_json::json!({
            "created_at": "2026-09-17T12:34:56.123456+00:00",
            "history": [{"detail": {"captured_at": "2026-09-17T12:34:56+00:00"}}],
            "message": "meet at 12:34+00:00",
            "value": {"text": "2026-09-17T12:34:56+00:00"}
        });
        super::normalize_json_timestamps(&mut value);
        assert_eq!(value["created_at"], "2026-09-17T12:34:56.123456Z");
        assert_eq!(
            value["history"][0]["detail"]["captured_at"],
            "2026-09-17T12:34:56Z"
        );
        assert_eq!(value["message"], "meet at 12:34+00:00");
        assert_eq!(value["value"]["text"], "2026-09-17T12:34:56+00:00");
    }
}

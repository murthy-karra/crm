//! Emits exact production history reader statements for isolated D-050 EXPLAIN.
//! No database or network access; only the test-support statement adapters.
fn main() {
    use crm_api::domain::migration::history_review::{self as h, Dated};
    let mut rows = Vec::new();
    for kind in [
        "fub_event_record_imported",
        "fub_call_record_imported",
        "fub_text_record_imported",
    ] {
        for (label, dated) in [("known", Dated::Known), ("unknown", Dated::Unknown)] {
            rows.push(serde_json::json!({"name":format!("{kind}-{label}"),"kind":kind,"sql":h::candidate_sql_for_test(kind,dated).unwrap(),"shape":"page"}));
            rows.push(serde_json::json!({"name":format!("{kind}-{label}-after"),"kind":kind,"sql":h::candidate_after_sql_for_test(kind,dated,kind).unwrap(),"shape":"after"}));
        }
        rows.push(serde_json::json!({"name":format!("{kind}-entry"),"kind":kind,"sql":h::entry_sql_for_test(kind).unwrap(),"shape":"entry"}));
        for shape in ["first", "page", "detail"] {
            rows.push(serde_json::json!({"name":format!("{kind}-version-{shape}"),"kind":kind,"sql":h::version_sql_for_test(kind,shape).unwrap(),"shape":format!("version-{shape}")}));
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}

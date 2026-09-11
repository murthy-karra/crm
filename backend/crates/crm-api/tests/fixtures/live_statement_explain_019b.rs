//! Candidate for `crm-api/tests/fixtures/live_statement_explain_019b.rs`.
//!
//! This file is intentionally prepared outside the checkout.  Copy it and
//! `current_count_filtered_matches.sql` together into `tests/fixtures/`.
//! The 14 statement inputs are the current live SQL, not the 6bad52a frozen
//! fixture.  The count statement is extracted from the current inline raw
//! literal because it is intentionally not a SQL file.

use chrono::{DateTime, NaiveDate, Utc};
use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

const EXPLAIN_PREFIX: &str = "EXPLAIN (ANALYZE, BUFFERS) ";

fn sha256_hex(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// Relative to crm-api/tests/fixtures after integration.
const SUMMARY_CREATED_DESC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries.sql");
const SUMMARY_CREATED_ASC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_created_asc.sql");
const SUMMARY_NAME_ASC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_name_asc.sql");
const SUMMARY_NAME_DESC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_name_desc.sql");
const SUMMARY_STAGE_ASC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_stage_asc.sql");
const SUMMARY_STAGE_DESC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_stage_desc.sql");
const SUMMARY_ASSIGNEE_ASC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_assignee_asc.sql");
const SUMMARY_ASSIGNEE_DESC: &str =
    include_str!("../../../crm-app/src/domain/person/sql/filtered_summaries_assignee_desc.sql");
const COUNT: &str = include_str!("current_count_filtered_matches.sql");
const SOURCE_MEMBERSHIP: &str =
    include_str!("../../../crm-app/src/domain/today/source_membership.sql");
const SOURCE_CANDIDATES: &str =
    include_str!("../../../crm-app/src/domain/today/source_candidates.sql");
const PERSON_STATE: &str =
    include_str!("../../../crm-app/src/domain/today/system_feeds/sql/person_state.sql");
const CALL_MEMBERSHIP: &str =
    include_str!("../../../crm-app/src/domain/today/system_feeds/sql/call_membership.sql");
const CALL_ONLY: &str =
    include_str!("../../../crm-app/src/domain/today/system_feeds/sql/call_only.sql");

#[derive(Clone, Copy, Debug)]
pub struct ExplainFieldIds {
    /// Slot order: text, text, number, date, choice.
    pub fields: [Uuid; 5],
    pub choice_option: Uuid,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExplainArtifact {
    pub statement: &'static str,
    pub bind_count: usize,
    pub sql_sha256: String,
    /// UUID literals are redacted before artifacts are retained.
    pub plan: String,
    /// Diagnostic only: never a pass/fail performance gate.
    pub repeated_value_table_scan: bool,
}

#[derive(Clone)]
struct Slot {
    field_id: Uuid,
    field_type: &'static str,
    operation: &'static str,
    text: Option<&'static str>,
    number_min: Option<&'static str>,
    number_max: Option<&'static str>,
    date_min: Option<NaiveDate>,
    date_max: Option<NaiveDate>,
    option_ids: Option<Vec<Uuid>>,
}

#[derive(Clone)]
struct Matrix {
    slots: [Slot; 5],
}

impl Matrix {
    fn meaningful(fields: ExplainFieldIds) -> Self {
        // These are all populated, valid clauses. The fixture must assign
        // matching values to the sampled People before plan capture.
        Self {
            slots: [
                Slot {
                    field_id: fields.fields[0],
                    field_type: "text",
                    operation: "contains",
                    text: Some("selective-value"),
                    number_min: None,
                    number_max: None,
                    date_min: None,
                    date_max: None,
                    option_ids: None,
                },
                Slot {
                    field_id: fields.fields[1],
                    field_type: "text",
                    operation: "not_contains",
                    text: Some("excluded-value"),
                    number_min: None,
                    number_max: None,
                    date_min: None,
                    date_max: None,
                    option_ids: None,
                },
                Slot {
                    field_id: fields.fields[2],
                    field_type: "number",
                    operation: "range",
                    text: None,
                    number_min: Some("400000"),
                    number_max: Some("600000"),
                    date_min: None,
                    date_max: None,
                    option_ids: None,
                },
                Slot {
                    field_id: fields.fields[3],
                    field_type: "date",
                    operation: "range",
                    text: None,
                    number_min: None,
                    number_max: None,
                    date_min: Some(NaiveDate::from_ymd_opt(2019, 1, 1).unwrap()),
                    date_max: Some(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap()),
                    option_ids: None,
                },
                Slot {
                    field_id: fields.fields[4],
                    field_type: "choice",
                    operation: "any_of",
                    text: None,
                    number_min: None,
                    number_max: None,
                    date_min: None,
                    date_max: None,
                    option_ids: Some(vec![fields.choice_option]),
                },
            ],
        }
    }
}

macro_rules! bind_slots {
    ($query:expr, $matrix:expr) => {{
        let mut query = $query;
        for slot in &$matrix.slots {
            query = query
                .bind(Some(slot.field_id))
                .bind(Some(slot.field_type))
                .bind(Some(slot.operation))
                .bind(slot.text)
                .bind(slot.number_min)
                .bind(slot.number_max)
                .bind(slot.date_min)
                .bind(slot.date_max)
                .bind(slot.option_ids.as_deref());
        }
        query
    }};
}

macro_rules! bind_filter_axis {
    ($query:expr, $viewer:expr) => {
        $query
            .bind(None::<Vec<Uuid>>)
            .bind(Some(vec![$viewer]))
            .bind(None::<bool>)
            .bind(None::<Vec<String>>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
    };
}

macro_rules! bind_base {
    ($query:expr, $org:expr, $viewer:expr) => {
        $query
            .bind($org)
            .bind(None::<Vec<Uuid>>)
            .bind(Some(vec![$viewer]))
            .bind(None::<bool>)
            .bind(None::<Vec<String>>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<i32>)
            .bind(None::<i32>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
    };
}

async fn finish(
    statement: &'static str,
    bind_count: usize,
    sql: &str,
    rows: Vec<(String,)>,
) -> ExplainArtifact {
    let plan = rows
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n");
    ExplainArtifact {
        statement,
        bind_count,
        sql_sha256: sha256_hex(sql.as_bytes()),
        repeated_value_table_scan: repeated_value_table_scan(&plan),
        plan: sanitize_plan(&plan),
    }
}

macro_rules! run_explain {
    ($pool:expr, $out:expr, $statement:expr, $bind_count:expr, $sql:expr, |$query:ident| $binds:expr) => {{
        let explain = format!("{EXPLAIN_PREFIX}{}", $sql);
        let $query = sqlx::query_as::<sqlx::Postgres, (String,)>(&explain);
        let rows: Vec<(String,)> = ($binds).fetch_all($pool).await?;
        $out.push(finish($statement, $bind_count, $sql, rows).await);
    }};
}

/// Captures current live matrices plus the four bounded custom-field reads.
/// No planner settings, cardinality thresholds, or absolute cost gates are used.
pub async fn capture_live_statement_plans(
    pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    now: DateTime<Utc>,
    fields: ExplainFieldIds,
) -> Result<Vec<ExplainArtifact>, sqlx::Error> {
    let m = Matrix::meaningful(fields);
    let mut out = Vec::with_capacity(18);

    for (name, sql) in [
        ("filtered_summaries_created_desc", SUMMARY_CREATED_DESC),
        ("filtered_summaries_created_asc", SUMMARY_CREATED_ASC),
        ("filtered_summaries_name_asc", SUMMARY_NAME_ASC),
        ("filtered_summaries_name_desc", SUMMARY_NAME_DESC),
        ("filtered_summaries_stage_asc", SUMMARY_STAGE_ASC),
        ("filtered_summaries_stage_desc", SUMMARY_STAGE_DESC),
        ("filtered_summaries_assignee_asc", SUMMARY_ASSIGNEE_ASC),
        ("filtered_summaries_assignee_desc", SUMMARY_ASSIGNEE_DESC),
    ] {
        run_explain!(pool, out, name, 72, sql, |q| {
            let q = bind_base!(q, organization_id, viewer_id)
                .bind(None::<DateTime<Utc>>)
                .bind(None::<bool>)
                .bind(None::<bool>)
                .bind(None::<bool>)
                .bind(viewer_id)
                .bind(None::<Vec<Uuid>>)
                .bind(None::<Vec<Uuid>>);
            bind_slots!(q, m)
        });
    }

    run_explain!(pool, out, "count_filtered_matches", 71, COUNT, |q| {
        let q = bind_base!(q, organization_id, viewer_id)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(viewer_id)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        bind_slots!(q, m)
    });

    let fixture_people: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM person WHERE organization_id = $1 ORDER BY id LIMIT 100",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await?;
    let builtin_ids = fixture_people.clone();
    run_explain!(pool, out, "source_membership", 73, SOURCE_MEMBERSHIP, |q| {
        let q = bind_base!(q, organization_id, viewer_id)
            .bind(now)
            .bind(&builtin_ids)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(viewer_id)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        bind_slots!(q, m)
    });
    run_explain!(pool, out, "source_candidates", 75, SOURCE_CANDIDATES, |q| {
        let q = bind_base!(q, organization_id, viewer_id)
            .bind(now)
            .bind(&builtin_ids)
            .bind(false)
            .bind(201_i64)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(viewer_id)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        bind_slots!(q, m)
    });

    // Both feed arms receive a non-empty five-slot matrix. Their anchor
    // predicates remain distinct, matching normal Today evaluation.
    run_explain!(pool, out, "person_state", 145, PERSON_STATE, |q| {
        let q = bind_filter_axis!(q.bind(organization_id), viewer_id)
            .bind(Some(true))
            .bind(None::<bool>)
            .bind(None::<bool>);
        let q = bind_filter_axis!(q, viewer_id)
            .bind(None::<bool>)
            .bind(Some(true))
            .bind(None::<bool>);
        let q = q
            .bind(now)
            .bind(viewer_id)
            .bind(true)
            .bind(true)
            .bind(24_i32)
            .bind(24_i32)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        let q = bind_slots!(q, m);
        bind_slots!(q, m)
    });

    let retained_ids = fixture_people.into_iter().take(90).collect::<Vec<_>>();
    run_explain!(pool, out, "call_membership", 73, CALL_MEMBERSHIP, |q| {
        let q = bind_base!(q, organization_id, viewer_id)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(Some(true))
            .bind(viewer_id)
            .bind(&retained_ids)
            .bind(now)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        bind_slots!(q, m)
    });
    run_explain!(pool, out, "call_only", 74, CALL_ONLY, |q| {
        let q = bind_base!(q, organization_id, viewer_id)
            .bind(None::<bool>)
            .bind(None::<bool>)
            .bind(Some(true))
            .bind(viewer_id)
            .bind(&retained_ids)
            .bind(201_i64)
            .bind(now)
            .bind(None::<Vec<Uuid>>)
            .bind(None::<Vec<Uuid>>);
        bind_slots!(q, m)
    });

    // Exact bounded reference/label queries from custom_field/queries.rs
    // lines 401-492. These are captured separately from the 14 matrices.
    run_explain!(pool, out, "custom_field_live_type", 2,
        "SELECT field_type FROM custom_field WHERE id = $1 AND organization_id = $2 AND archived_at IS NULL",
        |q| q.bind(fields.fields[0]).bind(organization_id));
    run_explain!(pool, out, "custom_field_option_belongs", 3,
        "SELECT count(*) as count FROM custom_field_option WHERE organization_id = $1 AND field_id = $2 AND id = ANY($3::uuid[])",
        |q| q.bind(organization_id).bind(fields.fields[4]).bind(vec![fields.choice_option]));
    run_explain!(
        pool,
        out,
        "custom_field_labels",
        2,
        "SELECT id, label FROM custom_field WHERE organization_id = $1 AND id = ANY($2::uuid[])",
        |q| q.bind(organization_id).bind(fields.fields.to_vec())
    );
    run_explain!(pool, out, "custom_field_option_labels", 2,
        "SELECT field_id, id, label FROM custom_field_option WHERE organization_id = $1 AND field_id = ANY($2::uuid[])",
        |q| q.bind(organization_id).bind(fields.fields.to_vec()));

    Ok(out)
}

fn repeated_value_table_scan(plan: &str) -> bool {
    let values = plan.matches("person_custom_field_value").count();
    values > 5
        && (plan.contains("Seq Scan on person_custom_field_value")
            || plan.contains("Bitmap Heap Scan on person_custom_field_value"))
}

fn sanitize_plan(plan: &str) -> String {
    // EXPLAIN commonly interpolates UUID bind values. Preserve useful plan
    // shape while retaining no fixture/resource identifiers in artifacts.
    let bytes = plan.as_bytes();
    let mut result = String::with_capacity(plan.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len()
            && [8, 13, 18, 23]
                .into_iter()
                .all(|offset| bytes[i + offset] == b'-')
            && bytes[i..i + 36].iter().enumerate().all(|(offset, byte)| {
                matches!(offset, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit()
            })
        {
            result.push_str("<uuid>");
            i += 36;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

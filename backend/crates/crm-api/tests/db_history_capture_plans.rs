//! Opt-in D-070/A8 constructed-source scale collector.
//!
//! The real history worker creates all 75,000 valid-AAD observations and their
//! raw captures. No copied ciphertext, migration-row bulk seed, disabled trigger,
//! workspace transition, role override, planner override or timing gate is used.
//! Only unrelated native People/member cardinality is bulk seeded through the
//! disposable migrator role, following prior plan-fixture precedent. That
//! connection is released before application commands/readers run as crm_app;
//! the existing review binding and write guard are checked before/after.
//!
//! Production SQL comes from the shared records accessor or uniquely identified
//! exact literals. EXPLAIN ANALYZE BUFFERS includes typed bindings, source hashes,
//! scan work and page limits. This measures the synthetic contract, not vendor
//! completeness, operational capacity or historical interpretation.
#![cfg(feature = "perf-harness")]

use std::collections::{BTreeMap, BTreeSet};

use crate::{db_history_capture_support as support, import_support::Fixture};
use chrono::{DateTime, Utc};
use crm_api::domain::migration::{
    history_capture as history, history_capture_source::Stream, reader::Capture,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

const PEOPLE: i64 = 25_000;
const PER_FAMILY: usize = 25_000;
const DENSE: usize = 501;
const BODY_SENTINEL: &str = "SYNTHETIC_HISTORY_SCALE_BODY_SENTINEL";
const QUERIES: &str = include_str!("../../crm-app/src/domain/migration/history_capture_queries.rs");
const STORE: &str = include_str!("../../crm-app/src/domain/migration/history_capture_store.rs");
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/history_capture_worker.rs");
const SOURCE: &str = include_str!("../../crm-app/src/domain/migration/history_capture_source.rs");
const COMMANDS: &str = include_str!("../../crm-app/src/domain/migration/history_capture.rs");
const SCHEMA: &str = include_str!("../migrations/20260921000001_fub_history_capture.sql");
const ACCOUNTING_TEST: &str = include_str!("db_history_capture.rs");

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let mut matches = source.match_indices(&marker).map(|(offset, _)| {
        serde_json::Deserializer::from_str(&source[offset..])
            .into_iter::<String>()
            .next()
            .expect("SQL literal")
            .expect("decode exact source SQL literal")
    });
    let sql = matches
        .next()
        .unwrap_or_else(|| panic!("missing SQL prefix: {prefix}"));
    assert!(
        matches.all(|other| other == sql),
        "ambiguous SQL prefix: {prefix}"
    );
    sql
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum Arg {
    Uuid(Uuid),
    Text(String),
    I64(i64),
    I32(i32),
    Bytes(Vec<u8>),
    OptionalText(Option<String>),
    OptionalUuid(Option<Uuid>),
    OptionalBytes(Option<Vec<u8>>),
    Time(DateTime<Utc>),
}

#[derive(Default)]
struct Report {
    plans: Vec<Value>,
    failures: Vec<String>,
    readers: Vec<Value>,
}

fn plan_nodes<'a>(value: &'a Value, nodes: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("Node Type") {
                nodes.push(value);
            }
            for child in fields.values() {
                plan_nodes(child, nodes);
            }
        }
        Value::Array(values) => {
            for child in values {
                plan_nodes(child, nodes);
            }
        }
        _ => {}
    }
}

async fn explain(
    report: &mut Report,
    conn: &mut PgConnection,
    name: &str,
    sql: String,
    args: Vec<Arg>,
    maximum_rows: u64,
) {
    let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    let mut query = sqlx::query_scalar::<_, Value>(&wrapped);
    for arg in &args {
        query = match arg {
            Arg::Uuid(v) => query.bind(*v),
            Arg::Text(v) => query.bind(v),
            Arg::I64(v) => query.bind(*v),
            Arg::I32(v) => query.bind(*v),
            Arg::Bytes(v) => query.bind(v),
            Arg::OptionalText(v) => query.bind(v),
            Arg::OptionalUuid(v) => query.bind(*v),
            Arg::OptionalBytes(v) => query.bind(v),
            Arg::Time(v) => query.bind(*v),
        };
    }
    let plan = query
        .fetch_one(conn)
        .await
        .unwrap_or_else(|error| panic!("EXPLAIN {name}: {error}"));
    let mut nodes = Vec::new();
    plan_nodes(&plan, &mut nodes);
    let actual = plan[0]["Plan"]["Actual Rows"]
        .as_f64()
        .expect("actual top-level rows");
    if actual > maximum_rows as f64 {
        report.failures.push(format!("{name}: returned row bound"));
    }
    for node in &nodes {
        if node["Sort Space Type"] == "Disk"
            || node["Temp Read Blocks"].as_u64().unwrap_or(0) > 0
            || node["Temp Written Blocks"].as_u64().unwrap_or(0) > 0
            || node["Hash Batches"].as_u64().unwrap_or(1) > 1
        {
            report
                .failures
                .push(format!("{name}: temporary spill or batched hash"));
        }
        if node["Relation Name"] == "migration_history_observation"
            && node["Node Type"] == "Seq Scan"
        {
            report.failures.push(format!(
                "{name}: sequential scan of the 75k observation relation"
            ));
        }
    }
    let scans = nodes.iter().filter(|node| node.get("Relation Name").is_some()).map(|node| json!({
        "relation":node["Relation Name"],"node":node["Node Type"],"index":node["Index Name"],
        "actual_rows_per_loop":node["Actual Rows"],"loops":node["Actual Loops"],
        "removed_per_loop":node["Rows Removed by Filter"],"index_condition":node["Index Cond"],
        "filter":node["Filter"],"shared_hit_blocks":node["Shared Hit Blocks"],
        "shared_read_blocks":node["Shared Read Blocks"],
    })).collect::<Vec<_>>();
    report.plans.push(
        json!({"name":name,"sql_sha256":sha(sql.as_bytes()),"sql":sql,
        "bindings":args,"maximum_returned_rows":maximum_rows,"scans":scans,"plan":plan}),
    );
}

async fn seed_envelope(migrator: &PgPool, f: &Fixture) -> Value {
    let before: Value = sqlx::query_scalar("SELECT jsonb_build_object('workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE organization_id=$1),'mode',(SELECT workspace_mode FROM organization WHERE id=$1),'revision',(SELECT workspace_revision FROM organization WHERE id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(before["mode"], "migration_review");
    let mut tx = migrator.begin().await.unwrap();
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(role, "crm_migrator");
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_membership WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(members, 2);
    assert_eq!(people, 2);
    sqlx::query("CREATE TEMP TABLE history_plan_members ON COMMIT DROP AS SELECT n,gen_random_uuid() AS id FROM generate_series(3,50)n")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'history-plan-'||n||'@synthetic.test','Synthetic scale member '||n FROM history_plan_members")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM history_plan_members")
        .bind(f.org).execute(&mut *tx).await.unwrap();
    let member_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM organization_membership WHERE organization_id=$1 ORDER BY user_id",
    )
    .bind(f.org)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO person(organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT $1,'Synthetic','History scale '||n,$2,($3::uuid[])[1+((n-1)%50)] FROM generate_series(3,$4::bigint)n")
        .bind(f.org).bind(f.lead_stage).bind(&member_ids).bind(PEOPLE).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    // No connection-wide SET ROLE/replication/workspace setting was introduced.
    // An actual app-role mutation must still fail behind the existing hold.
    let mut app = f.pool.begin().await.unwrap();
    let app_role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *app)
        .await
        .unwrap();
    assert_eq!(app_role, "crm_app");
    let denied = sqlx::query("INSERT INTO person(organization_id,first_name,stage_id) VALUES($1,'Must remain forbidden',$2)")
        .bind(f.org).bind(f.lead_stage).execute(&mut *app).await;
    let denied = denied.expect_err("native write guard must remain active");
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C")
    );
    app.rollback().await.unwrap();
    let after: Value = sqlx::query_scalar("SELECT jsonb_build_object('workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE organization_id=$1),'mode',(SELECT workspace_mode FROM organization WHERE id=$1),'revision',(SELECT workspace_revision FROM organization WHERE id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(before, after);
    let counts: Value = sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts["people"], PEOPLE);
    assert_eq!(counts["members"], 50);
    json!({"counts":counts,"application_role":app_role,"native_write_guard_denied":true,
        "workspace_unchanged":true,"seed_scope":"migrator native cardinality only; two genuinely imported People"})
}

async fn invariant(f: &Fixture) -> Value {
    let mut tables = BTreeMap::new();
    for table in [
        "person",
        "contact_method",
        "inquiry",
        "inquiry_received",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call",
        "call_completed",
        "note",
        "task",
        "person_imported",
        "correspondence_raw",
        "correspondence_captured",
        "migration_workspace",
        "migration_import",
        "migration_import_plan",
        "migration_import_identity",
        "migration_import_result",
        "migration_snapshot",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_metadata_import",
        "migration_metadata_result",
        "migration_activity_import",
        "migration_activity_result",
    ] {
        let sql = format!("SELECT jsonb_build_object('rows',count(*),'rows_hash',md5(coalesce(string_agg(md5(to_jsonb(t)::text),'' ORDER BY md5(to_jsonb(t)::text)),''))) FROM {table} t WHERE organization_id=$1");
        let value: Value = sqlx::query_scalar(&sql)
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        tables.insert(table, value);
    }
    json!(tables)
}

fn source_records(stream: Stream) -> Vec<Value> {
    (0..PER_FAMILY).map(|index| {
        let mut row = json!({"id":index + 1,"personId":if index < DENSE {101} else {1_000_000 + index},
            "userId":3,"createdById":3,"created":"2026-01-01T00:00:00Z","updated":"2026-01-02T00:00:00Z",
            "message":BODY_SENTINEL,"unknownRetained":{"ordinal":index,"payload":"constructed source evidence"}});
        if index == DENSE { row["personId"] = json!(102); }
        if index == DENSE + 1 { row["personId"] = json!(103); }
        if index == DENSE + 2 { row["personId"] = json!(0); }
        if index == DENSE + 3 { row.as_object_mut().unwrap().remove("personId"); }
        match stream {
            Stream::Events => row["type"] = json!("Registration"),
            Stream::Calls => { row["outcome"] = json!("Source-only outcome"); row["recordingUrl"] = json!("https://synthetic.invalid/not-fetched"); }
            Stream::TextMessages => {
                row["status"] = json!("Source-only status");
                if index == 0 { row["participants"] = json!([{"type":"person","personId":101},{"type":"person","personId":102}]); }
            }
        }
        row
    }).collect()
}

struct Traversal {
    ids: BTreeSet<Uuid>,
    family_counts: BTreeMap<String, usize>,
    source_ids: BTreeMap<String, BTreeSet<usize>>,
    link_counts: BTreeMap<String, usize>,
    dense: usize,
    pages: usize,
    max_page_bytes: usize,
    max_record_bytes: usize,
    max_variants: usize,
    max_observations: usize,
    sequence: Option<String>,
    last: Option<(u64, u64, Uuid)>,
}
impl Traversal {
    fn new() -> Self {
        Self {
            ids: BTreeSet::new(),
            family_counts: BTreeMap::new(),
            source_ids: BTreeMap::new(),
            link_counts: BTreeMap::new(),
            dense: 0,
            pages: 0,
            max_page_bytes: 0,
            max_record_bytes: 0,
            max_variants: 0,
            max_observations: 0,
            sequence: None,
            last: None,
        }
    }
    fn accept(&mut self, value: &Value) {
        self.pages += 1;
        let bytes = serde_json::to_vec(value).unwrap();
        assert!(bytes.len() <= 512 * 1024);
        assert!(!String::from_utf8_lossy(&bytes).contains(BODY_SENTINEL));
        self.max_page_bytes = self.max_page_bytes.max(bytes.len());
        let sequence = value["capture_sequence"].as_str().unwrap();
        if let Some(old) = &self.sequence {
            assert_eq!(old, sequence);
        } else {
            self.sequence = Some(sequence.into());
        }
        assert_eq!(value["counter_basis"], "current_run");
        let rows = value["records"].as_array().unwrap();
        assert!(rows.len() <= 50);
        for row in rows {
            let length = serde_json::to_vec(row).unwrap().len();
            assert!(length <= 8192);
            self.max_record_bytes = self.max_record_bytes.max(length);
            for field in [
                "body",
                "message",
                "subject",
                "note",
                "phone",
                "recordingUrl",
                "media",
                "participants",
            ] {
                assert!(
                    row.get(field).is_none(),
                    "content field escaped projection: {field}"
                );
            }
            assert_eq!(row["integrity_status"], "verified_projection");
            let source_id = row["source_id"].as_str().unwrap().parse::<usize>().unwrap();
            assert!((1..=PER_FAMILY).contains(&source_id));
            self.source_ids
                .entry(row["family"].as_str().unwrap().into())
                .or_default()
                .insert(source_id);
            let variants = row["variant_count"]
                .as_str()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let observations = row["observation_count"]
                .as_str()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert!(variants > 0 && variants <= observations);
            self.max_variants = self.max_variants.max(variants);
            self.max_observations = self.max_observations.max(observations);
            let id = Uuid::parse_str(row["id"].as_str().unwrap()).unwrap();
            let position = (
                row["capture_sequence"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap(),
                row["ordinal"].as_str().unwrap().parse::<u64>().unwrap(),
                id,
            );
            if let Some(last) = self.last {
                assert!(position > last);
            }
            self.last = Some(position);
            assert!(
                self.ids.insert(id),
                "duplicate local observation in cursor traversal"
            );
            *self
                .family_counts
                .entry(row["family"].as_str().unwrap().into())
                .or_default() += 1;
            *self
                .link_counts
                .entry(row["disposition"].as_str().unwrap().into())
                .or_default() += 1;
            if row["source_person_id"] == "101" {
                self.dense += 1;
            }
        }
    }
    fn evidence(&self, label: &str) -> Value {
        json!({"label":label,"rows":self.ids.len(),"pages":self.pages,"families":self.family_counts,
            "dispositions":self.link_counts,"source_person_101":self.dense,"maximum_page_bytes":self.max_page_bytes,
            "distinct_source_ids":self.source_ids.iter().map(|(family,ids)|(family,ids.len())).collect::<BTreeMap<_,_>>(),
            "maximum_variants":self.max_variants,"maximum_observations_per_identity":self.max_observations,
            "maximum_record_bytes":self.max_record_bytes,"fixed_capture_sequence":self.sequence,
            "unique_ordered_observations":true,"source_body_absent":true,"aad_projections_verified":true})
    }
}

async fn traverse(
    f: &Fixture,
    run: Uuid,
    family: Option<&str>,
    disposition: Option<&str>,
    record_id: Option<Uuid>,
    first: Option<Value>,
) -> Traversal {
    let mut result = Traversal::new();
    let mut cursor = None;
    if let Some(first) = first {
        cursor = first["next_cursor"].as_str().map(str::to_owned);
        result.accept(&first);
        if cursor.is_none() {
            return result;
        }
    }
    for _ in 0..1600 {
        let value = history::records(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            history::HistoryPage {
                limit: Some(50),
                cursor,
                family: family.map(str::to_owned),
                disposition: disposition.map(str::to_owned),
                record_id,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        cursor = value["next_cursor"].as_str().map(str::to_owned);
        result.accept(&value);
        if cursor.is_none() {
            return result;
        }
    }
    panic!("bounded traversal exceeded 1600 pages");
}

async fn observation_count(f: &Fixture, run: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2",
    )
    .bind(run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)] // Mirrors the production statement's typed bindings.
fn page_args(
    run: Uuid,
    org: Uuid,
    sequence: i64,
    family: Option<&str>,
    disposition: Option<&str>,
    identity: Option<Vec<u8>>,
    identity_family: Option<&str>,
    anchor: Option<(i64, i32, Uuid)>,
) -> Vec<Arg> {
    let (seq, ordinal, id) = anchor.unwrap_or((0, -1, Uuid::nil()));
    vec![
        Arg::Uuid(run),
        Arg::Uuid(org),
        Arg::I64(sequence),
        Arg::OptionalText(family.map(str::to_owned)),
        Arg::OptionalText(disposition.map(str::to_owned)),
        Arg::OptionalBytes(identity),
        Arg::OptionalText(identity_family.map(str::to_owned)),
        Arg::OptionalUuid(None),
        Arg::I64(seq),
        Arg::I32(ordinal),
        Arg::Uuid(id),
        Arg::I64(51),
    ]
}

async fn hot_plans(f: &Fixture, main: Uuid, overlap: Uuid, queued: Uuid, report: &mut Report) {
    // Statistics maintenance uses the migrator in the caller. Actual application
    // statements here execute as the application role, without planner settings.
    let mut conn = f.pool.acquire().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT current_user::text")
            .fetch_one(&mut *conn)
            .await
            .unwrap(),
        "crm_app"
    );
    let sequence: i64 = sqlx::query_scalar("SELECT capture_sequence FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2")
        .bind(main).bind(f.org).fetch_one(&mut *conn).await.unwrap();
    let point = sqlx::query("SELECT id,capture_sequence,ordinal,identity_hmac,semantic_hmac FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2 AND family='events' ORDER BY capture_sequence,ordinal,id OFFSET 23999 LIMIT 1")
        .bind(main).bind(f.org).fetch_one(&mut *conn).await.unwrap();
    let identity: Vec<u8> = point.get("identity_hmac");
    let page_sql = history::plans_records_sql();
    explain(
        report,
        &mut conn,
        "history_record_detail",
        history::plans_record_detail_sql(),
        vec![
            Arg::Uuid(main),
            Arg::Uuid(f.org),
            Arg::I64(sequence),
            Arg::Uuid(point.get("id")),
        ],
        1,
    )
    .await;
    for (name, family, disposition, anchor) in [
        ("records_first", None, None, None),
        (
            "records_late_events",
            Some("events"),
            None,
            Some((
                point.get("capture_sequence"),
                point.get("ordinal"),
                point.get("id"),
            )),
        ),
        (
            "records_rare_excluded",
            Some("events"),
            Some("parent_excluded"),
            None,
        ),
        (
            "records_empty_conflicts",
            None,
            Some("conflicting_reference"),
            None,
        ),
    ] {
        explain(
            report,
            &mut conn,
            name,
            page_sql.clone(),
            page_args(
                main,
                f.org,
                sequence,
                family,
                disposition,
                None,
                None,
                anchor,
            ),
            51,
        )
        .await;
    }
    explain(
        report,
        &mut conn,
        "records_one_identity",
        page_sql,
        page_args(
            main,
            f.org,
            sequence,
            None,
            None,
            Some(identity.clone()),
            Some("events"),
            None,
        ),
        51,
    )
    .await;
    explain(
        report,
        &mut conn,
        "history_identity_lookup",
        statement(
            WORKER,
            "SELECT disposition,primary_person_hmac FROM migration_history_identity",
        ),
        vec![
            Arg::Uuid(main),
            Arg::Uuid(f.org),
            Arg::Text("events".into()),
            Arg::Bytes(identity.clone()),
        ],
        1,
    )
    .await;
    explain(report,&mut conn,"history_semantic_replay",statement(WORKER,"SELECT 1 FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND identity_hmac=$4 AND semantic_hmac=$5"),
        vec![Arg::Uuid(main),Arg::Uuid(f.org),Arg::Text("events".into()),Arg::Bytes(identity.clone()),Arg::Bytes(point.get("semantic_hmac"))],1).await;
    explain(
        report,
        &mut conn,
        "history_variant_count",
        statement(
            QUERIES,
            "SELECT count(*) AS observations,count(DISTINCT semantic_hmac) AS variants",
        ),
        vec![
            Arg::Uuid(main),
            Arg::Uuid(f.org),
            Arg::Text("events".into()),
            Arg::Bytes(identity),
            Arg::I64(sequence),
        ],
        1,
    )
    .await;
    let digest: Vec<u8> = sqlx::query_scalar("SELECT digest FROM migration_history_seen WHERE run_id=$1 AND organization_id=$2 AND family='events' AND kind='page' LIMIT 1")
        .bind(main).bind(f.org).fetch_one(&mut *conn).await.unwrap();
    explain(
        report,
        &mut conn,
        "history_page_seen",
        statement(WORKER, "SELECT 1 FROM migration_history_seen WHERE run_id="),
        vec![
            Arg::Uuid(main),
            Arg::Uuid(f.org),
            Arg::Text("events".into()),
            Arg::Text("page".into()),
            Arg::Bytes(digest),
        ],
        1,
    )
    .await;
    explain(
        report,
        &mut conn,
        "history_queue_candidate",
        statement(
            WORKER,
            "SELECT id,organization_id FROM migration_history_capture_run WHERE",
        ),
        vec![],
        1,
    )
    .await;
    let selected: Uuid =
        sqlx::query_scalar("SELECT id FROM migration_history_capture_run WHERE state='queued'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(selected, queued);
    let parent_plan: Uuid =
        sqlx::query_scalar("SELECT parent_plan_id FROM migration_history_capture_run WHERE id=$1")
            .bind(main)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_import_id FROM migration_history_capture_run WHERE id=$1",
    )
    .bind(main)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    explain(
        report,
        &mut conn,
        "history_parent_identity",
        statement(
            STORE,
            "SELECT i.target_id,p.id AS live_id FROM migration_import_identity",
        ),
        vec![
            Arg::Uuid(f.org),
            Arg::I64(17),
            Arg::Text("101".into()),
            Arg::Uuid(parent),
            Arg::Uuid(parent_plan),
        ],
        1,
    )
    .await;
    let now = Utc::now();
    explain(
        report,
        &mut conn,
        "history_run_page",
        statement(
            QUERIES,
            "SELECT id,created_at FROM migration_history_capture_run WHERE",
        ),
        vec![
            Arg::Uuid(f.org),
            Arg::OptionalUuid(Some(parent)),
            Arg::Time(now),
            Arg::Time(now),
            Arg::Uuid(Uuid::max()),
            Arg::I64(51),
        ],
        51,
    )
    .await;
    let conflict = sqlx::query("SELECT identity_hmac FROM migration_history_identity WHERE run_id=$1 AND organization_id=$2 AND disposition='conflicting_reference'")
        .bind(overlap).bind(f.org).fetch_one(&mut *conn).await.unwrap();
    let overlap_sequence: i64 = sqlx::query_scalar(
        "SELECT capture_sequence FROM migration_history_capture_run WHERE id=$1",
    )
    .bind(overlap)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    explain(
        report,
        &mut conn,
        "records_conflicting_identity",
        history::plans_records_sql(),
        page_args(
            overlap,
            f.org,
            overlap_sequence,
            None,
            Some("conflicting_reference"),
            Some(conflict.get("identity_hmac")),
            Some("events"),
            None,
        ),
        51,
    )
    .await;
}

async fn relation_inventory(migrator: &PgPool) -> Vec<Value> {
    let mut conn = migrator.acquire().await.unwrap();
    let mut result = Vec::new();
    for table in [
        "person",
        "organization_membership",
        "migration_import_identity",
        "migration_import_result",
        "migration_history_capture_run",
        "migration_history_stream",
        "migration_history_capture",
        "migration_history_observation",
        "migration_history_identity",
        "migration_history_person_link",
        "migration_history_seen",
        "migration_history_reservation",
        "migration_history_receipt",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql=format!("SELECT jsonb_build_object('rows',count(*),'row_bytes',coalesce(sum(pg_column_size(t)),0),'table_bytes',pg_table_size($1::regclass),'index_bytes',pg_indexes_size($1::regclass)) FROM {table} t");
        let sizes: Value = sqlx::query_scalar(&sql)
            .bind(table)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        result.push(json!({"table":table,"sizes":sizes}));
    }
    result
}

async fn cancel(f: &Fixture, id: Uuid) {
    history::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        support::action(&support::ready(f, id).await),
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn history_capture_query_plans_and_complete_bounded_reads(migrator: PgPool) {
    let (f, parent, book) = support::fixture(&migrator).await;
    let envelope = seed_envelope(&migrator, &f).await;
    let baseline = invariant(&f).await;
    let org_before: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
        book.set_records(stream, source_records(stream));
    }
    let (main, _) = support::propose(&f, parent).await;
    assert_eq!(book.count(), 0);
    support::confirm(&f, main).await;
    for _ in 0..8 {
        if observation_count(&f, main).await == 200 {
            break;
        }
        assert!(support::one(&f, &book).await);
    }
    assert_eq!(observation_count(&f, main).await, 200);
    let first = history::records(
        &f.pool,
        &f.key,
        &f.ctx,
        main,
        history::HistoryPage {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(first["next_cursor"].is_string());
    assert!(support::one(&f, &book).await);
    assert_eq!(observation_count(&f, main).await, 300);
    let before_frozen_reads = book.count();
    let frozen = traverse(&f, main, None, None, None, Some(first)).await;
    assert_eq!(frozen.ids.len(), 200);
    assert_eq!(book.count(), before_frozen_reads);
    let mut report = Report::default();
    report
        .readers
        .push(frozen.evidence("frozen_200_during_300_committed"));
    support::drain(&f, &book).await;
    let done = support::ready(&f, main).await;
    assert_eq!(done["state"], "completed_with_gaps");
    assert_eq!(observation_count(&f, main).await, (PER_FAMILY * 3) as i64);
    for stream in done["streams"].as_array().unwrap() {
        assert_eq!(stream["unique_ids"], PER_FAMILY.to_string());
        assert_eq!(stream["occurrences"], PER_FAMILY.to_string());
        assert_eq!(stream["invalid_occurrences"], "0");
        assert_eq!(stream["linked"], (DENSE + 1).to_string());
        assert_eq!(stream["parent_excluded"], "1");
        assert_eq!(stream["invalid_person_reference"], "2");
        assert_eq!(
            stream["no_parent_identity"],
            (PER_FAMILY - DENSE - 4).to_string()
        );
        assert!(stream["api_inaccessible_count"].is_null());
    }
    let requests = book.requests.lock().unwrap().clone();
    for prefix in [
        "events?limit=100&offset=",
        "calls?limit=100&offset=",
        "textMessages?limit=100&offset=",
    ] {
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with(prefix))
                .count(),
            250
        );
    }
    assert!(requests.iter().all(|request| request == "identity"
        || [
            "events?limit=100&offset=",
            "calls?limit=100&offset=",
            "textMessages?limit=100&offset="
        ]
        .iter()
        .any(|prefix| request.starts_with(prefix))));
    let capture_calls = book.count();
    let complete = traverse(&f, main, None, None, None, None).await;
    assert_eq!(complete.ids.len(), PER_FAMILY * 3);
    assert_eq!(complete.dense, DENSE * 3);
    assert_eq!(complete.pages, 1500);
    assert_eq!(complete.max_variants, 1);
    assert_eq!(complete.max_observations, 1);
    for family in ["events", "calls", "text_messages"] {
        assert_eq!(complete.family_counts[family], PER_FAMILY);
        assert_eq!(
            complete.source_ids[family],
            (1..=PER_FAMILY).collect::<BTreeSet<_>>()
        );
    }
    report
        .readers
        .push(complete.evidence("all_75000_authorized_projection_reads"));
    for family in ["events", "calls", "text_messages"] {
        let rare = traverse(&f, main, Some(family), Some("parent_excluded"), None, None).await;
        assert_eq!(rare.ids.len(), 1);
        report
            .readers
            .push(rare.evidence(&format!("rare_{family}_excluded")));
    }
    let empty = traverse(&f, main, None, Some("conflicting_reference"), None, None).await;
    assert!(empty.ids.is_empty());
    report.readers.push(empty.evidence("empty_conflict_filter"));
    let sample = *complete.ids.first().unwrap();
    let one = traverse(&f, main, None, None, Some(sample), None).await;
    assert_eq!(one.ids.len(), 1);
    let detail = history::record_detail(&f.pool, &f.key, &f.ctx, main, sample)
        .await
        .unwrap();
    assert_eq!(detail["integrity_status"], "verified_projection");
    report.readers.push(one.evidence("same_identity_filter"));
    assert_eq!(
        book.count(),
        capture_calls,
        "retained readers must make zero source calls"
    );

    // Conflicting observations are real encrypted worker output in a separate
    // incomplete run; they cannot be smuggled into completed enumeration.
    book.set_records(Stream::Events, (1..=200).map(support::item).collect());
    let mut second = (100..=199).map(support::item).collect::<Vec<_>>();
    second[0]["personId"] = json!(102);
    second[0]["unknownChanged"] = json!("synthetic conflicting observation");
    book.set_raw(Stream::Events,100,Ok(Capture{status:200,body:serde_json::to_vec(&json!({"_metadata":{"collection":"events","limit":100,"offset":100,"total":200},"events":second})).unwrap(),truncated:false,source_version:Some("constructed-scale-overlap".into())}));
    let (overlap, _) = support::propose(&f, parent).await;
    support::confirm(&f, overlap).await;
    support::drain(&f, &book).await;
    let paused = support::ready(&f, overlap).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "enumeration_identity_uncertain");
    assert_eq!(paused["streams"][0]["unique_ids"], "199");
    let before_variant_reads = book.count();
    let conflicts = traverse(&f, overlap, None, Some("conflicting_reference"), None, None).await;
    assert_eq!(conflicts.ids.len(), 2);
    let variants = traverse(
        &f,
        overlap,
        None,
        None,
        Some(*conflicts.ids.first().unwrap()),
        None,
    )
    .await;
    assert_eq!(variants.ids, conflicts.ids);
    assert_eq!(variants.max_variants, 2);
    assert_eq!(variants.max_observations, 2);
    report
        .readers
        .push(variants.evidence("two_conflicting_observations_from_actual_worker"));
    assert_eq!(book.count(), before_variant_reads);
    let (queued, _) = support::propose(&f, parent).await;
    support::confirm(&f, queued).await;
    let sizes = relation_inventory(&migrator).await;
    hot_plans(&f, main, overlap, queued, &mut report).await;
    let listed = history::list(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        history::HistoryPage {
            parent_import_id: Some(parent),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(listed["captures"].as_array().unwrap().len(), 3);
    cancel(&f, queued).await;
    cancel(&f, overlap).await;
    assert_eq!(
        book.count(),
        before_variant_reads,
        "planning/read/cancel must not request the source"
    );
    assert_eq!(
        invariant(&f).await,
        baseline,
        "native/parent/sibling rows changed during history work"
    );
    let size_sql = statement(
        ACCOUNTING_TEST,
        "SELECT (SELECT octet_length(profile_version)",
    );
    let mut accounting = Vec::new();
    let mut retained = 0i64;
    for run in [main, overlap, queued] {
        let measured: i64 = sqlx::query_scalar(&size_sql)
            .bind(run)
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let state = support::ready(&f, run).await;
        assert_eq!(state["retained_bytes"], measured.to_string());
        assert_eq!(state["reserved_bytes"], "0");
        retained += measured;
        accounting.push(json!({"run":run,"state":state["state"],"retained_bytes":measured,"raw_bytes":state["raw_bytes"],"reserved_bytes":0}));
    }
    let ledger=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        ledger.get::<i64, _>("retained_bytes"),
        org_before + retained
    );
    assert_eq!(ledger.get::<i64, _>("reserved_bytes"), 0);
    let sources = [
        ("history_capture.rs", COMMANDS),
        ("history_capture_source.rs", SOURCE),
        ("history_capture_store.rs", STORE),
        ("history_capture_worker.rs", WORKER),
        ("history_capture_queries.rs", QUERIES),
        ("history_schema.sql", SCHEMA),
    ];
    let evidence = json!({"fixture":{"kind":"constructed source through real worker; valid AAD","envelope":envelope,
        "rows_per_family":PER_FAMILY,"dense_per_family":DENSE,"advancing_history_pages":750,"fake_source_calls_main":capture_calls,
        "fake_source_calls_final":book.count(),"real_source_requests":0,"main_terminal":done,"overlap_pause":paused,
        "relations":sizes,"parent_native_sibling_baseline":baseline,"parent_native_sibling_unchanged":true},
        "readers":report.readers,"accounting":accounting,"explain_plans":report.plans,
        "source_sha256":sources.iter().map(|(file,source)|json!({"file":file,"sha256":sha(source.as_bytes())})).collect::<Vec<_>>(),
        "checks":{"failures":report.failures,"returned_row_bounds":report.failures.is_empty(),"no_temp_spills":report.failures.is_empty()},
        "limits":"No absolute latency/capacity claim. Scan work is reported separately from bounded fetched/decrypted rows. Public source behavior remains unqualified."});
    if let Ok(path) = std::env::var("CRM_010D1_PLAN_OUTPUT") {
        let path = std::path::Path::new(&path);
        assert!(
            path.is_absolute(),
            "plan output must be an explicit absolute path"
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        println!("CRM_010D1_PLAN_OUTPUT {}", path.display());
    } else {
        println!("CRM_010D1_HISTORY_PLANS {evidence}");
    }
    assert!(
        report.failures.is_empty(),
        "plan checks failed: {}",
        report.failures.join("; ")
    );
}

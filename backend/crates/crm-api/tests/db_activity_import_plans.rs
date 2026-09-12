//! D-050/A10/A12 opt-in plans and bounded native-reader evidence.
//!
//! A bounded synthetic Book finishes the actual retained import first. Bulk
//! native rows then model exactly 25,000 People/50 members, diffuse activity and
//! 501 full-sized notes plus open/completed tasks on one imported Person.
//! Bulk retained rows are INERT plan-cardinality fixtures: ciphertext is copied,
//! not rebound to new AAD. No activity worker or retained decryptor runs after
//! bulk seeding. This is not import fidelity, accounting, or capacity proof.
//! Native paged reads do run with real admin authorization and the real binding.
//!
//! SQL is taken from exact production literals (unique-prefix asserted) or public
//! query constants. JSON includes SQL/source hashes, typed bindings, EXPLAIN
//! ANALYZE BUFFERS, relation sizes, page bounds, and exact traversal totals.
//! No planner overrides or absolute latency thresholds are used.
use std::collections::BTreeSet;

use crate::{
    db_activity_source as support,
    import_support::{self, Fixture},
};
use chrono::{DateTime, Utc};
use crm_api::{
    auth::AuthContext,
    domain::{
        admin::Role,
        migration::activity_review::{self as review, PageQuery, TaskState},
    },
    ids::{OrganizationId, PersonId, UserId},
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

const PEOPLE: i64 = 25_000;
const DENSE: i64 = 501;
const COMMANDS: &str = include_str!("../../crm-app/src/domain/migration/activity.rs");
const STORE: &str = include_str!("../../crm-app/src/domain/migration/activity_store.rs");
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/activity_worker.rs");
const QUERIES: &str = include_str!("../../crm-app/src/domain/migration/activity_queries.rs");
const REVIEW: &str = include_str!("../../crm-app/src/domain/migration/activity_review.rs");
const WORKSPACE: &str = include_str!("../../crm-app/src/auth/workspace.rs");
const SCHEMA: &str = include_str!("../migrations/20260920000001_fub_activity_import.sql");

fn sha(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let mut found = source.match_indices(&marker).map(|(start, _)| {
        serde_json::Deserializer::from_str(&source[start..])
            .into_iter::<String>()
            .next()
            .expect("production SQL literal")
            .expect("exact SQL literal decoding")
    });
    let sql = found
        .next()
        .unwrap_or_else(|| panic!("missing production SQL prefix: {prefix}"));
    assert!(
        found.all(|other| other == sql),
        "ambiguous production SQL prefix: {prefix}"
    );
    sql
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum Arg {
    Uuid(Uuid),
    Text(String),
    Int(i64),
    Bytes(Vec<u8>),
    NullableText(Option<String>),
    NullableUuid(Option<Uuid>),
    Time(Option<DateTime<Utc>>),
}
impl From<Uuid> for Arg {
    fn from(v: Uuid) -> Self {
        Self::Uuid(v)
    }
}
impl From<&str> for Arg {
    fn from(v: &str) -> Self {
        Self::Text(v.into())
    }
}
impl From<String> for Arg {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<i64> for Arg {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<Vec<u8>> for Arg {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}
impl From<Option<&str>> for Arg {
    fn from(v: Option<&str>) -> Self {
        Self::NullableText(v.map(str::to_owned))
    }
}
impl From<Option<Uuid>> for Arg {
    fn from(v: Option<Uuid>) -> Self {
        Self::NullableUuid(v)
    }
}
impl From<Option<DateTime<Utc>>> for Arg {
    fn from(v: Option<DateTime<Utc>>) -> Self {
        Self::Time(v)
    }
}

#[derive(Default)]
struct Report {
    plans: Vec<Value>,
    failures: Vec<String>,
    fixture: Value,
    readers: Value,
    workspace: Value,
}
fn nodes<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(values) => {
            if values.contains_key("Node Type") {
                out.push(v)
            }
            for child in values.values() {
                nodes(child, out)
            }
        }
        Value::Array(values) => {
            for child in values {
                nodes(child, out)
            }
        }
        _ => {}
    }
}
async fn collect(
    report: &mut Report,
    conn: &mut PgConnection,
    label: &str,
    sql: &str,
    args: Vec<Arg>,
    max_rows: i64,
) {
    let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    let mut query = sqlx::query_scalar::<_, Value>(&wrapped);
    for arg in &args {
        query = match arg {
            Arg::Uuid(v) => query.bind(*v),
            Arg::Text(v) => query.bind(v),
            Arg::Int(v) => query.bind(*v),
            Arg::Bytes(v) => query.bind(v),
            Arg::NullableText(v) => query.bind(v),
            Arg::NullableUuid(v) => query.bind(*v),
            Arg::Time(v) => query.bind(*v),
        };
    }
    let plan = query
        .fetch_one(conn)
        .await
        .unwrap_or_else(|error| panic!("EXPLAIN failed for {label}: {error}"));
    let mut all = vec![];
    nodes(&plan, &mut all);
    let returned = plan[0]["Plan"]["Actual Rows"]
        .as_f64()
        .expect("actual row count");
    if returned > max_rows as f64 {
        report
            .failures
            .push(format!("{label}: returned {returned} exceeds {max_rows}"));
    }
    for node in &all {
        if node["Sort Space Type"] == "Disk"
            || node["Temp Read Blocks"].as_u64().unwrap_or(0) > 0
            || node["Temp Written Blocks"].as_u64().unwrap_or(0) > 0
            || node["Hash Batches"].as_u64().unwrap_or(1) > 1
        {
            report
                .failures
                .push(format!("{label}: temporary spill or batched hash"));
        }
    }
    let scans:Vec<Value>=all.into_iter().filter(|v|v.get("Relation Name").is_some()).map(|v|json!({
        "table":v["Relation Name"],"node":v["Node Type"],"index":v["Index Name"],"loops":v["Actual Loops"],"rows":v["Actual Rows"],"removed":v["Rows Removed by Filter"],"index_condition":v["Index Cond"]})).collect();
    report.plans.push(json!({"label":label,"sql":sql,"sql_sha256":sha(sql),"bindings":args,"maximum_returned_rows":max_rows,"scans":scans,"plan":plan}));
}
macro_rules! probe {
    ($report:expr,$conn:expr,$label:expr,$source:expr,$prefix:expr,$max:expr $(,$arg:expr)* $(,)?)=>{{
        let sql=statement($source,$prefix);collect($report,$conn,$label,&sql,vec![$(Arg::from($arg)),*],$max).await;
    }};
}

struct Scale {
    f: Fixture,
    parent: Uuid,
    parent_plan: Uuid,
    child: Uuid,
    plan: Uuid,
    person: Uuid,
    capture: Uuid,
    source: Uuid,
    manifest: Uuid,
    result: Uuid,
    mapping: Uuid,
    key: Vec<u8>,
    request: Uuid,
    note: Uuid,
    task: Uuid,
    source_after: Uuid,
    manifest_after: Uuid,
    result_after: Uuid,
    empty_held: Uuid,
    note_after: Uuid,
    note_time: DateTime<Utc>,
    task_after: Uuid,
    task_time: Option<DateTime<Utc>>,
    completed_after: Uuid,
    completed_time: DateTime<Utc>,
}

async fn seed(migrator: &PgPool, report: &mut Report) -> Scale {
    let book = support::book();
    book.set_records(
        crm_api::domain::migration::snapshot_source::Stream::Notes,
        vec![json!({"id":11,"personId":101})],
    );
    book.set_raw(crm_api::domain::migration::snapshot_source::Stream::NoteDetail,0,200,
        serde_json::to_vec(&json!({"id":11,"personId":101,"createdById":3,"body":"Real retained note","isHtml":false,"created":"2026-09-01T12:00:00Z"})).unwrap(),false);
    book.set_records(
        crm_api::domain::migration::snapshot_source::Stream::TasksOpen,
        vec![support::task(21)],
    );
    let f = import_support::fixture_with_book(migrator, book).await;
    let parent = support::completed_parent(&f).await;
    let (child, first) = support::prepare(&f, parent).await;
    let ready = support::replan(&f, child, &first, support::choices(&f, child).await, None).await;
    let plan = support::plan_id(&ready);
    let done = support::confirm(&f, child, &ready).await;
    let actual_calls = f.reader.calls();
    let mut conn = migrator.acquire().await.unwrap();
    let parent_plan: Uuid =
        sqlx::query_scalar("SELECT confirmed_plan_id FROM migration_import WHERE id=$1")
            .bind(parent)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let person: Uuid = sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let request: Uuid = sqlx::query_scalar(
        "SELECT request_id FROM migration_activity_receipt WHERE import_id=$1 AND action='confirm'",
    )
    .bind(child)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    let real_native=sqlx::query("SELECT (SELECT id FROM note WHERE organization_id=$1) AS note,(SELECT id FROM task WHERE organization_id=$1) AS task").bind(f.org).fetch_one(&mut *conn).await.unwrap();
    let note: Uuid = real_native.get("note");
    let task: Uuid = real_native.get("task");
    sqlx::query("CREATE TEMP TABLE activity_members AS SELECT n,gen_random_uuid() AS id FROM generate_series(3,50)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'activity-plan-'||n||'@synthetic.test','Synthetic member '||n FROM activity_members").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM activity_members").bind(f.org).execute(&mut *conn).await.unwrap();
    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM organization_membership WHERE organization_id=$1 ORDER BY user_id",
    )
    .bind(f.org)
    .fetch_all(&mut *conn)
    .await
    .unwrap();
    sqlx::query("CREATE TEMP TABLE activity_rows AS SELECT n,CASE WHEN n=1 THEN $1 ELSE gen_random_uuid() END AS person,gen_random_uuid() AS source,gen_random_uuid() AS record,gen_random_uuid() AS manifest,gen_random_uuid() AS result,gen_random_uuid() AS native,((n-1)/100+1)::integer AS batch,((n-1)%100)::integer AS ordinal,(1000000+n)::text AS source_id FROM generate_series(1,$2::bigint)n").bind(person).bind(PEOPLE).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT person,$1,'Synthetic','Activity '||n,$2,($3::uuid[])[1+((n-1)%50)] FROM activity_rows WHERE n>1").bind(f.org).bind(f.lead_stage).bind(&members).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO note(id,organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,person,$2,left(repeat(md5(n::text),12),250),'migration',gen_random_uuid(),'2026-01-01'::timestamptz+n*interval '1 minute','2026-01-01'::timestamptz+n*interval '1 minute' FROM activity_rows").bind(f.org).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id,source,source_external_id,created_at,updated_at) SELECT native,$1,person,left(repeat('Synthetic source task '||n,30),499)||'x','call',CASE WHEN n%10<>0 THEN '2026-09-01'::timestamptz+n*interval '1 minute' END,$2,$3,'migration',gen_random_uuid(),'fub','v1:17:'||source_id,'2026-01-01','2026-01-01' FROM activity_rows").bind(f.org).bind(f.member).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,completed_at,assignee_user_id,created_by_user_id,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,person,left(repeat('Synthetic completed task '||n,30),499)||'x','email','2026-09-01'::timestamptz+n*interval '1 minute',$2,$3,'migration',gen_random_uuid(),'2026-01-01','2026-01-01' FROM activity_rows").bind(f.org).bind(f.member).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO note(id,organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,$2,$3,left((SELECT string_agg(md5(n::text||':'||part::text),'' ORDER BY part) FROM generate_series(1,313)part),10000),'migration',gen_random_uuid(),'2026-03-01'::timestamptz+n*interval '1 minute','2026-03-01'::timestamptz+n*interval '1 minute' FROM generate_series(1,$4::bigint)n").bind(f.org).bind(person).bind(f.actor).bind(DENSE).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,due_at,completed_at,assignee_user_id,created_by_user_id,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,$2,left((SELECT string_agg(md5(n::text||':'||part::text),'' ORDER BY part) FROM generate_series(1,16)part),500),'follow_up',CASE WHEN state=0 AND n%5<>0 THEN '2026-03-01'::timestamptz+n*interval '1 minute' END,CASE WHEN state=1 THEN '2026-03-01'::timestamptz+n*interval '1 minute' END,$3,$4,'migration',gen_random_uuid(),'2026-01-01','2026-01-01' FROM generate_series(1,$5::bigint)n CROSS JOIN generate_series(0,1)state").bind(f.org).bind(person).bind(f.member).bind(f.actor).bind(DENSE).execute(&mut *conn).await.unwrap();
    // Inert 100-item pages maintain the observed capture and row-key shapes.
    sqlx::query("CREATE TEMP TABLE activity_captures AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,250)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,source_version,classification,truncated,accepted) SELECT h.id,c.snapshot_id,c.organization_id,'tasks_open',1000+h.n,100000+h.n,decode(repeat(md5(h.n::text),2),'hex'),c.representation,200,c.raw_byte_len,c.nonce,c.ciphertext,'inert-plan-only','inert_plan_only',false,true FROM activity_captures h CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='tasks_open' AND accepted LIMIT 1)c").bind(f.snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT a.record,s.snapshot_id,s.organization_id,c.id,1000+a.batch,a.ordinal,'tasks',a.source_id,s.representation,decode(repeat(md5(a.source_id),2),'hex'),s.projection_nonce,s.projection_ciphertext,false FROM activity_rows a JOIN activity_captures c ON c.n=a.batch CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='tasks' LIMIT 1)s").bind(f.snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,nonce,ciphertext) SELECT a.source,$1,$2,$3,$4,'tasks',a.source_id,a.record,c.id,1000+a.batch,a.ordinal,'tasks_open',s.representation,decode(repeat(md5(a.source_id),2),'hex'),s.nonce,s.ciphertext FROM activity_rows a JOIN activity_captures c ON c.n=a.batch CROSS JOIN LATERAL(SELECT * FROM migration_activity_source WHERE plan_id=$1 AND family='tasks' LIMIT 1)s").bind(plan).bind(child).bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_manifest(id,plan_id,import_id,organization_id,kind,source_id,source_row_id,source_person_id,parent_result_id,person_id,target_id,native_source_key,creator_user_id,assignee_user_id,disposition,added_byte_bound,nonce,ciphertext) SELECT a.manifest,$1,$2,$3,'task',a.source_id,a.source,'101',s.parent_result_id,a.person,a.native,'v1:17:'||a.source_id,s.creator_user_id,s.assignee_user_id,CASE WHEN a.n%1000=0 THEN 'held' ELSE 'eligible' END,16384,s.nonce,s.ciphertext FROM activity_rows a CROSS JOIN LATERAL(SELECT * FROM migration_activity_manifest WHERE plan_id=$1 AND kind='task' LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_result(id,import_id,plan_id,organization_id,manifest_id,kind,source_id,person_id,target_id,disposition,nonce,ciphertext,actual_bytes) SELECT a.result,$1,$2,$3,a.manifest,'task',a.source_id,a.person,a.native,CASE WHEN a.n%1000=0 THEN 'held' ELSE 'applied' END,s.nonce,s.ciphertext,0 FROM activity_rows a CROSS JOIN LATERAL(SELECT * FROM migration_activity_result WHERE plan_id=$2 AND kind='task' LIMIT 1)s").bind(child).bind(plan).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,import_id,plan_id,manifest_id) SELECT $1,17,'task',source_id,native,$2,$3,manifest FROM activity_rows").bind(f.org).bind(child).bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_manifest_issue(manifest_id,plan_id,import_id,organization_id,code) SELECT manifest,$1,$2,$3,'source_variants' FROM activity_rows WHERE n%1000=0").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_result_issue(result_id,plan_id,import_id,organization_id,code) SELECT result,$1,$2,$3,'source_variants' FROM activity_rows WHERE n%1000=0").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("CREATE TEMP TABLE activity_mappings AS SELECT n,gen_random_uuid() AS id,decode(repeat(md5('role-'||n),2),'hex') AS key,CASE n%3 WHEN 0 THEN 'note_author' WHEN 1 THEN 'task_creator' ELSE 'task_assignee' END AS kind FROM generate_series(1,150)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_mapping(id,plan_id,import_id,organization_id,kind,source_key,dependent_count,nonce,ciphertext) SELECT a.id,$1,$2,$3,a.kind,a.key,500,s.nonce,s.ciphertext FROM activity_mappings a CROSS JOIN LATERAL(SELECT * FROM migration_activity_mapping WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),$1,$2,$3,a.kind,a.key,s.nonce,s.ciphertext FROM activity_mappings a CROSS JOIN LATERAL(SELECT * FROM migration_activity_choice WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_activity_source s SET source_only_counts=jsonb_build_object('unknown_properties_source_only',2) FROM activity_rows a WHERE s.id=a.source AND a.n%1000=0").execute(&mut *conn).await.unwrap();
    let mut sizes = vec![];
    for table in [
        "person",
        "organization_membership",
        "note",
        "task",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_activity_import",
        "migration_activity_plan",
        "migration_activity_source",
        "migration_activity_mapping",
        "migration_activity_choice",
        "migration_activity_manifest",
        "migration_activity_manifest_issue",
        "migration_activity_identity",
        "migration_activity_result",
        "migration_activity_result_issue",
        "migration_activity_receipt",
        "migration_activity_reservation",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
        let value:Value=sqlx::query_scalar(&format!("SELECT jsonb_build_object('rows',count(*),'row_bytes',sum(pg_column_size(t)),'table_bytes',pg_table_size($1::regclass),'index_bytes',pg_indexes_size($1::regclass)) FROM {table} t")).bind(table).fetch_one(&mut *conn).await.unwrap();
        sizes.push(json!({"table":table,"metrics":value}));
    }
    let counts:Value=sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1),'notes',(SELECT count(*) FROM note WHERE organization_id=$1),'tasks',(SELECT count(*) FROM task WHERE organization_id=$1),'dense_full_notes',(SELECT count(*) FROM note WHERE organization_id=$1 AND person_id=$2 AND char_length(body)=10000),'dense_notes',(SELECT count(*) FROM note WHERE organization_id=$1 AND person_id=$2),'dense_open',(SELECT count(*) FROM task WHERE organization_id=$1 AND person_id=$2 AND completed_at IS NULL),'dense_completed',(SELECT count(*) FROM task WHERE organization_id=$1 AND person_id=$2 AND completed_at IS NOT NULL))").bind(f.org).bind(person).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(counts["people"], PEOPLE);
    assert_eq!(counts["members"], 50);
    assert_eq!(counts["dense_full_notes"], DENSE);
    let point=sqlx::query("SELECT a.*,(SELECT id FROM activity_captures WHERE n=240) AS capture FROM activity_rows a WHERE n=24001").fetch_one(&mut *conn).await.unwrap();
    let mapping_row=sqlx::query("SELECT * FROM migration_activity_mapping WHERE plan_id=$1 AND kind='task_creator' ORDER BY id LIMIT 1").bind(plan).fetch_one(&mut *conn).await.unwrap();
    let anchors=sqlx::query("SELECT (SELECT id FROM migration_activity_source WHERE plan_id=$1 ORDER BY id OFFSET 23999 LIMIT 1) AS source_after,(SELECT id FROM migration_activity_manifest WHERE plan_id=$1 ORDER BY id OFFSET 23999 LIMIT 1) AS manifest_after,(SELECT id FROM migration_activity_result WHERE plan_id=$1 ORDER BY id OFFSET 23999 LIMIT 1) AS result_after,(SELECT max(id::text)::uuid FROM migration_activity_manifest WHERE plan_id=$1 AND disposition='held') AS empty_held").bind(plan).fetch_one(&mut *conn).await.unwrap();
    let n=sqlx::query("SELECT id,created_at FROM note WHERE organization_id=$1 AND person_id=$2 ORDER BY created_at,id OFFSET 450 LIMIT 1").bind(f.org).bind(person).fetch_one(&mut *conn).await.unwrap();
    let t=sqlx::query("SELECT id,due_at FROM task WHERE organization_id=$1 AND person_id=$2 AND completed_at IS NULL ORDER BY due_at NULLS LAST,id OFFSET 450 LIMIT 1").bind(f.org).bind(person).fetch_one(&mut *conn).await.unwrap();
    let c=sqlx::query("SELECT id,completed_at FROM task WHERE organization_id=$1 AND person_id=$2 AND completed_at IS NOT NULL ORDER BY completed_at,id OFFSET 450 LIMIT 1").bind(f.org).bind(person).fetch_one(&mut *conn).await.unwrap();
    report.fixture = json!({"people":PEOPLE,"members":50,"concentrated_full_size_notes":DENSE,"counts":counts,"native_row_widths":"diffuse250-char notes and500-char tasks; concentrated10000-char notes and500-char tasks","source_fixture_classification":"bounded_real_book_then_inert_copied_ciphertext_plan_cardinality_only","original_confirmed_child":done,"relation_storage":sizes,"actual_source_calls":actual_calls,"source_boundary_note":"250 inert capture pages added beyond real snapshot boundary, never processed by worker"});
    assert_eq!(f.reader.calls(), actual_calls);
    Scale {
        parent,
        parent_plan,
        child,
        plan,
        person,
        capture: point.get("capture"),
        source: point.get("source"),
        manifest: point.get("manifest"),
        result: point.get("result"),
        mapping: mapping_row.get("id"),
        key: mapping_row.get("source_key"),
        request,
        note,
        task,
        source_after: anchors.get("source_after"),
        manifest_after: anchors.get("manifest_after"),
        result_after: anchors.get("result_after"),
        empty_held: anchors.get("empty_held"),
        note_after: n.get("id"),
        note_time: n.get("created_at"),
        task_after: t.get("id"),
        task_time: t.get("due_at"),
        completed_after: c.get("id"),
        completed_time: c.get("completed_at"),
        f,
    }
}

async fn hot_plans(migrator: &PgPool, s: &Scale, r: &mut Report) {
    let mut conn = migrator.acquire().await.unwrap();
    let org = s.f.org;
    let nil = Uuid::nil();
    probe!(
        r,
        &mut conn,
        "parent_exists",
        COMMANDS,
        "SELECT EXISTS(SELECT 1 FROM migration_import WHERE",
        1,
        s.parent,
        org
    );
    probe!(
        r,
        &mut conn,
        "parent_eligibility",
        COMMANDS,
        "SELECT p.*,o.workspace_revision",
        1,
        s.parent,
        org
    );
    probe!(
        r,
        &mut conn,
        "member_destination",
        COMMANDS,
        "SELECT m.user_id,u.display_name",
        50,
        org
    );
    probe!(
        r,
        &mut conn,
        "status_storage",
        COMMANDS,
        "SELECT s.run_byte_limit",
        1,
        s.f.snapshot,
        org
    );
    probe!(
        r,
        &mut conn,
        "cancel_capacity",
        COMMANDS,
        "SELECT COALESCE(sum(byte_count)",
        1,
        s.child,
        org
    );
    probe!(
        r,
        &mut conn,
        "child_lifetime_exists",
        COMMANDS,
        "SELECT EXISTS(SELECT 1 FROM migration_activity_import",
        1,
        s.parent,
        org
    );
    probe!(
        r,
        &mut conn,
        "mapping_choice_scope",
        COMMANDS,
        "SELECT * FROM migration_activity_mapping WHERE id=",
        1,
        s.mapping,
        s.plan,
        s.child,
        org
    );
    probe!(
        r,
        &mut conn,
        "mapping_member_lock",
        COMMANDS,
        "SELECT status FROM organization_membership",
        1,
        org,
        s.f.member
    );
    probe!(
        r,
        &mut conn,
        "child_lock",
        STORE,
        "SELECT * FROM migration_activity_import WHERE",
        1,
        s.child,
        org
    );
    probe!(
        r,
        &mut conn,
        "plan_lock",
        STORE,
        "SELECT * FROM migration_activity_plan WHERE",
        1,
        s.plan,
        s.child,
        org
    );
    probe!(
        r,
        &mut conn,
        "retention_admission",
        STORE,
        "SELECT s.run_byte_limit",
        1,
        s.f.snapshot,
        org
    );
    probe!(
        r,
        &mut conn,
        "logical_settlement_delta",
        STORE,
        "SELECT measured_bytes-retained_bytes",
        1,
        s.child,
        org
    );
    probe!(
        r,
        &mut conn,
        "reservation_release_scope",
        STORE,
        "SELECT token FROM migration_activity_reservation",
        1,
        s.child,
        org,
        "work"
    );
    probe!(
        r,
        &mut conn,
        "receipt_lookup",
        STORE,
        "SELECT * FROM migration_activity_receipt",
        1,
        org,
        s.f.actor,
        "confirm",
        s.request
    );
    probe!(
        r,
        &mut conn,
        "worker_authority",
        WORKER,
        "SELECT role,status FROM organization_membership",
        1,
        org,
        s.f.actor
    );
    probe!(
        r,
        &mut conn,
        "worker_idle_claim",
        WORKER,
        "SELECT i.id,i.organization_id,i.executor_user_id",
        0
    );
    // EXPLAIN ANALYZE's read/lock effects are transaction scoped. This temporary
    // state alteration exercises the real active-claim predicate, then rolls back.
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_activity_import SET state='queued',lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(s.child).execute(&mut *conn).await.unwrap();
    probe!(
        r,
        &mut conn,
        "worker_active_claim",
        WORKER,
        "SELECT i.id,i.organization_id,i.executor_user_id",
        1
    );
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
    probe!(
        r,
        &mut conn,
        "choice_inheritance_page",
        WORKER,
        "SELECT * FROM migration_activity_choice WHERE plan_id=$1 AND organization_id=$2 AND id>",
        50,
        s.plan,
        org,
        nil
    );
    probe!(
        r,
        &mut conn,
        "choice_duplicate_lookup",
        WORKER,
        "SELECT EXISTS(SELECT 1 FROM migration_activity_choice",
        1,
        s.plan,
        org,
        "task_creator",
        s.key.clone()
    );
    probe!(
        r,
        &mut conn,
        "retained_capture_page",
        WORKER,
        "SELECT * FROM migration_snapshot_capture WHERE snapshot_id=",
        1,
        s.f.snapshot,
        org,
        1239_i64,
        1250_i64
    );
    probe!(
        r,
        &mut conn,
        "capture_record_links",
        WORKER,
        "SELECT * FROM migration_snapshot_record WHERE capture_id=",
        100,
        s.capture,
        s.f.snapshot,
        org
    );
    probe!(
        r,
        &mut conn,
        "parent_identity_result_target",
        WORKER,
        "SELECT r.id,r.person_id FROM migration_import_identity",
        1,
        org,
        17_i64,
        "101",
        s.parent,
        s.parent_plan
    );
    probe!(r,&mut conn,"role_mapping_key",WORKER,"SELECT * FROM migration_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=",1,s.plan,org,"task_creator",s.key.clone());
    probe!(
        r,
        &mut conn,
        "approved_choice_key",
        WORKER,
        "SELECT * FROM migration_activity_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=",
        1,
        s.plan,
        org,
        "task_creator",
        s.key.clone()
    );
    for (label, after) in [("source_first", nil), ("source_deep", s.source_after)] {
        probe!(
            r,
            &mut conn,
            label,
            WORKER,
            "SELECT * FROM migration_activity_source a WHERE",
            1,
            s.plan,
            org,
            after
        );
    }
    probe!(
        r,
        &mut conn,
        "manifest_identity_exists",
        WORKER,
        "SELECT EXISTS(SELECT 1 FROM migration_activity_manifest",
        1,
        s.plan,
        org,
        "task",
        "1024001"
    );
    probe!(
        r,
        &mut conn,
        "whole_representation_variants",
        WORKER,
        "SELECT representation,count(DISTINCT semantic_hmac)",
        3,
        s.plan,
        org,
        "tasks",
        "1024001"
    );
    probe!(r,&mut conn,"qualified_note_detail_selection",WORKER,"SELECT a.* FROM migration_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='notes'",1,s.plan,org,"11");
    probe!(r,&mut conn,"qualified_task_selection",WORKER,"SELECT a.* FROM migration_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='tasks'",1,s.plan,org,"1024001");
    probe!(
        r,
        &mut conn,
        "collection_previous_request_cursor",
        WORKER,
        "SELECT a.* FROM migration_snapshot_capture c JOIN migration_activity_source a",
        1,
        s.f.snapshot,
        org,
        "tasks_open",
        s.plan,
        100240_i64,
        1241_i64
    );
    for (label, id) in [
        ("source_only_observation_counts", "1024000"),
        ("source_only_empty_counts", "1024001"),
    ] {
        probe!(r,&mut conn,label,WORKER,"WITH observations AS (SELECT DISTINCT ON (representation,semantic_hmac) source_only_counts",1,s.plan,org,"tasks",id);
    }
    probe!(
        r,
        &mut conn,
        "negative_detail_fingerprint",
        WORKER,
        "SELECT EXISTS(SELECT 1 FROM migration_activity_source WHERE",
        1,
        s.plan,
        org,
        vec![0_u8; 32]
    );
    probe!(r,&mut conn,"captured_timezone_evidence",WORKER,"SELECT * FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND family='users'",101,s.plan,org,"3");
    probe!(
        r,
        &mut conn,
        "durable_source_identity",
        WORKER,
        "SELECT target_id FROM migration_activity_identity",
        1,
        org,
        17_i64,
        "task",
        "1024001"
    );
    for (label, prefix, key) in [
        (
            "native_note_source_lock",
            "SELECT * FROM note WHERE organization_id=$1 AND source=",
            "v1:17:11",
        ),
        (
            "legacy_note_collision",
            "SELECT EXISTS(SELECT 1 FROM note WHERE organization_id=",
            "11",
        ),
        (
            "native_task_source_lock",
            "SELECT * FROM task WHERE organization_id=$1 AND source=",
            "v1:17:21",
        ),
        (
            "legacy_task_collision",
            "SELECT EXISTS(SELECT 1 FROM task WHERE organization_id=",
            "21",
        ),
    ] {
        probe!(r, &mut conn, label, WORKER, prefix, 1, org, key);
    }
    for (label, prefix, id) in [
        (
            "note_target_tombstone",
            "SELECT EXISTS(SELECT 1 FROM note WHERE id=",
            s.note,
        ),
        (
            "task_target_tombstone",
            "SELECT EXISTS(SELECT 1 FROM task WHERE id=",
            s.task,
        ),
        ("native_note_storage", "SELECT pg_column_size(n)", s.note),
        ("native_task_storage", "SELECT pg_column_size(t)", s.task),
        (
            "person_write_lock",
            "SELECT id FROM person WHERE id=",
            s.person,
        ),
    ] {
        probe!(r, &mut conn, label, WORKER, prefix, 1, id, org);
    }
    probe!(
        r,
        &mut conn,
        "mapping_revalidation_page",
        WORKER,
        "SELECT * FROM migration_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>",
        50,
        s.plan,
        org,
        nil
    );
    probe!(
        r,
        &mut conn,
        "execution_manifest_deep",
        WORKER,
        "SELECT * FROM migration_activity_manifest WHERE plan_id=$1 AND organization_id=$2 AND id>",
        1,
        s.plan,
        org,
        s.manifest_after
    );
    for (label, parent) in [
        ("child_list_first", None),
        ("child_list_parent", Some(s.parent)),
        ("child_list_missing_parent", Some(nil)),
    ] {
        probe!(
            r,
            &mut conn,
            label,
            QUERIES,
            "SELECT id FROM migration_activity_import WHERE",
            51,
            org,
            nil,
            parent,
            51_i64
        );
    }
    let result_empty:Uuid=sqlx::query_scalar("SELECT id FROM migration_activity_result WHERE plan_id=$1 AND disposition='held' ORDER BY id DESC LIMIT 1").bind(s.plan).fetch_one(&mut *conn).await.unwrap();
    for (name, prefix, deep, empty) in [
        (
            "records",
            "SELECT a.id FROM migration_activity_manifest a WHERE",
            s.manifest_after,
            s.empty_held,
        ),
        (
            "results",
            "SELECT a.id FROM migration_activity_result a WHERE",
            s.result_after,
            result_empty,
        ),
    ] {
        for (suffix, after, kind, disposition, issue, max) in [
            ("first", nil, None, None, None, 51),
            ("deep", deep, None, None, None, 51),
            ("rare_held", nil, Some("task"), Some("held"), None, 25),
            (
                "rare_issue",
                nil,
                Some("task"),
                None,
                Some("source_variants"),
                25,
            ),
            (
                "empty_held_tail",
                empty,
                Some("task"),
                Some("held"),
                None,
                0,
            ),
            (
                "empty_issue",
                nil,
                Some("task"),
                None,
                Some("not_observed"),
                0,
            ),
        ] {
            let prefix = if issue.is_some() {
                if name == "records" {
                    "SELECT a.id FROM migration_activity_manifest_issue x JOIN"
                } else {
                    "SELECT a.id FROM migration_activity_result_issue x JOIN"
                }
            } else {
                prefix
            };
            probe!(
                r,
                &mut conn,
                &format!("{name}_{suffix}"),
                QUERIES,
                prefix,
                max,
                s.plan,
                org,
                after,
                kind,
                disposition,
                issue,
                51_i64
            );
        }
    }
    for (label, kind) in [
        ("mapping_page", None),
        ("mapping_role_page", Some("note_author")),
        ("mapping_rare_kind", Some("task_kind")),
    ] {
        probe!(
            r,
            &mut conn,
            label,
            QUERIES,
            "SELECT id FROM migration_activity_mapping WHERE",
            51,
            s.plan,
            org,
            nil,
            kind,
            None::<&str>,
            None::<&str>,
            51_i64
        );
    }
    for (label,prefix,id) in [
        ("record_summary_lookup","SELECT * FROM migration_activity_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=",s.manifest),
        ("result_summary_lookup","SELECT * FROM migration_activity_result WHERE id=$1 AND plan_id=$2 AND organization_id=",s.result),
        ("mapping_summary_lookup","SELECT * FROM migration_activity_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=",s.mapping),
    ] {probe!(r,&mut conn,label,QUERIES,prefix,1,id,s.plan,org);}
    for (label, prefix, id) in [
        (
            "record_field_scope",
            "SELECT * FROM migration_activity_manifest WHERE id=$1 AND plan_id=$2 AND import_id=",
            s.manifest,
        ),
        (
            "mapping_field_scope",
            "SELECT * FROM migration_activity_mapping WHERE id=$1 AND plan_id=$2 AND import_id=",
            s.mapping,
        ),
        (
            "result_field_scope",
            "SELECT * FROM migration_activity_result WHERE id=$1 AND plan_id=$2 AND import_id=",
            s.result,
        ),
        (
            "observation_field_scope",
            "SELECT * FROM migration_activity_source WHERE id=$1 AND plan_id=$2 AND import_id=",
            s.source,
        ),
        (
            "observation_owner_scope",
            "SELECT source_id,kind,source_row_id FROM migration_activity_manifest",
            s.manifest,
        ),
    ] {
        probe!(r, &mut conn, label, QUERIES, prefix, 1, id, s.plan, s.child, org);
    }
    probe!(
        r,
        &mut conn,
        "observation_raw_capture",
        QUERIES,
        "SELECT nonce,ciphertext FROM migration_snapshot_capture",
        1,
        s.capture,
        s.f.snapshot,
        org
    );
    probe!(r,&mut conn,"observation_page",QUERIES,"SELECT id,capture_id,record_id,stream,representation,negative,source_id FROM migration_activity_source",51,s.plan,org,nil,"tasks","1024001",s.source,vec![0_u8;32],51_i64);
    for (label, sql, args, max) in [
        (
            "review_binding",
            review::REVIEW_BINDING_SQL,
            vec![org.into(), s.person.into()],
            1,
        ),
        (
            "review_revision",
            review::REVIEW_REVISION_SQL,
            vec![org.into()],
            1,
        ),
        (
            "review_counts",
            review::REVIEW_COUNTS_SQL,
            vec![org.into(), s.person.into()],
            1,
        ),
        (
            "review_note_full",
            review::NOTE_DETAIL_SQL,
            vec![org.into(), s.person.into(), s.note_after.into()],
            1,
        ),
        (
            "review_note_missing",
            review::NOTE_DETAIL_SQL,
            vec![org.into(), s.person.into(), nil.into()],
            0,
        ),
    ] {
        collect(r, &mut conn, label, sql, args, max).await;
    }
    for (name, sql, time, id) in [
        (
            "notes",
            review::NOTES_PAGE_SQL,
            Some(s.note_time),
            s.note_after,
        ),
        (
            "open_tasks",
            review::TASKS_OPEN_PAGE_SQL,
            s.task_time,
            s.task_after,
        ),
        (
            "completed_tasks",
            review::TASKS_COMPLETED_PAGE_SQL,
            Some(s.completed_time),
            s.completed_after,
        ),
    ] {
        for (suffix, time, id) in [("first", None, None), ("deep", time, Some(id))] {
            collect(
                r,
                &mut conn,
                &format!("review_{name}_{suffix}"),
                sql,
                vec![
                    org.into(),
                    s.person.into(),
                    Arg::Time(time),
                    Arg::NullableUuid(id),
                    51_i64.into(),
                ],
                51,
            )
            .await;
        }
    }
    // An undated cursor exercises the separate due_at=NULL continuation arm.
    let undated:Uuid=sqlx::query_scalar("SELECT id FROM task WHERE organization_id=$1 AND person_id=$2 AND completed_at IS NULL AND due_at IS NULL ORDER BY id OFFSET 50 LIMIT 1").bind(org).bind(s.person).fetch_one(&mut *conn).await.unwrap();
    collect(
        r,
        &mut conn,
        "review_open_undated_tail",
        review::TASKS_OPEN_PAGE_SQL,
        vec![
            org.into(),
            s.person.into(),
            Arg::Time(None),
            Some(undated).into(),
            51_i64.into(),
        ],
        51,
    )
    .await;
}

async fn workspace_plans(migrator: &PgPool, s: &Scale, r: &mut Report) {
    let operational = crate::common::create_org(migrator, "Synthetic operational guard").await;
    let mut conn = migrator.acquire().await.unwrap();
    let function_call = statement(WORKSPACE, "SELECT crm_activity_complete_read($1)");
    collect(
        r,
        &mut conn,
        "workspace_complete_activity_read_operational",
        &function_call,
        vec![operational.into()],
        1,
    )
    .await;
    let denied = sqlx::query(&function_call)
        .bind(s.f.org)
        .execute(&mut *conn)
        .await
        .unwrap_err();
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("P010F")
    );

    // EXPLAIN cannot expose a PL/pgSQL nested plan. Extract the exact deployed
    // EXISTS subquery, replacing only its PL/pgSQL parameter with a SQL bind.
    let function = SCHEMA
        .split_once("CREATE FUNCTION crm_activity_complete_read(org UUID)")
        .expect("actual boundary function")
        .1
        .split_once("END $$;")
        .unwrap()
        .0;
    let predicate = function
        .split_once("IF EXISTS(")
        .unwrap()
        .1
        .split_once(")\n THEN RAISE")
        .unwrap()
        .0;
    assert_eq!(predicate.matches("o.id=org").count(), 1);
    let boundary = format!(
        "SELECT EXISTS({})",
        predicate.replace("o.id=org", "o.id=$1")
    );
    for (name, org, expected) in [
        ("confirmed_review", s.f.org, true),
        ("operational", operational, false),
        ("missing", Uuid::nil(), false),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, bool>(&boundary)
                .bind(org)
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            expected
        );
        collect(
            r,
            &mut conn,
            &format!("workspace_activity_boundary_{name}"),
            &boundary,
            vec![org.into()],
            1,
        )
        .await;
    }
    let index: String = sqlx::query_scalar("SELECT indexdef FROM pg_indexes WHERE schemaname='public' AND indexname='migration_activity_import_boundary'")
        .fetch_one(&mut *conn).await.unwrap();
    assert!(index.contains("organization_id") && index.contains("confirmed_plan_id IS NOT NULL"));
    let startup = statement(WORKSPACE, "SELECT to_regclass('migration_workspace') IS NOT NULL AND to_regclass('migration_activity_import') IS NOT NULL");
    let startup_ready: bool = sqlx::query_scalar(&startup)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert!(startup_ready);
    r.workspace = json!({"actual_function_call":function_call,"review_denial_sqlstate":"P010F",
        "nested_predicate_source":predicate,"predicate_bind_substitution":"o.id=org -> o.id=$1 only",
        "boundary_index_definition":index,"startup_workload_check":{"sql":startup,"ready":startup_ready,
        "classification":"startup schema readiness, not a hot query"}});
    // Exercise the actual immutable projection INSERT, including its accounting
    // trigger and composite FK, without changing the traversal fixture.
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    probe!(
        r,
        &mut conn,
        "committed_result_issue_insert",
        WORKER,
        "INSERT INTO migration_activity_result_issue",
        0,
        s.result,
        s.plan,
        s.child,
        s.f.org,
        "mapping_target_changed"
    );
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
}

async fn native_traversal(s: &Scale, r: &mut Report) {
    let auth = AuthContext {
        actor_user_id: UserId::new(s.f.actor),
        actor_email: "plan@synthetic.test".into(),
        actor_display_name: "Synthetic admin".into(),
        active_organization_id: OrganizationId::new(s.f.org),
        active_organization_name: "Synthetic plan workspace".into(),
        role: Role::Admin,
    };
    let before = s.f.reader.calls();
    let person = PersonId::new(s.person);
    let mut evidence = vec![];
    for family in ["notes", "open_tasks", "completed_tasks"] {
        let mut cursor = None;
        let mut ids = BTreeSet::new();
        let mut pages = 0;
        let mut maximum_bytes = 0;
        loop {
            let q = PageQuery {
                limit: Some(50),
                cursor,
            };
            let page = if family == "notes" {
                review::notes(&s.f.pool, &s.f.key, &auth, person, &q)
                    .await
                    .unwrap()
            } else {
                review::tasks(
                    &s.f.pool,
                    &s.f.key,
                    &auth,
                    person,
                    if family == "open_tasks" {
                        TaskState::Open
                    } else {
                        TaskState::Completed
                    },
                    &q,
                )
                .await
                .unwrap()
            };
            let value = serde_json::to_value(&page).unwrap();
            let bytes = serde_json::to_vec(&page).unwrap().len();
            maximum_bytes = maximum_bytes.max(bytes);
            assert!(bytes <= review::PAGE_BYTES);
            let items = value["items"].as_array().unwrap();
            assert!(items.len() <= 50);
            for item in items {
                assert!(
                    ids.insert(item["id"].as_str().unwrap().to_owned()),
                    "duplicate native page item"
                );
                assert_eq!(item["can_manage"], false);
                if family == "notes" {
                    assert!(item.get("body").is_none());
                    assert!(item["excerpt"].as_str().unwrap().chars().count() <= 512);
                }
            }
            pages += 1;
            assert!(pages < 100);
            cursor = value["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        let expected = r.fixture["counts"][match family {
            "notes" => "dense_notes",
            "open_tasks" => "dense_open",
            _ => "dense_completed",
        }]
        .as_u64()
        .unwrap();
        assert_eq!(ids.len() as u64, expected);
        evidence.push(json!({"family":family,"pages":pages,"items":ids.len(),"expected_exact_items":expected,"maximum_response_bytes":maximum_bytes,"maximum_page_items":50,"duplicates":0}));
    }
    let full = review::note(&s.f.pool, &auth, person, s.note_after)
        .await
        .unwrap();
    assert_eq!(full["body"].as_str().unwrap().chars().count(), 10000);
    assert_eq!(s.f.reader.calls(), before);
    r.readers = json!({"scope":"actual_admin_native_review_queries_after_inert_retained_seed","pages":evidence,"full_note_characters":10000,"source_calls_added":0});
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_import_query_plans_and_complete_bounded_native_reads(migrator: PgPool) {
    let mut report = Report::default();
    let s = seed(&migrator, &mut report).await;
    hot_plans(&migrator, &s, &mut report).await;
    workspace_plans(&migrator, &s, &mut report).await;
    native_traversal(&s, &mut report).await;
    let sources = [
        ("activity.rs", COMMANDS),
        ("activity_store.rs", STORE),
        ("activity_worker.rs", WORKER),
        ("activity_queries.rs", QUERIES),
        ("activity_review.rs", REVIEW),
        ("auth/workspace.rs", WORKSPACE),
        ("20260920000001_fub_activity_import.sql", SCHEMA),
    ];
    let evidence = json!({"fixture":report.fixture,"readers":report.readers,"workspace":report.workspace,"source_sha256":sources.iter().map(|(name,source)|json!({"file":name,"sha256":sha(source)})).collect::<Vec<_>>(),"explain_plans":report.plans,"checks":{"no_temp_spills":true,"returned_row_bounds":true,"failures":report.failures},"limits":"query shape/native page evidence; no general capacity/latency or copied-source qualification claim"});
    if let Ok(path) = std::env::var("CRM_010F2_PLAN_OUTPUT") {
        let path = std::path::Path::new(&path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap()
        }
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        println!("CRM_010F2_PLAN_OUTPUT {}", path.display());
    } else {
        println!("CRM_010F2_ACTIVITY_PLANS {evidence}");
    }
    assert!(
        report.failures.is_empty(),
        "plan checks failed: {}",
        report.failures.join("; ")
    );
}

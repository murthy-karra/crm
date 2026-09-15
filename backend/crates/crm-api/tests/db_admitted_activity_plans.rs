//! F4-10 exact production SQL, single-pass plans. The real retained fixture
//! completes before inert ciphertext/cardinality scaling. No worker or decryptor
//! runs afterwards; bulk rows are query-plan evidence only, not fidelity/accounting.
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
const PEOPLE: i64 = 25_000;
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/admitted_activity_worker.rs");
const QUERIES: &str =
    include_str!("../../crm-app/src/domain/migration/admitted_activity_queries.rs");
const REMAINDER: &str =
    include_str!("../../crm-app/src/domain/migration/admitted_activity_remainder.rs");
const ACTIONS: &str = include_str!("../../crm-app/src/domain/migration/admitted_activity.rs");
const STORE: &str = include_str!("../../crm-app/src/domain/migration/admitted_activity_store.rs");
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

#[sqlx::test]
#[ignore = "opt-in F4-10 indexed query plans"]
async fn admitted_activity_hot_queries_25k_people_50_members(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_ADMITTED_ACTIVITY_PLANS_OUTPUT").expect("absolute output"),
    );
    assert!(
        output.is_absolute() && !output.exists(),
        "preserve evidence"
    );
    let (f, _, child, ready) = crate::db_admitted_activity::prepared(&migrator, 1).await;
    crate::db_admitted_activity::confirm_ready(&f, child, &ready).await;
    crate::db_admitted_activity::drain(&f).await;
    let plan = Uuid::parse_str(ready["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let admission = Uuid::parse_str(ready["admission_id"].as_str().unwrap()).unwrap();
    let snapshot: Uuid = sqlx::query_scalar(
        "SELECT snapshot_id FROM migration_admitted_activity_import WHERE id=$1",
    )
    .bind(child)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let mut conn = migrator.acquire().await.unwrap();
    let existing: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 ORDER BY id")
            .bind(f.org)
            .fetch_all(&mut *conn)
            .await
            .unwrap();
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
    sqlx::query("CREATE TEMP TABLE activity_rows AS SELECT n,CASE WHEN n<=cardinality($1::uuid[]) THEN ($1::uuid[])[n] ELSE gen_random_uuid() END AS person,gen_random_uuid() AS source,gen_random_uuid() AS record,gen_random_uuid() AS manifest,gen_random_uuid() AS result,gen_random_uuid() AS native,((n-1)/100+1)::integer AS batch,((n-1)%100)::integer AS ordinal,(1000000+n)::text AS source_id FROM generate_series(1,$2::bigint)n").bind(&existing).bind(PEOPLE).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT person,$1,'Synthetic','Activity '||n,$2,($3::uuid[])[1+((n-1)%50)] FROM activity_rows WHERE n>$4").bind(f.org).bind(f.lead_stage).bind(&members).bind(existing.len() as i64).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO note(id,organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,person,$2,left(repeat(md5(n::text),12),250),'migration',gen_random_uuid(),'2026-01-01'::timestamptz+n*interval '1 minute','2026-01-01'::timestamptz+n*interval '1 minute' FROM activity_rows").bind(f.org).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id,source,source_external_id,created_at,updated_at) SELECT native,$1,person,left(repeat('Synthetic source task '||n,30),499)||'x','call',CASE WHEN n%10<>0 THEN '2026-09-01'::timestamptz+n*interval '1 minute' END,$2,$3,'migration',gen_random_uuid(),'fub','v1:17:'||source_id,'2026-01-01','2026-01-01' FROM activity_rows").bind(f.org).bind(f.member).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,completed_at,assignee_user_id,created_by_user_id,origin,correlation_id,created_at,updated_at) SELECT gen_random_uuid(),$1,person,left(repeat('Synthetic completed task '||n,30),499)||'x','email','2026-09-01'::timestamptz+n*interval '1 minute',$2,$3,'migration',gen_random_uuid(),'2026-01-01','2026-01-01' FROM activity_rows").bind(f.org).bind(f.member).bind(f.actor).execute(&mut *conn).await.unwrap();
    // Inert 100-item pages maintain the observed capture and row-key shapes.
    sqlx::query("CREATE TEMP TABLE activity_captures AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,250)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,source_version,classification,truncated,accepted) SELECT h.id,c.snapshot_id,c.organization_id,'tasks_open',1000+h.n,100000+h.n,decode(repeat(md5(h.n::text),2),'hex'),c.representation,200,c.raw_byte_len,c.nonce,c.ciphertext,'inert-plan-only','inert_plan_only',false,true FROM activity_captures h CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='tasks_open' AND accepted LIMIT 1)c").bind(snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT a.record,s.snapshot_id,s.organization_id,c.id,1000+a.batch,a.ordinal,'tasks',a.source_id,s.representation,decode(repeat(md5(a.source_id),2),'hex'),s.projection_nonce,s.projection_ciphertext,false FROM activity_rows a JOIN activity_captures c ON c.n=a.batch CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='tasks' LIMIT 1)s").bind(snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,nonce,ciphertext,source_person_id) SELECT a.source,$1,$2,$3,$4,'tasks',a.source_id,a.record,c.id,1000+a.batch,a.ordinal,'tasks_open',s.representation,decode(repeat(md5(a.source_id),2),'hex'),s.nonce,s.ciphertext,'104' FROM activity_rows a JOIN activity_captures c ON c.n=a.batch CROSS JOIN LATERAL(SELECT * FROM migration_admitted_activity_source WHERE plan_id=$1 AND family='tasks' LIMIT 1)s").bind(plan).bind(child).bind(snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_manifest(id,plan_id,import_id,organization_id,kind,source_id,source_row_id,source_person_id,admission_result_id,person_id,target_id,native_source_key,creator_user_id,assignee_user_id,disposition,added_byte_bound,nonce,ciphertext) SELECT a.manifest,$1,$2,$3,'task',a.source_id,a.source,'101',s.admission_result_id,a.person,a.native,'v1:17:'||a.source_id,s.creator_user_id,s.assignee_user_id,CASE WHEN a.n%1000=0 THEN 'held' ELSE 'eligible' END,16384,s.nonce,s.ciphertext FROM activity_rows a CROSS JOIN LATERAL(SELECT * FROM migration_admitted_activity_manifest WHERE plan_id=$1 AND kind='task' LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_result(id,import_id,plan_id,organization_id,manifest_id,kind,source_id,person_id,target_id,disposition,nonce,ciphertext,actual_bytes) SELECT a.result,$1,$2,$3,a.manifest,'task',a.source_id,a.person,a.native,CASE WHEN a.n%1000=0 THEN 'held' ELSE 'applied' END,s.nonce,s.ciphertext,0 FROM activity_rows a CROSS JOIN LATERAL(SELECT * FROM migration_admitted_activity_result WHERE plan_id=$2 AND kind='task' LIMIT 1)s WHERE a.n%2=0").bind(child).bind(plan).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,admitted_import_id,admitted_plan_id,admitted_manifest_id) SELECT $1,17,'task',source_id,native,$2,$3,manifest FROM activity_rows").bind(f.org).bind(child).bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_manifest_issue(manifest_id,plan_id,import_id,organization_id,code) SELECT manifest,$1,$2,$3,'source_variants' FROM activity_rows WHERE n%1000=0").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_result_issue(result_id,plan_id,import_id,organization_id,code) SELECT result,$1,$2,$3,'source_variants' FROM activity_rows WHERE n%1000=0").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("CREATE TEMP TABLE activity_mappings AS SELECT n,gen_random_uuid() AS id,decode(repeat(md5('role-'||n),2),'hex') AS key,CASE n%3 WHEN 0 THEN 'note_author' WHEN 1 THEN 'task_creator' ELSE 'task_assignee' END AS kind FROM generate_series(1,150)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_mapping(id,plan_id,import_id,organization_id,kind,source_key,dependent_count,nonce,ciphertext) SELECT a.id,$1,$2,$3,a.kind,a.key,500,s.nonce,s.ciphertext FROM activity_mappings a CROSS JOIN LATERAL(SELECT * FROM migration_admitted_activity_mapping WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_activity_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),$1,$2,$3,a.kind,a.key,s.nonce,s.ciphertext FROM activity_mappings a CROSS JOIN LATERAL(SELECT * FROM migration_admitted_activity_choice WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_admitted_activity_source s SET source_only_counts=jsonb_build_object('unknown_properties_source_only',2) FROM activity_rows a WHERE s.id=a.source AND a.n%1000=0").execute(&mut *conn).await.unwrap();
    let mut counts = serde_json::Map::new();
    let mut storage = serde_json::Map::new();
    for table in [
        "person",
        "organization_membership",
        "note",
        "task",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_admitted_activity_import",
        "migration_admitted_activity_plan",
        "migration_admitted_activity_source",
        "migration_admitted_activity_manifest",
        "migration_admitted_activity_manifest_issue",
        "migration_admitted_activity_result",
        "migration_admitted_activity_result_issue",
        "migration_admitted_activity_mapping",
        "migration_activity_identity",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(f.org)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        counts.insert(table.into(), json!(n));
        let bytes: i64 = sqlx::query_scalar("SELECT pg_total_relation_size($1::regclass)")
            .bind(table)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        storage.insert(table.into(), json!(bytes));
    }
    assert_eq!(counts["person"], 25000);
    assert_eq!(counts["organization_membership"], 50);
    assert!(
        counts["migration_admitted_activity_source"]
            .as_i64()
            .unwrap()
            >= 25000
    );
    drop(conn);
    let mut conn = f.pool.acquire().await.unwrap();
    let org = f.org;
    let nil = Uuid::nil();
    let mut r = Report::default();
    probe!(
        &mut r,
        &mut conn,
        "remainder_availability",
        ACTIONS,
        "SELECT EXISTS(SELECT 1 FROM migration_admitted_activity_manifest m WHERE m.plan_id=$1",
        1,
        plan,
        org,
        child
    );
    probe!(&mut r,&mut conn,"selected_person_qualification",WORKER,"SELECT * FROM migration_admitted_activity_source WHERE plan_id=$1 AND organization_id=$2 AND family='people'",101,plan,org,"104");
    probe!(&mut r,&mut conn,"cohort_coverage",WORKER,"SELECT EXISTS(SELECT 1 FROM migration_admitted_activity_source s JOIN migration_people_admission_result ar",1,plan,org,"tasks","1024001",admission);
    probe!(
        &mut r,
        &mut conn,
        "claim",
        WORKER,
        "SELECT i.id,i.organization_id,i.executor_user_id",
        1
    );
    probe!(
        &mut r,
        &mut conn,
        "source_checkpoint",
        WORKER,
        "SELECT * FROM migration_admitted_activity_source a WHERE",
        1,
        plan,
        org,
        nil
    );
    probe!(
        &mut r,
        &mut conn,
        "source_variants",
        WORKER,
        "SELECT representation,count(DISTINCT semantic_hmac)",
        3,
        plan,
        org,
        "tasks",
        "1024001"
    );
    probe!(
        &mut r,
        &mut conn,
        "source_counters",
        WORKER,
        "WITH observations AS (SELECT DISTINCT ON",
        20,
        plan,
        org,
        "tasks",
        "1024001"
    );
    probe!(&mut r,&mut conn,"qualified_task",WORKER,"SELECT a.* FROM migration_admitted_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='tasks'",1,plan,org,"1024001");
    probe!(&mut r,&mut conn,"qualified_note",WORKER,"SELECT a.* FROM migration_admitted_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='notes'",1,plan,org,"11");
    probe!(
        &mut r,
        &mut conn,
        "global_identity",
        WORKER,
        "SELECT target_id FROM migration_activity_identity",
        1,
        org,
        17_i64,
        "task",
        "1024001"
    );
    probe!(
        &mut r,
        &mut conn,
        "manifest_checkpoint",
        WORKER,
        "SELECT * FROM migration_admitted_activity_manifest WHERE plan_id=$1",
        1,
        plan,
        org,
        nil
    );
    probe!(
        &mut r,
        &mut conn,
        "remainder_mapping",
        REMAINDER,
        "SELECT * FROM migration_admitted_activity_mapping WHERE",
        1,
        plan,
        org,
        nil
    );
    probe!(
        &mut r,
        &mut conn,
        "remainder_source",
        REMAINDER,
        "SELECT * FROM migration_admitted_activity_source WHERE",
        1,
        plan,
        org,
        nil
    );
    probe!(
        &mut r,
        &mut conn,
        "remainder_manifest",
        REMAINDER,
        "SELECT m.* FROM migration_admitted_activity_manifest m WHERE",
        1,
        plan,
        org,
        nil,
        child
    );
    for (label, prefix) in [
        (
            "records_page",
            "SELECT a.id FROM migration_admitted_activity_manifest a WHERE",
        ),
        (
            "results_page",
            "SELECT a.id FROM migration_admitted_activity_result a WHERE",
        ),
        (
            "mapping_page",
            "SELECT id FROM migration_admitted_activity_mapping WHERE",
        ),
    ] {
        probe!(
            &mut r,
            &mut conn,
            label,
            QUERIES,
            prefix,
            51,
            plan,
            org,
            nil,
            None::<&str>,
            None::<&str>,
            None::<&str>,
            51_i64
        );
    }
    for (label, prefix) in [
        (
            "records_issue",
            "SELECT a.id FROM migration_admitted_activity_manifest_issue x",
        ),
        (
            "results_issue",
            "SELECT a.id FROM migration_admitted_activity_result_issue x",
        ),
    ] {
        for issue in ["source_variants", "absent_code"] {
            probe!(
                &mut r,
                &mut conn,
                &format!("{label}_{issue}"),
                QUERIES,
                prefix,
                51,
                plan,
                org,
                nil,
                None::<&str>,
                None::<&str>,
                issue,
                51_i64
            );
        }
    }
    let source: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_admitted_activity_source WHERE plan_id=$1 ORDER BY id LIMIT 1",
    )
    .bind(plan)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    probe!(&mut r,&mut conn,"copied_source_identity",REMAINDER,"SELECT c.id FROM migration_admitted_activity_source a JOIN migration_admitted_activity_source c",1,plan,source,plan,org);
    // Reject broad scans on the large per-record relations; tiny root/catalog
    // relations may legitimately use sequential scans.
    for p in &r.plans {
        for scan in p["scans"].as_array().unwrap() {
            let name = scan["table"].as_str().unwrap();
            if counts.get(name).and_then(Value::as_i64).unwrap_or(0) > 1000
                && scan["node"] == "Seq Scan"
            {
                r.failures
                    .push(format!("{}: broad sequential scan of {name}", p["label"]));
            }
        }
    }
    let report = json!({"fixture":{"counts":counts,"relation_bytes":storage,"bulk_rows":"inert copied ciphertext; no post-scale worker, native equality, or storage-capacity claim"},"sources":{"worker":sha(WORKER),"queries":sha(QUERIES),"remainder":sha(REMAINDER),"store":sha(STORE),"actions":sha(ACTIONS)},"plans":r.plans,"failures":r.failures});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert!(
        report["failures"].as_array().unwrap().is_empty(),
        "{}",
        report["failures"]
    );
}

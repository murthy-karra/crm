//! D-050 query-shape evidence at 25,000 People / 50 members / 100,000 contacts.
//!
//! This is an opt-in SQLx disposable-database test, not a capacity benchmark.
//! The empty source book and import parents use the normal synthetic fixture.
//! All cardinality rows below are then inserted by the migrator and are INERT:
//! ciphertext placeholders, counters and results are NOT capture-fidelity,
//! executable-import, authorization, accounting or reconciliation evidence.
//! No import worker, decryption, provider or source request runs after seeding.
//! Raw/command correctness tests remain authoritative for those guarantees.
//!
//! Actual inline production SQL is decoded from its Rust source literal; static
//! People statements are include_str! of the current production SQL files.
//! Output records statement SHA-256 plus EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON).
//! Gates concern indexed/local relation work, bounded pages and absence of
//! external-sort/hash spill. Absolute timings are reported, never gated. This
//! does not measure serialization/decryption memory or shared workspace-guard
//! overhead. The separate paired HTTP test owns five concurrent Today loads.
//!
//! Primary runs serially with privately loaded isolated SQLx credentials:
//! cargo test -p crm-api --test all --locked db_import_plans::import_and_people_hot_queries_are_bounded_at_d050
//! -- --ignored --exact --nocapture --test-threads=1
use chrono::{DateTime, Utc};
use crm_api::auth::password;
use crm_api::domain::{
    admin::{queries as admin_queries, MembershipStatus, Role},
    person::filter::PersonFilterParams,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Acquire, PgConnection, PgPool, Row};
use std::time::Instant;
use uuid::Uuid;

const PEOPLE: i64 = 25_000;
const HISTORY: i64 = 256;
const BOUNDARY: i64 = 1252;
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/import_worker.rs");
const IMPORTS: &str = include_str!("../../crm-app/src/domain/migration/imports.rs");
const PERSON: &str = include_str!("../../crm-app/src/domain/person/queries.rs");

fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let start = source
        .find(&marker)
        .expect("actual production SQL must exist");
    serde_json::Deserializer::from_str(&source[start..])
        .into_iter::<String>()
        .next()
        .unwrap()
        .unwrap()
}

fn person_statement(function: &str) -> &'static str {
    PERSON
        .split(&format!("pub async fn {function}("))
        .nth(1)
        .expect("actual Person function must exist")
        .split("r#\"")
        .nth(1)
        .unwrap()
        .split("\"#,")
        .next()
        .unwrap()
}

fn nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(v) => {
            if v.contains_key("Node Type") {
                out.push(value);
            }
            for child in v.values() {
                nodes(child, out);
            }
        }
        Value::Array(v) => {
            for child in v {
                nodes(child, out);
            }
        }
        _ => {}
    }
}

fn emit(label: &str, sql: &str, plan: &Value) {
    println!(
        "CRM_010C_IMPORT_PLAN {}",
        json!({"label":label,"sql_sha256":Sha256::digest(sql.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect::<String>(),"plan":plan})
    );
    let mut all = vec![];
    nodes(plan, &mut all);
    for node in all {
        assert_ne!(node["Sort Space Type"], "Disk", "external sort: {label}");
        for metric in ["Temp Read Blocks", "Temp Written Blocks"] {
            assert_eq!(
                node[metric].as_u64().unwrap_or(0),
                0,
                "spill: {label} {node}"
            );
        }
        assert!(
            node["Hash Batches"].as_u64().unwrap_or(1) <= 1,
            "batched hash: {label} {node}"
        );
    }
}

fn indexed(plan: &Value, table: &str) {
    let mut all = vec![];
    nodes(plan, &mut all);
    let relation: Vec<_> = all.iter().filter(|n| n["Relation Name"] == table).collect();
    assert!(
        !relation.is_empty(),
        "missing measured relation {table}: {plan}"
    );
    for node in relation {
        let kind = node["Node Type"].as_str().unwrap();
        assert!(
            kind.contains("Index") || kind == "Bitmap Heap Scan",
            "unindexed {table}: {node}"
        );
    }
}

fn bounded(plan: &Value, table: &str, maximum: f64) {
    let mut all = vec![];
    nodes(plan, &mut all);
    let mut found = false;
    let examined: f64 = all
        .iter()
        .filter(|n| n["Relation Name"] == table)
        .map(|n| {
            found = true;
            (n["Actual Rows"].as_f64().unwrap_or(0.0)
                + n["Rows Removed by Filter"].as_f64().unwrap_or(0.0)
                + n["Rows Removed by Index Recheck"].as_f64().unwrap_or(0.0))
                * n["Actual Loops"].as_f64().unwrap_or(0.0)
        })
        .sum();
    assert!(found, "missing bounded relation {table}");
    assert!(
        examined <= maximum,
        "{table} examined {examined}, bound {maximum}: {plan}"
    );
}

fn returned(plan: &Value, expected: f64) {
    assert_eq!(
        plan[0]["Plan"]["Actual Rows"].as_f64(),
        Some(expected),
        "unexpected page cardinality: {plan}"
    );
}

/// Keep plan assertion failures local so one failure does not hide later query
/// evidence. SQL execution and fixture errors remain outside these catch scopes.
#[derive(Default)]
struct Checks {
    label: String,
    failures: Vec<String>,
    count: usize,
}

impl Checks {
    fn verify(&mut self, check: impl FnOnce()) {
        self.count += 1;
        if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
            let message = error
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| error.downcast_ref::<&str>().copied())
                .unwrap_or("query-plan assertion panicked");
            self.failures.push(format!("{}: {message}", self.label));
        }
    }

    fn plan(&mut self, label: &str, sql: &str, plan: &Value) {
        self.label = label.to_owned();
        self.verify(|| emit(label, sql, plan));
    }

    fn indexed(&mut self, plan: &Value, table: &str) {
        self.verify(|| indexed(plan, table));
    }

    fn bounded(&mut self, plan: &Value, table: &str, maximum: f64) {
        self.verify(|| bounded(plan, table, maximum));
    }

    fn returned(&mut self, plan: &Value, expected: f64) {
        self.verify(|| returned(plan, expected));
    }

    fn contacts(&mut self, plan: &Value, displayed: f64) {
        self.verify(|| contact_work(plan, displayed));
    }

    fn finish(self) {
        println!(
            "CRM_010C_IMPORT_PLAN_CHECKS {}",
            json!({"checks":self.count,"failures":self.failures.len()})
        );
        assert!(
            self.failures.is_empty(),
            "{} query-plan checks failed after collecting every plan:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}

macro_rules! explain {
    ($checks:expr, $conn:expr, $label:expr, $source:expr, $prefix:expr $(, $bind:expr)* $(,)?) => {{
        let sql = statement($source, $prefix);
        let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan: Value = sqlx::query_scalar(&wrapped)$(.bind($bind))*
            .fetch_one(&mut *$conn).await.unwrap();
        $checks.plan($label, &sql, &plan);
        plan
    }};
}

struct Scale {
    f: crate::import_support::Fixture,
    import_id: Uuid,
    plan_id: Uuid,
    person: Uuid,
    source: Uuid,
    manifest: Uuid,
    capture: Uuid,
    stage_mapping: Uuid,
    account: i64,
}

fn fixture_elapsed(phase: &str, started: Instant) {
    println!(
        "CRM_010C_IMPORT_FIXTURE {}",
        json!({"phase":phase,"elapsed_ms":started.elapsed().as_millis()})
    );
}

async fn seed(migrator: &PgPool) -> Scale {
    let started = Instant::now();
    let f = crate::import_support::fixture(migrator, vec![]).await;
    let (import_id, plan_id) = crate::import_support::propose(&f).await;
    fixture_elapsed("base_fixture", started);
    let started = Instant::now();
    let mut member_conn = f.pool.acquire().await.unwrap();
    // These 48 cardinality-only members never authenticate. Reuse one purely
    // synthetic hash and the same typed persistence helpers as common fixtures;
    // authentication tests and the real fixture's two logins are unchanged.
    let hash = password::hash_password("synthetic plan fixture password").unwrap();
    for n in 2..50 {
        let id = admin_queries::insert_app_user(
            &mut member_conn,
            &format!("plan-member-{n}@synthetic.test"),
            &format!("Synthetic member {n:02}"),
        )
        .await
        .unwrap();
        admin_queries::insert_local_credential(&mut member_conn, id, &hash)
            .await
            .unwrap();
        admin_queries::insert_membership(
            &mut member_conn,
            f.ctx.organization_id,
            id,
            Role::Member,
            MembershipStatus::Active,
        )
        .await
        .unwrap();
    }
    drop(member_conn);
    fixture_elapsed("additional_members", started);
    let started = Instant::now();
    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM organization_membership WHERE organization_id=$1 ORDER BY user_id",
    )
    .bind(f.org)
    .fetch_all(migrator)
    .await
    .unwrap();
    assert_eq!(members.len(), 50);
    let mut conn = migrator.acquire().await.unwrap();
    // The temporary IDs coordinate synthetic cardinality only. No business
    // command, worker or HTTP route reads these placeholder encrypted rows.
    sqlx::query("CREATE TEMP TABLE import_plan_rows AS SELECT n,gen_random_uuid() AS person,gen_random_uuid() AS record,gen_random_uuid() AS source,gen_random_uuid() AS manifest,gen_random_uuid() AS mapping,CASE WHEN n<=25000 THEN 'people' WHEN n<=25020 THEN 'stages' ELSE 'users' END AS family,CASE WHEN n<=25000 THEN (100000+n)::text WHEN n<=25020 THEN (200000+n-25000)::text ELSE (300000+n-25020)::text END AS source_id,CASE WHEN n<=25000 THEN ((n-1)/100)+1 WHEN n<=25020 THEN 251 ELSE 252 END AS batch,CASE WHEN n<=25000 THEN (n-1)%100 WHEN n<=25020 THEN n-25001 ELSE n-25021 END AS ordinal FROM generate_series(1,25070) n").execute(&mut *conn).await.unwrap();
    sqlx::query("CREATE TEMP TABLE import_plan_captures AS SELECT n AS batch,gen_random_uuid() AS id FROM generate_series(1,252) n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id,created_at) SELECT person,$1,CASE WHEN n%7=0 THEN NULL ELSE 'Synthetic' END,'Plan '||lpad(n::text,5,'0'),$2,($3::uuid[])[1+(n%50)],TIMESTAMPTZ '2026-09-01 12:00:00+00'+n*interval '1 second' FROM import_plan_rows WHERE family='people'").bind(f.org).bind(f.lead_stage).bind(&members).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,created_at,import_order) SELECT $1,r.person,v.kind,v.value,v.value,p.created_at+v.ord*interval '1 second',CASE WHEN r.n%2=0 THEN v.ord ELSE NULL END FROM import_plan_rows r JOIN person p ON p.id=r.person CROSS JOIN LATERAL (VALUES ('email','person-'||r.n||'@synthetic.test',0),('email','shared@synthetic.test',1),('phone','+12025550100',0),('phone','+1'||lpad((3000000000::bigint+r.n)::text,10,'0'),1)) v(kind,value,ord) WHERE r.family='people'").bind(f.org).execute(&mut *conn).await.unwrap();
    // 250 People pages of 100 records, plus one stage and one user page.
    // Placeholders are deliberately not authenticated source evidence.
    sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,classification,truncated,accepted) SELECT c.id,$1,$2,CASE WHEN batch<=250 THEN 'people' WHEN batch=251 THEN 'stages' ELSE 'users' END,1000+batch,1000+batch,decode(repeat(md5(batch::text),2),'hex'),'synthetic_non_executable_plan',200,32,decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex'),'synthetic_non_executable_plan',false,true FROM import_plan_captures c").bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT r.record,$1,$2,c.id,1000+r.batch,r.ordinal,r.family,r.source_id,'synthetic_non_executable_plan',decode(repeat(md5(r.source_id),2),'hex'),decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex'),false FROM import_plan_rows r JOIN import_plan_captures c ON c.batch=r.batch").bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    // These newly populated fixture relations need statistics before the bulk
    // contact-key join. This only prepares test data efficiently; the separate
    // final ANALYZE below prepares the actual production-query measurements.
    for table in ["import_plan_rows", "person", "contact_method"] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO migration_snapshot_contact_key(snapshot_id,organization_id,record_id,capture_sequence,source_id,kind,key_hmac) SELECT $1,$2,r.record,1000+r.batch,r.source_id,c.kind,decode(repeat(md5(c.kind||c.normalized_value),2),'hex') FROM import_plan_rows r JOIN contact_method c ON c.person_id=r.person WHERE r.family='people'").bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,ordinal,representation,semantic_hmac,qualified,nonce,ciphertext) SELECT r.source,$1,$2,$3,$4,r.family,r.source_id,r.record,c.id,r.ordinal,'synthetic_non_executable_plan',decode(repeat(md5(r.source_id),2),'hex'),true,decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex') FROM import_plan_rows r JOIN import_plan_captures c ON c.batch=r.batch").bind(plan_id).bind(import_id).bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_mapping(id,plan_id,import_id,organization_id,kind,source_key,qualified,disposition,target_id,nonce,ciphertext,source_sequence,source_ordinal,label_hmac,dependent_count,added_byte_bound) SELECT mapping,$1,$2,$3,CASE WHEN family='stages' THEN 'stage' ELSE 'assignee' END,CASE WHEN family='stages' THEN source_id ELSE 'user:'||source_id END,true,CASE WHEN family='stages' THEN 'create' ELSE 'member' END,CASE WHEN family='users' THEN ($4::uuid[])[n-25020] ELSE NULL END,decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex'),1000+batch,ordinal,CASE WHEN family='stages' THEN decode(repeat(md5(source_id),2),'hex') ELSE NULL END,500,16384 FROM import_plan_rows WHERE family<>'people'").bind(plan_id).bind(import_id).bind(f.org).bind(&members).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext FROM migration_import_mapping WHERE plan_id=$1").bind(plan_id).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,disposition,stage_mapping_id,assignee_mapping_id,contact_count,overlap_count,added_byte_bound,nonce,ciphertext) SELECT r.manifest,$1,$2,$3,r.source_id,r.source,CASE WHEN r.n%10=1 THEN 'held' ELSE 'eligible' END,s.mapping,u.mapping,4,2,16384,decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex') FROM import_plan_rows r JOIN import_plan_rows s ON s.n=25001+((r.n-1)%20) JOIN import_plan_rows u ON u.n=25021+((r.n-1)%50) WHERE r.family='people'").bind(plan_id).bind(import_id).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_result(id,import_id,plan_id,organization_id,manifest_id,source_id,person_id,disposition,contact_count,provenance_nonce,provenance_ciphertext) SELECT gen_random_uuid(),$1,$2,$3,manifest,source_id,CASE WHEN n%10=1 THEN NULL ELSE person END,CASE WHEN n%10=1 THEN 'held' ELSE 'imported' END,CASE WHEN n%10=1 THEN 0 ELSE 4 END,decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex') FROM import_plan_rows WHERE family='people' AND n%10<>0").bind(import_id).bind(plan_id).bind(f.org).execute(&mut *conn).await.unwrap();
    let account: i64 =
        sqlx::query_scalar("SELECT source_account_id FROM migration_import WHERE id=$1")
            .bind(import_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id,mapping_id,manifest_id) SELECT $1,$2,r.family,r.source_id,CASE WHEN r.family='people' THEN r.person ELSE $5 END,$3,$4,CASE WHEN r.family='stages' THEN r.mapping END,CASE WHEN r.family='people' THEN r.manifest END FROM import_plan_rows r WHERE r.family='stages' OR (r.family='people' AND r.n%10 NOT IN (0,1))").bind(f.org).bind(account).bind(import_id).bind(plan_id).bind(f.lead_stage).execute(&mut *conn).await.unwrap();
    // Modest historical plan cardinality makes tenant/plan-leading mapping,
    // choice and claim access observable without exceeding the People envelope.
    sqlx::query("CREATE TEMP TABLE import_plan_history AS SELECT n,gen_random_uuid() AS id,gen_random_uuid() AS plan FROM generate_series(1,$1) n").bind(HISTORY).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import(id,organization_id,snapshot_id,preview_id,source_account_id,capture_sequence,executor_user_id,state,created_at) SELECT h.id,i.organization_id,i.snapshot_id,i.preview_id,i.source_account_id,i.capture_sequence,i.executor_user_id,'completed',i.created_at-h.n*interval '1 minute' FROM import_plan_history h CROSS JOIN migration_import i WHERE i.id=$1").bind(import_id).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_plan(id,import_id,snapshot_id,organization_id,revision,state,patch_nonce,patch_ciphertext) SELECT h.plan,h.id,$1,$2,1,'ready',decode(repeat('00',12),'hex'),decode(repeat('00',64),'hex') FROM import_plan_history h").bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_import i SET latest_plan_id=h.plan FROM import_plan_history h WHERE i.id=h.id").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_mapping(id,plan_id,import_id,organization_id,kind,source_key,qualified,disposition,nonce,ciphertext) SELECT gen_random_uuid(),h.plan,h.id,m.organization_id,m.kind,m.source_key,m.qualified,'hold',m.nonce,m.ciphertext FROM import_plan_history h CROSS JOIN migration_import_mapping m WHERE m.plan_id=$1").bind(plan_id).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),h.plan,h.id,c.organization_id,c.kind,c.source_key,c.nonce,c.ciphertext FROM import_plan_history h CROSS JOIN migration_import_choice c WHERE c.plan_id=$1").bind(plan_id).execute(&mut *conn).await.unwrap();
    let r = sqlx::query("SELECT r.person,r.source,r.manifest,c.id AS capture FROM import_plan_rows r JOIN import_plan_captures c ON c.batch=r.batch WHERE r.n=24002").fetch_one(&mut *conn).await.unwrap();
    let stage_mapping: Uuid =
        sqlx::query_scalar("SELECT mapping FROM import_plan_rows WHERE n=25002")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    fixture_elapsed("cardinality_rows", started);
    let started = Instant::now();
    for table in [
        "person",
        "contact_method",
        "app_user",
        "organization_membership",
        "migration_import",
        "migration_import_plan",
        "migration_import_source",
        "migration_import_choice",
        "migration_import_mapping",
        "migration_import_manifest",
        "migration_import_identity",
        "migration_import_result",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_snapshot_contact_key",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    fixture_elapsed("analyze", started);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&mut *conn)
            .await
            .unwrap(),
        PEOPLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM contact_method WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&mut *conn)
        .await
        .unwrap(),
        PEOPLE * 4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_snapshot_contact_key WHERE snapshot_id=$1"
        )
        .bind(f.snapshot)
        .fetch_one(&mut *conn)
        .await
        .unwrap(),
        PEOPLE * 4
    );
    Scale {
        f,
        import_id,
        plan_id,
        person: r.get("person"),
        source: r.get("source"),
        manifest: r.get("manifest"),
        capture: r.get("capture"),
        stage_mapping,
        account,
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_and_people_hot_queries_are_bounded_at_d050(migrator: PgPool) {
    let s = seed(&migrator).await;
    let mut conn = s.f.pool.acquire().await.unwrap();
    let mut checks = Checks::default();
    // Actual worker candidate and immutable run history. FOR UPDATE probes below
    // run as crm_app and release at each autocommit; no execution is dispatched.
    let p = explain!(
        checks,
        conn,
        "claim",
        WORKER,
        "SELECT i.id,i.organization_id FROM migration_import i JOIN migration_import_plan p"
    );
    checks.indexed(&p, "migration_import");
    checks.bounded(&p, "migration_import", 2.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "import_history_page",
        IMPORTS,
        "SELECT id,created_at FROM migration_import WHERE",
        s.f.org,
        Utc::now(),
        Uuid::from_u128(u128::MAX),
        51_i64
    );
    checks.indexed(&p, "migration_import");
    checks.bounded(&p, "migration_import", 51.0);
    checks.returned(&p, 51.0);
    let p = explain!(
        checks,
        conn,
        "import_lock",
        IMPORTS,
        "SELECT * FROM migration_import WHERE id=$1 AND organization_id=$2 FOR UPDATE",
        s.import_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import");
    checks.bounded(&p, "migration_import", 1.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "plan_lock",
        IMPORTS,
        "SELECT * FROM migration_import_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE",
        s.plan_id,
        s.import_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_plan");
    checks.bounded(&p, "migration_import_plan", 1.0);
    let p = explain!(
        checks,
        conn,
        "copy_choice_page",
        WORKER,
        "SELECT id,kind,source_key,nonce,ciphertext FROM migration_import_choice WHERE",
        s.plan_id,
        s.f.org,
        "assignee",
        "user:300020"
    );
    checks.indexed(&p, "migration_import_choice");
    checks.bounded(&p, "migration_import_choice", 50.0);
    checks.returned(&p, 50.0);
    let p = explain!(
        checks,
        conn,
        "choice_point",
        WORKER,
        "SELECT id,nonce,ciphertext FROM migration_import_choice WHERE",
        s.plan_id,
        s.f.org,
        "stage",
        "200002"
    );
    checks.indexed(&p, "migration_import_choice");
    checks.bounded(&p, "migration_import_choice", 1.0);
    let p = explain!(
        checks,
        conn,
        "capture_checkpoint",
        WORKER,
        "SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>",
        s.f.snapshot,
        s.f.org,
        1239_i64,
        BOUNDARY
    );
    checks.indexed(&p, "migration_snapshot_capture");
    checks.bounded(&p, "migration_snapshot_capture", 1.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "capture_payload_point",
        WORKER,
        "SELECT * FROM migration_snapshot_capture WHERE id=$1",
        s.capture,
        s.f.snapshot,
        s.f.org
    );
    checks.indexed(&p, "migration_snapshot_capture");
    checks.bounded(&p, "migration_snapshot_capture", 1.0);
    let p = explain!(
        checks,
        conn,
        "capture_records",
        WORKER,
        "SELECT id,source_id,ordinal,family,representation,semantic_hmac FROM migration_snapshot_record WHERE",
        s.capture,
        s.f.snapshot,
        s.f.org
    );
    checks.indexed(&p, "migration_snapshot_record");
    checks.bounded(&p, "migration_snapshot_record", 100.0);
    checks.returned(&p, 100.0);
    let p = explain!(
        checks,
        conn,
        "qualified_source_map",
        WORKER,
        "SELECT id,qualified,conflict,semantic_hmac,nonce,ciphertext FROM migration_import_source WHERE",
        s.plan_id,
        s.f.org,
        "people",
        "124002"
    );
    checks.indexed(&p, "migration_import_source");
    checks.bounded(&p, "migration_import_source", 1.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "supporting_source_checkpoint",
        WORKER,
        "SELECT s.id FROM migration_import_source s WHERE",
        s.plan_id,
        s.f.org,
        "stages",
        "200019"
    );
    checks.indexed(&p, "migration_import_source");
    checks.bounded(&p, "migration_import_source", 2.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "source_capture_point",
        WORKER,
        "SELECT s.*,c.sequence FROM migration_import_source s JOIN migration_snapshot_capture c",
        s.source,
        s.plan_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_source");
    checks.bounded(&p, "migration_import_source", 1.0);
    checks.indexed(&p, "migration_snapshot_capture");
    let p = explain!(
        checks,
        conn,
        "people_preparation_checkpoint",
        WORKER,
        "SELECT id FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people'",
        s.plan_id,
        s.f.org,
        "124000"
    );
    checks.indexed(&p, "migration_import_source");
    checks.bounded(&p, "migration_import_source", 1.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "mapping_point",
        WORKER,
        "SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4",
        s.plan_id,
        s.f.org,
        "stage",
        "200002"
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 1.0);
    let p = explain!(
        checks,
        conn,
        "confirmation_mapping_page",
        IMPORTS,
        "SELECT id,kind,disposition,target_id FROM migration_import_mapping WHERE",
        s.plan_id,
        s.f.org,
        Uuid::nil()
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 70.0);
    checks.returned(&p, 50.0);
    let p = explain!(
        checks,
        conn,
        "native_stage_match",
        WORKER,
        "SELECT id,name,position FROM stage WHERE organization_id=$1 AND name=$2",
        s.f.org,
        "Lead"
    );
    // Native catalogs are intentionally small (seeded stages / 50 members),
    // so an equivalent bounded catalog scan is valid at this envelope.
    checks.bounded(&p, "stage", 20.0);
    checks.returned(&p, 1.0);
    let member_email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(s.f.actor)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let p = explain!(
        checks,
        conn,
        "native_member_email_match",
        WORKER,
        "SELECT u.id,u.email,u.display_name FROM app_user u JOIN organization_membership m ON m.user_id=u.id WHERE m.organization_id=$1 AND m.status='active' AND lower(u.email)=lower($2)",
        s.f.org,
        &member_email
    );
    checks.bounded(&p, "organization_membership", 50.0);
    // The fixture platform admin is deliberately not an Organization member.
    checks.bounded(&p, "app_user", 51.0); // 50 members + one platform fixture actor.
    checks.returned(&p, 1.0);
    let label_hmac: Vec<u8> =
        sqlx::query_scalar("SELECT label_hmac FROM migration_import_mapping WHERE id=$1")
            .bind(s.stage_mapping)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let p = explain!(
        checks,
        conn,
        "stage_label_mapping",
        WORKER,
        "SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='stage' AND label_hmac=$3",
        s.plan_id,
        s.f.org,
        &label_hmac
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 1.0);
    let p = explain!(
        checks,
        conn,
        "assignee_mapping",
        WORKER,
        "SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='assignee' AND source_key=$3",
        s.plan_id,
        s.f.org,
        "user:300002"
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 1.0);
    let p = explain!(
        checks,
        conn,
        "dense_overlap_exists",
        WORKER,
        "SELECT count(*) FROM (SELECT DISTINCT kind,key_hmac FROM migration_snapshot_contact_key",
        s.f.snapshot,
        s.f.org,
        "124002",
        BOUNDARY
    );
    checks.indexed(&p, "migration_snapshot_contact_key");
    checks.bounded(&p, "migration_snapshot_contact_key", 32.0);
    checks.returned(&p, 1.0);
    // Equal shared phones do not create 25,000 x 25,000 stored pairs.
    let overlap_sql = statement(
        WORKER,
        "SELECT count(*) FROM (SELECT DISTINCT kind,key_hmac FROM migration_snapshot_contact_key",
    );
    let overlap: i64 = sqlx::query_scalar(&overlap_sql)
        .bind(s.f.snapshot)
        .bind(s.f.org)
        .bind("124002")
        .bind(BOUNDARY)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    checks.verify(|| assert_eq!(overlap, 2));
    let p = explain!(
        checks,
        conn,
        "stage_execution_checkpoint",
        WORKER,
        "SELECT id,added_byte_bound FROM migration_import_mapping WHERE",
        s.plan_id,
        s.f.org,
        1251_i64,
        0_i32,
        "200001"
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 2.0);
    checks.indexed(&p, "migration_import_manifest");
    checks.bounded(&p, "migration_import_manifest", 2.0);
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "people_execution_checkpoint",
        WORKER,
        "SELECT id,disposition,added_byte_bound FROM migration_import_manifest WHERE",
        s.plan_id,
        s.f.org,
        "124000"
    );
    checks.indexed(&p, "migration_import_manifest");
    checks.bounded(&p, "migration_import_manifest", 1.0);
    checks.returned(&p, 1.0);
    for (family, prefix, id) in [
        (
            "stages",
            "SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='stages'",
            "200002",
        ),
        (
            "people",
            "SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='people'",
            "124002",
        ),
    ] {
        let p = explain!(
            checks,
            conn,
            &format!("identity_{family}"),
            WORKER,
            prefix,
            s.f.org,
            s.account,
            id
        );
        checks.indexed(&p, "migration_import_identity");
        checks.bounded(&p, "migration_import_identity", 1.0);
        checks.returned(&p, 1.0);
    }
    // Both first and deep keyset pages use no offset pagination and do no work
    // proportional to preceding People. The populated-result bound depends on
    // held/pending occurring once per ten in this mixed fixture. Constant OFFSET
    // 0 inside the correlated result lookup preserves all matches, not row skips.
    for after in ["", "124000"] {
        for disposition in [None, Some("eligible"), Some("held")] {
            let p = explain!(
                checks,
                conn,
                &format!("record_page_{after}_{disposition:?}"),
                IMPORTS,
                "SELECT id,source_id FROM migration_import_manifest WHERE",
                s.plan_id,
                s.f.org,
                after,
                disposition,
                51_i64
            );
            checks.indexed(&p, "migration_import_manifest");
            // Either the disposition index or an ordered source-ID index is
            // valid. The latter may filter the held row in each ten records:
            // 51 unfiltered, at most 60 eligible, or 510 held-row candidates.
            let examined = match disposition {
                None => 51.0,
                Some("eligible") => 60.0,
                Some("held") => 510.0,
                _ => unreachable!(),
            };
            checks.bounded(&p, "migration_import_manifest", examined);
            checks.returned(&p, 51.0);
        }
        for disposition in [None, Some("imported"), Some("held"), Some("pending")] {
            let p = explain!(
                checks,
                conn,
                &format!("result_page_{after}_{disposition:?}"),
                IMPORTS,
                "SELECT m.id,m.source_id,m.disposition AS planned,r.disposition,r.person_id,r.contact_count,r.committed_at FROM migration_import_manifest m LEFT JOIN LATERAL",
                s.plan_id,
                s.f.org,
                after,
                disposition,
                51_i64
            );
            checks.indexed(&p, "migration_import_manifest");
            checks.indexed(&p, "migration_import_result");
            checks.bounded(&p, "migration_import_manifest", 600.0);
            checks.bounded(&p, "migration_import_result", 600.0);
            checks.returned(&p, 51.0);
        }
    }
    // No source was already imported in this fixture. Finding an empty result
    // disposition must examine the remaining keyset tail, unlike populated
    // filters above. Record that linear work explicitly without claiming the
    // mixed-density 600-row bound for all result filters.
    for (after, remaining) in [("", PEOPLE as f64), ("124000", 1000.0)] {
        let p = explain!(
            checks,
            conn,
            &format!("result_empty_page_{after}_already_imported"),
            IMPORTS,
            "SELECT m.id,m.source_id,m.disposition AS planned,r.disposition,r.person_id,r.contact_count,r.committed_at FROM migration_import_manifest m LEFT JOIN LATERAL",
            s.plan_id,
            s.f.org,
            after,
            Some("already_imported"),
            51_i64
        );
        checks.indexed(&p, "migration_import_manifest");
        checks.indexed(&p, "migration_import_result");
        checks.bounded(&p, "migration_import_manifest", remaining);
        checks.bounded(&p, "migration_import_result", remaining);
        checks.returned(&p, 0.0);
    }
    for (kind, after, expected) in [("stage", "200005", 15.0), ("assignee", "user:300005", 45.0)] {
        let p = explain!(
            checks,
            conn,
            &format!("mapping_page_{kind}"),
            IMPORTS,
            "SELECT id,source_key FROM migration_import_mapping WHERE",
            s.plan_id,
            s.f.org,
            kind,
            after,
            51_i64
        );
        checks.indexed(&p, "migration_import_mapping");
        checks.bounded(&p, "migration_import_mapping", expected);
        checks.returned(&p, expected);
    }
    let p = explain!(
        checks,
        conn,
        "mapping_payload_point",
        IMPORTS,
        "SELECT * FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
        s.stage_mapping,
        s.plan_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 1.0);
    let p = explain!(
        checks,
        conn,
        "mapping_field_payload_point",
        IMPORTS,
        "SELECT nonce,ciphertext FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4",
        s.stage_mapping,
        s.plan_id,
        s.import_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 1.0);
    let p = explain!(
        checks,
        conn,
        "manifest_payload_point",
        IMPORTS,
        "SELECT * FROM migration_import_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
        s.manifest,
        s.plan_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_manifest");
    checks.bounded(&p, "migration_import_manifest", 1.0);
    let p = explain!(
        checks,
        conn,
        "source_payload_point",
        IMPORTS,
        "SELECT * FROM migration_import_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
        s.source,
        s.plan_id,
        s.f.org
    );
    checks.indexed(&p, "migration_import_source");
    checks.bounded(&p, "migration_import_source", 1.0);
    let p = explain!(
        checks,
        conn,
        "full_field_page_source",
        IMPORTS,
        "SELECT s.* FROM migration_import_manifest m JOIN migration_import_source s",
        s.manifest,
        s.plan_id,
        s.import_id,
        s.f.org
    );
    for table in ["migration_import_manifest", "migration_import_source"] {
        checks.indexed(&p, table);
        checks.bounded(&p, table, 1.0);
    }
    checks.returned(&p, 1.0);
    let p = explain!(
        checks,
        conn,
        "person_provenance",
        IMPORTS,
        "SELECT r.*,i.snapshot_id,s.record_id,s.capture_id FROM migration_import_result r",
        s.f.org,
        s.person
    );
    for table in [
        "migration_import_result",
        "migration_import",
        "migration_import_manifest",
        "migration_import_source",
        "person",
    ] {
        checks.indexed(&p, table);
        checks.bounded(&p, table, 1.0);
    }
    checks.returned(&p, 1.0);
    // This is the real preparation-completion aggregate, measured inside a
    // rollback-only transaction: it must not turn the inert fixture into work.
    let mut tx = conn.begin().await.unwrap();
    let p = explain!(
        checks,
        tx,
        "ready_stage_dependency_count",
        WORKER,
        "UPDATE migration_import_plan SET phase='ready',state='ready',confirmation_digest=$3",
        s.plan_id,
        s.f.org,
        vec![0_u8; 32]
    );
    checks.indexed(&p, "migration_import_mapping");
    checks.bounded(&p, "migration_import_mapping", 20.0);
    checks.indexed(&p, "migration_import_manifest");
    checks.bounded(&p, "migration_import_manifest", 20.0);
    tx.rollback().await.unwrap();
    people_plans(&mut conn, &s, &mut checks).await;
    checks.finish();
}

async fn people_plans(conn: &mut PgConnection, s: &Scale, checks: &mut Checks) {
    let sql = person_statement("search_summaries");
    let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    let p: Value = sqlx::query_scalar(&wrapped)
        .bind(s.f.org)
        .bind("%+12025550100%")
        .bind(None::<&str>)
        .bind(Some("+12025550100"))
        .bind(51_i64)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    checks.plan("search_shared_phone", sql, &p);
    checks.contacts(&p, 51.0);
    checks.returned(&p, 51.0);
    let sql = person_statement("contact_methods_for_person");
    let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    let p: Value = sqlx::query_scalar(&wrapped)
        .bind(s.f.org)
        .bind(s.person)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    checks.plan("person_contacts", sql, &p);
    checks.indexed(&p, "contact_method");
    checks.bounded(&p, "contact_method", 4.0);
    checks.returned(&p, 4.0);
    // Every changed sort dispatch uses the exact production SQL and its full
    // 72-parameter contract. Stage + has_phone is a real broad People filter.
    let params = PersonFilterParams {
        stage_ids: Some(vec![s.f.lead_stage]),
        has_phone: Some(true),
        viewer_id: s.f.actor,
        ..Default::default()
    };
    let now = Utc::now();
    for (name, sql) in [
        (
            "created_desc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries.sql"),
        ),
        (
            "created_asc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_created_asc.sql"),
        ),
        (
            "name_asc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_name_asc.sql"),
        ),
        (
            "name_desc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_name_desc.sql"),
        ),
        (
            "stage_asc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_stage_asc.sql"),
        ),
        (
            "stage_desc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_stage_desc.sql"),
        ),
        (
            "assignee_asc",
            include_str!("../../crm-app/src/domain/person/sql/filtered_summaries_assignee_asc.sql"),
        ),
        (
            "assignee_desc",
            include_str!(
                "../../crm-app/src/domain/person/sql/filtered_summaries_assignee_desc.sql"
            ),
        ),
    ] {
        let p = filtered_plan(conn, sql, s.f.org, &params, Some(now)).await;
        checks.plan(&format!("filtered_people_{name}"), sql, &p);
        checks.returned(&p, 501.0);
        checks.contacts(&p, 501.0);
        checks.bounded(&p, "person", PEOPLE as f64);
    }
}

fn contact_work(plan: &Value, displayed: f64) {
    let mut all = vec![];
    nodes(plan, &mut all);
    let mut projections = 0;
    for node in all {
        if node["Relation Name"] != "contact_method" {
            continue;
        }
        // cm/cm_1 are the changed primary projections. The independent cm2/3/4
        // membership probes may legitimately hash one bounded 100k contact scan.
        let alias = node["Alias"].as_str().unwrap_or("");
        if alias == "cm" || alias.starts_with("cm_") {
            projections += 1;
            let kind = node["Node Type"].as_str().unwrap();
            assert!(
                kind.contains("Index") || kind == "Bitmap Heap Scan",
                "unindexed contact projection: {node}"
            );
            assert!(
                node["Actual Rows"].as_f64().unwrap_or(0.0)
                    + node["Rows Removed by Filter"].as_f64().unwrap_or(0.0)
                    <= 4.0,
                "non-local contact projection: {node}"
            );
            assert!(
                node["Actual Loops"].as_f64().unwrap_or(0.0) <= displayed,
                "contact projection before bounded page: {node}"
            );
        } else {
            let rows = node["Actual Rows"].as_f64().unwrap_or(0.0)
                + node["Rows Removed by Filter"].as_f64().unwrap_or(0.0);
            assert!(
                rows * node["Actual Loops"].as_f64().unwrap_or(0.0) <= PEOPLE as f64 * 4.0,
                "superlinear contact membership: {node}"
            );
        }
    }
    assert!(
        projections >= 2,
        "both primary projections must actually execute: {plan}"
    );
}

async fn filtered_plan(
    conn: &mut PgConnection,
    sql: &str,
    organization_id: Uuid,
    params: &PersonFilterParams,
    reference_now: Option<DateTime<Utc>>,
) -> Value {
    let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    sqlx::query_scalar(&wrapped)
        .bind(organization_id)
        .bind(params.stage_ids.as_deref())
        .bind(params.assigned_user_ids.as_deref())
        .bind(params.assigned_include_unassigned)
        .bind(params.sources.as_deref())
        .bind(params.created_within_days)
        .bind(params.created_not_within_days)
        .bind(params.created_never)
        .bind(params.last_inquiry_within_days)
        .bind(params.last_inquiry_not_within_days)
        .bind(params.last_inquiry_never)
        .bind(params.last_contact_within_days)
        .bind(params.last_contact_not_within_days)
        .bind(params.last_contact_never)
        .bind(params.last_inbound_within_days)
        .bind(params.last_inbound_not_within_days)
        .bind(params.last_inbound_never)
        .bind(params.has_replied)
        .bind(params.has_phone)
        .bind(params.has_email)
        .bind(reference_now)
        .bind(params.awaiting_response)
        .bind(params.client_replied_unanswered)
        .bind(params.awaiting_call_outcome)
        .bind(params.viewer_id)
        .bind(params.tag_ids_any.as_deref())
        .bind(params.tag_ids_none.as_deref())
        .bind(params.custom_slot(0).map(|s| s.field_id))
        .bind(params.custom_slot(0).map(|s| s.field_type))
        .bind(params.custom_slot(0).map(|s| s.operation))
        .bind(
            params
                .custom_slot(0)
                .and_then(|s| s.text_operand.as_deref()),
        )
        .bind(params.custom_slot(0).and_then(|s| s.number_min.as_deref()))
        .bind(params.custom_slot(0).and_then(|s| s.number_max.as_deref()))
        .bind(params.custom_slot(0).and_then(|s| s.date_min))
        .bind(params.custom_slot(0).and_then(|s| s.date_max))
        .bind(params.custom_slot(0).and_then(|s| s.option_ids.as_deref()))
        .bind(params.custom_slot(1).map(|s| s.field_id))
        .bind(params.custom_slot(1).map(|s| s.field_type))
        .bind(params.custom_slot(1).map(|s| s.operation))
        .bind(
            params
                .custom_slot(1)
                .and_then(|s| s.text_operand.as_deref()),
        )
        .bind(params.custom_slot(1).and_then(|s| s.number_min.as_deref()))
        .bind(params.custom_slot(1).and_then(|s| s.number_max.as_deref()))
        .bind(params.custom_slot(1).and_then(|s| s.date_min))
        .bind(params.custom_slot(1).and_then(|s| s.date_max))
        .bind(params.custom_slot(1).and_then(|s| s.option_ids.as_deref()))
        .bind(params.custom_slot(2).map(|s| s.field_id))
        .bind(params.custom_slot(2).map(|s| s.field_type))
        .bind(params.custom_slot(2).map(|s| s.operation))
        .bind(
            params
                .custom_slot(2)
                .and_then(|s| s.text_operand.as_deref()),
        )
        .bind(params.custom_slot(2).and_then(|s| s.number_min.as_deref()))
        .bind(params.custom_slot(2).and_then(|s| s.number_max.as_deref()))
        .bind(params.custom_slot(2).and_then(|s| s.date_min))
        .bind(params.custom_slot(2).and_then(|s| s.date_max))
        .bind(params.custom_slot(2).and_then(|s| s.option_ids.as_deref()))
        .bind(params.custom_slot(3).map(|s| s.field_id))
        .bind(params.custom_slot(3).map(|s| s.field_type))
        .bind(params.custom_slot(3).map(|s| s.operation))
        .bind(
            params
                .custom_slot(3)
                .and_then(|s| s.text_operand.as_deref()),
        )
        .bind(params.custom_slot(3).and_then(|s| s.number_min.as_deref()))
        .bind(params.custom_slot(3).and_then(|s| s.number_max.as_deref()))
        .bind(params.custom_slot(3).and_then(|s| s.date_min))
        .bind(params.custom_slot(3).and_then(|s| s.date_max))
        .bind(params.custom_slot(3).and_then(|s| s.option_ids.as_deref()))
        .bind(params.custom_slot(4).map(|s| s.field_id))
        .bind(params.custom_slot(4).map(|s| s.field_type))
        .bind(params.custom_slot(4).map(|s| s.operation))
        .bind(
            params
                .custom_slot(4)
                .and_then(|s| s.text_operand.as_deref()),
        )
        .bind(params.custom_slot(4).and_then(|s| s.number_min.as_deref()))
        .bind(params.custom_slot(4).and_then(|s| s.number_max.as_deref()))
        .bind(params.custom_slot(4).and_then(|s| s.date_min))
        .bind(params.custom_slot(4).and_then(|s| s.date_max))
        .bind(params.custom_slot(4).and_then(|s| s.option_ids.as_deref()))
        .fetch_one(&mut *conn)
        .await
        .unwrap()
}

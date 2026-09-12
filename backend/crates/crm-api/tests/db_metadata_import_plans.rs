//! D-050 SQL-plan collector: 25,000 People, 50 members, 250,000 tag links,
//! 100,000 native values and 350,000 frozen operation descriptors.
//!
//! Opt-in, disposable SQLx database; this is query-shape evidence, not a capacity
//! benchmark. The small parent completes through the real import commands and
//! its child reaches a ready plan. All subsequent bulk rows are INERT migrator
//! fixtures: copied ciphertext is deliberately not rebound to new AAD. No worker,
//! decryptor or source request runs afterward. These rows are not qualification,
//! fidelity, authorization, accounting, reconciliation or executable-plan proof.
//! The real ready child's retained-byte ledger is emitted before bulk seeding.
//! Copied ciphertext cardinalities and physical sizes are not accounting or
//! storage-fidelity proof. Native values use a deterministic 150–500-character
//! spread (except one short exact-match probe), representing moderate/dense
//! synthetic widths, not maximum storage capacity or a worst-case source book.
//! Relation sizes cover whole relations in the isolated database, including
//! fixture/history rows; pg_column_size summaries describe stored datum widths.
//!
//! Every measured statement is extracted from the production Rust literal, or
//! the migration's actual operation-permit expression. Evidence includes exact
//! source SQL, its SHA-256, bindings and EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON).
//! No planner settings or absolute latency gates are used. Sparse filters are
//! bounded by their actual remaining keyset tail, not by the returned page size.
//! Native per-Person probes remain independently indexed and bounded. This does
//! not benchmark full CRM readers, decryption, response sizing or concurrency.
use crate::import_support::Fixture;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::time::Instant;
use uuid::Uuid;

const PEOPLE: i64 = 25_000;
const HISTORY: i64 = 256;
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/metadata_worker.rs");
const QUERIES: &str = include_str!("../../crm-app/src/domain/migration/metadata_queries.rs");
const COMMANDS: &str = include_str!("../../crm-app/src/domain/migration/metadata.rs");
const STORE: &str = include_str!("../../crm-app/src/domain/migration/metadata_store.rs");
const SCHEMA: &str = include_str!("../migrations/20260919000001_fub_metadata_import.sql");

fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let mut found = source.match_indices(&marker).map(|(start, _)| {
        serde_json::Deserializer::from_str(&source[start..])
            .into_iter::<String>()
            .next()
            .expect("production SQL literal")
            .expect("production SQL literal must decode exactly")
    });
    let sql = found.next().expect("production statement must exist");
    assert!(
        found.all(|other| other == sql),
        "ambiguous SQL prefix {prefix}"
    );
    sql
}

fn sha(sql: &str) -> String {
    Sha256::digest(sql.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

#[derive(Default)]
struct Checks {
    label: String,
    failures: Vec<String>,
    count: usize,
}
impl Checks {
    fn verify(&mut self, check: impl FnOnce()) {
        self.count += 1;
        if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
            let message = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("plan assertion failed");
            self.failures.push(format!("{}: {message}", self.label));
        }
    }
    fn plan(&mut self, label: &str, sql: &str, bindings: Value, plan: &Value) {
        self.label = label.to_owned();
        println!(
            "CRM_010F1_METADATA_PLAN {}",
            json!({"label":label,"sql":sql,"sql_sha256":sha(sql),"bindings":bindings,"plan":plan})
        );
        self.verify(|| {
            let mut all = vec![];
            nodes(plan, &mut all);
            for n in all {
                assert_ne!(n["Sort Space Type"], "Disk", "external sort");
                for metric in ["Temp Read Blocks", "Temp Written Blocks"] {
                    assert_eq!(n[metric].as_u64().unwrap_or(0), 0, "spill {metric}");
                }
                assert!(n["Hash Batches"].as_u64().unwrap_or(1) <= 1, "batched hash");
            }
        });
    }
    fn relation(&mut self, plan: &Value, table: &str, max: f64, indexed: bool) {
        let mut all = vec![];
        nodes(plan, &mut all);
        let rows: Vec<_> = all
            .into_iter()
            .filter(|n| n["Relation Name"] == table)
            .collect();
        let examined: f64 = rows
            .iter()
            .map(|n| {
                (n["Actual Rows"].as_f64().unwrap_or(0.0)
                    + n["Rows Removed by Filter"].as_f64().unwrap_or(0.0)
                    + n["Rows Removed by Index Recheck"].as_f64().unwrap_or(0.0))
                    * n["Actual Loops"].as_f64().unwrap_or(0.0)
            })
            .sum();
        println!(
            "CRM_010F1_METADATA_BOUND {}",
            json!({"label":self.label,"relation":table,"examined":examined,"maximum":max,"indexed_required":indexed})
        );
        self.verify(|| {
            assert!(!rows.is_empty(), "missing relation {table}");
            assert!(examined <= max, "{table} examined {examined}, bound {max}");
            if indexed {
                for n in rows {
                    let kind = n["Node Type"].as_str().unwrap();
                    assert!(
                        kind.contains("Index") || kind == "Bitmap Heap Scan",
                        "unindexed {table}: {n}"
                    );
                }
            }
        });
    }
    fn returned(&mut self, plan: &Value, expected: i64) {
        self.verify(|| {
            // PostgreSQL 18 emits integral row counts as JSON 1.0. Our small
            // bounded results are exactly representable as f64; reject missing,
            // non-finite or fractional counts rather than truncating them.
            assert!((0..=PEOPLE).contains(&expected));
            let actual = plan[0]["Plan"]["Actual Rows"]
                .as_f64()
                .expect("numeric Actual Rows");
            assert!(actual.is_finite() && actual >= 0.0 && actual.fract() == 0.0);
            assert_eq!(actual, expected as f64);
        });
    }
    fn finish(self) {
        println!(
            "CRM_010F1_METADATA_PLAN_CHECKS {}",
            json!({"checks":self.count,"failures":self.failures})
        );
        assert!(
            self.failures.is_empty(),
            "query-plan checks failed:\n{}",
            self.failures.join("\n")
        );
    }
}
macro_rules! explain {
    ($checks:expr,$conn:expr,$label:expr,$source:expr,$prefix:expr $(,$bind:expr)* $(,)?) => {{
        let sql = statement($source, $prefix);
        let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan: Value = sqlx::query_scalar(&wrapped)$(.bind($bind))*
            .fetch_one(&mut *$conn).await.unwrap();
        $checks.plan($label, &sql, json!([$($bind),*]), &plan);
        plan
    }};
}
struct Scale {
    f: Fixture,
    parent: Uuid,
    parent_plan: Uuid,
    child: Uuid,
    plan: Uuid,
    old_plan: Uuid,
    account: i64,
    person: Uuid,
    receipt: Uuid,
    source: Uuid,
    manifest: Uuid,
    result: Uuid,
    tag: Uuid,
    field: Uuid,
    choice_field: Uuid,
    option: Uuid,
    source_before: Uuid,
    execution_mapping_fifty: Uuid,
    mapping_deep: Uuid,
    mapping_field_tail: Uuid,
    alias_deep: Uuid,
    manifest_deep: Uuid,
    manifest_empty_held: Uuid,
    result_deep: Uuid,
    result_empty_held: Uuid,
}
async fn seed(migrator: &PgPool) -> Scale {
    let started = Instant::now();
    let (f, parent, child, ready) = crate::db_metadata_import_gate::fixture(migrator, false).await;
    let plan = Uuid::parse_str(ready["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let mut conn = migrator.acquire().await.unwrap();
    let base = sqlx::query(
        "SELECT parent_plan_id,source_account_id,state,retained_bytes,reserved_bytes FROM migration_metadata_import WHERE id=$1",
    )
    .bind(child)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    println!(
        "CRM_010F1_METADATA_EXECUTABLE_FIXTURE_STORAGE {}",
        json!({"classification":"real_ready_child_before_inert_seed","child_id":child,"plan_id":plan,"state":base.get::<String,_>("state"),"retained_bytes":base.get::<i64,_>("retained_bytes"),"reserved_bytes":base.get::<i64,_>("reserved_bytes"),"latest_plan_counts":ready["latest_plan"]["counts"]})
    );
    let parent_plan: Uuid = base.get("parent_plan_id");
    let account: i64 = base.get("source_account_id");
    let person: Uuid = sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let old_plan: Uuid = sqlx::query_scalar("SELECT id FROM migration_metadata_plan WHERE import_id=$1 AND id<>$2 ORDER BY revision LIMIT 1").bind(child).bind(plan).fetch_one(&mut *conn).await.unwrap();
    let receipt: Uuid = sqlx::query_scalar("SELECT request_id FROM migration_metadata_receipt WHERE import_id=$1 AND action='plan' LIMIT 1").bind(child).fetch_one(&mut *conn).await.unwrap();
    // Cardinality-only members never authenticate and need no credentials.
    sqlx::query("CREATE TEMP TABLE metadata_members AS SELECT n,gen_random_uuid() AS id FROM generate_series(3,50) n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'metadata-plan-'||n||'@synthetic.test','Synthetic member '||n FROM metadata_members").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM metadata_members").bind(f.org).execute(&mut *conn).await.unwrap();
    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM organization_membership WHERE organization_id=$1 ORDER BY user_id",
    )
    .bind(f.org)
    .fetch_all(&mut *conn)
    .await
    .unwrap();
    assert_eq!(members.len(), 50);
    // Production metadata IDs are UUIDv4. Randomize paged fixture rows too:
    // sequential fixed prefixes artificially correlate scope with the global
    // primary-key order and make unrelated history occupy a separate range.
    // Semantic n remains stable for native values and source-order probes.
    sqlx::query("CREATE TEMP TABLE metadata_rows AS SELECT n,CASE WHEN n=1 THEN $1 ELSE ('10000000-0000-0000-0000-'||lpad(to_hex(n),12,'0'))::uuid END AS person,('20000000-0000-0000-0000-'||lpad(to_hex(n),12,'0'))::uuid AS record,gen_random_uuid() AS source,gen_random_uuid() AS manifest,gen_random_uuid() AS result,('60000000-0000-0000-0000-'||lpad(to_hex(n),12,'0'))::uuid AS parent_source,('70000000-0000-0000-0000-'||lpad(to_hex(n),12,'0'))::uuid AS parent_manifest,('80000000-0000-0000-0000-'||lpad(to_hex(n),12,'0'))::uuid AS parent_result,(1000000+n)::text AS source_id,((n-1)/100+1)::integer AS batch,((n-1)%100)::integer AS ordinal FROM generate_series(1,$2::bigint) n").bind(person).bind(PEOPLE).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT person,$1,'Synthetic','Metadata '||n,$2,($3::uuid[])[1+((n-1)%50)] FROM metadata_rows WHERE n>1").bind(f.org).bind(f.lead_stage).bind(&members).execute(&mut *conn).await.unwrap();
    sqlx::query("CREATE TEMP TABLE metadata_catalog AS SELECT n,gen_random_uuid() AS mapping,gen_random_uuid() AS target,decode(repeat(md5('metadata-key-'||n),2),'hex') AS key,CASE WHEN n<=10 THEN 'field' WHEN n<=20 THEN 'tag' ELSE 'option' END AS kind FROM generate_series(1,70) n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO tag(id,organization_id,name,created_by_user_id) SELECT target,$1,'Synthetic Tag '||n,$2 FROM metadata_catalog WHERE kind='tag'").bind(f.org).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id,source,external_key) SELECT target,$1,'Synthetic Field '||n,CASE WHEN n=10 THEN 'choice' ELSE 'text' END,n,$2,'fub','syntheticField'||n FROM metadata_catalog WHERE kind='field'").bind(f.org).bind(f.actor).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO custom_field_option(id,organization_id,field_id,label,position) SELECT c.target,$1,f.target,'Synthetic Option '||c.n,c.n FROM metadata_catalog c CROSS JOIN metadata_catalog f WHERE c.kind='option' AND f.n=10").bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) SELECT $1,r.person,c.target,$2 FROM metadata_rows r CROSS JOIN metadata_catalog c WHERE c.kind='tag'").bind(f.org).bind(f.actor).execute(&mut *conn).await.unwrap();
    // Keep the exact-match probe at Person 24001 / field 1. Every other cell
    // has 150–500 deterministic ASCII characters. Distinct digest segments
    // avoid making the width fixture one repeated, unusually compressible run.
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,origin,correlation_id) SELECT $1,r.person,c.target,'text',CASE WHEN r.n=24001 AND c.n=1 THEN 'Synthetic value' ELSE left((SELECT string_agg(md5(r.n::text||':'||c.n::text||':'||part::text),'' ORDER BY part) FROM generate_series(1,16) part),(150+((r.n*37+c.n*53)%351))::integer) END,'migration',gen_random_uuid() FROM metadata_rows r CROSS JOIN metadata_catalog c WHERE c.n<=4").bind(f.org).execute(&mut *conn).await.unwrap();
    // Captures contain 100 projected rows, preserving the production page shape.
    sqlx::query("CREATE TEMP TABLE metadata_captures AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,251) n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,source_version,classification,truncated,accepted) SELECT h.id,c.snapshot_id,c.organization_id,CASE WHEN h.n=251 THEN 'custom_fields' ELSE 'people' END,1000+h.n,100000+h.n,decode(repeat(md5(h.n::text),2),'hex'),c.representation,c.http_status,c.raw_byte_len,c.nonce,c.ciphertext,c.source_version,'synthetic_inert_plan',false,true FROM metadata_captures h CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='people' ORDER BY sequence LIMIT 1)c").bind(f.snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT r.record,s.snapshot_id,s.organization_id,c.id,1000+r.batch,r.ordinal,'people',r.source_id,s.representation,decode(repeat(md5(r.source_id),2),'hex'),s.projection_nonce,s.projection_ciphertext,false FROM metadata_rows r JOIN metadata_captures c ON c.n=r.batch CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='people' ORDER BY capture_sequence LIMIT 1)s").bind(f.snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,representation,semantic_hmac,qualified,nonce,ciphertext) SELECT r.source,$1,$2,$3,$4,'people',r.source_id,r.record,c.id,1000+r.batch,r.ordinal,s.representation,decode(repeat(md5(r.source_id),2),'hex'),true,s.nonce,s.ciphertext FROM metadata_rows r JOIN metadata_captures c ON c.n=r.batch CROSS JOIN LATERAL(SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND family='people' LIMIT 1)s").bind(plan).bind(child).bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    // Ten retained definitions back field/option catalog rows; all ciphertext is
    // copied evidence-shaped data, not a claim that these are executable sources.
    sqlx::query("CREATE TEMP TABLE metadata_fields AS SELECT n,gen_random_uuid() AS record,gen_random_uuid() AS source FROM generate_series(1,10) n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT h.record,s.snapshot_id,s.organization_id,c.id,1251,h.n-1,'custom_fields',(2000000+h.n)::text,s.representation,decode(repeat(md5(h.n::text),2),'hex'),s.projection_nonce,s.projection_ciphertext,false FROM metadata_fields h CROSS JOIN metadata_captures c CROSS JOIN LATERAL(SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='custom_fields' LIMIT 1)s WHERE c.n=251").bind(f.snapshot).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,representation,semantic_hmac,qualified,nonce,ciphertext) SELECT h.source,$1,$2,$3,$4,'custom_fields',(2000000+h.n)::text,h.record,c.id,1251,h.n-1,s.representation,decode(repeat(md5(h.n::text),2),'hex'),true,s.nonce,s.ciphertext FROM metadata_fields h CROSS JOIN metadata_captures c CROSS JOIN LATERAL(SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND family='custom_fields' LIMIT 1)s WHERE c.n=251").bind(plan).bind(child).bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,ordinal,representation,semantic_hmac,qualified,nonce,ciphertext) SELECT r.parent_source,$1,$2,$3,$4,'people',r.source_id,r.record,c.id,r.ordinal,s.representation,decode(repeat(md5(r.source_id),2),'hex'),true,s.nonce,s.ciphertext FROM metadata_rows r JOIN metadata_captures c ON c.n=r.batch CROSS JOIN LATERAL(SELECT * FROM migration_import_source WHERE plan_id=$1 AND family='people' LIMIT 1)s").bind(parent_plan).bind(parent).bind(f.snapshot).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,disposition,contact_count,added_byte_bound,nonce,ciphertext) SELECT r.parent_manifest,$1,$2,$3,r.source_id,r.parent_source,'eligible',0,4096,s.nonce,s.ciphertext FROM metadata_rows r CROSS JOIN LATERAL(SELECT * FROM migration_import_manifest WHERE plan_id=$1 LIMIT 1)s").bind(parent_plan).bind(parent).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_result(id,import_id,plan_id,organization_id,manifest_id,source_id,person_id,disposition,contact_count,provenance_nonce,provenance_ciphertext) SELECT r.parent_result,$1,$2,$3,r.parent_manifest,r.source_id,r.person,'imported',0,s.provenance_nonce,s.provenance_ciphertext FROM metadata_rows r CROSS JOIN LATERAL(SELECT * FROM migration_import_result WHERE import_id=$1 LIMIT 1)s").bind(parent).bind(parent_plan).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id,manifest_id) SELECT $1,$2,'people',source_id,person,$3,$4,parent_manifest FROM metadata_rows").bind(f.org).bind(account).bind(parent).bind(parent_plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_mapping(id,plan_id,import_id,organization_id,kind,source_key,source_row_id,source_sequence,source_ordinal,element_ordinal,name_hmac,qualified,disposition,target_id,target_field_id,candidate_disposition,candidate_target_id,candidate_field_id,target_label_hmac,dependent_count,added_byte_bound,nonce,ciphertext) SELECT c.mapping,$1,$2,$3,c.kind,c.key,CASE WHEN c.kind='tag' THEN (SELECT source FROM metadata_rows WHERE n=1) ELSE h.source END,CASE WHEN c.kind='tag' THEN 1001 ELSE 1251 END,CASE WHEN c.kind='tag' THEN 0 ELSE h.n-1 END,c.n,c.key,true,'create_matching',c.target,CASE WHEN c.kind='option' THEN f.target END,'create_matching',c.target,CASE WHEN c.kind='option' THEN f.target END,c.key,25000,4096,s.nonce,s.ciphertext FROM metadata_catalog c JOIN metadata_fields h ON h.n=CASE WHEN c.n<=10 THEN c.n ELSE 10 END CROSS JOIN metadata_catalog f CROSS JOIN LATERAL(SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 ORDER BY id LIMIT 1)s WHERE f.n=10").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    let catalog_points = sqlx::query("SELECT (SELECT mapping FROM metadata_catalog WHERE n=1) AS field,(SELECT mapping FROM metadata_catalog WHERE n=10) AS choice_field,(SELECT mapping FROM metadata_catalog WHERE n=11) AS tag,(SELECT mapping FROM metadata_catalog WHERE n=21) AS option,(SELECT mapping FROM metadata_catalog WHERE n=50) AS execution_mapping_fifty").fetch_one(&mut *conn).await.unwrap();
    let field: Uuid = catalog_points.get("field");
    let choice_field: Uuid = catalog_points.get("choice_field");
    let tag: Uuid = catalog_points.get("tag");
    let option: Uuid = catalog_points.get("option");
    let execution_mapping_fifty: Uuid = catalog_points.get("execution_mapping_fifty");
    sqlx::query("UPDATE migration_metadata_mapping SET parent_mapping_id=$1 WHERE plan_id=$2 AND kind='option'").bind(choice_field).bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),$1,$2,$3,c.kind,c.key,s.nonce,s.ciphertext FROM metadata_catalog c CROSS JOIN LATERAL(SELECT * FROM migration_metadata_choice WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,parent_result_id,person_id,disposition,added_byte_bound,nonce,ciphertext) SELECT r.manifest,$1,$2,$3,r.source_id,r.source,r.parent_result,r.person,CASE WHEN r.n%1000=0 THEN 'held' ELSE 'eligible' END,16384,s.nonce,s.ciphertext FROM metadata_rows r CROSS JOIN LATERAL(SELECT * FROM migration_metadata_manifest WHERE plan_id=$1 LIMIT 1)s").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_alias(id,plan_id,import_id,organization_id,mapping_id,source_row_id,element_ordinal,alias_key) SELECT gen_random_uuid(),$1,$2,$3,c.mapping,r.source,c.n-11,c.key FROM metadata_rows r CROSS JOIN metadata_catalog c WHERE c.kind='tag'").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_operation(id,plan_id,import_id,organization_id,manifest_id,mapping_id,kind,source_key,target_id,disposition) SELECT gen_random_uuid(),$1,$2,$3,r.manifest,c.mapping,CASE WHEN c.kind='tag' THEN 'tag_link' ELSE 'value' END,c.key,c.target,CASE WHEN r.n%1000=0 THEN 'held' ELSE 'eligible' END FROM metadata_rows r CROSS JOIN metadata_catalog c WHERE c.kind='tag' OR c.n<=4").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_result(id,import_id,plan_id,organization_id,kind,manifest_id,unit_id,source_id,person_id,disposition,counts,nonce,ciphertext) SELECT r.result,$1,$2,$3,'people',r.manifest,r.manifest,r.source_id,r.person,CASE WHEN r.n%1000=0 THEN 'held' ELSE 'applied' END,'{\"tags_applied\":10,\"values_applied\":4}'::jsonb,s.nonce,s.ciphertext FROM metadata_rows r CROSS JOIN LATERAL(SELECT * FROM migration_metadata_manifest WHERE plan_id=$2 LIMIT 1)s").bind(child).bind(plan).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_result(id,import_id,plan_id,organization_id,kind,mapping_id,unit_id,disposition,nonce,ciphertext) SELECT gen_random_uuid(),$1,$2,$3,c.kind,c.mapping,c.mapping,'created',s.nonce,s.ciphertext FROM metadata_catalog c CROSS JOIN LATERAL(SELECT * FROM migration_metadata_mapping WHERE plan_id=$2 LIMIT 1)s WHERE c.n%2=0").bind(child).bind(plan).bind(f.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_identity(organization_id,source_account_id,kind,source_key,target_id,import_id,plan_id,mapping_id) SELECT $1,$2,kind,key,target,$3,$4,mapping FROM metadata_catalog").bind(f.org).bind(account).bind(child).bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_issue(plan_id,import_id,organization_id,code,record_count) SELECT $1,$2,$3,'synthetic_issue_'||lpad(n::text,2,'0'),n FROM generate_series(1,60)n").bind(plan).bind(child).bind(f.org).execute(&mut *conn).await.unwrap();
    let person_points = sqlx::query("SELECT person,source,manifest,result,(SELECT source FROM metadata_rows WHERE n=24000) AS source_before FROM metadata_rows WHERE n=24001").fetch_one(&mut *conn).await.unwrap();
    let probe_person: Uuid = person_points.get("person");
    let source: Uuid = person_points.get("source");
    let manifest: Uuid = person_points.get("manifest");
    let result: Uuid = person_points.get("result");
    let source_before: Uuid = person_points.get("source_before");
    // One retained plan per terminal child plus the real superseded plan. These
    // historical imports are not attached to the live workspace or executable.
    sqlx::query("CREATE TEMP TABLE metadata_history AS SELECT n,gen_random_uuid() AS parent,gen_random_uuid() AS parent_plan,gen_random_uuid() AS child,gen_random_uuid() AS plan,gen_random_uuid() AS source,gen_random_uuid() AS manifest,gen_random_uuid() AS result FROM generate_series(1,$1::bigint)n").bind(HISTORY).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import(id,organization_id,snapshot_id,preview_id,source_account_id,capture_sequence,executor_user_id,state) SELECT h.parent,i.organization_id,i.snapshot_id,i.preview_id,i.source_account_id,i.capture_sequence,i.executor_user_id,'completed' FROM metadata_history h CROSS JOIN migration_import i WHERE i.id=$1").bind(parent).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_import_plan(id,import_id,snapshot_id,organization_id,revision,state,patch_nonce,patch_ciphertext) SELECT h.parent_plan,h.parent,p.snapshot_id,p.organization_id,1,'ready',p.patch_nonce,p.patch_ciphertext FROM metadata_history h CROSS JOIN migration_import_plan p WHERE p.id=$1").bind(parent_plan).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_import i SET latest_plan_id=h.parent_plan,confirmed_plan_id=h.parent_plan FROM metadata_history h WHERE i.id=h.parent").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state,phase,created_at) SELECT h.child,i.organization_id,h.parent,h.parent_plan,i.snapshot_id,i.preview_id,i.source_account_id,i.capture_sequence,i.workspace_revision,i.executor_user_id,'completed','complete',i.created_at-h.n*interval '1 minute' FROM metadata_history h CROSS JOIN migration_metadata_import i WHERE i.id=$1").bind(child).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_plan(id,import_id,snapshot_id,organization_id,revision,state,phase,patch_nonce,patch_ciphertext,destination_nonce,destination_ciphertext) SELECT h.plan,h.child,p.snapshot_id,p.organization_id,1,'ready','ready',p.patch_nonce,p.patch_ciphertext,p.destination_nonce,p.destination_ciphertext FROM metadata_history h CROSS JOIN migration_metadata_plan p WHERE p.id=$1").bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_metadata_import i SET latest_plan_id=h.plan,confirmed_plan_id=h.plan FROM metadata_history h WHERE i.id=h.child").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_mapping(id,plan_id,import_id,organization_id,kind,source_key,source_sequence,source_ordinal,element_ordinal,name_hmac,qualified,disposition,nonce,ciphertext) SELECT gen_random_uuid(),h.plan,h.child,m.organization_id,m.kind,m.source_key,m.source_sequence,m.source_ordinal,m.element_ordinal,m.name_hmac,m.qualified,'hold',m.nonce,m.ciphertext FROM metadata_history h CROSS JOIN migration_metadata_mapping m WHERE m.plan_id=$1").bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) SELECT gen_random_uuid(),h.plan,h.child,c.organization_id,c.kind,c.source_key,c.nonce,c.ciphertext FROM metadata_history h CROSS JOIN migration_metadata_choice c WHERE c.plan_id=$1").bind(plan).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,representation,semantic_hmac,qualified,nonce,ciphertext) SELECT h.source,h.plan,h.child,s.snapshot_id,s.organization_id,s.family,s.source_id,s.record_id,s.capture_id,s.capture_sequence,s.ordinal,s.representation,s.semantic_hmac,s.qualified,s.nonce,s.ciphertext FROM metadata_history h CROSS JOIN migration_metadata_source s WHERE s.id=$1").bind(source).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,person_id,disposition,added_byte_bound,nonce,ciphertext) SELECT h.manifest,h.plan,h.child,m.organization_id,m.source_id,h.source,m.person_id,m.disposition,m.added_byte_bound,m.nonce,m.ciphertext FROM metadata_history h CROSS JOIN migration_metadata_manifest m WHERE m.id=$1").bind(manifest).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_result(id,import_id,plan_id,organization_id,kind,manifest_id,unit_id,source_id,person_id,disposition,nonce,ciphertext) SELECT h.result,h.child,h.plan,r.organization_id,'people',h.manifest,h.manifest,r.source_id,r.person_id,'already_present',r.nonce,r.ciphertext FROM metadata_history h CROSS JOIN migration_metadata_result r WHERE r.id=$1").bind(result).execute(&mut *conn).await.unwrap();
    // Inert queued state exists only so the production claim SELECT returns one
    // candidate among terminal history. No executor is run with these fixtures.
    sqlx::query("UPDATE migration_metadata_import SET state='queued' WHERE id=$1")
        .bind(child)
        .execute(&mut *conn)
        .await
        .unwrap();
    for table in [
        "person",
        "organization_membership",
        "tag",
        "person_tag",
        "custom_field",
        "custom_field_option",
        "person_custom_field_value",
        "migration_import",
        "migration_import_plan",
        "migration_import_source",
        "migration_import_manifest",
        "migration_import_result",
        "migration_import_identity",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_metadata_import",
        "migration_metadata_plan",
        "migration_metadata_source",
        "migration_metadata_mapping",
        "migration_metadata_choice",
        "migration_metadata_alias",
        "migration_metadata_manifest",
        "migration_metadata_operation",
        "migration_metadata_identity",
        "migration_metadata_result",
        "migration_metadata_issue",
        "migration_metadata_receipt",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
        // Fixture storage observations, separate from the actual production
        // EXPLAIN gates. These are whole-relation sizes, not tenant billing or
        // an extrapolation of authentic retained source storage.
        let sizes: Value = sqlx::query_scalar(&format!("SELECT jsonb_build_object('row_count',count(*),'pg_column_size_row_bytes',jsonb_build_object('min',min(pg_column_size(t)),'max',max(pg_column_size(t)),'mean',round(avg(pg_column_size(t)),2),'sum',sum(pg_column_size(t))),'pg_total_relation_size_bytes',pg_total_relation_size($1::regclass),'pg_table_size_bytes',pg_table_size($1::regclass),'pg_indexes_size_bytes',pg_indexes_size($1::regclass)) FROM {table} t"))
            .bind(table).fetch_one(&mut *conn).await.unwrap();
        println!(
            "CRM_010F1_METADATA_RELATION_STORAGE {}",
            json!({"classification":"synthetic_inert_query_plan_only","scope":"whole_relation_in_isolated_database","table":table,"metrics":sizes})
        );
    }
    let widths: Value = sqlx::query_scalar("SELECT jsonb_build_object('row_count',count(*),'exact_probe_rows',count(*) FILTER (WHERE text_value='Synthetic value'),'text_characters',jsonb_build_object('min',min(char_length(text_value)),'max',max(char_length(text_value)),'mean',round(avg(char_length(text_value)),2)),'text_octets',jsonb_build_object('min',min(octet_length(text_value)),'max',max(octet_length(text_value)),'mean',round(avg(octet_length(text_value)),2),'sum',sum(octet_length(text_value))),'pg_column_size_text_bytes',jsonb_build_object('min',min(pg_column_size(text_value)),'max',max(pg_column_size(text_value)),'mean',round(avg(pg_column_size(text_value)),2),'sum',sum(pg_column_size(text_value))),'spread',jsonb_build_object('row_count',count(*) FILTER (WHERE text_value<>'Synthetic value'),'min_characters',min(char_length(text_value)) FILTER (WHERE text_value<>'Synthetic value'),'max_characters',max(char_length(text_value)) FILTER (WHERE text_value<>'Synthetic value'),'mean_characters',round(avg(char_length(text_value)) FILTER (WHERE text_value<>'Synthetic value'),2))) FROM person_custom_field_value WHERE organization_id=$1")
        .bind(f.org).fetch_one(&mut *conn).await.unwrap();
    println!(
        "CRM_010F1_METADATA_NATIVE_WIDTHS {}",
        json!({"classification":"deterministic_moderate_dense_ascii_width_spread_not_maximum_capacity","metrics":widths})
    );
    assert_eq!(widths["row_count"], PEOPLE * 4);
    assert_eq!(widths["exact_probe_rows"], 1);
    assert_eq!(widths["spread"]["row_count"], PEOPLE * 4 - 1);
    assert_eq!(widths["spread"]["min_characters"], 150);
    assert_eq!(widths["spread"]["max_characters"], 500);
    let counts: Value = sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1),'tag_links',(SELECT count(*) FROM person_tag WHERE organization_id=$1),'values',(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1),'sources',(SELECT count(*) FROM migration_metadata_source WHERE plan_id=$2),'mappings',(SELECT count(*) FROM migration_metadata_mapping WHERE plan_id=$2),'aliases',(SELECT count(*) FROM migration_metadata_alias WHERE plan_id=$2),'manifests',(SELECT count(*) FROM migration_metadata_manifest WHERE plan_id=$2),'operations',(SELECT count(*) FROM migration_metadata_operation WHERE plan_id=$2),'results',(SELECT count(*) FROM migration_metadata_result WHERE plan_id=$2),'child_history',(SELECT count(*) FROM migration_metadata_import WHERE organization_id=$1))").bind(f.org).bind(plan).fetch_one(&mut *conn).await.unwrap();
    println!(
        "CRM_010F1_METADATA_FIXTURE {}",
        json!({"elapsed_ms":started.elapsed().as_millis(),"classification":"synthetic_inert_query_plan_only","counts":counts})
    );
    assert_eq!(counts["people"], PEOPLE);
    assert_eq!(counts["members"], 50);
    assert_eq!(counts["tag_links"], PEOPLE * 10);
    assert_eq!(counts["values"], PEOPLE * 4);
    assert_eq!(counts["sources"], PEOPLE + 12);
    assert_eq!(counts["mappings"], 72);
    assert_eq!(counts["aliases"], PEOPLE * 10 + 1);
    assert_eq!(counts["manifests"], PEOPLE + 1);
    assert_eq!(counts["operations"], PEOPLE * 14 + 2);
    assert_eq!(counts["results"], PEOPLE + 35);
    assert_eq!(counts["child_history"], HISTORY + 1);
    // Fixture bookkeeping derives cursors from actual UUID order. First pages
    // start at nil; deep pages follow rank 24,000 in their selected scope
    // (mapping pages follow rank 50). Sparse held rows are still semantic
    // Person n % 1000 == 0, independent of random page order. Empty filtered
    // tails start at the greatest matching UUID, leaving any other rows in the
    // independently counted tail. Point/source-order probes keep semantic n.
    let anchors = sqlx::query("SELECT (SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 ORDER BY id OFFSET 49 LIMIT 1) AS mapping_deep,(SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND kind='field' ORDER BY id DESC LIMIT 1) AS mapping_field_tail,(SELECT id FROM migration_metadata_alias WHERE plan_id=$1 AND mapping_id=$2 ORDER BY id OFFSET 23999 LIMIT 1) AS alias_deep,(SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 ORDER BY id OFFSET 23999 LIMIT 1) AS manifest_deep,(SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 AND disposition='held' ORDER BY id DESC LIMIT 1) AS manifest_empty_held,(SELECT id FROM migration_metadata_result WHERE plan_id=$1 ORDER BY id OFFSET 23999 LIMIT 1) AS result_deep,(SELECT id FROM migration_metadata_result WHERE plan_id=$1 AND kind='people' AND disposition='held' ORDER BY id DESC LIMIT 1) AS result_empty_held").bind(plan).bind(tag).fetch_one(&mut *conn).await.unwrap();
    let mapping_deep: Uuid = anchors.get("mapping_deep");
    let mapping_field_tail: Uuid = anchors.get("mapping_field_tail");
    let alias_deep: Uuid = anchors.get("alias_deep");
    let manifest_deep: Uuid = anchors.get("manifest_deep");
    let manifest_empty_held: Uuid = anchors.get("manifest_empty_held");
    let result_deep: Uuid = anchors.get("result_deep");
    let result_empty_held: Uuid = anchors.get("result_empty_held");
    println!(
        "CRM_010F1_METADATA_FIXTURE_ANCHORS {}",
        json!({"id_distribution":"UUIDv4 from gen_random_uuid, independent of semantic n and insertion order","first":"nil, before all scoped UUIDs","points":{"person_semantic_n":24001,"person":probe_person,"source":source,"manifest":manifest,"result":result,"tag_semantic_n":11,"tag":tag,"field_semantic_n":1,"field":field,"choice_field_semantic_n":10,"choice_field":choice_field,"option_semantic_n":21,"option":option,"source_before_semantic_n":24000,"source_before":source_before,"execution_mapping_semantic_n":50,"execution_mapping":execution_mapping_fifty},"deep":{"mapping":{"id":mapping_deep,"scoped_rank":50,"scoped_count":72,"remaining":22},"alias":{"id":alias_deep,"mapping_id":tag,"scoped_rank":24000,"scoped_count":25000,"remaining":1000},"manifest":{"id":manifest_deep,"scoped_rank":24000,"scoped_count":25001,"remaining":1001},"result":{"id":result_deep,"scoped_rank":24000,"scoped_count":25035,"remaining":1035}},"empty_filtered_tails":{"meaning":"greatest matching UUID; other scoped rows remain in the reported bound","mapping_field":mapping_field_tail,"manifest_held":manifest_empty_held,"result_people_held":result_empty_held},"held_membership":"semantic Person n % 1000 == 0, 25 rows independently distributed through UUID order"})
    );
    Scale {
        f,
        parent,
        parent_plan,
        child,
        plan,
        old_plan,
        account,
        person: probe_person,
        receipt,
        source,
        manifest,
        result,
        tag,
        field,
        choice_field,
        option,
        source_before,
        execution_mapping_fifty,
        mapping_deep,
        mapping_field_tail,
        alias_deep,
        manifest_deep,
        manifest_empty_held,
        result_deep,
        result_empty_held,
    }
}

/// This count is fixture bookkeeping, not a measured production query. It makes
/// the available keyset tail explicit even when a rare filter returns no page.
async fn tail(conn: &mut sqlx::PgConnection, table: &str, plan: Uuid, after: Uuid) -> i64 {
    assert!(matches!(
        table,
        "migration_metadata_mapping" | "migration_metadata_manifest" | "migration_metadata_result"
    ));
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {table} WHERE plan_id=$1 AND id>$2"
    ))
    .bind(plan)
    .bind(after)
    .fetch_one(conn)
    .await
    .unwrap()
}

// The permit function is PL/pgSQL, so its SQL expression refers to local
// variables. Substitute only whole identifiers outside SQL string literals;
// all predicates/joins/casts remain the exact production expression.
fn operation_guard() -> (String, String) {
    let start = SCHEMA.find("RETURN EXISTS(SELECT 1 FROM migration_metadata_manifest a JOIN migration_metadata_operation x").expect("production operation permit");
    let original = SCHEMA[start..].split(';').next().unwrap().to_owned();
    let mut sql = String::new();
    let mut chars = original.trim_start_matches("RETURN ").chars().peekable();
    sql.push_str("SELECT ");
    while let Some(c) = chars.next() {
        if c == '\'' {
            sql.push(c);
            for q in chars.by_ref() {
                sql.push(q);
                if q == '\'' {
                    break;
                }
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut token = String::from(c);
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                token.push(chars.next().unwrap());
            }
            sql.push_str(match token.as_str() {
                "plan" => "$1::uuid",
                "run" => "$2::uuid",
                "org" => "$3::uuid",
                "unit" => "$4::uuid",
                "row_value" => "$5::jsonb",
                "table_name" => "$6::text",
                _ => &token,
            });
        } else {
            sql.push(c);
        }
    }
    (original, sql)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_hot_queries_at_d050(migrator: PgPool) {
    let s = seed(&migrator).await;
    let mut conn = s.f.pool.acquire().await.unwrap();
    let mut checks = Checks::default();
    let nil = Uuid::nil();
    let source = s.source;
    let manifest = s.manifest;
    let result = s.result;
    let tag = s.tag;
    let field = s.field;
    let choice_field = s.choice_field;
    let row = sqlx::query(
        "SELECT target_id,source_key,name_hmac FROM migration_metadata_mapping WHERE id=$1",
    )
    .bind(tag)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    let tag_target: Uuid = row.get("target_id");
    let tag_key: Vec<u8> = row.get("source_key");
    let field_row = sqlx::query(
        "SELECT target_id,source_key,source_row_id FROM migration_metadata_mapping WHERE id=$1",
    )
    .bind(field)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    let field_target: Uuid = field_row.get("target_id");
    let field_key: Vec<u8> = field_row.get("source_key");
    let field_source: Uuid = field_row.get("source_row_id");
    let choice_target: Uuid =
        sqlx::query_scalar("SELECT target_id FROM migration_metadata_mapping WHERE id=$1")
            .bind(choice_field)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let option_row =
        sqlx::query("SELECT target_id,source_key FROM migration_metadata_mapping WHERE id=$1")
            .bind(s.option)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let option_target: Uuid = option_row.get("target_id");
    let option_key: Vec<u8> = option_row.get("source_key");
    let p = explain!(checks,conn,"claim_among_terminal_history",WORKER,"SELECT i.id,i.organization_id,i.executor_user_id FROM migration_metadata_import i JOIN migration_metadata_plan p");
    checks.relation(&p, "migration_metadata_import", 1.0, true);
    checks.returned(&p, 1);
    let p = explain!(
        checks,
        conn,
        "current_run_lock",
        STORE,
        "SELECT * FROM migration_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE",
        s.child,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_import", 1.0, true);
    let p = explain!(checks,conn,"current_plan_lock",STORE,"SELECT * FROM migration_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE",s.plan,s.child,s.f.org);
    checks.relation(&p, "migration_metadata_plan", 1.0, true);
    let p = explain!(checks,conn,"superseded_plan_lock",STORE,"SELECT * FROM migration_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE",s.old_plan,s.child,s.f.org);
    checks.relation(&p, "migration_metadata_plan", 1.0, true);
    checks.returned(&p, 1);
    let p = explain!(checks,conn,"idempotency_receipt_point",STORE,"SELECT * FROM migration_metadata_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4",s.f.org,s.f.actor,"plan",s.receipt);
    checks.relation(&p, "migration_metadata_receipt", 3.0, false);
    checks.returned(&p, 1);
    let p = explain!(checks,conn,"snapshot_storage_lock",STORE,"SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l",s.f.snapshot,s.f.org);
    checks.relation(&p, "migration_snapshot", 1.0, false);
    checks.relation(&p, "migration_snapshot_storage", 1.0, false);
    let p = explain!(checks,conn,"completed_parent_eligibility",COMMANDS,"SELECT p.*,o.workspace_revision AS current_workspace_revision FROM migration_import p JOIN migration_workspace w",s.parent,s.f.org);
    checks.relation(&p, "migration_import", 1.0, true);
    checks.returned(&p, 1);
    for (label, after, parent) in [
        ("child_history_first", nil, None),
        ("child_history_parent", nil, Some(s.parent)),
        ("child_history_absent_parent", nil, Some(Uuid::nil())),
    ] {
        let p = explain!(
            checks,
            conn,
            label,
            QUERIES,
            "SELECT id FROM migration_metadata_import WHERE organization_id=$1 AND id>$2",
            s.f.org,
            after,
            parent,
            51_i64
        );
        // Parent-filter SQL may scan the bounded retained-child tail. Report it
        // honestly; it need not examine only the one matching child.
        checks.relation(&p, "migration_metadata_import", (HISTORY + 1) as f64, false);
        checks.returned(
            &p,
            if parent.is_none() {
                51
            } else if parent == Some(s.parent) {
                1
            } else {
                0
            },
        );
    }
    let history_after: Uuid = sqlx::query_scalar("SELECT id FROM migration_metadata_import WHERE organization_id=$1 ORDER BY id OFFSET 200 LIMIT 1").bind(s.f.org).fetch_one(&mut *conn).await.unwrap();
    let p = explain!(
        checks,
        conn,
        "child_history_deep",
        QUERIES,
        "SELECT id FROM migration_metadata_import WHERE organization_id=$1 AND id>$2",
        s.f.org,
        history_after,
        Option::<Uuid>::None,
        51_i64
    );
    checks.relation(&p, "migration_metadata_import", (HISTORY + 1) as f64, false);
    checks.returned(&p, 51);
    let p = explain!(
        checks,
        conn,
        "choice_copy_first",
        WORKER,
        "SELECT id,kind,source_key,nonce,ciphertext FROM migration_metadata_choice WHERE",
        s.plan,
        s.f.org,
        "",
        Vec::<u8>::new()
    );
    checks.relation(&p, "migration_metadata_choice", 72.0, true);
    checks.returned(&p, 50);
    let p = explain!(
        checks,
        conn,
        "choice_copy_deep",
        WORKER,
        "SELECT id,kind,source_key,nonce,ciphertext FROM migration_metadata_choice WHERE",
        s.plan,
        s.f.org,
        "option",
        &option_key
    );
    checks.relation(&p, "migration_metadata_choice", 72.0, true);
    let p = explain!(
        checks,
        conn,
        "choice_point",
        WORKER,
        "SELECT id,nonce,ciphertext FROM migration_metadata_choice WHERE",
        s.plan,
        s.f.org,
        "tag",
        &tag_key
    );
    checks.relation(&p, "migration_metadata_choice", 1.0, true);
    checks.returned(&p, 1);
    let capture: Uuid =
        sqlx::query_scalar("SELECT capture_id FROM migration_metadata_source WHERE id=$1")
            .bind(source)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let p = explain!(checks,conn,"capture_checkpoint_deep",WORKER,"SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>",s.f.snapshot,s.f.org,1239_i64,1251_i64);
    checks.relation(&p, "migration_snapshot_capture", 1.0, true);
    checks.returned(&p, 1);
    let p = explain!(
        checks,
        conn,
        "capture_payload_point",
        WORKER,
        "SELECT * FROM migration_snapshot_capture WHERE id=$1",
        capture,
        s.f.snapshot,
        s.f.org
    );
    checks.relation(&p, "migration_snapshot_capture", 1.0, true);
    let p = explain!(checks,conn,"capture_record_page",WORKER,"SELECT id,source_id,ordinal,family,representation,semantic_hmac FROM migration_snapshot_record WHERE",capture,s.f.snapshot,s.f.org);
    checks.relation(&p, "migration_snapshot_record", 100.0, true);
    checks.returned(&p, 100);
    let p = explain!(checks,conn,"source_qualifier_point",WORKER,"SELECT id,qualified,conflict,semantic_hmac,nonce,ciphertext FROM migration_metadata_source WHERE",s.plan,s.f.org,"people","1024001");
    checks.relation(&p, "migration_metadata_source", 1.0, true);
    checks.returned(&p, 1);
    for (label, family, sequence, ordinal, after) in [
        ("people_source_first", "people", 0_i64, -1_i32, nil),
        (
            "people_source_deep",
            "people",
            1240_i64,
            99_i32,
            s.source_before,
        ),
        ("field_source_deep", "custom_fields", 1251_i64, 8_i32, nil),
    ] {
        let p=explain!(checks,conn,label,WORKER,"SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND (capture_sequence,ordinal,id)>",s.plan,s.f.org,family,sequence,ordinal,after);
        checks.relation(&p, "migration_metadata_source", 1.0, true);
        checks.returned(&p, 1);
    }
    let p = explain!(
        checks,
        conn,
        "source_cursor_point",
        WORKER,
        "SELECT capture_sequence,ordinal FROM migration_metadata_source WHERE",
        source,
        s.plan,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_source", 1.0, true);
    let p = explain!(checks,conn,"field_catalog_source_resume",WORKER,"SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND source_row_id=$3",s.plan,s.f.org,field_source);
    checks.relation(&p, "migration_metadata_mapping", 1.0, true);
    let p = explain!(
        checks,
        conn,
        "parent_result_identity_person",
        WORKER,
        "SELECT r.id,r.person_id FROM migration_import_identity i JOIN migration_import_result r",
        s.f.org,
        s.account,
        "1024001",
        s.parent,
        s.parent_plan
    );
    checks.relation(&p, "migration_import_identity", 1.0, true);
    checks.relation(&p, "migration_import_result", 1.0, true);
    checks.relation(&p, "person", 1.0, true);
    checks.returned(&p, 1);
    for (label, after, kind) in [
        ("mapping_first", nil, None),
        ("mapping_deep", s.mapping_deep, None),
        ("mapping_option_first", nil, Some("option")),
        ("mapping_option_deep", s.mapping_deep, Some("option")),
        ("mapping_field_tail", s.mapping_field_tail, Some("field")),
    ] {
        let remaining = tail(&mut conn, "migration_metadata_mapping", s.plan, after).await;
        let expected:i64=sqlx::query_scalar("SELECT LEAST(count(*),51) FROM migration_metadata_mapping WHERE plan_id=$1 AND id>$2 AND ($3::text IS NULL OR kind=$3)").bind(s.plan).bind(after).bind(kind).fetch_one(&mut *conn).await.unwrap();
        let p = explain!(
            checks,
            conn,
            label,
            QUERIES,
            "SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND import_id=$2",
            s.plan,
            s.child,
            s.f.org,
            after,
            kind,
            51_i64
        );
        checks.relation(&p, "migration_metadata_mapping", remaining as f64, true);
        checks.returned(&p, expected);
    }
    let p = explain!(
        checks,
        conn,
        "mapping_field_payload_point",
        QUERIES,
        "SELECT * FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3",
        field,
        s.plan,
        s.child,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_mapping", 1.0, true);
    let p=explain!(checks,conn,"source_field_payload_point",QUERIES,"SELECT * FROM migration_metadata_source WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND snapshot_id=$4",source,s.plan,s.child,s.f.snapshot,s.f.org);
    checks.relation(&p, "migration_metadata_source", 1.0, true);
    let p=explain!(checks,conn,"field_name_collision_bound",WORKER,"SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND name_hmac=$3",s.plan,s.f.org,&field_key);
    checks.relation(&p, "migration_metadata_mapping", 2.0, true);
    let p=explain!(checks,conn,"candidate_target_collision_bound",WORKER,"SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_target_id=$4",s.plan,s.f.org,"tag",tag_target);
    checks.relation(&p, "migration_metadata_mapping", 2.0, true);
    let p=explain!(checks,conn,"candidate_label_collision_bound",WORKER,"SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_disposition='create_matching' AND target_label_hmac=$4",s.plan,s.f.org,"tag",&tag_key,Option::<Uuid>::None);
    checks.relation(&p, "migration_metadata_mapping", 2.0, true);
    let p=explain!(checks,conn,"candidate_option_capacity_bound",WORKER,"SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='option' AND candidate_field_id=$3",s.plan,s.f.org,choice_target);
    checks.relation(&p, "migration_metadata_mapping", 51.0, true);
    let p=explain!(checks,conn,"candidate_tag_capacity_bound",WORKER,"SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_disposition='create_matching' LIMIT 201",s.plan,s.f.org,"tag");
    checks.relation(&p, "migration_metadata_mapping", 201.0, true);
    let p = explain!(
        checks,
        conn,
        "option_parent_count",
        WORKER,
        "SELECT count(*) FROM migration_metadata_mapping WHERE parent_mapping_id=$1",
        choice_field,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_mapping", 50.0, true);
    let p=explain!(checks,conn,"option_parent_byte_bound",WORKER,"SELECT COALESCE(sum(added_byte_bound),0)::bigint FROM migration_metadata_mapping WHERE parent_mapping_id=$1",choice_field,s.f.org);
    checks.relation(&p, "migration_metadata_mapping", 50.0, true);
    let p = explain!(
        checks,
        conn,
        "catalog_identity_point",
        WORKER,
        "SELECT target_id FROM migration_metadata_identity WHERE organization_id=$1",
        s.f.org,
        s.account,
        "tag",
        &tag_key
    );
    checks.relation(&p, "migration_metadata_identity", 70.0, false);
    let p = explain!(
        checks,
        conn,
        "option_exact_source_key",
        WORKER,
        "SELECT target_id,disposition,target_field_id FROM migration_metadata_mapping WHERE",
        s.plan,
        s.f.org,
        &option_key,
        choice_field
    );
    checks.relation(&p, "migration_metadata_mapping", 1.0, true);
    let p=explain!(checks,conn,"person_field_catalog_page",WORKER,"SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND id>$3 ORDER BY id LIMIT 50",s.plan,s.f.org,nil);
    checks.relation(&p, "migration_metadata_mapping", 11.0, true);
    let p = explain!(
        checks,
        conn,
        "alias_person_element_point",
        WORKER,
        "SELECT m.* FROM migration_metadata_alias a JOIN migration_metadata_mapping m",
        s.plan,
        s.f.org,
        source,
        0_i32
    );
    checks.relation(&p, "migration_metadata_alias", 1.0, true);
    checks.relation(&p, "migration_metadata_mapping", 1.0, true);
    checks.returned(&p, 1);
    let p=explain!(checks,conn,"alias_exact_raw_shared_tag",WORKER,"SELECT a.element_ordinal,s.* FROM migration_metadata_alias a JOIN migration_metadata_source s",tag,s.f.org,&tag_key);
    checks.relation(&p, "migration_metadata_alias", 1.0, true);
    checks.relation(&p, "migration_metadata_source", 1.0, true);
    checks.returned(&p, 1);
    for (label, after) in [("alias_first", nil), ("alias_deep", s.alias_deep)] {
        let p = explain!(
            checks,
            conn,
            label,
            QUERIES,
            "SELECT id,source_row_id,element_ordinal FROM migration_metadata_alias WHERE",
            tag,
            s.plan,
            s.child,
            s.f.org,
            after,
            51_i64
        );
        checks.relation(&p, "migration_metadata_alias", 51.0, true);
        checks.returned(&p, 51);
    }
    let p = explain!(
        checks,
        conn,
        "alias_total_shared_tag",
        QUERIES,
        "SELECT count(*) FROM migration_metadata_alias WHERE",
        tag,
        s.plan,
        s.child,
        s.f.org
    );
    // This is intentionally a full matching-group count, not a 51-row page.
    checks.relation(&p, "migration_metadata_alias", PEOPLE as f64, true);
    let p=explain!(checks,conn,"alias_record_link_point",QUERIES,"SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND source_id=$4",s.plan,s.child,s.f.org,"1024001",source);
    checks.relation(&p, "migration_metadata_manifest", 1.0, true);
    for (label, after, filter) in [
        ("records_first", nil, None),
        ("records_deep", s.manifest_deep, None),
        ("records_eligible_first", nil, Some("eligible")),
        ("records_eligible_deep", s.manifest_deep, Some("eligible")),
        ("records_sparse_held_first", nil, Some("held")),
        ("records_sparse_held_deep", s.manifest_deep, Some("held")),
        (
            "records_empty_held_tail",
            s.manifest_empty_held,
            Some("held"),
        ),
    ] {
        let remaining = tail(&mut conn, "migration_metadata_manifest", s.plan, after).await;
        let expected:i64=sqlx::query_scalar("SELECT LEAST(count(*),51) FROM migration_metadata_manifest WHERE plan_id=$1 AND id>$2 AND ($3::text IS NULL OR disposition=$3)").bind(s.plan).bind(after).bind(filter).fetch_one(&mut *conn).await.unwrap();
        let p = explain!(
            checks,
            conn,
            label,
            QUERIES,
            "SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND id>$4",
            s.plan,
            s.child,
            s.f.org,
            after,
            filter,
            51_i64
        );
        let bound = if filter.is_none() {
            remaining.min(51)
        } else {
            remaining
        };
        checks.relation(&p, "migration_metadata_manifest", bound as f64, true);
        checks.returned(&p, expected);
    }
    let p = explain!(
        checks,
        conn,
        "record_field_payload_point",
        QUERIES,
        "SELECT * FROM migration_metadata_manifest WHERE id=$1 AND plan_id=$2 AND import_id=$3",
        manifest,
        s.plan,
        s.child,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_manifest", 1.0, true);
    let p = explain!(
        checks,
        conn,
        "execution_manifest_deep",
        WORKER,
        "SELECT m.* FROM migration_metadata_manifest m WHERE",
        s.plan,
        s.f.org,
        s.manifest_deep
    );
    checks.relation(&p, "migration_metadata_manifest", 1.0, true);
    checks.returned(&p, 1);
    let p=explain!(checks,conn,"execution_mapping_cursor",WORKER,"SELECT kind,source_sequence,source_ordinal,element_ordinal,id FROM migration_metadata_mapping WHERE",tag,s.plan,s.f.org);
    checks.relation(&p, "migration_metadata_mapping", 1.0, true);
    let p = explain!(
        checks,
        conn,
        "execution_mapping_replay_skip",
        WORKER,
        "SELECT m.* FROM migration_metadata_mapping m WHERE m.plan_id=$1",
        s.plan,
        s.f.org,
        s.child,
        "option",
        1251_i64,
        9_i32,
        50_i32,
        s.execution_mapping_fifty
    );
    checks.relation(&p, "migration_metadata_mapping", 72.0, true);
    checks.relation(&p, "migration_metadata_result", 72.0, true);
    checks.returned(&p, 1);
    let p = explain!(
        checks,
        conn,
        "execution_option_children",
        WORKER,
        "SELECT * FROM migration_metadata_mapping WHERE parent_mapping_id=$1",
        choice_field,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_mapping", 50.0, true);
    checks.returned(&p, 50);
    for (label, after, kind, disposition) in [
        ("results_first", nil, None, None),
        ("results_deep", s.result_deep, None, None),
        ("results_people_first", nil, Some("people"), None),
        ("results_people_deep", s.result_deep, Some("people"), None),
        (
            "results_people_applied",
            nil,
            Some("people"),
            Some("applied"),
        ),
        ("results_sparse_held", nil, Some("people"), Some("held")),
        (
            "results_sparse_held_deep",
            s.result_deep,
            Some("people"),
            Some("held"),
        ),
        (
            "results_option_created",
            nil,
            Some("option"),
            Some("created"),
        ),
        (
            "results_empty_source_null",
            s.result_deep,
            None,
            Some("source_null"),
        ),
        (
            "results_empty_people_tail",
            s.result_empty_held,
            Some("people"),
            Some("held"),
        ),
    ] {
        let remaining = tail(&mut conn, "migration_metadata_result", s.plan, after).await;
        let expected:i64=sqlx::query_scalar("SELECT LEAST(count(*),51) FROM migration_metadata_result WHERE plan_id=$1 AND id>$2 AND ($3::text IS NULL OR kind=$3) AND ($4::text IS NULL OR disposition=$4)").bind(s.plan).bind(after).bind(kind).bind(disposition).fetch_one(&mut *conn).await.unwrap();
        let p=explain!(checks,conn,label,QUERIES,"SELECT id FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 AND plan_id=$3",s.child,s.f.org,s.plan,after,kind,disposition,51_i64);
        let bound = if kind.is_none() && disposition.is_none() {
            remaining.min(51)
        } else {
            remaining
        };
        checks.relation(&p, "migration_metadata_result", bound as f64, true);
        checks.returned(&p, expected);
    }
    let p = explain!(
        checks,
        conn,
        "result_field_payload_point",
        QUERIES,
        "SELECT * FROM migration_metadata_result WHERE id=$1 AND import_id=$2",
        result,
        s.child,
        s.f.org
    );
    checks.relation(&p, "migration_metadata_result", 1.0, true);
    let provenance_after:Uuid=sqlx::query_scalar("SELECT id FROM migration_metadata_result WHERE organization_id=$1 AND person_id=$2 ORDER BY id OFFSET 200 LIMIT 1").bind(s.f.org).bind(s.person).fetch_one(&mut *conn).await.unwrap();
    for (label, after) in [
        ("provenance_first", nil),
        ("provenance_deep", provenance_after),
    ] {
        let p=explain!(checks,conn,label,QUERIES,"SELECT id,import_id,plan_id FROM migration_metadata_result WHERE organization_id=$1 AND person_id=$2",s.f.org,s.person,after,51_i64);
        checks.relation(&p, "migration_metadata_result", 51.0, true);
        checks.returned(&p, 51);
    }
    let p=explain!(checks,conn,"provenance_field_payload_point",QUERIES,"SELECT * FROM migration_metadata_result WHERE id=$1 AND organization_id=$2 AND person_id=$3",result,s.f.org,s.person);
    checks.relation(&p, "migration_metadata_result", 1.0, true);
    let p = explain!(
        checks,
        conn,
        "provenance_live_person_scope",
        QUERIES,
        "SELECT EXISTS(SELECT 1 FROM person WHERE id=$1 AND organization_id=$2)",
        s.person,
        s.f.org
    );
    checks.relation(&p, "person", 1.0, true);
    let p = explain!(
        checks,
        conn,
        "issue_first_page",
        QUERIES,
        "SELECT code,record_count FROM migration_metadata_issue WHERE plan_id=$1 AND import_id=$2",
        s.plan,
        s.child,
        s.f.org,
        "",
        51_i64
    );
    checks.relation(&p, "migration_metadata_issue", 64.0, false);
    checks.returned(&p, 51);
    let p = explain!(
        checks,
        conn,
        "issue_deep_page",
        QUERIES,
        "SELECT code,record_count FROM migration_metadata_issue WHERE plan_id=$1 AND import_id=$2",
        s.plan,
        s.child,
        s.f.org,
        "synthetic_issue_55",
        51_i64
    );
    checks.relation(&p, "migration_metadata_issue", 64.0, false);
    checks.returned(&p, 5);
    let p = explain!(
        checks,
        conn,
        "native_person_tag_count",
        WORKER,
        "SELECT count(*) FROM person_tag WHERE organization_id=$1 AND person_id=$2",
        s.f.org,
        s.person
    );
    checks.relation(&p, "person_tag", 20.0, true);
    let p=explain!(checks,conn,"native_person_tag_exists",WORKER,"SELECT EXISTS(SELECT 1 FROM person_tag WHERE organization_id=$1 AND person_id=$2 AND tag_id=$3)",s.f.org,s.person,tag_target);
    checks.relation(&p, "person_tag", 1.0, true);
    let p=explain!(checks,conn,"native_person_value_exact",WORKER,"SELECT text_value IS NOT DISTINCT FROM $4 AND number_value IS NOT DISTINCT FROM CAST($5::text AS numeric)",s.f.org,s.person,field_target,Some("Synthetic value"),Option::<String>::None,Option::<chrono::NaiveDate>::None,Option::<Uuid>::None);
    checks.relation(&p, "person_custom_field_value", 1.0, true);
    checks.returned(&p, 1);
    // Small native catalogs may validly use a bounded sequential scan.
    let p = explain!(
        checks,
        conn,
        "native_tag_label",
        WORKER,
        "SELECT id FROM tag WHERE organization_id=$1 AND lower(name)=$2",
        s.f.org,
        "synthetic tag 11"
    );
    checks.relation(&p, "tag", 200.0, false);
    checks.returned(&p, 1);
    let p=explain!(checks,conn,"native_field_label",WORKER,"SELECT id FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL AND lower(label)=$2",s.f.org,"synthetic field 1");
    checks.relation(&p, "custom_field", 50.0, false);
    let p=explain!(checks,conn,"native_option_label",WORKER,"SELECT id FROM custom_field_option WHERE organization_id=$1 AND field_id=$2 AND archived_at IS NULL AND lower(label)=$3",s.f.org,choice_target,"synthetic option 21");
    checks.relation(&p, "custom_field_option", 50.0, false);
    let p=explain!(checks,conn,"native_machine_key",WORKER,"SELECT EXISTS(SELECT 1 FROM custom_field WHERE organization_id=$1 AND source='fub' AND external_key=$2)",s.f.org,"syntheticField1");
    checks.relation(&p, "custom_field", 50.0, false);
    let p = explain!(
        checks,
        conn,
        "native_option_exact_destination",
        WORKER,
        "SELECT EXISTS(SELECT 1 FROM custom_field_option o JOIN custom_field f",
        option_target,
        s.f.org,
        "Synthetic Option 21",
        choice_target
    );
    checks.relation(&p, "custom_field_option", 50.0, false);
    checks.relation(&p, "custom_field", 50.0, false);
    let p = explain!(
        checks,
        conn,
        "frozen_target_tags",
        COMMANDS,
        "SELECT id,name FROM tag WHERE organization_id=$1 ORDER BY id LIMIT 201",
        s.f.org
    );
    checks.relation(&p, "tag", 200.0, false);
    let p=explain!(checks,conn,"frozen_target_fields",COMMANDS,"SELECT id,label,field_type,source,external_key,position FROM custom_field WHERE organization_id=$1",s.f.org);
    checks.relation(&p, "custom_field", 50.0, false);
    let p = explain!(
        checks,
        conn,
        "frozen_target_options",
        COMMANDS,
        "SELECT o.id,o.field_id,o.label,o.position FROM custom_field_option o JOIN custom_field f",
        s.f.org
    );
    checks.relation(&p, "custom_field_option", 50.0, false);
    // A nested-loop join may inspect the same bounded native field per option.
    checks.relation(&p, "custom_field", 2500.0, false);
    let (original, sql) = operation_guard();
    println!(
        "CRM_010F1_METADATA_GUARD_SOURCE {}",
        json!({"source":"20260919000001_fub_metadata_import.sql:crm_metadata_insert_allowed","sql":original,"sql_sha256":sha(&original),"bound_sql_sha256":sha(&sql),"substitutions":{"plan":"$1 uuid","run":"$2 uuid","org":"$3 uuid","unit":"$4 uuid","row_value":"$5 jsonb","table_name":"$6 text"}})
    );
    for (label, table, target_name, target) in [
        (
            "operation_permit_tag_point",
            "person_tag",
            "tag_id",
            tag_target,
        ),
        (
            "operation_permit_value_point",
            "person_custom_field_value",
            "field_id",
            field_target,
        ),
        (
            "operation_permit_missing_target",
            "person_tag",
            "tag_id",
            Uuid::nil(),
        ),
    ] {
        let data = json!({"person_id":s.person,target_name:target});
        let wrapped = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let p: Value = sqlx::query_scalar(&wrapped)
            .bind(s.plan)
            .bind(s.child)
            .bind(s.f.org)
            .bind(manifest)
            .bind(&data)
            .bind(table)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        checks.plan(
            label,
            &sql,
            json!([s.plan, s.child, s.f.org, manifest, data, table]),
            &p,
        );
        checks.relation(&p, "migration_metadata_manifest", 1.0, true);
        checks.relation(&p, "migration_metadata_operation", 70.0, true);
        checks.returned(&p, 1);
    }
    checks.finish();
}

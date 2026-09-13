//! D-050 one-shot 25k-People query-shape evidence, not executable import proof.
//! A tiny parent/report/refresh first completes through real commands. Thereafter
//! migrator-only INERT clones increase cardinality; copied ciphertext is not
//! rebound to new AAD, and no domain reader/decryptor/worker runs after seeding.
//! Source literals and hashes accompany every EXPLAIN; no planner toggles or
//! laptop latency caps. This is separate from byte-accounting/fidelity tests.
use crate::db_people_refresh_execution as execution;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;
const WORKER: &str = include_str!("../../crm-app/src/domain/migration/people_refresh_worker.rs");
const QUERIES: &str = include_str!("../../crm-app/src/domain/migration/people_refresh_queries.rs");
fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}
fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let values = source
        .match_indices(&marker)
        .map(|(start, _)| {
            serde_json::Deserializer::from_str(&source[start..].replace('\n', "\\n"))
                .into_iter::<String>()
                .next()
                .unwrap()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(!values.is_empty(), "missing production SQL: {prefix}");
    assert!(
        values.iter().all(|v| v == &values[0]),
        "ambiguous production SQL: {prefix}"
    );
    values[0].clone()
}
fn nodes<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(map) => {
            if map.contains_key("Node Type") {
                out.push(v);
            }
            for v in map.values() {
                nodes(v, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                nodes(v, out)
            }
        }
        _ => {}
    }
}
fn record(
    label: &str,
    sql: &str,
    binds: Value,
    plan: &Value,
    output: &mut Vec<Value>,
    require_index: &[&str],
) {
    let mut all = vec![];
    nodes(plan, &mut all);
    let hash = Sha256::digest(sql.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut violations = vec![];
    for n in &all {
        if n["Temp Written Blocks"].as_u64().unwrap_or(0) > 0 || n["Sort Space Type"] == "Disk" {
            violations.push("disk spill".to_owned());
        }
    }
    for table in require_index {
        let matches = all
            .iter()
            .filter(|n| n["Relation Name"] == *table)
            .collect::<Vec<_>>();
        if matches.is_empty()
            || matches.iter().any(|n| {
                !n["Node Type"].as_str().unwrap_or("").contains("Index")
                    && n["Node Type"] != "Bitmap Heap Scan"
            })
        {
            violations.push(format!("missing index access: {table}"));
        }
    }
    output.push(json!({"label":label,"sql":sql,"sql_sha256":hash,"bindings":binds,"plan":plan,"violations":violations}));
}
macro_rules! explain {
    ($out:expr,$conn:expr,$label:expr,$source:expr,$prefix:expr,$tables:expr $(,$bind:expr)* $(,)?) => {{
        let sql=statement($source,$prefix); let wrapped=format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan:Value=sqlx::query_scalar(&wrapped)$(.bind($bind))* .fetch_one(&mut *$conn).await.unwrap();
        record($label,&sql,json!([$($bind),*]),&plan,$out,$tables);
    }};
}

#[sqlx::test]
#[ignore = "one explicit isolated D-050 25k plan collection"]
async fn people_refresh_25k_plan_shapes(migrator: PgPool) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let (run, detail) = execution::ready(
        &f,
        parent,
        vec![
            json!({"id":101,"firstName":"Refreshed One"}),
            json!({"id":102,"firstName":"Refreshed Two"}),
        ],
    )
    .await;
    execution::confirm(&f, run, &execution::confirmation(&detail)).await;
    execution::drain(&f, run).await;
    let plan = uuid(&detail["plan"]["id"]);
    let base=sqlx::query("SELECT i.id item_id,i.person_id,i.original_result_id,r.report_id FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id WHERE i.refresh_id=$1 AND i.source_id='101'").bind(run).fetch_one(&migrator).await.unwrap();
    let item: Uuid = base.get("item_id");
    let person: Uuid = base.get("person_id");
    let original: Uuid = base.get("original_result_id");
    let report: Uuid = base.get("report_id");
    let mut conn = migrator.acquire().await.unwrap();
    sqlx::query("CREATE TEMP TABLE refresh_scale_members AS SELECT n,gen_random_uuid() id FROM generate_series(1,48)n").execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO app_user SELECT (jsonb_populate_record(NULL::app_user,to_jsonb(u)||jsonb_build_object('id',s.id,'email','refresh-scale-'||s.n||'@synthetic.test'))).* FROM (SELECT * FROM app_user WHERE id=$1)u CROSS JOIN refresh_scale_members s").bind(f.member).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO organization_membership SELECT (jsonb_populate_record(NULL::organization_membership,to_jsonb(m)||jsonb_build_object('user_id',s.id))).* FROM (SELECT * FROM organization_membership WHERE organization_id=$1 AND user_id=$2)m CROSS JOIN refresh_scale_members s").bind(f.org).bind(f.member).execute(&mut *conn).await.unwrap();
    sqlx::query("CREATE TEMP TABLE refresh_scale AS SELECT n,gen_random_uuid() person_id,gen_random_uuid() result_id,gen_random_uuid() item_id FROM generate_series(100000,124997) n").execute(&mut *conn).await.unwrap();
    // Inert cardinality copies have no qualification/accounting meaning. Restore
    // both fixture-only trigger overrides immediately after the closed copy loop.
    sqlx::query("ALTER TABLE migration_core_change_group DISABLE TRIGGER migration_core_change_group_immutable").execute(&mut *conn).await.unwrap();
    sqlx::query("ALTER TABLE migration_core_change_group DISABLE TRIGGER migration_core_change_group_charge").execute(&mut *conn).await.unwrap();
    // Closed table list, fixed projection overrides. None of these clones is executable.
    for (table,lookup,patch) in [
        ("person",format!("id='{person}'"),"jsonb_build_object('id',s.person_id,'first_name','Synthetic scale '||s.n)"),
        ("migration_import_manifest",format!("id=(SELECT manifest_id FROM migration_import_result WHERE id='{original}')"),"jsonb_build_object('id',s.result_id,'source_id',s.n::text)"),
        ("migration_import_result",format!("id='{original}'"),"jsonb_build_object('id',s.result_id,'manifest_id',s.result_id,'person_id',s.person_id,'source_id',s.n::text)"),
        ("migration_people_refresh_item",format!("id='{item}'"),"jsonb_build_object('id',s.item_id,'person_id',s.person_id,'source_id',s.n::text,'source_key',s.n::text,'original_result_id',s.result_id,'settled_at',NULL,'settled_result_id',NULL,'disposition',CASE WHEN s.n%13=0 THEN 'held_evidence_gap' ELSE 'eligible' END)"),
        ("migration_core_change_group",format!("report_id='{report}' AND family='people' AND source_id='101'"),"jsonb_build_object('id',s.item_id,'source_key',s.n::text,'source_id',s.n::text)"),
        ("migration_people_refresh_result",format!("refresh_id='{run}' AND item_id='{item}'"),"jsonb_build_object('id',s.result_id,'item_id',s.item_id,'person_id',s.person_id,'source_id',s.n::text,'committed_at','2026-09-12T00:00:00Z'::timestamptz + s.n * interval '1 millisecond')"),
    ] {
        let sql=format!("INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},to_jsonb(t)||{patch})).* FROM (SELECT * FROM {table} WHERE {lookup} LIMIT 1) t CROSS JOIN refresh_scale s");
        assert_eq!(sqlx::query(&sql).execute(&mut *conn).await.unwrap().rows_affected(),24998);
    }
    sqlx::query("INSERT INTO contact_method SELECT (jsonb_populate_record(NULL::contact_method,to_jsonb(c)||jsonb_build_object('id',gen_random_uuid(),'person_id',s.person_id,'kind','email','value','scale-'||s.n||'-'||j||'@synthetic.test','normalized_value','scale-'||s.n||'-'||j||'@synthetic.test','import_order',j))).* FROM (SELECT * FROM contact_method WHERE person_id=$1 LIMIT 1)c CROSS JOIN refresh_scale s CROSS JOIN generate_series(0,3)j").bind(person).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_people_refresh_contact SELECT (jsonb_populate_record(NULL::migration_people_refresh_contact,to_jsonb(c)||jsonb_build_object('id',gen_random_uuid(),'item_id',s.item_id,'side',sides.side,'contact_id',gen_random_uuid(),'import_order',j))).* FROM (SELECT * FROM migration_people_refresh_contact WHERE item_id=$1 LIMIT 1)c CROSS JOIN refresh_scale s CROSS JOIN unnest(ARRAY['baseline','current','proposed'])sides(side) CROSS JOIN generate_series(0,3)j").bind(item).execute(&mut *conn).await.unwrap();
    sqlx::query("ALTER TABLE migration_core_change_group ENABLE TRIGGER migration_core_change_group_immutable").execute(&mut *conn).await.unwrap();
    sqlx::query(
        "ALTER TABLE migration_core_change_group ENABLE TRIGGER migration_core_change_group_charge",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    for table in [
        "person",
        "contact_method",
        "migration_import_result",
        "migration_import_manifest",
        "migration_core_change_group",
        "migration_people_refresh",
        "migration_people_refresh_item",
        "migration_people_refresh_contact",
        "migration_people_refresh_result",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let counts=sqlx::query("SELECT (SELECT count(*) FROM person WHERE organization_id=$1) people,(SELECT count(*) FROM migration_people_refresh_item WHERE refresh_id=$2) items,(SELECT count(*) FROM migration_people_refresh_contact WHERE refresh_id=$2) contacts").bind(f.org).bind(run).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(counts.get::<i64, _>("people"), 25000);
    assert_eq!(counts.get::<i64, _>("items"), 25000);
    let sampled = sqlx::query("SELECT person_id,item_id FROM refresh_scale WHERE n=112500")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let sample_person: Uuid = sampled.get("person_id");
    let sample_item: Uuid = sampled.get("item_id");
    let mut output = Vec::new();
    explain!(
        &mut output,
        conn,
        "imported descriptor keyset",
        WORKER,
        "SELECT ir.*,i.snapshot_id",
        &["migration_import_result"],
        parent,
        f.org,
        "112000"
    );
    explain!(
        &mut output,
        conn,
        "bounded raw report descriptor page",
        WORKER,
        "SELECT g.source_key,g.source_id,g.disposition",
        &["migration_core_change_group"],
        report,
        f.org,
        "112000"
    );
    explain!(
        &mut output,
        conn,
        "exact imported membership per descriptor",
        WORKER,
        "SELECT EXISTS(SELECT 1 FROM migration_import_result",
        &["migration_import_result"],
        parent,
        f.org,
        "112500"
    );
    explain!(
        &mut output,
        conn,
        "native per-Person projection",
        WORKER,
        "SELECT p.first_name,p.last_name,p.stage_id,p.assigned_user_id",
        &["person", "contact_method"],
        sample_person,
        f.org
    );
    explain!(
        &mut output,
        conn,
        "preview first page",
        QUERIES,
        "SELECT id,source_id,person_id,disposition,name_clear_count",
        &["migration_people_refresh_item"],
        run,
        f.org,
        plan,
        Uuid::nil(),
        Option::<String>::None,
        51_i64
    );
    explain!(
        &mut output,
        conn,
        "preview filtered page",
        QUERIES,
        "SELECT id,source_id,person_id,disposition,name_clear_count",
        &["migration_people_refresh_item"],
        run,
        f.org,
        plan,
        sample_item,
        Some("held_evidence_gap"),
        51_i64
    );
    explain!(
        &mut output,
        conn,
        "contact page with exact-side existence probes",
        QUERIES,
        "SELECT c.*,CASE c.side",
        &["migration_people_refresh_contact"],
        run,
        f.org,
        sample_item,
        Uuid::nil(),
        51_i64
    );
    let bound=sqlx::query("SELECT committed_at,id FROM migration_people_refresh_result WHERE refresh_id=$1 ORDER BY committed_at DESC,id DESC LIMIT 1").bind(run).fetch_one(&mut *conn).await.unwrap();
    let upper: DateTime<Utc> = bound.get("committed_at");
    let upper_id: Uuid = bound.get("id");
    explain!(
        &mut output,
        conn,
        "settlement upper key",
        QUERIES,
        "SELECT id,committed_at FROM migration_people_refresh_result",
        &["migration_people_refresh_result"],
        run,
        f.org
    );
    explain!(
        &mut output,
        conn,
        "settlement first page",
        QUERIES,
        "SELECT id,item_id,person_id,source_id,disposition,committed_at",
        &["migration_people_refresh_result"],
        run,
        f.org,
        upper,
        upper_id,
        Option::<DateTime<Utc>>::None,
        Option::<Uuid>::None,
        51_i64
    );
    // Explain the actual lock/claim statement inside a rolled-back transaction.
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    explain!(
        &mut output,
        conn,
        "one unsettled Person claim",
        WORKER,
        "SELECT * FROM migration_people_refresh_item WHERE refresh_id",
        &["migration_people_refresh_item"],
        run,
        f.org,
        plan
    );
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
    let evidence = json!({"synthetic_only":true,"inert_scale_clones":true,"people":25000,"members":50,"preview_items":25000,"contact_comparison_rows":counts.get::<i64,_>("contacts"),"plans":output});
    let path = std::path::Path::new("/private/tmp/crm-010e2-qa/plan-shapes.json");
    std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    println!(
        "People refresh 25k source-literal plan evidence: {}",
        path.display()
    );
    assert!(
        evidence["plans"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["violations"].as_array().unwrap().is_empty()),
        "inspect plan-shapes.json violations"
    );
}

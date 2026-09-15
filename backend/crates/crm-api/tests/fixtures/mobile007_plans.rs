//! Opt-in Mobile007 discovery plans on the D-050 25k-Person/50-member book.
//! The exact production SQL is explained; the fixture never claims provider or
//! production-host capacity.
use crm_api::domain::person::discovery::DISCOVERY_SQL;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

const DISCOVERY_SOURCE: &str = include_str!("../../../crm-app/src/domain/person/discovery.rs");

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn nodes<'a>(value: &'a Value, output: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("Node Type") {
                output.push(value);
            }
            for child in fields.values() {
                nodes(child, output);
            }
        }
        Value::Array(items) => {
            for child in items {
                nodes(child, output);
            }
        }
        _ => {}
    }
}

fn relation_work(plan: &Value, table: &str) -> (f64, bool, bool) {
    let mut all = Vec::new();
    nodes(plan, &mut all);
    let mut examined = 0.0;
    let mut indexed = false;
    let mut sequential = false;
    for node in all {
        if node["Relation Name"] == table {
            examined += (node["Actual Rows"].as_f64().unwrap_or(0.0)
                + node["Rows Removed by Filter"].as_f64().unwrap_or(0.0))
                * node["Actual Loops"].as_f64().unwrap_or(1.0);
            indexed |= node["Index Name"].is_string();
            sequential |= node["Node Type"] == "Seq Scan";
        }
    }
    (examined, indexed, sequential)
}

fn assert_no_spill(plan: &Value, label: &str) {
    let mut all = Vec::new();
    nodes(plan, &mut all);
    assert!(
        all.into_iter().all(|node| {
            node["Sort Space Type"] != "Disk"
                && node["Temp Read Blocks"].as_u64().unwrap_or(0) == 0
                && node["Temp Written Blocks"].as_u64().unwrap_or(0) == 0
                && node["Hash Batches"].as_u64().unwrap_or(1) <= 1
        }),
        "{label}: discovery plan spilled"
    );
}

async fn explain(
    conn: &mut PgConnection,
    label: &str,
    org: Uuid,
    pattern: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> Value {
    let plan: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {DISCOVERY_SQL}"
    ))
    .bind(org)
    .bind(pattern)
    .bind(email)
    .bind(phone)
    .fetch_one(&mut *conn)
    .await
    .unwrap_or_else(|error| panic!("{label}: EXPLAIN failed: {error}"));
    assert!(
        plan[0]["Plan"]["Actual Rows"]
            .as_f64()
            .unwrap_or(f64::INFINITY)
            <= 26.0,
        "{label}: result cap"
    );
    assert_no_spill(&plan, label);
    plan
}

#[sqlx::test]
#[ignore = "opt-in Mobile007 D-050 discovery plan evidence"]
async fn mobile007_discovery_25k_people_50_members(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_MOBILE007_PLANS_OUTPUT").expect("absolute evidence output"),
    );
    assert!(
        output.is_absolute() && !output.exists(),
        "preserve earlier evidence"
    );
    let (org, actor) = crate::common::create_org_with_stages_and_member(
        &migrator,
        "Mobile007 discovery plans",
        "mobile007-plan@example.test",
        "Mobile007 plan actor",
        "mobile007-plan-password",
    )
    .await;
    let app = crate::common::connect_as_app(&migrator).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position,id LIMIT 1",
    )
    .bind(org)
    .fetch_one(&app)
    .await
    .unwrap();

    // Synthetic bulk cardinality only. The Organization/member roots above use
    // the normal fixture command path; production triggers remain enabled.
    let mut conn = migrator.acquire().await.unwrap();
    sqlx::query("CREATE TEMP TABLE mobile007_members AS SELECT n,gen_random_uuid() AS id FROM generate_series(2,50)n")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'mobile007-plan-'||n||'@synthetic.test','Mobile007 member '||n FROM mobile007_members")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM mobile007_members")
        .bind(org)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE mobile007_people AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,25000)n")
        .execute(&mut *conn)
        .await
        .unwrap();
    let members: Vec<Uuid> = sqlx::query_scalar("SELECT user_id FROM organization_membership WHERE organization_id=$1 AND status='active' ORDER BY user_id")
        .bind(org)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT id,$1,'Discovery','Prospect '||lpad(n::text,5,'0'),$2,($3::uuid[])[1+((n-1)%50)] FROM mobile007_people")
        .bind(org)
        .bind(stage)
        .bind(&members)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,import_order) SELECT $1,id,'email','discovery-'||n||'@synthetic.test','discovery-'||n||'@synthetic.test',1 FROM mobile007_people")
        .bind(org)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,import_order) SELECT $1,id,'phone','+1555'||lpad(n::text,7,'0'),'+1555'||lpad(n::text,7,'0'),2 FROM mobile007_people")
        .bind(org)
        .execute(&mut *conn)
        .await
        .unwrap();
    for table in [
        "organization_membership",
        "person",
        "contact_method",
        "stage",
        "app_user",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let counts: Value = sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'),'contacts',(SELECT count(*) FROM contact_method WHERE organization_id=$1))")
        .bind(org)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        counts,
        json!({"people":25000,"members":50,"contacts":50000})
    );
    assert!(members.contains(&actor));

    let name = explain(&mut conn, "name_fragment", org, "%Discovery%", None, None).await;
    let email_term = "discovery-25000@synthetic.test";
    let email = explain(
        &mut conn,
        "exact_email",
        org,
        &format!("%{email_term}%"),
        Some(email_term),
        None,
    )
    .await;
    let (name_people, _, _) = relation_work(&name, "person");
    let (name_contacts, name_contact_index, name_contact_seq) =
        relation_work(&name, "contact_method");
    let (email_people, _, _) = relation_work(&email, "person");
    let (email_contacts, email_contact_index, email_contact_seq) =
        relation_work(&email, "contact_method");
    assert!(name_people <= 25_000.0 && email_people <= 25_000.0);
    assert!(
        name_contact_index && !name_contact_seq && name_contacts <= 52.0,
        "name discovery must bound the two primary-contact lookups: {name_contacts}"
    );
    assert!(
        email_contact_index && !email_contact_seq && email_contacts <= 25_052.0,
        "exact-contact discovery must stay indexed and at most linear: {email_contacts}"
    );
    let evidence = json!({
        "protocol":"mobile007-d050-discovery-plans-v1",
        "scope":"Exact production discovery SQL on a synthetic 25k-Person/50-member Organization; laptop plan shape only",
        "fixture_counts":counts,
        "sql":DISCOVERY_SQL,
        "sql_sha256":sha256(DISCOVERY_SQL.as_bytes()),
        "source_sha256":sha256(DISCOVERY_SOURCE.as_bytes()),
        "plans":{
            "name_fragment":{"plan":name,"person_examined":name_people,"contact_examined":name_contacts},
            "exact_email":{"plan":email,"person_examined":email_people,"contact_examined":email_contacts}
        }
    });
    std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}

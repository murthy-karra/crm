//! Opt-in, single-pass M6-09 production SQL plans on a populated synthetic book.
use super::*;
use sha2::{Digest, Sha256};

const GENERATIONS: &str = include_str!("../../../crm-app/src/domain/mobile/generations.rs");
const MOBILE: &str = include_str!("../../../crm-app/src/domain/mobile/mod.rs");
const COMPOSER: &str = include_str!("../../../crm-app/src/domain/person_metadata.rs");

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// Read the actual Rust SQL literal. A renamed query fails this inventory instead
// of silently measuring a test approximation. None of these literals is escaped.
fn sql<'a>(source: &'a str, prefix: &str) -> &'a str {
    let start = source
        .find(&format!("\"{prefix}"))
        .expect("production SQL literal")
        + 1;
    let end = source[start..].find('"').unwrap() + start;
    &source[start..end]
}

fn nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(node);
    if let Some(children) = node["Plans"].as_array() {
        for child in children {
            nodes(child, out);
        }
    }
}

pub(super) async fn run(pool: &PgPool, f: Fixture) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_MOBILE006_PLANS_OUTPUT").expect("absolute evidence file"),
    );
    assert!(
        output.is_absolute() && !output.exists(),
        "preserve prior evidence"
    );
    for n in 0..48 {
        let user = crate::common::create_user(
            pool,
            &format!("metadata-plan-{n}@fixture.test"),
            "Plan member",
            PW,
        )
        .await;
        crate::common::add_membership_with(
            pool,
            f.org,
            user,
            Role::Member,
            MembershipStatus::Active,
        )
        .await;
    }
    let members: Vec<Uuid> = sqlx::query_scalar("SELECT user_id FROM organization_membership WHERE organization_id=$1 AND status='active' ORDER BY user_id").bind(f.org).fetch_all(&f.app).await.unwrap();
    assert_eq!(members.len(), 50);
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Metadata plan',p.stage_id,($2::uuid[])[1+((n-1)%50)] FROM person p CROSS JOIN generate_series(1,24999) n WHERE p.id=$3")
        .bind(f.org).bind(&members).bind(f.person).execute(&f.app).await.unwrap();
    let tags: Vec<Uuid> = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) SELECT $1,$2,'Plan tag '||n FROM generate_series(1,200) n RETURNING id").bind(f.org).bind(f.actor).fetch_all(&f.app).await.unwrap();
    let text: Vec<Uuid> = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) SELECT $1,'Plan text '||n,'text',n,$2 FROM generate_series(1,47) n RETURNING id").bind(f.org).bind(f.actor).fetch_all(&f.app).await.unwrap();
    let choices: Vec<Uuid> = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) SELECT $1,'Plan choice '||n,'choice',47+n,$2 FROM generate_series(1,3) n RETURNING id").bind(f.org).bind(f.actor).fetch_all(&f.app).await.unwrap();
    sqlx::query("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id,archived_at) SELECT $1,'Plan archived '||n,'text',50+n,$2,now() FROM generate_series(1,75) n").bind(f.org).bind(f.actor).execute(&f.app).await.unwrap();
    for field in &choices {
        sqlx::query("INSERT INTO custom_field_option(organization_id,field_id,label,position) SELECT $1,$2,'Option '||n,n FROM generate_series(1,50) n").bind(f.org).bind(field).execute(&f.app).await.unwrap();
    }
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) SELECT $1,id,$2,$3 FROM person WHERE organization_id=$1").bind(f.org).bind(tags[0]).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) SELECT $1,$2,unnest($3::uuid[]),$4").bind(f.org).bind(f.person).bind(&tags[1..20]).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) SELECT $1,id,$2,'text','Plan value',$3,'mobile_session',gen_random_uuid() FROM person WHERE organization_id=$1").bind(f.org).bind(text[0]).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) SELECT $1,$2,unnest($3::uuid[]),'text','Plan value',$4,'mobile_session',gen_random_uuid()").bind(f.org).bind(f.person).bind(&text[1..]).bind(f.actor).execute(&f.app).await.unwrap();
    for field in &choices {
        sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,option_id,updated_by_user_id,origin,correlation_id) SELECT $1,$2,$3,'choice',id,$4,'mobile_session',gen_random_uuid() FROM custom_field_option WHERE field_id=$3 ORDER BY id LIMIT 1").bind(f.org).bind(f.person).bind(field).bind(f.actor).execute(&f.app).await.unwrap();
    }
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM person_tag WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let values: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert_eq!((people, links, values), (25000, 25019, 25049));
    let (status, generation) = f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person],"include_metadata":true})).await;
    assert_eq!(status, StatusCode::OK, "{generation}");
    let generation_id = id(&generation, "generation_id");
    // Planner statistics belong to this throwaway fixture, not a shared service.
    sqlx::query("ANALYZE").execute(pool).await.unwrap();
    enum Bind {
        Org,
        Actor,
        Person,
        Gen,
        GenPerson,
        Page,
        Selection,
    }
    let statements = [
        ("membership", COMPOSER, "SELECT role FROM organization_membership", Bind::Actor, 1),
        ("person_token", COMPOSER, "SELECT metadata_revision FROM person", Bind::Person, 1),
        ("command_catalog_token", COMPOSER, "SELECT revision FROM mobile_metadata_catalog", Bind::Org, 1),
        ("generation_catalog_token", GENERATIONS, "SELECT crm_mobile_metadata_catalog_revision", Bind::Org, 1),
        ("current_metadata", MOBILE, "SELECT p.mobile_revision,p.metadata_revision,", Bind::Person, 1),
        ("person_tags", MOBILE, "SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'name',name)", Bind::Person, 1),
        ("person_values", MOBILE, "SELECT COALESCE(jsonb_agg(jsonb_build_object('field_id',field_id", Bind::Person, 1),
        ("catalog_tag_snapshot", GENERATIONS, "SELECT id,name FROM tag", Bind::Org, 200),
        ("catalog_field_snapshot", GENERATIONS, "SELECT id,label,field_type,position,archived_at FROM custom_field", Bind::Org, 125),
        ("catalog_option_snapshot", GENERATIONS, "SELECT id,field_id,label,position,archived_at FROM custom_field_option", Bind::Org, 150),
        ("catalog_tag_page", GENERATIONS, "SELECT tag_id AS id,jsonb_build_object", Bind::Page, 101),
        ("catalog_field_page", GENERATIONS, "SELECT field_id AS id,jsonb_build_object", Bind::Page, 101),
        ("catalog_option_page", GENERATIONS, "SELECT option_id AS id,jsonb_build_object", Bind::Page, 101),
        ("selection", GENERATIONS, "SELECT id,mobile_revision,metadata_revision,assigned_user_id", Bind::Selection, 501),
        ("manifest", GENERATIONS, "SELECT person_id,revision,metadata_revision,reasons FROM mobile_reconciliation_person WHERE generation_id=$1 AND", Bind::Page, 251),
        ("seal_members", GENERATIONS, "SELECT person_id,revision,metadata_revision,reasons FROM mobile_reconciliation_person WHERE generation_id=$1 ORDER", Bind::Gen, 501),
        ("component_metadata_token", GENERATIONS, "SELECT metadata_revision FROM mobile_reconciliation_person", Bind::GenPerson, 1),
    ];
    let mut evidence = Vec::new();
    let mut failures = Vec::new();
    for (name, source, prefix, bind, max_rows) in statements {
        let statement = sql(source, prefix);
        let explain = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {statement}");
        let query = sqlx::query_scalar::<_, Value>(&explain);
        let query = match bind {
            Bind::Org => query.bind(f.org),
            Bind::Actor => query.bind(f.org).bind(f.actor),
            Bind::Person => query.bind(f.org).bind(f.person),
            Bind::Gen => query.bind(generation_id),
            Bind::GenPerson => query.bind(generation_id).bind(f.person),
            Bind::Page => query.bind(generation_id).bind(None::<Uuid>),
            Bind::Selection => query
                .bind(f.org)
                .bind(f.actor)
                .bind(vec![f.person])
                .bind(Vec::<Uuid>::new()),
        };
        match query.fetch_one(&f.app).await {
            Ok(plan) => {
                if plan[0]["Plan"]["Actual Rows"]
                    .as_f64()
                    .unwrap_or(f64::INFINITY)
                    > max_rows as f64
                {
                    failures.push(format!("{name}: output exceeded fixture bound"));
                }
                let mut all = Vec::new();
                nodes(&plan[0]["Plan"], &mut all);
                // Point metadata reads must use the Person key, not scan the
                // 25k-wide value/link tables. Small catalogs may scan cheaply.
                for node in all {
                    if matches!(
                        node["Relation Name"].as_str(),
                        Some("person_tag" | "person_custom_field_value")
                    ) {
                        let examined = (node["Actual Rows"].as_f64().unwrap_or(0.)
                            + node["Rows Removed by Filter"].as_f64().unwrap_or(0.))
                            * node["Actual Loops"].as_f64().unwrap_or(1.);
                        if examined > 101. || node["Node Type"] == "Seq Scan" {
                            failures.push(format!("{name}: unbounded metadata relation access"));
                        }
                    }
                }
                evidence.push(json!({"name":name,"sql":statement,"sql_sha256":hash(statement.as_bytes()),"source_sha256":hash(source.as_bytes()),"expected_max_output":max_rows,"plan":plan}));
            }
            Err(error) => {
                failures.push(format!("{name}: {error}"));
            }
        }
    }
    // EXPLAIN of a scalar SQL function hides its internal plan. Inspect its
    // exact catalog row-lock body separately as its owning migrator role.
    let function: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef('crm_mobile_metadata_catalog_revision(uuid)'::regprocedure)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(function.contains("WHERE organization_id=p_organization_id FOR SHARE"));
    let internal =
        "SELECT revision FROM public.mobile_metadata_catalog WHERE organization_id=$1 FOR SHARE";
    let plan: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {internal}"
    ))
    .bind(f.org)
    .fetch_one(pool)
    .await
    .unwrap();
    evidence.push(json!({"name":"catalog_token_function_body","sql":internal,"role":"crm_migrator (function owner)","function_definition_sha256":hash(function.as_bytes()),"plan":plan}));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let report = json!({"fixture":{"people":people,"active_members":members.len(),"person_tags":links,"person_values":values,"tags":200,"live_fields":50,"archived_fields":75,"options":150},"role":"crm_app except explicitly identified function owner","generation":generation,"plans":evidence,"failures":failures});
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&output)
        .unwrap()
        .write_all(&serde_json::to_vec_pretty(&report).unwrap())
        .unwrap();
    assert!(
        failures.is_empty(),
        "query-plan acceptance failed: {failures:?}; evidence {}",
        output.display()
    );
    println!("M6-09 plans: {}", output.display());
}

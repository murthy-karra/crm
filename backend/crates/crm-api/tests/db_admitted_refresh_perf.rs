//! Inert 25k relation-volume EXPLAIN fixture, not source qualification or worker throughput.
//! A real retained admission/preview supplies the foreign-key roots. Synthetic
//! rows below are never decrypted or submitted. Run once in the isolated DB slot.
use crate::db_admitted_people_refresh_execution as execution;
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_people_refresh as refresh, admitted_people_refresh_worker},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

const WORKER: &str =
    include_str!("../../crm-app/src/domain/migration/admitted_people_refresh_worker.rs");
const QUERIES: &str =
    include_str!("../../crm-app/src/domain/migration/admitted_people_refresh_queries.rs");

fn statement(source: &str, prefix: &str) -> String {
    let start = source
        .find(&format!("\"{prefix}"))
        .expect("production SQL still present");
    serde_json::Deserializer::from_str(&source[start..])
        .into_iter::<String>()
        .next()
        .unwrap()
        .unwrap()
}
macro_rules! explain {
    ($pool:expr, $source:expr, $prefix:expr $(,$arg:expr)* $(,)?) => {{
        let sql = statement($source, $prefix);
        let query = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan = sqlx::query_scalar::<_,Value>(&query)$(.bind($arg))*.fetch_one($pool).await.unwrap();
        json!({"sql":sql,"plan":plan})
    }};
}
fn bounded_index(value: &Value, table: &str, maximum: f64) {
    fn walk(value: &Value, table: &str, examined: &mut f64, indexed: &mut bool) {
        if value["Relation Name"] == table {
            *examined += (value["Actual Rows"].as_f64().unwrap_or(0.)
                + value["Rows Removed by Filter"].as_f64().unwrap_or(0.))
                * value["Actual Loops"].as_f64().unwrap_or(1.);
            *indexed |= value["Index Name"].is_string();
        }
        match value {
            Value::Object(v) => {
                for child in v.values() {
                    walk(child, table, examined, indexed);
                }
            }
            Value::Array(v) => {
                for child in v {
                    walk(child, table, examined, indexed);
                }
            }
            _ => {}
        }
    }
    let (mut examined, mut indexed) = (0., false);
    walk(&value["plan"], table, &mut examined, &mut indexed);
    assert!(
        indexed && examined <= maximum,
        "{table}: indexed={indexed}, examined={examined}, limit={maximum}: {value}"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator and perf-harness"]
async fn admitted_refresh_25k_hot_plans(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_ADMITTED_REFRESH_PLAN_OUTPUT").expect("explicit evidence output"),
    );
    assert!(output.is_absolute());
    let (f, parent, admission) = execution::fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Plan seed","stage":"Lead"})],
    )
    .await;
    let report = execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Plan changed","stage":"Lead"})],
    )
    .await;
    let prepared = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let run = Uuid::parse_str(prepared["refresh_id"].as_str().unwrap()).unwrap();
    for _ in 0..100 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "ready" {
            break;
        }
        assert!(admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    assert_eq!(detail["state"], "ready");
    let plan = Uuid::parse_str(detail["plan"]["id"].as_str().unwrap()).unwrap();
    let admission_plan = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_admission_plan(id,admission_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) SELECT $1,$2,$3,coalesce(max(revision),0)+1,'building',$4,$5 FROM migration_people_admission_plan WHERE admission_id=$2")
        .bind(admission_plan).bind(admission).bind(f.org).bind(vec![0_u8;24]).bind(vec![0_u8;16]).execute(&migrator).await.unwrap();
    // Independent native scale rows give the requested 25k People / 50 members.
    sqlx::query("WITH members AS (SELECT gen_random_uuid() id FROM generate_series(1,48)) INSERT INTO app_user(id,email,display_name) SELECT id,id::text||'@refresh-plan.synthetic.test','Scale member' FROM members").execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM app_user WHERE email LIKE '%@refresh-plan.synthetic.test'").bind(f.org).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,stage_id,assigned_user_id,first_name) SELECT ('00000000-0000-4000-8001-'||lpad(g::text,12,'0'))::uuid,$1,$2,$3,'Plan scale' FROM generate_series(1,25000) g")
        .bind(f.org).bind(f.lead_stage).bind(f.actor).execute(&migrator).await.unwrap();
    // Copy opaque columns only for relational cardinality; these IDs do not have
    // qualified retained observations and are never offered to a worker.
    sqlx::query("INSERT INTO migration_people_admission_item SELECT (jsonb_populate_record(NULL::migration_people_admission_item,to_jsonb(seed)||jsonb_build_object('id',('00000000-0000-4000-8002-'||lpad(g::text,12,'0'))::uuid,'plan_id',$1::uuid,'source_key',(1000000+g)::text,'source_id',(1000000+g)::text,'prospective_person_id',('00000000-0000-4000-8001-'||lpad(g::text,12,'0'))::uuid,'settled_result_id',NULL,'settled_at',NULL))).* FROM (SELECT * FROM migration_people_admission_item WHERE admission_id=$2 AND disposition='settled' LIMIT 1) seed CROSS JOIN generate_series(1,25000) g")
        .bind(admission_plan).bind(admission).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_people_admission_result(id,admission_id,item_id,organization_id,person_id,source_id,disposition,actor_user_id) SELECT ('00000000-0000-4000-8003-'||lpad(g::text,12,'0'))::uuid,$1,('00000000-0000-4000-8002-'||lpad(g::text,12,'0'))::uuid,$2,('00000000-0000-4000-8001-'||lpad(g::text,12,'0'))::uuid,(1000000+g)::text,'settled',$3 FROM generate_series(1,25000) g")
        .bind(admission).bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_people_refresh_item SELECT (jsonb_populate_record(NULL::migration_admitted_people_refresh_item,to_jsonb(seed)||jsonb_build_object('id',('00000000-0000-4000-8004-'||lpad(g::text,12,'0'))::uuid,'source_key',(1000000+g)::text,'source_id',(1000000+g)::text,'person_id',('00000000-0000-4000-8001-'||lpad(g::text,12,'0'))::uuid,'admission_item_id',('00000000-0000-4000-8002-'||lpad(g::text,12,'0'))::uuid,'admission_result_id',('00000000-0000-4000-8003-'||lpad(g::text,12,'0'))::uuid,'disposition',CASE WHEN g%500=0 THEN 'eligible' ELSE 'held_evidence_gap' END,'settled_result_id',NULL,'settled_at',NULL))).* FROM (SELECT * FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 LIMIT 1) seed CROSS JOIN generate_series(1,25000) g")
        .bind(run).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_people_refresh_result(id,refresh_id,item_id,organization_id,person_id,source_id,disposition,before_nonce,before_ciphertext,after_nonce,after_ciphertext,actor_user_id) SELECT ('00000000-0000-4000-8005-'||lpad(g::text,12,'0'))::uuid,$1,('00000000-0000-4000-8004-'||lpad(g::text,12,'0'))::uuid,$2,('00000000-0000-4000-8001-'||lpad(g::text,12,'0'))::uuid,(1000000+g)::text,CASE WHEN g%1000=1 THEN 'settled' ELSE 'held_evidence_gap' END,$3,$4,$3,$4,$5 FROM generate_series(1,25000) g WHERE g%500<>0")
        .bind(run).bind(f.org).bind(vec![0_u8;24]).bind(vec![0_u8;16]).bind(f.actor).execute(&migrator).await.unwrap();
    sqlx::query("UPDATE migration_admitted_people_refresh_item i SET settled_result_id=r.id,settled_at=r.committed_at FROM migration_admitted_people_refresh_result r WHERE i.refresh_id=$1 AND r.item_id=i.id AND r.refresh_id=i.refresh_id").bind(run).execute(&migrator).await.unwrap();
    for table in [
        "person",
        "organization_membership",
        "migration_people_admission",
        "migration_people_admission_result",
        "migration_people_admission_item",
        "migration_admitted_people_refresh_item",
        "migration_admitted_people_refresh_result",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&migrator)
            .await
            .unwrap();
    }
    let members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    let people: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Plan scale'",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!((people, members), (25000, 50));
    let cohort = explain!(
        &f.pool,
        WORKER,
        "SELECT ar.id AS admission_result_id",
        admission,
        f.org,
        "1012500"
    );
    let all = explain!(&f.pool,QUERIES,"SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND id>",run,f.org,plan,Uuid::nil(),51_i64);
    let sparse = explain!(&f.pool,QUERIES,"SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND disposition=",run,f.org,plan,"eligible",Uuid::nil(),51_i64);
    let results = explain!(
        &f.pool,
        QUERIES,
        "SELECT item_id FROM migration_admitted_people_refresh_result WHERE",
        run,
        f.org,
        "settled",
        Uuid::nil(),
        51_i64
    );
    let cancelled = explain!(&f.pool,QUERIES,"SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND settled_at IS NULL",run,f.org,plan,Uuid::nil(),51_i64);
    let claim = explain!(
        &f.pool,
        WORKER,
        "SELECT * FROM migration_admitted_people_refresh_item WHERE refresh_id=",
        run,
        f.org,
        plan
    );
    let evidence = json!({"fixture":"inert relational volume; no source qualification or worker throughput claim","people":people,"members":members,"cohort":cohort,"all":all,"sparse":sparse,"results":results,"cancelled":cancelled,"claim":claim});
    std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    bounded_index(
        &evidence["cohort"],
        "migration_people_admission_result",
        50.,
    );
    for name in ["all", "sparse", "cancelled"] {
        bounded_index(
            &evidence[name],
            "migration_admitted_people_refresh_item",
            51.,
        );
    }
    bounded_index(
        &evidence["results"],
        "migration_admitted_people_refresh_result",
        51.,
    );
    bounded_index(
        &evidence["claim"],
        "migration_admitted_people_refresh_item",
        1.,
    );
}

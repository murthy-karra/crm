//! One opt-in D-050 plan-shape run for Slice 010d3 admitted history.
//! A real completed retained import supplies every authority/source root. The
//! additional rows are inert relational volume inserted with production
//! guards and charging triggers enabled; no synthetic row is decrypted or run.
use crate::db_admitted_history as base;
use chrono::{DateTime, Utc};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_history as history, history_review},
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

const WORKER: &str = include_str!("../../crm-app/src/domain/migration/admitted_history_worker.rs");
const COMMANDS: &str = include_str!("../../crm-app/src/domain/migration/admitted_history.rs");
const QUERIES: &str =
    include_str!("../../crm-app/src/domain/migration/admitted_history_queries.rs");
const REVIEW: &str = include_str!("../../crm-app/src/domain/migration/history_review.rs");

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let mut matches = source.match_indices(&marker).map(|(start, _)| {
        serde_json::Deserializer::from_str(&source[start..])
            .into_iter::<String>()
            .next()
            .expect("production SQL literal")
            .expect("decode exact production SQL")
    });
    let sql = matches
        .next()
        .unwrap_or_else(|| panic!("missing production SQL prefix: {prefix}"));
    assert!(
        matches.all(|candidate| candidate == sql),
        "ambiguous production SQL prefix: {prefix}"
    );
    sql
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum Arg {
    Uuid(Uuid),
    Bytes(Vec<u8>),
    Int(i64),
    Int32(i32),
    Small(Option<i16>),
    NullableUuid(Option<Uuid>),
    NullableText(Option<String>),
    Time(Option<DateTime<Utc>>),
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

async fn collect(
    conn: &mut PgConnection,
    label: &str,
    sql: String,
    args: Vec<Arg>,
    maximum_rows: f64,
    bounded_relation: Option<(&str, f64)>,
) -> Value {
    let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
    let mut query = sqlx::query_scalar::<_, Value>(&explain_sql);
    for arg in &args {
        query = match arg {
            Arg::Uuid(value) => query.bind(*value),
            Arg::Bytes(value) => query.bind(value),
            Arg::Int(value) => query.bind(*value),
            Arg::Int32(value) => query.bind(*value),
            Arg::Small(value) => query.bind(*value),
            Arg::NullableUuid(value) => query.bind(*value),
            Arg::NullableText(value) => query.bind(value),
            Arg::Time(value) => query.bind(*value),
        };
    }
    let plan = query
        .fetch_one(&mut *conn)
        .await
        .unwrap_or_else(|error| panic!("{label}: EXPLAIN failed: {error}"));
    assert!(
        plan[0]["Plan"]["Actual Rows"]
            .as_f64()
            .unwrap_or(f64::INFINITY)
            <= maximum_rows,
        "{label}: output exceeds bound"
    );
    let mut all = Vec::new();
    nodes(&plan, &mut all);
    assert!(
        all.iter().all(|node| {
            node["Sort Space Type"] != "Disk"
                && node["Temp Read Blocks"].as_u64().unwrap_or(0) == 0
                && node["Temp Written Blocks"].as_u64().unwrap_or(0) == 0
                && node["Hash Batches"].as_u64().unwrap_or(1) <= 1
        }),
        "{label}: temporary spill"
    );
    if let Some((table, maximum_examined)) = bounded_relation {
        let relation: Vec<_> = all
            .iter()
            .filter(|node| node["Relation Name"] == table)
            .collect();
        let examined: f64 = relation
            .iter()
            .map(|node| {
                (node["Actual Rows"].as_f64().unwrap_or(0.0)
                    + node["Rows Removed by Filter"].as_f64().unwrap_or(0.0))
                    * node["Actual Loops"].as_f64().unwrap_or(1.0)
            })
            .sum();
        assert!(
            !relation.is_empty()
                && examined <= maximum_examined
                && relation.iter().all(|node| node["Node Type"] != "Seq Scan")
                && all.iter().any(|node| node["Index Name"].is_string()),
            "{label}: {table} must use bounded indexed work; examined={examined}"
        );
    }
    let scans: Vec<Value> = all
        .into_iter()
        .filter(|node| node["Relation Name"].is_string())
        .map(|node| {
            json!({
                "table":node["Relation Name"],"node":node["Node Type"],
                "index":node["Index Name"],"loops":node["Actual Loops"],
                "rows":node["Actual Rows"],"removed":node["Rows Removed by Filter"],
                "index_condition":node["Index Cond"]
            })
        })
        .collect();
    json!({
        "label":label,"sql":sql,"sql_sha256":sha256(sql.as_bytes()),
        "bindings":args,"maximum_rows":maximum_rows,"bounded_relation":bounded_relation,
        "scans":scans,"plan":plan
    })
}

#[sqlx::test]
#[ignore = "opt-in Slice 010d3 D-050 admitted-history plan evidence"]
async fn admitted_history_hot_queries_25k_people_50_members(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_ADMITTED_HISTORY_PERF_OUTPUT")
            .expect("absolute performance evidence output"),
    );
    assert!(
        output.is_absolute() && !output.exists(),
        "preserve earlier evidence"
    );

    // Establish and finish one real retained import before creating inert plan
    // volume. This proves all copied roots descend from qualified admission and
    // capture evidence; the scaled rows are never treated as fidelity proof.
    let (fixture, admission, capture) = base::fixture(&migrator).await;
    let completed_root = base::ready(&fixture, admission, capture).await;
    let ready = history::get(&fixture.pool, &fixture.ctx, completed_root)
        .await
        .unwrap();
    let completed_plan = Uuid::parse_str(ready["latest_plan_id"].as_str().unwrap()).unwrap();
    history::confirm(
        &fixture.pool,
        &fixture.key,
        &fixture.policy,
        &fixture.ctx,
        completed_root,
        history::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: completed_plan,
            expected_revision: ready["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    base::drain(&fixture).await;
    let completed = history::get(&fixture.pool, &fixture.ctx, completed_root)
        .await
        .unwrap();
    assert_eq!(completed["state"], "completed");

    let mut conn = migrator.acquire().await.unwrap();
    let existing_people: i64 =
        sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
            .bind(fixture.org)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let existing_members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'",
    )
    .bind(fixture.org)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert!(existing_people <= 25_000 && existing_members <= 50);
    sqlx::query("CREATE TEMP TABLE h3_perf_members AS SELECT n,gen_random_uuid() AS id FROM generate_series($1::bigint,49)n")
        .bind(existing_members)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,'h3-perf-'||n||'@synthetic.test','H3 performance member '||n FROM h3_perf_members")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM h3_perf_members")
        .bind(fixture.org)
        .execute(&mut *conn)
        .await
        .unwrap();
    let members: Vec<Uuid> = sqlx::query_scalar("SELECT user_id FROM organization_membership WHERE organization_id=$1 AND status='active' ORDER BY user_id")
        .bind(fixture.org)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE h3_perf_new_people AS SELECT n,gen_random_uuid() AS id FROM generate_series(1,$1::bigint)n")
        .bind(25_000 - existing_people)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT id,$1,'H3 performance','Person '||n,$2,($3::uuid[])[1+((n-1)%50)] FROM h3_perf_new_people")
        .bind(fixture.org)
        .bind(fixture.lead_stage)
        .bind(&members)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE h3_perf_people AS SELECT row_number() OVER(ORDER BY id)::bigint n,id FROM person WHERE organization_id=$1")
        .bind(fixture.org)
        .execute(&mut *conn)
        .await
        .unwrap();

    let scale_root = Uuid::new_v4();
    let scale_plan = Uuid::new_v4();
    let scale_attempt = Uuid::new_v4();
    let lease = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_history_root SELECT (jsonb_populate_record(NULL::migration_admitted_history_root,to_jsonb(seed)||jsonb_build_object('id',$2::uuid,'state','running','phase','preparing','latest_plan_id',NULL,'confirmed_plan_id',NULL,'current_attempt_id',NULL,'lease_token',$3::uuid,'lease_expires_at',clock_timestamp()+interval '30 minutes','pause_reason',NULL,'admitted_at',NULL,'revision',1,'retained_bytes',0,'reserved_bytes',0,'confirmed_at',NULL,'completed_at',NULL,'created_at',clock_timestamp(),'updated_at',clock_timestamp()))).* FROM migration_admitted_history_root seed WHERE id=$1 AND organization_id=$4")
        .bind(completed_root).bind(scale_root).bind(lease).bind(fixture.org)
        .execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_history_plan SELECT (jsonb_populate_record(NULL::migration_admitted_history_plan,to_jsonb(seed)||jsonb_build_object('id',$2::uuid,'root_id',$3::uuid,'state','building','revision',1,'last_capture_sequence',0,'page_ordinal',0,'classified_position',0,'occurrences',0,'eligible',0,'equal_repeats',0,'held',0,'excluded',0,'unknown_dates',0,'expires_at',NULL,'created_at',clock_timestamp(),'sealed_at',NULL))).* FROM migration_admitted_history_plan seed WHERE id=$1 AND organization_id=$4")
        .bind(completed_plan).bind(scale_plan).bind(scale_root).bind(fixture.org)
        .execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_admitted_history_root SET latest_plan_id=$2 WHERE id=$1 AND organization_id=$3")
        .bind(scale_root).bind(scale_plan).bind(fixture.org).execute(&mut *conn).await.unwrap();
    // The retained-size trigger charges the same root and Organization row for
    // each inserted child. Autocommit batches preserve every production trigger
    // while avoiding one transaction's quadratic same-row version chain.
    for first in (1_i64..=25_000).step_by(500) {
        let last = (first + 499).min(25_000);
        sqlx::query("WITH seeds AS (SELECT DISTINCT ON(family) * FROM migration_admitted_history_manifest WHERE root_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY family,position) INSERT INTO migration_admitted_history_manifest SELECT (jsonb_populate_record(NULL::migration_admitted_history_manifest,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8000-'||lpad(g::text,12,'0'))::uuid,'root_id',$4::uuid,'plan_id',$5::uuid,'attempt_id',NULL,'position',g,'identity_hmac',decode(md5('h3-identity-a-'||g)||md5('h3-identity-b-'||g),'hex'),'semantic_hmac',decode(md5('h3-semantic-a-'||g)||md5('h3-semantic-b-'||g),'hex'),'person_id',person.id,'disposition',CASE WHEN g<=24950 THEN 'held' ELSE 'eligible' END,'reason',CASE WHEN g<=24950 THEN 'capture_integrity' ELSE NULL END))).* FROM generate_series($6::bigint,$7::bigint) g JOIN h3_perf_people person ON person.n=g JOIN seeds seed ON seed.family=CASE g%3 WHEN 0 THEN 'events' WHEN 1 THEN 'calls' ELSE 'text_messages' END")
            .bind(completed_root).bind(completed_plan).bind(fixture.org).bind(scale_root).bind(scale_plan).bind(first).bind(last)
            .execute(&mut *conn).await.unwrap();
    }
    for first in (1_i64..=25_000).step_by(500) {
        let last = (first + 499).min(25_000);
        sqlx::query("INSERT INTO migration_admitted_history_candidate(root_id,plan_id,organization_id,identity_hmac,semantic_hmac,first_manifest_id,person_id,occurrences,conflicting) SELECT root_id,plan_id,organization_id,identity_hmac,semantic_hmac,id,person_id,1,false FROM migration_admitted_history_manifest WHERE root_id=$1 AND plan_id=$2 AND organization_id=$3 AND position BETWEEN $4 AND $5")
            .bind(scale_root).bind(scale_plan).bind(fixture.org).bind(first).bind(last)
            .execute(&mut *conn).await.unwrap();
    }
    sqlx::query("INSERT INTO migration_admitted_history_issue(root_id,plan_id,organization_id,code,record_count) SELECT $1,$2,$3,code,CASE WHEN code='capture_integrity' THEN 24950 ELSE 1 END FROM unnest(ARRAY['invalid_identity','ambiguous_relationship','conflicting_variants','out_of_cohort','target_missing','target_erased','target_identity_mismatch','identity_erased','identity_conflict','fact_or_display_missing','capture_integrity','source_binding_changed']) code")
        .bind(scale_root).bind(scale_plan).bind(fixture.org).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO migration_admitted_history_attempt(id,root_id,plan_id,organization_id,state,executor_user_id,lease_token,lease_expires_at) VALUES($1,$2,$3,$4,'running',$5,$6,clock_timestamp()+interval '30 minutes')")
        .bind(scale_attempt).bind(scale_root).bind(scale_plan).bind(fixture.org).bind(fixture.actor).bind(lease)
        .execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_admitted_history_plan SET state='ready',expires_at=clock_timestamp()+interval '10 minutes',sealed_at=clock_timestamp() WHERE id=$1 AND root_id=$2 AND organization_id=$3")
        .bind(scale_plan).bind(scale_root).bind(fixture.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_admitted_history_root SET state='running',phase='applying',confirmed_plan_id=$2,current_attempt_id=$3,lease_token=$4,lease_expires_at=clock_timestamp()+interval '30 minutes',admitted_at=clock_timestamp(),confirmed_at=clock_timestamp() WHERE id=$1 AND organization_id=$5")
        .bind(scale_root).bind(scale_plan).bind(scale_attempt).bind(lease).bind(fixture.org)
        .execute(&mut *conn).await.unwrap();
    let org_reserved_before: i64 = sqlx::query_scalar(
        "SELECT reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(fixture.org)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    // This is the production 50-record unit reservation:
    // 50 * 32 KiB per record plus 16 KiB control headroom.
    sqlx::query("INSERT INTO migration_admitted_history_reservation(token,root_id,plan_id,organization_id,purpose,byte_count,expires_at) VALUES(gen_random_uuid(),$1,$2,$3,'work',1654784,clock_timestamp()+interval '30 minutes')")
        .bind(scale_root).bind(scale_plan).bind(fixture.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_admitted_history_root SET reserved_bytes=reserved_bytes+1654784 WHERE id=$1 AND organization_id=$2")
        .bind(scale_root).bind(fixture.org).execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+1654784 WHERE organization_id=$1")
        .bind(fixture.org).execute(&mut *conn).await.unwrap();
    // Populate a dense settled prefix through the actual per-manifest result
    // permit and immutable production result trigger. The temporary server-side
    // loop avoids 49,900 client round trips; it is setup only and is never timed.
    sqlx::query("CREATE FUNCTION pg_temp.h3_perf_hold(p_root uuid,p_plan uuid,p_attempt uuid,p_org uuid,p_token uuid,p_first bigint,p_last bigint) RETURNS void LANGUAGE plpgsql AS $$ DECLARE settled record; BEGIN FOR settled IN SELECT id,position,family FROM migration_admitted_history_manifest WHERE root_id=p_root AND plan_id=p_plan AND organization_id=p_org AND position BETWEEN p_first AND p_last ORDER BY position LOOP PERFORM set_config('crm.admitted_history_import_permit',jsonb_build_object('root',p_root,'plan',p_plan,'attempt',p_attempt,'manifest',settled.id,'token',p_token)::text,true); INSERT INTO migration_admitted_history_result(id,root_id,plan_id,attempt_id,manifest_id,organization_id,position,family,disposition,reason,fact_id) VALUES(gen_random_uuid(),p_root,p_plan,p_attempt,settled.id,p_org,settled.position,settled.family,'held','capture_integrity',NULL); END LOOP; END $$")
        .execute(&mut *conn).await.unwrap();
    for first in (1_i64..=24_950).step_by(500) {
        let last = (first + 499).min(24_950);
        sqlx::query("SELECT pg_temp.h3_perf_hold($1,$2,$3,$4,$5,$6,$7)")
            .bind(scale_root)
            .bind(scale_plan)
            .bind(scale_attempt)
            .bind(fixture.org)
            .bind(lease)
            .bind(first)
            .bind(last)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE migration_admitted_history_attempt SET applied_position=24950 WHERE id=$1 AND root_id=$2 AND organization_id=$3")
        .bind(scale_attempt).bind(scale_root).bind(fixture.org).execute(&mut *conn).await.unwrap();

    for table in [
        "organization_membership",
        "person",
        "migration_admitted_history_root",
        "migration_admitted_history_plan",
        "migration_admitted_history_manifest",
        "migration_admitted_history_candidate",
        "migration_admitted_history_result",
        "migration_admitted_history_issue",
        "migration_history_observation",
        "migration_history_import_identity",
        "fub_event_record_imported",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let counts: Value = sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'),'manifests',(SELECT count(*) FROM migration_admitted_history_manifest WHERE root_id=$2 AND plan_id=$3 AND organization_id=$1),'results',(SELECT count(*) FROM migration_admitted_history_result WHERE root_id=$2 AND plan_id=$3 AND organization_id=$1))")
        .bind(fixture.org).bind(scale_root).bind(scale_plan).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(
        counts,
        json!({"people":25000,"members":50,"manifests":25000,"results":24950})
    );
    let charging: Value = sqlx::query_scalar("SELECT jsonb_build_object('root_reserved_bytes',r.reserved_bytes,'root_retained_bytes',r.retained_bytes,'work_reservations',(SELECT count(*) FROM migration_admitted_history_reservation x WHERE x.root_id=r.id AND x.organization_id=r.organization_id AND x.purpose='work'),'work_reserved_bytes',(SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_admitted_history_reservation x WHERE x.root_id=r.id AND x.organization_id=r.organization_id AND x.purpose='work'),'org_reserved_before',$3::bigint,'org_reserved_bytes',s.reserved_bytes,'org_fixture_reservation_delta',s.reserved_bytes-$3::bigint,'org_admitted_history_reservation_bytes',(SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_admitted_history_reservation x WHERE x.organization_id=r.organization_id)) FROM migration_admitted_history_root r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2")
        .bind(scale_root).bind(fixture.org).bind(org_reserved_before).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(charging["root_reserved_bytes"], json!(1_654_784));
    assert_eq!(charging["work_reservations"], json!(1));
    assert_eq!(charging["work_reserved_bytes"], json!(1_654_784));
    assert_eq!(charging["org_fixture_reservation_delta"], json!(1_654_784));
    assert!(charging["root_retained_bytes"].as_i64().unwrap() > 0);

    let observation = sqlx::query("SELECT m.capture_id,o.run_id,m.ordinal,m.identity_hmac FROM migration_admitted_history_manifest m JOIN migration_history_observation o ON o.id=m.observation_id AND o.organization_id=m.organization_id WHERE m.root_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 ORDER BY m.position LIMIT 1")
        .bind(completed_root).bind(completed_plan).bind(fixture.org).fetch_one(&mut *conn).await.unwrap();
    let timeline_person: Uuid = sqlx::query_scalar("SELECT person_id FROM fub_event_record_imported WHERE admitted_root_id=$1 AND organization_id=$2 LIMIT 1")
        .bind(completed_root).bind(fixture.org).fetch_one(&mut *conn).await.unwrap();
    let identity = observation.get::<Vec<u8>, _>("identity_hmac");
    let dense_tail: i64 = sqlx::query_scalar("SELECT applied_position FROM migration_admitted_history_attempt WHERE id=$1 AND root_id=$2 AND organization_id=$3")
        .bind(scale_attempt).bind(scale_root).bind(fixture.org).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(dense_tail, 24_950);
    let mut plans = Vec::new();
    plans.push(
        collect(
            &mut conn,
            "worker_claim",
            statement(
                WORKER,
                "SELECT id,organization_id,executor_user_id FROM migration_admitted_history_root",
            ),
            vec![],
            1.0,
            None,
        )
        .await,
    );
    plans.push(
        collect(
            &mut conn,
            "observation_page",
            statement(
                WORKER,
                "SELECT * FROM migration_history_observation WHERE capture_id=$1",
            ),
            vec![
                Arg::Uuid(observation.get("capture_id")),
                Arg::Uuid(observation.get("run_id")),
                Arg::Uuid(fixture.org),
                Arg::Int32(observation.get("ordinal")),
                Arg::Int32(observation.get::<i32, _>("ordinal") + 1),
            ],
            51.0,
            None,
        )
        .await,
    );
    plans.push(collect(&mut conn,"classification_page",statement(WORKER,"SELECT m.*,x.conflicting,x.first_manifest_id FROM migration_admitted_history_manifest m LEFT JOIN"),vec![Arg::Uuid(scale_plan),Arg::Uuid(fixture.org),Arg::Int(0)],50.0,Some(("migration_admitted_history_manifest",50.0))).await);
    plans.push(collect(&mut conn,"application_page_dense_settled_prefix",statement(WORKER,"SELECT m.*,x.first_manifest_id FROM migration_admitted_history_manifest m LEFT JOIN"),vec![Arg::Uuid(scale_root),Arg::Uuid(scale_plan),Arg::Uuid(fixture.org),Arg::Int(dense_tail)],50.0,Some(("migration_admitted_history_manifest",50.0))).await);
    plans.push(collect(&mut conn,"identity_lock",statement(WORKER,"SELECT * FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2 FOR UPDATE"),vec![Arg::Uuid(fixture.org),Arg::Bytes(identity)],1.0,None).await);
    plans.push(
        collect(
            &mut conn,
            "root_list",
            statement(
                QUERIES,
                "SELECT id,created_at FROM migration_admitted_history_root",
            ),
            vec![
                Arg::Uuid(fixture.org),
                Arg::NullableUuid(None),
                Arg::Time(None),
                Arg::NullableUuid(None),
                Arg::Int(51),
            ],
            51.0,
            None,
        )
        .await,
    );
    let page_sql = |table: &str| {
        format!("SELECT * FROM {table} WHERE root_id=$1 AND plan_id=$2 AND organization_id=$3 AND position>$4 AND ($5::text IS NULL OR family=$5) AND ($6::text IS NULL OR disposition=$6) ORDER BY position LIMIT $7")
    };
    plans.push(
        collect(
            &mut conn,
            "manifest_reader",
            page_sql("migration_admitted_history_manifest"),
            vec![
                Arg::Uuid(scale_root),
                Arg::Uuid(scale_plan),
                Arg::Uuid(fixture.org),
                Arg::Int(24_950),
                Arg::NullableText(None),
                Arg::NullableText(None),
                Arg::Int(51),
            ],
            51.0,
            Some(("migration_admitted_history_manifest", 51.0)),
        )
        .await,
    );
    plans.push(
        collect(
            &mut conn,
            "result_reader",
            page_sql("migration_admitted_history_result"),
            vec![
                Arg::Uuid(scale_root),
                Arg::Uuid(scale_plan),
                Arg::Uuid(fixture.org),
                Arg::Int(24_900),
                Arg::NullableText(None),
                Arg::NullableText(None),
                Arg::Int(51),
            ],
            51.0,
            Some(("migration_admitted_history_result", 51.0)),
        )
        .await,
    );
    plans.push(
        collect(
            &mut conn,
            "issue_reader",
            statement(
                QUERIES,
                "SELECT code,record_count FROM migration_admitted_history_issue",
            ),
            vec![
                Arg::Uuid(scale_root),
                Arg::Uuid(scale_plan),
                Arg::Uuid(fixture.org),
                Arg::NullableText(None),
                Arg::Int(51),
            ],
            51.0,
            None,
        )
        .await,
    );
    let timeline_sql = history_review::candidate_sql_for_test(
        "fub_event_record_imported",
        history_review::Dated::Known,
    )
    .unwrap();
    plans.push(
        collect(
            &mut conn,
            "admitted_timeline_candidate",
            timeline_sql,
            vec![
                Arg::Uuid(fixture.org),
                Arg::Uuid(timeline_person),
                Arg::Time(None),
                Arg::Time(None),
                Arg::Small(None),
                Arg::NullableUuid(None),
                Arg::NullableText(None),
                Arg::Int(51),
                Arg::Int(0),
            ],
            51.0,
            None,
        )
        .await,
    );

    let evidence = json!({
        "protocol":"slice-010d3-d050-hot-plans-v1",
        "scope":"Real qualified completed H3 fixture plus inert guarded relational volume; exact production SQL; no worker throughput or production capacity claim",
        "fixture_counts":counts,
        "synthetic_charging":charging,
        "real_completed_root":completed_root,
        "inert_scale_root":scale_root,
        "source_sha256":{"worker":sha256(WORKER.as_bytes()),"commands":sha256(COMMANDS.as_bytes()),"queries":sha256(QUERIES.as_bytes()),"history_review":sha256(REVIEW.as_bytes())},
        "plans":plans
    });
    std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}

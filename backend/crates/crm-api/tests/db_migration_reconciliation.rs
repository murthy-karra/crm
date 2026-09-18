//! Synthetic reconciliation contract, authorization, and tenant evidence.
use axum::http::StatusCode;
use crm_api::{
    auth::workspace::{self, ReleaseReadiness},
    domain::migration::{
        family_refresh::{
            commands::{self as family_refresh_commands, PrepareFamilyRefresh},
            model::Family,
        },
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        people_admission, people_admission_worker, reconciliation,
    },
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    common::get_with_cookie,
    import_support::{self, Fixture},
};

async fn completed_import(fixture: &Fixture) -> Uuid {
    let (import_id, _) = import_support::propose(fixture).await;
    import_support::drain_import(fixture).await;
    let (_, plan_id) = import_support::replan(
        fixture,
        import_id,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: StageChoice::Existing {
                stage_id: fixture.lead_stage,
            },
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Unassigned,
        }],
    )
    .await;
    import_support::drain_import(fixture).await;
    let detail = imports::detail(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        import_id,
        &fixture.policy,
    )
    .await
    .unwrap();
    imports::confirm(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        import_id,
        imports::ConfirmPeopleImport {
            request_id: Uuid::new_v4(),
            plan_id,
            plan_revision: detail["plan"]["revision"].as_str().unwrap().into(),
            confirmation_digest: detail["plan"]["confirmation_digest"]
                .as_str()
                .unwrap()
                .into(),
            acknowledgments: imports::Acknowledgments {
                held_count: detail["counts"]["held_people"].as_str().unwrap().into(),
                review_only: true,
                remaining_data: true,
            },
        },
        &workspace::ReleaseReadiness::for_tests(),
        &fixture.policy,
    )
    .await
    .unwrap();
    import_support::drain_import(fixture).await;
    import_id
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[sqlx::test]
#[ignore]
async fn reconciliation_is_bounded_tenant_scoped_and_review_only(migrator: PgPool) {
    let fixture = import_support::fixture(&migrator, import_support::default_people()).await;
    let import_id = completed_import(&fixture).await;

    // Existing import detail takes the Organization row before the import row.
    // A snapshot-only report must not invert that order by locking its joined
    // import first and then waiting on this Organization row.
    let mut detail_tx = fixture.pool.begin().await.unwrap();
    workspace::shared(&mut detail_tx, fixture.ctx.organization_id)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM organization WHERE id=$1 FOR UPDATE")
        .bind(fixture.org)
        .fetch_one(&mut *detail_tx)
        .await
        .unwrap();
    let summary = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        reconciliation::summary(&fixture.pool, &fixture.ctx, import_id),
    )
    .await
    .expect("report must not wait on import-detail row locks")
    .unwrap();
    detail_tx.rollback().await.unwrap();
    let value = serde_json::to_value(&summary).unwrap();
    assert_eq!(value["original_import_id"], json!(import_id));
    assert_eq!(value["review_hold"], true);
    assert!(value["workspace_revision"].as_str().is_some());
    assert!(value["snapshot_revision"].is_null());
    assert!(value["families"].as_array().unwrap().len() >= 20);
    let people = value["families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["coverage"]["family"] == "people_contacts")
        .unwrap();
    assert_eq!(people["unit"], "people");
    assert_eq!(people["cohorts"][0]["cohort_origin"], "original");
    assert_eq!(people["cohorts"][0]["cohort_id"], json!(import_id));
    assert_ne!(people["cohorts"][0]["result_totals"]["applied"], "0");
    assert!(serde_json::to_vec(&summary).unwrap().len() <= 512 * 1024);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains("Synthetic One"));
    assert!(!serialized.contains("4155550100"));

    let path = format!("/api/migrations/fub/imports/{import_id}/reconciliation");
    let response = get_with_cookie(&fixture.app, &path, &fixture.cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let route_value = response_json(response).await;
    assert_eq!(route_value["original_import_id"], json!(import_id));

    let denied = get_with_cookie(&fixture.app, &path, &fixture.member_cookie).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(denied.headers()["cache-control"], "no-store");

    let malformed = get_with_cookie(
        &fixture.app,
        "/api/migrations/fub/imports/not-a-uuid/reconciliation",
        &fixture.cookie,
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(malformed.headers()["cache-control"], "no-store");

    let foreign = import_support::fixture(&migrator, import_support::default_people()).await;
    let (foreign_import_id, _) = import_support::propose(&foreign).await;
    let hidden = get_with_cookie(
        &fixture.app,
        &format!("/api/migrations/fub/imports/{foreign_import_id}/reconciliation"),
        &fixture.cookie,
    )
    .await;
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    assert_eq!(hidden.headers()["cache-control"], "no-store");
}

#[sqlx::test]
#[ignore]
async fn reconciliation_reads_nonempty_metadata_activity_and_history_ledgers(migrator: PgPool) {
    let (fixture, import_id, _, _) = crate::db_family_refresh_acceptance::fixture(&migrator).await;
    let value = serde_json::to_value(
        reconciliation::summary(&fixture.pool, &fixture.ctx, import_id)
            .await
            .unwrap(),
    )
    .unwrap();
    for family_name in [
        "embedded_person_tags",
        "custom_fields",
        "notes",
        "tasks",
        "historical_events",
        "calls",
        "texts",
    ] {
        let family = value["families"]
            .as_array()
            .unwrap()
            .iter()
            .find(|family| family["coverage"]["family"] == family_name)
            .unwrap_or_else(|| panic!("missing family {family_name}"));
        let cohort = family["cohorts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|cohort| cohort["cohort_origin"] == "original")
            .unwrap();
        let totals = &cohort["result_totals"];
        let settled = totals["applied"].as_str().unwrap().parse::<u64>().unwrap()
            + totals["already_current"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
            + totals["held"].as_str().unwrap().parse::<u64>().unwrap()
            + totals["excluded"].as_str().unwrap().parse::<u64>().unwrap();
        assert!(settled > 0, "{family_name} retained ledger was not read");
    }
}

#[sqlx::test]
#[ignore]
async fn reconciliation_keeps_recovery_as_its_own_bounded_origin(migrator: PgPool) {
    let (fixture, import_id, admission_id) =
        crate::db_people_recovery::recovered_fixture(&migrator).await;
    let value = serde_json::to_value(
        reconciliation::summary(&fixture.pool, &fixture.ctx, import_id)
            .await
            .unwrap(),
    )
    .unwrap();
    let people = value["families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["coverage"]["family"] == "people_contacts")
        .unwrap();
    let recovery = people["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cohort| cohort["cohort_origin"] == "recovery")
        .expect("recovery cohort");
    assert_eq!(recovery["cohort_id"], json!(admission_id));
    assert_ne!(recovery["result_totals"]["applied"], "0");
}

#[sqlx::test]
#[ignore]
async fn reconciliation_deduplicates_people_across_terminal_admissions(migrator: PgPool) {
    let person = json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550104"}]});
    let (fixture, import_id, first_admission) =
        crate::db_admitted_people_refresh_execution::fixture_with_admission(
            &migrator,
            vec![person.clone()],
        )
        .await;
    let second_admission = crate::db_admitted_people_refresh_execution::ready_admission(
        &fixture,
        import_id,
        vec![
            person,
            json!({"id":105,"firstName":"Second","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550105"}]}),
            json!({"id":101,"firstName":"Original","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
        ],
    )
    .await;
    crate::db_admitted_people_refresh_execution::confirm_admission(&fixture, second_admission)
        .await;
    for _ in 0..100 {
        if people_admission::detail(&fixture.pool, &fixture.key, &fixture.ctx, second_admission)
            .await
            .unwrap()["state"]
            == "completed"
        {
            break;
        }
        assert!(people_admission_worker::run_once(
            &fixture.pool,
            &fixture.key,
            &fixture.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap());
    }
    assert_eq!(
        people_admission::detail(&fixture.pool, &fixture.key, &fixture.ctx, second_admission)
            .await
            .unwrap()["state"],
        "completed"
    );

    let value = serde_json::to_value(
        reconciliation::summary(&fixture.pool, &fixture.ctx, import_id)
            .await
            .unwrap(),
    )
    .unwrap();
    let admission = value["families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["coverage"]["family"] == "people_contacts")
        .unwrap()["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cohort| cohort["cohort_origin"] == "admission")
        .unwrap();
    assert_eq!(admission["cohort_id"], json!(first_admission));
    assert_eq!(admission["result_totals"]["applied"], "2");
    assert_eq!(admission["result_totals"]["already_current"], "1");
    assert_eq!(admission["result_totals"]["held"], "0");
    assert_eq!(admission["result_totals"]["excluded"], "0");
    assert_eq!(admission["result_totals"]["unprocessed"], "0");
    let categorized: u64 = [
        "applied",
        "already_current",
        "held",
        "excluded",
        "unprocessed",
    ]
    .into_iter()
    .map(|key| {
        admission["result_totals"][key]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    })
    .sum();
    assert_eq!(
        categorized, 3,
        "each stable source key has exactly one outcome"
    );
}

#[sqlx::test]
#[ignore]
async fn reconciliation_deduplicates_history_identity_across_terminal_roots(migrator: PgPool) {
    let (fixture, import_id, _, _) = crate::db_family_refresh_acceptance::fixture(&migrator).await;
    let new_root = Uuid::new_v4();
    let new_plan = Uuid::new_v4();
    let new_manifest = Uuid::new_v4();
    let new_result = Uuid::new_v4();
    let mut tx = migrator.begin().await.unwrap();
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO migration_history_import_run SELECT (jsonb_populate_record(NULL::migration_history_import_run,to_jsonb(seed)||jsonb_build_object('id',$3::uuid,'plan_id',$4::uuid,'created_at',clock_timestamp()+interval '1 second'))).* FROM (SELECT * FROM migration_history_import_run WHERE parent_import_id=$1 AND organization_id=$2 ORDER BY created_at,id LIMIT 1) seed")
        .bind(import_id).bind(fixture.org).bind(new_root).bind(new_plan).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_plan SELECT (jsonb_populate_record(NULL::migration_history_import_plan,to_jsonb(seed)||jsonb_build_object('id',$3::uuid,'owner_run_id',$4::uuid))).* FROM (SELECT p.* FROM migration_history_import_plan p JOIN migration_history_import_run root ON root.id=p.owner_run_id AND root.organization_id=p.organization_id WHERE root.parent_import_id=$1 AND p.organization_id=$2 ORDER BY root.created_at,root.id LIMIT 1) seed")
        .bind(import_id).bind(fixture.org).bind(new_plan).bind(new_root).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_manifest SELECT (jsonb_populate_record(NULL::migration_history_import_manifest,to_jsonb(seed)||jsonb_build_object('id',$3::uuid,'plan_id',$4::uuid,'owner_run_id',$5::uuid))).* FROM (SELECT m.* FROM migration_history_import_manifest m JOIN migration_history_import_run root ON root.id=m.owner_run_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.family='events' AND m.identity_hmac IS NOT NULL ORDER BY m.position LIMIT 1) seed")
        .bind(import_id).bind(fixture.org).bind(new_manifest).bind(new_plan).bind(new_root).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_result SELECT (jsonb_populate_record(NULL::migration_history_import_result,to_jsonb(seed)||jsonb_build_object('id',$3::uuid,'plan_id',$4::uuid,'owner_run_id',$5::uuid,'manifest_id',$6::uuid,'disposition','already_imported'))).* FROM (SELECT r.* FROM migration_history_import_result r JOIN migration_history_import_run root ON root.id=r.owner_run_id AND root.organization_id=r.organization_id WHERE root.parent_import_id=$1 AND r.organization_id=$2 AND r.family='events' ORDER BY r.position LIMIT 1) seed")
        .bind(import_id).bind(fixture.org).bind(new_result).bind(new_plan).bind(new_root).bind(new_manifest).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    let value = serde_json::to_value(
        reconciliation::summary(&fixture.pool, &fixture.ctx, import_id)
            .await
            .unwrap(),
    )
    .unwrap();
    let events = value["families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["coverage"]["family"] == "historical_events")
        .unwrap()["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cohort| cohort["cohort_origin"] == "original")
        .unwrap();
    assert_eq!(events["result_totals"]["applied"], "0");
    assert_eq!(events["result_totals"]["already_current"], "1");
}
fn sql_sha256(sql: &str) -> String {
    Sha256::digest(sql.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn explain(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    label: &str,
    sql: &str,
    org: Uuid,
    import: Uuid,
    maximum_rows: usize,
) -> Value {
    let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"))
        .bind(org)
        .bind(import)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    println!(
        "RECONCILIATION_PLAN {}",
        json!({
            "label": label,
            "sql": sql,
            "sql_sha256": sql_sha256(sql),
            "bindings": {"organization_id": org, "original_import_id": import},
            "maximum_returned_rows": maximum_rows,
            "plan": plan,
        })
    );
    let rows = plan[0]["Plan"]["Actual Rows"].as_f64().unwrap();
    assert!(
        rows.is_finite() && rows >= 0.0 && rows <= maximum_rows as f64,
        "{label} returned {rows} rows"
    );
    plan
}

async fn explain_root(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: Uuid,
    import: Uuid,
) -> Value {
    let sql = reconciliation::ROOT_SQL;
    let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"))
        .bind(import)
        .bind(org)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    println!(
        "RECONCILIATION_PLAN {}",
        json!({
            "label": "root",
            "sql": sql,
            "sql_sha256": sql_sha256(sql),
            "bindings": {"original_import_id": import, "organization_id": org},
            "maximum_returned_rows": 1,
            "plan": plan,
        })
    );
    let rows = plan[0]["Plan"]["Actual Rows"].as_f64().unwrap();
    assert!(
        rows.is_finite() && rows >= 0.0 && rows <= 1.0,
        "root returned {rows} rows"
    );
    plan
}

#[sqlx::test]
#[ignore = "D-050 one-shot plan evidence at the declared operating envelope"]
async fn d050_query_plans_at_operating_envelope(migrator: PgPool) {
    let (fixture, import_id, core_report_id, history_capture_id) =
        crate::db_family_refresh_acceptance::fixture(&migrator).await;
    let refresh = family_refresh_commands::prepare(
        &fixture.pool,
        &fixture.key,
        &fixture.policy,
        &ReleaseReadiness::for_tests(),
        &fixture.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: import_id,
            core_report_id: Some(core_report_id),
            history_capture_id: Some(history_capture_id),
            families: vec![Family::Metadata, Family::Activity, Family::History],
        },
    )
    .await
    .unwrap();
    let mut tx = migrator.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role='replica'")
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query("CREATE TEMP TABLE reconciliation_members AS SELECT g,gen_random_uuid() AS id FROM generate_series(1,50-(SELECT count(*)::int FROM organization_membership WHERE organization_id=$1)) g")
        .bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app_user SELECT (jsonb_populate_record(NULL::app_user,to_jsonb(seed)||jsonb_build_object('id',scale.id,'email','reconciliation-plan-'||scale.g||'@synthetic.test','display_name','Reconciliation plan member'))).* FROM (SELECT * FROM app_user WHERE id=$1) seed CROSS JOIN reconciliation_members scale")
        .bind(fixture.actor).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO organization_membership SELECT (jsonb_populate_record(NULL::organization_membership,to_jsonb(seed)||jsonb_build_object('user_id',scale.id))).* FROM (SELECT * FROM organization_membership WHERE organization_id=$1 AND user_id=$2) seed CROSS JOIN reconciliation_members scale")
        .bind(fixture.org).bind(fixture.actor).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO person SELECT (jsonb_populate_record(NULL::person,to_jsonb(seed)||jsonb_build_object('id',gen_random_uuid(),'first_name','Reconciliation plan Person'))).* FROM (SELECT * FROM person WHERE organization_id=$1 LIMIT 1) seed CROSS JOIN generate_series(1,25000-(SELECT count(*)::int FROM person WHERE organization_id=$1))")
        .bind(fixture.org).execute(&mut *tx).await.unwrap();

    sqlx::query("CREATE TEMP TABLE reconciliation_people_scale AS SELECT g,gen_random_uuid() AS manifest,gen_random_uuid() AS result FROM generate_series(1,25000-(SELECT count(*)::int FROM migration_import_manifest WHERE import_id=$1 AND organization_id=$2 AND plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2))) g")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_import_manifest SELECT (jsonb_populate_record(NULL::migration_import_manifest,to_jsonb(seed)||jsonb_build_object('id',scale.manifest,'source_id',(1000000+scale.g)::text))).* FROM (SELECT * FROM migration_import_manifest WHERE import_id=$1 AND organization_id=$2 AND plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2) LIMIT 1) seed CROSS JOIN reconciliation_people_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_import_result SELECT (jsonb_populate_record(NULL::migration_import_result,to_jsonb(seed)||jsonb_build_object('id',scale.result,'manifest_id',scale.manifest,'source_id',(1000000+scale.g)::text,'disposition','imported'))).* FROM (SELECT * FROM migration_import_result WHERE import_id=$1 AND organization_id=$2 AND plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2) AND disposition='imported' LIMIT 1) seed CROSS JOIN reconciliation_people_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_import_identity SELECT (jsonb_populate_record(NULL::migration_import_identity,to_jsonb(seed)||jsonb_build_object('source_id',(1000000+scale.g)::text,'manifest_id',scale.manifest))).* FROM (SELECT * FROM migration_import_identity WHERE import_id=$1 AND organization_id=$2 AND plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2) AND family='people' LIMIT 1) seed CROSS JOIN reconciliation_people_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();

    sqlx::query("CREATE TEMP TABLE reconciliation_metadata_scale AS SELECT g,gen_random_uuid() AS manifest,gen_random_uuid() AS result FROM generate_series(1,25000-(SELECT count(*)::int FROM migration_metadata_manifest m JOIN migration_metadata_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.confirmed_plan_id)) g")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_manifest SELECT (jsonb_populate_record(NULL::migration_metadata_manifest,to_jsonb(seed)||jsonb_build_object('id',scale.manifest,'source_id',(2000000+scale.g)::text))).* FROM (SELECT m.* FROM migration_metadata_manifest m JOIN migration_metadata_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.confirmed_plan_id LIMIT 1) seed CROSS JOIN reconciliation_metadata_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_metadata_result SELECT (jsonb_populate_record(NULL::migration_metadata_result,to_jsonb(seed)||jsonb_build_object('id',scale.result,'manifest_id',scale.manifest,'unit_id',gen_random_uuid(),'source_id',(2000000+scale.g)::text,'disposition','applied'))).* FROM (SELECT r.* FROM migration_metadata_result r JOIN migration_metadata_import root ON root.id=r.import_id AND root.organization_id=r.organization_id WHERE root.parent_import_id=$1 AND r.organization_id=$2 AND r.plan_id=root.confirmed_plan_id AND r.kind='people' LIMIT 1) seed CROSS JOIN reconciliation_metadata_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();

    sqlx::query("CREATE TEMP TABLE reconciliation_activity_scale AS SELECT g,gen_random_uuid() AS source,gen_random_uuid() AS manifest,gen_random_uuid() AS result FROM generate_series(1,25000-(SELECT count(*)::int FROM migration_activity_manifest m JOIN migration_activity_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.confirmed_plan_id)) g")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_source SELECT (jsonb_populate_record(NULL::migration_activity_source,to_jsonb(seed)||jsonb_build_object('id',scale.source,'family',CASE WHEN scale.g%2=0 THEN 'notes' ELSE 'tasks' END,'source_id',(3000000+scale.g)::text,'ordinal',1000000+scale.g))).* FROM (SELECT s.* FROM migration_activity_source s JOIN migration_activity_import root ON root.id=s.import_id AND root.organization_id=s.organization_id WHERE root.parent_import_id=$1 AND s.organization_id=$2 AND s.plan_id=root.confirmed_plan_id AND s.family IN ('notes','tasks') LIMIT 1) seed CROSS JOIN reconciliation_activity_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_manifest SELECT (jsonb_populate_record(NULL::migration_activity_manifest,to_jsonb(seed)||jsonb_build_object('id',scale.manifest,'source_row_id',scale.source,'kind',CASE WHEN scale.g%2=0 THEN 'note' ELSE 'task' END,'source_id',(3000000+scale.g)::text,'native_source_key',(3000000+scale.g)::text))).* FROM (SELECT m.* FROM migration_activity_manifest m JOIN migration_activity_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.confirmed_plan_id LIMIT 1) seed CROSS JOIN reconciliation_activity_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_result SELECT (jsonb_populate_record(NULL::migration_activity_result,to_jsonb(seed)||jsonb_build_object('id',scale.result,'manifest_id',scale.manifest,'kind',CASE WHEN scale.g%2=0 THEN 'note' ELSE 'task' END,'source_id',(3000000+scale.g)::text,'disposition','applied'))).* FROM (SELECT r.* FROM migration_activity_result r JOIN migration_activity_import root ON root.id=r.import_id AND root.organization_id=r.organization_id WHERE root.parent_import_id=$1 AND r.organization_id=$2 AND r.plan_id=root.confirmed_plan_id LIMIT 1) seed CROSS JOIN reconciliation_activity_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();

    sqlx::query("CREATE TEMP TABLE reconciliation_history_scale AS SELECT g,gen_random_uuid() AS manifest,gen_random_uuid() AS result,gen_random_uuid() AS observation FROM generate_series(1,25000-(SELECT count(*)::int FROM migration_history_import_manifest m JOIN migration_history_import_run root ON root.id=m.owner_run_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.plan_id)) g")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_manifest SELECT (jsonb_populate_record(NULL::migration_history_import_manifest,to_jsonb(seed)||jsonb_build_object('id',scale.manifest,'position',1000000+scale.g,'observation_id',scale.observation,'ordinal',0,'family',CASE scale.g%3 WHEN 0 THEN 'events' WHEN 1 THEN 'calls' ELSE 'text_messages' END,'identity_hmac',sha256(convert_to(scale.g::text,'UTF8')),'semantic_hmac',sha256(convert_to(('semantic-'||scale.g)::text,'UTF8'))))).* FROM (SELECT m.* FROM migration_history_import_manifest m JOIN migration_history_import_run root ON root.id=m.owner_run_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$1 AND m.organization_id=$2 AND m.plan_id=root.plan_id LIMIT 1) seed CROSS JOIN reconciliation_history_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_result SELECT (jsonb_populate_record(NULL::migration_history_import_result,to_jsonb(seed)||jsonb_build_object('id',scale.result,'manifest_id',scale.manifest,'position',1000000+scale.g,'family',CASE scale.g%3 WHEN 0 THEN 'events' WHEN 1 THEN 'calls' ELSE 'text_messages' END,'disposition','imported'))).* FROM (SELECT r.* FROM migration_history_import_result r JOIN migration_history_import_run root ON root.id=r.owner_run_id AND root.organization_id=r.organization_id WHERE root.parent_import_id=$1 AND r.organization_id=$2 AND r.disposition='imported' LIMIT 1) seed CROSS JOIN reconciliation_history_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();

    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM person WHERE organization_id=$1),(SELECT count(*) FROM organization_membership WHERE organization_id=$1),(SELECT count(*) FROM migration_import_manifest m JOIN migration_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE m.import_id=$2 AND m.organization_id=$1 AND m.plan_id=root.confirmed_plan_id),(SELECT count(*) FROM migration_metadata_manifest m JOIN migration_metadata_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1 AND m.plan_id=root.confirmed_plan_id),(SELECT count(*) FROM migration_activity_manifest m JOIN migration_activity_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1 AND m.plan_id=root.confirmed_plan_id),(SELECT count(*) FROM migration_history_import_manifest m JOIN migration_history_import_run root ON root.id=m.owner_run_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1 AND m.plan_id=root.plan_id)")
        .bind(fixture.org).bind(import_id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(counts, (25_000, 50, 25_000, 25_000, 25_000, 25_000));
    sqlx::query("CREATE TEMP TABLE reconciliation_refresh_people AS SELECT row_number() OVER (ORDER BY id)::bigint AS g,id AS person,gen_random_uuid() AS cohort FROM person WHERE organization_id=$1")
        .bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id) SELECT scale.cohort,$1,$2,(5000000+scale.g)::text,scale.person,seed.result_id,seed.snapshot_id FROM reconciliation_refresh_people scale CROSS JOIN (SELECT r.id AS result_id,i.snapshot_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id WHERE r.import_id=$3 AND r.organization_id=$2 AND r.disposition='imported' ORDER BY r.source_id LIMIT 1) seed")
        .bind(refresh.bundle_id).bind(fixture.org).bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query("CREATE TEMP TABLE reconciliation_refresh_scale AS SELECT p.id AS plan,p.family,people.g,people.person,people.cohort,gen_random_uuid() AS manifest FROM migration_family_refresh_plan p CROSS JOIN reconciliation_refresh_people people WHERE p.bundle_id=$1 AND p.organization_id=$2 AND p.state<>'superseded'")
        .bind(refresh.bundle_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,position,kind,source_key_hmac,person_id,disposition,counts,nonce,ciphertext,added_byte_bound,source_id) SELECT scale.manifest,$1,scale.plan,$2,scale.cohort,scale.g,CASE scale.family WHEN 'metadata' THEN 'metadata' WHEN 'activity' THEN CASE WHEN scale.g%2=0 THEN 'note' ELSE 'task' END ELSE CASE scale.g%3 WHEN 0 THEN 'event' WHEN 1 THEN 'call' ELSE 'text' END END,sha256(convert_to(scale.family||':'||scale.g::text,'UTF8')),scale.person,'already_current','{}'::jsonb,decode(repeat('00',24),'hex'),''::bytea,0,(4000000+scale.g)::text FROM reconciliation_refresh_scale scale")
        .bind(refresh.bundle_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_family_refresh_result(id,bundle_id,plan_id,organization_id,manifest_id,disposition,person_id,nonce,ciphertext,native_bytes) SELECT gen_random_uuid(),$1,scale.plan,$2,scale.manifest,'already_current',scale.person,decode(repeat('00',24),'hex'),''::bytea,0 FROM reconciliation_refresh_scale scale")
        .bind(refresh.bundle_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    sqlx::query("CREATE TEMP TABLE reconciliation_warning_scale AS SELECT g,gen_random_uuid() AS capture FROM generate_series(1,25000) g")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture SELECT (jsonb_populate_record(NULL::migration_snapshot_capture,to_jsonb(seed)||jsonb_build_object('id',scale.capture,'sequence',1000000+scale.g,'checkpoint',1000000+scale.g,'stream',CASE scale.g%6 WHEN 0 THEN 'people' WHEN 1 THEN 'users' WHEN 2 THEN 'stages' WHEN 3 THEN 'custom_fields' WHEN 4 THEN 'notes' ELSE 'tasks' END,'accepted',false,'classification','incomplete','captured_at',clock_timestamp()))).* FROM (SELECT c.* FROM migration_snapshot_capture c JOIN migration_import i ON i.snapshot_id=c.snapshot_id AND i.organization_id=c.organization_id WHERE i.id=$1 AND c.organization_id=$2 LIMIT 1) seed CROSS JOIN reconciliation_warning_scale scale")
        .bind(import_id).bind(fixture.org).execute(&mut *tx).await.unwrap();
    let fixture_counts: Value = sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=$1),'selected_original_people_manifests',$3::bigint,'selected_metadata_manifests',$4::bigint,'selected_activity_manifests',$5::bigint,'selected_history_manifests',$6::bigint,'all_original_people_manifests',(SELECT count(*) FROM migration_import_manifest WHERE import_id=$2 AND organization_id=$1),'all_metadata_manifests',(SELECT count(*) FROM migration_metadata_manifest m JOIN migration_metadata_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1),'all_activity_manifests',(SELECT count(*) FROM migration_activity_manifest m JOIN migration_activity_import root ON root.id=m.import_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1),'all_history_manifests',(SELECT count(*) FROM migration_history_import_manifest m JOIN migration_history_import_run root ON root.id=m.owner_run_id AND root.organization_id=m.organization_id WHERE root.parent_import_id=$2 AND m.organization_id=$1),'refresh_cohorts',(SELECT count(*) FROM migration_family_refresh_cohort WHERE bundle_id=$7 AND organization_id=$1),'refresh_manifests',(SELECT count(*) FROM migration_family_refresh_manifest WHERE bundle_id=$7 AND organization_id=$1),'refresh_results',(SELECT count(*) FROM migration_family_refresh_result WHERE bundle_id=$7 AND organization_id=$1),'inert_warning_captures',(SELECT count(*) FROM migration_snapshot_capture c JOIN migration_import i ON i.snapshot_id=c.snapshot_id AND i.organization_id=c.organization_id WHERE i.id=$2 AND c.organization_id=$1 AND NOT c.accepted AND c.sequence>=1000001))")
        .bind(fixture.org).bind(import_id).bind(counts.2).bind(counts.3).bind(counts.4).bind(counts.5).bind(refresh.bundle_id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(fixture_counts["refresh_cohorts"], 25_000);
    assert_eq!(fixture_counts["refresh_manifests"], 75_000);
    assert_eq!(fixture_counts["refresh_results"], 75_000);
    assert_eq!(fixture_counts["inert_warning_captures"], 25_000);
    println!("RECONCILIATION_FIXTURE {fixture_counts}");
    sqlx::query("SET LOCAL session_replication_role='origin'")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ANALYZE person,organization_membership,migration_import_manifest,migration_import_result,migration_import_identity,migration_metadata_manifest,migration_metadata_result,migration_activity_manifest,migration_activity_result,migration_history_import_manifest,migration_history_import_result,migration_family_refresh_bundle,migration_family_refresh_plan,migration_family_refresh_cohort,migration_family_refresh_manifest,migration_family_refresh_result,migration_snapshot_capture")
        .execute(&mut *tx).await.unwrap();

    let mut plans = vec![explain_root(&mut tx, fixture.org, import_id).await];
    plans.push(
        explain(
            &mut tx,
            "cohorts",
            reconciliation::COHORTS_SQL,
            fixture.org,
            import_id,
            3,
        )
        .await,
    );
    plans.push(
        explain(
            &mut tx,
            "metadata_totals",
            reconciliation::METADATA_TOTALS_SQL,
            fixture.org,
            import_id,
            3,
        )
        .await,
    );
    plans.push(
        explain(
            &mut tx,
            "activity_totals",
            reconciliation::ACTIVITY_TOTALS_SQL,
            fixture.org,
            import_id,
            6,
        )
        .await,
    );
    plans.push(
        explain(
            &mut tx,
            "history_totals",
            reconciliation::HISTORY_TOTALS_SQL,
            fixture.org,
            import_id,
            9,
        )
        .await,
    );
    plans.push(
        explain(
            &mut tx,
            "latest_refresh",
            reconciliation::LATEST_REFRESH_SQL,
            fixture.org,
            import_id,
            21,
        )
        .await,
    );
    plans.push(
        explain(
            &mut tx,
            "source_warnings",
            reconciliation::SOURCE_WARNINGS_SQL,
            fixture.org,
            import_id,
            18,
        )
        .await,
    );
    for plan in &plans {
        let text = plan.to_string();
        assert!(!text.contains("migration_snapshot_record"));
        assert!(!text.contains("migration_raw_response"));
    }
    tx.rollback().await.unwrap();
}

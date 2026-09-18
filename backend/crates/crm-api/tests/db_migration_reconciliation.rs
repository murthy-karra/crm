//! Synthetic reconciliation contract, authorization, and tenant evidence.
use axum::http::StatusCode;
use crm_api::{
    auth::workspace::{self, ReleaseReadiness},
    domain::migration::{
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        people_admission, people_admission_worker, reconciliation,
    },
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
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

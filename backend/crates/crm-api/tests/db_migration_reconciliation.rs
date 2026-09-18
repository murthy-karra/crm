//! Synthetic reconciliation contract, authorization, and tenant evidence.
use axum::http::StatusCode;
use crm_api::{
    auth::workspace,
    domain::migration::{
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        reconciliation,
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

    let summary = reconciliation::summary(&fixture.pool, &fixture.ctx, import_id)
        .await
        .unwrap();
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

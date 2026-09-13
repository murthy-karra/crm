//! Real retained-source API read and recovery regression coverage for 010e4.
use crate::{common, db_admitted_people_refresh_execution as execution, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_people_refresh as refresh, admitted_people_refresh_worker, MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn get(f: &Fixture, path: &str) -> Value {
    let response = common::get_with_cookie(&f.app, path, &f.cookie).await;
    assert_eq!(response.status(), 200, "{path}");
    assert_eq!(response.headers()["cache-control"], "no-store");
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn detail(f: &Fixture, id: Uuid) -> Value {
    get(
        f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}"),
    )
    .await
}
async fn drain(f: &Fixture) {
    for _ in 0..100 {
        if !admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            return;
        }
    }
    panic!("bounded refresh fixture did not finish");
}
fn lifecycle(value: &Value) -> refresh::LifecycleAdmittedPeopleRefresh {
    refresh::LifecycleAdmittedPeopleRefresh {
        request_id: Uuid::new_v4(),
        expected_lifecycle_revision: value["lifecycle_revision"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    }
}
async fn setup(migrator: &PgPool) -> (Fixture, Uuid, Uuid) {
    let (f, parent, admission) = execution::fixture_with_admission(
        migrator,
        vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let report = execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"New name","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let listing = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes?admission_id={admission}"),
    )
    .await;
    assert_eq!(listing["refreshes"], json!([]));
    let receipt = refresh::prepare(
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
    let id = Uuid::parse_str(receipt["refresh_id"].as_str().unwrap()).unwrap();
    (f, admission, id)
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admission_scoped_http_reads_repreview_and_preparation_retry(migrator: PgPool) {
    let (f, admission, id) = setup(&migrator).await;
    // Actual release withdrawal pauses preparation before any confirmed plan.
    assert!(
        admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, None)
            .await
            .unwrap()
    );
    let paused = detail(&f, id).await;
    assert_eq!(paused["admission_id"], admission.to_string());
    assert_eq!(paused["state"], "paused");
    let retry = refresh::retry(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        lifecycle(&paused),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(retry["state"], "preparing");
    drain(&f).await;
    let first = detail(&f, id).await;
    assert_eq!(first["state"], "ready");
    assert_ne!(
        first["plan"]["revision"], first["lifecycle_revision"],
        "retry makes these revisions independent"
    );
    let request = Uuid::new_v4();
    let revision = first["plan"]["revision"].as_str().unwrap().parse().unwrap();
    let receipt = refresh::repreview(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        refresh::RepreviewAdmittedPeopleRefresh {
            request_id: request,
            expected_plan_revision: revision,
        },
    )
    .await
    .unwrap();
    drain(&f).await;
    let second = detail(&f, id).await;
    assert_eq!(second["state"], "ready");
    assert_eq!(second["plan"]["revision"], "2");
    assert_eq!(second["plan"]["counts"]["eligible"], "1");
    let items = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}/items"),
    )
    .await;
    let item_id = items["items"][0]["id"].as_str().unwrap();
    let comparison = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}/items/{item_id}"),
    )
    .await;
    for side in ["baseline", "current", "proposed"] {
        assert_eq!(comparison[side]["stage_id"], f.lead_stage.to_string());
        assert_eq!(comparison[side]["current_stage_label"], "Lead");
        assert!(comparison[side]["current_assignee_label"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }
    assert_eq!(
        refresh::repreview(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            refresh::RepreviewAdmittedPeopleRefresh {
                request_id: request,
                expected_plan_revision: revision
            }
        )
        .await
        .unwrap(),
        receipt
    );
    assert!(matches!(
        refresh::repreview(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            refresh::RepreviewAdmittedPeopleRefresh {
                request_id: Uuid::new_v4(),
                expected_plan_revision: revision
            }
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let listing = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes?admission_id={admission}"),
    )
    .await;
    assert_eq!(listing["refreshes"][0]["id"], id.to_string());
    assert_eq!(
        listing["refreshes"][0]["admission_id"],
        admission.to_string()
    );
    let command = lifecycle(&second);
    let cancel_id = command.request_id;
    let cancel_revision = command.expected_lifecycle_revision;
    let receipt = refresh::cancel(&f.pool, &f.key, &f.ctx, id, command)
        .await
        .unwrap();
    assert_eq!(
        refresh::cancel(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            refresh::LifecycleAdmittedPeopleRefresh {
                request_id: cancel_id,
                expected_lifecycle_revision: cancel_revision
            }
        )
        .await
        .unwrap(),
        receipt
    );
    let terminal = detail(&f, id).await;
    assert!(matches!(
        refresh::cancel(&f.pool, &f.key, &f.ctx, id, lifecycle(&terminal)).await,
        Err(MigrationError::Conflict)
    ));
    assert_eq!(detail(&f, id).await, terminal);
    let eligible = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}/items?disposition=eligible"),
    )
    .await;
    assert_eq!(eligible["items"], json!([]));
    let cancelled = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}/items?disposition=cancelled"),
    )
    .await;
    assert_eq!(cancelled["items"].as_array().unwrap().len(), 1);
    assert_eq!(cancelled["items"][0]["disposition"], "cancelled");
    assert!(cancelled["items"][0]["settled_at"].is_null());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn exact_eligible_confirmation_execution_retry_and_completed_terminal(migrator: PgPool) {
    let (f, _, id) = setup(&migrator).await;
    drain(&f).await;
    let ready = detail(&f, id).await;
    let p = &ready["plan"];
    let body = json!({"request_id":Uuid::new_v4(),"plan_id":p["id"],"plan_revision":p["revision"].as_str().unwrap().parse::<i64>().unwrap(),"plan_digest":p["digest"],"acknowledged_eligible_count":0,"acknowledged_coverage":true,"acknowledged_exclusions":true,"acknowledged_name_clears":0,"acknowledged_assignment_clears":0,"acknowledged_contact_removals":0});
    assert!(matches!(
        refresh::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            serde_json::from_value(body.clone()).unwrap(),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let mut accepted = body;
    accepted["acknowledged_eligible_count"] = json!(1);
    refresh::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(accepted).unwrap(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert!(
        admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, None)
            .await
            .unwrap()
    );
    let paused = detail(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(
        refresh::retry(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            lifecycle(&paused),
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap()["state"],
        "queued"
    );
    drain(&f).await;
    let terminal = detail(&f, id).await;
    assert_eq!(terminal["state"], "completed");
    assert!(matches!(
        refresh::cancel(&f.pool, &f.key, &f.ctx, id, lifecycle(&terminal)).await,
        Err(MigrationError::Conflict)
    ));
    assert_eq!(detail(&f, id).await, terminal);
    let settled = get(
        &f,
        &format!("/api/migrations/fub/admitted-people-refreshes/{id}/items?disposition=settled"),
    )
    .await;
    assert_eq!(settled["items"].as_array().unwrap().len(), 1);
    assert_eq!(settled["items"][0]["disposition"], "settled");
    assert!(settled["items"][0]["settled_at"].is_string());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_refresh_readiness_rejects_partial_identity_and_mapping_schema(migrator: PgPool) {
    use crm_api::auth::workspace::startup_compatible;
    let release = ReleaseReadiness::for_tests();
    let mut connection = migrator.acquire().await.unwrap();
    startup_compatible(&mut connection).await.unwrap();
    release
        .require_admitted_people_refresh(&mut connection)
        .await
        .unwrap();
    // Each mutation is confined to a rolled-back transaction in this disposable
    // schema. 00008 installs source_account_id and the exact permit atomically.
    for column in [
        "baseline_version",
        "baseline_result_id",
        "stage_mapping_id",
        "assignee_mapping_id",
        "source_account_id",
        "source_semantic_hmac",
    ] {
        let mut tx = migrator.begin().await.unwrap();
        sqlx::query(&format!("ALTER TABLE migration_admitted_people_refresh_item RENAME COLUMN {column} TO unavailable_column"))
            .execute(&mut *tx).await.unwrap();
        assert!(
            startup_compatible(&mut tx).await.is_err(),
            "startup missing {column}"
        );
        assert!(
            release
                .require_admitted_people_refresh(&mut tx)
                .await
                .is_err(),
            "confirmation missing {column}"
        );
        tx.rollback().await.unwrap();
    }
    for alteration in [
        "ALTER COLUMN source_account_id TYPE text USING source_account_id::text",
        "ALTER COLUMN source_account_id SET NOT NULL",
        "ALTER COLUMN stage_mapping_id SET NOT NULL",
    ] {
        let mut tx = migrator.begin().await.unwrap();
        sqlx::query(&format!(
            "ALTER TABLE migration_admitted_people_refresh_item {alteration}"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            startup_compatible(&mut tx).await.is_err(),
            "startup {alteration}"
        );
        assert!(
            release
                .require_admitted_people_refresh(&mut tx)
                .await
                .is_err(),
            "confirmation {alteration}"
        );
        tx.rollback().await.unwrap();
    }
    startup_compatible(&mut connection).await.unwrap();
    release
        .require_admitted_people_refresh(&mut connection)
        .await
        .unwrap();
}

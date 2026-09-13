//! Server-qualified preparation availability regression coverage for 010e4.
use crate::{common, db_admitted_people_refresh_execution as execution, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::admitted_people_refresh::{self as refresh, AvailabilityQuery},
};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn availability(f: &Fixture, admission_id: Uuid, report_id: Uuid) -> serde_json::Value {
    refresh::availability(
        &f.pool,
        &f.ctx,
        AvailabilityQuery {
            admission_id,
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn availability_reuses_prepare_qualification_and_hides_foreign_bindings(migrator: PgPool) {
    let (f, parent, admission) = execution::fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let report = execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Newer","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let allowed = availability(&f, admission, report).await;
    assert_eq!(allowed["available"], true);
    assert_eq!(allowed["admission_id"], admission.to_string());
    assert_eq!(allowed["report_id"], report.to_string());
    assert_eq!(allowed["parent_import_id"], parent.to_string());
    assert_eq!(allowed["closed_reason_code"], serde_json::Value::Null);

    refresh::prepare(
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
    let active = availability(&f, admission, report).await;
    assert_eq!(active["available"], false);
    assert_eq!(active["closed_reason_code"], "active_refresh_exists");

    let foreign = common::get_with_cookie(
        &f.app,
        &format!(
            "/api/migrations/fub/admitted-people-refreshes/availability?admission_id={}&report_id={}",
            Uuid::new_v4(),
            report
        ),
        &f.cookie,
    )
    .await;
    assert_eq!(foreign.status(), 404);
    assert_eq!(foreign.headers()["cache-control"], "no-store");

    let (foreign_fixture, foreign_parent, foreign_admission) = execution::fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Foreign","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let foreign_report = execution::report(
        &foreign_fixture,
        foreign_parent,
        vec![json!({"id":104,"firstName":"Foreign newer","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let foreign = common::get_with_cookie(
        &f.app,
        &format!(
            "/api/migrations/fub/admitted-people-refreshes/availability?admission_id={foreign_admission}&report_id={foreign_report}",
        ),
        &f.cookie,
    )
    .await;
    assert_eq!(foreign.status(), 404);
    assert_eq!(foreign.headers()["cache-control"], "no-store");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn availability_rejects_an_older_or_equal_boundary_after_a_successor(migrator: PgPool) {
    let (f, parent, admission) = execution::fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let earlier = execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Earlier","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let successor = execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Successor","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let successor_started: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT s.started_at FROM migration_core_change_report r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2",
    )
    .bind(successor)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let earlier_source = sqlx::query(
        "SELECT r.parent_plan_id,r.source_account_id,r.newer_snapshot_id,r.newer_sequence,s.started_at,s.completed_at,o.workspace_revision FROM migration_core_change_report r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id JOIN organization o ON o.id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2",
    )
    .bind(earlier)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO migration_admitted_people_refresh(id,organization_id,admission_id,parent_import_id,parent_plan_id,report_id,source_account_id,newer_snapshot_id,newer_sequence,newer_started_at,newer_completed_at,workspace_revision,initiated_by_user_id,engine_version,state,confirmed_boundary,confirmed_snapshot_id,confirmed_started_at,confirmed_completed_at,completed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'fub-admitted-people-refresh-v1','completed',$9,$8,$10,$14,clock_timestamp())")
        .bind(Uuid::new_v4())
        .bind(f.org)
        .bind(admission)
        .bind(parent)
        .bind(earlier_source.get::<Uuid, _>("parent_plan_id"))
        .bind(earlier)
        .bind(earlier_source.get::<i64, _>("source_account_id"))
        .bind(earlier_source.get::<Uuid, _>("newer_snapshot_id"))
        .bind(earlier_source.get::<i64, _>("newer_sequence"))
        .bind(earlier_source.get::<chrono::DateTime<chrono::Utc>, _>("started_at"))
        .bind(earlier_source.get::<chrono::DateTime<chrono::Utc>, _>("completed_at"))
        .bind(earlier_source.get::<i64, _>("workspace_revision"))
        .bind(f.actor)
        .bind(successor_started)
        .execute(&f.pool)
        .await
        .unwrap();

    let rejected = availability(&f, admission, successor).await;
    assert_eq!(rejected["available"], false);
    assert_eq!(rejected["closed_reason_code"], "source_boundary_not_newer");
}

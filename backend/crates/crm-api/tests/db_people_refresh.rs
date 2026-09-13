//! D-076 baseline proof through the real 010c parent, retained core report and refresh worker.
use crate::{db_activity_source, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, core_change_worker, people_refresh, people_refresh_worker,
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_source::Stream,
    },
};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn recapture(f: &import_support::Fixture) -> Uuid {
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let v = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection.get("id"),
            expected_revision: connection.get("revision"),
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(v["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    id
}
async fn report(f: &import_support::Fixture, parent: Uuid, newer: Uuid) -> Uuid {
    let v = core_change_reports::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        core_change_reports::PrepareCoreChangeReport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            newer_snapshot_id: newer,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(v["report_id"].as_str().unwrap()).unwrap();
    for _ in 0..100 {
        if !core_change_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    assert_eq!(
        core_change_reports::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"],
        "completed"
    );
    id
}
async fn prepare(f: &import_support::Fixture, report: Uuid) -> Uuid {
    let v = people_refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_refresh::PreparePeopleRefresh {
            request_id: Uuid::new_v4(),
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(v["refresh_id"].as_str().unwrap()).unwrap();
    assert!(people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    id
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn original_baseline_distinguishes_current_and_local_contact_state(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::People,vec![json!({"id":101,"firstName":"Synthetic Renamed","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}]}),json!({"id":103,"firstName":"Synthetic held","stage":"Lead","isTrash":true})]);
    let newer = recapture(&f).await;
    let report = report(&f, parent, newer).await;
    let refresh = prepare(&f, report).await;
    let items = people_refresh::items(
        &f.pool,
        &f.key,
        &f.ctx,
        refresh,
        people_refresh::Page {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let rows = items["items"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|v| v["source_id"] == "101" && v["disposition"] == "eligible"));
    assert!(rows
        .iter()
        .any(|v| v["source_id"] == "102" && v["disposition"] == "already_current"));
    let p: Uuid = sqlx::query_scalar(
        "SELECT person_id FROM migration_import_result WHERE import_id=$1 AND source_id='101'",
    )
    .bind(parent)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE person SET first_name='Local edit' WHERE id=$1")
        .bind(p)
        .execute(&migrator)
        .await
        .unwrap();
    let rerun = people_refresh::repreview(
        &f.pool,
        &f.key,
        &f.ctx,
        refresh,
        people_refresh::RepreviewPeopleRefresh {
            request_id: Uuid::new_v4(),
            expected_plan_revision: 1,
        },
    )
    .await
    .unwrap();
    assert_eq!(rerun["state"], "preparing");
    assert!(people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let held:String=sqlx::query_scalar("SELECT i.disposition FROM migration_people_refresh_item i JOIN migration_people_refresh_plan p ON p.id=i.plan_id WHERE i.refresh_id=$1 AND i.source_id='101' ORDER BY p.revision DESC LIMIT 1").bind(refresh).fetch_one(&f.pool).await.unwrap();
    assert_eq!(held, "held_local_change");
}

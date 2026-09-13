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
    for _ in 0..100 {
        if people_refresh::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"]
            == "ready"
        {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
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
    // The retained report's non-imported source group remains reviewable;
    // preparation never silently drops it while building executable items.
    assert!(rows.iter().any(|v| {
        v["source_id"] == "103"
            && matches!(
                v["disposition"].as_str(),
                Some("held_original_hold" | "not_seen_again" | "held_evidence_gap")
            )
    }));
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
    for _ in 0..100 {
        let state: String =
            sqlx::query_scalar("SELECT state FROM migration_people_refresh WHERE id=$1")
                .bind(refresh)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        if state == "ready" {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    let held:String=sqlx::query_scalar("SELECT i.disposition FROM migration_people_refresh_item i JOIN migration_people_refresh_plan p ON p.id=i.plan_id WHERE i.refresh_id=$1 AND i.source_id='101' ORDER BY p.revision DESC LIMIT 1").bind(refresh).fetch_one(&f.pool).await.unwrap();
    assert_eq!(held, "held_local_change");
}

fn many_people(count: i64) -> Vec<serde_json::Value> {
    (1..=count)
        .map(|id| {
            json!({
                "id": id,
                "firstName": format!("Chunk {id}"),
                "stage": "Lead",
                "phones": [{"value": format!("415555{:04}", id)}],
            })
        })
        .collect()
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn preparation_resumes_after_fifty_descriptors_without_rebuilding_plan(migrator: PgPool) {
    let people = many_people(51);
    let f = import_support::fixture(&migrator, people.clone()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let mut newer_people = people;
    // This final report descriptor forces group traversal to advance across
    // the first fifty imported source IDs before reaching a source-only row.
    newer_people.push(json!({
        "id": 52,
        "firstName": "Source only",
        "stage": "Lead",
        "phones": [{"value": "4155550052"}],
    }));
    f.reader.set_records(Stream::People, newer_people);
    let newer = recapture(&f).await;
    let core_report = report(&f, parent, newer).await;
    let created = people_refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_refresh::PreparePeopleRefresh {
            request_id: Uuid::new_v4(),
            report_id: core_report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let refresh = Uuid::parse_str(created["refresh_id"].as_str().unwrap()).unwrap();

    assert!(people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let first = sqlx::query(
        "SELECT state,preparation_phase,preparation_checkpoint_key FROM migration_people_refresh WHERE id=$1",
    )
    .bind(refresh)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(first.get::<String, _>("state"), "preparing");
    assert_eq!(first.get::<String, _>("preparation_phase"), "imported");
    assert!(!first
        .get::<String, _>("preparation_checkpoint_key")
        .is_empty());
    let first_plan: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_people_refresh_plan WHERE refresh_id=$1 AND state='building'",
    )
    .bind(refresh)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_refresh_item WHERE refresh_id=$1 AND plan_id=$2",
        )
        .bind(refresh)
        .bind(first_plan)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        50
    );

    for _ in 0..10 {
        if sqlx::query_scalar::<_, String>("SELECT state FROM migration_people_refresh WHERE id=$1")
            .bind(refresh)
            .fetch_one(&f.pool)
            .await
            .unwrap()
            == "ready"
        {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM migration_people_refresh WHERE id=$1")
            .bind(refresh)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "ready"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_refresh_plan WHERE refresh_id=$1",
        )
        .bind(refresh)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_refresh_item WHERE refresh_id=$1 AND plan_id=$2",
        )
        .bind(refresh)
        .bind(first_plan)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        52
    );
    let source_only: String = sqlx::query_scalar(
        "SELECT disposition FROM migration_people_refresh_item
          WHERE refresh_id=$1 AND plan_id=$2 AND source_id='52'",
    )
    .bind(refresh)
    .bind(first_plan)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(source_only, "excluded_source_only");
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn cancelled_confirmed_input_can_be_repreviewed_from_the_same_snapshot(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(
        Stream::People,
        vec![
            json!({"id":101,"firstName":"Changed once","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
            json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}]}),
            json!({"id":103,"firstName":"Synthetic held","stage":"Lead","isTrash":true}),
        ],
    );
    let newer = recapture(&f).await;
    let core_report = report(&f, parent, newer).await;
    let first = prepare(&f, core_report).await;
    let detail = people_refresh::detail(&f.pool, &f.key, &f.ctx, first)
        .await
        .unwrap();
    let plan = &detail["plan"];
    people_refresh::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        first,
        people_refresh::ConfirmPeopleRefresh {
            request_id: Uuid::new_v4(),
            plan_id: Uuid::parse_str(plan["id"].as_str().unwrap()).unwrap(),
            plan_revision: plan["revision"].as_str().unwrap().parse().unwrap(),
            plan_digest: plan["digest"].as_str().unwrap().to_owned(),
            acknowledged_coverage: true,
            acknowledged_exclusions: true,
            acknowledged_name_clears: plan["counts"]["name_clears"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            acknowledged_assignment_clears: plan["counts"]["assignment_clears"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            acknowledged_contact_removals: plan["counts"]["contact_removals"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let lifecycle_revision: i64 =
        sqlx::query_scalar("SELECT lifecycle_revision FROM migration_people_refresh WHERE id=$1")
            .bind(first)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    people_refresh::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        first,
        people_refresh::LifecyclePeopleRefresh {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: lifecycle_revision,
        },
    )
    .await
    .unwrap();

    let replay = people_refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_refresh::PreparePeopleRefresh {
            request_id: Uuid::new_v4(),
            report_id: core_report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .expect("the same retained snapshot remains valid after cancellation");
    let replay_id = Uuid::parse_str(replay["refresh_id"].as_str().unwrap()).unwrap();
    assert_ne!(replay_id, first);
    for _ in 0..10 {
        if sqlx::query_scalar::<_, String>("SELECT state FROM migration_people_refresh WHERE id=$1")
            .bind(replay_id)
            .fetch_one(&f.pool)
            .await
            .unwrap()
            == "ready"
        {
            return;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("same-input re-preview did not become ready");
}

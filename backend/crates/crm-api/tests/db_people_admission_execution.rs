//! D-078 real retained-capture admission acceptance.  This drives the existing
//! 010c parent and 010e1 report; no plan, result, identity, or provenance rows
//! are fabricated by the test.
use crate::{db_activity_source, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, core_change_worker, people_admission, people_admission_worker,
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_source::Stream,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
fn number(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}

async fn report(f: &import_support::Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    f.reader.set_records(Stream::People, people);
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let snapshot = snapshot::propose(
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
    let newer = uuid(&snapshot["snapshot"]["id"]);
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        newer,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    let report = core_change_reports::prepare(
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
    let id = uuid(&report["report_id"]);
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

async fn ready(f: &import_support::Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    let report = report(f, parent, people).await;
    let value = people_admission::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_admission::PreparePeopleAdmission {
            request_id: Uuid::new_v4(),
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = uuid(&value["admission_id"]);
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"]
            == "ready"
        {
            return id;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("admission preview did not seal")
}

async fn drain(f: &import_support::Fixture, id: Uuid) {
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"]
            == "completed"
        {
            return;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("admission execution did not complete")
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn retained_new_people_admit_with_native_identity_provenance_and_facts(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id=ready(&f,parent,vec![
        json!({"id":101,"firstName":"Synthetic One","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
        json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}]}),
        json!({"id":103,"firstName":"Synthetic held","stage":"Lead","isTrash":true}),
        json!({"id":104,"firstName":"New Person","lastName":"Unicode λ","stage":"Lead","emails":[{"value":"new@example.test"}],"phones":[{"value":"4155550104"}]})
    ]).await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let mappings: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('kind',kind,'source_key',source_key,'disposition',disposition,'qualified',qualified,'target_id',target_id)),'[]'::jsonb) FROM migration_import_mapping WHERE plan_id=(SELECT parent_plan_id FROM migration_people_admission WHERE id=$1)").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        number(&detail["plan"]["counts"]["eligible"]),
        1,
        "{detail} {mappings}"
    );
    let items = people_admission::items(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::Page {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let new = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["source_id"] == "104")
        .unwrap();
    assert_eq!(new["disposition"], "eligible");
    let plan = &detail["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&plan["id"]),
            plan_revision: number(&plan["revision"]),
            plan_digest: plan["digest"].as_str().unwrap().into(),
            eligible_count: number(&plan["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f, id).await;
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='104' AND disposition='settled'").bind(id).fetch_one(&f.pool).await.unwrap();
    let contacts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contact_method WHERE organization_id=$1 AND person_id=$2",
    )
    .bind(f.org)
    .bind(person)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(contacts, 2);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=17 AND family='people' AND source_id='104' AND admission_id=$2 AND target_id=$3").bind(f.org).bind(id).bind(person).fetch_one(&f.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_admitted WHERE organization_id=$1 AND person_id=$2 AND admission_id=$3").bind(f.org).bind(person).bind(id).fetch_one(&f.pool).await.unwrap(),1);
    let provenance = people_admission::admission_provenance(&f.pool, &f.key, &f.ctx, person)
        .await
        .unwrap();
    assert_eq!(provenance["provenance"]["source_id"], "104");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inquiry WHERE organization_id=$1 AND person_id=$2"
        )
        .bind(f.org)
        .bind(person)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    let direct = sqlx::query(
        "INSERT INTO person(id,organization_id,first_name,stage_id) VALUES($1,$2,'forbidden',$3)",
    )
    .bind(Uuid::new_v4())
    .bind(f.org)
    .bind(f.lead_stage)
    .execute(&f.pool)
    .await;
    assert!(direct.is_err(), "an app connection without an exact admission permit must not write review-workspace People");
}

//! D-084 admitted notes/tasks acceptance: a sealed terminal admission and later
//! retained core snapshot are the only source. The fixture has no live source calls.
use crate::{
    db_admitted_people_refresh_execution::{fixture_with_admission, report},
    import_support::Fixture,
};
use crm_api::domain::migration::{
    admitted_activity::{
        self, ActivityPage, Choice, ConfirmAdmittedActivityImport, MappingPatch,
        PlanAdmittedActivityImport, PrepareAdmittedActivityImport,
    },
    admitted_activity_worker,
    snapshot_source::Stream,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().expect("UUID string")).expect("valid UUID")
}

async fn drain(fixture: &Fixture) {
    let reads = fixture.reader.calls();
    for _ in 0..500 {
        if !admitted_activity_worker::run_once(&fixture.pool, &fixture.key, &fixture.policy)
            .await
            .expect("worker result")
        {
            assert_eq!(
                fixture.reader.calls(),
                reads,
                "activity worker is retained-only"
            );
            return;
        }
    }
    panic!("admitted activity worker exceeded bounded synthetic work")
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_freezes_terminal_cohort_full_source_and_global_owner(migrator: PgPool) {
    let people = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"Activity","stage":"Lead","assignedUserId":3}),
    ];
    let (fixture, parent, admission) = fixture_with_admission(&migrator, people.clone()).await;
    fixture.reader.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Retained author","timezone":"America/Los_Angeles"})],
    );
    fixture.reader.set_records(Stream::Notes, vec![json!({"id":11,"personId":104,"body":"list projection only","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false})]);
    fixture.reader.set_raw(Stream::NoteDetail, 0, 200, serde_json::to_vec(&json!({"id":11,"personId":104,"createdById":3,"subject":"Imported note","body":"retained note body","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null,"type":"Note"})).unwrap(), false);
    fixture.reader.set_records(Stream::TasksOpen, vec![json!({"id":21,"personId":104,"name":"Call admitted Person","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z","updated":null})]);
    fixture.reader.set_records(Stream::TasksCompleted, vec![]);
    let selected = report(&fixture, parent, people).await;
    let created = admitted_activity::prepare(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        PrepareAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id: selected,
        },
        &fixture.policy,
    )
    .await
    .expect("prepare accepted terminal cohort");
    let root = uuid(&created["import"]["id"]);
    drain(&fixture).await;
    let detail = admitted_activity::detail(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        &fixture.policy,
    )
    .await
    .expect("ready detail");
    assert_eq!(detail["state"], "ready");
    assert_eq!(detail["admission_id"], admission.to_string());
    assert_eq!(detail["source_report_id"], selected.to_string());
    let mappings = admitted_activity::mappings(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        ActivityPage::default(),
    )
    .await
    .expect("bounded mappings");
    let choices = mappings["items"]
        .as_array()
        .expect("mappings array")
        .iter()
        .map(|row| MappingPatch {
            mapping_id: uuid(&row["id"]),
            choice: match row["role"].as_str().expect("role") {
                "task_kind" => Choice::MapKind {
                    native_kind: "call".into(),
                },
                "task_assignee" => Choice::MapExisting {
                    target_id: fixture.member,
                },
                "task_creator" | "note_author" => Choice::MapExisting {
                    target_id: fixture.actor,
                },
                value => panic!("unexpected mapping role {value}"),
            },
        })
        .collect();
    let replanned = admitted_activity::replan(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        PlanAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            expected_plan_id: uuid(&detail["latest_plan"]["id"]),
            choices,
            source_timezone: Some(Some("America/Los_Angeles".into())),
        },
        &fixture.policy,
    )
    .await
    .expect("mapped replan");
    drain(&fixture).await;
    let plan = uuid(&replanned["import"]["latest_plan"]["id"]);
    let owner = sqlx::query("SELECT import_id,plan_id,manifest_id,admitted_import_id,admitted_plan_id,admitted_manifest_id FROM migration_activity_identity WHERE organization_id=$1")
        .bind(fixture.org).fetch_optional(&fixture.pool).await.expect("identity query");
    assert!(owner.is_none(), "preparation has no global identity writes");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM migration_admitted_activity_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND kind IN ('note','task')")
        .bind(root).bind(plan).bind(fixture.org).fetch_one(&fixture.pool).await.expect("manifest count"), 2);
    let frozen: (Uuid, Uuid, Uuid) = sqlx::query_as("SELECT admission_id,admission_plan_id,source_report_id FROM migration_admitted_activity_import WHERE id=$1 AND organization_id=$2")
        .bind(root).bind(fixture.org).fetch_one(&fixture.pool).await.expect("root binding");
    assert_eq!(frozen.0, admission);
    assert_eq!(frozen.2, selected);

    let ready = admitted_activity::detail(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        &fixture.policy,
    )
    .await
    .expect("mapped plan is ready");
    admitted_activity::confirm(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        ConfirmAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&ready["latest_plan"]["id"]),
            expected_revision: ready["revision"].as_str().expect("revision").into(),
            acknowledge_held: ready["latest_plan"]["counts"]["held_count"]
                .as_str()
                .expect("held count")
                .into(),
            acknowledge_source_only: ready["latest_plan"]["counts"]["source_only_count"]
                .as_str()
                .expect("source only count")
                .into(),
        },
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &fixture.policy,
    )
    .await
    .expect("confirm admitted activity");
    drain(&fixture).await;
    let completed = admitted_activity::detail(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        &fixture.policy,
    )
    .await
    .expect("completed detail");
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["activity_revision"], "2");
    let admitted_identities: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1 AND admitted_import_id=$2 AND admitted_plan_id=$3 AND import_id IS NULL AND plan_id IS NULL AND manifest_id IS NULL")
        .bind(fixture.org).bind(root).bind(plan).fetch_one(&fixture.pool).await.expect("admitted identity owners");
    assert_eq!(
        admitted_identities, 2,
        "native inserts own the global identities exclusively"
    );
}

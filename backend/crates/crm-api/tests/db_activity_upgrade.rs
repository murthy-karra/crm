//! A12: the actual additive migration preserves populated 010f1 data.
use crate::{common, db_activity_source as source, import_support};
use axum::http::StatusCode;
use crm_api::{
    auth::workspace,
    domain::{
        envelope::{CommandContext, Origin},
        migration::imports,
        note::{self, AddNote},
        task::{self, CompleteTask, CreateTask, TaskKind},
    },
    ids::{CorrelationId, OrganizationId, PersonId, UserId},
    realtime::Publisher,
};
use serde_json::{json, Value};
use sqlx::{migrate::Migrator, PgPool};
use std::borrow::Cow;
use uuid::Uuid;

const ACTIVITY_VERSION: i64 = 20260920000001;
const PW: &str = "synthetic migration upgrade password";

async fn existing_rows(pool: &PgPool) -> Value {
    let mut values = serde_json::Map::new();
    // These are all populated before the activity migration. Compare full rows,
    // including provenance, ciphertext, source identities and retained budgets.
    for table in [
        "organization",
        "person",
        "contact_method",
        "note",
        "task",
        "migration_workspace",
        "migration_import",
        "migration_import_plan",
        "migration_import_identity",
        "migration_import_result",
        "migration_snapshot",
        "migration_snapshot_storage",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_metadata_import",
        "migration_metadata_plan",
        "migration_metadata_source",
        "migration_metadata_mapping",
        "migration_metadata_manifest",
        "migration_metadata_receipt",
        "migration_metadata_reservation",
    ] {
        let rows:Value=sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM {table} t"))
            .fetch_one(pool).await.unwrap();
        values.insert(table.into(), rows);
    }
    Value::Object(values)
}

#[sqlx::test(migrations = false)]
#[ignore]
async fn populated_010f1_upgrade_preserves_native_and_import_state(migrator: PgPool) {
    let all = sqlx::migrate!("./migrations");
    // SQLx's pinned migration objects keep their original SQL and checksums.
    let before = Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| m.version < ACTIVITY_VERSION)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    before.run(&migrator).await.unwrap();
    assert!(sqlx::query_scalar::<_, Option<String>>(
        "SELECT to_regclass('migration_activity_import')::text"
    )
    .fetch_one(&migrator)
    .await
    .unwrap()
    .is_none());

    let (org, actor) = common::create_org_with_stages_and_member(
        &migrator,
        "Synthetic ordinary upgrade",
        "activity-upgrade@synthetic.test",
        "Upgrade member",
        PW,
    )
    .await;
    let app = common::connect_as_app(&migrator).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(&app)
    .await
    .unwrap();
    let person:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id) VALUES($1,'Preserved native Person',$2) RETURNING id").bind(org).bind(stage).fetch_one(&app).await.unwrap();
    let ctx = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(actor),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let publisher = Publisher::recording();
    note::add_note(
        &app,
        &publisher,
        &ctx,
        AddNote {
            person_id: PersonId::new(person),
            body: "Preserved ordinary Unicode note 🏡".into(),
        },
    )
    .await
    .unwrap();
    for completed in [false, true] {
        let t = task::create_task(
            &app,
            &publisher,
            &ctx,
            CreateTask {
                person_id: PersonId::new(person),
                title: if completed {
                    "Preserved completed task"
                } else {
                    "Preserved open task"
                }
                .into(),
                kind: TaskKind::FollowUp,
                due_at: None,
                assignee_user_id: None,
            },
        )
        .await
        .unwrap();
        if completed {
            task::complete_task(
                &app,
                &publisher,
                &ctx,
                CompleteTask {
                    person_id: PersonId::new(person),
                    task_id: t.id,
                },
            )
            .await
            .unwrap();
        }
    }
    let book = source::book();
    book.set_records(crm_api::domain::migration::snapshot_source::Stream::People,vec![json!({"id":101,"firstName":"Preserved imported Person","stage":"Lead","assignedUserId":3,"tags":["Preserved source tag"],"phones":[{"value":"4155550100"}]})]);
    let f = import_support::fixture_with_book(&migrator, book).await;
    let (parent, _) = import_support::propose(&f).await;
    import_support::drain_import(&f).await;
    let parent_detail = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    assert_eq!(parent_detail["state"], "proposed");
    // Current binaries correctly reject confirmation on an old schema. This
    // inert old-schema metadata row tests preservation only, not a legal child
    // lifecycle. Real parent/child execution is covered by lifecycle and API QA.
    let sibling_id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_metadata_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state) SELECT $1,organization_id,id,latest_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,1,$3,'cancelled' FROM migration_import WHERE id=$2")
        .bind(sibling_id).bind(parent).bind(f.actor).execute(&migrator).await.unwrap();
    let frozen = existing_rows(&migrator).await;
    assert_eq!(frozen["note"].as_array().unwrap().len(), 1);
    assert_eq!(frozen["task"].as_array().unwrap().len(), 2);
    assert_eq!(
        frozen["migration_metadata_import"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!frozen["migration_snapshot_capture"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!frozen["migration_import_plan"]
        .as_array()
        .unwrap()
        .is_empty());

    let through_activity = Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| m.version <= ACTIVITY_VERSION)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_activity.run(&migrator).await.unwrap();
    assert_eq!(
        existing_rows(&migrator).await,
        frozen,
        "additive upgrade rewrote existing rows"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM migration_activity_import")
            .fetch_one(&app)
            .await
            .unwrap(),
        0
    );
    let mut connection = app.acquire().await.unwrap();
    workspace::startup_compatible(&mut connection)
        .await
        .unwrap();
    drop(connection);
    let router = common::build_router(&migrator).await;
    let cookie = common::login_cookie(&router, "activity-upgrade@synthetic.test", PW).await;
    let response =
        common::get_with_cookie(&router, &format!("/api/people/{person}"), &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = common::body_json(response).await;
    assert_eq!(body["tasks"].as_array().unwrap().len(), 1);
    assert!(body["history"].as_array().unwrap().iter().any(|item| item
        .to_string()
        .contains("Preserved ordinary Unicode note 🏡")));
    assert_eq!(existing_rows(&migrator).await, frozen);
}

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
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().expect("UUID string")).expect("valid UUID")
}

pub(super) async fn drain(fixture: &Fixture) {
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

pub(super) async fn prepared(migrator: &PgPool, task_count: u32) -> (Fixture, Uuid, Uuid, Value) {
    prepared_source(migrator, task_count, None, vec![]).await
}
async fn prepared_source(
    migrator: &PgPool,
    task_count: u32,
    selected_people: Option<Vec<Value>>,
    extra_tasks: Vec<Value>,
) -> (Fixture, Uuid, Uuid, Value) {
    let extra_count = extra_tasks.len();
    let people = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"Activity","stage":"Lead","assignedUserId":3}),
    ];
    let (fixture, parent, admission) = fixture_with_admission(migrator, people.clone()).await;
    fixture.reader.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Retained author","timezone":"America/Los_Angeles"})],
    );
    fixture.reader.set_records(Stream::Notes, vec![json!({"id":11,"personId":104,"body":"list projection only","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false})]);
    fixture.reader.set_raw(Stream::NoteDetail, 0, 200, serde_json::to_vec(&json!({"id":11,"personId":104,"createdById":3,"subject":"Imported note","body":"retained note body","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null,"type":"Note"})).unwrap(), false);
    fixture.reader.set_records(Stream::TasksOpen, (21..21+task_count).map(|id| json!({"id":id,"personId":104,"name":"Call admitted Person","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z","updated":null})).chain(extra_tasks).collect());
    fixture.reader.set_records(Stream::TasksCompleted, vec![]);
    let selected = report(&fixture, parent, selected_people.unwrap_or(people)).await;
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
        .bind(root).bind(plan).bind(fixture.org).fetch_one(&fixture.pool).await.expect("manifest count"), i64::from(task_count)+1+extra_count as i64);
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
    (fixture, parent, root, ready)
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_freezes_terminal_cohort_full_source_and_global_owner(migrator: PgPool) {
    let (fixture, _parent, root, ready) = prepared(&migrator, 1).await;
    let confirmed = admitted_activity::confirm(
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
    let cancelled = admitted_activity::action(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        admitted_activity::AdmittedActivityAction {
            request_id: Uuid::new_v4(),
            expected_revision: confirmed["import"]["revision"]
                .as_str()
                .expect("revision")
                .into(),
        },
        false,
        &fixture.policy,
    )
    .await
    .expect("cancel the confirmed attempt before a worker unit settles");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_activity_result WHERE import_id=$1 AND organization_id=$2",
        )
        .bind(root)
        .bind(fixture.org)
        .fetch_one(&fixture.pool)
        .await
        .expect("zero-write cancellation count"),
        0,
        "a confirmed cancellation preserves the durable reader fence without a native write",
    );
    let successor = admitted_activity::create_remainder(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        admitted_activity::CreateAdmittedActivityRemainder {
            request_id: Uuid::new_v4(),
            expected_revision: cancelled["import"]["revision"]
                .as_str()
                .expect("revision")
                .into(),
        },
        &fixture.policy,
    )
    .await
    .expect("construct exact never-settled successor");
    let successor_id = uuid(&successor["import"]["id"]);
    let successor_plan = uuid(&successor["import"]["latest_plan"]["id"]);
    let second_successor = admitted_activity::create_remainder(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        admitted_activity::CreateAdmittedActivityRemainder {
            request_id: Uuid::new_v4(),
            expected_revision: cancelled["import"]["revision"]
                .as_str()
                .expect("revision")
                .into(),
        },
        &fixture.policy,
    )
    .await;
    assert!(matches!(
        second_successor,
        Err(crm_api::domain::migration::MigrationError::ImportConflict)
    ));
    drain(&fixture).await;
    let completed = admitted_activity::detail(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        successor_id,
        &fixture.policy,
    )
    .await
    .expect("completed detail");
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["activity_revision"], "2");
    let admitted_identities: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1 AND admitted_import_id=$2 AND admitted_plan_id=$3 AND import_id IS NULL AND plan_id IS NULL AND manifest_id IS NULL")
        .bind(fixture.org).bind(successor_id).bind(successor_plan).fetch_one(&fixture.pool).await.expect("admitted identity owners");
    assert_eq!(
        admitted_identities, 2,
        "native inserts own the global identities exclusively"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_unconfirmed_cancel_can_be_prepared_again(migrator: PgPool) {
    let people = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"Retry","stage":"Lead","assignedUserId":3}),
    ];
    let (fixture, parent, admission) = fixture_with_admission(&migrator, people.clone()).await;
    for stream in [
        Stream::Users,
        Stream::Notes,
        Stream::TasksOpen,
        Stream::TasksCompleted,
    ] {
        fixture.reader.set_records(stream, vec![]);
    }
    let selected = report(&fixture, parent, people).await;
    let cmd = PrepareAdmittedActivityImport {
        request_id: Uuid::new_v4(),
        admission_id: admission,
        report_id: selected,
    };
    let first = admitted_activity::prepare(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        cmd,
        &fixture.policy,
    )
    .await
    .expect("first prepare");
    let root = uuid(&first["import"]["id"]);
    admitted_activity::action(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        root,
        admitted_activity::AdmittedActivityAction {
            request_id: Uuid::new_v4(),
            expected_revision: first["import"]["revision"]
                .as_str()
                .expect("revision")
                .into(),
        },
        false,
        &fixture.policy,
    )
    .await
    .expect("unconfirmed cancel");
    let second = admitted_activity::prepare(
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
    .expect("reprepare after an unconfirmed cancellation");
    assert_ne!(uuid(&second["import"]["id"]), root);
}

async fn current(f: &Fixture, root: Uuid) -> Value {
    admitted_activity::detail(&f.pool, &f.key, &f.ctx, root, &f.policy)
        .await
        .unwrap()
}

pub(super) async fn confirm_ready(f: &Fixture, root: Uuid, ready: &Value) -> Value {
    admitted_activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        ConfirmAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&ready["latest_plan"]["id"]),
            expected_revision: ready["revision"].as_str().unwrap().into(),
            acknowledge_held: ready["latest_plan"]["counts"]["held_count"]
                .as_str()
                .unwrap()
                .into(),
            acknowledge_source_only: ready["latest_plan"]["counts"]["source_only_count"]
                .as_str()
                .unwrap()
                .into(),
        },
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap()
}

fn action(v: &Value) -> admitted_activity::AdmittedActivityAction {
    admitted_activity::AdmittedActivityAction {
        request_id: Uuid::new_v4(),
        expected_revision: v["revision"].as_str().unwrap().into(),
    }
}

// Exclude lifecycle revision/state: recording a pause is itself a durable change.
// Everything attributable to a failed work unit must remain byte-for-byte equal.
async fn unit_state(f: &Fixture, root: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('checkpoint',checkpoint_id,'activity_revision',activity_revision,'retained_bytes',retained_bytes,'measured_bytes',measured_bytes,'native_bytes',native_bytes,'counts',counts,'ledger',(SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$2),'control_bytes',crm_admitted_activity_retained_size('migration_admitted_activity_import',to_jsonb(i))) FROM migration_admitted_activity_import i WHERE id=$1 AND organization_id=$2")
        .bind(root).bind(f.org).fetch_one(&f.pool).await.unwrap()
}

async fn assert_bytes(f: &Fixture, root: Uuid) {
    let r = sqlx::query("SELECT measured_bytes,retained_bytes,reserved_bytes FROM migration_admitted_activity_import WHERE id=$1 AND organization_id=$2").bind(root).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        r.get::<i64, _>("measured_bytes"),
        r.get::<i64, _>("retained_bytes")
    );
    assert_eq!(r.get::<i64, _>("reserved_bytes"), 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_result_failure_rolls_back_native_identity_checkpoint_and_bytes(
    migrator: PgPool,
) {
    let (f, parent, root, ready) = prepared(&migrator, 1).await;
    let parent_before: Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM migration_import p WHERE id=$1")
            .bind(parent)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    confirm_ready(&f, root, &ready).await;
    let before = unit_state(&f, root).await;
    sqlx::raw_sql("CREATE FUNCTION synthetic_admitted_result_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='P0099',MESSAGE='synthetic admitted result failure'; END $$; CREATE TRIGGER synthetic_admitted_result_failure BEFORE INSERT ON migration_admitted_activity_result FOR EACH ROW EXECUTE FUNCTION synthetic_admitted_result_failure();").execute(&migrator).await.unwrap();
    drain(&f).await;
    let paused = current(&f, root).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "storage_unavailable");
    let after = unit_state(&f, root).await;
    let control_delta =
        after["control_bytes"].as_i64().unwrap() - before["control_bytes"].as_i64().unwrap();
    let mut expected = before.clone();
    for field in ["retained_bytes", "measured_bytes", "ledger"] {
        expected[field] = json!(before[field].as_i64().unwrap() + control_delta);
    }
    expected["control_bytes"] = after["control_bytes"].clone();
    assert_eq!(after,expected,"only the separately committed pause record consumes bytes; failed unit has no checkpoint/counter/native/ledger effect");
    for table in [
        "note",
        "task",
        "migration_activity_identity",
        "migration_admitted_activity_result",
        "migration_admitted_activity_result_issue",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "{table} cannot escape the failed transaction");
    }
    sqlx::raw_sql("DROP TRIGGER synthetic_admitted_result_failure ON migration_admitted_activity_result; DROP FUNCTION synthetic_admitted_result_failure();").execute(&migrator).await.unwrap();
    admitted_activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        action(&paused),
        true,
        &f.policy,
    )
    .await
    .unwrap();
    drain(&f).await;
    assert_eq!(current(&f, root).await["state"], "completed");
    let settled = unit_state(&f, root).await;
    assert_eq!(settled["activity_revision"], 2);
    for (table, expected) in [
        ("note", 1),
        ("task", 1),
        ("migration_activity_identity", 2),
        ("migration_admitted_activity_result", 2),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {table} WHERE organization_id=$1"
            ))
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            expected
        );
    }
    drain(&f).await;
    assert_eq!(
        unit_state(&f, root).await,
        settled,
        "worker replay cannot settle bytes twice"
    );
    let parent_after: Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM migration_import p WHERE id=$1")
            .bind(parent)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        parent_after, parent_before,
        "admitted activity does not rewrite its parent"
    );
    assert_bytes(&f, root).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_authority_loss_two_workers_and_exact_remainder_replay(migrator: PgPool) {
    use crm_api::{domain::migration::MigrationError, ids::UserId};
    let (mut f, _, root, ready) = prepared(&migrator, 8).await;
    let original_actor = f.actor;
    confirm_ready(&f, root, &ready).await;
    assert!(
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
    );
    assert_eq!(current(&f, root).await["activity_revision"], "1");
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
    );
    assert!(matches!(
        admitted_activity::detail(&f.pool, &f.key, &f.ctx, root, &f.policy).await,
        Err(MigrationError::Forbidden)
    ));
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(&migrator)
    .await
    .unwrap();
    f.ctx.actor_user_id = UserId::new(f.member);
    let paused = current(&f, root).await;
    assert_eq!(paused["pause_reason"], "authority_changed");
    assert_eq!(paused["activity_revision"], "1");
    admitted_activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        action(&paused),
        true,
        &f.policy,
    )
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy),
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
    );
    a.unwrap();
    b.unwrap();
    let running = current(&f, root).await;
    let settled = running["activity_revision"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(
        (2..=3).contains(&settled),
        "two workers settle distinct bounded units"
    );
    let cancelled = admitted_activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        action(&running),
        false,
        &f.policy,
    )
    .await
    .unwrap();
    let expected:Vec<(String,String,Uuid)>=sqlx::query_as("SELECT m.kind,m.source_id,m.target_id FROM migration_admitted_activity_manifest m WHERE m.plan_id=$1 AND m.organization_id=$2 AND NOT EXISTS(SELECT 1 FROM migration_admitted_activity_result r WHERE r.import_id=$3 AND r.organization_id=m.organization_id AND r.manifest_id=m.id) ORDER BY m.kind,m.source_id")
        .bind(uuid(&ready["latest_plan"]["id"])).bind(f.org).bind(root).fetch_all(&f.pool).await.unwrap();
    assert_eq!(expected.len() as i64, 9 - settled);
    let command =
        json!({"request_id":Uuid::new_v4(),"expected_revision":cancelled["import"]["revision"]});
    let receipt = admitted_activity::create_remainder(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        serde_json::from_value(command.clone()).unwrap(),
        &f.policy,
    )
    .await
    .unwrap();
    let child = uuid(&receipt["import"]["id"]);
    drain(&f).await;
    let actual:Vec<(String,String,Uuid)>=sqlx::query_as("SELECT kind,source_id,target_id FROM migration_admitted_activity_manifest WHERE import_id=$1 AND organization_id=$2 ORDER BY kind,source_id").bind(child).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        actual, expected,
        "successor preserves exact never-settled targets and source IDs"
    );
    assert_eq!(current(&f, child).await["state"], "completed");
    assert_eq!(
        admitted_activity::create_remainder(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            serde_json::from_value(command.clone()).unwrap(),
            &f.policy
        )
        .await
        .unwrap(),
        receipt,
        "lost-response replay recovers original receipt even after completion"
    );
    let mut changed = command.clone();
    changed["expected_revision"] = json!("99999");
    assert!(matches!(
        admitted_activity::create_remainder(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            serde_json::from_value(changed).unwrap(),
            &f.policy
        )
        .await,
        Err(MigrationError::ImportConflict)
    ));
    f.ctx.actor_user_id = UserId::new(original_actor);
    assert!(
        matches!(
            admitted_activity::create_remainder(
                &f.pool,
                &f.key,
                &f.ctx,
                root,
                serde_json::from_value(command).unwrap(),
                &f.policy
            )
            .await,
            Err(MigrationError::Forbidden)
        ),
        "receipt cannot bypass current admin authority"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        9
    );
    assert_bytes(&f, root).await;
    assert_bytes(&f, child).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_partial_remainder_copy_cancel_preserves_complete_source(
    migrator: PgPool,
) {
    let (f, _, root, ready) = prepared(&migrator, 1).await;
    let queued = confirm_ready(&f, root, &ready).await;
    let cancelled = admitted_activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        action(&queued["import"]),
        false,
        &f.policy,
    )
    .await
    .unwrap();
    let child = admitted_activity::create_remainder(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        admitted_activity::CreateAdmittedActivityRemainder {
            request_id: Uuid::new_v4(),
            expected_revision: cancelled["import"]["revision"].as_str().unwrap().into(),
        },
        &f.policy,
    )
    .await
    .unwrap();
    let child_id = uuid(&child["import"]["id"]);
    assert_eq!(child["import"]["state"], "preparing");
    assert!(
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_activity_mapping WHERE import_id=$1"
        )
        .bind(child_id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1,
        "one bounded copied row"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "partial copy cannot execute native writes"
    );
    let cancelled = admitted_activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        child_id,
        action(&current(&f, child_id).await),
        false,
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(
        cancelled["import"]["actions"]["remainder"], true,
        "partial copy retains a resumable exact source"
    );
    let successor = admitted_activity::create_remainder(
        &f.pool,
        &f.key,
        &f.ctx,
        child_id,
        admitted_activity::CreateAdmittedActivityRemainder {
            request_id: Uuid::new_v4(),
            expected_revision: cancelled["import"]["revision"].as_str().unwrap().into(),
        },
        &f.policy,
    )
    .await
    .unwrap();
    let successor_id = uuid(&successor["import"]["id"]);
    drain(&f).await;
    assert_eq!(current(&f, successor_id).await["state"], "completed");
    assert_eq!(current(&f, successor_id).await["activity_revision"], "2");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        2
    );
    assert_bytes(&f, child_id).await;
    assert_bytes(&f, successor_id).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_readiness_rejects_incomplete_schema_grants_guards_and_owner(
    migrator: PgPool,
) {
    use crm_api::auth::workspace;
    let mut tx = migrator.begin().await.unwrap();
    workspace::ReleaseReadiness::for_tests()
        .require_admitted_activity(&mut tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    for mutation in [
        "DROP TABLE migration_admitted_activity_result_issue",
        "DROP TABLE migration_admitted_activity_manifest_issue",
        "DROP TABLE migration_admitted_activity_issue",
        "REVOKE INSERT ON migration_admitted_activity_result FROM crm_app",
        "GRANT UPDATE ON migration_activity_identity TO crm_app",
        "REVOKE EXECUTE ON FUNCTION crm_admitted_activity_insert_allowed(uuid,text,jsonb) FROM crm_app",
        "GRANT EXECUTE ON FUNCTION crm_admitted_activity_insert_allowed(uuid,text,jsonb) TO PUBLIC",
        "ALTER TABLE migration_admitted_activity_result DISABLE TRIGGER migration_admitted_activity_result_measure",
        "ALTER TABLE migration_activity_identity DROP CONSTRAINT migration_activity_identity_exclusive_owner",
        "ALTER TABLE migration_activity_identity DROP CONSTRAINT migration_activity_identity_admitted_owner_fk",
        "ALTER TABLE migration_admitted_activity_import DROP COLUMN remainder_source_plan_id CASCADE",
        "DROP INDEX migration_admitted_activity_import_claim",
        "CREATE OR REPLACE FUNCTION crm_admitted_activity_insert_allowed(org uuid,table_name text,row_value jsonb) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
    ] {
        let mut tx=migrator.begin().await.unwrap();
        sqlx::query(mutation).execute(&mut *tx).await.unwrap();
        assert!(workspace::ReleaseReadiness::for_tests().require_admitted_activity(&mut tx).await.is_err(),"readiness accepted incomplete installation: {mutation}");
        tx.rollback().await.unwrap();
    }
    let mut tx = migrator.begin().await.unwrap();
    workspace::startup_compatible(&mut tx).await.unwrap();
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_source_binding_requires_all_six_streams_and_frozen_output(
    migrator: PgPool,
) {
    use crm_api::domain::migration::MigrationError;
    let (f, _, root, ready) = prepared(&migrator, 1).await;
    let (snapshot,output):(Uuid,Uuid)=sqlx::query_as("SELECT snapshot_id,source_output_revision FROM migration_admitted_activity_import WHERE id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
    for stream in [
        "people",
        "users",
        "notes",
        "note_detail",
        "tasks_open",
        "tasks_completed",
    ] {
        sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND stream=$2").bind(snapshot).bind(stream).execute(&migrator).await.unwrap();
        assert!(
            matches!(
                admitted_activity::detail(&f.pool, &f.key, &f.ctx, root, &f.policy).await,
                Err(MigrationError::SourceNotEligible)
            ),
            "required exhausted stream {stream}"
        );
        sqlx::query("UPDATE migration_snapshot_stream SET state='completed' WHERE snapshot_id=$1 AND stream=$2").bind(snapshot).bind(stream).execute(&migrator).await.unwrap();
    }
    sqlx::query(
        "UPDATE migration_admitted_activity_import SET source_output_revision=$2 WHERE id=$1",
    )
    .bind(root)
    .bind(Uuid::new_v4())
    .execute(&migrator)
    .await
    .unwrap();
    assert!(matches!(
        admitted_activity::detail(&f.pool, &f.key, &f.ctx, root, &f.policy).await,
        Err(MigrationError::SourceNotEligible)
    ));
    sqlx::query(
        "UPDATE migration_admitted_activity_import SET source_output_revision=$2 WHERE id=$1",
    )
    .bind(root)
    .bind(output)
    .execute(&migrator)
    .await
    .unwrap();
    assert_eq!(
        current(&f, root).await["latest_plan"]["id"],
        ready["latest_plan"]["id"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_metadata_import WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "metadata import is not an activity prerequisite"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_existing_equality_local_edits_tombstones_and_missing_identity_target(
    migrator: PgPool,
) {
    let (f, _, root, ready) = prepared(&migrator, 4).await;
    let plan = uuid(&ready["latest_plan"]["id"]);
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_admitted_activity_manifest WHERE plan_id=$1 AND kind='task' LIMIT 1").bind(plan).fetch_one(&f.pool).await.unwrap();
    for sid in 21..=23 {
        sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id,source,source_external_id,created_at,updated_at,deleted_at,deleted_by_user_id) VALUES($1,$2,$3,'call',$4,$5,'migration',gen_random_uuid(),'fub',$6,'2026-09-01T12:00:00Z','2026-09-01T12:00:00Z',CASE WHEN $7 THEN now() ELSE NULL END,CASE WHEN $7 THEN $5 ELSE NULL END)")
            .bind(f.org).bind(person).bind(if sid==23 {""} else if sid==22 {"Local edit retained"} else {"Call admitted Person"}).bind(f.member).bind(f.actor).bind(format!("v1:17:{sid}")).bind(sid==23).execute(&migrator).await.unwrap();
    }
    sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,admitted_import_id,admitted_plan_id,admitted_manifest_id) SELECT organization_id,17,kind,source_id,target_id,import_id,plan_id,id FROM migration_admitted_activity_manifest WHERE plan_id=$1 AND source_id='24' AND kind='task'").bind(plan).execute(&migrator).await.unwrap();
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let identity_before:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM migration_activity_identity i WHERE organization_id=$1 AND kind='task' AND source_id='24'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    confirm_ready(&f, root, &ready).await;
    drain(&f).await;
    let done = current(&f, root).await;
    assert_eq!(done["state"], "completed");
    let outcomes:Vec<(String,String)>=sqlx::query_as("SELECT source_id,disposition FROM migration_admitted_activity_result WHERE import_id=$1 AND kind='task' ORDER BY source_id").bind(root).fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        outcomes,
        vec![
            ("21".into(), "already_present".into()),
            ("22".into(), "held".into()),
            ("23".into(), "held".into()),
            ("24".into(), "held".into())
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        before,
        "no local edit overwrite or deleted-target resurrection"
    );
    assert_eq!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(i) FROM migration_activity_identity i WHERE organization_id=$1 AND kind='task' AND source_id='24'").bind(f.org).fetch_one(&f.pool).await.unwrap(),identity_before,"missing native target keeps its deletion-protection owner");
    let mut tx = migrator.begin().await.unwrap();
    let duplicate=sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,import_id,plan_id,manifest_id,admitted_import_id,admitted_plan_id,admitted_manifest_id) SELECT organization_id,source_account_id,kind,source_id,target_id,import_id,plan_id,manifest_id,admitted_import_id,admitted_plan_id,admitted_manifest_id FROM migration_activity_identity WHERE organization_id=$1 AND kind='task' AND source_id='24'").bind(f.org).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505")
    );
    tx.rollback().await.unwrap();
    assert_bytes(&f, root).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_http_tenants_private_permit_and_native_review_cursor_fences(
    migrator: PgPool,
) {
    use crate::common::{body_json, get_with_cookie};
    use axum::http::StatusCode;
    use crm_api::{
        auth::{workspace, AuthContext},
        domain::{
            admin::Role,
            migration::{
                activity_review::{self, PageQuery, ReviewError, TaskState},
                commands, MigrationError,
            },
            note,
        },
        ids::{OrganizationId, PersonId, UserId},
        realtime::Publisher,
    };
    let (f, _, root, ready) = prepared(&migrator, 4).await;
    let foreign = crate::import_support::fixture(&migrator, vec![]).await;
    let path = format!("/api/migrations/fub/admitted-activity-imports/{root}");
    for (cookie, status) in [
        (&f.cookie, StatusCode::OK),
        (&f.member_cookie, StatusCode::FORBIDDEN),
        (&foreign.cookie, StatusCode::NOT_FOUND),
    ] {
        let response = get_with_cookie(&f.app, &path, cookie).await;
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    assert!(matches!(
        admitted_activity::detail(&f.pool, &f.key, &foreign.ctx, root, &f.policy).await,
        Err(MigrationError::NotFound)
    ));
    let person: Uuid = sqlx::query_scalar(
        "SELECT person_id FROM migration_admitted_activity_manifest WHERE import_id=$1 LIMIT 1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let ordinary = note::add_note(
        &f.pool,
        &Publisher::recording(),
        &f.ctx,
        note::AddNote {
            person_id: PersonId::new(person),
            body: "Synthetic ordinary write".into(),
        },
    )
    .await
    .err()
    .expect("ordinary admin mutation must fail");
    assert!(
        matches!(ordinary,note::NoteError::Database(ref e) if e.as_database_error().and_then(|e|e.code()).as_deref()==Some("P010C")),
        "admin cannot bypass workspace hold"
    );
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_activity_token',$1,true),set_config('crm.admitted_activity_unit',$2,true)").bind(Uuid::new_v4().to_string()).bind(Uuid::new_v4().to_string()).execute(&mut *tx).await.unwrap();
    let denied=sqlx::query("INSERT INTO note(organization_id,person_id,body,author_user_id,origin,correlation_id,source,source_external_id) VALUES($1,$2,'Untrusted permit',$3,'migration',gen_random_uuid(),'fub','v1:17:999')").bind(f.org).bind(person).bind(f.actor).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("P010C")
    );
    tx.rollback().await.unwrap();
    confirm_ready(&f, root, &ready).await;
    let mut legacy = f.pool.begin().await.unwrap();
    let error = workspace::activity_complete_read(&mut legacy, OrganizationId::new(f.org))
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("P010F"),
        "confirmation fences complete legacy reads before any native insert"
    );
    legacy.rollback().await.unwrap();
    let auth = AuthContext {
        actor_user_id: UserId::new(f.actor),
        actor_email: "admin@synthetic.test".into(),
        actor_display_name: "Synthetic admin".into(),
        active_organization_id: OrganizationId::new(f.org),
        active_organization_name: "Synthetic review".into(),
        role: Role::Admin,
    };
    for _ in 0..5 {
        if sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap()
            >= 2
        {
            break;
        }
        assert!(
            admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
                .await
                .unwrap()
        );
    }
    let page = activity_review::tasks(
        &f.pool,
        &f.key,
        &auth,
        PersonId::new(person),
        TaskState::Open,
        &PageQuery {
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert!(page.next_cursor.is_some());
    assert_eq!(page.items[0]["can_manage"], false);
    assert!(page.items[0]["provenance"]
        .to_string()
        .contains("admitted-activity-imports"));
    // A later admitted commit changes the composite revision atomically with
    // its native row; the old original-only cursor must not remain usable.
    assert!(
        admitted_activity_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
    );
    assert!(matches!(
        activity_review::tasks(
            &f.pool,
            &f.key,
            &auth,
            PersonId::new(person),
            TaskState::Open,
            &PageQuery {
                limit: Some(1),
                cursor: page.next_cursor
            }
        )
        .await,
        Err(ReviewError::RefreshRequired)
    ));
    drain(&f).await;
    let connection: Uuid =
        sqlx::query_scalar("SELECT id FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    commands::disconnect_fub(
        &f.pool,
        &f.ctx,
        commands::DisconnectFub {
            connection_id: connection,
        },
    )
    .await
    .unwrap();
    let response = get_with_cookie(&f.app, &path, &f.cookie).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "disconnect preserves retained review"
    );
    let detail = body_json(response).await;
    assert_eq!(detail["state"], "completed");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_selected_people_missing_and_conflicting_hold_related_units(
    migrator: PgPool,
) {
    for people in [
        vec![],
        vec![
            json!({"id":104,"firstName":"One","stage":"Lead","assignedUserId":3}),
            json!({"id":104,"firstName":"Conflicting","stage":"Lead","assignedUserId":3}),
        ],
    ] {
        let (f, _, root, ready) = prepared_source(&migrator, 1, Some(people), vec![]).await;
        assert_eq!(ready["latest_plan"]["counts"]["held_count"], "2");
        assert_eq!(ready["latest_plan"]["counts"]["notes"]["eligible"], "0");
        let page =
            admitted_activity::records(&f.pool, &f.key, &f.ctx, root, ActivityPage::default())
                .await
                .unwrap();
        assert!(page["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["preview"]["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "selected_person_source_unqualified")));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1"
            )
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0
        );
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_excluded_coverage_never_enters_native_reconciliation(migrator: PgPool) {
    let extra = json!({"id":99,"personId":101,"name":"Outside cohort","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z"});
    let (f, _, root, ready) = prepared_source(&migrator, 1, None, vec![extra]).await;
    let counts = &ready["latest_plan"]["counts"];
    assert_eq!(counts["excluded_count"], "1");
    assert_eq!(counts["tasks"]["planned"], "1");
    assert_eq!(counts["held_count"], "0");
    let page = admitted_activity::records(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        ActivityPage {
            disposition: Some("excluded".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["source_id"], "99");
    confirm_ready(&f, root, &ready).await;
    drain(&f).await;
    let done = current(&f, root).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["counts"]["tasks"]["applied"], "1");
    assert_eq!(done["counts"]["tasks"]["pending"], "0");
    assert_eq!(done["counts"]["excluded_count"], "1");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_activity_result WHERE import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        2
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1 AND source_id='99'").bind(f.org).fetch_one(&f.pool).await.unwrap(),0);
    assert_bytes(&f, root).await;
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_old_reader_and_worker_barrier_fails_after_confirmation(
    migrator: PgPool,
) {
    let (f, _, root, ready) = prepared(&migrator, 1).await;
    let mut old = f.pool.begin().await.unwrap();
    sqlx::query("SELECT crm_workspace_shared($1)")
        .bind(f.org)
        .execute(&mut *old)
        .await
        .unwrap();
    // The old request owns the same barrier; confirmation cannot cut across its read.
    let blocked = admitted_activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        ConfirmAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&ready["latest_plan"]["id"]),
            expected_revision: ready["revision"].as_str().unwrap().into(),
            acknowledge_held: "0".into(),
            acknowledge_source_only: ready["latest_plan"]["counts"]["source_only_count"]
                .as_str()
                .unwrap()
                .into(),
        },
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await;
    assert!(blocked.is_err());
    old.rollback().await.unwrap();
    confirm_ready(&f, root, &ready).await;
    for marker in ["", "fub-history-timeline-v1"] {
        let mut old = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.history_reader',$1,true),set_config('crm.admitted_activity_reader','',true)").bind(marker).execute(&mut *old).await.unwrap();
        let error = sqlx::query("SELECT crm_workspace_shared($1)")
            .bind(f.org)
            .execute(&mut *old)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("P010F")
        );
        old.rollback().await.unwrap();
    }
    // Current stamped workers/readers recover normally with the exact same auth.
    drain(&f).await;
    assert_eq!(current(&f, root).await["state"], "completed");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_activity_corrupt_people_capture_pauses_before_native_work(migrator: PgPool) {
    let (f, _, root, ready) = prepared(&migrator, 1).await;
    sqlx::query("UPDATE migration_snapshot_capture SET ciphertext=set_byte(ciphertext,0,get_byte(ciphertext,0)#1) WHERE snapshot_id=$1 AND stream='people' AND accepted")
        .bind(uuid(&ready["snapshot_id"])).execute(&migrator).await.unwrap();
    admitted_activity::replan(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        PlanAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            expected_plan_id: uuid(&ready["latest_plan"]["id"]),
            choices: vec![],
            source_timezone: None,
        },
        &f.policy,
    )
    .await
    .unwrap();
    drain(&f).await;
    let paused = current(&f, root).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_activity_result WHERE import_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_bytes(&f, root).await;
}

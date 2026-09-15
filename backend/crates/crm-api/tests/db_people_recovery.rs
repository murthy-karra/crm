//! Recovery uses real retained captures and normal original/admission commands.
use crate::{db_activity_source, db_people_admission_execution, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        people_admission, people_admission_worker, people_recovery as recovery,
        snapshot_source::Stream,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
fn id(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
async fn drain(f: &import_support::Fixture) {
    for _ in 0..200 {
        if !people_admission_worker::run_once(
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
    panic!("recovery did not finish bounded fixture");
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn original_mapping_hold_recovers_once_with_honest_fact(migrator: PgPool) {
    let (f, _parent, run) = recovered_fixture(&migrator).await;
    let native=sqlx::query("SELECT p.id,p.first_name FROM person p JOIN migration_import_identity mi ON mi.target_id=p.id AND mi.organization_id=p.organization_id WHERE mi.organization_id=$1 AND mi.family='people' AND mi.source_id='104'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        native.get::<String, _>("first_name"),
        "Recovered current name"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person_recovered WHERE organization_id=$1 AND person_id=$2"
        )
        .bind(f.org)
        .bind(native.get::<Uuid, _>("id"))
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person_admitted WHERE organization_id=$1 AND person_id=$2"
        )
        .bind(f.org)
        .bind(native.get::<Uuid, _>("id"))
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        people_admission::retained_byte_audit(&f.pool, &f.ctx, run)
            .await
            .unwrap(),
        sqlx::query_scalar::<_, i64>(
            "SELECT retained_bytes FROM migration_people_admission WHERE id=$1"
        )
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    );
    drain(&f).await;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND disposition='settled'").bind(run).fetch_one(&f.pool).await.unwrap(),1);
}

async fn recovered_fixture(migrator: &PgPool) -> (import_support::Fixture, Uuid, Uuid) {
    recovery_fixture(migrator, true, 1).await
}
async fn recovery_fixture(
    migrator: &PgPool,
    execute: bool,
    count: i64,
) -> (import_support::Fixture, Uuid, Uuid) {
    recovery_fixture_phase(migrator, execute, count, true).await
}
async fn recovery_fixture_phase(
    migrator: &PgPool,
    execute: bool,
    count: i64,
    confirm: bool,
) -> (import_support::Fixture, Uuid, Uuid) {
    let mut original =
        vec![json!({"id":101,"firstName":"Existing","stage":"Lead","assignedUserId":3})];
    original.extend(
        (104..104 + count).map(
            |source| json!({"id":source,"firstName":"Held","stage":"Later","assignedUserId":3}),
        ),
    );
    let reader = std::sync::Arc::new(import_support::Book::new(original));
    reader.set_records(
        Stream::Stages,
        vec![
            json!({"id":4,"name":"Lead"}),
            json!({"id":5,"name":"Later"}),
        ],
    );
    let f = import_support::fixture_with_book(migrator, reader).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let anchor=sqlx::query("SELECT p.id,p.revision FROM migration_import i JOIN migration_import_plan p ON p.id=i.confirmed_plan_id WHERE i.id=$1").bind(parent).fetch_one(&f.pool).await.unwrap();
    let mut newer =
        vec![json!({"id":101,"firstName":"Existing","stage":"Lead","assignedUserId":3})];
    newer.extend((104..104+count).map(|source|json!({"id":source,"firstName":"Recovered current name","stage":"Later","assignedUserId":3,"phones":[{"value":"4155550111"}]})));
    let report = db_people_admission_execution::report(&f, parent, newer).await;
    let response = recovery::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        recovery::PrepareRecovery {
            request_id: Uuid::new_v4(),
            report_id: report,
            anchor: recovery::RecoveryAnchor::Original {
                import_id: parent,
                plan_id: anchor.get("id"),
            },
            expected_anchor_revision: anchor.get("revision"),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let run = id(&response["admission_id"]);
    drain(&f).await;
    let mappings = recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        recovery::RecoveryMappingPage {
            cursor: None,
            limit: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(mappings["candidate_count"], count.to_string());
    let mut revision = 0;
    for mapping in mappings["items"].as_array().unwrap() {
        let stage = mapping["kind"] == "stage";
        recovery::edit_mapping(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            run,
            recovery::EditRecoveryMapping {
                request_id: Uuid::new_v4(),
                expected_draft_revision: revision,
                key_id: id(&mapping["id"]),
                disposition: if stage { "existing" } else { "member" }.into(),
                target_id: Some(if stage { f.lead_stage } else { f.actor }),
            },
        )
        .await
        .unwrap();
        revision += 1;
    }
    recovery::seal_choices(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        run,
        recovery::SealRecoveryChoices {
            request_id: Uuid::new_v4(),
            expected_draft_revision: revision,
        },
    )
    .await
    .unwrap();
    drain(&f).await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, run)
        .await
        .unwrap();
    let p = &detail["plan"];
    assert_eq!(p["counts"]["eligible"], count.to_string());
    if !confirm {
        return (f, parent, run);
    }
    recovery::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: id(&p["id"]),
            plan_revision: p["revision"].as_str().unwrap().parse().unwrap(),
            plan_digest: p["digest"].as_str().unwrap().into(),
            eligible_count: count,
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        recovery::RecoveryAcknowledgement {
            mapping_digest: serde_json::from_value(detail["recovery"]["mapping_digest"].clone())
                .unwrap(),
            candidate_count: count,
            contact_count: count,
            unassigned_count: 0,
            acknowledged_creation: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    if execute {
        drain(&f).await;
    }
    (f, parent, run)
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovered_initial_approval_flows_through_core_and_metadata(migrator: PgPool) {
    use crate::db_admitted_people_refresh_execution as support;
    use crm_api::domain::migration::{admitted_metadata, admitted_people_refresh as refresh};
    let (f, parent, admission) = recovered_fixture(&migrator).await;
    f.reader.set_records(Stream::CustomFields,vec![json!({"id":21,"name":"customText","label":"Recovery text","type":"text","isRecurring":false})]);
    let report=support::report(&f,parent,vec![json!({"id":104,"firstName":"Later refreshed name","stage":"Later","assignedUserId":3,"phones":[{"value":"4155550111"}],"tags":["Recovered Client"],"customText":"Retained recovery metadata"})]).await;
    let (run, detail) = support::prepare(&f, admission, report).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1", "{detail}");
    support::confirm(&f, run, &support::confirmation(&detail)).await;
    support::drain(&f, run).await;
    assert_eq!(
        refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"],
        "completed"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND recovery_stage_choice_id IS NOT NULL AND stage_mapping_id IS NULL").bind(run).fetch_one(&f.pool).await.unwrap(),1);
    let value = admitted_metadata::prepare(
        &f.pool,
        &f.key,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        admitted_metadata::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id: report,
        },
    )
    .await
    .unwrap();
    let root = id(&value["import"]["id"]);
    crate::db_admitted_metadata::drain_preparation(&f, root).await;
    let plan = sqlx::query_scalar::<_, Uuid>(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    crate::db_admitted_metadata::approve_all(&f, root, plan).await;
    crate::db_admitted_metadata::finish(&f, root).await;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_tag p JOIN tag t ON t.id=p.tag_id AND t.organization_id=p.organization_id WHERE p.organization_id=$1 AND t.name='Recovered Client'").bind(f.org).fetch_one(&f.pool).await.unwrap(),1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT workspace_mode FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "migration_review"
    );
    assert_follow_on(&f, admission, "core").await;
    assert_follow_on(&f, admission, "metadata").await;
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovered_cohort_accepts_separately_confirmed_history(migrator: PgPool) {
    use crate::{db_admitted_history as support, db_history_capture_support as capture};
    use crm_api::domain::migration::{
        admitted_history as h, history_capture_source::Stream as HistoryStream,
    };
    let (f, parent, admission) = recovered_fixture(&migrator).await;
    let book = capture::HistoryBook::new();
    book.set_records(HistoryStream::Events,vec![json!({"id":81,"personId":104,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"Recovered Person history"})]);
    let (capture_id, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture_id).await;
    capture::drain(&f, &book).await;
    let root = support::ready(&f, admission, capture_id).await;
    let detail = h::get(&f.pool, &f.ctx, root).await.unwrap();
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: id(&detail["latest_plan_id"]),
            expected_revision: detail["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    support::drain(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM fub_event_record_imported WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_follow_on(&f, admission, "history").await;
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovered_cohort_accepts_separate_notes_tasks_approval(migrator: PgPool) {
    use crate::db_admitted_activity as support;
    use crm_api::domain::migration::admitted_activity as a;
    let (f, parent, admission) = recovered_fixture(&migrator).await;
    f.reader.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Source agent","timezone":"America/Los_Angeles"})],
    );
    f.reader.set_records(Stream::Notes,vec![json!({"id":11,"personId":104,"body":"list projection","createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":false})]);
    f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":11,"personId":104,"createdById":3,"subject":"Recovered note","body":"retained detail","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null,"type":"Note"})).unwrap(),false);
    f.reader.set_records(Stream::TasksOpen,vec![json!({"id":21,"personId":104,"name":"Call recovered Person","type":"Call","createdById":3,"assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z","updated":null})]);
    let report = db_people_admission_execution::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Recovered","stage":"Later","assignedUserId":3})],
    )
    .await;
    let value = a::prepare(
        &f.pool,
        &f.key,
        &f.ctx,
        a::PrepareAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id: report,
        },
        &f.policy,
    )
    .await
    .unwrap();
    let root = id(&value["import"]["id"]);
    support::drain(&f).await;
    let detail = a::detail(&f.pool, &f.key, &f.ctx, root, &f.policy)
        .await
        .unwrap();
    let mappings = a::mappings(&f.pool, &f.key, &f.ctx, root, a::ActivityPage::default())
        .await
        .unwrap();
    let choices = mappings["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| a::MappingPatch {
            mapping_id: id(&row["id"]),
            choice: if row["role"] == "task_kind" {
                a::Choice::MapKind {
                    native_kind: "call".into(),
                }
            } else {
                a::Choice::MapExisting { target_id: f.actor }
            },
        })
        .collect();
    a::replan(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        a::PlanAdmittedActivityImport {
            request_id: Uuid::new_v4(),
            expected_plan_id: id(&detail["latest_plan"]["id"]),
            choices,
            source_timezone: Some(Some("America/Los_Angeles".into())),
        },
        &f.policy,
    )
    .await
    .unwrap();
    support::drain(&f).await;
    let detail = a::detail(&f.pool, &f.key, &f.ctx, root, &f.policy)
        .await
        .unwrap();
    support::confirm_ready(&f, root, &detail).await;
    support::drain(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_follow_on(&f, admission, "activity").await;
}

async fn confirm_preview(f: &import_support::Fixture, run: Uuid) {
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, run)
        .await
        .unwrap();
    let p = &detail["plan"];
    let number = |v: &Value| v.as_str().unwrap().parse::<i64>().unwrap();
    recovery::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: id(&p["id"]),
            plan_revision: number(&p["revision"]),
            plan_digest: p["digest"].as_str().unwrap().into(),
            eligible_count: number(&p["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        recovery::RecoveryAcknowledgement {
            mapping_digest: serde_json::from_value(detail["recovery"]["mapping_digest"].clone())
                .unwrap(),
            candidate_count: number(&p["counts"]["recovery_candidates"]),
            contact_count: number(&p["counts"]["intended_contacts"]),
            unassigned_count: number(&p["counts"]["recovery_unassigned"]),
            acknowledged_creation: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovery_late_failure_rolls_back_and_expired_lease_reclaims(migrator: PgPool) {
    let (f, _, run) = recovery_fixture(&migrator, false, 1).await;
    let bytes: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_people_admission WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    sqlx::raw_sql("CREATE FUNCTION recovery_test_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic failure after native and provenance writes'; END $$; CREATE TRIGGER recovery_test_failure BEFORE INSERT ON person_recovered FOR EACH ROW EXECUTE FUNCTION recovery_test_failure();").execute(&migrator).await.unwrap();
    assert!(people_admission_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .is_err());
    for table in [
        "person_recovered",
        "migration_people_admission_result",
        "person_admission_provenance",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {table} WHERE admission_id=$1"
            ))
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_import_identity WHERE admission_id=$1"
        )
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT retained_bytes FROM migration_people_admission WHERE id=$1"
        )
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        bytes
    );
    sqlx::query("DROP TRIGGER recovery_test_failure ON person_recovered")
        .execute(&migrator)
        .await
        .unwrap();
    let revision =
        sqlx::query_scalar("SELECT lifecycle_revision FROM migration_people_admission WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    people_admission::retry(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: revision,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    people_admission_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let epoch: i64 =
        sqlx::query_scalar("SELECT lease_epoch FROM migration_people_admission WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let mut fault = migrator.begin().await.unwrap();
    sqlx::query("SET LOCAL crm.people_recovery_reader='fub-people-recovery-v1'")
        .execute(&mut *fault)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_people_admission SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(run).execute(&mut *fault).await.unwrap();
    fault.commit().await.unwrap();
    drain(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT lease_epoch FROM migration_people_admission WHERE id=$1"
        )
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        epoch + 1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_recovered WHERE admission_id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovery_partial_cancel_remainder_copies_only_unfinished_approvals(migrator: PgPool) {
    let (f, _, run) = recovery_fixture(&migrator, false, 2).await;
    people_admission_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let r = sqlx::query("SELECT * FROM migration_people_admission WHERE id=$1")
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: r.get("lifecycle_revision"),
        },
    )
    .await
    .unwrap();
    let created = recovery::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        recovery::PrepareRecovery {
            request_id: Uuid::new_v4(),
            report_id: r.get("report_id"),
            anchor: recovery::RecoveryAnchor::Admission {
                admission_id: run,
                plan_id: r.get("confirmed_admission_plan_id"),
                remainder: true,
            },
            expected_anchor_revision: r.get::<i64, _>("lifecycle_revision") + 1,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let remainder = id(&created["admission_id"]);
    drain(&f).await;
    let mappings = recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        remainder,
        recovery::RecoveryMappingPage {
            cursor: None,
            limit: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(mappings["candidate_count"], "1");
    assert_eq!(mappings["draft_revision"], "1");
    assert!(mappings["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["choice_id"].is_null()));
    recovery::seal_choices(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        remainder,
        recovery::SealRecoveryChoices {
            request_id: Uuid::new_v4(),
            expected_draft_revision: 1,
        },
    )
    .await
    .unwrap();
    drain(&f).await;
    confirm_preview(&f, remainder).await;
    drain(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person_recovered WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        people_admission::retained_byte_audit(&f.pool, &f.ctx, remainder)
            .await
            .unwrap(),
        sqlx::query_scalar::<_, i64>(
            "SELECT retained_bytes FROM migration_people_admission WHERE id=$1"
        )
        .bind(remainder)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn stale_dispatch_cannot_revive_cancelled_recovery(migrator: PgPool) {
    let (f, _, run) = recovery_fixture(&migrator, false, 1).await;
    let revision =
        sqlx::query_scalar("SELECT lifecycle_revision FROM migration_people_admission WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: revision,
        },
    )
    .await
    .unwrap();
    people_admission_worker::execute_stale_selection_for_test(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
        f.ctx.organization_id,
        run,
    )
    .await
    .unwrap();
    people_admission_worker::prepare_stale_selection_for_test(
        &f.pool,
        &f.key,
        &f.policy,
        f.ctx.organization_id,
        run,
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM migration_people_admission WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "cancelled"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_recovered WHERE admission_id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

async fn repeat_original(f: &import_support::Fixture, parent: Uuid, prior: Uuid) -> Uuid {
    let anchor=sqlx::query("SELECT p.id,p.revision,a.report_id FROM migration_import i JOIN migration_import_plan p ON p.id=i.confirmed_plan_id JOIN migration_people_admission a ON a.id=$2 WHERE i.id=$1").bind(parent).bind(prior).fetch_one(&f.pool).await.unwrap();
    let v = recovery::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        recovery::PrepareRecovery {
            request_id: Uuid::new_v4(),
            report_id: anchor.get("report_id"),
            anchor: recovery::RecoveryAnchor::Original {
                import_id: parent,
                plan_id: anchor.get("id"),
            },
            expected_anchor_revision: anchor.get("revision"),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let root = id(&v["admission_id"]);
    drain(f).await;
    root
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovery_http_scope_signed_cursors_replay_and_stale_revision(migrator: PgPool) {
    use crate::common::{body_json, get_with_cookie, post_json_with_cookie};
    use axum::http::StatusCode;
    let (f, parent, old) = recovered_fixture(&migrator).await;
    let root = repeat_original(&f, parent, old).await;
    let other = import_support::fixture(&migrator, import_support::default_people()).await;
    let url = format!("/api/migrations/fub/people-admissions/{root}/recovery-mappings?limit=1");
    for (cookie, status) in [
        (&f.member_cookie, StatusCode::FORBIDDEN),
        (&other.cookie, StatusCode::NOT_FOUND),
    ] {
        assert_eq!(get_with_cookie(&f.app, &url, cookie).await.status(), status);
    }
    let response = get_with_cookie(&f.app, &url, &f.cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let page = body_json(response).await;
    let cursor = page["next_cursor"].as_str().unwrap();
    assert!(cursor.len() > 36);
    let next = recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        recovery::RecoveryMappingPage {
            cursor: Some(cursor.into()),
            limit: Some(1),
        },
    )
    .await
    .unwrap();
    assert_ne!(page["items"][0]["id"], next["items"][0]["id"]);
    assert!(recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        old,
        recovery::RecoveryMappingPage {
            cursor: Some(cursor.into()),
            limit: Some(1)
        }
    )
    .await
    .is_err());
    {
        use tower::ServiceExt;
        let response = f
            .app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/migrations/fub/people-admissions/{root}/recovery-mappings"
                    ))
                    .header("content-type", "text/plain")
                    .header("cookie", &f.cookie)
                    .header("origin", "https://untrusted.invalid")
                    .body(axum::body::Body::from(json!({"request_id":Uuid::new_v4(),"expected_draft_revision":"0","key_id":page["items"][0]["id"],"disposition":"hold","target_id":null}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Existing CSRF posture: SameSite=Lax cookies plus JSON-only commands;
        // a cross-site simple request cannot reach the mutation layer.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
    }
    let edit = format!("/api/migrations/fub/people-admissions/{root}/recovery-mappings");
    let key = &page["items"][0];
    let disposition = if key["kind"] == "stage" {
        "existing"
    } else {
        "member"
    };
    let foreign = if key["kind"] == "stage" {
        other.lead_stage
    } else {
        other.actor
    };
    let bad = json!({"request_id":Uuid::new_v4(),"expected_draft_revision":"0","key_id":key["id"],"disposition":disposition,"target_id":foreign});
    assert!(!post_json_with_cookie(&f.app, &edit, &f.cookie, bad)
        .await
        .status()
        .is_success());
    let body = json!({"request_id":Uuid::new_v4(),"expected_draft_revision":"0","key_id":key["id"],"disposition":"hold","target_id":null});
    let a = post_json_with_cookie(&f.app, &edit, &f.cookie, body.clone()).await;
    assert!(a.status().is_success());
    let a = body_json(a).await;
    let b = post_json_with_cookie(&f.app, &edit, &f.cookie, body.clone()).await;
    assert!(b.status().is_success());
    assert_eq!(a, body_json(b).await);
    assert!(recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        recovery::RecoveryMappingPage {
            cursor: Some(cursor.into()),
            limit: Some(1)
        }
    )
    .await
    .is_err());
    let mut changed = body;
    changed["disposition"] = json!(disposition);
    changed["target_id"] = json!(if key["kind"] == "stage" {
        f.lead_stage
    } else {
        f.actor
    });
    assert!(!post_json_with_cookie(&f.app, &edit, &f.cookie, changed)
        .await
        .status()
        .is_success());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn healthy_identity_is_existing_and_missing_identity_success_stays_held(migrator: PgPool) {
    let (f, parent, first) = recovered_fixture(&migrator).await;
    for missing in [false, true] {
        if missing {
            let mut tx = migrator.begin().await.unwrap();
            sqlx::query("ALTER TABLE migration_import_identity DISABLE TRIGGER USER")
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("DELETE FROM migration_import_identity WHERE admission_id=$1")
                .bind(first)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("ALTER TABLE migration_import_identity ENABLE TRIGGER USER")
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let root = repeat_original(&f, parent, first).await;
        recovery::seal_choices(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            root,
            recovery::SealRecoveryChoices {
                request_id: Uuid::new_v4(),
                expected_draft_revision: 0,
            },
        )
        .await
        .unwrap();
        drain(&f).await;
        let items = people_admission::items(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            people_admission::Page::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            items["items"][0]["disposition"],
            if missing {
                "held_identity"
            } else {
                "already_admitted"
            }
        );
        let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, root)
            .await
            .unwrap();
        assert_eq!(detail["actions"]["confirm"], false);
        people_admission::cancel(
            &f.pool,
            &f.key,
            &f.ctx,
            root,
            people_admission::LifecyclePeopleAdmission {
                request_id: Uuid::new_v4(),
                expected_lifecycle_revision: detail["lifecycle_revision"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person_recovered WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn all_held_admission_preview_is_retired_and_recovery_choices_create_once(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let long_stage = format!("Later {}", "🦄".repeat(600));
    f.reader.set_records(
        Stream::Stages,
        vec![
            json!({"id":4,"name":"Lead"}),
            json!({"id":5,"name":long_stage}),
        ],
    );
    let held = db_people_admission_execution::ready(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Admission hold","stage":long_stage,"assignedUserId":3})],
    )
    .await;
    let r = sqlx::query("SELECT * FROM migration_people_admission WHERE id=$1")
        .bind(held)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let d = people_admission::detail(&f.pool, &f.key, &f.ctx, held)
        .await
        .unwrap();
    assert_eq!(d["plan"]["counts"]["eligible"], "0");
    let v = recovery::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        recovery::PrepareRecovery {
            request_id: Uuid::new_v4(),
            report_id: r.get("report_id"),
            anchor: recovery::RecoveryAnchor::Admission {
                admission_id: held,
                plan_id: id(&d["plan"]["id"]),
                remainder: false,
            },
            expected_anchor_revision: r.get("lifecycle_revision"),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let root = id(&v["admission_id"]);
    drain(&f).await;
    assert_eq!(
        people_admission::detail(&f.pool, &f.key, &f.ctx, held)
            .await
            .unwrap()["state"],
        "cancelled"
    );
    let page = recovery::mapping_page(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        recovery::RecoveryMappingPage {
            cursor: None,
            limit: None,
        },
    )
    .await
    .unwrap();
    let stage_row = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "stage")
        .unwrap();
    assert_eq!(stage_row["source_key_truncated"], true);
    let fragment = recovery::mapping_field(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        id(&stage_row["id"]),
        people_admission::Page::default(),
    )
    .await
    .unwrap();
    assert_eq!(fragment["fragment"], long_stage);
    for (n, row) in page["items"].as_array().unwrap().iter().enumerate() {
        let stage = row["kind"] == "stage";
        recovery::edit_mapping(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            root,
            recovery::EditRecoveryMapping {
                request_id: Uuid::new_v4(),
                expected_draft_revision: n as i64,
                key_id: id(&row["id"]),
                disposition: if stage { "existing" } else { "unassigned" }.into(),
                target_id: if stage { Some(f.lead_stage) } else { None },
            },
        )
        .await
        .unwrap();
    }
    recovery::seal_choices(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        recovery::SealRecoveryChoices {
            request_id: Uuid::new_v4(),
            expected_draft_revision: page["items"].as_array().unwrap().len() as i64,
        },
    )
    .await
    .unwrap();
    drain(&f).await;
    confirm_preview(&f, root).await;
    drain(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_recovered WHERE admission_id=$1")
            .bind(root)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "explicit isolated 25k recovery query-plan evidence"]
async fn recovery_25k_hot_queries(migrator: PgPool) {
    let output = std::env::var("CRM_RECOVERY_PLAN_OUTPUT").expect("explicit evidence path");
    assert!(std::path::Path::new(&output).is_absolute());
    let (f, _, run) = recovered_fixture(&migrator).await;
    // Inert ciphertext clones measure indexes only. Never decrypt or execute them.
    for table in [
        "migration_people_recovery_candidate",
        "migration_people_recovery_key",
        "migration_people_recovery_choice",
    ] {
        sqlx::query(&format!("ALTER TABLE {table} DISABLE TRIGGER USER"))
            .execute(&migrator)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO migration_people_recovery_candidate SELECT (jsonb_populate_record(NULL::migration_people_recovery_candidate,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8101-'||lpad(g::text,12,'0'))::uuid,'source_id',(1000000+g)::text,'stage_source_hmac',sha256(convert_to(g::text,'UTF8'))))).* FROM (SELECT * FROM migration_people_recovery_candidate WHERE admission_id=$1 LIMIT 1) seed CROSS JOIN generate_series(1,25000) g").bind(run).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_people_recovery_key SELECT (jsonb_populate_record(NULL::migration_people_recovery_key,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8102-'||lpad(g::text,12,'0'))::uuid,'source_key_hmac',sha256(convert_to(g::text,'UTF8'))))).* FROM (SELECT * FROM migration_people_recovery_key WHERE admission_id=$1 AND kind='stage' LIMIT 1) seed CROSS JOIN generate_series(1,25000) g").bind(run).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_people_recovery_choice SELECT (jsonb_populate_record(NULL::migration_people_recovery_choice,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8103-'||lpad(g::text,12,'0'))::uuid,'key_id',('10000000-0000-4000-8102-'||lpad(g::text,12,'0'))::uuid,'source_key_hmac',sha256(convert_to(g::text,'UTF8'))))).* FROM (SELECT * FROM migration_people_recovery_choice WHERE admission_id=$1 AND kind='stage' LIMIT 1) seed CROSS JOIN generate_series(1,25000) g").bind(run).execute(&migrator).await.unwrap();
    for table in [
        "migration_people_recovery_candidate",
        "migration_people_recovery_key",
        "migration_people_recovery_choice",
    ] {
        sqlx::query(&format!("ALTER TABLE {table} ENABLE TRIGGER USER"))
            .execute(&migrator)
            .await
            .unwrap();
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&migrator)
            .await
            .unwrap();
    }
    let source = include_str!("../../crm-app/src/domain/migration/people_recovery/mappings.rs");
    let at = source.find("\"SELECT k.*,c.id choice_id").unwrap();
    let statement = serde_json::Deserializer::from_str(&source[at..].replace('\n', "\\n"))
        .into_iter::<String>()
        .next()
        .unwrap()
        .unwrap();
    let mut evidence = Vec::new();
    for after in [
        None,
        Some(Uuid::parse_str("10000000-0000-4000-8102-000000024950").unwrap()),
    ] {
        let plan: Value = sqlx::query_scalar(&format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {statement}"
        ))
        .bind(run)
        .bind(f.org)
        .bind(after)
        .bind(51i64)
        .fetch_one(&migrator)
        .await
        .unwrap();
        fn work(v: &Value, relation: &str) -> f64 {
            let own = if v["Relation Name"] == relation {
                (v["Actual Rows"].as_f64().unwrap_or(0.)
                    + v["Rows Removed by Filter"].as_f64().unwrap_or(0.))
                    * v["Actual Loops"].as_f64().unwrap_or(1.)
            } else {
                0.
            };
            own + match v {
                Value::Object(m) => m.values().map(|v| work(v, relation)).sum(),
                Value::Array(a) => a.iter().map(|v| work(v, relation)).sum(),
                _ => 0.,
            }
        }
        assert!(work(&plan, "migration_people_recovery_key") < 100.);
        assert!(work(&plan, "migration_people_recovery_choice") < 100.);
        assert!(plan.to_string().contains("recovery_choice_key_version"));
        evidence.push(json!({"statement":statement,"after":after,"plan":plan}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovery_capability_and_exact_inventory_fail_closed(migrator: PgPool) {
    let (f, _, root) = recovered_fixture(&migrator).await;
    let release = ReleaseReadiness::for_tests();
    release
        .require_people_recovery(&mut f.pool.acquire().await.unwrap())
        .await
        .unwrap();
    let mut legacy = f.pool.begin().await.unwrap();
    assert!(sqlx::query("SELECT crm_workspace_shared($1)")
        .bind(f.org)
        .execute(&mut *legacy)
        .await
        .is_err());
    legacy.rollback().await.unwrap();
    for sql in [
        "ALTER TABLE migration_people_recovery_choice DISABLE TRIGGER people_recovery_immutable",
        "DROP INDEX recovery_choice_key_version",
        "GRANT UPDATE ON migration_people_recovery_choice TO crm_app",
    ] {
        let mut tx = migrator.begin().await.unwrap();
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        assert!(!sqlx::query_scalar::<_, bool>(include_str!(
            "../../crm-app/src/auth/people_recovery_schema.sql"
        ))
        .fetch_one(&mut *tx)
        .await
        .unwrap());
        tx.rollback().await.unwrap();
    }
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, root)
        .await
        .unwrap();
    assert!(detail["coverage"]["follow_on"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["status"] == "not_started" && s["can_review"] == true));
}

#[cfg(feature = "perf-harness")]
#[tokio::test]
#[ignore = "explicit long-lived synthetic recovery browser fixture"]
async fn serve_people_recovery_ui_fixture() {
    use crate::common;
    use crm_api::{realtime::Publisher, state::AppState};
    use std::{str::FromStr, sync::Arc};
    assert_eq!(
        std::env::var("CRM_RECOVERY_UI_RUN").as_deref(),
        Ok("approved-synthetic")
    );
    let options = sqlx::postgres::PgConnectOptions::from_str(
        &std::env::var("MIGRATION_DATABASE_URL").unwrap(),
    )
    .unwrap();
    assert_eq!(options.get_database(), Some("crm_010e6_qa"));
    assert!(matches!(options.get_host(), "localhost" | "127.0.0.1"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3107")
        .await
        .unwrap();
    let migrator = PgPool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&migrator).await.unwrap();
    let reader = Arc::new(import_support::Book::new(vec![
        json!({"id":101,"firstName":"Original kept","stage":"Lead","assignedUserId":3}),
        json!({"id":104,"firstName":"Never imported","stage":"Later","assignedUserId":3}),
        json!({"id":105,"firstName":"Second hold","stage":"Later","assignedUserId":3}),
    ]));
    reader.set_records(
        Stream::Stages,
        vec![
            json!({"id":4,"name":"Lead"}),
            json!({"id":5,"name":"Later"}),
        ],
    );
    let f = import_support::fixture_with_book(&migrator, reader).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let report=db_people_admission_execution::report(&f,parent,vec![json!({"id":101,"firstName":"Original kept","stage":"Lead","assignedUserId":3}),json!({"id":104,"firstName":"Recovery desktop","stage":"Later","assignedUserId":3,"emails":[{"value":"recovery@synthetic.test"}]}),json!({"id":105,"firstName":"Recovery narrow","stage":"Later","assignedUserId":3})]).await;
    let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    std::fs::create_dir_all("/private/tmp/crm-010e6-qa").unwrap();
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open("/private/tmp/crm-010e6-qa/fixture.json")
        .unwrap();
    out.write_all(serde_json::to_vec(&json!({"email":email,"password":"synthetic import fixture password","parent":parent,"report":report,"organization":f.org})).unwrap().as_slice()).unwrap();
    let mut config = common::test_config();
    config.cors_allowed_origin = Some("http://127.0.0.1:5187".into());
    let mut state = AppState::for_tests(f.pool.clone(), &config, Publisher::recording());
    state.import_release = Some(Arc::new(ReleaseReadiness::for_tests()));
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            for _ in 0..32 {
                match people_admission_worker::run_once(
                    worker_state.db.as_ref().unwrap(),
                    &worker_state.raw_payload_key,
                    &worker_state.snapshot_policy,
                    worker_state.import_release.as_deref(),
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(e) => {
                        eprintln!("Synthetic recovery worker: {e:?}");
                        break;
                    }
                }
            }
        }
    });
    eprintln!("Synthetic recovery API ready on 3107");
    axum::serve(listener, crm_api::build_app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    worker.abort();
    f.pool.close().await;
    migrator.close().await;
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn recovery_exact_confirmation_and_capacity_fence(migrator: PgPool) {
    use crate::common::post_json_with_cookie;
    let (f, _, root) = recovery_fixture_phase(&migrator, false, 1, false).await;
    let d = people_admission::detail(&f.pool, &f.key, &f.ctx, root)
        .await
        .unwrap();
    let p = &d["plan"];
    let mut b = json!({"request_id":Uuid::new_v4(),"plan_id":p["id"],"plan_revision":p["revision"],"plan_digest":p["digest"],"eligible_count":p["counts"]["eligible"],"acknowledged_coverage":true,"acknowledged_mappings":true,"acknowledged_distinct_contacts":true,"acknowledged_review_hold":true});
    let url = format!("/api/migrations/fub/people-admissions/{root}/confirm");
    assert!(!post_json_with_cookie(&f.app, &url, &f.cookie, b.clone())
        .await
        .status()
        .is_success());
    b["request_id"] = json!(Uuid::new_v4());
    b["recovery"] = json!({"mapping_digest":d["recovery"]["mapping_digest"],"candidate_count":"1","contact_count":"2","unassigned_count":"0","acknowledged_creation":true});
    assert!(!post_json_with_cookie(&f.app, &url, &f.cookie, b)
        .await
        .status()
        .is_success());
    confirm_preview(&f, root).await;
    let tiny = crm_api::domain::migration::snapshot::SnapshotPolicy {
        run_ceiling_bytes: 1,
        org_ceiling_bytes: 1,
    };
    assert!(matches!(
        people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &tiny,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(crm_api::domain::migration::MigrationError::StorageLimit)
    ));
    let d = people_admission::detail(&f.pool, &f.key, &f.ctx, root)
        .await
        .unwrap();
    assert_eq!(d["state"], "paused");
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: d["lifecycle_revision"].as_str().unwrap().parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_recovered WHERE admission_id=$1")
            .bind(root)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

async fn assert_follow_on(f: &import_support::Fixture, root: Uuid, family: &str) {
    let d = people_admission::detail(&f.pool, &f.key, &f.ctx, root)
        .await
        .unwrap();
    let status = d["coverage"]["follow_on"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["family"] == family)
        .unwrap();
    assert!(!status["root_id"].is_null());
    assert_ne!(status["status"], "not_started");
    assert_eq!(status["admission_id"], root.to_string());
    assert_eq!(status["can_review"], true);
}

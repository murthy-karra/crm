//! D-080 execution acceptance. Every refresh in this module is rooted in an
//! actual confirmed 010c import, actual 010e3 admission, and later retained
//! core-change report; fixture SQL is only used to model concurrent native
//! writers and a post-write database fault.
use std::sync::Arc;

use crate::{
    db_people_admission_execution,
    import_support::{self as support, Book, Fixture},
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_people_refresh as refresh, admitted_people_refresh_worker, imports,
        imports::{AssigneeChoice, AssigneePatch, StageChoice, StagePatch}, people_admission,
        people_admission_worker, snapshot_source::Stream, MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().expect("uuid string")).unwrap()
}
fn number(value: &Value) -> i64 {
    value.as_str().expect("numeric string").parse().unwrap()
}

async fn native(f: &Fixture, person: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('person',to_jsonb(p),'contacts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY kind,import_order,id) FROM contact_method c WHERE c.organization_id=p.organization_id AND c.person_id=p.id),'[]'::jsonb)) FROM person p WHERE p.organization_id=$1 AND p.id=$2")
        .bind(f.org).bind(person).fetch_one(&f.pool).await.unwrap()
}

async fn admitted_person(f: &Fixture, admission: Uuid, source: &str) -> Uuid {
    sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE organization_id=$1 AND admission_id=$2 AND source_id=$3 AND disposition='settled'")
        .bind(f.org).bind(admission).bind(source).fetch_one(&f.pool).await.unwrap()
}

async fn seal_admission(f: &Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    let report = db_people_admission_execution::report(f, parent, people).await;
    let prepared = people_admission::prepare(
        &f.pool, &f.key, &f.policy, &f.ctx,
        people_admission::PreparePeopleAdmission { request_id: Uuid::new_v4(), report_id: report },
        Some(&ReleaseReadiness::for_tests()),
    ).await.unwrap();
    let admission = uuid(&prepared["admission_id"]);
    for _ in 0..100 {
        let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, admission).await.unwrap();
        if detail["state"] == "ready" {
            let plan = &detail["plan"];
            people_admission::confirm(&f.pool, &f.key, &f.ctx, admission,
                people_admission::ConfirmPeopleAdmission {
                    request_id: Uuid::new_v4(), plan_id: uuid(&plan["id"]),
                    plan_revision: number(&plan["revision"]),
                    plan_digest: plan["digest"].as_str().unwrap().to_owned(),
                    eligible_count: number(&plan["counts"]["eligible"]),
                    acknowledged_coverage: true, acknowledged_mappings: true,
                    acknowledged_distinct_contacts: true, acknowledged_review_hold: true,
                }, Some(&ReleaseReadiness::for_tests())).await.unwrap();
            break;
        }
        assert!(people_admission_worker::run_once(&f.pool, &f.key, &f.policy,
            Some(&ReleaseReadiness::for_tests())).await.unwrap());
    }
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, admission).await.unwrap()["state"] == "completed" {
            return admission;
        }
        assert!(people_admission_worker::run_once(&f.pool, &f.key, &f.policy,
            Some(&ReleaseReadiness::for_tests())).await.unwrap());
    }
    panic!("admission did not complete")
}

async fn prepare(f: &Fixture, admission: Uuid, report: Uuid) -> (Uuid, Value) {
    let request_id = Uuid::new_v4();
    let created = refresh::prepare(&f.pool, &f.key, &f.policy, &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh { request_id, admission_id: admission, report_id: report },
        Some(&ReleaseReadiness::for_tests())).await.unwrap();
    assert_eq!(refresh::prepare(&f.pool, &f.key, &f.policy, &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh { request_id, admission_id: admission, report_id: report },
        Some(&ReleaseReadiness::for_tests())).await.unwrap(), created, "prepare replay");
    let run = uuid(&created["refresh_id"]);
    for _ in 0..100 {
        let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
        if detail["state"] == "ready" { return (run, detail); }
        let progressed = admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy,
            Some(&ReleaseReadiness::for_tests())).await.unwrap();
        assert!(progressed, "refresh worker lost a preparing run: {}",
            refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap());
    }
    panic!("refresh did not seal")
}

fn confirmation(detail: &Value) -> refresh::ConfirmAdmittedPeopleRefresh {
    let plan = &detail["plan"];
    refresh::ConfirmAdmittedPeopleRefresh {
        request_id: Uuid::new_v4(), plan_id: uuid(&plan["id"]),
        plan_revision: number(&plan["revision"]),
        plan_digest: plan["digest"].as_str().unwrap().to_owned(),
        acknowledged_coverage: true, acknowledged_exclusions: true,
        acknowledged_name_clears: number(&plan["counts"]["name_clears"]),
        acknowledged_assignment_clears: number(&plan["counts"]["assignment_clears"]),
        acknowledged_contact_removals: number(&plan["counts"]["contact_removals"]),
    }
}

async fn confirm(f: &Fixture, run: Uuid, command: &refresh::ConfirmAdmittedPeopleRefresh) -> Value {
    refresh::confirm(&f.pool, &f.key, &f.ctx, run,
        refresh::ConfirmAdmittedPeopleRefresh {
            request_id: command.request_id, plan_id: command.plan_id,
            plan_revision: command.plan_revision, plan_digest: command.plan_digest.clone(),
            acknowledged_coverage: command.acknowledged_coverage,
            acknowledged_exclusions: command.acknowledged_exclusions,
            acknowledged_name_clears: command.acknowledged_name_clears,
            acknowledged_assignment_clears: command.acknowledged_assignment_clears,
            acknowledged_contact_removals: command.acknowledged_contact_removals,
        },
        Some(&ReleaseReadiness::for_tests())).await.unwrap()
}

async fn drain(f: &Fixture, run: Uuid) {
    for _ in 0..100 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "completed" { return; }
        assert!(admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy,
            Some(&ReleaseReadiness::for_tests())).await.unwrap());
    }
    panic!("refresh did not complete")
}

async fn mapped_fixture(migrator: &PgPool) -> (Fixture, Uuid) {
    let book = Arc::new(Book::new(vec![json!({"id":101,"firstName":"Original","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]})]));
    book.set_records(Stream::Stages, vec![json!({"id":4,"name":"Lead"})]);
    book.set_records(Stream::Users, vec![json!({"id":3,"name":"Source admin"})]);
    let f = support::fixture_with_book(migrator, book).await;
    let (parent, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(&f, parent, "1", &[StagePatch { source_key: "4".into(), choice: StageChoice::Existing { stage_id: f.lead_stage } }],
        &[AssigneePatch { source_key: "3".into(), choice: AssigneeChoice::Member { user_id: f.actor } }]).await;
    support::drain_import(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy).await.unwrap();
    imports::confirm(&f.pool, &f.key, &f.ctx, parent, serde_json::from_value(json!({
        "request_id": Uuid::new_v4(), "plan_id":detail["plan"]["id"], "plan_revision":detail["plan"]["revision"],
        "confirmation_digest":detail["plan"]["confirmation_digest"], "acknowledgments":{"held_count":detail["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}
    })).unwrap(), &ReleaseReadiness::for_tests(), &f.policy).await.unwrap();
    support::drain_import(&f).await;
    assert_eq!(imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy).await.unwrap()["state"], "completed");
    (f, parent)
}

pub(super) async fn fixture_with_admission(migrator: &PgPool, people: Vec<Value>) -> (Fixture, Uuid, Uuid) {
    let (f, parent) = mapped_fixture(migrator).await;
    let admission = seal_admission(&f, parent, people).await;
    (f, parent, admission)
}

pub(super) async fn report(f: &Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    db_people_admission_execution::report(f, parent, people).await
}

async fn ledger_bytes(f: &Fixture, run: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)) FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(proposed_nonce)+octet_length(proposed_ciphertext)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)+octet_length(current_nonce)+octet_length(current_ciphertext)+octet_length(instructions_nonce)+octet_length(instructions_ciphertext)) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_admitted_people_refresh_contact WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_people_refresh_receipt WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(projection_nonce)+octet_length(projection_ciphertext)) FROM migration_admitted_people_refresh_baseline WHERE refresh_id=$1),0)::bigint")
        .bind(run).fetch_one(&f.pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn completed_admitted_refresh_replays_preserves_bytes_and_advances_baseline(migrator: PgPool) {
    let admitted = vec![json!({"id":104,"firstName":"Admitted","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"old@synthetic.test"}],"phones":[{"value":"4155550104"}]})];
    let (f, parent, admission) = fixture_with_admission(&migrator, admitted).await;
    let person = admitted_person(&f, admission, "104").await;
    let original = native(&f, person).await;
    let report_one = report(&f, parent, vec![json!({"id":104,"firstName":"Changed","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"new@synthetic.test"}],"phones":[{"value":"4155550104"}]})]).await;
    let (run, detail) = prepare(&f, admission, report_one).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    let command = confirmation(&detail);
    let receipt = confirm(&f, run, &command).await;
    drain(&f, run).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Changed");
    assert_eq!(confirm(&f, run, &command).await, receipt, "confirm replay receipt");
    let stored: i64 = sqlx::query_scalar("SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    assert_eq!(stored, ledger_bytes(&f, run).await, "exact encrypted byte ledger");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_admitted_refresh_provenance WHERE refresh_id=$1 AND person_id=$2")
        .bind(run).bind(person).fetch_one(&f.pool).await.unwrap(), 1);
    let report_two = report(&f, parent, vec![json!({"id":104,"firstName":"Second","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"second@synthetic.test"}],"phones":[{"value":"4155550104"}]})]).await;
    let (second, second_detail) = prepare(&f, admission, report_two).await;
    let item_id = uuid(&refresh::items(&f.pool, &f.key, &f.ctx, second, refresh::Page::default()).await.unwrap()["items"][0]["id"]);
    let baseline = refresh::item(&f.pool, &f.key, &f.ctx, second, item_id).await.unwrap()["baseline"].clone();
    assert_eq!(baseline["first_name"], "Changed");
    confirm(&f, second, &confirmation(&second_detail)).await;
    drain(&f, second).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Second");
    assert_eq!(original["person"]["created_at"], native(&f, person).await["person"]["created_at"]);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn local_change_and_replaced_contact_hold_without_advancing_baseline(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"emails":[{"value":"old@synthetic.test"}]})]).await;
    let person = admitted_person(&f, admission, "104").await;
    let source = vec![json!({"id":104,"firstName":"Source change","stage":"Lead","assignedUserId":3,"emails":[{"value":"source@synthetic.test"}]})];
    let report_one = report(&f, parent, source.clone()).await;
    let (run, detail) = prepare(&f, admission, report_one).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'")
        .bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap();
    confirm(&f, run, &confirmation(&detail)).await;
    sqlx::query("UPDATE person SET first_name='Local writer' WHERE organization_id=$1 AND id=$2").bind(f.org).bind(person).execute(&migrator).await.unwrap();
    let before = native(&f, person).await;
    drain(&f, run).await;
    assert_eq!(native(&f, person).await, before);
    assert_eq!(refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default()).await.unwrap()["results"][0]["disposition"], "held_stale");
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap(), baseline);
    sqlx::query("UPDATE person SET first_name='Admitted' WHERE organization_id=$1 AND id=$2").bind(f.org).bind(person).execute(&migrator).await.unwrap();
    let report_two = report(&f, parent, source).await;
    let (second, second_detail) = prepare(&f, admission, report_two).await;
    confirm(&f, second, &confirmation(&second_detail)).await;
    sqlx::query("UPDATE contact_method SET value='replacement@local.test',normalized_value='replacement@local.test' WHERE organization_id=$1 AND person_id=$2 AND kind='email'").bind(f.org).bind(person).execute(&migrator).await.unwrap();
    let contact_before = native(&f, person).await;
    drain(&f, second).await;
    assert_eq!(native(&f, person).await, contact_before, "replaced owned contact is C != B and must hold");
    assert_eq!(refresh::results(&f.pool, &f.key, &f.ctx, second, refresh::Page::default()).await.unwrap()["results"][0]["disposition"], "held_stale");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn cancelled_and_completed_boundaries_allow_remainder_previews(migrator: PgPool) {
    let people = vec![
        json!({"id":104,"firstName":"One","stage":"Lead","assignedUserId":3}),
        json!({"id":105,"firstName":"Two","stage":"Lead","assignedUserId":3}),
    ];
    let (f, parent, admission) = fixture_with_admission(&migrator, people).await;
    let report_id = report(&f, parent, vec![
        json!({"id":104,"firstName":"One changed","stage":"Lead","assignedUserId":3}),
        json!({"id":105,"firstName":"Two changed","stage":"Lead","assignedUserId":3}),
    ]).await;
    let (first, first_detail) = prepare(&f, admission, report_id).await;
    confirm(&f, first, &confirmation(&first_detail)).await;
    assert!(admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, Some(&ReleaseReadiness::for_tests())).await.unwrap());
    assert!(admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, Some(&ReleaseReadiness::for_tests())).await.unwrap());
    let revision: i64 = sqlx::query_scalar("SELECT lifecycle_revision FROM migration_admitted_people_refresh WHERE id=$1").bind(first).fetch_one(&f.pool).await.unwrap();
    refresh::cancel(&f.pool, &f.key, &f.ctx, first, refresh::LifecycleAdmittedPeopleRefresh { request_id: Uuid::new_v4(), expected_lifecycle_revision: revision }).await.unwrap();
    let settled: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND disposition='settled'").bind(first).fetch_one(&f.pool).await.unwrap();
    assert!(settled <= 1, "bounded worker leaves a cancelled remainder");
    let (remainder, remainder_detail) = prepare(&f, admission, report_id).await;
    assert!(number(&remainder_detail["plan"]["counts"]["eligible"]) >= 1 - settled);
    confirm(&f, remainder, &confirmation(&remainder_detail)).await;
    drain(&f, remainder).await;
    let (after_complete, after_complete_detail) = prepare(&f, admission, report_id).await;
    assert_eq!(after_complete_detail["state"], "ready", "completed exact boundary remains reviewable");
    assert_ne!(after_complete, remainder);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn source_identity_and_permit_misuse_are_rejected(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})]).await;
    let report_id = report(&f, parent, vec![json!({"id":104,"firstName":"Changed","stage":"Lead","assignedUserId":3})]).await;
    let mismatch = refresh::prepare(&f.pool, &f.key, &f.policy, &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh { request_id: Uuid::new_v4(), admission_id: Uuid::new_v4(), report_id }, Some(&ReleaseReadiness::for_tests())).await;
    assert!(matches!(mismatch, Err(MigrationError::NotFound | MigrationError::SourceNotEligible)));
    sqlx::query("ALTER TABLE migration_core_change_report DISABLE TRIGGER migration_core_change_report_immutable").execute(&migrator).await.unwrap();
    sqlx::query("UPDATE migration_core_change_report SET source_account_id=source_account_id+1 WHERE id=$1").bind(report_id).execute(&migrator).await.unwrap();
    let account = refresh::prepare(&f.pool, &f.key, &f.policy, &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh { request_id: Uuid::new_v4(), admission_id: admission, report_id }, Some(&ReleaseReadiness::for_tests())).await;
    assert!(matches!(account, Err(MigrationError::SourceNotEligible)));
    sqlx::query("UPDATE migration_core_change_report SET source_account_id=source_account_id-1 WHERE id=$1").bind(report_id).execute(&migrator).await.unwrap();
    sqlx::query("ALTER TABLE migration_core_change_report ENABLE TRIGGER migration_core_change_report_immutable").execute(&migrator).await.unwrap();
    let (run, detail) = prepare(&f, admission, report_id).await;
    confirm(&f, run, &confirmation(&detail)).await;
    assert!(admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, Some(&ReleaseReadiness::for_tests())).await.unwrap());
    let person = admitted_person(&f, admission, "104").await;
    let lease: Uuid = sqlx::query_scalar("SELECT lease_token FROM migration_admitted_people_refresh WHERE id=$1")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    let item: Uuid = sqlx::query_scalar("SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND disposition='eligible'")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    let unrelated: Uuid = sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE import_id=$1 AND source_id='101' AND disposition='imported'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    for (setting, token) in [("crm.people_refresh_permit", Uuid::new_v4().to_string()), ("crm.admitted_people_refresh_permit", json!({"lease":Uuid::new_v4(),"item":Uuid::new_v4()}).to_string())] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config($1,$2,true)").bind(setting).bind(token).execute(&mut *tx).await.unwrap();
        let denied = sqlx::query("UPDATE person SET first_name='forged' WHERE organization_id=$1 AND id=$2").bind(f.org).bind(person).execute(&mut *tx).await.unwrap_err();
        assert_eq!(denied.as_database_error().and_then(|error| error.code()).as_deref(), Some("P010C"));
        tx.rollback().await.unwrap();
    }
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string()).execute(&mut *tx).await.unwrap();
    let denied = sqlx::query("UPDATE person SET first_name='valid-token-off-target' WHERE organization_id=$1 AND id=$2")
        .bind(f.org).bind(unrelated).execute(&mut *tx).await.unwrap_err();
    assert_eq!(denied.as_database_error().and_then(|error| error.code()).as_deref(), Some("P010C"), "a valid lease/item cannot authorize another Person");
    tx.rollback().await.unwrap();
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn result_fault_rolls_back_native_baseline_and_progress(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550104"}]})]).await;
    let person = admitted_person(&f, admission, "104").await;
    let report_id = report(&f, parent, vec![json!({"id":104,"firstName":"Proposed","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550104"}]})]).await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    confirm(&f, run, &confirmation(&detail)).await;
    assert!(admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, Some(&ReleaseReadiness::for_tests())).await.unwrap());
    let before = native(&f, person).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION qa_010e4_fail_settlement() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'qa_010e4_atomic_settlement_failure'; END $$; CREATE TRIGGER qa_010e4_fail BEFORE INSERT ON migration_admitted_people_refresh_result FOR EACH ROW EXECUTE FUNCTION qa_010e4_fail_settlement();").execute(&migrator).await.unwrap();
    let error = admitted_people_refresh_worker::run_once(&f.pool, &f.key, &f.policy, Some(&ReleaseReadiness::for_tests())).await.unwrap_err();
    assert!(format!("{error:?}").contains("qa_010e4_atomic_settlement_failure"));
    sqlx::query("DROP TRIGGER qa_010e4_fail ON migration_admitted_people_refresh_result").execute(&migrator).await.unwrap();
    assert_eq!(native(&f, person).await, before);
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap(), baseline);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT settled_items FROM migration_admitted_people_refresh WHERE id=$1").bind(run).fetch_one(&f.pool).await.unwrap(), 0);
    drain(&f, run).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Proposed");
}

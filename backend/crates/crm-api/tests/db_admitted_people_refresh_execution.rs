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
    auth::{workspace::ReleaseReadiness, AuthContext},
    domain::migration::{
        admitted_people_refresh as refresh, admitted_people_refresh_worker, imports,
        imports::{AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        people_admission, people_admission_worker,
        snapshot_source::Stream,
        MigrationError,
    },
    domain::{admin::Role, mobile},
    ids::{OrganizationId, UserId},
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
    let admission = uuid(&prepared["admission_id"]);
    for _ in 0..100 {
        let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
            .await
            .unwrap();
        if detail["state"] == "ready" {
            let plan = &detail["plan"];
            people_admission::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                admission,
                people_admission::ConfirmPeopleAdmission {
                    request_id: Uuid::new_v4(),
                    plan_id: uuid(&plan["id"]),
                    plan_revision: number(&plan["revision"]),
                    plan_digest: plan["digest"].as_str().unwrap().to_owned(),
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
            break;
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
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
            .await
            .unwrap()["state"]
            == "completed"
        {
            return admission;
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
    panic!("admission did not complete")
}

async fn ready_admission(f: &Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    let report = db_people_admission_execution::report(f, parent, people).await;
    let prepared = people_admission::prepare(
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
    let admission = uuid(&prepared["admission_id"]);
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
            .await
            .unwrap()["state"]
            == "ready"
        {
            return admission;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap());
    }
    panic!("admission did not seal")
}

async fn confirm_admission(f: &Fixture, admission: Uuid) {
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, admission)
        .await
        .unwrap();
    let plan = &detail["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        admission,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&plan["id"]),
            plan_revision: number(&plan["revision"]),
            plan_digest: plan["digest"].as_str().unwrap().to_owned(),
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
}

async fn prepare(f: &Fixture, admission: Uuid, report: Uuid) -> (Uuid, Value) {
    let request_id = Uuid::new_v4();
    let created = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id,
            admission_id: admission,
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(
        refresh::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            refresh::PrepareAdmittedPeopleRefresh {
                request_id,
                admission_id: admission,
                report_id: report
            },
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap(),
        created,
        "prepare replay"
    );
    let run = uuid(&created["refresh_id"]);
    for _ in 0..100 {
        let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
        if detail["state"] == "ready" {
            return (run, detail);
        }
        let progressed = admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap();
        assert!(
            progressed,
            "refresh worker lost a preparing run: {}",
            refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()
        );
    }
    panic!("refresh did not seal")
}

fn confirmation(detail: &Value) -> refresh::ConfirmAdmittedPeopleRefresh {
    let plan = &detail["plan"];
    refresh::ConfirmAdmittedPeopleRefresh {
        request_id: Uuid::new_v4(),
        plan_id: uuid(&plan["id"]),
        plan_revision: number(&plan["revision"]),
        plan_digest: plan["digest"].as_str().unwrap().to_owned(),
        acknowledged_coverage: true,
        acknowledged_exclusions: true,
        acknowledged_name_clears: number(&plan["counts"]["name_clears"]),
        acknowledged_assignment_clears: number(&plan["counts"]["assignment_clears"]),
        acknowledged_contact_removals: number(&plan["counts"]["contact_removals"]),
    }
}

async fn confirm(f: &Fixture, run: Uuid, command: &refresh::ConfirmAdmittedPeopleRefresh) -> Value {
    refresh::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::ConfirmAdmittedPeopleRefresh {
            request_id: command.request_id,
            plan_id: command.plan_id,
            plan_revision: command.plan_revision,
            plan_digest: command.plan_digest.clone(),
            acknowledged_coverage: command.acknowledged_coverage,
            acknowledged_exclusions: command.acknowledged_exclusions,
            acknowledged_name_clears: command.acknowledged_name_clears,
            acknowledged_assignment_clears: command.acknowledged_assignment_clears,
            acknowledged_contact_removals: command.acknowledged_contact_removals,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap()
}

async fn drain(f: &Fixture, run: Uuid) {
    for _ in 0..100 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "completed" {
            return;
        }
        assert!(admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("refresh did not complete")
}

async fn mapped_fixture(migrator: &PgPool) -> (Fixture, Uuid) {
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Original","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
    ]));
    book.set_records(Stream::Stages, vec![json!({"id":4,"name":"Lead"})]);
    book.set_records(Stream::Users, vec![json!({"id":3,"name":"Source admin"})]);
    let f = support::fixture_with_book(migrator, book).await;
    let (parent, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(
        &f,
        parent,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: StageChoice::Existing {
                stage_id: f.lead_stage,
            },
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Member { user_id: f.member },
        }],
    )
    .await;
    support::drain_import(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool, &f.key, &f.ctx, parent, serde_json::from_value(json!({
        "request_id": Uuid::new_v4(), "plan_id":detail["plan"]["id"], "plan_revision":detail["plan"]["revision"],
        "confirmation_digest":detail["plan"]["confirmation_digest"], "acknowledgments":{"held_count":detail["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}
    })).unwrap(), &ReleaseReadiness::for_tests(), &f.policy).await.unwrap();
    support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    (f, parent)
}

async fn mapped_fixture_with_two_qualified_stages(
    migrator: &PgPool,
    same_target: bool,
) -> (Fixture, Uuid, Uuid) {
    let book = Arc::new(Book::new(vec![json!({
        "id": 101,
        "firstName": "Original",
        "stage": "Lead",
        "assignedUserId": 3,
        "phones": [{"value": "4155550100"}]
    })]));
    book.set_records(
        Stream::Stages,
        vec![
            json!({"id": 4, "name": "Lead"}),
            json!({"id": 5, "name": "Qualified"}),
        ],
    );
    book.set_records(
        Stream::Users,
        vec![json!({"id": 3, "name": "Source admin"})],
    );
    let f = support::fixture_with_book(migrator, book).await;
    let qualified_stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 AND id<>$2 ORDER BY position LIMIT 1",
    )
    .bind(f.org)
    .bind(f.lead_stage)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let (parent, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(
        &f,
        parent,
        "1",
        &[
            StagePatch {
                source_key: "4".into(),
                choice: StageChoice::Existing {
                    stage_id: f.lead_stage,
                },
            },
            StagePatch {
                source_key: "5".into(),
                choice: StageChoice::Existing {
                    stage_id: if same_target {
                        f.lead_stage
                    } else {
                        qualified_stage
                    },
                },
            },
        ],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Member { user_id: f.member },
        }],
    )
    .await;
    support::drain_import(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        parent,
        serde_json::from_value(json!({
            "request_id": Uuid::new_v4(), "plan_id": detail["plan"]["id"],
            "plan_revision": detail["plan"]["revision"],
            "confirmation_digest": detail["plan"]["confirmation_digest"],
            "acknowledgments": {
                "held_count": detail["plan"]["counts"]["held_people"],
                "review_only": true, "remaining_data": true
            }
        }))
        .unwrap(),
        &ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_import_mapping WHERE plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$1) AND organization_id=$2 AND kind='stage' AND qualified"
        )
        .bind(parent)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        2,
        "both frozen source stage targets must be qualified before admission"
    );
    (f, parent, qualified_stage)
}

pub(super) async fn fixture_with_admission(
    migrator: &PgPool,
    people: Vec<Value>,
) -> (Fixture, Uuid, Uuid) {
    let (f, parent) = mapped_fixture(migrator).await;
    let admission = seal_admission(&f, parent, people).await;
    (f, parent, admission)
}

pub(super) async fn report(f: &Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    db_people_admission_execution::report(f, parent, people).await
}

async fn ledger_bytes(f: &Fixture, run: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)) FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(source_key)+COALESCE(octet_length(source_id),0)+COALESCE(octet_length(source_semantic_hmac),0)+octet_length(proposed_nonce)+octet_length(proposed_ciphertext)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)+octet_length(current_nonce)+octet_length(current_ciphertext)+octet_length(instructions_nonce)+octet_length(instructions_ciphertext)) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_admitted_people_refresh_contact WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(source_id)+octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM person_admitted_refresh_provenance WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_people_refresh_receipt WHERE refresh_id=$1),0) + COALESCE((SELECT sum(octet_length(source_id)+octet_length(projection_nonce)+octet_length(projection_ciphertext)) FROM migration_admitted_people_refresh_baseline WHERE refresh_id=$1),0) + COALESCE((SELECT octet_length(preparation_checkpoint_key) FROM migration_admitted_people_refresh WHERE id=$1),0)::bigint")
        .bind(run).fetch_one(&f.pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn completed_admitted_refresh_replays_preserves_bytes_and_advances_baseline(
    migrator: PgPool,
) {
    let admitted = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"old@synthetic.test"}],"phones":[{"value":"4155550104"}]}),
    ];
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
    assert_eq!(
        confirm(&f, run, &command).await,
        receipt,
        "confirm replay receipt"
    );
    let stored: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        ledger_bytes(&f, run).await,
        "exact encrypted byte ledger"
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_admitted_refresh_provenance WHERE refresh_id=$1 AND person_id=$2")
        .bind(run).bind(person).fetch_one(&f.pool).await.unwrap(), 1);
    let report_two = report(&f, parent, vec![json!({"id":104,"firstName":"Second","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"second@synthetic.test"}],"phones":[{"value":"4155550104"}]})]).await;
    // The successor uses settled B and must never decrypt the obsolete
    // original admission envelope.
    sqlx::query("ALTER TABLE migration_people_admission_item DISABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_people_admission_item SET projection_ciphertext='\\x00'::bytea WHERE admission_id=$1 AND organization_id=$2 AND source_id='104'")
        .bind(admission)
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE migration_people_admission_item ENABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    let (second, second_detail) = prepare(&f, admission, report_two).await;
    let item_id = uuid(
        &refresh::items(&f.pool, &f.key, &f.ctx, second, refresh::Page::default())
            .await
            .unwrap()["items"][0]["id"],
    );
    let baseline = refresh::item(&f.pool, &f.key, &f.ctx, second, item_id)
        .await
        .unwrap()["baseline"]
        .clone();
    assert_eq!(baseline["first_name"], "Changed");
    confirm(&f, second, &confirmation(&second_detail)).await;
    drain(&f, second).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Second");
    assert_eq!(
        original["person"]["created_at"],
        native(&f, person).await["person"]["created_at"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT refresh_id FROM migration_admitted_people_refresh_baseline WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'",
        )
        .bind(f.org)
        .bind(admission)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        second,
        "the latest settled result owns the moved baseline"
    );
    for refresh in [run, second] {
        let stored: i64 = sqlx::query_scalar(
            "SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1",
        )
        .bind(refresh)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            ledger_bytes(&f, refresh).await,
            "baseline ownership transfer keeps each physical ledger exact"
        );
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn local_change_and_replaced_contact_hold_without_advancing_baseline(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"emails":[{"value":"old@synthetic.test"}]})]).await;
    let person = admitted_person(&f, admission, "104").await;
    let source = vec![
        json!({"id":104,"firstName":"Source change","stage":"Lead","assignedUserId":3,"emails":[{"value":"source@synthetic.test"}]}),
    ];
    let report_one = report(&f, parent, source.clone()).await;
    let (run, detail) = prepare(&f, admission, report_one).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'")
        .bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap();
    confirm(&f, run, &confirmation(&detail)).await;
    sqlx::query("UPDATE person SET first_name='Local writer' WHERE organization_id=$1 AND id=$2")
        .bind(f.org)
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    let before = native(&f, person).await;
    drain(&f, run).await;
    assert_eq!(native(&f, person).await, before);
    assert_eq!(
        refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap()["results"][0]["disposition"],
        "held_stale"
    );
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap(), baseline);
    sqlx::query("UPDATE person SET first_name='Admitted' WHERE organization_id=$1 AND id=$2")
        .bind(f.org)
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    let report_two = report(&f, parent, source).await;
    let (second, second_detail) = prepare(&f, admission, report_two).await;
    confirm(&f, second, &confirmation(&second_detail)).await;
    sqlx::query("UPDATE contact_method SET value='replacement@local.test',normalized_value='replacement@local.test' WHERE organization_id=$1 AND person_id=$2 AND kind='email'").bind(f.org).bind(person).execute(&migrator).await.unwrap();
    let contact_before = native(&f, person).await;
    drain(&f, second).await;
    assert_eq!(
        native(&f, person).await,
        contact_before,
        "replaced owned contact is C != B and must hold"
    );
    assert_eq!(
        refresh::results(&f.pool, &f.key, &f.ctx, second, refresh::Page::default())
            .await
            .unwrap()["results"][0]["disposition"],
        "held_stale"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn cancelled_and_completed_boundaries_allow_remainder_previews(migrator: PgPool) {
    let people = vec![
        json!({"id":104,"firstName":"One","stage":"Lead","assignedUserId":3}),
        json!({"id":105,"firstName":"Two","stage":"Lead","assignedUserId":3}),
    ];
    let (f, parent, admission) = fixture_with_admission(&migrator, people).await;
    let report_id = report(
        &f,
        parent,
        vec![
            json!({"id":104,"firstName":"One changed","stage":"Lead","assignedUserId":3}),
            json!({"id":105,"firstName":"Two changed","stage":"Lead","assignedUserId":3}),
        ],
    )
    .await;
    let (first, first_detail) = prepare(&f, admission, report_id).await;
    confirm(&f, first, &confirmation(&first_detail)).await;
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let revision: i64 = sqlx::query_scalar(
        "SELECT lifecycle_revision FROM migration_admitted_people_refresh WHERE id=$1",
    )
    .bind(first)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    refresh::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        first,
        refresh::LifecycleAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: revision,
        },
    )
    .await
    .unwrap();
    let settled: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND disposition='settled'").bind(first).fetch_one(&f.pool).await.unwrap();
    assert!(settled <= 1, "bounded worker leaves a cancelled remainder");
    let cancelled_accounting: (i64, i64) = sqlx::query_as(
        "SELECT retained_bytes,reserved_bytes FROM migration_admitted_people_refresh WHERE id=$1",
    )
    .bind(first)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        cancelled_accounting.1, 0,
        "cancellation settles its retained receipt and releases every reservation"
    );
    assert_eq!(
        cancelled_accounting.0,
        ledger_bytes(&f, first).await,
        "cancellation retained bytes include its exact receipt envelope"
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM migration_admitted_people_refresh_reservation WHERE refresh_id=$1").bind(first).fetch_one(&f.pool).await.unwrap(), 0);
    let (remainder, remainder_detail) = prepare(&f, admission, report_id).await;
    assert!(number(&remainder_detail["plan"]["counts"]["eligible"]) >= 1 - settled);
    confirm(&f, remainder, &confirmation(&remainder_detail)).await;
    drain(&f, remainder).await;
    let (after_complete, after_complete_detail) = prepare(&f, admission, report_id).await;
    assert_eq!(
        after_complete_detail["state"], "ready",
        "completed exact boundary remains reviewable"
    );
    assert_ne!(after_complete, remainder);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn mixed_eligible_and_held_cohort_settles_every_closed_outcome(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(
        &migrator,
        vec![
            json!({"id": 104, "firstName": "Eligible", "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 105, "firstName": "Locally changed", "stage": "Lead", "assignedUserId": 3}),
        ],
    )
    .await;
    let held_person = admitted_person(&f, admission, "105").await;
    sqlx::query("UPDATE person SET first_name='local edit before preview' WHERE organization_id=$1 AND id=$2")
        .bind(f.org)
        .bind(held_person)
        .execute(&migrator)
        .await
        .unwrap();
    let held_before = native(&f, held_person).await;
    let report_id = report(
        &f,
        parent,
        vec![
            json!({"id": 104, "firstName": "Eligible refreshed", "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 105, "firstName": "Source wants a different value", "stage": "Lead", "assignedUserId": 3}),
        ],
    )
    .await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    assert_eq!(detail["plan"]["counts"]["held"], "1");
    confirm(&f, run, &confirmation(&detail)).await;
    drain(&f, run).await;
    let outcomes: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(disposition ORDER BY disposition) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(outcomes, json!(["held_local_change", "settled"]));
    assert_eq!(
        native(&f, held_person).await,
        held_before,
        "a closed held outcome never applies a partial native change"
    );
    let stored: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        ledger_bytes(&f, run).await,
        "mixed settlement charges every result and provenance envelope exactly once"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn checkpointed_preparation_charges_source_keys_and_checkpoint_exactly(migrator: PgPool) {
    let admitted: Vec<Value> = (104..=154)
        .map(|id| {
            json!({
                "id": id,
                "firstName": format!("Admitted {id}"),
                "stage": "Lead",
                "assignedUserId": 3,
            })
        })
        .collect();
    let (f, parent, admission) = fixture_with_admission(&migrator, admitted).await;
    let refreshed: Vec<Value> = (104..=154)
        .map(|id| {
            json!({
                "id": id,
                "firstName": format!("Refreshed {id}"),
                "stage": "Lead",
                "assignedUserId": 3,
            })
        })
        .collect();
    let report_id = report(&f, parent, refreshed).await;
    let created = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let run = uuid(&created["refresh_id"]);
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap());
    let checkpoint: String = sqlx::query_scalar(
        "SELECT preparation_checkpoint_key FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2",
    )
    .bind(run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(
        !checkpoint.is_empty(),
        "the first descriptor turn must persist a checkpoint"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2",
        )
        .bind(run)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1,
        "one descriptor, rather than a 50-row projection page, is the transaction unit"
    );
    let stored: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2",
    )
    .bind(run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(stored, ledger_bytes(&f, run).await);
    for _ in 0..60 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "ready" {
            break;
        }
        assert!(admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap());
    }
    assert_eq!(
        refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"],
        "ready"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT preparation_checkpoint_key FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2",
        )
        .bind(run)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        ""
    );
    let stored: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2",
    )
    .bind(run)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(stored, ledger_bytes(&f, run).await);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn oversized_admission_projection_pauses_before_ciphertext_read_without_partial_plan(
    migrator: PgPool,
) {
    let (f, parent, admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Bounded","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let person = admitted_person(&f, admission, "104").await;
    let before = native(&f, person).await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Later","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let created = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let run = uuid(&created["refresh_id"]);
    sqlx::query("ALTER TABLE migration_people_admission_item DISABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE migration_people_admission_item
            SET projection_ciphertext=convert_to(repeat('x',67108865),'UTF8')
          WHERE admission_id=$1 AND organization_id=$2 AND source_id='104'",
    )
    .bind(admission)
    .bind(f.org)
    .execute(&migrator)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE migration_people_admission_item ENABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap());
    let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    assert_eq!(detail["state"], "paused");
    assert_eq!(detail["pause_reason"], "storage_budget_exhausted");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2",
        )
        .bind(run)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(native(&f, person).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn large_live_current_projection_reserves_a_bounded_held_unit(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Small","stage":"Lead","assignedUserId":3,"emails":[{"value":"old@synthetic.test"}]})],
    )
    .await;
    let person = admitted_person(&f, admission, "104").await;
    let before = native(&f, person).await;
    // People source projections are deliberately bounded (16 KiB fields and
    // 1 MiB contacts), but live native C can legitimately be much larger.
    // Keep it below the 64 MiB item ceiling while exceeding the obsolete 8 MiB
    // fixed prepare reservation.
    sqlx::query("UPDATE contact_method SET value=repeat('v',4500000),normalized_value=repeat('n',4500000) WHERE person_id=$1 AND organization_id=$2")
        .bind(person)
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    let large_current = native(&f, person).await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Later","stage":"Lead","assignedUserId":3,"emails":[{"value":"new@synthetic.test"}]})],
    )
    .await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    assert_eq!(detail["state"], "ready");
    let item = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap()["items"][0]
        .clone();
    assert_eq!(item["disposition"], "held_local_change");
    let bound: i64 = sqlx::query_scalar(
        "SELECT item_byte_bound FROM migration_admitted_people_refresh_item WHERE refresh_id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(bound > 8 * 1024 * 1024 && bound <= 64 * 1024 * 1024);
    confirm(&f, run, &confirmation(&detail)).await;
    drain(&f, run).await;
    assert_eq!(native(&f, person).await, large_current);
    assert_eq!(
        refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap()["results"][0]["disposition"],
        "held_local_change"
    );
    assert_ne!(
        large_current, before,
        "fixture C was enlarged before preparation"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn missing_and_untrustworthy_later_observations_have_distinct_closed_outcomes(
    migrator: PgPool,
) {
    async fn disposition(f: &Fixture, run: Uuid) -> String {
        sqlx::query_scalar(
            "SELECT disposition FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND source_id='104'",
        )
        .bind(run)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    }

    let (absent, absent_parent, absent_admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id": 104, "firstName": "Absent candidate", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let absent_report = report(&absent, absent_parent, vec![]).await;
    let (absent_run, _) = prepare(&absent, absent_admission, absent_report).await;
    assert_eq!(disposition(&absent, absent_run).await, "not_seen_again");

    let (truncated, truncated_parent, truncated_admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id": 104, "firstName": "Truncated candidate", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let truncated_report = report(
        &truncated,
        truncated_parent,
        vec![json!({"id": 104, "firstName": "Truncated changed", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let truncated_snapshot: Uuid = sqlx::query_scalar(
        "SELECT newer_snapshot_id FROM migration_core_change_report WHERE id=$1",
    )
    .bind(truncated_report)
    .fetch_one(&truncated.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE migration_snapshot_capture SET truncated=true WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people'")
        .bind(truncated_snapshot)
        .bind(truncated.org)
        .execute(&migrator)
        .await
        .unwrap();
    let (truncated_run, _) = prepare(&truncated, truncated_admission, truncated_report).await;
    assert_eq!(
        disposition(&truncated, truncated_run).await,
        "held_evidence_gap"
    );

    let (ambiguous, ambiguous_parent, ambiguous_admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id": 104, "firstName": "Ambiguous candidate", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let ambiguous_report = report(
        &ambiguous,
        ambiguous_parent,
        vec![
            json!({"id": 104, "firstName": "Observed once", "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 104, "firstName": "Observed twice differently", "stage": "Lead", "assignedUserId": 3}),
        ],
    )
    .await;
    let (ambiguous_run, _) = prepare(&ambiguous, ambiguous_admission, ambiguous_report).await;
    assert_eq!(
        disposition(&ambiguous, ambiguous_run).await,
        "held_evidence_gap",
        "conflicting valid retained observations are ambiguity, never absence"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn cancelled_admission_refreshes_only_its_settled_cohort(migrator: PgPool) {
    let (f, parent) = mapped_fixture(&migrator).await;
    let original: Uuid = sqlx::query_scalar(
        "SELECT person_id FROM migration_import_result WHERE import_id=$1 AND organization_id=$2 AND source_id='101' AND disposition='imported'",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let sibling_admission = seal_admission(
        &f,
        parent,
        vec![json!({"id": 106, "firstName": "Sibling", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let sibling = admitted_person(&f, sibling_admission, "106").await;
    let partial = ready_admission(
        &f,
        parent,
        vec![
            json!({"id": 104, "firstName": "Cohort 104", "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 105, "firstName": "Cohort 105", "stage": "Lead", "assignedUserId": 3}),
        ],
    )
    .await;
    confirm_admission(&f, partial).await;
    assert!(
        people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap(),
        "one actual admission item must settle before cancellation"
    );
    let settled: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT source_id,person_id FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND disposition='settled' ORDER BY source_id",
    )
    .bind(partial)
    .bind(f.org)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(settled.len(), 1, "the admission must be genuinely partial");
    let (settled_source, settled_person) = settled[0].clone();
    let unsettled_source = if settled_source == "104" {
        "105"
    } else {
        "104"
    };
    let revision: i64 = sqlx::query_scalar(
        "SELECT lifecycle_revision FROM migration_people_admission WHERE id=$1 AND organization_id=$2",
    )
    .bind(partial)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        partial,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: revision,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        people_admission::detail(&f.pool, &f.key, &f.ctx, partial)
            .await
            .unwrap()["state"],
        "cancelled"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND source_id=$3 AND disposition='settled'",
        )
        .bind(partial)
        .bind(f.org)
        .bind(unsettled_source)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "an unsettled admission item never becomes cohort identity"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_import_identity WHERE organization_id=$1 AND admission_id=$2 AND family='people' AND source_id=$3",
        )
        .bind(f.org)
        .bind(partial)
        .bind(unsettled_source)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    let original_before = native(&f, original).await;
    let sibling_before = native(&f, sibling).await;
    let changed = format!("{} refreshed", settled_source);
    let report_id = report(
        &f,
        parent,
        vec![
            json!({"id": 101, "firstName": "Original", "stage": "Lead", "assignedUserId": 3, "phones": [{"value": "4155550100"}]}),
            json!({"id": 104, "firstName": if settled_source == "104" { changed.as_str() } else { "Cohort 104" }, "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 105, "firstName": if settled_source == "105" { changed.as_str() } else { "Cohort 105" }, "stage": "Lead", "assignedUserId": 3}),
            json!({"id": 106, "firstName": "Sibling", "stage": "Lead", "assignedUserId": 3}),
        ],
    )
    .await;
    let (run, detail) = prepare(&f, partial, report_id).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND source_id=$3",
        )
        .bind(run)
        .bind(f.org)
        .bind(&settled_source)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND source_id=$3",
        )
        .bind(run)
        .bind(f.org)
        .bind(unsettled_source)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "unsettled admission work cannot appear in the frozen refresh plan"
    );
    confirm(&f, run, &confirmation(&detail)).await;
    drain(&f, run).await;
    assert_eq!(
        native(&f, settled_person).await["person"]["first_name"],
        changed
    );
    assert_eq!(
        native(&f, original).await,
        original_before,
        "original import Person is outside the admitted cohort"
    );
    assert_eq!(
        native(&f, sibling).await,
        sibling_before,
        "a different admission cohort remains untouched"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn qualified_later_stage_transition_is_once_and_failure_rolls_back_revisions_and_fact(
    migrator: PgPool,
) {
    async fn stage_state(f: &Fixture, person: Uuid) -> Value {
        sqlx::query_scalar(
            "SELECT jsonb_build_object('stage_id',stage_id,'stage_revision',stage_revision,'mobile_revision',mobile_revision) FROM person WHERE organization_id=$1 AND id=$2",
        )
        .bind(f.org)
        .bind(person)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    }
    async fn migration_stage_facts(f: &Fixture, person: Uuid) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM stage_changed WHERE organization_id=$1 AND person_id=$2 AND actor_kind='system' AND origin='migration' AND reason='migration_refresh'",
        )
        .bind(f.org)
        .bind(person)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    }

    let (f, parent, qualified_stage) =
        mapped_fixture_with_two_qualified_stages(&migrator, false).await;
    let admission = seal_admission(
        &f,
        parent,
        vec![json!({"id": 104, "firstName": "Stage candidate", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let person = admitted_person(&f, admission, "104").await;
    let before = stage_state(&f, person).await;
    let facts_before = migration_stage_facts(&f, person).await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id": 104, "firstName": "Stage candidate", "stage": "Qualified", "assignedUserId": 3})],
    )
    .await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    confirm(&f, run, &confirmation(&detail)).await;
    drain(&f, run).await;
    let after = stage_state(&f, person).await;
    assert_eq!(after["stage_id"], qualified_stage.to_string());
    assert_eq!(
        after["stage_revision"].as_i64().unwrap(),
        before["stage_revision"].as_i64().unwrap() + 1,
        "one actual stage transition advances the stage-specific revision once"
    );
    assert_eq!(
        migration_stage_facts(&f, person).await,
        facts_before + 1,
        "one actual stage transition appends exactly one migration fact"
    );

    let (failure, failure_parent, _) =
        mapped_fixture_with_two_qualified_stages(&migrator, false).await;
    let failure_admission = seal_admission(
        &failure,
        failure_parent,
        vec![json!({"id": 104, "firstName": "Failure candidate", "stage": "Lead", "assignedUserId": 3})],
    )
    .await;
    let failure_person = admitted_person(&failure, failure_admission, "104").await;
    let failure_before = stage_state(&failure, failure_person).await;
    let failure_facts = migration_stage_facts(&failure, failure_person).await;
    let failure_report = report(
        &failure,
        failure_parent,
        vec![json!({"id": 104, "firstName": "Failure candidate", "stage": "Qualified", "assignedUserId": 3})],
    )
    .await;
    let (failure_run, failure_detail) = prepare(&failure, failure_admission, failure_report).await;
    confirm(&failure, failure_run, &confirmation(&failure_detail)).await;
    assert!(admitted_people_refresh_worker::run_once(
        &failure.pool,
        &failure.key,
        &failure.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap());
    sqlx::raw_sql("CREATE FUNCTION qa_010e4_stage_settlement_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'qa_010e4_stage_settlement_failure'; END $$; CREATE TRIGGER qa_010e4_stage_failure BEFORE INSERT ON migration_admitted_people_refresh_result FOR EACH ROW EXECUTE FUNCTION qa_010e4_stage_settlement_failure();")
        .execute(&migrator)
        .await
        .unwrap();
    let error = admitted_people_refresh_worker::run_once(
        &failure.pool,
        &failure.key,
        &failure.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap_err();
    assert!(format!("{error:?}").contains("qa_010e4_stage_settlement_failure"));
    sqlx::query("DROP TRIGGER qa_010e4_stage_failure ON migration_admitted_people_refresh_result")
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        stage_state(&failure, failure_person).await,
        failure_before,
        "the failed settlement rolls back the stage and both schema-derived revisions"
    );
    assert_eq!(
        migration_stage_facts(&failure, failure_person).await,
        failure_facts,
        "the failed settlement rolls back its migration stage fact"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn missing_execution_mapping_targets_settle_held_stale_without_native_write(
    migrator: PgPool,
) {
    async fn assert_held_stale(f: &Fixture, run: Uuid, person: Uuid, before: Value) {
        drain(f, run).await;
        assert_eq!(native(f, person).await, before);
        assert_eq!(
            refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
                .await
                .unwrap()["results"][0]["disposition"],
            "held_stale"
        );
    }

    let (stage, stage_parent, target_stage) =
        mapped_fixture_with_two_qualified_stages(&migrator, false).await;
    let stage_admission = seal_admission(
        &stage,
        stage_parent,
        vec![json!({"id":104,"firstName":"Stage","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let stage_person = admitted_person(&stage, stage_admission, "104").await;
    let stage_before = native(&stage, stage_person).await;
    let stage_report = report(
        &stage,
        stage_parent,
        vec![json!({"id":104,"firstName":"Later","stage":"Qualified","assignedUserId":3})],
    )
    .await;
    let (stage_run, stage_detail) = prepare(&stage, stage_admission, stage_report).await;
    confirm(&stage, stage_run, &confirmation(&stage_detail)).await;
    sqlx::query("DELETE FROM stage WHERE organization_id=$1 AND id=$2")
        .bind(stage.org)
        .bind(target_stage)
        .execute(&migrator)
        .await
        .unwrap();
    assert_held_stale(&stage, stage_run, stage_person, stage_before).await;

    let (assignee, assignee_parent, _) =
        mapped_fixture_with_two_qualified_stages(&migrator, false).await;
    let assignee_admission = seal_admission(
        &assignee,
        assignee_parent,
        vec![json!({"id":104,"firstName":"Assignee","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let assignee_person = admitted_person(&assignee, assignee_admission, "104").await;
    let assignee_before = native(&assignee, assignee_person).await;
    let assignee_report = report(
        &assignee,
        assignee_parent,
        vec![json!({"id":104,"firstName":"Later","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let (assignee_run, assignee_detail) =
        prepare(&assignee, assignee_admission, assignee_report).await;
    confirm(&assignee, assignee_run, &confirmation(&assignee_detail)).await;
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2")
        .bind(assignee.org)
        .bind(assignee.member)
        .execute(&migrator)
        .await
        .unwrap();
    assert_held_stale(&assignee, assignee_run, assignee_person, assignee_before).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn selected_mapping_cannot_be_replaced_by_a_same_target_sibling(migrator: PgPool) {
    let (f, parent, _) = mapped_fixture_with_two_qualified_stages(&migrator, true).await;
    let admission = seal_admission(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Original","stage":"Qualified","assignedUserId":3})],
    )
    .await;
    let person = admitted_person(&f, admission, "104").await;
    let before = native(&f, person).await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Later","stage":"Qualified","assignedUserId":3})],
    )
    .await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    confirm(&f, run, &confirmation(&detail)).await;
    // Source stage 4 remains a qualified sibling for the same native Lead
    // target.  Only source stage 5 was selected by this item.
    sqlx::query("UPDATE migration_import_mapping SET qualified=false WHERE organization_id=$1 AND plan_id=(SELECT confirmed_plan_id FROM migration_import WHERE id=$2) AND kind='stage' AND source_key='5'")
        .bind(f.org)
        .bind(parent)
        .execute(&migrator)
        .await
        .unwrap();
    drain(&f, run).await;
    assert_eq!(native(&f, person).await, before);
    assert_eq!(
        refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap()["results"][0]["disposition"],
        "held_stale"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn source_identity_and_permit_misuse_are_rejected(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(
        &migrator,
        vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Changed","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let mismatch = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: Uuid::new_v4(),
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await;
    assert!(matches!(
        mismatch,
        Err(MigrationError::NotFound | MigrationError::SourceNotEligible)
    ));
    sqlx::query("ALTER TABLE migration_core_change_report DISABLE TRIGGER migration_core_change_report_immutable").execute(&migrator).await.unwrap();
    sqlx::query(
        "UPDATE migration_core_change_report SET source_account_id=source_account_id+1 WHERE id=$1",
    )
    .bind(report_id)
    .execute(&migrator)
    .await
    .unwrap();
    let account = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await;
    assert!(matches!(account, Err(MigrationError::SourceNotEligible)));
    sqlx::query(
        "UPDATE migration_core_change_report SET source_account_id=source_account_id-1 WHERE id=$1",
    )
    .bind(report_id)
    .execute(&migrator)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE migration_core_change_report ENABLE TRIGGER migration_core_change_report_immutable").execute(&migrator).await.unwrap();
    let (run, detail) = prepare(&f, admission, report_id).await;
    confirm(&f, run, &confirmation(&detail)).await;
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let person = admitted_person(&f, admission, "104").await;
    let lease: Uuid =
        sqlx::query_scalar("SELECT lease_token FROM migration_admitted_people_refresh WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let item: Uuid = sqlx::query_scalar("SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND disposition='eligible'")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    let unrelated: Uuid = sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE import_id=$1 AND source_id='101' AND disposition='imported'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    for (setting, token) in [
        ("crm.people_refresh_permit", Uuid::new_v4().to_string()),
        (
            "crm.admitted_people_refresh_permit",
            json!({"lease":Uuid::new_v4(),"item":Uuid::new_v4()}).to_string(),
        ),
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(token)
            .execute(&mut *tx)
            .await
            .unwrap();
        let denied =
            sqlx::query("UPDATE person SET first_name='forged' WHERE organization_id=$1 AND id=$2")
                .bind(f.org)
                .bind(person)
                .execute(&mut *tx)
                .await
                .unwrap_err();
        assert_eq!(
            denied
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("P010C")
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = sqlx::query(
        "UPDATE person SET first_name='valid-token-off-target' WHERE organization_id=$1 AND id=$2",
    )
    .bind(f.org)
    .bind(unrelated)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C"),
        "a valid lease/item cannot authorize another Person"
    );
    tx.rollback().await.unwrap();

    let target_before = native(&f, person).await;
    let unrelated_before = native(&f, unrelated).await;
    let notes_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM note WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let tasks_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let catalog_before: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let unrelated_facts_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stage_changed WHERE organization_id=$1 AND person_id=$2",
    )
    .bind(f.org)
    .bind(unrelated)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let run_bytes_before: (i64, i64) = sqlx::query_as(
        "SELECT retained_bytes,reserved_bytes FROM migration_admitted_people_refresh WHERE id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let reservations_before: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('token',token,'purpose',purpose,'lease_token',lease_token,'byte_count',byte_count) ORDER BY purpose),'[]'::jsonb) FROM migration_admitted_people_refresh_reservation WHERE refresh_id=$1 AND organization_id=$2")
        .bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let ledger_before = ledger_bytes(&f, run).await;

    // The lease/item pair is real and current: it allows an owned core delta,
    // but every proof attempt is rolled back so the subsequent negative cases
    // preserve the exact frozen execution state.
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE person SET first_name='allowed_then_rolled_back' WHERE organization_id=$1 AND id=$2")
        .bind(f.org).bind(person).execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();

    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = sqlx::query(
        "UPDATE person SET last_contact_at=clock_timestamp() WHERE organization_id=$1 AND id=$2",
    )
    .bind(f.org)
    .bind(person)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C"),
        "a valid permit cannot change an extra Person field"
    );
    tx.rollback().await.unwrap();

    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = sqlx::query("INSERT INTO note(id,organization_id,person_id,author_user_id,body,origin,correlation_id) VALUES($1,$2,$3,$4,'forged note','web_session',$5)")
        .bind(Uuid::new_v4()).bind(f.org).bind(person).bind(f.actor).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C"),
        "a valid permit cannot write a note"
    );
    tx.rollback().await.unwrap();

    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'forged task','follow_up',$4,$4,'web_session',$5)")
        .bind(Uuid::new_v4()).bind(f.org).bind(person).bind(f.actor).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C"),
        "a valid permit cannot write a task"
    );
    tx.rollback().await.unwrap();

    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied =
        sqlx::query("UPDATE stage SET name=name||' forged' WHERE organization_id=$1 AND id=$2")
            .bind(f.org)
            .bind(f.lead_stage)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("42501"),
        "the stage catalog's own revision trigger rejects a permit-backed mutation"
    );
    tx.rollback().await.unwrap();

    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
        .bind(json!({"lease": lease, "item": item}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = sqlx::query("INSERT INTO stage_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,to_stage_id,reason) VALUES($1,$2,'system',$3,'migration',clock_timestamp(),$4,$5,$6,'migration_refresh')")
        .bind(Uuid::new_v4()).bind(f.org).bind(f.actor).bind(Uuid::new_v4()).bind(unrelated).bind(f.lead_stage).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("P010C"),
        "a valid permit cannot append an unrelated fact"
    );
    tx.rollback().await.unwrap();

    let auth = AuthContext {
        actor_user_id: UserId(f.actor),
        actor_email: "migration-admin@synthetic.test".into(),
        actor_display_name: "Migration admin".into(),
        active_organization_id: OrganizationId(f.org),
        active_organization_name: "Synthetic import workspace".into(),
        role: Role::Admin,
    };
    let mobile_error = mobile::bootstrap(
        &f.pool,
        &auth,
        mobile::BootstrapRequest {
            protocol: mobile::PROTOCOL.into(),
            installation_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        mobile_error.code(),
        (403, "workspace_in_migration_review"),
        "ordinary mobile entry is held before it can write"
    );

    assert_eq!(native(&f, person).await, target_before);
    assert_eq!(native(&f, unrelated).await, unrelated_before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        notes_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        tasks_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        catalog_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM stage_changed WHERE organization_id=$1 AND person_id=$2"
        )
        .bind(f.org)
        .bind(unrelated)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        unrelated_facts_before
    );
    assert_eq!(sqlx::query_as::<_, (i64, i64)>("SELECT retained_bytes,reserved_bytes FROM migration_admitted_people_refresh WHERE id=$1").bind(run).fetch_one(&f.pool).await.unwrap(), run_bytes_before);
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT COALESCE(jsonb_agg(jsonb_build_object('token',token,'purpose',purpose,'lease_token',lease_token,'byte_count',byte_count) ORDER BY purpose),'[]'::jsonb) FROM migration_admitted_people_refresh_reservation WHERE refresh_id=$1 AND organization_id=$2").bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap(), reservations_before);
    assert_eq!(ledger_bytes(&f, run).await, ledger_before);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn result_fault_rolls_back_native_baseline_and_progress(migrator: PgPool) {
    let (f, parent, admission) = fixture_with_admission(&migrator, vec![json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550104"}]})]).await;
    let person = admitted_person(&f, admission, "104").await;
    let report_id = report(&f, parent, vec![json!({"id":104,"firstName":"Proposed","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550104"}]})]).await;
    let (run, detail) = prepare(&f, admission, report_id).await;
    confirm(&f, run, &confirmation(&detail)).await;
    assert!(admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let before = native(&f, person).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION qa_010e4_fail_settlement() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'qa_010e4_atomic_settlement_failure'; END $$; CREATE TRIGGER qa_010e4_fail BEFORE INSERT ON migration_admitted_people_refresh_result FOR EACH ROW EXECUTE FUNCTION qa_010e4_fail_settlement();").execute(&migrator).await.unwrap();
    let error = admitted_people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap_err();
    assert!(format!("{error:?}").contains("qa_010e4_atomic_settlement_failure"));
    sqlx::query("DROP TRIGGER qa_010e4_fail ON migration_admitted_people_refresh_result")
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(native(&f, person).await, before);
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(b) FROM migration_admitted_people_refresh_baseline b WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'").bind(f.org).bind(admission).fetch_one(&f.pool).await.unwrap(), baseline);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT settled_items FROM migration_admitted_people_refresh WHERE id=$1"
        )
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    drain(&f, run).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Proposed");
}

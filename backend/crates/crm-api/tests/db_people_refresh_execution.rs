//! Coordinator QA: real retained capture -> 010c import -> 010e1 report ->
//! confirmed 010e2 execution. Migrator writes only inject a local race/failure;
//! no refresh plan, settlement, ownership or provenance row is fabricated.
use std::sync::Arc;

use crate::{
    db_activity_source,
    import_support::{self as support, Book, Fixture},
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, core_change_worker, crypto, imports,
        imports::{AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        people_refresh as refresh, people_refresh_worker,
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

async fn recapture(f: &Fixture, people: Vec<Value>) -> Uuid {
    f.reader.set_records(Stream::People, people);
    let c = sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let proposed = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: c.get("id"),
            expected_revision: c.get("revision"),
        },
    )
    .await
    .unwrap();
    let id = uuid(&proposed["snapshot"]["id"]);
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
    support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    id
}

async fn ready(f: &Fixture, parent: Uuid, people: Vec<Value>) -> (Uuid, Value) {
    let newer = recapture(f, people).await;
    let calls = f.reader.calls();
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
    let report = uuid(&report["report_id"]);
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
        core_change_reports::detail(&f.pool, &f.key, &f.ctx, report)
            .await
            .unwrap()["state"],
        "completed"
    );
    let request_id = Uuid::new_v4();
    let created = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PreparePeopleRefresh {
            request_id,
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
            refresh::PreparePeopleRefresh {
                request_id,
                report_id: report
            },
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap(),
        created
    );
    let id = uuid(&created["refresh_id"]);
    for _ in 0..100 {
        let detail = refresh::detail(&f.pool, &f.key, &f.ctx, id).await.unwrap();
        if detail["state"] == "ready" {
            assert_eq!(
                f.reader.calls(),
                calls,
                "refresh/report must consume retained evidence only"
            );
            return (id, detail);
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
    panic!("bounded synthetic preview failed to become ready");
}

fn confirmation(detail: &Value) -> Value {
    let p = &detail["plan"];
    json!({"request_id":Uuid::new_v4(),"plan_id":p["id"],"plan_revision":number(&p["revision"]),
        "plan_digest":p["digest"],"acknowledged_coverage":true,"acknowledged_exclusions":true,
        "acknowledged_name_clears":number(&p["counts"]["name_clears"]),
        "acknowledged_assignment_clears":number(&p["counts"]["assignment_clears"]),
        "acknowledged_contact_removals":number(&p["counts"]["contact_removals"])})
}

async fn confirm(f: &Fixture, id: Uuid, cmd: &Value) -> Value {
    refresh::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(cmd.clone()).unwrap(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .expect("exact reviewed confirmation must queue")
}

async fn drain(f: &Fixture, id: Uuid) {
    let calls = f.reader.calls();
    for _ in 0..100 {
        let detail = refresh::detail(&f.pool, &f.key, &f.ctx, id).await.unwrap();
        if detail["state"] == "completed" {
            assert_eq!(
                f.reader.calls(),
                calls,
                "executor must never contact source"
            );
            return;
        }
        assert!(
            people_refresh_worker::run_once(
                &f.pool,
                &f.key,
                &f.policy,
                Some(&ReleaseReadiness::for_tests())
            )
            .await
            .unwrap(),
            "unfinished refresh lost work"
        );
    }
    panic!("bounded synthetic refresh failed to settle");
}

async fn person_id(f: &Fixture, parent: Uuid, source: &str) -> Uuid {
    sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE import_id=$1 AND source_id=$2 AND disposition='imported'")
        .bind(parent).bind(source).fetch_one(&f.pool).await.unwrap()
}

async fn native(f: &Fixture, person: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('person',to_jsonb(p),'contacts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY kind,import_order,id) FROM contact_method c WHERE c.organization_id=p.organization_id AND c.person_id=p.id),'[]'::jsonb)) FROM person p WHERE p.organization_id=$1 AND p.id=$2")
        .bind(f.org).bind(person).fetch_one(&f.pool).await.unwrap()
}

async fn original_receipts(f: &Fixture, parent: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('results',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM migration_import_result r WHERE import_id=$1),'[]'::jsonb),'contacts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.result_id,c.kind,c.import_order) FROM migration_import_contact c JOIN migration_import_result r ON r.id=c.result_id WHERE r.import_id=$1),'[]'::jsonb),'workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE w.import_id=$1))")
        .bind(parent).fetch_one(&f.pool).await.unwrap()
}

async fn item(f: &Fixture, run: Uuid, source: &str) -> Value {
    let page = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    let id = uuid(
        &page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["source_id"] == source)
            .unwrap()["id"],
    );
    refresh::item(&f.pool, &f.key, &f.ctx, run, id)
        .await
        .unwrap()
}

async fn proposed_contacts(f: &Fixture, run: Uuid, source: &str) -> Vec<Value> {
    let page = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    let item = uuid(
        &page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["source_id"] == source)
            .unwrap()["id"],
    );
    let mut values =
        refresh::contacts(&f.pool, &f.key, &f.ctx, run, item, refresh::Page::default())
            .await
            .unwrap()["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|contact| contact["side"] == "proposed")
            .map(|contact| {
                let mut value = contact["value"].clone();
                value["id"] = contact["contact_id"].clone();
                value
            })
            .collect::<Vec<_>>();
    values.sort_by_key(|value| {
        (
            value["kind"].as_str().unwrap_or_default().to_owned(),
            value["import_order"].as_i64().unwrap_or_default(),
        )
    });
    values
}

fn rich_people() -> Vec<Value> {
    vec![
        json!({"id":101,"firstName":"Original","lastName":"Family","stage":"Lead","assignedUserId":3,
        "emails":[{"value":"first@synthetic.test"},{"value":"second@synthetic.test"},{"value":"remove@synthetic.test"}],
        "phones":[{"value":"4155550100"}]}),
        json!({"id":102,"firstName":"Clear Me","lastName":"Keep Family","stage":"Lead","assignedUserId":3,
        "emails":[{"value":"clear@synthetic.test"}],"phones":[{"value":"4155550101"}]}),
    ]
}

async fn mapped_fixture(migrator: &PgPool) -> (Fixture, Uuid, Uuid) {
    let book = Arc::new(Book::new(rich_people()));
    book.set_records(
        Stream::Stages,
        vec![
            json!({"id":4,"name":"Lead"}),
            json!({"id":5,"name":"Qualified"}),
        ],
    );
    book.set_records(
        Stream::Users,
        vec![
            json!({"id":3,"name":"Source admin"}),
            json!({"id":9,"name":"Source member"}),
        ],
    );
    let f = support::fixture_with_book(migrator, book).await;
    let other_stage: Uuid = sqlx::query_scalar(
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
                    stage_id: other_stage,
                },
            },
        ],
        &[
            AssigneePatch {
                source_key: "3".into(),
                choice: AssigneeChoice::Member { user_id: f.actor },
            },
            AssigneePatch {
                source_key: "9".into(),
                choice: AssigneeChoice::Member { user_id: f.member },
            },
        ],
    )
    .await;
    support::drain_import(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool, &f.key, &f.ctx, parent, serde_json::from_value(json!({
        "request_id":Uuid::new_v4(),"plan_id":detail["plan"]["id"],"plan_revision":detail["plan"]["revision"],
        "confirmation_digest":detail["plan"]["confirmation_digest"],"acknowledgments":{
            "held_count":detail["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}
    })).unwrap(), &ReleaseReadiness::for_tests(), &f.policy).await.unwrap();
    support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    (f, parent, other_stage)
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn confirmed_refresh_applies_owned_contacts_clears_mappings_facts_and_replays(
    migrator: PgPool,
) {
    let (f, parent, other_stage) = mapped_fixture(&migrator).await;
    let first = person_id(&f, parent, "101").await;
    let second = person_id(&f, parent, "102").await;
    let original_first = native(&f, first).await;
    let original_second = native(&f, second).await;
    let immutable_parent = original_receipts(&f, parent).await;
    let (run, detail) = ready(&f, parent, vec![
        json!({"id":101,"firstName":"Updated","lastName":"Family","stage":"Qualified","assignedUserId":9,
            "emails":[{"value":"second@synthetic.test"},{"value":"first@synthetic.test"},{"value":"added@synthetic.test"}],"phones":[]}),
        json!({"id":102,"firstName":null,"lastName":"Keep Family","stage":"Lead","assignedUserId":null,"assignedPondId":null,
            "emails":[],"phones":[{"value":"4155550101"}]}),
    ]).await;
    assert_eq!(number(&detail["plan"]["counts"]["eligible"]), 2);
    assert_eq!(
        number(&detail["plan"]["counts"]["name_clears"]),
        1,
        "clear must be visible before confirmation"
    );
    assert_eq!(number(&detail["plan"]["counts"]["assignment_clears"]), 1);
    assert_eq!(number(&detail["plan"]["counts"]["contact_removals"]), 3);
    let preview = item(&f, run, "101").await;
    let planned_contacts = proposed_contacts(&f, run, "101").await;
    assert_eq!(preview["proposed"]["stage_id"], other_stage.to_string());
    assert_eq!(
        preview["proposed"]["assigned_user_id"],
        f.member.to_string()
    );
    let cmd = confirmation(&detail);
    let queued = confirm(&f, run, &cmd).await;
    drain(&f, run).await;
    let changed = native(&f, first).await;
    let cleared = native(&f, second).await;
    assert_eq!(changed["person"]["first_name"], "Updated");
    assert_eq!(changed["person"]["stage_id"], other_stage.to_string());
    assert_eq!(changed["person"]["assigned_user_id"], f.member.to_string());
    assert!(cleared["person"]["first_name"].is_null());
    assert_eq!(cleared["person"]["last_name"], "Keep Family");
    assert!(cleared["person"]["assigned_user_id"].is_null());
    assert_eq!(
        changed["person"]["created_at"],
        original_first["person"]["created_at"]
    );
    assert_eq!(
        cleared["person"]["created_at"],
        original_second["person"]["created_at"]
    );
    let contacts = changed["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 3);
    for (order, address) in [
        "second@synthetic.test",
        "first@synthetic.test",
        "added@synthetic.test",
    ]
    .iter()
    .enumerate()
    {
        let contact = contacts
            .iter()
            .find(|v| v["normalized_value"] == *address)
            .unwrap();
        assert_eq!(contact["import_order"], json!(order));
        let original = original_first["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["normalized_value"] == *address);
        if let Some(original) = original {
            assert_eq!(
                contact["id"], original["id"],
                "retained normalized contact must keep identity"
            );
        } else {
            assert!(!original_first["contacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == contact["id"]));
        }
        let planned = planned_contacts
            .iter()
            .find(|v| v["normalized_value"] == *address)
            .unwrap();
        assert_eq!(
            contact["id"], planned["id"],
            "additions must use frozen preview IDs"
        );
    }
    assert_eq!(cleared["contacts"].as_array().unwrap().len(), 1);
    let facts: Value = sqlx::query_scalar("SELECT jsonb_build_object('stages',(SELECT jsonb_agg(to_jsonb(s)) FROM stage_changed s WHERE correlation_id=$1),'assignments',(SELECT jsonb_agg(to_jsonb(a)) FROM assignment_changed a WHERE correlation_id=$1))")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    assert_eq!(facts["stages"].as_array().unwrap().len(), 1);
    assert_eq!(facts["assignments"].as_array().unwrap().len(), 2);
    for fact in facts["stages"]
        .as_array()
        .unwrap()
        .iter()
        .chain(facts["assignments"].as_array().unwrap())
    {
        assert_eq!(fact["actor_kind"], "system");
        assert_eq!(fact["origin"], "migration");
        assert_eq!(fact["on_behalf_of_user_id"], f.actor.to_string());
        assert_eq!(fact["reason"], "migration_refresh");
    }
    assert_eq!(
        original_receipts(&f, parent).await,
        immutable_parent,
        "010c provenance and workspace binding are immutable"
    );
    let results = refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    assert_eq!(results["results"].as_array().unwrap().len(), 2);
    assert!(results["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["disposition"] == "settled"));
    let result = sqlx::query(
        "SELECT * FROM migration_people_refresh_result WHERE refresh_id=$1 AND person_id=$2",
    )
    .bind(run)
    .bind(first)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let after = crypto::open_history(
        &f.key,
        f.ctx.organization_id,
        run,
        result.get("item_id"),
        "people-refresh-result-after",
        &result.get::<Vec<u8>, _>("after_nonce"),
        &result.get::<Vec<u8>, _>("after_ciphertext"),
    )
    .unwrap();
    let mut expected_after = preview["proposed"].clone();
    expected_after
        .as_object_mut()
        .unwrap()
        .remove("contact_counts");
    expected_after["contacts"] = Value::Array(planned_contacts.clone());
    assert_eq!(
        serde_json::from_slice::<Value>(&after).unwrap(),
        expected_after
    );
    assert_eq!(
        confirm(&f, run, &cmd).await,
        queued,
        "lost-response confirm replay returns its original receipt"
    );
    assert!(!people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    assert_eq!(native(&f, first).await, changed);
    assert_eq!(
        refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap(),
        results
    );
    let mode: String = sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(mode, "migration_review");
}

async fn simple(migrator: &PgPool) -> (Fixture, Uuid, Uuid, Uuid, Value) {
    let f = support::fixture(migrator, vec![json!({"id":101,"firstName":"Original","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]})]).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let person = person_id(&f, parent, "101").await;
    let (run, detail) = ready(&f, parent, vec![json!({"id":101,"firstName":"Proposed","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]})]).await;
    (f, parent, person, run, detail)
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn destination_changed_after_preview_is_held_without_advancing_baseline(migrator: PgPool) {
    let (f, parent, person, run, detail) = simple(&migrator).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_people_refresh_baseline b WHERE parent_import_id=$1 AND source_id='101'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    confirm(&f, run, &confirmation(&detail)).await;
    // A privileged fixture mutation represents another authorized destination
    // writer winning after preview. It does not forge refresh settlement.
    sqlx::query("UPDATE person SET first_name='Concurrent local change' WHERE id=$1")
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    let before = native(&f, person).await;
    drain(&f, run).await;
    assert_eq!(native(&f, person).await, before);
    let results = refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    assert_eq!(results["results"][0]["disposition"], "held_stale");
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_people_refresh_baseline b WHERE parent_import_id=$1 AND source_id='101'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    assert_eq!(after, baseline);
    let facts: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM stage_changed WHERE correlation_id=$1)+(SELECT count(*) FROM assignment_changed WHERE correlation_id=$1)")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    assert_eq!(facts, 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn settlement_failure_rolls_back_native_provenance_and_progress_then_retries(
    migrator: PgPool,
) {
    let (f, parent, person, run, detail) = simple(&migrator).await;
    confirm(&f, run, &confirmation(&detail)).await;
    assert!(people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let before = native(&f, person).await;
    let baseline: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_people_refresh_baseline b WHERE parent_import_id=$1 AND source_id='101'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION qa_010e2_fail_settlement() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'qa_010e2_atomic_settlement_failure'; END $$; CREATE TRIGGER qa_010e2_fail BEFORE INSERT ON migration_people_refresh_result FOR EACH ROW EXECUTE FUNCTION qa_010e2_fail_settlement();")
        .execute(&migrator).await.unwrap();
    let error = people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .expect_err("settlement fault must abort the unit");
    assert!(
        format!("{error:?}").contains("qa_010e2_atomic_settlement_failure"),
        "must reach intended post-write fault: {error:?}"
    );
    sqlx::query("DROP TRIGGER qa_010e2_fail ON migration_people_refresh_result")
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        native(&f, person).await,
        before,
        "body/contact/revision changes must all roll back"
    );
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(b) FROM migration_people_refresh_baseline b WHERE parent_import_id=$1 AND source_id='101'")
        .bind(parent).fetch_one(&f.pool).await.unwrap();
    assert_eq!(after, baseline);
    let progress: i64 =
        sqlx::query_scalar("SELECT settled_items FROM migration_people_refresh WHERE id=$1")
            .bind(run)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(progress, 0);
    assert!(
        refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    drain(&f, run).await;
    assert_eq!(native(&f, person).await["person"]["first_name"], "Proposed");
    let results = refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    assert_eq!(results["results"].as_array().unwrap().len(), 1);
    assert_eq!(results["results"][0]["disposition"], "settled");
}

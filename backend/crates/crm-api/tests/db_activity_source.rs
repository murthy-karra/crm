//! TRUST/BOUNDARY: retained activity uses actual synthetic Book captures and a
//! completed 010c parent. Migrator writes only inject explicitly corrupt source
//! evidence/stream states. Ordinary commands never bypass the review hold.
use std::sync::Arc;

use crate::import_support::{self as support, Book, Fixture};
use chrono::{DateTime, Utc};
use crm_api::domain::migration::{
    activity::{self, ActivityPage, Choice, MappingPatch},
    activity_worker, crypto,
    imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
    snapshot_source::Stream,
    MigrationError,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub(super) fn book() -> Arc<Book> {
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Synthetic activity Person",
        "stage":"Lead","assignedUserId":3}),
    ]));
    book.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Captured source author",
        "timezone":"America/Los_Angeles"})],
    );
    book
}

pub(super) fn task(id: u64) -> Value {
    json!({"id":id,"personId":101,"name":"Call the client","type":"Call","createdById":3,
        "assignedUserId":3,"isCompleted":false,"created":"2026-09-01T12:00:00Z","updated":null})
}

fn note() -> Value {
    json!({"id":11,"personId":101,"createdById":3,"subject":"Source subject","body":"main detail",
        "isHtml":false,"created":"2026-09-01T12:00:00.123456Z","updated":null,"type":"Note"})
}

fn note_capture(book: &Book, detail: Value) {
    // Book keys captures by stream/offset; use one note ID per fixture. A list
    // body intentionally differs: only the requested enriched detail executes.
    book.set_records(
        Stream::Notes,
        vec![json!({"id":11,"personId":101,"body":"WRONG list fallback",
        "createdById":999,"created":"1999-01-01T00:00:00Z","isHtml":false})],
    );
    book.set_raw(
        Stream::NoteDetail,
        0,
        200,
        serde_json::to_vec(&detail).unwrap(),
        false,
    );
}

pub(super) async fn completed_parent(f: &Fixture) -> Uuid {
    let calls = f.reader.calls();
    let (id, _) = support::propose(f).await;
    support::drain_import(f).await;
    support::replan(
        f,
        id,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: StageChoice::Existing {
                stage_id: f.lead_stage,
            },
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Member { user_id: f.actor },
        }],
    )
    .await;
    support::drain_import(f).await;
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool,&f.key,&f.ctx,id,serde_json::from_value(json!({
        "request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],"plan_revision":ready["plan"]["revision"],
        "confirmation_digest":ready["plan"]["confirmation_digest"],"acknowledgments":{
            "held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}
    })).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    support::drain_import(f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(f.reader.calls(), calls);
    id
}

pub(super) async fn drain(f: &Fixture) {
    let calls = f.reader.calls();
    for _ in 0..2_000 {
        if !activity_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            assert_eq!(
                f.reader.calls(),
                calls,
                "retained processing must not contact FUB"
            );
            return;
        }
    }
    panic!("synthetic activity exceeded bounded fixture work");
}

pub(super) async fn ready(f: &Fixture, id: Uuid) -> Value {
    activity::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap()
}

pub(super) fn plan_id(value: &Value) -> Uuid {
    Uuid::parse_str(value["latest_plan"]["id"].as_str().unwrap()).unwrap()
}

pub(super) async fn prepare(f: &Fixture, parent: Uuid) -> (Uuid, Value) {
    let value = activity::prepare(
        &f.pool,
        &f.key,
        &f.ctx,
        activity::PrepareActivityImport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
        },
        &f.policy,
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    drain(f).await;
    let value = ready(f, id).await;
    assert_eq!(value["latest_plan"]["state"], "ready");
    (id, value)
}

pub(super) async fn choices(f: &Fixture, id: Uuid) -> Vec<MappingPatch> {
    let page = activity::mappings(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    assert!(
        page["next_cursor"].is_null(),
        "fixture mappings fit one bounded page"
    );
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| MappingPatch {
            mapping_id: Uuid::parse_str(row["id"].as_str().unwrap()).unwrap(),
            choice: match row["role"].as_str().unwrap() {
                "task_kind" => Choice::MapKind {
                    native_kind: match row["source_value"].as_str().unwrap() {
                        "Call" => "call",
                        "Email" => "email",
                        "Text" => "text",
                        "Follow Up" => "follow_up",
                        "Appointment" => "other",
                        _ => panic!("fixture requires an explicit type policy"),
                    }
                    .into(),
                },
                "task_assignee" => Choice::MapExisting {
                    target_id: f.member,
                },
                "task_creator" | "note_author" => Choice::MapExisting { target_id: f.actor },
                _ => panic!("unexpected activity role"),
            },
        })
        .collect()
}

pub(super) async fn replan(
    f: &Fixture,
    id: Uuid,
    previous: &Value,
    choices: Vec<MappingPatch>,
    source_timezone: Option<Option<String>>,
) -> Value {
    activity::replan(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        activity::PlanActivityImport {
            request_id: Uuid::new_v4(),
            expected_plan_id: plan_id(previous),
            choices,
            source_timezone,
        },
        &f.policy,
    )
    .await
    .unwrap();
    drain(f).await;
    let value = ready(f, id).await;
    assert_eq!(value["latest_plan"]["state"], "ready");
    value
}

pub(super) fn confirmation(value: &Value) -> activity::ConfirmActivityImport {
    serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":value["latest_plan"]["id"],
        "expected_revision":value["revision"],"acknowledge_held":value["latest_plan"]["counts"]["held_count"],
        "acknowledge_source_only":value["latest_plan"]["counts"]["source_only_count"]})).unwrap()
}

pub(super) async fn parent_state(f: &Fixture, parent: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('import',to_jsonb(i),'workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE w.organization_id=i.organization_id),'results',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM migration_import_result r WHERE r.import_id=i.id AND r.organization_id=i.organization_id),'identities',(SELECT jsonb_agg(to_jsonb(k) ORDER BY k.family,k.source_id) FROM migration_import_identity k WHERE k.import_id=i.id AND k.organization_id=i.organization_id)) FROM migration_import i WHERE i.id=$1 AND i.organization_id=$2")
        .bind(parent).bind(f.org).fetch_one(&f.pool).await.unwrap()
}

pub(super) async fn confirm(f: &Fixture, id: Uuid, value: &Value) -> Value {
    let parent = Uuid::parse_str(value["parent_import_id"].as_str().unwrap()).unwrap();
    let before = parent_state(f, parent).await;
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        confirmation(value),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    drain(f).await;
    let completed = ready(f, id).await;
    assert_eq!(completed["state"], "completed");
    assert_eq!(
        parent_state(f, parent).await,
        before,
        "activity cannot rewrite its parent or review binding"
    );
    completed
}

async fn records(f: &Fixture, id: Uuid) -> Vec<Value> {
    let page = activity::records(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    assert!(page["next_cursor"].is_null());
    page["items"].as_array().unwrap().clone()
}

fn instant(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap()
        .with_timezone(&Utc)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_source_imports_exact_notes_tasks_roles_and_confirmed_date_policy(
    migrator: PgPool,
) {
    for html in [false, true] {
        let book = book();
        let tail = "界".repeat(6_000);
        let raw_body = if html {
            format!("<p>Hello &amp; <b>world</b></p><p>{tail}</p>")
        } else {
            format!("  Hello <b>literal</b>\r\n{tail}  ")
        };
        let expected = if html {
            format!("Subject: Source subject\n\nHello & world\n{tail}")
        } else {
            format!("Subject: Source subject\n\n  Hello <b>literal</b>\n{tail}")
        };
        let mut source_note = note();
        source_note["body"] = json!(raw_body);
        source_note["isHtml"] = json!(html);
        source_note["replies"] = json!([{"id":12,"body":"retained reply, never flattened"}]);
        source_note["reactions"] = json!([{"reaction":"like","userId":3}]);
        note_capture(&book, source_note);
        let mut dated = task(21);
        dated["dueDate"] = json!("2026-03-08");
        let mut appointment = task(23);
        appointment["type"] = json!("Appointment");
        appointment["description"] = json!("Retained appointment details");
        appointment["createdById"] = Value::Null;
        appointment["createdBy"] = json!("Name-only creator");
        appointment["assignedUserId"] = Value::Null;
        let mut completed = task(22);
        completed["type"] = json!("Email");
        completed["isCompleted"] = json!(1);
        completed["dueDate"] = json!("2026-09-11");
        completed["dueDateTime"] = json!("2026-09-11T14:00:00.123456-07:00");
        completed["updated"] = json!("2026-09-02T12:00:00Z");
        completed["completed"] = json!("2026-09-03T12:00:00.234567Z");
        completed["updatedById"] = json!(999);
        completed["remindSecondsBefore"] = json!(0);
        book.set_records(Stream::TasksOpen, vec![dated, appointment]);
        book.set_records(Stream::TasksCompleted, vec![completed]);
        let f = support::fixture_with_book(&migrator, book).await;
        let calls = f.reader.calls();
        let parent = completed_parent(&f).await;
        let frozen = parent_state(&f, parent).await;
        let (child, first) = prepare(&f, parent).await;
        assert_eq!(first["latest_plan"]["counts"]["notes"]["planned"], "1");
        assert_eq!(first["latest_plan"]["counts"]["tasks"]["planned"], "3");
        let mapped = replan(&f, child, &first, choices(&f, child).await, None).await;
        let without_zone = records(&f, child).await;
        assert_eq!(
            without_zone
                .iter()
                .find(|r| r["source_id"] == "21")
                .unwrap()["disposition"],
            "held"
        );
        let final_plan = replan(
            &f,
            child,
            &mapped,
            vec![],
            Some(Some("America/Los_Angeles".into())),
        )
        .await;
        assert_eq!(final_plan["latest_plan"]["counts"]["held_count"], "0");
        assert_eq!(final_plan["latest_plan"]["tzdb_version"], "2025b");
        let rows = records(&f, child).await;
        let preview = rows.iter().find(|r| r["source_id"] == "11").unwrap();
        assert_eq!(preview["preview"]["native"]["body"], expected);
        assert_eq!(
            preview["preview"]["source_only"]["replies_source_only"],
            "1"
        );
        let field = activity::field(
            &f.pool,
            &f.key,
            &f.ctx,
            child,
            "records",
            Uuid::parse_str(preview["id"].as_str().unwrap()).unwrap(),
            "all",
            activity::FieldQuery {
                limit: Some(65536),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(field["next_cursor"].is_null());
        let full: Value = serde_json::from_str(field["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            full["source"]["body"],
            serde_json::to_string(&raw_body).unwrap()
        );
        assert!(full["source"]["replies"]
            .as_str()
            .unwrap()
            .contains("never flattened"));
        let mut unacknowledged = confirmation(&final_plan);
        unacknowledged.acknowledge_source_only = "0".into();
        assert!(matches!(
            activity::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                child,
                unacknowledged,
                &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
                &f.policy
            )
            .await,
            Err(MigrationError::InvalidImportChoice)
        ));
        let done = confirm(&f, child, &final_plan).await;
        assert_eq!(done["counts"]["notes"]["applied"], "1");
        assert_eq!(done["counts"]["tasks"]["applied"], "3");
        let native = sqlx::query("SELECT * FROM note WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(native.get::<String, _>("body"), expected);
        assert_eq!(
            native.get::<Option<Uuid>, _>("author_user_id"),
            Some(f.actor)
        );
        assert_eq!(native.get::<String, _>("source_external_id"), "v1:17:11");
        assert_eq!(native.get::<String, _>("source"), "fub");
        let person:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND import_id=$2 AND family='people' AND source_id='101'")
            .bind(f.org).bind(parent).fetch_one(&f.pool).await.unwrap();
        assert_eq!(native.get::<Uuid, _>("person_id"), person);
        assert_eq!(
            native.get::<DateTime<Utc>, _>("created_at"),
            instant("2026-09-01T12:00:00.123456Z")
        );
        assert_eq!(
            native.get::<DateTime<Utc>, _>("updated_at"),
            native.get::<DateTime<Utc>, _>("created_at")
        );
        let tasks =
            sqlx::query("SELECT * FROM task WHERE organization_id=$1 ORDER BY source_external_id")
                .bind(f.org)
                .fetch_all(&f.pool)
                .await
                .unwrap();
        assert_eq!(tasks.len(), 3);
        assert!(tasks
            .iter()
            .all(|row| row.get::<String, _>("title") == "Call the client"
                && row.get::<Uuid, _>("person_id") == person));
        assert_eq!(tasks[0].get::<String, _>("kind"), "call");
        assert_eq!(
            tasks[0].get::<Option<DateTime<Utc>>, _>("due_at"),
            Some(instant("2026-03-09T06:59:59Z"))
        );
        assert_eq!(
            tasks[0].get::<Option<Uuid>, _>("created_by_user_id"),
            Some(f.actor)
        );
        assert_eq!(
            tasks[0].get::<Option<Uuid>, _>("assignee_user_id"),
            Some(f.member)
        );
        assert_eq!(tasks[1].get::<String, _>("kind"), "email");
        assert_eq!(
            tasks[1].get::<Option<DateTime<Utc>>, _>("due_at"),
            Some(instant("2026-09-11T21:00:00.123456Z"))
        );
        assert_eq!(
            tasks[1].get::<Option<DateTime<Utc>>, _>("completed_at"),
            Some(instant("2026-09-03T12:00:00.234567Z"))
        );
        assert!(tasks[1]
            .get::<Option<Uuid>, _>("completed_by_user_id")
            .is_none());
        assert_eq!(
            tasks[1].get::<DateTime<Utc>, _>("updated_at"),
            instant("2026-09-02T12:00:00Z")
        );
        assert_eq!(tasks[2].get::<String, _>("kind"), "other");
        assert!(tasks[2].get::<Option<DateTime<Utc>>, _>("due_at").is_none());
        assert!(tasks[2]
            .get::<Option<Uuid>, _>("created_by_user_id")
            .is_none());
        assert!(tasks[2]
            .get::<Option<Uuid>, _>("assignee_user_id")
            .is_none());
        assert_eq!(parent_state(&f, parent).await, frozen);
        assert_eq!(f.reader.calls(), calls);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_source_holds_missing_details_and_all_task_variants_without_list_fallback(
    migrator: PgPool,
) {
    for inaccessible in [false, true] {
        let book = book();
        let mut sparse = note();
        sparse.as_object_mut().unwrap().remove("body");
        note_capture(&book, sparse);
        if inaccessible {
            book.set_raw(
                Stream::NoteDetail,
                0,
                404,
                br#"{"error":"restricted"}"#.to_vec(),
                false,
            )
        }
        let mut first = task(21);
        first["unknownProperty"] = json!({"exact":1});
        let mut different = first.clone();
        different["unknownProperty"] = json!({"exact":2});
        let partition_conflict = task(22);
        book.set_records(
            Stream::TasksOpen,
            vec![first, different, partition_conflict.clone(), task(23)],
        );
        book.set_records(Stream::TasksCompleted, vec![partition_conflict]);
        let f = support::fixture_with_book(&migrator, book).await;
        let parent = completed_parent(&f).await;
        let (child, first) = prepare(&f, parent).await;
        let final_plan = replan(&f, child, &first, choices(&f, child).await, None).await;
        let rows = records(&f, child).await;
        assert_eq!(rows.len(), 4);
        for id in ["11", "21", "22"] {
            assert_eq!(
                rows.iter().find(|r| r["source_id"] == id).unwrap()["disposition"],
                "held"
            )
        }
        for id in ["21", "22"] {
            assert!(
                rows.iter().find(|r| r["source_id"] == id).unwrap()["preview"]["reasons"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source_variants"))
            )
        }
        let original=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND source_id='21'").bind(plan_id(&final_plan)).bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(
            original, 2,
            "unknown-only variants remain separate retained observations"
        );
        let done = confirm(&f, child, &final_plan).await;
        assert_eq!(done["counts"]["notes"]["held"], "1");
        assert_eq!(done["counts"]["tasks"]["held"], "2");
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM note WHERE organization_id=$1),(SELECT count(*) FROM task WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(counts, (0, 1));
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_source_requires_both_exhausted_families_without_a_metadata_child(
    migrator: PgPool,
) {
    let book = book();
    book.set_records(Stream::TasksOpen, vec![task(21)]);
    let f = support::fixture_with_book(&migrator, book).await;
    let parent = completed_parent(&f).await;
    for stream in [
        "people",
        "users",
        "notes",
        "note_detail",
        "tasks_open",
        "tasks_completed",
    ] {
        sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(f.snapshot).bind(f.org).bind(stream).execute(&migrator).await.unwrap();
        assert!(matches!(
            activity::prepare(
                &f.pool,
                &f.key,
                &f.ctx,
                activity::PrepareActivityImport {
                    request_id: Uuid::new_v4(),
                    parent_import_id: parent
                },
                &f.policy
            )
            .await,
            Err(MigrationError::SourceNotEligible)
        ));
        sqlx::query("UPDATE migration_snapshot_stream SET state='completed' WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(f.snapshot).bind(f.org).bind(stream).execute(&migrator).await.unwrap();
    }
    // An unrelated metadata stream is explicitly not an activity prerequisite.
    sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND organization_id=$2 AND stream='custom_fields'").bind(f.snapshot).bind(f.org).execute(&migrator).await.unwrap();
    let metadata: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_metadata_import WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(metadata, 0);
    let (child, first) = prepare(&f, parent).await;
    let final_plan = replan(&f, child, &first, choices(&f, child).await, None).await;
    assert_eq!(
        confirm(&f, child, &final_plan).await["counts"]["tasks"]["applied"],
        "1"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_source_corrupt_evidence_pauses_before_native_writes(migrator: PgPool) {
    for case in [
        "ciphertext",
        "semantic_hmac",
        "request_fingerprint",
        "representation",
        "record_link",
    ] {
        let book = book();
        note_capture(&book, note());
        book.set_records(Stream::TasksOpen, vec![task(21)]);
        let f = support::fixture_with_book(&migrator, book).await;
        let parent = completed_parent(&f).await;
        let frozen = parent_state(&f, parent).await;
        let calls = f.reader.calls();
        let capture=sqlx::query("SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='note_detail' AND accepted").bind(f.snapshot).bind(f.org).fetch_one(&migrator).await.unwrap();
        let id: Uuid = capture.get("id");
        match case {
            "ciphertext" => {
                let raw = serde_json::to_vec(&note()).unwrap();
                let sealed = crypto::seal_snapshot(
                    &f.key,
                    f.ctx.organization_id,
                    f.snapshot,
                    Uuid::new_v4(),
                    "capture",
                    &raw,
                )
                .unwrap();
                sqlx::query(
                    "UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3 WHERE id=$1",
                )
                .bind(id)
                .bind(sealed.nonce.as_slice())
                .bind(sealed.ciphertext)
                .execute(&migrator)
                .await
                .unwrap();
            }
            "semantic_hmac" => {
                sqlx::query("UPDATE migration_snapshot_record SET semantic_hmac=decode(repeat('00',32),'hex') WHERE capture_id=$1").bind(id).execute(&migrator).await.unwrap();
            }
            "request_fingerprint" => {
                sqlx::query("UPDATE migration_snapshot_capture SET request_fingerprint=decode(repeat('00',32),'hex') WHERE id=$1").bind(id).execute(&migrator).await.unwrap();
            }
            "representation" => {
                sqlx::query("UPDATE migration_snapshot_capture SET representation='unqualified' WHERE id=$1").bind(id).execute(&migrator).await.unwrap();
            }
            "record_link" => {
                sqlx::query("UPDATE migration_snapshot_record SET ordinal=99 WHERE capture_id=$1")
                    .bind(id)
                    .execute(&migrator)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let proposed = activity::prepare(
            &f.pool,
            &f.key,
            &f.ctx,
            activity::PrepareActivityImport {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
            },
            &f.policy,
        )
        .await
        .unwrap();
        let child = Uuid::parse_str(proposed["import"]["id"].as_str().unwrap()).unwrap();
        drain(&f).await;
        let paused = ready(&f, child).await;
        assert_eq!(paused["state"], "paused");
        assert!(matches!(
            paused["pause_reason"].as_str(),
            Some("retained_key_unavailable" | "source_integrity")
        ));
        let writes:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM note WHERE organization_id=$1),(SELECT count(*) FROM task WHERE organization_id=$1),(SELECT count(*) FROM migration_activity_identity WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(writes, (0, 0, 0));
        assert_eq!(parent_state(&f, parent).await, frozen);
        assert_eq!(f.reader.calls(), calls);
    }
}

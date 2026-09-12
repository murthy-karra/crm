//! R1 regressions for exact retained request context, source retry recovery and
//! complementary note-observation exclusions. Synthetic retained source only.
use crate::{
    db_activity_source as source,
    import_support::{self as support, Book, Fixture},
};
use crm_api::{
    auth::workspace,
    domain::migration::{
        activity::{self, ActivityPage},
        crypto,
        reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_preview,
        snapshot_source::{Request, Stream},
        snapshot_worker, MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

fn page(book: &Book, stream: Stream, offset: u64, total: u64, items: Vec<Value>) {
    let collection = match stream {
        Stream::Notes => "notes",
        Stream::Users => "users",
        _ => "tasks",
    };
    book.set_raw(stream,offset,200,serde_json::to_vec(&json!({"_metadata":{"collection":collection,"limit":100,"offset":offset,"total":total},collection:items})).unwrap(),false);
}
async fn start_snapshot(f: &mut Fixture) {
    let old=sqlx::query("SELECT connection_id,connection_revision FROM migration_snapshot WHERE id=$1 AND organization_id=$2").bind(f.snapshot).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let proposed = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: old.get("connection_id"),
            expected_revision: old.get("connection_revision"),
        },
    )
    .await
    .unwrap();
    f.snapshot = Uuid::parse_str(proposed["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        f.snapshot,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
}
async fn run_source(f: &Fixture, reader: &dyn FubReader) {
    for _ in 0..200 {
        if !snapshot_worker::run_once(&f.pool, &f.key, reader, &f.policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("bounded source fixture exceeded its work limit");
}
async fn source_state(f: &Fixture) -> Value {
    snapshot::detail(&f.pool, &f.policy, &f.ctx, f.snapshot)
        .await
        .unwrap()["snapshot"]
        .clone()
}
async fn finish_preview(f: &mut Fixture) {
    assert!(matches!(
        source_state(f).await["state"].as_str(),
        Some("completed" | "completed_with_gaps")
    ));
    let p = snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        f.snapshot,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    f.preview = Uuid::parse_str(p["preview_id"].as_str().unwrap()).unwrap();
    support::drain_preview(&f.pool, &f.key, &f.policy).await;
}
async fn retry(f: &Fixture) {
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        f.snapshot,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Retry,
    )
    .await
    .unwrap();
    run_source(f, f.reader.as_ref()).await;
}
async fn assert_activity_rejects_capture(f: &Fixture, parent: Uuid) {
    let calls = f.reader.calls();
    let before = source::parent_state(f, parent).await;
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
    let child = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    source::drain(f).await;
    let state = source::ready(f, child).await;
    assert_eq!(state["state"], "paused");
    assert_eq!(state["pause_reason"], "source_integrity");
    let native:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM task WHERE organization_id=$1)+(SELECT count(*) FROM note WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(native, 0);
    assert_eq!(source::parent_state(f, parent).await, before);
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_r1_completed_capture_rejects_only_open_request_fingerprint_change(
    migrator: PgPool,
) {
    let book = source::book();
    let mut task = source::task(21);
    task["isCompleted"] = json!(true);
    task["completed"] = json!("2026-09-02T12:00:00Z");
    book.set_records(Stream::TasksCompleted, vec![task]);
    let f = support::fixture_with_book(&migrator, book).await;
    let parent = source::completed_parent(&f).await;
    let wrong = crypto::snapshot_hmac(
        &f.key,
        f.ctx.organization_id,
        "request",
        &serde_json::to_vec(
            &json!({"stream":"tasks_open","offset":0,"next":null,"source_id":null}),
        )
        .unwrap(),
    );
    let rows=sqlx::query("UPDATE migration_snapshot_capture SET request_fingerprint=$3 WHERE snapshot_id=$1 AND organization_id=$2 AND stream='tasks_completed' AND accepted").bind(f.snapshot).bind(f.org).bind(wrong.as_slice()).execute(&migrator).await.unwrap().rows_affected();
    assert_eq!(rows, 1);
    assert_activity_rejects_capture(&f, parent).await;
}

struct NextUsers(Arc<Book>);
#[async_trait::async_trait]
impl FubReader for NextUsers {
    async fn identity(&self, key: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        self.0.identity(key).await
    }
    async fn probe(&self, key: &str, p: Probe) -> Result<ProbeResult, ReaderError> {
        self.0.probe(key, p).await
    }
    async fn snapshot(&self, key: &str, request: &Request) -> Result<Capture, ReaderError> {
        if request.stream != Stream::Users {
            return self.0.snapshot(key, request).await;
        }
        match request.cursor.offset {
            0 if request.cursor.next.is_none() => {}
            1 if request.cursor.next.as_deref() == Some("opaque/next?cursor") => {}
            _ => return Err(ReaderError::MalformedResponse),
        }
        // Book stores synthetic responses by returned-item position. Only this
        // fixture transport adapter uses that lookup; snapshot_worker receives
        // the real token-bearing Request and retains its original fingerprint.
        let mut lookup = request.clone();
        lookup.cursor.next = None;
        self.0.snapshot(key, &lookup).await
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_r1_users_next_request_reconstructs_exact_local_position_and_token(
    migrator: PgPool,
) {
    for corrupt in [false, true] {
        let book = source::book();
        book.set_records(Stream::TasksOpen, vec![source::task(21)]);
        let mut f = support::fixture_with_book(&migrator, book).await;
        f.reader.set_raw(Stream::Users,0,200,serde_json::to_vec(&json!({"_metadata":{"collection":"users","limit":100,"offset":0,"total":2,"next":"opaque/next?cursor"},"users":[{"id":3,"timezone":"America/Los_Angeles"}]})).unwrap(),false);
        // Token mode permits a server offset differing from our item position.
        f.reader.set_raw(Stream::Users,1,200,serde_json::to_vec(&json!({"_metadata":{"collection":"users","limit":100,"offset":777,"total":2,"next":null},"users":[{"id":4}]})).unwrap(),false);
        start_snapshot(&mut f).await;
        run_source(&f, &NextUsers(f.reader.clone())).await;
        finish_preview(&mut f).await;
        let parent = source::completed_parent(&f).await;
        if corrupt {
            let wrong = crypto::snapshot_hmac(
                &f.key,
                f.ctx.organization_id,
                "request",
                &serde_json::to_vec(
                    &json!({"stream":"users","offset":1,"next":"different-token","source_id":null}),
                )
                .unwrap(),
            );
            assert_eq!(sqlx::query("UPDATE migration_snapshot_capture SET request_fingerprint=$3 WHERE snapshot_id=$1 AND organization_id=$2 AND stream='users' AND checkpoint=1 AND accepted").bind(f.snapshot).bind(f.org).bind(wrong.as_slice()).execute(&migrator).await.unwrap().rows_affected(),1);
            assert_activity_rejects_capture(&f, parent).await;
        } else {
            let calls = f.reader.calls();
            let (child, ready) = source::prepare(&f, parent).await;
            let ready =
                source::replan(&f, child, &ready, source::choices(&f, child).await, None).await;
            source::confirm(&f, child, &ready).await;
            assert_eq!(
                source::ready(&f, child).await["counts"]["tasks"]["applied"],
                "1"
            );
            assert_eq!(f.reader.calls(), calls);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_r1_real_source_retry_preserves_invalid_and_rejected_variants(migrator: PgPool) {
    for invalid in [true, false] {
        let mut f = support::fixture_with_book(&migrator, source::book()).await;
        if invalid {
            let mut bad = source::task(0);
            bad["id"] = json!(0);
            let mut variant = source::task(2);
            variant["name"] = json!("Rejected earlier title");
            f.reader.set_records(
                Stream::TasksOpen,
                vec![source::task(1), bad, variant, source::task(3)],
            );
        } else {
            page(&f.reader, Stream::TasksOpen, 0, 2, vec![source::task(1)]);
            page(&f.reader, Stream::TasksOpen, 1, 2, vec![source::task(1)]);
        }
        start_snapshot(&mut f).await;
        run_source(&f, f.reader.as_ref()).await;
        assert_eq!(source_state(&f).await["state"], "paused");
        assert_eq!(
            source_state(&f).await["pause_reason"],
            if invalid {
                "invalid_source_id"
            } else {
                "pagination_no_progress"
            }
        );
        let rejected:i64=sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_record r JOIN migration_snapshot_capture c ON c.id=r.capture_id AND c.snapshot_id=r.snapshot_id AND c.organization_id=r.organization_id WHERE c.snapshot_id=$1 AND c.organization_id=$2 AND NOT c.accepted").bind(f.snapshot).bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert_eq!(rejected, if invalid { 4 } else { 1 });
        if invalid {
            f.reader.set_records(
                Stream::TasksOpen,
                vec![source::task(1), source::task(2), source::task(4)],
            );
        } else {
            page(&f.reader, Stream::TasksOpen, 1, 2, vec![source::task(2)]);
        }
        retry(&f).await;
        finish_preview(&mut f).await;
        let calls = f.reader.calls();
        let parent = source::completed_parent(&f).await;
        let frozen = source::parent_state(&f, parent).await;
        let (child, ready) = source::prepare(&f, parent).await;
        let ready = source::replan(&f, child, &ready, source::choices(&f, child).await, None).await;
        assert_eq!(
            ready["latest_plan"]["counts"]["tasks"]["planned"],
            if invalid { "5" } else { "2" }
        );
        assert_eq!(
            ready["latest_plan"]["counts"]["invalid_occurrences"],
            if invalid { "1" } else { "0" }
        );
        assert_eq!(
            ready["latest_plan"]["counts"]["held_count"],
            if invalid { "3" } else { "0" }
        );
        let records = activity::records(&f.pool, &f.key, &f.ctx, child, ActivityPage::default())
            .await
            .unwrap();
        if invalid {
            let items = records["items"].as_array().unwrap();
            assert!(
                items.iter().find(|r| r["source_id"] == "2").unwrap()["preview"]["reasons"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source_variants"))
            );
            assert!(
                items.iter().find(|r| r["source_id"] == "3").unwrap()["preview"]["reasons"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("capture_not_accepted"))
            );
        }
        source::confirm(&f, child, &ready).await;
        let tasks:Vec<String>=sqlx::query_scalar("SELECT source_external_id FROM task WHERE organization_id=$1 ORDER BY source_external_id").bind(f.org).fetch_all(&f.pool).await.unwrap();
        assert_eq!(
            tasks,
            if invalid {
                vec!["v1:17:1", "v1:17:4"]
            } else {
                vec!["v1:17:1", "v1:17:2"]
            }
        );
        assert_eq!(source::parent_state(&f, parent).await, frozen);
        assert_eq!(f.reader.calls(), calls);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_r1_complementary_note_exclusions_count_without_native_list_fallback(
    migrator: PgPool,
) {
    for body_present in [true, false] {
        let mut f = support::fixture_with_book(&migrator, source::book()).await;
        let list = json!({"id":11,"personId":101,"createdById":999,"created":"1999-01-01T00:00:00Z","isHtml":false,"body":"Wrong list fallback","attachments":[{"id":1},{"id":2}],"unknownListOnly":{"exact":"retained"}});
        page(&f.reader, Stream::Notes, 0, 2, vec![list.clone()]);
        page(&f.reader, Stream::Notes, 1, 2, vec![list]);
        let mut detail = json!({"id":11,"personId":101,"createdById":3,"created":"2026-09-01T12:00:00Z","isHtml":true,"body":"<p>Qualified detail<!-- retained comment --></p>","replies":[{"id":12,"body":"retained reply"}]});
        if !body_present {
            detail.as_object_mut().unwrap().remove("body");
        }
        f.reader.set_raw(
            Stream::NoteDetail,
            0,
            200,
            serde_json::to_vec(&detail).unwrap(),
            false,
        );
        start_snapshot(&mut f).await;
        run_source(&f, f.reader.as_ref()).await;
        assert_eq!(
            source_state(&f).await["pause_reason"],
            "pagination_no_progress"
        );
        // Equal rejected duplicate remains visible; corrected terminal empty
        // page completes enumeration without inventing another note identity.
        page(&f.reader, Stream::Notes, 1, 1, vec![]);
        retry(&f).await;
        finish_preview(&mut f).await;
        let calls = f.reader.calls();
        let parent = source::completed_parent(&f).await;
        let (child, ready) = source::prepare(&f, parent).await;
        let ready = source::replan(&f, child, &ready, source::choices(&f, child).await, None).await;
        let expected = if body_present { "5" } else { "4" };
        assert_eq!(
            ready["latest_plan"]["counts"]["source_only_count"],
            expected
        );
        let records = activity::records(&f.pool, &f.key, &f.ctx, child, ActivityPage::default())
            .await
            .unwrap();
        let preview = &records["items"][0]["preview"];
        assert_eq!(preview["source_observations"], "3");
        assert_eq!(preview["source_only"]["attachments_source_only"], "2");
        assert_eq!(
            preview["source_only"]["unknown_properties_source_only"],
            "1"
        );
        assert_eq!(preview["source_only"]["replies_source_only"], "1");
        if body_present {
            assert_eq!(preview["source_only"]["html_comments_source_only"], "1");
            assert_eq!(preview["native"]["body"], "Qualified detail");
            let mut stale = source::confirmation(&ready);
            stale.acknowledge_source_only = "2".into();
            assert!(matches!(
                activity::confirm(
                    &f.pool,
                    &f.key,
                    &f.ctx,
                    child,
                    stale,
                    &workspace::ReleaseReadiness::for_tests(),
                    &f.policy
                )
                .await,
                Err(MigrationError::InvalidImportChoice)
            ));
            source::confirm(&f, child, &ready).await;
            let note = sqlx::query(
                "SELECT body,author_user_id,created_at::text FROM note WHERE organization_id=$1",
            )
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
            assert_eq!(note.get::<String, _>("body"), "Qualified detail");
            assert_eq!(note.get::<Uuid, _>("author_user_id"), f.actor);
        } else {
            assert!(preview["native"].is_null());
            assert_eq!(records["items"][0]["disposition"], "held");
            assert!(matches!(
                activity::confirm(
                    &f.pool,
                    &f.key,
                    &f.ctx,
                    child,
                    source::confirmation(&ready),
                    &workspace::ReleaseReadiness::for_tests(),
                    &f.policy
                )
                .await,
                Err(MigrationError::ImportConflict)
            ));
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE organization_id=$1")
                    .bind(f.org)
                    .fetch_one(&f.pool)
                    .await
                    .unwrap(),
                0
            );
        }
        assert_eq!(f.reader.calls(), calls);
    }
}

//! Isolated, synthetic production-Web and final query-plan acceptance fixtures.
use crate::{
    db_activity_source as activity, db_history_capture_support as capture,
    db_history_import_support as history, db_metadata_import_source as metadata,
    db_people_admission_execution as admission,
    import_support::{self, Book, Fixture},
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        family_refresh::{execution, preparation_worker},
        history_capture_source::Stream as HistoryStream,
        snapshot_source::Stream,
    },
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub(super) async fn fixture(pool: &PgPool) -> (Fixture, Uuid, Uuid, Uuid) {
    let mut person = json!({"id":101,"firstName":"Family","lastName":"Review","stage":"Lead","assignedUserId":3,"tags":["Original"],"customText":"Before refresh"});
    let reader = std::sync::Arc::new(Book::new(vec![person.clone()]));
    reader.set_records(
        Stream::CustomFields,
        vec![json!({"id":10,"name":"customText","label":"Text","type":"text","isRecurring":false})],
    );
    reader.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Synthetic author","timezone":"America/Los_Angeles"})],
    );
    activity::note_capture(&reader, activity::note());
    reader.set_records(Stream::TasksOpen, vec![activity::task(21)]);
    let f = import_support::fixture_with_book(pool, reader).await;
    let parent = activity::completed_parent(&f).await;
    let (root, ready) = metadata::propose(&f, parent).await;
    let ready = metadata::replan(
        &f,
        root,
        &ready,
        metadata::matching_choices(&f, metadata::plan_id(&ready)).await,
    )
    .await;
    metadata::execute(&f, root, &ready, parent).await;
    let (root, ready) = activity::prepare(&f, parent).await;
    let choices = activity::choices(&f, root).await;
    let ready = activity::replan(&f, root, &ready, choices, None).await;
    activity::confirm(&f, root, &ready).await;
    let book = capture::HistoryBook::new();
    book.set_records(HistoryStream::Events,vec![json!({"id":1,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"Original retained history"})]);
    book.set_records(HistoryStream::Calls, vec![json!({"id":10,"personId":101,"userId":3,"created":"2026-01-02T00:00:00Z","duration":1.5,"outcome":"Original imported call"})]);
    book.set_records(HistoryStream::TextMessages, vec![json!({"id":20,"personId":101,"userId":3,"created":"2026-01-03T00:00:00Z","message":"Original imported text","status":"Sent"})]);
    let (first, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, first).await;
    capture::drain(&f, &book).await;
    let root = history::ready(&f, parent, first).await;
    history::confirm(&f, root).await;
    history::drain(&f).await;
    // Migrator-only fixture simulates a local edit made before review hold;
    // ordinary product writers remain denied in the imported workspace.
    sqlx::query("UPDATE task SET title='Local edit preserved' WHERE organization_id=$1")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
    person["tags"] = json!(["New qualified tag"]);
    person["customText"] = json!("After refresh");
    let mut task = activity::task(21);
    task["name"] = json!("Updated retained task");
    f.reader
        .set_records(Stream::TasksOpen, vec![task, activity::task(22)]);
    let mut note = activity::note();
    note["body"] = json!("Updated retained note");
    activity::note_capture(&f.reader, note);
    let report = admission::report(&f, parent, vec![person]).await;
    book.set_records(HistoryStream::Events,vec![json!({"id":1,"personId":101,"type":"Inquiry corrected","created":"2026-01-01T00:00:00Z","description":"Corrected retained history"}),json!({"id":2,"personId":101,"type":"New inquiry","created":"2026-01-02T00:00:00Z","description":"Additional retained history"})]);
    book.set_records(HistoryStream::Calls, vec![json!({"id":10,"personId":101,"userId":3,"created":"2026-01-02T00:00:00Z","duration":2.5,"outcome":"Corrected imported call"})]);
    book.set_records(HistoryStream::TextMessages, vec![json!({"id":20,"personId":101,"userId":3,"created":"2026-01-03T00:00:00Z","message":"Corrected imported text","status":"Delivered"})]);
    let (selected, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, selected).await;
    capture::drain(&f, &book).await;
    (f, parent, report, selected)
}

/// Create OUTPUT/stop to stop. OUTPUT/execute contains a nonnegative maximum
/// number of execution turns; preparation runs normally. No external source I/O.
#[sqlx::test]
#[ignore = "manual production-Web slot; CRM_FAMILY_BROWSER_OUTPUT required"]
async fn family_refresh_browser_fixture(pool: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_FAMILY_BROWSER_OUTPUT").expect("explicit private evidence directory"),
    );
    assert!(output.is_absolute());
    std::fs::create_dir_all(&output).unwrap();
    let (f, parent, report, capture) = fixture(&pool).await;
    let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(&pool)
        .await
        .unwrap();
    let person: Uuid = sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 LIMIT 1")
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    std::fs::write(output.join("fixture.json"),serde_json::to_vec_pretty(&json!({"email":email,"password":"synthetic import fixture password","parent":parent,"report":report,"capture":capture,"organization":f.org,"database":database,"person":person})).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3118")
        .await
        .unwrap();
    let router = f.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let release = ReleaseReadiness::for_tests();
    let mut executed = 0usize;
    while !output.join("stop").exists() {
        if output.join("pause").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        for _ in 0..32 {
            if preparation_worker::run_once(&f.pool, &f.key, &f.policy)
                .await
                .unwrap()
                == preparation_worker::Progress::Idle
            {
                break;
            }
        }
        let limit = std::fs::read_to_string(output.join("execute"))
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if executed < limit
            && execution::run_once(&f.pool, &f.key, &f.policy, Some(&release))
                .await
                .unwrap()
                != preparation_worker::Progress::Idle
        {
            executed += 1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    server.abort();
}

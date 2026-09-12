//! Isolated synthetic retained-history fixtures; no external source requests.
#![allow(dead_code)]
use crate::{db_history_capture_support as capture, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        history_capture_source::Stream, history_import as h, history_import_worker,
    },
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
pub async fn fixture(pool: &PgPool) -> (Fixture, Uuid, Uuid, Arc<capture::HistoryBook>) {
    let (f, parent, book) = capture::fixture(pool).await;
    book.set_records(Stream::Events,vec![json!({"id":1,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"IMPORT_BODY_SENTINEL"})]);
    book.set_records(Stream::Calls,vec![json!({"id":2,"personId":101,"userId":3,"created":"2026-01-02T00:00:00Z","duration":1.5,"outcome":"Vendor label","phone":"IMPORT_PHONE_SENTINEL"})]);
    book.set_records(Stream::TextMessages,vec![json!({"id":3,"personId":102,"userId":3,"created":"unknown","sent":"2026-01-03T00:00:00Z","message":"IMPORT_TEXT_SENTINEL","status":"Vendor status"})]);
    let (id, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, id).await;
    capture::drain(&f, &book).await;
    assert_eq!(capture::ready(&f, id).await["state"], "completed_with_gaps");
    (f, parent, id, book)
}
pub async fn prepare(f: &Fixture, parent: Uuid, capture_id: Uuid) -> Uuid {
    let source = capture::ready(f, capture_id).await;
    let workspace: i64 =
        sqlx::query_scalar("SELECT workspace_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let value = h::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::PrepareFubHistoryImport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            capture_id,
            expected_capture_revision: source["revision"].as_str().unwrap().into(),
            expected_workspace_revision: workspace.to_string(),
            expected_policy_revision: f.policy.revision(),
        },
    )
    .await
    .unwrap();
    Uuid::parse_str(value["import_id"].as_str().unwrap()).unwrap()
}
pub async fn detail(f: &Fixture, id: Uuid) -> Value {
    h::detail(&f.pool, &f.policy, &f.ctx, id).await.unwrap()
}
pub async fn one(f: &Fixture) -> bool {
    history_import_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap()
}
pub async fn drain(f: &Fixture) {
    for _ in 0..10000 {
        if !one(f).await {
            return;
        }
    }
    panic!("bounded fixture did not drain");
}
pub fn confirmation(v: &Value) -> h::ConfirmFubHistoryImport {
    h::ConfirmFubHistoryImport {
        request_id: Uuid::new_v4(),
        plan_id: Uuid::parse_str(v["plan_id"].as_str().unwrap()).unwrap(),
        expected_revision: v["revision"].as_str().unwrap().into(),
        expected_plan_revision: v["plan_revision"].as_str().unwrap().into(),
        expected_workspace_revision: v["workspace_revision"].as_str().unwrap().into(),
        expected_policy_revision: v["policy_revision"].as_str().unwrap().into(),
        acknowledgements: h::Acknowledgements {
            external_facts: true,
            date_uncertainty: true,
            coverage_and_holds: true,
            review_only: true,
        },
    }
}
pub async fn confirm(f: &Fixture, id: Uuid) {
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        confirmation(&detail(f, id).await),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
pub async fn ready(f: &Fixture, parent: Uuid, capture_id: Uuid) -> Uuid {
    let id = prepare(f, parent, capture_id).await;
    drain(f).await;
    assert_eq!(detail(f, id).await["state"], "ready");
    id
}

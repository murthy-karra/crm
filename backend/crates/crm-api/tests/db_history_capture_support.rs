//! Constructed history fixtures. All source I/O is an in-process recorder.
#![allow(dead_code)]
use crate::import_support::{self, Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        history_capture as h,
        history_capture_source::{Request, Stream},
        history_capture_worker,
        reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
};
use uuid::Uuid;
pub struct HistoryBook {
    pub calls: AtomicUsize,
    pub identity_calls: AtomicUsize,
    pub account: AtomicI64,
    pub identity_raw: RwLock<Option<Vec<u8>>>,
    pub user: AtomicI64,
    pub requests: Mutex<Vec<String>>,
    pages: RwLock<BTreeMap<(String, u64), Result<Capture, ReaderError>>>,
    pub block: AtomicBool,
    pub entered: tokio::sync::Notify,
    pub continue_io: tokio::sync::Notify,
}
impl HistoryBook {
    pub fn new() -> Arc<Self> {
        let b = Arc::new(Self {
            calls: AtomicUsize::new(0),
            identity_calls: AtomicUsize::new(0),
            account: AtomicI64::new(17),
            identity_raw: RwLock::new(None),
            user: AtomicI64::new(3),
            requests: Mutex::new(Vec::new()),
            pages: RwLock::new(BTreeMap::new()),
            block: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            continue_io: tokio::sync::Notify::new(),
        });
        for s in [Stream::Events, Stream::Calls, Stream::TextMessages] {
            b.set_records(s, vec![])
        }
        b
    }
    pub fn set_records(&self, s: Stream, records: Vec<Value>) {
        let mut pages = self.pages.write().unwrap();
        pages.retain(|(f, _), _| f != s.as_str());
        for offset in (0..records.len().max(1)).step_by(100) {
            let end = (offset + 100).min(records.len());
            pages.insert((s.as_str().into(),offset as u64),Ok(Capture{status:200,body:serde_json::to_vec(&json!({"_metadata":{"collection":s.collection(),"limit":100,"offset":offset,"total":records.len()},s.collection():&records[offset..end]})).unwrap(),truncated:false,source_version:Some("synthetic-v1".into())}));
        }
    }
    pub fn set_raw(&self, s: Stream, offset: u64, value: Result<Capture, ReaderError>) {
        self.pages
            .write()
            .unwrap()
            .insert((s.as_str().into(), offset), value);
    }
    pub fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    async fn wait(&self) {
        if self.block.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.continue_io.notified().await;
        }
    }
}
#[async_trait::async_trait]
impl FubReader for HistoryBook {
    async fn history(&self, _: &str, r: &Request) -> Result<Capture, ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(r.path()?);
        self.wait().await;
        self.pages
            .read()
            .unwrap()
            .get(&(r.stream.as_str().into(), r.cursor.offset))
            .cloned()
            .unwrap_or(Err(ReaderError::Unavailable))
    }
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.identity_calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push("identity".into());
        self.wait().await;
        let account = self.account.load(Ordering::SeqCst);
        let user = self.user.load(Ordering::SeqCst);
        Ok((
            Identity {
                account_id: account,
                user_id: Some(user),
                account_domain: None,
                display_name: None,
            },
            self.identity_raw
                .read()
                .unwrap()
                .clone()
                .unwrap_or_else(|| {
                    serde_json::to_vec(&json!({"account":{"id":account},"user":{"id":user}}))
                        .unwrap()
                }),
        ))
    }
    async fn probe(&self, _: &str, _: Probe) -> Result<ProbeResult, ReaderError> {
        panic!("history must not use assessment probes")
    }
}
pub async fn fixture(migrator: &PgPool) -> (Fixture, Uuid, Arc<HistoryBook>) {
    let f = import_support::fixture(migrator, import_support::default_people()).await;
    let parent = crate::db_activity_source::completed_parent(&f).await;
    (f, parent, HistoryBook::new())
}
pub fn item(id: u64) -> Value {
    json!({"id":id,"personId":101,"userId":3,"type":"Registration","created":"2026-01-01T00:00:00Z","message":"SYNTHETIC_PRIVATE_HISTORY_SENTINEL","participants":[{"personId":102}]})
}
pub async fn propose(f: &Fixture, parent: Uuid) -> (Uuid, Value) {
    let c = sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let v = h::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::ProposeFubHistoryCapture {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            connection_id: c.get("id"),
            expected_revision: c.get::<i32, _>("revision").to_string(),
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(v["capture_id"].as_str().unwrap()).unwrap();
    (id, ready(f, id).await)
}
pub async fn ready(f: &Fixture, id: Uuid) -> Value {
    h::detail(&f.pool, &f.policy, &f.ctx, id).await.unwrap()
}
pub fn action(v: &Value) -> h::HistoryRequest {
    h::HistoryRequest {
        request_id: Uuid::new_v4(),
        expected_run_revision: v["revision"].as_str().unwrap().into(),
    }
}
pub fn confirmation(v: &Value) -> h::ConfirmFubHistoryCapture {
    h::ConfirmFubHistoryCapture {
        request_id: Uuid::new_v4(),
        expected_run_revision: v["revision"].as_str().unwrap().into(),
        acknowledgements: h::Acknowledgements {
            api_visible_account_scope: true,
            coverage_gaps: true,
            retained_not_imported: true,
            source_user_evidence_revision: v["source_user_evidence_revision"]
                .as_str()
                .unwrap()
                .into(),
            source_user_difference: v["source_user_difference"].as_bool().unwrap(),
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
        confirmation(&ready(f, id).await),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
pub async fn one(f: &Fixture, book: &HistoryBook) -> bool {
    history_capture_worker::run_once(
        &f.pool,
        &f.key,
        book,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap()
}
pub async fn drain(f: &Fixture, book: &HistoryBook) {
    for _ in 0..2000 {
        if !one(f, book).await {
            return;
        }
    }
    panic!("history fixture exceeded bounded work")
}

//! Shared synthetic 010c fixture using the real retained snapshot/preview path.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};

use axum::Router;
use crm_api::{
    config::RawPayloadKey,
    domain::{
        admin::{MembershipStatus, Role},
        envelope::{CommandContext, Origin},
        migration::{
            commands, import_worker,
            imports::{self, AssigneePatch, StagePatch},
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::{self, SnapshotPolicy, SnapshotRequest, SourceAction},
            snapshot_preview,
            snapshot_source::{Request, Stream},
            snapshot_worker,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "synthetic import fixture password";

pub struct Book {
    pub calls: AtomicUsize,
    pages: RwLock<BTreeMap<(String, u64), Capture>>,
}

fn collection(stream: Stream) -> &'static str {
    match stream {
        Stream::Users => "users",
        Stream::Stages => "stages",
        Stream::CustomFields => "customfields",
        Stream::People => "people",
        Stream::Notes => "notes",
        Stream::TasksOpen | Stream::TasksCompleted => "tasks",
        Stream::NoteDetail => panic!("use set_raw for note detail fixtures"),
    }
}

impl Book {
    pub fn new(people: Vec<Value>) -> Self {
        let book = Self {
            calls: AtomicUsize::new(0),
            pages: RwLock::new(BTreeMap::new()),
        };
        for stream in [
            Stream::CustomFields,
            Stream::Notes,
            Stream::TasksOpen,
            Stream::TasksCompleted,
        ] {
            book.set_records(stream, vec![]);
        }
        book.set_records(
            Stream::Users,
            vec![
                json!({"id":3,"name":"Synthetic source user","email":"source-user@synthetic.test"}),
            ],
        );
        book.set_records(Stream::Stages, vec![json!({"id":4,"name":"Lead"})]);
        book.set_records(Stream::People, people);
        book
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn set_records(&self, stream: Stream, records: Vec<Value>) {
        let collection = collection(stream);
        let mut pages = self.pages.write().unwrap();
        pages.retain(|(name, _), _| name != stream.as_str());
        for offset in (0..records.len().max(1)).step_by(100) {
            let end = (offset + 100).min(records.len());
            let body = json!({"_metadata":{"collection":collection,"limit":100,"offset":offset,"total":records.len()},collection:&records[offset..end]});
            pages.insert(
                (stream.as_str().into(), offset as u64),
                Capture {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                    truncated: false,
                    source_version: Some("v1-synthetic".into()),
                },
            );
        }
    }

    pub fn set_raw(
        &self,
        stream: Stream,
        offset: u64,
        status: u16,
        body: Vec<u8>,
        truncated: bool,
    ) {
        self.pages.write().unwrap().insert(
            (stream.as_str().into(), offset),
            Capture {
                status,
                body,
                truncated,
                source_version: Some("v1-synthetic".into()),
            },
        );
    }
}

#[async_trait::async_trait]
impl FubReader for Book {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((
            Identity {
                account_id: 17,
                user_id: Some(3),
                account_domain: None,
                display_name: None,
            },
            br#"{"account":{"id":17},"user":{"id":3}}"#.to_vec(),
        ))
    }

    async fn probe(&self, _: &str, _: Probe) -> Result<ProbeResult, ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ReaderError::Unavailable)
    }

    async fn snapshot(&self, _: &str, request: &Request) -> Result<Capture, ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if request.cursor.next.is_some() {
            return Err(ReaderError::MalformedResponse);
        }
        self.pages
            .read()
            .unwrap()
            .get(&(request.stream.as_str().into(), request.cursor.offset))
            .cloned()
            .ok_or(ReaderError::Unavailable)
    }
}

pub struct Fixture {
    pub app: Router,
    pub pool: PgPool,
    pub key: RawPayloadKey,
    pub org: Uuid,
    pub actor: Uuid,
    pub member: Uuid,
    pub cookie: String,
    pub member_cookie: String,
    pub snapshot: Uuid,
    pub preview: Uuid,
    pub reader: Arc<Book>,
    pub ctx: CommandContext,
    pub lead_stage: Uuid,
    pub policy: SnapshotPolicy,
}

pub fn default_people() -> Vec<Value> {
    vec![
        json!({"id":101,"firstName":"Synthetic One","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
        json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}]}),
        json!({"id":103,"firstName":"Synthetic held","stage":"Lead","isTrash":true}),
    ]
}

pub async fn fixture(migrator: &PgPool, people: Vec<Value>) -> Fixture {
    fixture_with_book(migrator, Arc::new(Book::new(people))).await
}

pub async fn fixture_with_book(migrator: &PgPool, reader: Arc<Book>) -> Fixture {
    let suffix = Uuid::new_v4();
    let org =
        crate::common::create_org(migrator, &format!("Synthetic import workspace {suffix}")).await;
    let email = format!("import-admin-{suffix}@synthetic.test");
    let member_email = format!("import-member-{suffix}@synthetic.test");
    let actor = crate::common::create_user(migrator, &email, "Synthetic admin", PASSWORD).await;
    let member =
        crate::common::create_user(migrator, &member_email, "Synthetic member", PASSWORD).await;
    crate::common::add_membership_with(migrator, org, actor, Role::Admin, MembershipStatus::Active)
        .await;
    crate::common::add_membership_with(
        migrator,
        org,
        member,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let pool = crate::common::connect_as_app(migrator).await;
    let config = crate::common::test_config();
    let key = config.raw_payload_key.clone();
    let mut state = AppState::for_tests(pool.clone(), &config, Publisher::recording())
        .with_migration_reader(reader.clone());
    // Compile-time test-support capability: never a tenant request parameter.
    state.import_release = Some(Arc::new(
        crm_app::auth::workspace::ReleaseReadiness::for_tests(),
    ));
    let app = crm_api::build_app(state);
    let cookie = crate::common::login_cookie(&app, &email, PASSWORD).await;
    let member_cookie = crate::common::login_cookie(&app, &member_email, PASSWORD).await;
    let ctx = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(actor),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let policy = SnapshotPolicy::default();
    let connection = commands::connect_fub(
        &pool,
        &key,
        reader.as_ref(),
        &ctx,
        commands::ConnectFub {
            request_id: Uuid::new_v4(),
            api_key: "synthetic import source key".into(),
        },
    )
    .await
    .unwrap()
    .value;
    let proposal = snapshot::propose(
        &pool,
        &key,
        &policy,
        &ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection.id,
            expected_revision: connection.revision,
        },
    )
    .await
    .unwrap();
    let snapshot = Uuid::parse_str(proposal["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &pool,
        &key,
        &policy,
        &ctx,
        snapshot,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    drain_source(&pool, &key, reader.as_ref(), &policy).await;
    let source = snapshot::detail(&pool, &policy, &ctx, snapshot)
        .await
        .unwrap();
    assert!(
        matches!(
            source["snapshot"]["state"].as_str(),
            Some("completed" | "completed_with_gaps")
        ),
        "synthetic snapshot must settle before import: {source}"
    );
    let proposed = snapshot_preview::generate(
        &pool,
        &key,
        &policy,
        &ctx,
        snapshot,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let preview = Uuid::parse_str(proposed["preview_id"].as_str().unwrap()).unwrap();
    drain_preview(&pool, &key, &policy).await;
    let lead_stage =
        sqlx::query_scalar("SELECT id FROM stage WHERE organization_id=$1 AND name='Lead'")
            .bind(org)
            .fetch_one(&pool)
            .await
            .unwrap();
    Fixture {
        app,
        pool,
        key,
        org,
        actor,
        member,
        cookie,
        member_cookie,
        snapshot,
        preview,
        reader,
        ctx,
        lead_stage,
        policy,
    }
}

pub async fn drain_source(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &Book,
    policy: &SnapshotPolicy,
) {
    for _ in 0..200 {
        if !snapshot_worker::run_once(pool, key, reader, policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("synthetic source exceeded bounded fixture work");
}

pub async fn drain_preview(pool: &PgPool, key: &RawPayloadKey, policy: &SnapshotPolicy) {
    for _ in 0..200 {
        if !snapshot_preview::run_once(pool, key, policy).await.unwrap() {
            return;
        }
    }
    panic!("synthetic preview exceeded bounded fixture work");
}

pub async fn drain_import(fixture: &Fixture) {
    for _ in 0..2000 {
        if !import_worker::run_once(&fixture.pool, &fixture.key, &fixture.policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("synthetic import exceeded bounded fixture work");
}

pub async fn propose(fixture: &Fixture) -> (Uuid, Uuid) {
    let value = imports::propose(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        imports::PlanPeopleImport {
            request_id: Uuid::new_v4(),
            snapshot_id: fixture.snapshot,
            preview_id: fixture.preview,
        },
        &fixture.policy,
    )
    .await
    .unwrap();
    (
        Uuid::parse_str(value["import_id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(value["plan_id"].as_str().unwrap()).unwrap(),
    )
}

pub async fn replan(
    fixture: &Fixture,
    import_id: Uuid,
    revision: &str,
    stages: &[StagePatch],
    assignees: &[AssigneePatch],
) -> (Uuid, Uuid) {
    let value = imports::replan(
        &fixture.pool,
        &fixture.key,
        &fixture.ctx,
        import_id,
        imports::ReplanPeopleImport {
            request_id: Uuid::new_v4(),
            expected_plan_revision: revision.into(),
            stage_mappings: stages.to_vec(),
            assignee_mappings: assignees.to_vec(),
        },
        &fixture.policy,
    )
    .await
    .unwrap();
    (
        Uuid::parse_str(value["import_id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(value["plan_id"].as_str().unwrap()).unwrap(),
    )
}

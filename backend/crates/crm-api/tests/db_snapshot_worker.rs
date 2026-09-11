//! Synthetic-only source durability, frozen preview and tenant/lease integration.
use crm_api::{
    config::RawPayloadKey,
    domain::{
        envelope::{CommandContext, Origin},
        migration::{
            commands, crypto,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::{self, SnapshotPolicy, SnapshotRequest, SourceAction},
            snapshot_preview,
            snapshot_source::{Request, Stream},
            snapshot_worker, MigrationError,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;
use uuid::Uuid;
#[derive(Default)]
struct Book {
    block: AtomicUsize,
    mode: AtomicUsize,
    entered: Notify,
    release: Notify,
}
#[async_trait::async_trait]
impl FubReader for Book {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
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
        Err(ReaderError::Unavailable)
    }
    async fn snapshot(&self, _: &str, request: &Request) -> Result<Capture, ReaderError> {
        if request.stream == Stream::People && self.block.swap(0, Ordering::SeqCst) == 1 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        let (status, body) = match request.stream {
            Stream::Users => (
                200,
                json!({"_metadata":{"collection":"users","offset":0,"limit":100,"total":1},"users":[{"id":3,"name":"Synthetic","email":"snapshot-worker@synthetic.test"}]}),
            ),
            Stream::Stages => (
                200,
                json!({"_metadata":{"collection":"stages","offset":0,"limit":100,"total":1},"stages":[{"id":4,"name":"Lead"}]}),
            ),
            Stream::CustomFields => (
                200,
                json!({"_metadata":{"collection":"customfields","offset":0,"limit":100,"total":"1"},"customfields":[{"id":5,"name":"customBirthday","label":"Birthday","type":"date","isRecurring":true}]}),
            ),
            Stream::People if matches!(self.mode.load(Ordering::SeqCst), 2 | 3) => {
                if self.mode.load(Ordering::SeqCst) == 3 && request.cursor.offset > 0 {
                    (403, json!({"error":"synthetic pause after first page"}))
                } else {
                    let start = request.cursor.offset + 1;
                    let end = (start + 100).min(102);
                    (
                        200,
                        json!({"_metadata":{"collection":"people","offset":request.cursor.offset,"limit":100,"total":101},"people":(start..end).map(|id|json!({"id":id,"firstName":"Synthetic paged Person","stage":"Lead","phones":[{"value":"4155550100"}]})).collect::<Vec<_>>()}),
                    )
                }
            }
            Stream::People if self.mode.load(Ordering::SeqCst) == 1 => {
                (403, json!({"errorMessage":"synthetic restricted private"}))
            }
            Stream::People => (
                200,
                json!({"_metadata":{"collection":"people","offset":0,"limit":100,"total":2},"people":[{"id":9007199254740993u64,"firstName":"Synthetic One","stage":"Lead","phones":[{"value":"4155550100"}]},{"id":9007199254740994u64,"firstName":"Synthetic Two","phones":[{"value":"(415)555-0100"}]}]}),
            ),
            Stream::Notes => (
                200,
                json!({"_metadata":{"collection":"notes","offset":0,"limit":100,"total":2},"notes":[{"id":31,"personId":9007199254740993u64,"body":"<b>Synthetic note</b>","createdById":3},{"id":32,"body":"List note remains readable","personId":9007199254740994u64}]}),
            ),
            Stream::NoteDetail if request.source_id.as_deref() == Some("32") => {
                (404, json!({"errorMessage":"Synthetic detail unavailable"}))
            }
            Stream::NoteDetail => (
                200,
                json!({"id":31,"personId":9007199254740993u64,"body":"<b>Synthetic note</b>","createdById":3,"replies":[{"id":99,"body":"Synthetic reply"}],"reactions":{"thumbs_up":[3]}}),
            ),
            Stream::TasksOpen => (
                200,
                json!({"_metadata":{"collection":"tasks","offset":0,"limit":100,"total":1},"tasks":[{"id":41,"name":"Synthetic task","type":"call","assignedUserId":3,"dueDate":"2026-09-15","isCompleted":false}]}),
            ),
            Stream::TasksCompleted => (
                200,
                json!({"_metadata":{"collection":"tasks","offset":0,"limit":100,"total":1},"tasks":[{"id":41,"name":"Synthetic task","type":"call","assignedUserId":3,"dueDate":"2026-09-15","isCompleted":true}]}),
            ),
        };
        Ok(Capture {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            truncated: false,
            source_version: Some("v1".into()),
        })
    }
}
struct Fixture {
    pool: PgPool,
    ctx: CommandContext,
    key: RawPayloadKey,
    reader: Arc<Book>,
    policy: SnapshotPolicy,
}
async fn fixture(migrator: &PgPool) -> Fixture {
    let (org, user) = crate::common::create_org_with_stages_and_member(
        migrator,
        "Snapshot worker synthetic",
        "snapshot-worker@synthetic.test",
        "Synthetic",
        "synthetic password long enough",
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(org)
    .bind(user)
    .execute(migrator)
    .await
    .unwrap();
    Fixture {
        pool: crate::common::connect_as_app(migrator).await,
        ctx: CommandContext {
            organization_id: OrganizationId::new(org),
            actor_user_id: UserId::new(user),
            origin: Origin::WebSession,
            correlation_id: CorrelationId::new(Uuid::new_v4()),
        },
        key: RawPayloadKey::new([17; 32]),
        reader: Arc::new(Book::default()),
        policy: SnapshotPolicy::default(),
    }
}
async fn start(f: &Fixture) -> Uuid {
    let c = commands::connect_fub(
        &f.pool,
        &f.key,
        f.reader.as_ref(),
        &f.ctx,
        commands::ConnectFub {
            request_id: Uuid::new_v4(),
            api_key: "synthetic source key".into(),
        },
    )
    .await
    .unwrap()
    .value;
    let proposed = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: c.id,
            expected_revision: c.revision,
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(proposed["snapshot"]["id"].as_str().unwrap()).unwrap();
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
    id
}
async fn drain(f: &Fixture) {
    for _ in 0..40 {
        if !snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("worker failed to settle bounded book")
}
async fn preview(f: &Fixture, id: Uuid) -> Uuid {
    let v = snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let preview = Uuid::parse_str(v["preview_id"].as_str().unwrap()).unwrap();
    for _ in 0..10 {
        if !snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
    }
    preview
}
fn page(family: &str) -> snapshot::PageQuery {
    snapshot::PageQuery {
        family: Some(family.into()),
        ..Default::default()
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn core_capture_preserves_encrypted_bytes_gaps_variants_and_frozen_preview(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let id = start(&f).await;
    drain(&f).await;
    let detail = snapshot::detail(&f.pool, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(detail["snapshot"]["state"], "completed_with_gaps");
    assert_eq!(detail["snapshot"]["reserved_bytes"], "0");
    let business: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.ctx.organization_id.0)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(business, 0);
    let rows = sqlx::query(
        "SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 ORDER BY sequence",
    )
    .bind(id)
    .fetch_all(&migrator)
    .await
    .unwrap();
    assert!(rows.len() >= 10);
    for r in &rows {
        let body = crypto::open_snapshot(
            &f.key,
            f.ctx.organization_id,
            id,
            r.get("id"),
            "capture",
            r.get("nonce"),
            r.get("ciphertext"),
        )
        .unwrap();
        assert_eq!(body.len() as i64, r.get::<i64, _>("raw_byte_len"));
        assert!(serde_json::from_slice::<Value>(&body).is_ok());
        assert!(crypto::open_snapshot(
            &f.key,
            f.ctx.organization_id,
            Uuid::new_v4(),
            r.get("id"),
            "capture",
            r.get("nonce"),
            r.get("ciphertext")
        )
        .is_err());
    }
    let preview = preview(&f, id).await;
    let report = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, id, preview)
        .await
        .unwrap();
    assert_eq!(report["preview"]["state"], "completed");
    assert_eq!(report["destination_stale"], false);
    let notes = snapshot_preview::records(&f.pool, &f.key, &f.ctx, id, preview, page("notes"))
        .await
        .unwrap();
    assert_eq!(notes["records"].as_array().unwrap().len(), 2);
    let denied = notes["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["source_id"] == "32")
        .unwrap();
    assert!(denied["issues"]
        .as_array()
        .unwrap()
        .contains(&json!("note_detail_unavailable")));
    let tasks = snapshot_preview::records(&f.pool, &f.key, &f.ctx, id, preview, page("tasks"))
        .await
        .unwrap();
    assert_eq!(tasks["records"].as_array().unwrap().len(), 1);
    assert_eq!(tasks["records"][0]["disposition"], "needs_decision");
    let groups = snapshot_preview::groups(&f.pool, &f.key, &f.ctx, id, preview, Default::default())
        .await
        .unwrap();
    assert_eq!(groups["groups"].as_array().unwrap().len(), 1);
    assert_eq!(groups["groups"][0]["member_count"], "2");
    let group = Uuid::parse_str(groups["groups"][0]["id"].as_str().unwrap()).unwrap();
    let first = snapshot_preview::members(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        preview,
        group,
        snapshot::PageQuery {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(first["members"].as_array().unwrap().len(), 1);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = snapshot_preview::members(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        preview,
        group,
        snapshot::PageQuery {
            limit: Some(1),
            cursor: Some(cursor.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_ne!(
        first["members"][0]["source_id"],
        second["members"][0]["source_id"]
    );
    assert!(snapshot_preview::groups(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        preview,
        snapshot::PageQuery {
            cursor: Some(cursor),
            ..Default::default()
        }
    )
    .await
    .is_err());
    sqlx::query(
        "UPDATE stage SET name='Changed synthetic stage' WHERE organization_id=$1 AND name='Lead'",
    )
    .bind(f.ctx.organization_id.0)
    .execute(&migrator)
    .await
    .unwrap();
    let stale = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, id, preview)
        .await
        .unwrap();
    assert_eq!(stale["destination_stale"], true);
    assert_eq!(stale["counts"], report["counts"]);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn reservations_pause_without_advancing_and_preview_survives_disconnection(migrator: PgPool) {
    let mut f = fixture(&migrator).await;
    let id = start(&f).await;
    assert!(
        snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
            .await
            .unwrap()
    );
    let before = snapshot::detail(&f.pool, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    f.policy.run_ceiling_bytes = snapshot::SOURCE_RESERVATION - 1;
    assert!(
        !snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
            .await
            .unwrap()
    );
    let after = snapshot::detail(&f.pool, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(
        after["snapshot"]["pause_reason"],
        "storage_budget_exhausted"
    );
    assert_eq!(
        after["snapshot"]["capture_sequence"],
        before["snapshot"]["capture_sequence"]
    );
    assert_eq!(after["snapshot"]["reserved_bytes"], "0");
    let connection = Uuid::parse_str(after["snapshot"]["connection_id"].as_str().unwrap()).unwrap();
    commands::disconnect_fub(
        &f.pool,
        &f.ctx,
        commands::DisconnectFub {
            connection_id: connection,
        },
    )
    .await
    .unwrap();
    let preview = preview(&f, id).await;
    assert_eq!(
        snapshot_preview::detail(&f.pool, &f.key, &f.ctx, id, preview)
            .await
            .unwrap()["preview"]["state"],
        "completed"
    );
    let d = snapshot::detail(&f.pool, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(d["snapshot"]["reserved_bytes"], "0");
    assert!(!d["snapshot"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("retry")));
}
#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn revoked_inflight_source_cannot_commit_and_another_admin_cannot_takeover(migrator: PgPool) {
    let mut single = fixture(&migrator).await;
    single.pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(single.pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let f = Arc::new(single);
    let id = start(&f).await;
    for _ in 0..4 {
        assert!(
            snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
                .await
                .unwrap()
        );
    }
    f.reader.block.store(1, Ordering::SeqCst);
    let pending = {
        let f = f.clone();
        tokio::spawn(async move {
            snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await
        })
    };
    f.reader.entered.notified().await;
    let acquired = tokio::time::timeout(std::time::Duration::from_secs(1), f.pool.acquire())
        .await
        .unwrap()
        .unwrap();
    drop(acquired);
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.ctx.organization_id.0)
    .bind(f.ctx.actor_user_id.0)
    .execute(&migrator)
    .await
    .unwrap();
    f.reader.release.notify_one();
    pending.await.unwrap().unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_snapshot_record WHERE snapshot_id=$1 AND family='people'",
    )
    .bind(id)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(
        snapshot::detail(&f.pool, &f.policy, &f.ctx, id).await,
        Err(MigrationError::Forbidden)
    ));
    let user = crate::common::create_user(
        &migrator,
        "other-snapshot-admin@synthetic.test",
        "Other",
        "synthetic password long enough",
    )
    .await;
    crate::common::add_membership_with(
        &migrator,
        f.ctx.organization_id.0,
        user,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let other = CommandContext {
        organization_id: f.ctx.organization_id,
        actor_user_id: UserId::new(user),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    assert!(matches!(
        snapshot::source_action(
            &f.pool,
            &f.key,
            &f.policy,
            &other,
            id,
            SnapshotRequest {
                request_id: Uuid::new_v4()
            },
            SourceAction::Retry
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert!(snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &other,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4()
        }
    )
    .await
    .is_ok());
}
#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn failed_gap_capture_commit_reclaims_lease_without_advancing_checkpoint(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let id = start(&f).await;
    for _ in 0..7 {
        assert!(
            snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
                .await
                .unwrap()
        );
    }
    // identity/users/stages/fields/people/notes/detail31: the next call is detail32's404.
    let before:i64=sqlx::query_scalar("SELECT checkpoint FROM migration_snapshot_stream WHERE snapshot_id=$1 AND stream='note_detail'").bind(id).fetch_one(&migrator).await.unwrap();
    assert_eq!(before, 1);
    sqlx::query("CREATE FUNCTION reject_snapshot_gap() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.classification='content_inaccessible' THEN RAISE EXCEPTION 'synthetic settlement failure'; END IF; RETURN NEW; END $$").execute(&migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_snapshot_gap BEFORE INSERT ON migration_snapshot_capture FOR EACH ROW EXECUTE FUNCTION reject_snapshot_gap()").execute(&migrator).await.unwrap();
    assert!(
        snapshot_worker::run_once(&f.pool, &f.key, f.reader.as_ref(), &f.policy)
            .await
            .is_err()
    );
    let checkpoint:i64=sqlx::query_scalar("SELECT checkpoint FROM migration_snapshot_stream WHERE snapshot_id=$1 AND stream='note_detail'").bind(id).fetch_one(&migrator).await.unwrap();
    assert_eq!(checkpoint, before);
    sqlx::query("DROP TRIGGER reject_snapshot_gap ON migration_snapshot_capture")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE migration_snapshot SET lease_expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(&migrator)
    .await
    .unwrap();
    drain(&f).await;
    let gaps:i64=sqlx::query_scalar("SELECT content_gaps FROM migration_snapshot_stream WHERE snapshot_id=$1 AND stream='note_detail'").bind(id).fetch_one(&migrator).await.unwrap();
    assert_eq!(gaps, 1);
    let identity_captures:i64=sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='identity' AND accepted").bind(id).fetch_one(&migrator).await.unwrap();
    assert_eq!(
        identity_captures, 2,
        "expired same-process source lease must revalidate identity"
    );

    let reserved: i64 =
        sqlx::query_scalar("SELECT reserved_bytes FROM migration_snapshot WHERE id=$1")
            .bind(id)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(reserved, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn preview_resume_uses_frozen_source_destination_and_gap_coverage(migrator: PgPool) {
    let mut f = fixture(&migrator).await;
    f.reader.mode.store(3, Ordering::SeqCst);
    let run = start(&f).await;
    drain(&f).await;
    assert_eq!(
        snapshot::detail(&f.pool, &f.policy, &f.ctx, run)
            .await
            .unwrap()["snapshot"]["state"],
        "paused"
    );
    let proposed = snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        run,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(proposed["preview_id"].as_str().unwrap()).unwrap();
    for _ in 0..2 {
        assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap());
    }
    let partial = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, run, id)
        .await
        .unwrap();
    assert_eq!(partial["preview"]["state"], "queued");
    let frozen_boundary = partial["preview"]["capture_sequence"].clone();
    let retained: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_snapshot WHERE id=$1")
            .bind(run)
            .fetch_one(&migrator)
            .await
            .unwrap();
    f.policy.run_ceiling_bytes = retained + snapshot::PREVIEW_RESERVATION - 1;
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    assert_eq!(
        snapshot_preview::detail(&f.pool, &f.key, &f.ctx, run, id)
            .await
            .unwrap()["preview"]["pause_reason"],
        "storage_budget_exhausted"
    );
    sqlx::query("UPDATE stage SET name='Changed while preview paused' WHERE organization_id=$1 AND name='Lead'").bind(f.ctx.organization_id.0).execute(&migrator).await.unwrap();
    f.policy = SnapshotPolicy::default();
    f.reader.mode.store(2, Ordering::SeqCst);
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        run,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Retry,
    )
    .await
    .unwrap();
    drain(&f).await;
    // A worker unable to read the pinned engine version must not adopt old pages.
    sqlx::query(
        "UPDATE migration_snapshot_preview SET engine_version='future',state='queued' WHERE id=$1",
    )
    .bind(id)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    assert_eq!(
        snapshot_preview::detail(&f.pool, &f.key, &f.ctx, run, id)
            .await
            .unwrap()["preview"]["pause_reason"],
        "comparison_version_unavailable"
    );
    sqlx::query("UPDATE migration_snapshot_preview SET engine_version='1' WHERE id=$1")
        .bind(id)
        .execute(&migrator)
        .await
        .unwrap();
    snapshot_preview::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        run,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    for _ in 0..10 {
        if !snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
    }
    let old = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, run, id)
        .await
        .unwrap();
    assert_eq!(old["preview"]["state"], "completed");
    assert_eq!(old["preview"]["capture_sequence"], frozen_boundary);
    assert_eq!(old["coverage"], partial["coverage"]);
    assert_eq!(old["destination_stale"], true);
    let total = |report: &Value| {
        report["counts"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["family"] == "people")
            .map(|r| r["count"].as_str().unwrap().parse::<usize>().unwrap())
            .sum::<usize>()
    };
    assert_eq!(total(&old), 100);
    let overlap = old["counts"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == "people" && r["issue"] == "normalized_contact_overlap")
        .unwrap();
    assert_eq!(
        overlap["count"], "100",
        "resumed pages must not double count issues"
    );
    let field_issues: usize = old["counts"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] == "custom_fields")
        .map(|r| r["count"].as_str().unwrap().parse::<usize>().unwrap())
        .sum();
    assert!(
        field_issues >= 2,
        "one field may have several separately counted issues"
    );

    let first = snapshot_preview::records(&f.pool, &f.key, &f.ctx, run, id, page("people"))
        .await
        .unwrap();
    assert!(first["records"][0]["candidates"]["stage_id"].is_string());
    let fresh = preview(&f, run).await;
    let new = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, run, fresh)
        .await
        .unwrap();
    assert_eq!(total(&new), 101);
    assert_eq!(new["destination_stale"], false);
    // IDs31/41/etc also exist as People; their note/task/user/stage records are
    // separate source families and must never inherit the Person's group.
    for family in ["users", "stages", "custom_fields", "notes", "tasks"] {
        let records = snapshot_preview::records(&f.pool, &f.key, &f.ctx, run, fresh, page(family))
            .await
            .unwrap();
        for record in records["records"].as_array().unwrap() {
            assert_eq!(record["overlap_group_count"], "0");
            assert!(!record["issues"]
                .as_array()
                .unwrap()
                .contains(&json!("normalized_contact_overlap")));
        }
    }
    assert!(new["counts"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["issue"] == "normalized_contact_overlap")
        .all(|r| r["family"] == "people"));

    let first = snapshot_preview::records(&f.pool, &f.key, &f.ctx, run, fresh, page("people"))
        .await
        .unwrap();
    assert!(first["records"][0]["candidates"]["stage_id"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn retained_ledger_matches_all_counted_payloads_after_capture_preview_and_cancel(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    let run = start(&f).await;
    drain(&f).await;
    preview(&f, run).await;
    let actual:i64=sqlx::query_scalar("SELECT (COALESCE((SELECT sum(octet_length(ciphertext)+octet_length(nonce)+octet_length(request_fingerprint)+octet_length(representation)+COALESCE(octet_length(source_version),0)) FROM migration_snapshot_capture WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(projection_ciphertext)+octet_length(projection_nonce)+octet_length(semantic_hmac)+octet_length(representation)+COALESCE(octet_length(source_id),0)) FROM migration_snapshot_record WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(source_id)+octet_length(kind)+octet_length(key_hmac)) FROM migration_snapshot_contact_key WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(source_id)) FROM migration_snapshot_note_detail WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(COALESCE(octet_length(cursor_nonce),0)+COALESCE(octet_length(cursor_ciphertext),0)) FROM migration_snapshot_stream WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(input_nonce)+octet_length(input_ciphertext)+octet_length(destination_fingerprint)+octet_length(checkpoint_source_id)+octet_length(checkpoint_group_kind)+octet_length(checkpoint_group_hash)) FROM migration_snapshot_preview WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_id)) FROM migration_snapshot_preview_record WHERE snapshot_id=$1),0)+COALESCE((SELECT sum(octet_length(kind)+octet_length(key_hmac)) FROM migration_snapshot_preview_group WHERE snapshot_id=$1),0))::bigint").bind(run).fetch_one(&migrator).await.unwrap();
    let retained: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_snapshot WHERE id=$1")
            .bind(run)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(actual, retained);
    let org: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.ctx.organization_id.0)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(actual, org);
    snapshot::cancel(&f.pool, &f.policy, &f.ctx, run)
        .await
        .unwrap();
    let after: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_snapshot WHERE id=$1")
            .bind(run)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(after, actual);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn failed_preview_batch_rolls_back_issue_counts_and_fenced_admin_retry_adopts_work(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    f.reader.mode.store(2, Ordering::SeqCst);
    let run = start(&f).await;
    drain(&f).await;
    let generated = snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        run,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(generated["preview_id"].as_str().unwrap()).unwrap();
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    sqlx::query("CREATE FUNCTION reject_preview_page() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic preview page commit failure'; END $$").execute(&migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_preview_page BEFORE INSERT ON migration_snapshot_preview_record FOR EACH ROW EXECUTE FUNCTION reject_preview_page()").execute(&migrator).await.unwrap();
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .is_err());
    let issues: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_snapshot_preview_issue WHERE preview_id=$1",
    )
    .bind(id)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(issues, 0);
    let records: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_snapshot_preview_record WHERE preview_id=$1",
    )
    .bind(id)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(records, 0);
    let old_token: Uuid =
        sqlx::query_scalar("SELECT lease_token FROM migration_snapshot_preview WHERE id=$1")
            .bind(id)
            .fetch_one(&migrator)
            .await
            .unwrap();
    sqlx::query("DROP TRIGGER reject_preview_page ON migration_snapshot_preview_record")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_snapshot_preview SET lease_expires_at=now()-interval '1 second' WHERE id=$1").bind(id).execute(&migrator).await.unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.ctx.organization_id.0)
    .bind(f.ctx.actor_user_id.0)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let user = crate::common::create_user(
        &migrator,
        "preview-resume-admin@synthetic.test",
        "Other current admin",
        "synthetic fixture password",
    )
    .await;
    crate::common::add_membership_with(
        &migrator,
        f.ctx.organization_id.0,
        user,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let ctx = CommandContext {
        organization_id: f.ctx.organization_id,
        actor_user_id: UserId::new(user),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let report = snapshot_preview::detail(&f.pool, &f.key, &ctx, run, id)
        .await
        .unwrap();
    assert_eq!(
        report["preview"]["pause_reason"],
        "requester_not_authorized"
    );
    snapshot_preview::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &ctx,
        run,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let reservations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_reservation WHERE token=$1")
            .bind(old_token)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(
        reservations, 0,
        "old writer admission is fenced before retry"
    );
    for _ in 0..10 {
        if !snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
    }
    let report = snapshot_preview::detail(&f.pool, &f.key, &ctx, run, id)
        .await
        .unwrap();
    assert_eq!(report["preview"]["state"], "completed");
    let overlap = report["counts"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == "people" && r["issue"] == "normalized_contact_overlap")
        .unwrap();
    assert_eq!(overlap["count"], "101");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn budget_receipt_records_previous_policy_and_actual_approver(migrator: PgPool) {
    let mut f = fixture(&migrator).await;
    let run = start(&f).await;
    snapshot::cancel(&f.pool, &f.policy, &f.ctx, run)
        .await
        .unwrap();
    let oldpolicy = f.policy.revision();
    f.policy.run_ceiling_bytes *= 2;
    f.policy.org_ceiling_bytes *= 2;
    let before = snapshot::detail(&f.pool, &f.policy, &f.ctx, run)
        .await
        .unwrap();
    let s = &before["snapshot"];
    let cmd = snapshot::IncreaseCoreSnapshotBudget {
        request_id: Uuid::new_v4(),
        expected_run_budget_revision: s["run_budget_revision"].as_str().unwrap().into(),
        expected_org_budget_revision: s["org_budget_revision"].as_str().unwrap().into(),
        expected_policy_revision: f.policy.revision(),
        run_byte_limit: "3221225472".into(),
        org_byte_limit: "6442450944".into(),
    };
    let result = snapshot::increase_budget(&f.pool, &f.key, &f.policy, &f.ctx, run, cmd)
        .await
        .unwrap();
    assert_eq!(result["budget"]["old_run_policy_revision"], oldpolicy);
    assert_eq!(result["budget"]["old_org_policy_revision"], oldpolicy);
    assert_eq!(result["budget"]["policy_revision"], f.policy.revision());
    assert_eq!(
        result["budget"]["approved_by_user_id"],
        json!(f.ctx.actor_user_id.0)
    );
    assert_eq!(
        result["snapshot"]["run_budget_policy_revision"],
        f.policy.revision()
    );
    assert_eq!(
        result["snapshot"]["org_budget_policy_revision"],
        f.policy.revision()
    );
    assert_eq!(result["snapshot"]["state"], "cancelled");
}

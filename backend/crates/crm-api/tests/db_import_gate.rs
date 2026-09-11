//! Synthetic confirmation, workspace guards, recovery and exact byte inventory.
use std::{sync::Arc, time::Duration};

use chrono::Utc;
use crm_api::{
    domain::{
        admin::{
            commands::{change_member_role, set_member_status, ChangeMemberRole, SetMemberStatus},
            AdminActor, MembershipStatus, Role,
        },
        commands::{change_person_stage, ChangePersonStage, CommandError},
        envelope::{CommandContext, Origin},
        migration::{
            import_worker,
            imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
            snapshot::SnapshotPolicy,
            snapshot_source::Stream,
            MigrationError,
        },
        person::queries as people,
    },
    ids::{PersonId, StageId, UserId},
    realtime::Publisher,
};
use crm_app::auth::{workspace, AuthContext};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::import_support::{
    drain_import, fixture, fixture_with_book, propose, replan, Book, Fixture,
};

fn source_people() -> Vec<Value> {
    vec![
        json!({"id":101,"firstName":"Synthetic review","stage":"Lead","assignedUserId":3,"emails":[{"value":"gate@synthetic.test"}]}),
    ]
}

fn auth(f: &Fixture, user: Uuid, role: Role) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(user),
        actor_email: "synthetic@invalid.test".into(),
        actor_display_name: "Synthetic".into(),
        active_organization_id: f.ctx.organization_id,
        active_organization_name: "Synthetic".into(),
        role,
    }
}

async fn prepare(f: &Fixture, stage: StageChoice) -> (Uuid, Uuid) {
    let (run, _) = propose(f).await;
    drain_import(f).await;
    let (_, plan) = replan(
        f,
        run,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: stage,
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Unassigned,
        }],
    )
    .await;
    drain_import(f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
        .await
        .unwrap();
    assert_eq!(detail["plan"]["state"], "ready");
    assert_ne!(detail["counts"]["eligible_people"], "0");
    (run, plan)
}

async fn confirmation(f: &Fixture, run: Uuid, plan: Uuid) -> imports::ConfirmPeopleImport {
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
        .await
        .unwrap();
    imports::ConfirmPeopleImport {
        request_id: Uuid::new_v4(),
        plan_id: plan,
        plan_revision: detail["plan"]["revision"].as_str().unwrap().into(),
        confirmation_digest: detail["plan"]["confirmation_digest"]
            .as_str()
            .unwrap()
            .into(),
        acknowledgments: imports::Acknowledgments {
            held_count: detail["counts"]["held_people"].as_str().unwrap().into(),
            review_only: true,
            remaining_data: true,
        },
    }
}

async fn confirm(f: &Fixture, run: Uuid, plan: Uuid) -> Result<Value, MigrationError> {
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        confirmation(f, run, plan).await,
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
}

fn spawn_confirm(
    f: &Fixture,
    run: Uuid,
    command: imports::ConfirmPeopleImport,
) -> tokio::task::JoinHandle<Result<Value, MigrationError>> {
    let pool = f.pool.clone();
    let key = f.key.clone();
    let ctx = f.ctx.clone();
    let policy = f.policy.clone();
    tokio::spawn(async move {
        imports::confirm(
            &pool,
            &key,
            &ctx,
            run,
            command,
            &workspace::ReleaseReadiness::for_tests(),
            &policy,
        )
        .await
    })
}

/// Observe the actual blocked advisory request in this isolated database.
/// A scheduling delay is not evidence that the exclusive gate was respected.
async fn wait_for_blocked_confirmation(migrator: &PgPool, blocker: i32) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l WHERE l.locktype='advisory' AND NOT l.granted AND l.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND $1=ANY(pg_blocking_pids(l.pid)))")
                .bind(blocker).fetch_one(migrator).await.unwrap();
            if blocked { return; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("confirmation must reach the held workspace gate");
}

async fn person_count(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

async fn drain_under_policy(f: &Fixture, policy: &SnapshotPolicy) {
    // Execution can first finish its stage phase without allocating a Person.
    // Drive the bounded fixture through the actual admission attempt and idle.
    for _ in 0..10 {
        if !import_worker::run_once(&f.pool, &f.key, policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("small gate fixture did not reach an idle admission boundary");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_shared_read_finishes_before_actual_exclusive_confirmation(migrator: PgPool) {
    let f = fixture(&migrator, source_people()).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    let command = confirmation(&f, run, plan).await;
    let mut connection = f.pool.acquire().await.unwrap();
    let mut read = workspace::read(&mut connection, f.ctx.organization_id)
        .await
        .unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *read)
        .await
        .unwrap();
    assert!(people::summary_by_id(
        &mut read,
        f.ctx.organization_id,
        PersonId::new(Uuid::new_v4())
    )
    .await
    .unwrap()
    .is_none());
    let pending = spawn_confirm(&f, run, command);
    wait_for_blocked_confirmation(&migrator, blocker).await;
    let mode: String = sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(mode, "operational");
    assert!(!pending.is_finished());
    read.commit().await.unwrap();
    let confirmed = pending.await.unwrap().unwrap();
    assert_eq!(confirmed["workspace_mode"], "migration_review");
    assert_eq!(person_count(&f).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_ordinary_insert_commits_before_entry_and_emptiness_rejects(migrator: PgPool) {
    let f = fixture(&migrator, source_people()).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    let command = confirmation(&f, run, plan).await;
    let mut ordinary = workspace::begin(&f.pool, f.ctx.organization_id)
        .await
        .unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *ordinary)
        .await
        .unwrap();
    let person = people::insert_person(
        &mut ordinary,
        f.ctx.organization_id,
        Some("Ordinary prior commit"),
        None,
        StageId::new(f.lead_stage),
        None,
    )
    .await
    .unwrap();
    let pending = spawn_confirm(&f, run, command);
    wait_for_blocked_confirmation(&migrator, blocker).await;
    ordinary.commit().await.unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(MigrationError::WorkspaceNotEmpty)
    ));
    let mut connection = f.pool.acquire().await.unwrap();
    assert!(
        people::summary_by_id(&mut connection, f.ctx.organization_id, person)
            .await
            .unwrap()
            .is_some()
    );
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_workspace WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(bindings, 0);
}

async fn historical_proposal(f: &Fixture) -> Uuid {
    let id = Uuid::new_v4();
    // The IDs-only proposal table deliberately has no Person/contact FK. This
    // models retained Operator audit after erasure, using application grants.
    sqlx::query("INSERT INTO operator_proposal(id,organization_id,actor_user_id,turn_id,tool,person_id,contact_method_id,status,expires_at) VALUES($1,$2,$3,$4,'start_call',$5,$6,'proposed',now()+interval '5 minutes')")
        .bind(id).bind(f.org).bind(f.actor).bind(Uuid::new_v4()).bind(Uuid::new_v4()).bind(Uuid::new_v4()).execute(&f.pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_active_admission_and_claimed_proposals_block_but_expired_metadata_does_not(
    migrator: PgPool,
) {
    let f = fixture(&migrator, source_people()).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    let admission = Uuid::new_v4();
    workspace::admit_operator(
        &f.pool,
        &auth(&f, f.actor, Role::Admin),
        admission,
        Utc::now() + chrono::Duration::minutes(5),
    )
    .await
    .unwrap();
    assert!(matches!(
        confirm(&f, run, plan).await,
        Err(MigrationError::WorkspaceNotEmpty)
    ));
    // Expiration is a terminal execution deadline, not authority for more work.
    sqlx::query("UPDATE workspace_operation_admission SET created_at=now()-interval '2 hours',deadline=now()-interval '1 hour' WHERE id=$1")
        .bind(admission).execute(&migrator).await.unwrap();
    let claimed = historical_proposal(&f).await;
    assert!(matches!(
        confirm(&f, run, plan).await,
        Err(MigrationError::WorkspaceNotEmpty)
    ));
    sqlx::query("UPDATE operator_proposal SET status='claimed',expires_at=now()-interval '1 hour' WHERE id=$1")
        .bind(claimed).execute(&migrator).await.unwrap();
    assert!(matches!(
        confirm(&f, run, plan).await,
        Err(MigrationError::WorkspaceNotEmpty)
    ));
    sqlx::query(
        "UPDATE operator_proposal SET status='failed',failure_code='synthetic_settled' WHERE id=$1",
    )
    .bind(claimed)
    .execute(&f.pool)
    .await
    .unwrap();
    let expired = historical_proposal(&f).await;
    sqlx::query("UPDATE operator_proposal SET expires_at=now()-interval '1 hour' WHERE id=$1")
        .bind(expired)
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        confirm(&f, run, plan).await.unwrap()["workspace_mode"],
        "migration_review"
    );
    let preserved: i64 =
        sqlx::query_scalar("SELECT count(*) FROM operator_proposal WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(preserved, 2);
    assert_eq!(person_count(&f).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_direct_reads_commands_and_forged_tokens_respect_review(migrator: PgPool) {
    let f = fixture(&migrator, source_people()).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    confirm(&f, run, plan).await.unwrap();
    drain_import(&f).await;
    let person: Uuid = sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let person = PersonId::new(person);
    let mut connection = f.pool.acquire().await.unwrap();
    let error = people::summary_by_id(&mut connection, f.ctx.organization_id, person)
        .await
        .expect_err("unscoped query cannot read review data");
    assert!(workspace::is_review_error(&error));
    // Even a forged cached admin role does not replace current membership.
    let error = workspace::with_reader(
        &auth(&f, f.member, Role::Admin),
        people::summary_by_id(&mut connection, f.ctx.organization_id, person),
    )
    .await
    .expect_err("member cannot read review data");
    assert!(workspace::is_review_error(&error));
    let admin = workspace::with_reader(
        &auth(&f, f.actor, Role::Admin),
        people::summary_by_id(&mut connection, f.ctx.organization_id, person),
    )
    .await
    .unwrap();
    assert!(admin.is_some());
    drop(connection);
    for origin in [Origin::WebSession, Origin::Migration] {
        let ctx = CommandContext {
            origin,
            ..f.ctx.clone()
        };
        let result = change_person_stage(
            &f.pool,
            &Publisher::recording(),
            &ctx,
            ChangePersonStage {
                person_id: person,
                stage_id: StageId::new(f.lead_stage),
            },
        )
        .await;
        assert!(
            matches!(result, Err(CommandError::Database(ref error)) if workspace::is_review_error(error))
        );
    }
    let mut forged = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.import_token',$1,true)")
        .bind(Uuid::new_v4().to_string())
        .execute(&mut *forged)
        .await
        .unwrap();
    let error = people::update_stage(
        &mut forged,
        person,
        f.ctx.organization_id,
        StageId::new(f.lead_stage),
    )
    .await
    .unwrap_err();
    assert!(workspace::is_review_error(&error));
    forged.rollback().await.unwrap();
    let error = sqlx::query("UPDATE organization SET workspace_mode='operational',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org).execute(&f.pool).await.unwrap_err();
    assert!(workspace::is_review_error(&error));
    assert!(
        sqlx::query("DELETE FROM migration_workspace WHERE organization_id=$1")
            .bind(f.org)
            .execute(&f.pool)
            .await
            .is_err()
    );
    let mode: String = sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(mode, "migration_review");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_expired_lease_revalidates_executor_and_explicit_retry_adopts_current_admin(
    migrator: PgPool,
) {
    let f = fixture(&migrator, source_people()).await;
    let publisher = Publisher::recording();
    change_member_role(
        &f.pool,
        AdminActor {
            actor_user_id: UserId::new(f.actor),
            origin: Origin::WebSession,
        },
        ChangeMemberRole {
            organization_id: f.ctx.organization_id,
            user_id: UserId::new(f.member),
            role: Role::Admin,
        },
    )
    .await
    .unwrap();
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    confirm(&f, run, plan).await.unwrap();
    let calls = f.reader.calls();
    sqlx::query("UPDATE migration_import SET state='running',lease_token=$2,lease_expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(run).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
    set_member_status(
        &f.pool,
        &publisher,
        AdminActor {
            actor_user_id: UserId::new(f.member),
            origin: Origin::WebSession,
        },
        SetMemberStatus {
            organization_id: f.ctx.organization_id,
            user_id: UserId::new(f.actor),
            status: MembershipStatus::Inactive,
        },
    )
    .await
    .unwrap();
    assert!(import_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let paused: String = sqlx::query_scalar("SELECT state FROM migration_import WHERE id=$1")
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(paused, "paused");
    assert_eq!(person_count(&f).await, 0);
    assert!(matches!(
        imports::action(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            imports::ImportRequest {
                request_id: Uuid::new_v4()
            },
            true,
            &f.policy,
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    let next = CommandContext {
        actor_user_id: UserId::new(f.member),
        ..f.ctx.clone()
    };
    imports::action(
        &f.pool,
        &f.key,
        &next,
        run,
        imports::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        true,
        &f.policy,
    )
    .await
    .unwrap();
    drain_import(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &next, run, &f.policy)
        .await
        .unwrap();
    assert_eq!(detail["state"], "completed");
    assert_eq!(detail["executor_user_id"], json!(f.member));
    let actor: Uuid = sqlx::query_scalar("SELECT on_behalf_of_user_id FROM person_imported WHERE import_id=$1 AND organization_id=$2")
        .bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(actor, f.member);
    assert_eq!(f.reader.calls(), calls);
}

async fn baseline(f: &Fixture) -> (i64, i64) {
    let row = sqlx::query("SELECT s.retained_bytes,l.retained_bytes AS org_retained FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2")
        .bind(f.snapshot).bind(f.org).fetch_one(&f.pool).await.unwrap();
    (row.get("retained_bytes"), row.get("org_retained"))
}

async fn assert_inventory(f: &Fixture, run: Uuid, before: (i64, i64)) {
    // Independent SQL inventory of every counted persisted import column.
    // Closed enums, UUIDs, timestamps, counters, and native Person/contact bytes
    // are excluded. Prior plan revisions remain retained and included.
    let exact: i64 = sqlx::query_scalar(r#"SELECT COALESCE(sum(amount),0)::bigint FROM (
      SELECT octet_length(checkpoint_source_id)+octet_length(checkpoint_stage_id) AS amount FROM migration_import WHERE id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(patch_nonce)+octet_length(patch_ciphertext)+COALESCE(octet_length(confirmation_digest),0)+octet_length(checkpoint_source_id) FROM migration_import_plan WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key) FROM migration_import_choice WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(nonce)+octet_length(ciphertext)+octet_length(source_id)+octet_length(representation)+octet_length(semantic_hmac) FROM migration_import_source WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key)+COALESCE(octet_length(label_hmac),0) FROM migration_import_mapping WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(nonce)+octet_length(ciphertext)+octet_length(source_id) FROM migration_import_manifest WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(source_id) FROM migration_import_identity WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(provenance_nonce)+octet_length(provenance_ciphertext)+octet_length(source_id) FROM migration_import_result WHERE import_id=$1 AND organization_id=$2
      UNION ALL SELECT octet_length(nonce)+octet_length(ciphertext)+octet_length(input_digest) FROM migration_import_receipt WHERE import_id=$1 AND organization_id=$2
    ) inventory"#).bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let stored = sqlx::query("SELECT i.state,i.cancel_reservation_token,i.retained_bytes,i.reserved_bytes,s.retained_bytes AS snapshot_retained,s.reserved_bytes AS snapshot_reserved,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id JOIN migration_snapshot_storage l ON l.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
        .bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(stored.get::<i64, _>("retained_bytes"), exact);
    assert_eq!(stored.get::<i64, _>("snapshot_retained") - before.0, exact);
    assert_eq!(stored.get::<i64, _>("org_retained") - before.1, exact);
    let terminal = matches!(
        stored.get::<String, _>("state").as_str(),
        "completed" | "cancelled" | "expired"
    );
    let cancellation = if terminal {
        0
    } else {
        imports::CANCEL_RESERVATION
    };
    assert_eq!(
        stored
            .get::<Option<Uuid>, _>("cancel_reservation_token")
            .is_some(),
        !terminal
    );
    for field in ["reserved_bytes", "snapshot_reserved", "org_reserved"] {
        assert_eq!(stored.get::<i64, _>(field), cancellation);
    }
    let reservations = sqlx::query("SELECT count(*) FILTER(WHERE purpose='work') AS work,count(*) FILTER(WHERE purpose='cancel') AS cancel,COALESCE(sum(byte_count),0)::bigint AS bytes FROM migration_import_reservation WHERE import_id=$1 AND organization_id=$2")
        .bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(reservations.get::<i64, _>("work"), 0);
    assert_eq!(reservations.get::<i64, _>("cancel"), i64::from(!terminal));
    assert_eq!(reservations.get::<i64, _>("bytes"), cancellation);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_exact_inventory_matches_revisions_execution_and_receipt_replay(
    migrator: PgPool,
) {
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Native bytes excluded","stage":"Synthetic created stage","assignedUserId":3,"emails":[{"value":"accounting@synthetic.test"}],"sourceUrl":"https://synthetic.invalid/retained"}),
        json!({"id":102,"firstName":"Held","stage":"Synthetic created stage","isTrash":true}),
    ]));
    book.set_records(
        Stream::Stages,
        vec![json!({"id":4,"name":"Synthetic created stage"})],
    );
    let f = fixture_with_book(&migrator, book).await;
    let before = baseline(&f).await;
    let (run, plan) = prepare(&f, StageChoice::Create).await;
    assert_inventory(&f, run, before).await;
    let command = confirmation(&f, run, plan).await;
    let replay = serde_json::to_value(&command).unwrap();
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        command,
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    assert_inventory(&f, run, before).await;
    let retained = baseline(&f).await;
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        serde_json::from_value(replay).unwrap(),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(baseline(&f).await, retained);
    assert_inventory(&f, run, before).await;
    assert_eq!(person_count(&f).await, 1);
    let created: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stage WHERE organization_id=$1 AND name='Synthetic created stage'",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(created, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_large_item_admission_pauses_before_writes_and_requires_explicit_retry(
    migrator: PgPool,
) {
    let mut people = source_people();
    people[0]["sourceUrl"] = json!(format!(
        "https://synthetic.invalid/{}",
        "x".repeat(2 * 1024 * 1024 + 1)
    ));
    let f = fixture(&migrator, people).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    confirm(&f, run, plan).await.unwrap();
    let bound: i64 = sqlx::query_scalar("SELECT added_byte_bound FROM migration_import_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id='101'")
        .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert!(bound > 2 * 1024 * 1024 && bound <= imports::UNIT);
    let capacity = sqlx::query("SELECT s.retained_bytes+s.reserved_bytes AS run_total,l.retained_bytes+l.reserved_bytes AS org_total FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1")
        .bind(f.snapshot).fetch_one(&f.pool).await.unwrap();
    // Stored allowances are monotonic. A deployment ceiling can reduce current
    // effective capacity without rewriting their original approved floor.
    let insufficient = SnapshotPolicy {
        run_ceiling_bytes: capacity.get::<i64, _>("run_total") + bound - 1,
        org_ceiling_bytes: capacity.get::<i64, _>("org_total") + bound - 1,
    };
    let unchanged = baseline(&f).await;
    drain_under_policy(&f, &insufficient).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &insufficient)
        .await
        .unwrap();
    assert_eq!(detail["state"], "paused");
    assert_eq!(detail["pause_reason"], "storage_limit");
    assert_eq!(person_count(&f).await, 0);
    assert_eq!(baseline(&f).await, unchanged);
    let state = sqlx::query("SELECT checkpoint_source_id,reserved_bytes,(SELECT count(*) FROM migration_import_identity WHERE import_id=$1) AS identities,(SELECT count(*) FROM migration_import_result WHERE import_id=$1) AS results FROM migration_import WHERE id=$1")
        .bind(run).fetch_one(&f.pool).await.unwrap();
    assert_eq!(state.get::<String, _>("checkpoint_source_id"), "");
    assert_eq!(
        state.get::<i64, _>("reserved_bytes"),
        imports::CANCEL_RESERVATION
    );
    for field in ["identities", "results"] {
        assert_eq!(state.get::<i64, _>(field), 0);
    }
    let work: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_import_reservation WHERE import_id=$1 AND purpose='work'",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(work, 0);
    assert!(
        !import_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap(),
        "a budget increase cannot resume a paused import"
    );
    imports::action(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        imports::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        true,
        &f.policy,
    )
    .await
    .unwrap();
    drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(person_count(&f).await, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_cancel_fences_persisted_lease_and_preserves_committed_subset(
    migrator: PgPool,
) {
    let mut people = source_people();
    let mut second = people[0].clone();
    second["id"] = json!(102);
    people.push(second);
    let f = fixture(&migrator, people).await;
    let before = baseline(&f).await;
    let (run, plan) = prepare(
        &f,
        StageChoice::Existing {
            stage_id: f.lead_stage,
        },
    )
    .await;
    confirm(&f, run, plan).await.unwrap();
    for _ in 0..10 {
        if person_count(&f).await == 1 {
            break;
        }
        assert!(import_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap());
    }
    assert_eq!(person_count(&f).await, 1);
    let stale_token = Uuid::new_v4();
    sqlx::query("UPDATE migration_import SET lease_token=$2,lease_expires_at=now()+interval '1 minute' WHERE id=$1")
        .bind(run).bind(stale_token).execute(&migrator).await.unwrap();
    let capacity = sqlx::query("SELECT s.retained_bytes+s.reserved_bytes AS run_total,l.retained_bytes+l.reserved_bytes AS org_total FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1")
        .bind(f.snapshot).fetch_one(&f.pool).await.unwrap();
    let exhausted = SnapshotPolicy {
        run_ceiling_bytes: capacity.get("run_total"),
        org_ceiling_bytes: capacity.get("org_total"),
    };
    // Current effective allowance is exactly retained plus already reserved
    // control bytes. Cancellation must not require additional capacity.
    imports::action(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        imports::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        false,
        &exhausted,
    )
    .await
    .unwrap();
    assert!(!import_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let row = sqlx::query(
        "SELECT state,lease_token,checkpoint_source_id FROM migration_import WHERE id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "cancelled");
    assert!(row.get::<Option<Uuid>, _>("lease_token").is_none());
    assert_eq!(row.get::<String, _>("checkpoint_source_id"), "101");
    assert_eq!(person_count(&f).await, 1);
    for table in [
        "migration_import_identity",
        "migration_import_result",
        "person_imported",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE import_id=$1 AND organization_id=$2"
        ))
        .bind(run)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }
    let mode: String = sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(mode, "migration_review");
    assert_inventory(&f, run, before).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_current_deployment_ceilings_clamp_stored_allowances_and_require_retry(
    migrator: PgPool,
) {
    for ceiling in ["run", "organization"] {
        let f = fixture(&migrator, source_people()).await;
        let (run, plan) = prepare(
            &f,
            StageChoice::Existing {
                stage_id: f.lead_stage,
            },
        )
        .await;
        confirm(&f, run, plan).await.unwrap();
        let capacity = sqlx::query("SELECT s.retained_bytes+s.reserved_bytes AS run_total,l.retained_bytes+l.reserved_bytes AS org_total,s.run_byte_limit,l.byte_limit FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1")
            .bind(f.snapshot).fetch_one(&f.pool).await.unwrap();
        let mut lowered = f.policy.clone();
        if ceiling == "run" {
            lowered.run_ceiling_bytes = capacity.get("run_total");
            assert!(capacity.get::<i64, _>("run_byte_limit") > lowered.run_ceiling_bytes);
        } else {
            lowered.org_ceiling_bytes = capacity.get("org_total");
            assert!(capacity.get::<i64, _>("byte_limit") > lowered.org_ceiling_bytes);
        }
        let before = baseline(&f).await;
        drain_under_policy(&f, &lowered).await;
        let paused = imports::detail(&f.pool, &f.key, &f.ctx, run, &lowered)
            .await
            .unwrap();
        assert_eq!(paused["state"], "paused", "{ceiling}");
        assert_eq!(paused["pause_reason"], "storage_limit");
        assert_eq!(
            paused["policy"]["run_ceiling_bytes"],
            lowered.run_ceiling_bytes.to_string()
        );
        assert_eq!(
            paused["policy"]["org_ceiling_bytes"],
            lowered.org_ceiling_bytes.to_string()
        );
        assert_eq!(
            paused["cancellation_reserved_bytes"],
            imports::CANCEL_RESERVATION.to_string()
        );
        assert_eq!(person_count(&f).await, 0);
        assert_eq!(baseline(&f).await, before);
        let checkpoint: String =
            sqlx::query_scalar("SELECT checkpoint_source_id FROM migration_import WHERE id=$1")
                .bind(run)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert_eq!(checkpoint, "");
        for table in ["migration_import_identity", "migration_import_result"] {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {table} WHERE import_id=$1 AND organization_id=$2"
            ))
            .bind(run)
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
            assert_eq!(count, 0);
        }
        assert!(
            !import_worker::run_once(&f.pool, &f.key, &f.policy)
                .await
                .unwrap(),
            "restored deployment ceilings cannot implicitly resume {ceiling}"
        );
        imports::action(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            imports::ImportRequest {
                request_id: Uuid::new_v4(),
            },
            true,
            &f.policy,
        )
        .await
        .unwrap();
        drain_import(&f).await;
        let completed = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
            .await
            .unwrap();
        assert_eq!(completed["state"], "completed");
        assert_eq!(completed["reserved_bytes"], "0");
        assert_eq!(person_count(&f).await, 1);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_gate_shared_commands_and_initial_claim_bound_row_lock_waits(migrator: PgPool) {
    let f = fixture(&migrator, source_people()).await;
    let (run, _) = propose(&f).await;
    for membership in [true, false] {
        let mut blocker = migrator.begin().await.unwrap();
        if membership {
            sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR UPDATE")
                .bind(f.org).bind(f.actor).execute(&mut *blocker).await.unwrap();
        } else {
            sqlx::query("SELECT 1 FROM organization WHERE id=$1 FOR UPDATE")
                .bind(f.org)
                .execute(&mut *blocker)
                .await
                .unwrap();
        }
        let error = tokio::time::timeout(
            Duration::from_secs(4),
            imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy),
        )
        .await
        .expect("shared command row wait must be bounded")
        .unwrap_err();
        assert_lock_timeout(error);
        assert_workspace_released(&migrator, f.org).await;
        blocker.rollback().await.unwrap();
    }
    let mut blocker = migrator.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM organization WHERE id=$1 FOR UPDATE")
        .bind(f.org)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(4),
        import_worker::run_once(&f.pool, &f.key, &f.policy),
    )
    .await
    .expect("initial claim row wait must be bounded")
    .unwrap_err();
    assert_lock_timeout(error);
    assert_workspace_released(&migrator, f.org).await;
    let (lease, work): (Option<Uuid>, i64) = sqlx::query_as("SELECT lease_token,(SELECT count(*) FROM migration_import_reservation q WHERE q.import_id=i.id AND q.purpose='work') FROM migration_import i WHERE id=$1")
        .bind(run).fetch_one(&migrator).await.unwrap();
    assert!(lease.is_none());
    assert_eq!(work, 0);
    blocker.rollback().await.unwrap();
    drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
            .await
            .unwrap()["plan"]["state"],
        "ready"
    );
}
fn assert_lock_timeout(error: MigrationError) {
    let MigrationError::Database(error) = error else {
        panic!("expected closed database lock failure")
    };
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
}
async fn assert_workspace_released(migrator: &PgPool, org: Uuid) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||$1::text,0))")
                .bind(org).fetch_one(migrator).await.unwrap();
            if acquired { break; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("failed shared transaction must release its workspace lock");
}

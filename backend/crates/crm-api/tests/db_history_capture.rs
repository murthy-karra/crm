//! BOUNDARY/TRUST: real capture, terminal reconciliation, bounded reads and accounting.
use crate::{
    db_history_capture_support as h,
    import_support::{self, Fixture},
};
use crm_api::auth::workspace::ReleaseReadiness;
use crm_api::domain::migration::{
    activity, history_capture as api,
    history_capture_source::Stream,
    history_capture_worker as worker,
    imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
    metadata,
    reader::Capture,
    MigrationError,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn bytes(f: &Fixture, id: Uuid) -> i64 {
    let measured:i64=sqlx::query_scalar("SELECT (SELECT octet_length(profile_version)+octet_length(parser_version)+octet_length(schema_version)+octet_length(budget_policy_revision) FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(content_hmac)+octet_length(representation)+COALESCE(octet_length(source_version),0)) FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(semantic_hmac)+COALESCE(octet_length(identity_hmac),0)+COALESCE(octet_length(primary_person_hmac),0)) FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(identity_hmac)+COALESCE(octet_length(primary_person_hmac),0)) FROM migration_history_identity WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_person_hmac)) FROM migration_history_person_link WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(digest)) FROM migration_history_seen WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(COALESCE(octet_length(cursor_nonce),0)+COALESCE(octet_length(cursor_ciphertext),0)+COALESCE(octet_length(reported_total),0)) FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(digest)+octet_length(operation)) FROM migration_history_receipt WHERE run_id=$1 AND organization_id=$2),0)").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let state = h::ready(f, id).await;
    assert_eq!(state["retained_bytes"], measured.to_string());
    measured
}

#[sqlx::test]
#[ignore]
async fn history_proposal_rejects_real_proposed_and_queued_parent_until_completed(
    migrator: PgPool,
) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let book = h::HistoryBook::new();
    let (parent, _) = import_support::propose(&f).await;
    let (connection, revision): (Uuid, i32) =
        sqlx::query_as("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let rejected = async || {
        let snapshot = async || {
            sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('runs',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY id),'[]'::jsonb) FROM migration_history_capture_run r WHERE organization_id=$1),'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY request_id),'[]'::jsonb) FROM migration_history_receipt r WHERE organization_id=$1),'ledger',(SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1),'parent',(SELECT to_jsonb(p) FROM migration_import p WHERE id=$2 AND organization_id=$1))")
                .bind(f.org).bind(parent).fetch_one(&f.pool).await.unwrap()
        };
        let before = snapshot().await;
        let core_calls = f.reader.calls();
        let history_calls = book.count();
        let result = api::propose(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            api::ProposeFubHistoryCapture {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                connection_id: connection,
                expected_revision: revision.to_string(),
            },
        )
        .await;
        assert!(matches!(result, Err(MigrationError::SourceNotEligible)));
        assert_eq!(
            snapshot().await,
            before,
            "ineligible parent cannot create history rows, receipts or byte charges"
        );
        assert_eq!(f.reader.calls(), core_calls);
        assert_eq!(book.count(), history_calls);
    };
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "proposed"
    );
    rejected().await;
    import_support::drain_import(&f).await;
    import_support::replan(
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
            choice: AssigneeChoice::Member { user_id: f.actor },
        }],
    )
    .await;
    import_support::drain_import(&f).await;
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool, &f.key, &f.ctx, parent, serde_json::from_value(json!({
        "request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],"plan_revision":ready["plan"]["revision"],
        "confirmation_digest":ready["plan"]["confirmation_digest"],"acknowledgments":{
            "held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}
    })).unwrap(), &ReleaseReadiness::for_tests(), &f.policy).await.unwrap();
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "queued"
    );
    rejected().await;
    import_support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    let (history, state) = h::propose(&f, parent).await;
    assert_eq!(
        state["state"], "proposed",
        "same completed parent is now eligible"
    );
    assert_eq!(state["parent_import_id"], parent.to_string());
    assert_eq!(book.count(), 0);
    bytes(&f, history).await;
}

#[sqlx::test]
#[ignore]
async fn history_shared_identity_never_bypasses_a_worker_sessions_own_release_admission(
    migrator: PgPool,
) {
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=101).map(h::item).collect());
    let a = worker::WorkerSession::default();
    let b = worker::WorkerSession::default();
    let release = ReleaseReadiness::for_tests();
    // B starts idle without observed capability. Another worker's subsequent
    // identity verification is newer than B's DB-clock boundary.
    assert!(
        !worker::run_once_with_session(&f.pool, &f.key, book.as_ref(), &f.policy, None, &b)
            .await
            .unwrap()
    );
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    assert!(worker::run_once_with_session(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        Some(&release),
        &a
    )
    .await
    .unwrap());
    let before = h::ready(&f, id).await;
    assert_eq!(book.count(), 1);
    assert!(
        !worker::run_once_with_session(&f.pool, &f.key, book.as_ref(), &f.policy, None, &b)
            .await
            .unwrap()
    );
    let paused = h::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "release_not_ready");
    assert_eq!(paused["capture_sequence"], before["capture_sequence"]);
    assert_eq!(paused["retained_bytes"], before["retained_bytes"]);
    assert_eq!(paused["streams"], before["streams"]);
    assert_eq!(
        book.count(),
        1,
        "new worker must prove its own capability before any collection I/O"
    );
    api::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::action(&paused),
        Some(&release),
    )
    .await
    .unwrap();
    assert!(worker::run_once_with_session(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        Some(&release),
        &b
    )
    .await
    .unwrap());
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    // Both sessions are admitted now. Ordinary page ownership can alternate
    // without a renewed report; an identity/recovery boundary still requires it.
    for index in 0..10 {
        let session = [&a, &b][index % 2];
        if !worker::run_once_with_session(&f.pool, &f.key, book.as_ref(), &f.policy, None, session)
            .await
            .unwrap()
        {
            break;
        }
    }
    assert_eq!(h::ready(&f, id).await["state"], "completed_with_gaps");
    assert_eq!(
        book.count(),
        6,
        "two identities, two event pages and two empty family pages"
    );
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn history_alternating_worker_sessions_advance_and_restart_revalidates_once(
    migrator: PgPool,
) {
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=401).map(h::item).collect());
    book.set_records(Stream::Calls, (1..=101).map(h::item).collect());
    book.set_records(Stream::TextMessages, (1..=101).map(h::item).collect());
    let a = worker::WorkerSession::default();
    let b = worker::WorkerSession::default();
    let release = ReleaseReadiness::for_tests();
    // Independent compatible workers start before a job is admitted. Initializing
    // their DB-clock boundaries performs no source I/O or migration mutation.
    for session in [&a, &b] {
        assert!(!worker::run_once_with_session(
            &f.pool,
            &f.key,
            book.as_ref(),
            &f.policy,
            Some(&release),
            session
        )
        .await
        .unwrap());
    }
    assert_eq!(book.count(), 0);
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    assert!(worker::run_once_with_session(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        Some(&release),
        &a
    )
    .await
    .unwrap());
    let verified = async || {
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>("SELECT identity_verified_at FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap().unwrap()
    };
    let first_identity = verified().await;
    for session in [&b, &a] {
        assert!(worker::run_once_with_session(
            &f.pool,
            &f.key,
            book.as_ref(),
            &f.policy,
            Some(&release),
            session
        )
        .await
        .unwrap());
    }
    assert_eq!(h::ready(&f, id).await["streams"][0]["unique_ids"], "200");
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "ordinary ownership handoff must not ping-pong identity checks"
    );
    assert_eq!(
        verified().await,
        first_identity,
        "collection pages cannot renew identity verification"
    );

    // A new worker process starts after those committed pages. It must validate
    // identity once while keeping the collection checkpoint and retained rows.
    let restarted = worker::WorkerSession::default();
    let before_restart = h::ready(&f, id).await;
    assert!(worker::run_once_with_session(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.policy,
        Some(&release),
        &restarted
    )
    .await
    .unwrap());
    assert!(verified().await > first_identity);
    assert_eq!(h::ready(&f, id).await["streams"], before_restart["streams"]);
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    let second_identity = verified().await;
    for index in 0..30 {
        let session = [&a, &b, &restarted][index % 3];
        if !worker::run_once_with_session(
            &f.pool,
            &f.key,
            book.as_ref(),
            &f.policy,
            Some(&release),
            session,
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    let done = h::ready(&f, id).await;
    assert_eq!(done["state"], "completed_with_gaps");
    assert_eq!(done["streams"][0]["unique_ids"], "401");
    assert_eq!(done["streams"][1]["unique_ids"], "101");
    assert_eq!(done["streams"][2]["unique_ids"], "101");
    assert_eq!(done["reserved_bytes"], "0");
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    assert_eq!(
        book.count(),
        11,
        "nine advancing collection pages plus exactly two identity checks"
    );
    assert_eq!(verified().await, second_identity);
    let observations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(observations, 603);
    bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn history_and_older_children_cancel_only_their_own_shared_ledger_reservations(
    migrator: PgPool,
) {
    let (f, parent, book) = h::fixture(&migrator).await;
    let metadata = metadata::propose(
        &f.pool,
        &f.key,
        &f.ctx,
        metadata::PlanMetadataImport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
        },
        &f.policy,
    )
    .await
    .unwrap();
    let metadata_id = Uuid::parse_str(metadata["import"]["id"].as_str().unwrap()).unwrap();
    let activity = activity::prepare(
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
    let activity_id = Uuid::parse_str(activity["import"]["id"].as_str().unwrap()).unwrap();
    let older = async || {
        sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('metadata',(SELECT to_jsonb(r) FROM migration_metadata_import r WHERE id=$1 AND organization_id=$3),'activity',(SELECT to_jsonb(r) FROM migration_activity_import r WHERE id=$2 AND organization_id=$3),'metadata_reservations',(SELECT jsonb_agg(to_jsonb(r) ORDER BY token) FROM migration_metadata_reservation r WHERE organization_id=$3),'activity_reservations',(SELECT jsonb_agg(to_jsonb(r) ORDER BY token) FROM migration_activity_reservation r WHERE organization_id=$3))")
            .bind(metadata_id).bind(activity_id).bind(f.org).fetch_one(&f.pool).await.unwrap()
    };
    let baseline = older().await;
    let (first, _) = h::propose(&f, parent).await;
    h::confirm(&f, first).await;
    api::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        first,
        h::action(&h::ready(&f, first).await),
    )
    .await
    .unwrap();
    assert_eq!(
        older().await,
        baseline,
        "history cancellation preserves older child rows and owned reservations exactly"
    );
    assert_eq!(h::ready(&f, first).await["reserved_bytes"], "0");

    let (second, _) = h::propose(&f, parent).await;
    let retained_history = async || {
        sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('run',(SELECT to_jsonb(r) FROM migration_history_capture_run r WHERE id=$1 AND organization_id=$2),'reservations',(SELECT jsonb_agg(to_jsonb(r) ORDER BY token) FROM migration_history_reservation r WHERE run_id=$1 AND organization_id=$2))")
            .bind(second).bind(f.org).fetch_one(&f.pool).await.unwrap()
    };
    let history_before = retained_history().await;
    metadata::action(
        &f.pool,
        &f.key,
        &f.ctx,
        metadata_id,
        metadata::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        false,
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(retained_history().await, history_before);
    activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        activity_id,
        activity::ActivityAction {
            request_id: Uuid::new_v4(),
            expected_revision: activity["import"]["revision"].as_str().unwrap().to_owned(),
        },
        false,
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(
        retained_history().await,
        history_before,
        "older child cancellation preserves history run and control reserve exactly"
    );
    let totals: (i64, i64) = sqlx::query_as("SELECT reserved_bytes,(COALESCE((SELECT sum(byte_count) FROM migration_snapshot_reservation WHERE organization_id=$1),0)+COALESCE((SELECT sum(byte_count) FROM migration_import_reservation WHERE organization_id=$1),0)+COALESCE((SELECT sum(byte_count) FROM migration_metadata_reservation WHERE organization_id=$1),0)+COALESCE((SELECT sum(byte_count) FROM migration_activity_reservation WHERE organization_id=$1),0)+COALESCE((SELECT sum(byte_count) FROM migration_history_reservation WHERE organization_id=$1),0))::bigint FROM migration_snapshot_storage WHERE organization_id=$1")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(totals, (8192, 8192));
    api::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        second,
        h::action(&h::ready(&f, second).await),
    )
    .await
    .unwrap();
    let reserved: i64 = sqlx::query_scalar(
        "SELECT reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(reserved, 0);
    assert_eq!(book.count(), 0);
    assert_eq!(
        book.identity_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    bytes(&f, first).await;
    bytes(&f, second).await;
}
#[sqlx::test]
#[ignore]
async fn history_budget_admission_and_revision_races_fail_closed_then_cancel_below_ceiling(
    migrator: PgPool,
) {
    let (mut f, parent, book) = h::fixture(&migrator).await;
    f.policy.run_ceiling_bytes = 16 * 1024 * 1024;
    let (id, initial) = h::propose(&f, parent).await;
    assert!(matches!(
        api::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            h::confirmation(&initial),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::StorageLimit)
    ));
    assert_eq!(h::ready(&f, id).await["state"], "proposed");
    assert_eq!(book.count(), 0);
    f.policy.run_ceiling_bytes = 32 * 1024 * 1024;
    let state = h::ready(&f, id).await;
    let command = json!({
        "request_id":Uuid::new_v4(),
        "expected_run_revision":state["revision"],
        "expected_run_budget_revision":state["run_budget_revision"],
        "expected_org_budget_revision":state["org_budget_revision"],
        "expected_policy_revision":f.policy.revision(),
        "run_byte_limit":f.policy.run_ceiling_bytes.to_string(),
        "org_byte_limit":state["org_byte_limit"]
    });
    let mut competitor = command.clone();
    competitor["request_id"] = json!(Uuid::new_v4());
    let (a, b) = tokio::join!(
        api::increase_budget(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(command.clone()).unwrap()
        ),
        api::increase_budget(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(competitor.clone()).unwrap()
        )
    );
    let (winning_command, receipt) = match (a, b) {
        (Ok(receipt), Err(MigrationError::Conflict)) => (command, receipt),
        (Err(MigrationError::Conflict), Ok(receipt)) => (competitor, receipt),
        _ => panic!("exactly one budget update must win the reviewed revision"),
    };
    let retained = bytes(&f, id).await;
    assert_eq!(
        api::increase_budget(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(winning_command.clone()).unwrap()
        )
        .await
        .unwrap(),
        receipt
    );
    assert_eq!(
        bytes(&f, id).await,
        retained,
        "lost-response replay does not re-charge receipt or policy bytes"
    );
    let mut changed_body = winning_command;
    changed_body["run_byte_limit"] = json!((f.policy.run_ceiling_bytes - 1).to_string());
    assert!(matches!(
        api::increase_budget(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(changed_body).unwrap()
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let before = h::ready(&f, id).await;
    h::confirm(&f, id).await;
    assert_eq!(h::ready(&f, id).await["state"], "queued");
    f.policy.run_ceiling_bytes = 1;
    f.policy.org_ceiling_bytes = 1;
    assert!(!h::one(&f, &book).await);
    let paused = h::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "storage_limit");
    assert_eq!(paused["reserved_bytes"], "8192");
    assert_eq!(paused["org_budget_revision"], before["org_budget_revision"]);
    assert_eq!(book.count(), 0);
    api::cancel(&f.pool, &f.key, &f.policy, &f.ctx, id, h::action(&paused))
        .await
        .unwrap();
    assert_eq!(h::ready(&f, id).await["reserved_bytes"], "0");
    bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn history_capture_exact_evidence_paging_linkage_and_shared_bytes(migrator: PgPool) {
    let (f, parent, book) = h::fixture(&migrator).await;
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let mut events = (1..=125).map(h::item).collect::<Vec<_>>();
    events[1]["personId"] = json!(103);
    events[2]["personId"] = json!(999);
    events[3]["personId"] = json!(0);
    book.set_records(Stream::Events, events);
    book.set_records(Stream::Calls, vec![h::item(1001)]);
    book.set_records(Stream::TextMessages, vec![h::item(2001)]);
    let (id, _) = h::propose(&f, parent).await;
    assert_eq!(book.count(), 0);
    bytes(&f, id).await;
    h::confirm(&f, id).await;
    h::drain(&f, &book).await;
    let state = h::ready(&f, id).await;
    assert_eq!(state["state"], "completed_with_gaps");
    assert_eq!(state["reserved_bytes"], "0");
    assert_eq!(state["streams"][0]["unique_ids"], "125");
    assert_eq!(state["streams"][0]["linked"], "122");
    assert_eq!(state["streams"][0]["parent_excluded"], "1");
    assert_eq!(state["streams"][0]["no_parent_identity"], "1");
    assert_eq!(state["streams"][0]["invalid_person_reference"], "1");
    assert!(state["streams"][0]["api_inaccessible_count"].is_null());
    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = api::records(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            api::HistoryPage {
                limit: Some(17),
                cursor,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let body = serde_json::to_string(&page).unwrap();
        assert!(!body.contains("SYNTHETIC_PRIVATE_HISTORY_SENTINEL"));
        assert!(body.len() <= 512 * 1024);
        for row in page["records"].as_array().unwrap() {
            assert!(serde_json::to_vec(row).unwrap().len() <= 8192);
            ids.push(row["id"].as_str().unwrap().to_owned())
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 127);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 127);
    let actual = bytes(&f, id).await;
    let ledger: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(ledger, before["retained_bytes"].as_i64().unwrap() + actual);
    let encrypted:Vec<u8>=sqlx::query_scalar("SELECT ciphertext FROM migration_history_capture WHERE run_id=$1 AND classification='advancing' ORDER BY sequence LIMIT 1").bind(id).fetch_one(&f.pool).await.unwrap();
    assert!(!encrypted.windows(20).any(|w| w == b"SYNTHETIC_PRIVATE_HI"));
    assert!(book.requests.lock().unwrap().iter().all(|p| p == "identity"
        || p.starts_with("events?limit=100&offset=")
        || p.starts_with("calls?limit=100&offset=")
        || p.starts_with("textMessages?limit=100&offset=")));
}
#[sqlx::test]
#[ignore]
async fn history_capture_overlapping_terminal_pages_are_retained_never_exhausted_or_refetched(
    migrator: PgPool,
) {
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=200).map(h::item).collect());
    let mut second = (100..=199).map(h::item).collect::<Vec<_>>();
    second[0]["unknownChangedField"] = json!("retained variant");
    second[0]["personId"] = json!(102);
    book.set_raw(Stream::Events,100,Ok(Capture{status:200,body:serde_json::to_vec(&json!({"_metadata":{"collection":"events","limit":100,"offset":100,"total":200},"events":second})).unwrap(),truncated:false,source_version:None}));
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    assert!(h::one(&f, &book).await);
    assert!(h::one(&f, &book).await);
    let first = api::records(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        api::HistoryPage {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    assert!(h::one(&f, &book).await);
    let paused = h::ready(&f, id).await;
    assert_eq!(paused["pause_reason"], "enumeration_identity_uncertain");
    assert_eq!(paused["streams"][0]["occurrences"], "200");
    assert_eq!(paused["streams"][0]["unique_ids"], "199");
    assert_eq!(paused["streams"][0]["conflicting_variants"], "1");
    assert_eq!(paused["streams"][0]["conflicting_reference"], "1");
    let old = api::records(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        api::HistoryPage {
            cursor: Some(cursor),
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let last = old["records"].as_array().unwrap().last().unwrap();
    assert_eq!(last["source_id"], "100");
    assert_eq!(last["variant_count"], "1");
    assert_eq!(last["disposition"], "linked");
    assert!(old["next_cursor"].is_null());
    let observation = Uuid::parse_str(last["id"].as_str().unwrap()).unwrap();
    let variants = api::records(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        api::HistoryPage {
            record_id: Some(observation),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(variants["records"].as_array().unwrap().len(), 2);
    assert!(variants["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["disposition"] == "conflicting_reference" && r["variant_count"] == "2"));
    let calls = book.count();
    api::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::action(&paused),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    h::drain(&f, &book).await;
    assert_eq!(
        book.count(),
        calls + 1,
        "resume validates identity but cannot refetch retained terminal candidate"
    );
    assert_eq!(
        h::ready(&f, id).await["pause_reason"],
        "enumeration_identity_uncertain"
    );
    bytes(&f, id).await;
}
#[sqlx::test]
#[ignore]
async fn history_capture_invalid_ids_do_not_fill_terminal_identity_total_and_cancellation_works_at_ceiling(
    migrator: PgPool,
) {
    let (mut f, parent, book) = h::fixture(&migrator).await;
    book.set_records(
        Stream::Events,
        vec![
            h::item(1),
            json!({"id":0,"personId":101}),
            json!({"personId":101}),
        ],
    );
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    h::drain(&f, &book).await;
    let v = h::ready(&f, id).await;
    assert_eq!(v["state"], "paused");
    assert_eq!(v["streams"][0]["invalid_occurrences"], "2");
    assert_eq!(v["streams"][0]["unique_ids"], "1");
    let page = api::records(&f.pool, &f.key, &f.ctx, id, api::HistoryPage::default())
        .await
        .unwrap();
    assert_eq!(page["records"].as_array().unwrap().len(), 3);
    let invalid = Uuid::parse_str(page["records"][1]["id"].as_str().unwrap()).unwrap();
    let one = api::records(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        api::HistoryPage {
            record_id: Some(invalid),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(one["records"].as_array().unwrap().len(), 1);
    f.policy.run_ceiling_bytes = 1;
    f.policy.org_ceiling_bytes = 1;
    assert!(matches!(
        api::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            h::action(&v),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::StorageLimit)
    ));
    let cmd = h::action(&v);
    let value = serde_json::to_value(&cmd).unwrap();
    let cancelled = api::cancel(&f.pool, &f.key, &f.policy, &f.ctx, id, cmd)
        .await
        .unwrap();
    let measured = bytes(&f, id).await;
    let replay = api::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        serde_json::from_value(value).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(cancelled, replay);
    assert_eq!(measured, bytes(&f, id).await);
    assert_eq!(h::ready(&f, id).await["reserved_bytes"], "0");
}

#[sqlx::test]
#[ignore]
async fn history_capture_oversize_failure_is_encrypted_accounted_and_logs_have_safe_positive_control(
    migrator: PgPool,
) {
    use crm_api::domain::migration::{crypto, reader::ReaderError};
    use crm_api::ids::OrganizationId;
    use sqlx::Row;
    use std::sync::{Arc, Mutex};
    use tracing::instrument::WithSubscriber;
    #[derive(Clone, Default)]
    struct Logs(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Logs {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logs {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }
    let (f, parent, book) = h::fixture(&migrator).await;
    let sentinel = "HISTORY_RAW_FAILURE_SENTINEL";
    let mut raw = vec![b'x'; 4 * 1024 * 1024];
    raw[..sentinel.len()].copy_from_slice(sentinel.as_bytes());
    book.set_raw(
        Stream::Events,
        0,
        Err(ReaderError::ResponseTooLarge.with_capture(Capture {
            status: 200,
            body: raw.clone(),
            truncated: true,
            source_version: None,
        })),
    );
    let logs = Logs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .without_time()
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let id = async {
        let (id, _) = h::propose(&f, parent).await;
        h::confirm(&f, id).await;
        h::drain(&f, &book).await;
        id
    }
    .with_subscriber(subscriber)
    .await;
    let report = h::ready(&f, id).await;
    assert_eq!(report["pause_reason"], "response_too_large");
    assert_eq!(report["streams"][0]["occurrences"], "0");
    let r = sqlx::query(
        "SELECT * FROM migration_history_capture WHERE run_id=$1 AND classification='diagnostic'",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(r.get::<bool, _>("truncated"));
    assert_eq!(r.get::<i64, _>("raw_byte_len"), 4 * 1024 * 1024);
    let opened = crypto::open_history(
        &f.key,
        OrganizationId::new(f.org),
        id,
        r.get("id"),
        "capture",
        r.get("nonce"),
        r.get("ciphertext"),
    )
    .unwrap();
    assert_eq!(opened, raw);
    assert!(crypto::open_history(
        &f.key,
        OrganizationId::new(Uuid::new_v4()),
        id,
        r.get("id"),
        "capture",
        r.get("nonce"),
        r.get("ciphertext")
    )
    .is_err());
    assert!(crypto::open_history(
        &f.key,
        OrganizationId::new(f.org),
        id,
        r.get("id"),
        "projection",
        r.get("nonce"),
        r.get("ciphertext")
    )
    .is_err());
    bytes(&f, id).await;
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("history capture settled"));
    assert!(text.contains(&id.to_string()));
    assert!(text.contains("response_too_large"));
    assert!(!text.contains(sentinel));
}

#[sqlx::test]
#[ignore]
async fn history_expired_lease_reclaims_only_owned_reservation_and_bad_checkpoint_pauses_without_source(
    migrator: PgPool,
) {
    use std::sync::atomic::Ordering;
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=150).map(h::item).collect());
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    h::one(&f, &book).await;
    book.block.store(true, Ordering::SeqCst);
    let worker = h::one(&f, &book);
    let expire = async {
        book.entered.notified().await;
        sqlx::query("UPDATE migration_history_capture_run SET lease_expires_at=now()-interval '1 second' WHERE id=$1").bind(id).execute(&migrator).await.unwrap();
        book.continue_io.notify_one();
    };
    let (result, ()) = tokio::join!(worker, expire);
    assert!(result);
    assert_eq!(
        h::ready(&f, id).await["capture_sequence"],
        "1",
        "expired response cannot commit"
    );
    assert_eq!(
        h::ready(&f, id).await["reserved_bytes"],
        (16 * 1024 * 1024 + 8192).to_string()
    );
    h::one(&f, &book).await;
    assert_eq!(book.identity_calls.load(Ordering::SeqCst), 2);
    assert_eq!(h::ready(&f, id).await["reserved_bytes"], "8192");
    h::one(&f, &book).await;
    let calls = book.count();
    sqlx::query("UPDATE migration_history_stream SET cursor_ciphertext=set_byte(cursor_ciphertext,0,get_byte(cursor_ciphertext,0)#1) WHERE run_id=$1 AND family='events'").bind(id).execute(&migrator).await.unwrap();
    assert!(!h::one(&f, &book).await);
    assert_eq!(book.count(), calls);
    let paused = h::ready(&f, id).await;
    assert_eq!(paused["pause_reason"], "retained_integrity_failed");
    assert_eq!(paused["reserved_bytes"], "8192");
    assert!(!h::one(&f, &book).await);
    bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn history_retry_cycle_exhausts_requires_explicit_resume_and_preserves_checkpoint(
    migrator: PgPool,
) {
    use crm_api::domain::migration::reader::ReaderError;
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_raw(Stream::Events, 0, Err(ReaderError::Unavailable));
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    h::one(&f, &book).await;
    for attempt in 1..=3 {
        assert!(h::one(&f, &book).await);
        let state = h::ready(&f, id).await;
        assert_eq!(state["streams"][0]["attempts"], attempt.to_string());
        if attempt < 3 {
            assert_eq!(state["state"], "waiting_retry");
            assert!(!h::one(&f, &book).await);
            sqlx::query(
                "UPDATE migration_history_capture_run SET next_attempt_at=now() WHERE id=$1",
            )
            .bind(id)
            .execute(&migrator)
            .await
            .unwrap();
        } else {
            assert_eq!(state["state"], "paused")
        }
    }
    let calls = book.count();
    book.set_records(Stream::Events, vec![h::item(1)]);
    assert!(!h::one(&f, &book).await);
    assert_eq!(book.count(), calls);
    let paused = h::ready(&f, id).await;
    api::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::action(&paused),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    h::drain(&f, &book).await;
    assert_eq!(h::ready(&f, id).await["state"], "completed_with_gaps");
    bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn history_duplicate_decoded_identity_keys_are_diagnostic_not_authority(migrator: PgPool) {
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, vec![h::item(1)]);
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    *book.identity_raw.write().unwrap() =
        Some(br#"{"account":{"id":999},"\u0061ccount":{"id":17},"user":{"id":3}}"#.to_vec());
    assert!(h::one(&f, &book).await);
    let report = h::ready(&f, id).await;
    assert_eq!(report["pause_reason"], "duplicate_json_key");
    assert_eq!(book.requests.lock().unwrap().as_slice(), ["identity"]);
    assert_eq!(report["streams"][0]["occurrences"], "0");
    let diagnostics:i64=sqlx::query_scalar("SELECT count(*) FROM migration_history_capture WHERE run_id=$1 AND classification='diagnostic'").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(diagnostics, 1);
    bytes(&f, id).await;
    *book.identity_raw.write().unwrap() = None;
    api::retry(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::action(&report),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    h::drain(&f, &book).await;
    assert_eq!(h::ready(&f, id).await["state"], "completed_with_gaps");
}

#[sqlx::test]
#[ignore]
async fn history_erased_parent_person_is_unavailable_and_never_resurrected(migrator: PgPool) {
    let (f, parent, book) = h::fixture(&migrator).await;
    book.set_records(Stream::Events, vec![h::item(1)]);
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    h::drain(&f, &book).await;
    let first = api::records(&f.pool, &f.key, &f.ctx, id, api::HistoryPage::default())
        .await
        .unwrap();
    assert_eq!(first["records"][0]["reference_available"], true);
    let person = Uuid::parse_str(first["records"][0]["person_id"].as_str().unwrap()).unwrap();
    sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
        .bind(person)
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    let erased = api::records(&f.pool, &f.key, &f.ctx, id, api::HistoryPage::default())
        .await
        .unwrap();
    assert_eq!(erased["records"][0]["reference_available"], false);
    assert!(erased["records"][0]["person_id"].is_null());
    let (next, _) = h::propose(&f, parent).await;
    h::confirm(&f, next).await;
    h::drain(&f, &book).await;
    let report = h::ready(&f, next).await;
    assert_eq!(report["streams"][0]["linked"], "0");
    assert_eq!(report["streams"][0]["parent_excluded"], "1");
    let remains: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE id=$1")
        .bind(person)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(remains, 0);
    bytes(&f, next).await;
}

#[sqlx::test]
#[ignore]
async fn history_corrupt_credential_with_valid_identity_pauses_without_hot_loop_or_source(
    migrator: PgPool,
) {
    let (f, parent, book) = h::fixture(&migrator).await;
    let (id, _) = h::propose(&f, parent).await;
    h::confirm(&f, id).await;
    let before = bytes(&f, id).await;
    sqlx::query("UPDATE migration_connection SET credential_ciphertext=set_byte(credential_ciphertext,0,get_byte(credential_ciphertext,0)#1) WHERE organization_id=$1").bind(f.org).execute(&migrator).await.unwrap();
    assert!(!h::one(&f, &book).await);
    assert_eq!(book.count(), 0);
    let paused = h::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "retained_integrity_failed");
    assert_eq!(paused["reserved_bytes"], "8192");
    assert_eq!(before, bytes(&f, id).await);
    assert!(!h::one(&f, &book).await);
    assert_eq!(book.count(), 0);
}

//! TRUST/CONTRACT/BOUNDARY: real completed People parents and synthetic source
//! requests exercise authority, replay, admission and late-response fencing.
use std::{sync::atomic::Ordering, time::Duration};

use axum::http::StatusCode;
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::{
        envelope::CommandContext,
        migration::{
            commands, history_capture as h, history_capture_source::Stream, history_capture_worker,
            snapshot, MigrationError,
        },
    },
    ids::{OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    common::{body_json, get_with_cookie, post_json_with_cookie},
    db_activity_source as parent_source, db_history_capture_support as hs,
    import_support::Fixture,
};

const ROOT: &str = "/api/migrations/fub/history-captures";

async fn connection(f: &Fixture) -> (Uuid, i32) {
    let row = sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    (row.get("id"), row.get("revision"))
}

async fn other_admin(migrator: &PgPool, f: &Fixture) -> CommandContext {
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(migrator)
    .await
    .unwrap();
    let mut ctx = f.ctx.clone();
    ctx.actor_user_id = UserId::new(f.member);
    ctx
}

async fn native_rows(f: &Fixture) -> Value {
    let mut rows = serde_json::Map::new();
    // All columns, including Person activity maxima and timestamps, are compared.
    for table in [
        "person",
        "contact_method",
        "inquiry",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "call",
        "call_completed",
        "contact_attempted",
        "correspondence_captured",
        "note",
        "task",
    ] {
        let value: Value = sqlx::query_scalar(&format!(
            "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.id),'[]'::jsonb) FROM \"{table}\" t WHERE organization_id=$1"
        )).bind(f.org).fetch_one(&f.pool).await.unwrap();
        rows.insert(table.into(), value);
    }
    Value::Object(rows)
}

async fn captures(f: &Fixture, id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_capture WHERE organization_id=$1 AND run_id=$2",
    )
    .bind(f.org)
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

async fn observations(f: &Fixture, id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM migration_history_observation WHERE organization_id=$1 AND run_id=$2",
    )
    .bind(f.org)
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore]
async fn history_actual_tenant_admin_boundaries_and_native_parent_preservation(migrator: PgPool) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let (foreign, foreign_parent, _) = hs::fixture(&migrator).await;
    let parent_before = parent_source::parent_state(&f, parent).await;
    let source_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot s WHERE id=$1 AND organization_id=$2",
    )
    .bind(f.snapshot)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let native_before = native_rows(&f).await;
    assert!(
        !native_before["person"].as_array().unwrap().is_empty(),
        "real imported Person positive control"
    );
    let core_calls = f.reader.calls();
    let (id, _) = hs::propose(&f, parent).await;
    let (foreign_id, _) = hs::propose(&foreign, foreign_parent).await;
    assert_eq!(book.count(), 0, "proposal is DB-only");
    assert_eq!(f.reader.calls(), core_calls);
    let path = format!("{ROOT}/{id}");
    let positive = get_with_cookie(&f.app, &path, &f.cookie).await;
    assert_eq!(positive.status(), StatusCode::OK);
    assert_eq!(positive.headers()["cache-control"], "no-store");
    for (cookie, status) in [
        (f.member_cookie.as_str(), StatusCode::FORBIDDEN),
        ("", StatusCode::UNAUTHORIZED),
    ] {
        assert_eq!(
            get_with_cookie(&f.app, &path, cookie).await.status(),
            status
        );
    }
    assert_eq!(
        get_with_cookie(&foreign.app, &path, &foreign.cookie)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get_with_cookie(&f.app, &format!("{ROOT}/{foreign_id}"), &f.cookie)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut member = f.ctx.clone();
    member.actor_user_id = UserId::new(f.member);
    assert!(matches!(
        h::detail(&f.pool, &f.policy, &member, id).await,
        Err(MigrationError::Forbidden)
    ));
    let mut forged = f.ctx.clone();
    forged.organization_id = OrganizationId::new(foreign.org);
    assert!(matches!(
        h::detail(&f.pool, &f.policy, &forged, foreign_id).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        h::detail(&f.pool, &f.policy, &foreign.ctx, id).await,
        Err(MigrationError::NotFound)
    ));
    let (connection, revision) = connection(&f).await;
    let foreign_proposal = json!({"request_id":Uuid::new_v4(),"parent_import_id":foreign_parent,
        "connection_id":connection,"expected_revision":revision.to_string()});
    assert_eq!(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, foreign_proposal)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
        book.set_records(stream, vec![hs::item(1)]);
    }
    hs::confirm(&f, id).await;
    assert_eq!(
        book.count(),
        0,
        "HTTP/typed confirmation must not call source"
    );
    hs::drain(&f, &book).await;
    assert_eq!(hs::ready(&f, id).await["state"], "completed_with_gaps");
    assert_eq!(observations(&f, id).await, 3);
    let page = h::records(&f.pool, &f.key, &f.ctx, id, h::HistoryPage::default())
        .await
        .unwrap();
    assert_eq!(page["records"].as_array().unwrap().len(), 3);
    assert!(!serde_json::to_string(&page)
        .unwrap()
        .contains("SYNTHETIC_PRIVATE_HISTORY_SENTINEL"));
    let row = page["records"][0]["id"].as_str().unwrap();
    assert_eq!(
        get_with_cookie(
            &foreign.app,
            &format!("{path}/records/{row}"),
            &foreign.cookie
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(native_rows(&f).await, native_before);
    assert_eq!(parent_source::parent_state(&f, parent).await, parent_before);
    let source_after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot s WHERE id=$1 AND organization_id=$2",
    )
    .bind(f.snapshot)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(source_after, source_before);
    assert_eq!(
        f.reader.calls(),
        core_calls,
        "history must not re-read core streams"
    );
    for cookie in [&f.cookie, &f.member_cookie] {
        assert_eq!(
            get_with_cookie(&f.app, "/api/today", cookie).await.status(),
            StatusCode::CONFLICT
        );
    }
}

#[sqlx::test]
#[ignore]
async fn history_lost_response_receipts_bind_actor_exact_body_operation_and_current_authority(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let (id, proposed) = hs::propose(&f, parent).await;
    let path = format!("{ROOT}/{id}");
    let reviewed = serde_json::to_value(hs::confirmation(&proposed)).unwrap();
    let first = post_json_with_cookie(
        &f.app,
        &format!("{path}/confirm"),
        &f.cookie,
        reviewed.clone(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let receipt = body_json(first).await;
    assert_eq!(receipt.as_object().unwrap().len(), 3);
    assert_eq!(receipt["capture_id"], id.to_string());
    assert_eq!(receipt["state"], "queued");
    hs::drain(&f, &book).await;
    let settled = hs::ready(&f, id).await;
    assert_eq!(settled["state"], "completed_with_gaps");
    let retained = settled["retained_bytes"].clone();
    let calls = book.count();
    let replay = post_json_with_cookie(
        &f.app,
        &format!("{path}/confirm"),
        &f.cookie,
        reviewed.clone(),
    )
    .await;
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    assert_eq!(
        body_json(replay).await,
        receipt,
        "lost response recovery returns original receipt, not a fresh action"
    );
    assert_eq!(hs::ready(&f, id).await["retained_bytes"], retained);
    assert_eq!(book.count(), calls);
    let mut changed = reviewed.clone();
    changed["acknowledgements"]["coverage_gaps"] = json!(false);
    assert_eq!(
        post_json_with_cookie(&f.app, &format!("{path}/confirm"), &f.cookie, changed)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let _admin = other_admin(&migrator, &f).await;
    assert_eq!(
        get_with_cookie(&f.app, &path, &f.member_cookie)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            &format!("{path}/confirm"),
            &f.member_cookie,
            reviewed.clone()
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let wrong_operation =
        json!({"request_id":reviewed["request_id"],"expected_run_revision":settled["revision"]});
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            &format!("{path}/cancel"),
            &f.cookie,
            wrong_operation
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    assert_eq!(
        post_json_with_cookie(&f.app, &format!("{path}/confirm"), &f.cookie, reviewed)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(book.count(), calls);
}

#[sqlx::test]
#[ignore]
async fn history_source_admission_is_symmetric_and_concurrent_with_assessment_and_core(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let (connection, revision) = connection(&f).await;
    let (id, _) = hs::propose(&f, parent).await;
    let core = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection,
            expected_revision: revision,
        },
    )
    .await
    .unwrap();
    let core = Uuid::parse_str(core["snapshot"]["id"].as_str().unwrap()).unwrap();
    hs::confirm(&f, id).await;
    let assessment = || commands::StartFubAssessment {
        request_id: Uuid::new_v4(),
        connection_id: connection,
        expected_revision: revision,
    };
    assert!(matches!(
        commands::start_fub_assessment(&f.pool, &f.key, &f.ctx, assessment()).await,
        Err(MigrationError::Conflict)
    ));
    assert!(matches!(
        snapshot::source_action(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            core,
            snapshot::SnapshotRequest {
                request_id: Uuid::new_v4()
            },
            snapshot::SourceAction::Confirm
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        hs::action(&hs::ready(&f, id).await),
    )
    .await
    .unwrap();
    let a = commands::start_fub_assessment(&f.pool, &f.key, &f.ctx, assessment())
        .await
        .unwrap()
        .value;
    let (next, proposed) = hs::propose(&f, parent).await;
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            next,
            hs::confirmation(&proposed),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    commands::cancel_fub_assessment(
        &f.pool,
        &f.key,
        &f.ctx,
        commands::CancelFubAssessment {
            assessment_id: a.id,
        },
    )
    .await
    .unwrap();
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        core,
        snapshot::SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        snapshot::SourceAction::Confirm,
    )
    .await
    .unwrap();
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            next,
            hs::confirmation(&proposed),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    snapshot::cancel(&f.pool, &f.policy, &f.ctx, core)
        .await
        .unwrap();
    let release = ReleaseReadiness::for_tests();
    let (history, assessment) = tokio::join!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            next,
            hs::confirmation(&proposed),
            Some(&release)
        ),
        commands::start_fub_assessment(&f.pool, &f.key, &f.ctx, assessment())
    );
    assert_eq!(
        usize::from(history.is_ok()) + usize::from(assessment.is_ok()),
        1
    );
    assert!(matches!(
        (&history, &assessment),
        (Ok(_), Err(MigrationError::Conflict)) | (Err(MigrationError::Conflict), Ok(_))
    ));
    let active: i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry'))+(SELECT count(*) FROM migration_snapshot WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry'))+(SELECT count(*) FROM migration_history_capture_run WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry'))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(active, 1);
    assert_eq!(
        book.count(),
        0,
        "admission commands never perform source I/O"
    );
}

#[sqlx::test]
#[ignore]
async fn history_cancellation_fences_inflight_source_and_preserves_other_run_reservations(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    book.set_records(Stream::Events, vec![hs::item(1)]);
    let (id, _) = hs::propose(&f, parent).await;
    hs::confirm(&f, id).await;
    assert!(hs::one(&f, &book).await); // identity only
    assert_eq!(captures(&f, id).await, 1);
    book.block.store(true, Ordering::SeqCst);
    let release = ReleaseReadiness::for_tests();
    let (worker, next) = tokio::join!(
        history_capture_worker::run_once(&f.pool, &f.key, book.as_ref(), &f.policy, Some(&release)),
        async {
            tokio::time::timeout(Duration::from_secs(5), book.entered.notified())
                .await
                .unwrap();
            let running = hs::ready(&f, id).await;
            assert_eq!(running["state"], "running");
            let command = serde_json::to_value(hs::action(&running)).unwrap();
            let first = tokio::time::timeout(
                Duration::from_secs(3),
                h::cancel(
                    &f.pool,
                    &f.key,
                    &f.policy,
                    &f.ctx,
                    id,
                    serde_json::from_value(command.clone()).unwrap(),
                ),
            )
            .await
            .expect("no Org transaction remains held during source I/O")
            .unwrap();
            let replay = h::cancel(
                &f.pool,
                &f.key,
                &f.policy,
                &f.ctx,
                id,
                serde_json::from_value(command).unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(first, replay);
            let (next, proposed) = hs::propose(&f, parent).await;
            assert_eq!(proposed["reserved_bytes"], "8192");
            book.continue_io.notify_one();
            next
        }
    );
    assert!(worker.unwrap());
    let cancelled = hs::ready(&f, id).await;
    assert_eq!(cancelled["state"], "cancelled");
    assert!(!cancelled["confirmed_at"].is_null());
    assert_eq!(cancelled["reserved_bytes"], "0");
    assert_eq!(
        captures(&f, id).await,
        1,
        "late collection cannot append a capture"
    );
    assert_eq!(observations(&f, id).await, 0);
    assert_eq!(hs::ready(&f, next).await["reserved_bytes"], "8192");
    assert!(matches!(
        h::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            hs::action(&cancelled),
            Some(&release)
        )
        .await,
        Err(MigrationError::Conflict)
    ));
}

#[sqlx::test]
#[ignore]
async fn history_disconnect_and_demotion_during_source_fence_commit_without_admin_takeover(
    migrator: PgPool,
) {
    for disconnect in [true, false] {
        let (f, parent, book) = hs::fixture(&migrator).await;
        let admin = other_admin(&migrator, &f).await;
        let (connection, _) = connection(&f).await;
        book.set_records(Stream::Events, vec![hs::item(1)]);
        let (id, _) = hs::propose(&f, parent).await;
        hs::confirm(&f, id).await;
        hs::one(&f, &book).await;
        book.block.store(true, Ordering::SeqCst);
        let release = ReleaseReadiness::for_tests();
        let (worker, ()) = tokio::join!(
            history_capture_worker::run_once(
                &f.pool,
                &f.key,
                book.as_ref(),
                &f.policy,
                Some(&release)
            ),
            async {
                tokio::time::timeout(Duration::from_secs(5), book.entered.notified())
                    .await
                    .unwrap();
                if disconnect {
                    tokio::time::timeout(
                        Duration::from_secs(3),
                        commands::disconnect_fub(
                            &f.pool,
                            &f.ctx,
                            commands::DisconnectFub {
                                connection_id: connection,
                            },
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                } else {
                    sqlx::query("UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2")
                        .bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
                }
                book.continue_io.notify_one();
            }
        );
        assert!(worker.unwrap());
        let paused = h::detail(&f.pool, &f.policy, &admin, id).await.unwrap();
        assert_eq!(paused["state"], "paused");
        assert_eq!(paused["reserved_bytes"], "8192");
        assert_eq!(observations(&f, id).await, 0);
        assert_eq!(captures(&f, id).await, 1);
        let count = book.count();
        assert!(!hs::one(&f, &book).await);
        assert_eq!(book.count(), count);
        assert!(matches!(
            h::retry(
                &f.pool,
                &f.key,
                &f.policy,
                &admin,
                id,
                hs::action(&paused),
                Some(&release)
            )
            .await,
            Err(MigrationError::Forbidden)
        ));
        assert_eq!(
            h::records(&f.pool, &f.key, &admin, id, h::HistoryPage::default())
                .await
                .unwrap()["records"],
            json!([])
        );
        let cancelled = h::cancel(&f.pool, &f.key, &f.policy, &admin, id, hs::action(&paused))
            .await
            .unwrap();
        assert_eq!(cancelled["state"], "cancelled");
        assert_eq!(
            book.count(),
            count,
            "retained admin access and cancel never require source authority"
        );
    }
}

#[sqlx::test]
#[ignore]
async fn history_credential_replacement_requires_new_proposal_and_live_identity_matches_acknowledged_pair(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let (connection, revision) = connection(&f).await;
    let (old, _) = hs::propose(&f, parent).await;
    hs::confirm(&f, old).await;
    book.user.store(9, Ordering::SeqCst);
    commands::replace_fub_credential(
        &f.pool,
        &f.key,
        book.as_ref(),
        &f.ctx,
        commands::ReplaceFubCredential {
            request_id: Uuid::new_v4(),
            connection_id: connection,
            expected_revision: revision,
            api_key: "synthetic replacement history key".into(),
        },
    )
    .await
    .unwrap();
    let paused = hs::ready(&f, old).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "connection_changed");
    let calls = book.count();
    assert!(matches!(
        h::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            old,
            hs::action(&paused),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    assert_eq!(book.count(), calls);
    let (fresh, proposed) = hs::propose(&f, parent).await;
    assert_eq!(proposed["source_user_id"], "9");
    assert_eq!(proposed["source_user_difference"], true);
    let mut wrong = hs::confirmation(&proposed);
    wrong.acknowledgements.source_user_difference = false;
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            fresh,
            wrong,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    hs::confirm(&f, fresh).await;
    // Credential revision did not change, but a new live identity must still
    // equal the pair acknowledged before confirmation, not become a new baseline.
    book.user.store(10, Ordering::SeqCst);
    assert!(hs::one(&f, &book).await);
    let rejected = hs::ready(&f, fresh).await;
    assert_eq!(rejected["state"], "paused");
    assert_eq!(rejected["pause_reason"], "source_identity_mismatch");
    assert_eq!(observations(&f, fresh).await, 0);
    assert!(book
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request == "identity"));
    assert!(matches!(
        h::retry(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            fresh,
            hs::action(&rejected),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
}

#[sqlx::test]
#[ignore]
async fn history_changed_parent_binding_and_live_source_account_pause_before_collection_work(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    let (id, proposed) = hs::propose(&f, parent).await;
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            hs::confirmation(&proposed),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    assert_eq!(book.count(), 0);
    let (next, _) = hs::propose(&f, parent).await;
    hs::confirm(&f, next).await;
    book.account.store(99, Ordering::SeqCst);
    hs::one(&f, &book).await;
    let paused = hs::ready(&f, next).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "source_identity_mismatch");
    assert_eq!(observations(&f, next).await, 0);
    assert_eq!(book.requests.lock().unwrap().as_slice(), ["identity"]);
}

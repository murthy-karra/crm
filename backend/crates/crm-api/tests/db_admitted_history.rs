//! Retained-only 010d3 acceptance through real admission and capture commands.
use crate::{
    db_admitted_people_refresh_execution::fixture_with_admission,
    db_history_capture_support as capture, import_support::Fixture,
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_history as h, admitted_history_worker as worker, history_capture_source::Stream,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
pub(super) async fn fixture(pool: &PgPool) -> (Fixture, Uuid, Uuid) {
    fixture_count(pool, 1).await
}
pub(super) async fn fixture_count(pool: &PgPool, events: usize) -> (Fixture, Uuid, Uuid) {
    let (f, parent, admission) = fixture_with_admission(pool, vec![json!({"id":104,"firstName":"History","lastName":"Admitted","stage":"Lead","assignedUserId":3})]).await;
    let book = capture::HistoryBook::new();
    book.set_records(Stream::Events, (0..events).map(|i|json!({"id":81+i,"personId":104,"type":"Inquiry","created":"2026-01-01T00:00:00Z","description":"H3_BODY_SENTINEL"})).collect());
    book.set_records(Stream::Calls, vec![json!({"id":82,"personId":104,"userId":3,"created":"2026-01-02T00:00:00Z","duration":1.5,"phone":"H3_PHONE_SENTINEL"})]);
    book.set_records(Stream::TextMessages, vec![json!({"id":83,"personId":104,"created":"unknown","sent":"2026-01-03T00:00:00Z","message":"H3_TEXT_SENTINEL"})]);
    let (capture_id, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture_id).await;
    capture::drain(&f, &book).await;
    assert_eq!(
        capture::ready(&f, capture_id).await["state"],
        "completed_with_gaps"
    );
    (f, admission, capture_id)
}
pub(super) async fn drain(f: &Fixture) {
    let source_reads = f.reader.calls();
    let session = worker::WorkerSession::default();
    for _ in 0..200 {
        if !worker::run_once_with_session(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
            &session,
        )
        .await
        .expect("admitted-history unit")
        {
            assert_eq!(f.reader.calls(), source_reads);
            return;
        }
    }
    panic!("worker failed to reach a resting state");
}
pub(super) async fn ready(f: &Fixture, admission: Uuid, capture: Uuid) -> Uuid {
    let v = h::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            history_capture_id: capture,
        },
    )
    .await
    .expect("qualified prepare");
    let id = uuid(&v["import"]["id"]);
    drain(f).await;
    let v = h::get(&f.pool, &f.ctx, id).await.unwrap();
    assert_eq!(v["state"], "ready", "{v}");
    assert!(
        v["current_attempt_id"].is_null(),
        "preparation creates no execution attempt"
    );
    id
}
#[sqlx::test]
#[ignore]
async fn admitted_history_three_families_and_exact_replay(pool: PgPool) {
    let (f, admission, capture) = fixture(&pool).await;
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM person p WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let root = ready(&f, admission, capture).await;
    let v = h::get(&f.pool, &f.ctx, root).await.unwrap();
    let plan = uuid(&v["latest_plan_id"]);
    let request = Uuid::new_v4();
    let command = || h::Confirm {
        request_id: request,
        plan_id: plan,
        expected_revision: v["revision"].as_str().unwrap().into(),
        acknowledge_held: true,
        acknowledge_coverage: true,
    };
    let receipt = h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        command(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        h::confirm(&f.pool, &f.key, &f.policy, &f.ctx, root, command(), None)
            .await
            .unwrap()
    );
    let mut changed = command();
    changed.acknowledge_held = false;
    assert!(matches!(
        h::confirm(&f.pool, &f.key, &f.policy, &f.ctx, root, changed, None).await,
        Err(MigrationError::ImportConflict)
    ));
    drain(&f).await;
    let done = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(done["state"], "completed", "{done}");
    for table in [
        "fub_event_record_imported",
        "fub_call_record_imported",
        "fub_text_record_imported",
    ] {
        let r = sqlx::query(&format!("SELECT * FROM {table} WHERE organization_id=$1"))
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(r.get::<Uuid, _>("admitted_root_id"), root);
        assert!(r.get::<Option<Uuid>, _>("plan_id").is_none());
        assert_eq!(r.get::<Uuid, _>("actor_user_id"), f.actor);
    }
    let after: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM person p WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(before, after, "history does not alter native People");
    let page = h::records(&f.pool, &f.key, &f.ctx, root, plan, h::Page::default())
        .await
        .unwrap();
    assert_eq!(page["manifests"].as_array().unwrap().len(), 3);
    for sentinel in ["H3_BODY_SENTINEL", "H3_PHONE_SENTINEL", "H3_TEXT_SENTINEL"] {
        assert!(!page.to_string().contains(sentinel));
    }

    use crm_api::{
        auth::AuthContext,
        domain::{admin::Role, migration::history_review as review},
        ids::{OrganizationId, PersonId, UserId},
    };
    let auth = AuthContext {
        actor_user_id: UserId::new(f.actor),
        actor_email: "review@synthetic.test".into(),
        actor_display_name: "Synthetic admin".into(),
        active_organization_id: OrganizationId::new(f.org),
        active_organization_name: "Synthetic history review".into(),
        role: Role::Admin,
    };
    let person = PersonId::new(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT person_id FROM fub_event_record_imported WHERE admitted_root_id=$1",
        )
        .bind(root)
        .fetch_one(&pool)
        .await
        .unwrap(),
    );
    for (family, dated, kind) in [
        (
            review::Family::Events,
            review::Dated::Known,
            "fub_event_record_imported",
        ),
        (
            review::Family::Calls,
            review::Dated::Known,
            "fub_call_record_imported",
        ),
        (
            review::Family::TextMessages,
            review::Dated::Unknown,
            "fub_text_record_imported",
        ),
    ] {
        let timeline = review::timeline(
            &f.pool,
            &f.key,
            &auth,
            person,
            &review::TimelineQuery {
                family: Some(family),
                dated: Some(dated),
                limit: Some(1),
                cursor: None,
            },
        )
        .await
        .expect("admitted timeline");
        assert_eq!(timeline.items.len(), 1);
        let entry = review::entry(
            &f.pool,
            &f.key,
            &auth,
            person,
            kind,
            uuid(&timeline.items[0]["id"]),
        )
        .await
        .expect("admitted provenance");
        assert_eq!(entry["provenance"]["owner_kind"], "admitted");
        assert_eq!(entry["provenance"]["admitted_root_id"], json!(root));
        for sentinel in ["H3_BODY_SENTINEL", "H3_PHONE_SENTINEL", "H3_TEXT_SENTINEL"] {
            assert!(!entry.to_string().contains(sentinel));
        }
    }
    let bounded = h::records(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        plan,
        h::Page {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let erased = uuid(&bounded["manifests"][0]["id"]);
    let revision = done["revision"].clone();
    sqlx::query("DELETE FROM migration_history_import_display WHERE id=$1 AND organization_id=$2")
        .bind(erased)
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    assert_ne!(
        h::get(&f.pool, &f.ctx, root).await.unwrap()["revision"],
        revision
    );
    assert!(h::records(
        &f.pool,
        &f.key,
        &f.ctx,
        root,
        plan,
        h::Page {
            limit: Some(1),
            cursor: bounded["next_cursor"].as_str().map(str::to_owned),
            ..Default::default()
        }
    )
    .await
    .is_err());
    assert!(sqlx::query_scalar::<_,bool>("SELECT erased_at IS NOT NULL FROM migration_history_import_identity WHERE admitted_manifest_id=$1").bind(erased).fetch_one(&pool).await.unwrap());
    assert!(sqlx::query_scalar::<_,bool>("SELECT nonce IS NULL AND ciphertext IS NULL FROM migration_admitted_history_manifest WHERE id=$1").bind(erased).fetch_one(&pool).await.unwrap());
    measured(&f, root).await;

    // A later preparation shares the registry, cannot adopt owners, and holds
    // the suppressed identity permanently while recognizing the two live facts.
    let replay_root = ready(&f, admission, capture).await;
    let replay_plan = h::get(&f.pool, &f.ctx, replay_root).await.unwrap();
    assert_eq!(replay_plan["latest_plan"]["counts"]["held"], "1");
    confirm(&f, replay_root).await;
    drain(&f).await;
    let replay_done = h::get(&f.pool, &f.ctx, replay_root).await.unwrap();
    assert_eq!(replay_done["results"]["already_present"], "2");
    assert_eq!(replay_done["results"]["held"], "1");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_history_import_identity WHERE organization_id=$1 AND admitted_root_id=$2").bind(f.org).bind(root).fetch_one(&pool).await.unwrap(),3);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_history_import_identity WHERE organization_id=$1 AND admitted_root_id=$2").bind(f.org).bind(replay_root).fetch_one(&pool).await.unwrap(),0);
    measured(&f, replay_root).await;
}

async fn confirm(f: &Fixture, root: Uuid) {
    let v = h::get(&f.pool, &f.ctx, root).await.unwrap();
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&v["latest_plan_id"]),
            expected_revision: v["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
async fn cancel(f: &Fixture, root: Uuid) {
    let v = h::get(&f.pool, &f.ctx, root).await.unwrap();
    h::action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Action {
            request_id: Uuid::new_v4(),
            expected_revision: v["revision"].as_str().unwrap().into(),
        },
        true,
        None,
    )
    .await
    .unwrap();
}
async fn measured(f: &Fixture, root: Uuid) -> i64 {
    let mut measured = 0;
    for (table,owner,bytes) in [
        ("migration_admitted_history_root","id","octet_length(result_counts::text)+COALESCE(octet_length(pause_reason),0)"),
        ("migration_admitted_history_plan","root_id","octet_length(binding_hmac)+octet_length(source_binding::text)+octet_length(coverage::text)+octet_length(counts::text)+octet_length(stream_progress::text)"),
        ("migration_admitted_history_manifest","root_id","octet_length(representation)+COALESCE(octet_length(identity_hmac),0)+octet_length(semantic_hmac)+COALESCE(octet_length(nonce),0)+COALESCE(octet_length(ciphertext),0)"),
        ("migration_admitted_history_candidate","root_id","octet_length(identity_hmac)+octet_length(semantic_hmac)"),
        ("migration_admitted_history_issue","root_id","octet_length(code)"),
        ("migration_admitted_history_receipt","root_id","octet_length(input_digest)+octet_length(nonce)+octet_length(ciphertext)"),
        ("migration_history_import_display","admitted_root_id","octet_length(nonce)+octet_length(ciphertext)"),
        ("migration_history_import_identity","admitted_root_id","octet_length(identity_hmac)+octet_length(semantic_hmac)"),
    ] {
        measured+=sqlx::query_scalar::<_,i64>(&format!("SELECT COALESCE(sum({bytes}),0)::bigint FROM {table} WHERE {owner}=$1 AND organization_id=$2")).bind(root).bind(f.org).fetch_one(&f.pool).await.unwrap();
    }
    let r = sqlx::query(
        "SELECT retained_bytes,reserved_bytes FROM migration_admitted_history_root WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        r.get::<i64, _>("retained_bytes"),
        measured,
        "independent byte inventory"
    );
    let reserved:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_admitted_history_reservation WHERE root_id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
    assert_eq!(r.get::<i64, _>("reserved_bytes"), reserved);
    measured
}
#[sqlx::test]
#[ignore]
async fn admitted_history_partial_cancel_remainder_and_full_budget_control(pool: PgPool) {
    let (f, admission, capture) = fixture_count(&pool, 75).await;
    let baseline: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let root = ready(&f, admission, capture).await;
    measured(&f, root).await;
    confirm(&f, root).await;
    assert!(worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let applied: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_admitted_history_result WHERE root_id=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(applied, 50, "one bounded commit");
    cancel(&f, root).await;
    let cancelled = h::get(&f.pool, &f.ctx, root).await.unwrap();
    let previous = uuid(&cancelled["current_attempt_id"]);
    h::create_remainder(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Remainder {
            request_id: Uuid::new_v4(),
            attempt_id: previous,
            expected_revision: cancelled["revision"].as_str().unwrap().into(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT a.applied_position FROM migration_admitted_history_attempt a JOIN migration_admitted_history_root r ON r.current_attempt_id=a.id AND r.organization_id=a.organization_id WHERE r.id=$1").bind(root).fetch_one(&pool).await.unwrap(),50,"remainder inherits the durable settled prefix");
    // Deliberate isolated quota fault. This must preserve cancellation capacity.
    sqlx::query("UPDATE migration_admitted_history_root SET run_byte_limit=retained_bytes+reserved_bytes WHERE id=$1").bind(root).execute(&pool).await.unwrap();
    drain(&f).await;
    assert_eq!(
        h::get(&f.pool, &f.ctx, root).await.unwrap()["state"],
        "paused"
    );
    cancel(&f, root).await;
    let bytes = measured(&f, root).await;
    let total: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(total - baseline, bytes);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_history_result WHERE root_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        50
    );
    let settled:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(r) ORDER BY position) FROM migration_admitted_history_result r WHERE root_id=$1").bind(root).fetch_one(&f.pool).await.unwrap();
    sqlx::query("UPDATE migration_admitted_history_root SET run_byte_limit=2147483648 WHERE id=$1")
        .bind(root)
        .execute(&pool)
        .await
        .unwrap();
    let cancelled = h::get(&f.pool, &f.ctx, root).await.unwrap();
    h::create_remainder(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Remainder {
            request_id: Uuid::new_v4(),
            attempt_id: uuid(&cancelled["current_attempt_id"]),
            expected_revision: cancelled["revision"].as_str().unwrap().into(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f).await;
    assert_eq!(
        h::get(&f.pool, &f.ctx, root).await.unwrap()["state"],
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_history_result WHERE root_id=$1"
        )
        .bind(root)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        77
    );
    let preserved:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(r) ORDER BY position) FROM migration_admitted_history_result r WHERE root_id=$1 AND position<=50").bind(root).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        settled, preserved,
        "settled units survive all remainder attempts"
    );
    measured(&f, root).await;
}

#[sqlx::test]
#[ignore]
async fn admitted_history_recovery_integrity_and_http_boundaries(pool: PgPool) {
    use crate::common::{body_json, get_with_cookie, post_json_with_cookie};
    use axum::http::StatusCode;
    let (f, admission, capture) = fixture(&pool).await;
    let uri = "/api/migrations/fub/admitted-history-imports";
    let request = Uuid::new_v4();
    let body = json!({"request_id":request,"admission_id":admission,"history_capture_id":capture});
    for (cookie, status) in [
        (f.member_cookie.as_str(), StatusCode::FORBIDDEN),
        ("", StatusCode::UNAUTHORIZED),
    ] {
        let r = post_json_with_cookie(&f.app, uri, cookie, body.clone()).await;
        assert_eq!(r.status(), status);
        assert_eq!(r.headers()["cache-control"], "no-store");
    }
    for (path, b) in [
        (format!("{uri}?unexpected=1"), body.clone()),
        (
            uri.into(),
            json!({"request_id":request,"admission_id":admission,"history_capture_id":capture,"organization_id":f.org}),
        ),
        (uri.into(), json!({"term":"HIDDEN".repeat(2000)})),
    ] {
        let r = post_json_with_cookie(&f.app, &path, &f.cookie, b).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert_eq!(body_json(r).await, json!({"error":"malformed_request"}));
    }
    let r = post_json_with_cookie(&f.app, uri, &f.cookie, body.clone()).await;
    assert_eq!(r.status(), StatusCode::CREATED);
    assert_eq!(r.headers()["cache-control"], "no-store");
    let receipt = body_json(r).await;
    let root = uuid(&receipt["import"]["id"]);
    let response = post_json_with_cookie(&f.app, uri, &f.cookie, body.clone()).await;
    assert_eq!(body_json(response).await, receipt);
    let wrong = crm_api::config::RawPayloadKey::new([0x42; 32]);
    assert!(!worker::run_once_with_session(
        &f.pool,
        &wrong,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
        &worker::WorkerSession::default()
    )
    .await
    .unwrap());
    let paused = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(paused["pause_reason"], "retained_integrity_failed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_admitted_history_manifest WHERE root_id=$1"
        )
        .bind(root)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    measured(&f, root).await;
    h::action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        root,
        h::Action {
            request_id: Uuid::new_v4(),
            expected_revision: paused["revision"].as_str().unwrap().into(),
        },
        false,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    // Fresh process recovery needs an observed compatible release; no source I/O.
    assert!(!worker::run_once_with_session(
        &f.pool,
        &f.key,
        &f.policy,
        None,
        &worker::WorkerSession::default()
    )
    .await
    .unwrap());
    let paused = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(paused["pause_reason"], "release_not_ready");
    // A different current admin must explicitly adopt paused work; the old
    // executor cannot recover it or replay its earlier receipt after revocation.
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(&pool)
    .await
    .unwrap();
    let mut adopter = f.ctx.clone();
    adopter.actor_user_id = crm_api::ids::UserId::new(f.member);
    h::action(
        &f.pool,
        &f.key,
        &f.policy,
        &adopter,
        root,
        h::Action {
            request_id: Uuid::new_v4(),
            expected_revision: paused["revision"].as_str().unwrap().into(),
        },
        false,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f).await;
    assert_eq!(
        h::get(&f.pool, &adopter, root).await.unwrap()["state"],
        "ready"
    );
    let ready = h::get(&f.pool, &adopter, root).await.unwrap();
    h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &adopter,
        root,
        h::Confirm {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&ready["latest_plan_id"]),
            expected_revision: ready["revision"].as_str().unwrap().into(),
            acknowledge_held: true,
            acknowledge_coverage: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f).await;
    assert!(sqlx::query_scalar::<_,bool>("SELECT bool_and(actor_user_id=$2) FROM fub_event_record_imported WHERE admitted_root_id=$1").bind(root).bind(f.member).fetch_one(&pool).await.unwrap());
    let r = get_with_cookie(&f.app, &format!("{uri}/{root}?unexpected=1"), &f.cookie).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(r.headers()["cache-control"], "no-store");
    // Current authority is checked before serving a known receipt.
    let r = post_json_with_cookie(&f.app, uri, &f.cookie, body).await;
    assert!(!r.status().is_success());
    assert_eq!(r.headers()["cache-control"], "no-store");
}

#[sqlx::test]
#[ignore]
async fn admitted_history_original_identity_overlap_preserves_first_owner_in_both_orders(
    pool: PgPool,
) {
    use crate::db_history_import_support as original;
    for admitted_first in [false, true] {
        let (f, admission, new_capture) = fixture(&pool).await;
        let parent: Uuid = sqlx::query_scalar(
            "SELECT parent_import_id FROM migration_people_admission WHERE id=$1",
        )
        .bind(admission)
        .fetch_one(&pool)
        .await
        .unwrap();
        let book = capture::HistoryBook::new();
        book.set_records(
            Stream::Events,
            vec![json!({"id":81,"personId":101,"type":"Inquiry","created":"2026-01-01T00:00:00Z"})],
        );
        let (old_capture, _) = capture::propose(&f, parent).await;
        capture::confirm(&f, old_capture).await;
        capture::drain(&f, &book).await;
        let old = original::ready(&f, parent, old_capture).await;
        let new = ready(&f, admission, new_capture).await;
        // Both immutable previews precede the competing identity insert.
        original::confirm(&f, old).await;
        confirm(&f, new).await;
        if admitted_first {
            drain(&f).await;
            original::drain(&f).await;
        } else {
            original::drain(&f).await;
            drain(&f).await;
        }
        let facts=sqlx::query("SELECT admitted_root_id,attempt_id,identity_id FROM fub_event_record_imported WHERE organization_id=$1").bind(f.org).fetch_all(&pool).await.unwrap();
        assert_eq!(
            facts.len(),
            1,
            "global source identity never duplicates across owners"
        );
        assert_eq!(
            facts[0].get::<Option<Uuid>, _>("admitted_root_id"),
            admitted_first.then_some(new)
        );
        assert_eq!(
            facts[0].get::<Option<Uuid>, _>("attempt_id"),
            (!admitted_first).then_some(old)
        );
        let owner=sqlx::query("SELECT admitted_root_id,owner_run_id FROM migration_history_import_identity WHERE id=$1").bind(facts[0].get::<Uuid,_>("identity_id")).fetch_one(&pool).await.unwrap();
        assert_eq!(
            owner.get::<Option<Uuid>, _>("admitted_root_id"),
            admitted_first.then_some(new)
        );
        assert_eq!(
            owner.get::<Option<Uuid>, _>("owner_run_id"),
            (!admitted_first).then_some(old)
        );
        if admitted_first {
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_history_import_result WHERE owner_run_id=$1 AND disposition='held'").bind(old).fetch_one(&pool).await.unwrap(),1);
        } else {
            assert_eq!(
                h::get(&f.pool, &f.ctx, new).await.unwrap()["results"]["held"],
                "1"
            );
        }
        measured(&f, new).await;
    }
}

#[sqlx::test]
#[ignore]
async fn admitted_history_cancelled_cohort_excludes_sibling_and_unsettled_people(pool: PgPool) {
    use crate::db_admitted_people_refresh_execution as admission_fixture;
    use crm_api::domain::migration::{people_admission, people_admission_worker};
    let (f, parent, _sibling) = fixture_with_admission(
        &pool,
        vec![json!({"id":106,"firstName":"Sibling","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let partial = admission_fixture::ready_admission(
        &f,
        parent,
        vec![
            json!({"id":104,"firstName":"First","stage":"Lead","assignedUserId":3}),
            json!({"id":105,"firstName":"Second","stage":"Lead","assignedUserId":3}),
        ],
    )
    .await;
    admission_fixture::confirm_admission(&f, partial).await;
    assert!(people_admission_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND disposition='settled'").bind(partial).fetch_one(&pool).await.unwrap(),1);
    let revision: i64 =
        sqlx::query_scalar("SELECT lifecycle_revision FROM migration_people_admission WHERE id=$1")
            .bind(partial)
            .fetch_one(&pool)
            .await
            .unwrap();
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        partial,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: revision,
        },
    )
    .await
    .unwrap();
    let book = capture::HistoryBook::new();
    book.set_records(
        Stream::Events,
        vec![
            json!({"id":900,"personId":104}),
            json!({"id":901,"personId":105}),
            json!({"id":902,"personId":101}),
            json!({"id":903,"personId":106}),
            json!({"id":904,"personIds":[104,105]}),
        ],
    );
    let (captured, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, captured).await;
    capture::drain(&f, &book).await;
    let root = ready(&f, partial, captured).await;
    let preview = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(preview["latest_plan"]["counts"]["occurrences"], "5");
    assert_eq!(preview["latest_plan"]["counts"]["eligible"], "1");
    assert_eq!(preview["latest_plan"]["counts"]["held"], "1");
    assert_eq!(preview["latest_plan"]["counts"]["excluded"], "3");
    confirm(&f, root).await;
    drain(&f).await;
    let done = h::get(&f.pool, &f.ctx, root).await.unwrap();
    assert_eq!(done["results"]["imported"], "1");
    assert_eq!(done["results"]["held"], "1");
    assert_eq!(done["results"]["excluded"], "3");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM fub_event_record_imported WHERE admitted_root_id=$1"
        )
        .bind(root)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    measured(&f, root).await;
}

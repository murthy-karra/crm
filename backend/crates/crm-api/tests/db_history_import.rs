//! Retained-only interpretation and durable execution, using isolated SQLx databases.
//! Migrator edits below are explicitly labelled corruption/erasure controls.
use crate::{
    db_history_capture_support as capture, db_history_import_support as fns,
    import_support::Fixture,
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::migration::{
        history_capture_source::Stream, history_import as h, history_import_worker as worker,
        MigrationError,
    },
    ids::UserId,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

async fn frozen(f: &Fixture) -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    // Prepared native/source baseline: exact complete rows, not only counts.
    for table in [
        "person",
        "contact_method",
        "person_tag",
        "person_custom_field_value",
        "note",
        "task",
        "inquiry",
        "person_imported",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call_completed",
        "correspondence_captured",
        "migration_import",
        "migration_import_plan",
        "migration_history_capture_run",
        "migration_history_capture",
        "migration_history_observation",
        "migration_history_identity",
        "migration_history_stream",
    ] {
        let hash: String = sqlx::query_scalar(&format!("SELECT md5(COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text) FROM {table} r WHERE organization_id=$1")).bind(f.org).fetch_one(&f.pool).await.unwrap();
        rows.insert(table.into(), hash);
    }
    rows
}
async fn ledger(pool: &PgPool, f: &Fixture) -> i64 {
    let mut total = 0;
    for run in sqlx::query("SELECT id,retained_bytes,reserved_bytes FROM migration_history_import_run WHERE organization_id=$1").bind(f.org).fetch_all(pool).await.unwrap() {
        let id: Uuid=run.get("id");
        let mut measured: i64 = sqlx::query_scalar("SELECT octet_length(budget_policy_revision)::bigint FROM migration_history_import_run WHERE id=$1").bind(id).fetch_one(pool).await.unwrap();
        // Independent explicit byte inventory; do not call the production measurement function.
        for (table, expression) in [
            ("migration_history_import_plan", "octet_length(profile_version)+octet_length(parser_version)+octet_length(schema_version)+octet_length(interpretation_version)+octet_length(reader_version)+octet_length(binding_hmac)+octet_length(coverage::text)"),
            ("migration_history_import_stream", "octet_length(reported_total)+COALESCE(octet_length(nonce),0)+COALESCE(octet_length(ciphertext),0)"),
            ("migration_history_import_manifest", "octet_length(representation)+COALESCE(octet_length(identity_hmac),0)+octet_length(semantic_hmac)"),
            ("migration_history_import_candidate", "octet_length(identity_hmac)+octet_length(semantic_hmac)"),
            ("migration_history_import_display", "octet_length(nonce)+octet_length(ciphertext)"),
            ("migration_history_import_identity", "octet_length(identity_hmac)+octet_length(semantic_hmac)"),
            ("migration_history_import_receipt", "octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)"),
        ] {
            measured += sqlx::query_scalar::<_,i64>(&format!("SELECT COALESCE(sum({expression}),0)::bigint FROM {table} WHERE owner_run_id=$1 AND organization_id=$2")).bind(id).bind(f.org).fetch_one(pool).await.unwrap();
        }
        measured += sqlx::query_scalar::<_,i64>("SELECT COALESCE(sum(octet_length(a.interpretation_version)+octet_length(a.reader_version)),0)::bigint FROM migration_history_import_anchor a JOIN migration_history_import_plan p ON p.id=a.plan_id AND p.organization_id=a.organization_id WHERE p.owner_run_id=$1 AND a.organization_id=$2").bind(id).bind(f.org).fetch_one(pool).await.unwrap();
        assert_eq!(run.get::<i64,_>("retained_bytes"),measured,"exact retained logical-byte accounting");
        let reserved:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_history_import_reservation WHERE run_id=$1 AND organization_id=$2").bind(id).bind(f.org).fetch_one(pool).await.unwrap();
        assert_eq!(run.get::<i64,_>("reserved_bytes"),reserved);
        total+=measured;
    }
    total
}
async fn org_retained(f: &Fixture) -> i64 {
    sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}
async fn count_facts(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT (SELECT count(*) FROM fub_event_record_imported WHERE organization_id=$1)+(SELECT count(*) FROM fub_call_record_imported WHERE organization_id=$1)+(SELECT count(*) FROM fub_text_record_imported WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn cancel(f: &Fixture, id: Uuid) -> Value {
    let v = fns::detail(f, id).await;
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::CancelFubHistoryImport {
            request_id: Uuid::new_v4(),
            expected_revision: v["revision"].as_str().unwrap().into(),
        },
    )
    .await
    .unwrap()
}
async fn resume(f: &Fixture, id: Uuid) {
    let v = fns::detail(f, id).await;
    h::resume(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::ResumeFubHistoryImport {
            request_id: Uuid::new_v4(),
            expected_revision: v["revision"].as_str().unwrap().into(),
            expected_policy_revision: f.policy.revision(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
fn page(limit: u16, cursor: Option<String>) -> h::ImportPage {
    h::ImportPage {
        limit: Some(limit),
        cursor,
        ..Default::default()
    }
}

#[sqlx::test]
#[ignore]
async fn retained_three_families_preserve_native_truth_exact_bytes_and_actor_bound_receipts(
    pool: PgPool,
) {
    let (f, parent, capture, book) = fns::fixture(&pool).await;
    let baseline = frozen(&f).await;
    let bytes = org_retained(&f).await;
    let calls = (f.reader.calls(), book.count());
    let id = fns::ready(&f, parent, capture).await;
    let ready = fns::detail(&f, id).await;
    assert_eq!(ready["counts"]["occurrences"], "3");
    assert_eq!(ready["counts"]["eligible"], "3");
    assert_eq!(count_facts(&f).await, 0);
    assert_eq!(org_retained(&f).await - bytes, ledger(&pool, &f).await);
    let records = h::records(&f.pool, &f.key, &f.ctx, id, page(50, None))
        .await
        .unwrap();
    let encoded = records.to_string();
    for forbidden in [
        "IMPORT_BODY_SENTINEL",
        "IMPORT_PHONE_SENTINEL",
        "IMPORT_TEXT_SENTINEL",
    ] {
        assert!(!encoded.contains(forbidden));
    }
    let items = records["records"].as_array().unwrap();
    let text = items
        .iter()
        .find(|r| r["family"] == "text_messages")
        .unwrap();
    assert!(text["metadata"]["source_created"].is_null());
    assert_eq!(text["metadata"]["source_sent"], "2026-01-03T00:00:00Z");
    assert_eq!(text["metadata"]["source_access_user_id"], "3");
    let call = items.iter().find(|r| r["family"] == "calls").unwrap();
    assert_eq!(call["metadata"]["source_duration"], "15e-1");
    assert_eq!(call["metadata"]["source_duration_unit"], "seconds");
    // Every acknowledgement, plan revision and executor is server checked.
    let mut bad = fns::confirmation(&ready);
    bad.acknowledgements.date_uncertainty = false;
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            bad,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    assert_eq!(fns::detail(&f, id).await, ready);
    let command = serde_json::to_value(fns::confirmation(&ready)).unwrap();
    let receipt = h::confirm(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        serde_json::from_value(command.clone()).unwrap(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    fns::drain(&f).await;
    let done = fns::detail(&f, id).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["counts"]["inserted"], "3");
    assert_eq!(done["reserved_bytes"], "0");
    assert_eq!(count_facts(&f).await, 3);
    assert_eq!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(command.clone()).unwrap(),
            None
        )
        .await
        .unwrap(),
        receipt,
        "lost-response receipt survives completion and unavailable readiness"
    );
    let mut changed = command.clone();
    changed["acknowledgements"]["external_facts"] = json!(false);
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            serde_json::from_value(changed).unwrap(),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let mut member = f.ctx.clone();
    member.actor_user_id = UserId::new(f.member);
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &member,
            id,
            serde_json::from_value(command.clone()).unwrap(),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    let facts=sqlx::query("SELECT actor_kind,actor_user_id,origin,occurred_at<=recorded_at AND occurred_at>clock_timestamp()-interval '10 minutes' AS local_time,on_behalf_of_user_id,corrects_id,source_created_at,source_time_basis FROM fub_text_record_imported WHERE organization_id=$1").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(facts.get::<String, _>("actor_kind"), "user");
    assert_eq!(facts.get::<Uuid, _>("actor_user_id"), f.actor);
    assert_eq!(facts.get::<String, _>("origin"), "migration");
    assert!(facts.get::<bool, _>("local_time"));
    assert!(facts
        .get::<Option<Uuid>, _>("on_behalf_of_user_id")
        .is_none());
    assert!(facts.get::<Option<Uuid>, _>("corrects_id").is_none());
    assert_eq!(facts.get::<String, _>("source_time_basis"), "unknown");
    assert_eq!(frozen(&f).await, baseline);
    assert_eq!(org_retained(&f).await - bytes, ledger(&pool, &f).await);
    assert_eq!((f.reader.calls(), book.count()), calls);
}

#[sqlx::test]
#[ignore]
async fn cancelled_attempt_reuses_frozen_plan_and_two_workers_converge_without_duplicates(
    pool: PgPool,
) {
    let (f, parent, book) = capture::fixture(&pool).await;
    book.set_records(Stream::Events,(1..=125).map(|id|json!({"id":id,"personId":101,"created":"2020-01-01T00:00:00Z","type":"Synthetic external"})).collect());
    let (source, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, source).await;
    capture::drain(&f, &book).await;
    let calls = book.count();
    let baseline = frozen(&f).await;
    let bytes = org_retained(&f).await;
    let first = fns::ready(&f, parent, source).await;
    let plan = fns::detail(&f, first).await["plan_id"].clone();
    fns::confirm(&f, first).await;
    assert!(fns::one(&f).await);
    assert_eq!(count_facts(&f).await, 50);
    cancel(&f, first).await;
    assert_eq!(fns::detail(&f, first).await["reserved_bytes"], "0");
    let anchor: Value = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM migration_history_import_anchor a WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let plan_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM migration_history_import_plan p WHERE owner_run_id=$1",
    )
    .bind(first)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let next = fns::prepare(&f, parent, source).await;
    assert_eq!(fns::detail(&f, next).await["plan_id"], plan);
    assert_eq!(fns::detail(&f, next).await["state"], "ready");
    fns::confirm(&f, next).await;
    let a = worker::WorkerSession::default();
    let b = worker::WorkerSession::default();
    for _ in 0..20 {
        let release = ReleaseReadiness::for_tests();
        let (left, right) = tokio::join!(
            worker::run_once_with_session(&f.pool, &f.key, &f.policy, Some(&release), &a),
            worker::run_once_with_session(&f.pool, &f.key, &f.policy, Some(&release), &b)
        );
        if !left.unwrap() && !right.unwrap() {
            break;
        }
    }
    let done = fns::detail(&f, next).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["counts"]["inserted"], "75");
    assert_eq!(done["counts"]["already_imported"], "50");
    assert_eq!(count_facts(&f).await, 125);
    assert_eq!(
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(p) FROM migration_history_import_plan p WHERE owner_run_id=$1"
        )
        .bind(first)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        plan_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(a) FROM migration_history_import_anchor a WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        anchor
    );
    assert_eq!(frozen(&f).await, baseline);
    assert_eq!(org_retained(&f).await - bytes, ledger(&pool, &f).await);
    assert_eq!(book.count(), calls);
}

#[sqlx::test]
#[ignore]
async fn display_suppression_invalidates_import_cursors_and_never_resurrects_facts(pool: PgPool) {
    let (f, parent, source, book) = fns::fixture(&pool).await;
    let id = fns::ready(&f, parent, source).await;
    fns::confirm(&f, id).await;
    fns::drain(&f).await;
    let calls = book.count();
    let before = fns::detail(&f, id).await;
    let plan = Uuid::parse_str(before["plan_id"].as_str().unwrap()).unwrap();
    let records = h::records(&f.pool, &f.key, &f.ctx, id, page(1, None))
        .await
        .unwrap();
    let results = h::results(&f.pool, &f.key, &f.ctx, id, page(1, None))
        .await
        .unwrap();
    let record = Uuid::parse_str(records["records"][0]["id"].as_str().unwrap()).unwrap();
    // Narrow existing erasure boundary, executed by isolated migrator, not an import mutation.
    sqlx::query("DELETE FROM migration_history_import_display WHERE id=$1 AND organization_id=$2")
        .bind(record)
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    assert_ne!(fns::detail(&f, id).await["revision"], before["revision"]);
    assert!(matches!(
        h::records(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            page(1, Some(records["next_cursor"].as_str().unwrap().into()))
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    assert!(matches!(
        h::results(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            page(1, Some(results["next_cursor"].as_str().unwrap().into()))
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let new = h::records(&f.pool, &f.key, &f.ctx, id, page(50, None))
        .await
        .unwrap();
    assert!(new["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == record.to_string())
        .unwrap()["metadata"]
        .is_null());
    assert_eq!(
        count_facts(&f).await,
        3,
        "immutable external fact envelope remains tombstoned"
    );
    // A new same-plan attempt must hold the suppressed identity rather than recreating its display.
    let again = fns::prepare(&f, parent, source).await;
    assert_eq!(fns::detail(&f, again).await["plan_id"], plan.to_string());
    fns::confirm(&f, again).await;
    fns::drain(&f).await;
    assert_eq!(
        fns::detail(&f, again).await["counts"]["application_held"],
        "1"
    );
    assert_eq!(fns::detail(&f, again).await["counts"]["inserted"], "0");
    assert_eq!(count_facts(&f).await, 3);
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM migration_history_import_display WHERE id=$1)"
    )
    .bind(record)
    .fetch_one(&f.pool)
    .await
    .unwrap());
    ledger(&pool, &f).await;
    assert_eq!(book.count(), calls);
}

#[sqlx::test]
#[ignore]
async fn corrupt_retained_display_pauses_without_partial_facts_and_resume_adopts_current_admin(
    pool: PgPool,
) {
    let (f, parent, source, book) = fns::fixture(&pool).await;
    let id = fns::ready(&f, parent, source).await;
    let calls = book.count();
    let display=sqlx::query("SELECT id,ciphertext FROM migration_history_import_display WHERE owner_run_id=$1 ORDER BY id LIMIT 1").bind(id).fetch_one(&pool).await.unwrap();
    let display_id: Uuid = display.get("id");
    let original: Vec<u8> = display.get("ciphertext");
    // Authenticated ciphertext corruption is observable and atomically discards this unit.
    sqlx::query("UPDATE migration_history_import_display SET ciphertext=set_byte(ciphertext,0,get_byte(ciphertext,0)#1) WHERE id=$1").bind(display_id).execute(&pool).await.unwrap();
    fns::confirm(&f, id).await;
    fns::drain(&f).await;
    let paused = fns::detail(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "retained_integrity_failed");
    assert_eq!(count_facts(&f).await, 0);
    assert_eq!(paused["reserved_bytes"], "8192");
    sqlx::query("UPDATE migration_history_import_display SET ciphertext=$2 WHERE id=$1")
        .bind(display_id)
        .bind(original)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE organization_membership SET role=CASE WHEN user_id=$2 THEN 'member' ELSE 'admin' END WHERE organization_id=$1 AND user_id IN ($2,$3)").bind(f.org).bind(f.actor).bind(f.member).execute(&pool).await.unwrap();
    assert!(matches!(
        h::detail(&f.pool, &f.policy, &f.ctx, id).await,
        Err(MigrationError::Forbidden)
    ));
    let mut adopter = f.ctx.clone();
    adopter.actor_user_id = UserId::new(f.member);
    let paused = h::detail(&f.pool, &f.policy, &adopter, id).await.unwrap();
    h::resume(
        &f.pool,
        &f.key,
        &f.policy,
        &adopter,
        id,
        h::ResumeFubHistoryImport {
            request_id: Uuid::new_v4(),
            expected_revision: paused["revision"].as_str().unwrap().into(),
            expected_policy_revision: f.policy.revision(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    fns::drain(&f).await;
    assert_eq!(
        h::detail(&f.pool, &f.policy, &adopter, id).await.unwrap()["state"],
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT actor_user_id FROM fub_event_record_imported WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        f.member
    );
    assert_eq!(book.count(), calls);
    ledger(&pool, &f).await;
}

#[sqlx::test]
#[ignore]
async fn recovery_readiness_wrong_key_and_lowered_budget_pause_before_work_and_preserve_cancel(
    pool: PgPool,
) {
    let (f, parent, source, book) = fns::fixture(&pool).await;
    let (sibling, _) = capture::propose(&f, parent).await;
    let sibling_owned = async || {
        sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('run',to_jsonb(r),'reservations',(SELECT COALESCE(jsonb_agg(to_jsonb(x) ORDER BY token),'[]'::jsonb) FROM migration_history_reservation x WHERE x.run_id=r.id),'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(x) ORDER BY request_id),'[]'::jsonb) FROM migration_history_receipt x WHERE x.run_id=r.id)) FROM migration_history_capture_run r WHERE id=$1 AND organization_id=$2").bind(sibling).bind(f.org).fetch_one(&f.pool).await.unwrap()
    };
    let sibling_before = sibling_owned().await;
    let id = fns::prepare(&f, parent, source).await;
    let calls = book.count();
    let wrong = RawPayloadKey::new([0x42; 32]);
    let session = worker::WorkerSession::default();
    worker::run_once_with_session(
        &f.pool,
        &wrong,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
        &session,
    )
    .await
    .unwrap();
    assert_eq!(
        fns::detail(&f, id).await["pause_reason"],
        "retained_integrity_failed"
    );
    assert_eq!(count_facts(&f).await, 0);
    resume(&f, id).await;
    fns::drain(&f).await;
    fns::confirm(&f, id).await;
    // Simulate a dead process after durable claim/reservation, before any unit commit.
    let token = Uuid::new_v4();
    let unit = 50 * 32768 + 16384i64;
    let mut fault = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO migration_history_import_reservation(token,organization_id,run_id,kind,byte_count,expires_at) VALUES($1,$2,$3,'unit',$4,clock_timestamp()-interval '1 second')").bind(token).bind(f.org).bind(id).bind(unit).execute(&mut *fault).await.unwrap();
    sqlx::query("UPDATE migration_history_import_run SET state='running',lease_token=$2,lease_expires_at=clock_timestamp()-interval '1 second',reserved_bytes=reserved_bytes+$3 WHERE id=$1").bind(id).bind(token).bind(unit).execute(&mut *fault).await.unwrap();
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(f.org).bind(unit).execute(&mut *fault).await.unwrap();
    fault.commit().await.unwrap();
    worker::run_once_with_session(
        &f.pool,
        &f.key,
        &f.policy,
        None,
        &worker::WorkerSession::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        fns::detail(&f, id).await["pause_reason"],
        "release_not_ready"
    );
    assert_eq!(count_facts(&f).await, 0);
    resume(&f, id).await;
    let mut lowered = f.policy.clone();
    lowered.run_ceiling_bytes = 20_000;
    worker::run_once_with_session(
        &f.pool,
        &f.key,
        &lowered,
        Some(&ReleaseReadiness::for_tests()),
        &worker::WorkerSession::default(),
    )
    .await
    .unwrap();
    let held = fns::detail(&f, id).await;
    assert_eq!(held["state"], "paused");
    assert_eq!(held["pause_reason"], "storage_limit");
    assert_eq!(held["reserved_bytes"], "8192");
    let request = Uuid::new_v4();
    let cmd = json!({"request_id":request,"expected_revision":held["revision"]});
    let receipt = h::cancel(
        &f.pool,
        &f.key,
        &lowered,
        &f.ctx,
        id,
        serde_json::from_value(cmd.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        h::cancel(
            &f.pool,
            &f.key,
            &lowered,
            &f.ctx,
            id,
            serde_json::from_value(cmd).unwrap()
        )
        .await
        .unwrap(),
        receipt
    );
    assert_eq!(fns::detail(&f, id).await["reserved_bytes"], "0");
    assert_eq!(count_facts(&f).await, 0);
    ledger(&pool, &f).await;
    assert_eq!(book.count(), calls);
    assert_eq!(
        sibling_owned().await,
        sibling_before,
        "another capture's control reservation and receipt remain untouched"
    );
}

#[sqlx::test]
#[ignore]
async fn person_erased_before_preparation_creates_only_holds_and_permanent_tombstones(
    pool: PgPool,
) {
    let (f, parent, source, book) = fns::fixture(&pool).await;
    let person:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND family='people' AND source_id='101'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    // Existing retained source remains; deletion happens before any 010d2 materialization.
    sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
        .bind(person)
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    let calls = book.count();
    let id = fns::ready(&f, parent, source).await;
    let ready = fns::detail(&f, id).await;
    assert_eq!(ready["counts"]["held"], "2");
    assert_eq!(ready["counts"]["eligible"], "1");
    let rows = h::records(&f.pool, &f.key, &f.ctx, id, page(50, None))
        .await
        .unwrap();
    for r in rows["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] != "text_messages")
    {
        assert_eq!(r["reason"], "parent_erased");
        assert!(r["metadata"].is_null());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_history_import_display WHERE owner_run_id=$1"
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_history_import_identity WHERE owner_run_id=$1 AND erased_at IS NOT NULL AND fact_id IS NULL").bind(id).fetch_one(&f.pool).await.unwrap(),2);
    fns::confirm(&f, id).await;
    fns::drain(&f).await;
    assert_eq!(count_facts(&f).await, 1);
    assert_eq!(fns::detail(&f, id).await["counts"]["application_held"], "2");
    ledger(&pool, &f).await;
    assert_eq!(book.count(), calls);
}

#[sqlx::test]
#[ignore]
async fn nonenumerated_repeat_variant_and_invalid_source_capture_cannot_prepare(pool: PgPool) {
    let (f, parent, book) = capture::fixture(&pool).await;
    // Exact equal repeats, unknown-field variants and invalid IDs are faithfully
    // retained by010d1, but prevent its distinct-identity terminal reconciliation.
    book.set_records(
        Stream::Events,
        vec![
            json!({"id":1,"personId":101,"unknown":{"value":"a"}}),
            json!({"id":1,"personId":101,"unknown":{"value":"a"}}),
            json!({"id":1,"personId":101,"unknown":{"value":"b"}}),
            Value::Null,
        ],
    );
    let (source, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, source).await;
    capture::drain(&f, &book).await;
    let captured = capture::ready(&f, source).await;
    assert_eq!(captured["state"], "paused");
    assert_eq!(captured["pause_reason"], "enumeration_identity_uncertain");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_history_observation WHERE run_id=$1"
        )
        .bind(source)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        4
    );
    let before = frozen(&f).await;
    let bytes = org_retained(&f).await;
    let calls = book.count();
    let workspace: i64 =
        sqlx::query_scalar("SELECT workspace_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let result = h::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::PrepareFubHistoryImport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            capture_id: source,
            expected_capture_revision: captured["revision"].as_str().unwrap().into(),
            expected_workspace_revision: workspace.to_string(),
            expected_policy_revision: f.policy.revision(),
        },
    )
    .await;
    assert!(matches!(result, Err(MigrationError::Conflict)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_history_import_run WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(org_retained(&f).await, bytes);
    assert_eq!(frozen(&f).await, before);
    assert_eq!(book.count(), calls);
}

#[sqlx::test]
#[ignore]
async fn groups_excluded_and_unmapped_are_held_while_disconnected_retained_positive_imports(
    pool: PgPool,
) {
    let (f, parent, book) = capture::fixture(&pool).await;
    book.set_records(
        Stream::Events,
        vec![
            json!({"id":1,"personId":103}),
            json!({"id":2,"personId":999}),
            json!({"id":3,"personId":101,"participants":[{"personId":102}]}),
            json!({"id":4,"personId":101,"type":"Inquiry","created":"2020-01-01T00:00:00Z"}),
        ],
    );
    let (source, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, source).await;
    capture::drain(&f, &book).await;
    assert_eq!(
        capture::ready(&f, source).await["state"],
        "completed_with_gaps"
    );
    let connection: Uuid =
        sqlx::query_scalar("SELECT id FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    crm_api::domain::migration::commands::disconnect_fub(
        &f.pool,
        &f.ctx,
        crm_api::domain::migration::commands::DisconnectFub {
            connection_id: connection,
        },
    )
    .await
    .unwrap();
    let calls = book.count();
    let baseline = frozen(&f).await;
    let id = fns::ready(&f, parent, source).await;
    let ready = fns::detail(&f, id).await;
    assert_eq!(ready["counts"]["held"], "3");
    assert_eq!(ready["counts"]["eligible"], "1");
    let page = h::records(&f.pool, &f.key, &f.ctx, id, page(50, None))
        .await
        .unwrap();
    let reasons: std::collections::BTreeSet<_> = page["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["reason"].as_str())
        .collect();
    assert_eq!(
        reasons,
        std::collections::BTreeSet::from([
            "parent_excluded",
            "no_parent_identity",
            "ambiguous_relationship"
        ])
    );
    fns::confirm(&f, id).await;
    fns::drain(&f).await;
    assert_eq!(count_facts(&f).await, 1);
    assert_eq!(fns::detail(&f, id).await["counts"]["application_held"], "3");
    assert_eq!(frozen(&f).await, baseline);
    assert_eq!(book.count(), calls);
    ledger(&pool, &f).await;
}

#[sqlx::test]
#[ignore]
async fn all_held_ready_plan_rejects_confirmation_without_anchor_receipt_or_byte_changes(
    pool: PgPool,
) {
    let (f, parent, book) = capture::fixture(&pool).await;
    book.set_records(Stream::Events, vec![json!({"id":1,"personId":103})]);
    book.set_records(Stream::Calls, vec![json!({"id":2,"personId":999})]);
    book.set_records(
        Stream::TextMessages,
        vec![json!({"id":3,"personId":101,"participants":[{"personId":102}]})],
    );
    let (source, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, source).await;
    capture::drain(&f, &book).await;
    assert_eq!(
        capture::ready(&f, source).await["state"],
        "completed_with_gaps"
    );
    let id = fns::ready(&f, parent, source).await;
    let ready = fns::detail(&f, id).await;
    assert_eq!(ready["counts"]["occurrences"], "3");
    assert_eq!(ready["counts"]["held"], "3");
    assert_eq!(ready["counts"]["eligible"], "0");
    assert_eq!(ready["actions"]["confirm"], false);
    let audit = async || {
        sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('run',(SELECT to_jsonb(r) FROM migration_history_import_run r WHERE id=$1),'ledger',(SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$2),'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY request_id),'[]'::jsonb) FROM migration_history_import_receipt r WHERE owner_run_id=$1),'anchors',(SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY parent_import_id),'[]'::jsonb) FROM migration_history_import_anchor a WHERE organization_id=$2))").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap()
    };
    let before = audit().await;
    let native = frozen(&f).await;
    let calls = book.count();
    assert!(matches!(
        h::confirm(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            fns::confirmation(&ready),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    assert_eq!(audit().await, before);
    assert_eq!(frozen(&f).await, native);
    assert_eq!(count_facts(&f).await, 0);
    assert_eq!(book.count(), calls);
    assert_eq!(
        h::records(&f.pool, &f.key, &f.ctx, id, page(50, None))
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "all held source evidence stays reviewable"
    );
    cancel(&f, id).await;
    assert_eq!(fns::detail(&f, id).await["reserved_bytes"], "0");
    ledger(&pool, &f).await;
}

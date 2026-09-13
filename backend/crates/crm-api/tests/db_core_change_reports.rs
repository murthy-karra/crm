//! D-075 synthetic collector → 010c parent → recapture → retained report proof.
use crate::{
    common,
    import_support::{self, Book, Fixture},
};
use axum::{http::StatusCode, response::Response};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::{
        envelope::CommandContext,
        migration::{
            core_change_reports as h, core_change_worker, imports,
            snapshot::{self, SnapshotRequest, SourceAction},
            snapshot_source::Stream,
            MigrationError,
        },
    },
    ids::{OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;
const ROOT: &str = "/api/migrations/fub/core-change-reports";

fn task(completed: bool) -> Value {
    json!({"id":41,"personId":101,"name":"SYNTHETIC TASK CONTENT","isCompleted":completed,"dueDateTime":"2026-09-20T12:00:00Z"})
}
async fn fixture(pool: &PgPool) -> (Fixture, Uuid, Uuid) {
    let book = Arc::new(Book::new(import_support::default_people()));
    book.set_records(Stream::TasksOpen, vec![task(false)]);
    book.set_records(
        Stream::Notes,
        vec![json!({"id":11,"personId":101,"body":"SYNTHETIC LIST CONTENT"})],
    );
    book.set_raw(
        Stream::NoteDetail,
        0,
        200,
        serde_json::to_vec(
            &json!({"id":11,"personId":101,"body":"SYNTHETIC ENRICHED CONTENT","showContent":true}),
        )
        .unwrap(),
        false,
    );
    let f = import_support::fixture_with_book(pool, book).await;
    let parent = crate::db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::People,vec![import_support::default_people()[0].clone(),json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}],"unknownNewProperty":{"large":9007199254740993_u64}}),json!({"id":104,"firstName":"Synthetic newly observed","stage":"Lead"})]);
    f.reader.set_records(Stream::TasksOpen, vec![]);
    f.reader
        .set_records(Stream::TasksCompleted, vec![task(true)]);
    f.reader.set_raw(
        Stream::NoteDetail,
        0,
        404,
        br#"{"errorMessage":"Synthetic restricted detail"}"#.to_vec(),
        false,
    );
    let newer = recapture(&f).await;
    (f, parent, newer)
}
async fn recapture(f: &Fixture) -> Uuid {
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let v = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection.get("id"),
            expected_revision: connection.get("revision"),
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(v["snapshot"]["id"].as_str().unwrap()).unwrap();
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
    import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    assert!(matches!(
        snapshot::detail(&f.pool, &f.policy, &f.ctx, id)
            .await
            .unwrap()["snapshot"]["state"]
            .as_str(),
        Some("completed" | "completed_with_gaps")
    ));
    id
}
async fn prepare(f: &Fixture, parent: Uuid, newer: Uuid) -> Uuid {
    let v = h::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::PrepareCoreChangeReport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            newer_snapshot_id: newer,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    Uuid::parse_str(v["report_id"].as_str().unwrap()).unwrap()
}
async fn detail(f: &Fixture, id: Uuid) -> Value {
    h::detail(&f.pool, &f.key, &f.ctx, id).await.unwrap()
}
async fn one(f: &Fixture) -> bool {
    core_change_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap()
}
async fn drain(f: &Fixture) {
    for _ in 0..200 {
        if !one(f).await {
            return;
        }
    }
    panic!("bounded synthetic report did not drain")
}
async fn pages(f: &Fixture, id: Uuid) -> Vec<Value> {
    let mut cursor = None;
    let mut result = Vec::new();
    let revision = detail(f, id).await["output_revision"].clone();
    loop {
        let v = h::rows(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            h::ReportPage {
                cursor,
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(v["output_revision"], revision);
        assert!(serde_json::to_vec(&v).unwrap().len() < 256 * 1024);
        result.extend(v["rows"].as_array().unwrap().clone());
        cursor = v["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return result;
        }
    }
}
async fn checked(response: Response, status: StatusCode) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let v = common::body_json(response).await;
    assert!(!v.to_string().contains("SYNTHETIC ENRICHED CONTENT"));
    assert!(!v.to_string().contains("unknownNewProperty"));
    v
}
async fn frozen(f: &Fixture, parent: Uuid) -> Value {
    let mut hashes = BTreeMap::new();
    for table in imports::EMPTY_TABLES {
        let v:Option<String>=sqlx::query_scalar(&format!("SELECT md5(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text)) FROM {table} t WHERE organization_id=$1")).bind(f.org).fetch_one(&f.pool).await.unwrap();
        hashes.insert(*table, v);
    }
    let p: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM migration_import p WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let w:Value=sqlx::query_scalar("SELECT jsonb_build_object('mode',workspace_mode,'revision',workspace_revision) FROM organization WHERE id=$1").bind(f.org).fetch_one(&f.pool).await.unwrap();
    json!({"tables":hashes,"parent":p,"workspace":w})
}
async fn ledger(f: &Fixture, id: Uuid) -> i64 {
    let run = sqlx::query(
        "SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let mut expected:i64=sqlx::query_scalar("SELECT (octet_length(engine_version)+octet_length(state)+octet_length(phase)+COALESCE(octet_length(pause_reason),0)+octet_length(to_jsonb(baseline_uncertain)::text)+octet_length(to_jsonb(newer_uncertain)::text)+octet_length(tuple_hmac)+octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(summary_nonce),0)+COALESCE(octet_length(summary_ciphertext),0))::bigint FROM migration_core_change_report WHERE id=$1").bind(id).fetch_one(&f.pool).await.unwrap();
    for (table,expr) in [("migration_core_change_group","octet_length(family)+octet_length(source_key)+COALESCE(octet_length(source_id),0)+COALESCE(octet_length(disposition),0)+octet_length(nonce)+octet_length(ciphertext)+COALESCE(octet_length(output_nonce),0)+COALESCE(octet_length(output_ciphertext),0)"),("migration_core_change_variant","octet_length(representation)+octet_length(semantic_hmac)"),("migration_core_change_note_key","octet_length(source_id)+octet_length(request_hmac)"),("migration_core_change_receipt","octet_length(operation)+octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)")]{expected+=sqlx::query_scalar::<_,i64>(&format!("SELECT COALESCE(sum({expr}),0)::bigint FROM {table} WHERE report_id=$1 AND organization_id=$2")).bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();}
    assert_eq!(
        run.get::<i64, _>("retained_bytes"),
        expected,
        "independent exact byte inventory"
    );
    let reserved:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_core_change_reservation WHERE report_id=$1 AND organization_id=$2").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(run.get::<i64, _>("reserved_bytes"), reserved);
    expected
}
#[sqlx::test]
#[ignore]
async fn real_dual_capture_reports_seal_exact_counts_and_preserve_crm(pool: PgPool) {
    let (f, parent, newer) = fixture(&pool).await;
    let before = frozen(&f, parent).await;
    let calls = f.reader.calls();
    let start: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_snapshot WHERE id=$1")
            .bind(newer)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let request = Uuid::new_v4();
    let body = json!({"request_id":request,"parent_import_id":parent,"newer_snapshot_id":newer});
    let receipt = checked(
        common::post_json_with_cookie(&f.app, ROOT, &f.cookie, body.clone()).await,
        StatusCode::CREATED,
    )
    .await;
    let id = Uuid::parse_str(receipt["report_id"].as_str().unwrap()).unwrap();
    assert_eq!(
        receipt,
        checked(
            common::post_json_with_cookie(&f.app, ROOT, &f.cookie, body.clone()).await,
            StatusCode::CREATED
        )
        .await
    );
    let mut changed = body;
    changed["newer_snapshot_id"] = json!(f.snapshot);
    checked(
        common::post_json_with_cookie(&f.app, ROOT, &f.cookie, changed).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        prepare(&f, parent, newer).await,
        id,
        "same frozen tuple reuses existing report"
    );
    for _ in 0..100 {
        let v = detail(&f, id).await;
        if v["state"] == "completed" {
            break;
        }
        assert!(v["counts"].is_null());
        checked(
            common::get_with_cookie(&f.app, &format!("{ROOT}/{id}/rows"), &f.cookie).await,
            StatusCode::CONFLICT,
        )
        .await;
        assert!(one(&f).await);
    }
    let v = detail(&f, id).await;
    assert_eq!(v["state"], "completed", "{v}");
    assert_eq!(
        v["counts"]["families"]["people"],
        json!({"unchanged":"1","changed":"1","newly_observed":"1","not_seen_again":"1","unresolved":"0"})
    );
    assert_eq!(v["counts"]["families"]["tasks"]["changed"], "1");
    assert_eq!(v["counts"]["families"]["notes"]["unresolved"], "1");
    assert_eq!(v["counts"]["source_ids"], "8");
    let rows = pages(&f, id).await;
    assert_eq!(rows.len(), 8);
    assert_eq!(
        rows.iter()
            .find(|r| r["family"] == "people" && r["source_id"] == "102")
            .unwrap()["categories"],
        json!(["other_properties"])
    );
    assert!(
        rows.iter().find(|r| r["family"] == "notes").unwrap()["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("note_content_inaccessible"))
    );
    let first = h::rows(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        h::ReportPage {
            limit: Some(2),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let token = first["next_cursor"].as_str().unwrap().to_owned();
    assert!(matches!(
        h::rows(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            h::ReportPage {
                cursor: Some(token.clone()),
                limit: Some(2),
                family: Some("people".into()),
                ..Default::default()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    let rowid = Uuid::parse_str(rows[0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        h::row_detail(&f.pool, &f.key, &f.ctx, id, rowid)
            .await
            .unwrap()["row"],
        rows[0]
    );
    assert!(sqlx::query(
        "UPDATE migration_core_change_group SET disposition='changed' WHERE report_id=$1"
    )
    .bind(id)
    .execute(&pool)
    .await
    .is_err());
    assert_eq!(pages(&f, id).await, rows);
    let measured = ledger(&f, id).await;
    let end: i64 = sqlx::query_scalar("SELECT retained_bytes FROM migration_snapshot WHERE id=$1")
        .bind(newer)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(end - start, measured);
    assert_eq!(f.reader.calls(), calls);
    assert_eq!(frozen(&f, parent).await, before);
    checked(
        common::get_with_cookie(&f.app, &format!("{ROOT}/{id}"), &f.member_cookie).await,
        StatusCode::FORBIDDEN,
    )
    .await;
}
#[sqlx::test]
#[ignore]
async fn corruption_budget_pause_cancellation_and_principal_boundaries(pool: PgPool) {
    let (f, parent, newer) = fixture(&pool).await;
    let id = prepare(&f, parent, newer).await;
    let before = frozen(&f, parent).await;
    let other = OrganizationId::new(Uuid::new_v4());
    let denied = CommandContext {
        organization_id: other,
        ..f.ctx
    };
    assert!(matches!(
        h::detail(&f.pool, &f.key, &denied, id).await,
        Err(MigrationError::Forbidden)
    ));
    let member = CommandContext {
        actor_user_id: UserId::new(f.member),
        ..f.ctx
    };
    assert!(matches!(
        h::detail(&f.pool, &f.key, &member, id).await,
        Err(MigrationError::Forbidden)
    ));
    let cap=sqlx::query("SELECT id,ciphertext FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='people' LIMIT 1").bind(f.snapshot).fetch_one(&pool).await.unwrap();
    let original: Vec<u8> = cap.get("ciphertext");
    let mut corrupted = original.clone();
    corrupted[0] ^= 1;
    sqlx::query("UPDATE migration_snapshot_capture SET ciphertext=$2 WHERE id=$1")
        .bind(cap.get::<Uuid, _>("id"))
        .bind(corrupted)
        .execute(&pool)
        .await
        .unwrap();
    drain(&f).await;
    assert_eq!(
        detail(&f, id).await["pause_reason"],
        "retained_integrity_failed"
    );
    ledger(&f, id).await;
    sqlx::query("UPDATE migration_snapshot_capture SET ciphertext=$2 WHERE id=$1")
        .bind(cap.get::<Uuid, _>("id"))
        .bind(original)
        .execute(&pool)
        .await
        .unwrap();
    let resume = Uuid::new_v4();
    let receipt = h::resume(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::ReportRequest { request_id: resume },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(
        h::resume(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            id,
            h::ReportRequest { request_id: resume },
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap(),
        receipt
    );
    let mut policy = f.policy.clone();
    policy.run_ceiling_bytes = 1;
    assert!(core_change_worker::run_once(
        &f.pool,
        &f.key,
        &policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    assert_eq!(
        detail(&f, id).await["pause_reason"],
        "storage_budget_exhausted"
    );
    let cancel = Uuid::new_v4();
    let cancelled = h::cancel(
        &f.pool,
        &f.key,
        &policy,
        &f.ctx,
        id,
        h::ReportRequest { request_id: cancel },
    )
    .await
    .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(
        h::cancel(
            &f.pool,
            &f.key,
            &policy,
            &f.ctx,
            id,
            h::ReportRequest { request_id: cancel }
        )
        .await
        .unwrap(),
        cancelled
    );
    ledger(&f, id).await;
    assert!(matches!(
        h::rows(&f.pool, &f.key, &f.ctx, id, Default::default()).await,
        Err(MigrationError::Conflict)
    ));
    assert!(!one(&f).await);
    let fresh = prepare(&f, parent, newer).await;
    assert_ne!(fresh, id);
    drain(&f).await;
    assert_eq!(detail(&f, fresh).await["state"], "completed");
    assert_eq!(frozen(&f, parent).await, before);
}

/// Explicit retained walkthrough setup, absent from ordinary check-db builds.
/// All source responses and identities are synthetic; output has no auth token.
#[cfg(feature = "perf-harness")]
#[tokio::test]
#[ignore]
async fn retain_core_change_ui_fixture() {
    assert_eq!(
        std::env::var("CRM_CORE_CHANGE_UI_SEED").as_deref(),
        Ok("approved-synthetic")
    );
    let url = std::env::var("MIGRATION_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&url).await.unwrap();
    assert_eq!(
        pool.connect_options().get_database(),
        Some("crm_migration_010e1")
    );
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let (f, parent, newer) = fixture(&pool).await;
    let cancelled = prepare(&f, parent, newer).await;
    h::cancel(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        cancelled,
        h::ReportRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let id = prepare(&f, parent, newer).await;
    drain(&f).await;
    assert_eq!(detail(&f, id).await["state"], "completed");
    let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(&pool)
        .await
        .unwrap();
    let info = json!({"organization_id":f.org,"actor_id":f.actor,"email":email,"password":"synthetic import fixture password","baseline_snapshot_id":f.snapshot,"newer_snapshot_id":newer,"parent_import_id":parent,"report_id":id,"cancelled_report_id":cancelled,"raw_payload_key_hex":common::TEST_RAW_PAYLOAD_KEY_HEX,"counts":detail(&f,id).await["counts"]});
    let path = std::path::Path::new("/private/tmp/crm-core-change-ui.json");
    std::fs::write(path, serde_json::to_vec_pretty(&info).unwrap()).unwrap();
    println!("Retained isolated synthetic UI fixture: {}", path.display());
}

#[sqlx::test]
#[ignore]
async fn repeated_variants_partial_enumeration_and_scope_change_remain_uncertain(pool: PgPool) {
    let (f, parent, _) = fixture(&pool).await;
    let a = import_support::default_people()[0].clone();
    let mut b = a.clone();
    b["unknownProperty"] = json!([1, 2, 3]);
    f.reader.set_records(Stream::People, vec![a.clone(), a, b]);
    let newer = recapture(&f).await;
    // Controlled retained source access fixture: successful identity evidence
    // names another source user, keeping the same qualified account.
    let identity = sqlx::query(
        "SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='identity'",
    )
    .bind(newer)
    .fetch_one(&pool)
    .await
    .unwrap();
    let raw = br#"{"account":{"id":17},"user":{"id":7}}"#;
    let sealed = crm_api::domain::migration::crypto::seal_snapshot(
        &f.key,
        f.ctx.organization_id,
        newer,
        identity.get("id"),
        "capture",
        raw,
    )
    .unwrap();
    sqlx::query(
        "UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3,raw_byte_len=$4 WHERE id=$1",
    )
    .bind(identity.get::<Uuid, _>("id"))
    .bind(sealed.nonce.as_slice())
    .bind(sealed.ciphertext)
    .bind(raw.len() as i64)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE migration_snapshot_stream SET state='partial' WHERE snapshot_id=$1 AND stream='people'").bind(newer).execute(&pool).await.unwrap();
    let id = prepare(&f, parent, newer).await;
    drain(&f).await;
    let report = detail(&f, id).await;
    assert_eq!(report["inputs"]["source_scope"], "changed_identity");
    assert_eq!(report["counts"]["equal_repeats"], "1");
    assert_eq!(report["counts"]["conflicting_groups"], "1");
    let rows = pages(&f, id).await;
    for row in rows.iter().filter(|v| v["family"] == "people") {
        assert_eq!(row["disposition"], "unresolved");
        assert_eq!(row["evidence_is_exhaustive"], false);
    }
    assert_eq!(
        report["counts"]["families"]["people"]["not_seen_again"],
        "0"
    );
    ledger(&f, id).await;
    let other = common::create_org(&pool, "Synthetic other report tenant").await;
    common::add_membership_with(
        &pool,
        other,
        f.actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let ctx = CommandContext {
        organization_id: OrganizationId::new(other),
        ..f.ctx
    };
    assert!(matches!(
        h::detail(&f.pool, &f.key, &ctx, id).await,
        Err(MigrationError::NotFound)
    ));
    let same_actor_newer = prepare(&f, parent, newer).await;
    assert_eq!(same_actor_newer, id);
}

#[sqlx::test]
#[ignore]
async fn stale_worker_cannot_pause_or_publish_after_takeover_or_expiry(pool: PgPool) {
    let (f, parent, newer) = fixture(&pool).await;
    let id = prepare(&f, parent, newer).await;
    let old = core_change_worker::hold_for_test(&f.pool, &f.key, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_core_change_report SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    assert!(one(&f).await);
    let before = detail(&f, id).await;
    core_change_worker::settle_for_test(&f.pool, &f.key, &f.policy, &old)
        .await
        .unwrap();
    core_change_worker::pause_for_test(&f.pool, &old)
        .await
        .unwrap();
    assert_eq!(
        detail(&f, id).await,
        before,
        "stale worker changes neither progress, state nor bytes"
    );
    // Expire the final unit during its bounded summary write. The atomic seal
    // predicate must reject it and the entire transaction must roll back.
    for _ in 0..100 {
        let r = sqlx::query(
            "SELECT phase,groups_compared FROM migration_core_change_report WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        if r.get::<String, _>("phase") == "compare" && r.get::<i64, _>("groups_compared") > 0 {
            break;
        }
        assert!(one(&f).await);
    }
    sqlx::query("CREATE FUNCTION synthetic_expire_core_change_seal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.phase='compare' AND NEW.groups_compared>0 AND NEW.state='running' THEN NEW.lease_expires_at:=clock_timestamp()-interval '1 second'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER synthetic_expire_core_change_seal BEFORE UPDATE OF summary_nonce ON migration_core_change_report FOR EACH ROW EXECUTE FUNCTION synthetic_expire_core_change_seal()").execute(&pool).await.unwrap();
    assert!(one(&f).await);
    assert_eq!(detail(&f, id).await["state"], "paused");
    assert!(detail(&f, id).await["output_revision"].is_null());
    assert!(matches!(
        h::rows(&f.pool, &f.key, &f.ctx, id, Default::default()).await,
        Err(MigrationError::Conflict)
    ));
    sqlx::query("DROP TRIGGER synthetic_expire_core_change_seal ON migration_core_change_report")
        .execute(&pool)
        .await
        .unwrap();
    h::resume(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        id,
        h::ReportRequest {
            request_id: Uuid::new_v4(),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f).await;
    assert_eq!(detail(&f, id).await["state"], "completed");
    ledger(&f, id).await;
}

#[cfg(feature = "perf-harness")]
#[tokio::test]
#[ignore]
async fn core_change_worker_child() {
    let url =
        std::env::var("CRM_CORE_CHANGE_CHILD_DATABASE").expect("parent subprocess fixture only");
    let migrator = PgPool::connect(&url).await.unwrap();
    let pool = common::connect_as_app(&migrator).await;
    let config = common::test_config();
    assert!(core_change_worker::run_once(
        &pool,
        &config.raw_payload_key,
        &Default::default(),
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    println!(
        "Synthetic retained worker process {} committed one unit",
        std::process::id()
    );
}
#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn independent_process_report_recovery(pool: PgPool) {
    let (f, parent, newer) = fixture(&pool).await;
    let id = prepare(&f, parent, newer).await;
    let calls = f.reader.calls();
    let old = core_change_worker::hold_for_test(&f.pool, &f.key, &f.policy, &f.ctx, id)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_core_change_report SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let mut progress = 0;
    let mut output = Vec::new();
    for _ in 0..4 {
        let executable = std::env::current_exe().unwrap();
        let database = common::migrator_url_for(&pool);
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(executable)
                .args([
                    "--ignored",
                    "--exact",
                    "db_core_change_reports::core_change_worker_child",
                    "--nocapture",
                ])
                .env("CRM_CORE_CHANGE_CHILD_DATABASE", database)
                .output()
        })
        .await
        .unwrap()
        .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        output.push(String::from_utf8(result.stdout).unwrap());
        let v = detail(&f, id).await;
        let next = v["progress"]["captures_processed"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(next > progress);
        progress = next;
    }
    let before = detail(&f, id).await;
    core_change_worker::settle_for_test(&f.pool, &f.key, &f.policy, &old)
        .await
        .unwrap();
    core_change_worker::pause_for_test(&f.pool, &old)
        .await
        .unwrap();
    assert_eq!(detail(&f, id).await, before);
    drain(&f).await;
    assert_eq!(detail(&f, id).await["state"], "completed");
    assert_eq!(f.reader.calls(), calls);
    ledger(&f, id).await;
    let path = std::path::Path::new("/private/tmp/crm-core-change-process-proof.log");
    std::fs::write(path, output.join("\n")).unwrap();
    println!(
        "Four independent retained-only worker processes: {}",
        path.display()
    );
}

#[sqlx::test]
#[ignore]
async fn frozen_identity_bounds_account_denial_and_revoked_executor(pool: PgPool) {
    let (f, parent, newer) = fixture(&pool).await;
    assert!(matches!(
        h::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            h::PrepareCoreChangeReport {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                newer_snapshot_id: f.snapshot
            },
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    let row=sqlx::query("SELECT id,nonce,ciphertext,raw_byte_len FROM migration_snapshot_capture WHERE snapshot_id=$1 AND stream='identity'").bind(newer).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE migration_snapshot_capture SET raw_byte_len=17000000 WHERE id=$1")
        .bind(row.get::<Uuid, _>("id"))
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(
            h::prepare(
                &f.pool,
                &f.key,
                &f.policy,
                &f.ctx,
                h::PrepareCoreChangeReport {
                    request_id: Uuid::new_v4(),
                    parent_import_id: parent,
                    newer_snapshot_id: newer
                },
                Some(&ReleaseReadiness::for_tests())
            )
            .await,
            Err(MigrationError::SourceNotEligible)
        ),
        "metadata bounds reject before loading/decrypting oversized identity history"
    );
    let raw = br#"{"account":{"id":18},"user":{"id":3}}"#;
    let sealed = crm_api::domain::migration::crypto::seal_snapshot(
        &f.key,
        f.ctx.organization_id,
        newer,
        row.get("id"),
        "capture",
        raw,
    )
    .unwrap();
    sqlx::query(
        "UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3,raw_byte_len=$4 WHERE id=$1",
    )
    .bind(row.get::<Uuid, _>("id"))
    .bind(sealed.nonce.as_slice())
    .bind(sealed.ciphertext)
    .bind(raw.len() as i64)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        h::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            h::PrepareCoreChangeReport {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                newer_snapshot_id: newer
            },
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::SourceAccountMismatch)
    ));
    sqlx::query(
        "UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3,raw_byte_len=$4 WHERE id=$1",
    )
    .bind(row.get::<Uuid, _>("id"))
    .bind(row.get::<Vec<u8>, _>("nonce"))
    .bind(row.get::<Vec<u8>, _>("ciphertext"))
    .bind(row.get::<i64, _>("raw_byte_len"))
    .execute(&pool)
    .await
    .unwrap();
    let id = prepare(&f, parent, newer).await;
    let other = common::create_user(
        &pool,
        "other-core-report-admin@synthetic.test",
        "Synthetic other admin",
        "long synthetic password",
    )
    .await;
    common::add_membership_with(
        &pool,
        f.org,
        other,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let other_ctx = CommandContext {
        actor_user_id: UserId::new(other),
        ..f.ctx
    };
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    assert!(one(&f).await);
    let report = h::detail(&f.pool, &f.key, &other_ctx, id).await.unwrap();
    assert_eq!(report["pause_reason"], "executor_not_authorized");
    assert_eq!(report["actions"]["resume"], false);
    assert!(matches!(
        h::resume(
            &f.pool,
            &f.key,
            &f.policy,
            &other_ctx,
            id,
            h::ReportRequest {
                request_id: Uuid::new_v4()
            },
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    assert_eq!(
        h::cancel(
            &f.pool,
            &f.key,
            &f.policy,
            &other_ctx,
            id,
            h::ReportRequest {
                request_id: Uuid::new_v4()
            }
        )
        .await
        .unwrap()["state"],
        "cancelled"
    );
    ledger(&f, id).await;
}

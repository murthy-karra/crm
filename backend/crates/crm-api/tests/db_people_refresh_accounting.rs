//! Exact retained-byte audits are deliberately test-only full scans. Runtime
//! workers must charge their bounded unit deltas without scanning prior units.
use crate::{db_people_refresh_execution as execution, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{people_refresh as refresh, people_refresh_worker},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, PartialEq)]
struct Ledger {
    run: i64,
    reserved: i64,
    snapshot: i64,
    snapshot_reserved: i64,
    org: i64,
    org_reserved: i64,
}
async fn ledger(f: &Fixture, run: Uuid) -> Ledger {
    let row=sqlx::query("SELECT r.retained_bytes,r.reserved_bytes,s.retained_bytes snapshot,s.reserved_bytes snapshot_reserved,l.retained_bytes org,l.reserved_bytes org_reserved FROM migration_people_refresh r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id JOIN migration_snapshot_storage l ON l.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2").bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    Ledger {
        run: row.get("retained_bytes"),
        reserved: row.get("reserved_bytes"),
        snapshot: row.get("snapshot"),
        snapshot_reserved: row.get("snapshot_reserved"),
        org: row.get("org"),
        org_reserved: row.get("org_reserved"),
    }
}
async fn audit(f: &Fixture, run: Uuid) -> Ledger {
    let actual:i64=sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)) FROM migration_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2),0) + COALESCE((SELECT sum(octet_length(proposed_nonce)+octet_length(proposed_ciphertext)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)+octet_length(current_nonce)+octet_length(current_ciphertext)+octet_length(instructions_nonce)+octet_length(instructions_ciphertext)) FROM migration_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2),0) + COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_people_refresh_contact WHERE refresh_id=$1 AND organization_id=$2),0) + COALESCE((SELECT sum(octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM migration_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2),0) + COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_people_refresh_receipt WHERE refresh_id=$1 AND organization_id=$2),0) + COALESCE((SELECT sum(octet_length(projection_nonce)+octet_length(projection_ciphertext)) FROM migration_people_refresh_baseline WHERE refresh_id=$1 AND organization_id=$2),0)").bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let reservations:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_people_refresh_reservation WHERE refresh_id=$1 AND organization_id=$2").bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let state = ledger(f, run).await;
    assert_eq!(
        state.run, actual,
        "run ledger must match exact retained encrypted payloads"
    );
    assert_eq!(state.reserved, reservations, "owned reservation sum");
    state
}
async fn native(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM person p WHERE organization_id=$1),'contacts',(SELECT jsonb_agg(to_jsonb(c) ORDER BY id) FROM contact_method c WHERE organization_id=$1),'stages',(SELECT jsonb_agg(to_jsonb(s) ORDER BY id) FROM stage_changed s WHERE organization_id=$1),'assignments',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM assignment_changed a WHERE organization_id=$1))").bind(f.org).fetch_one(&f.pool).await.unwrap()
}
fn revised(label: &str) -> Vec<Value> {
    vec![
        json!({"id":101,"firstName":format!("{label} One")}),
        json!({"id":102,"firstName":format!("{label} Two")}),
    ]
}
async fn replay_prepare(f: &Fixture, run: Uuid) {
    let row=sqlx::query("SELECT q.request_id,r.report_id FROM migration_people_refresh_receipt q JOIN migration_people_refresh r ON r.id=q.refresh_id WHERE q.refresh_id=$1 AND q.action='prepare'").bind(run).fetch_one(&f.pool).await.unwrap();
    refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PreparePeopleRefresh {
            request_id: row.get("request_id"),
            report_id: row.get("report_id"),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
async fn cancel(f: &Fixture, run: Uuid, request: Uuid, revision: i64) -> Value {
    refresh::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::LifecyclePeopleRefresh {
            request_id: request,
            expected_lifecycle_revision: revision,
        },
    )
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn exact_ledger_replay_and_successor_baseline_accounting(migrator: PgPool) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let (run, detail) = execution::ready(&f, parent, revised("First")).await;
    let before = audit(&f, run).await;
    assert!(
        before.reserved > 0,
        "bounded cancellation reserve retained while ready"
    );
    replay_prepare(&f, run).await;
    assert_eq!(audit(&f, run).await, before);
    let command = execution::confirmation(&detail);
    let receipt = execution::confirm(&f, run, &command).await;
    let confirmed = audit(&f, run).await;
    assert!(confirmed.run > before.run, "confirmation receipt charged");
    assert_eq!(execution::confirm(&f, run, &command).await, receipt);
    assert_eq!(audit(&f, run).await, confirmed);
    execution::drain(&f, run).await;
    let completed = audit(&f, run).await;
    assert_eq!(completed.reserved, 0);
    assert_eq!(
        completed.snapshot_reserved - before.snapshot_reserved,
        -before.reserved
    );
    assert_eq!(
        completed.org_reserved - before.org_reserved,
        -before.reserved
    );
    assert_eq!(
        completed.snapshot - before.snapshot,
        completed.run - before.run
    );
    assert_eq!(completed.org - before.org, completed.run - before.run);
    let business = native(&f).await;
    assert_eq!(execution::confirm(&f, run, &command).await, receipt);
    assert_eq!(audit(&f, run).await, completed);
    assert_eq!(native(&f).await, business);
    // A later baseline head must not leave bytes charged to both owners or
    // omit the old retained provenance. Both refresh ledgers are audited.
    let (second, second_detail) = execution::ready(&f, parent, revised("Second")).await;
    execution::confirm(&f, second, &execution::confirmation(&second_detail)).await;
    execution::drain(&f, second).await;
    audit(&f, run).await;
    assert_eq!(audit(&f, second).await.reserved, 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn policy_clamp_pauses_before_write_and_cancel_uses_its_owned_reserve(migrator: PgPool) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let (run, detail) = execution::ready(&f, parent, revised("Budget")).await;
    execution::confirm(&f, run, &execution::confirmation(&detail)).await;
    let before = native(&f).await;
    let initial = audit(&f, run).await;
    let mut low = f.policy.clone();
    low.run_ceiling_bytes = 1;
    low.org_ceiling_bytes = 1;
    for _ in 0..4 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "paused" {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &low,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .expect("shortage must durably pause without losing the job"));
    }
    let paused = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    assert_eq!(paused["state"], "paused");
    assert_eq!(native(&f).await, before);
    let waiting = audit(&f, run).await;
    assert_eq!(waiting.run, initial.run);
    assert_eq!(waiting.reserved, initial.reserved);
    let result_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_people_refresh_result WHERE refresh_id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(result_count, 0);
    let request = Uuid::new_v4();
    let revision = paused["lifecycle_revision"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let receipt = cancel(&f, run, request, revision).await;
    let done = audit(&f, run).await;
    assert_eq!(done.reserved, 0);
    assert!(done.run > waiting.run, "cancellation receipt retained");
    assert_eq!(native(&f).await, before);
    assert_eq!(cancel(&f, run, request, revision).await, receipt);
    assert_eq!(audit(&f, run).await, done);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn repreview_charges_new_plan_once_and_cancel_preserves_sealed_preview(migrator: PgPool) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let (run, detail) = execution::ready(&f, parent, revised("Review")).await;
    let old = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    let before = audit(&f, run).await;
    let request = Uuid::new_v4();
    let revision = detail["plan"]["revision"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let command = || refresh::RepreviewPeopleRefresh {
        request_id: request,
        expected_plan_revision: revision,
    };
    let receipt = refresh::repreview(&f.pool, &f.key, &f.ctx, run, command())
        .await
        .unwrap();
    let after = audit(&f, run).await;
    assert!(after.run > before.run);
    assert_eq!(
        refresh::repreview(&f.pool, &f.key, &f.ctx, run, command())
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(audit(&f, run).await, after);
    for _ in 0..20 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "ready" {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    let ready = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    assert_ne!(ready["plan"]["id"], detail["plan"]["id"]);
    audit(&f, run).await;
    cancel(
        &f,
        run,
        Uuid::new_v4(),
        ready["lifecycle_revision"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    )
    .await;
    assert_eq!(audit(&f, run).await.reserved, 0);
    let old_plan = Uuid::parse_str(detail["plan"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        refresh::items(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            refresh::Page {
                plan_id: Some(old_plan),
                ..Default::default()
            }
        )
        .await
        .unwrap(),
        old
    );
}

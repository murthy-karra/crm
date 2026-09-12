//! 010f1 private permits, atomic recovery and exact logical-byte accounting.
//! Migrator-only failures/lease edits are explicitly adversarial test fixtures.
use crate::import_support::{self as support, Book, Fixture};
use crm_api::domain::{
    migration::{
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        metadata, metadata_worker,
        snapshot_source::Stream,
    },
    tag::{self, CreateTag},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

pub(super) async fn fixture(migrator: &PgPool, large: bool) -> (Fixture, Uuid, Uuid, Value) {
    fixture_book(migrator, large, false).await
}
async fn fixture_book(
    migrator: &PgPool,
    large: bool,
    batches: bool,
) -> (Fixture, Uuid, Uuid, Value) {
    let mut p = json!({"id":101,"firstName":"Atomic synthetic","stage":"Lead","assignedUserId":3,"tags":["Atomic Tag"],"customAtomic":"value"});
    if large {
        p["exactEvidence"] = json!("x".repeat(2 * 1024 * 1024 + 31));
    }
    if batches {
        p["tags"] = json!((0..121)
            .map(|n| format!("Batch Tag {n:03}"))
            .collect::<Vec<_>>());
    }
    let book = Arc::new(Book::new(vec![p]));
    book.set_records(
        Stream::CustomFields,
        vec![json!({"id":21,"name":"customAtomic","label":"Atomic Field","type":"text"})],
    );
    if batches {
        book.set_records(Stream::CustomFields,vec![json!({"id":21,"name":"customAtomic","label":"Atomic Field","type":"text"}),json!({"id":22,"name":"customOverChoice","label":"Large option evidence","type":"dropdown","choices":(0..121).map(|n|format!("Choice {n:03}")).collect::<Vec<_>>()})]);
    }
    let f = support::fixture_with_book(migrator, book).await;
    let (parent, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(
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
    support::drain_import(&f).await;
    let p = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool,&f.key,&f.ctx,parent,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":p["plan"]["id"],"plan_revision":p["plan"]["revision"],"confirmation_digest":p["plan"]["confirmation_digest"],"acknowledgments":{"held_count":p["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}})).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    support::drain_import(&f).await;
    let p = metadata::propose(
        &f.pool,
        &f.key,
        &f.ctx,
        serde_json::from_value(json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}))
            .unwrap(),
        &f.policy,
    )
    .await
    .unwrap();
    let child = Uuid::parse_str(p["import"]["id"].as_str().unwrap()).unwrap();
    drain(&f, &f.policy).await;
    let p = detail(&f, child).await;
    if batches {
        return (f, parent, child, p);
    }
    let plan = Uuid::parse_str(p["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2",
    )
    .bind(plan)
    .bind(f.org)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    metadata::replan(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":p["latest_plan"]["revision"],"mappings":ids.iter().map(|id|json!({"mapping_id":id,"choice":{"kind":"create_matching"}})).collect::<Vec<_>>()})).unwrap(),&f.policy).await.unwrap();
    drain(&f, &f.policy).await;
    let p = detail(&f, child).await;
    assert_eq!(p["latest_plan"]["state"], "ready");
    (f, parent, child, p)
}
async fn detail(f: &Fixture, id: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap()
}
async fn drain(f: &Fixture, policy: &crm_api::domain::migration::snapshot::SnapshotPolicy) {
    for _ in 0..1000 {
        if !metadata_worker::run_once(&f.pool, &f.key, policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("bounded fixture stalled")
}
async fn confirm(f: &Fixture, id: Uuid, p: &Value) {
    metadata::confirm(&f.pool,&f.key,&f.ctx,id,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":p["latest_plan"]["id"],"plan_revision":p["latest_plan"]["revision"],"confirmation_digest":p["latest_plan"]["confirmation_digest"],"workspace_revision":p["workspace_revision"],"acknowledgments":{"held_count":p["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
}
async fn retry(f: &Fixture, id: Uuid) {
    metadata::action(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        metadata::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        true,
        &f.policy,
    )
    .await
    .unwrap();
}
async fn inventory(f: &Fixture, id: Uuid) {
    let counted:i64=sqlx::query_scalar(r#"SELECT
 (SELECT COALESCE(sum(octet_length(patch_nonce)+octet_length(patch_ciphertext)+octet_length(destination_nonce)+octet_length(destination_ciphertext)+COALESCE(octet_length(confirmation_digest),0)+COALESCE(octet_length(checkpoint_key),0)),0) FROM migration_metadata_plan WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key)),0) FROM migration_metadata_choice WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(semantic_hmac)+octet_length(source_id)+octet_length(representation)),0) FROM migration_metadata_source WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key)+COALESCE(octet_length(name_hmac),0)+COALESCE(octet_length(target_label_hmac),0)),0) FROM migration_metadata_mapping WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(alias_key)),0) FROM migration_metadata_alias WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_id)),0) FROM migration_metadata_manifest WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(source_key)),0) FROM migration_metadata_operation WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(source_key)),0) FROM migration_metadata_identity WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+COALESCE(octet_length(source_id),0)),0) FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2)+
 (SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(input_digest)),0) FROM migration_metadata_receipt WHERE import_id=$1 AND organization_id=$2)
 "#).bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let row=sqlx::query("SELECT retained_bytes,reserved_bytes,(SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_metadata_reservation WHERE import_id=$1 AND organization_id=$2) AS reservations FROM migration_metadata_import WHERE id=$1 AND organization_id=$2").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        row.get::<i64, _>("retained_bytes"),
        counted,
        "exact persisted-column inventory"
    );
    assert_eq!(
        row.get::<i64, _>("reserved_bytes"),
        row.get::<i64, _>("reservations")
    );
}
async fn native(f: &Fixture) -> (i64, i64, i64, i64) {
    let r=sqlx::query("SELECT (SELECT count(*) FROM tag WHERE organization_id=$1) AS tags,(SELECT count(*) FROM custom_field WHERE organization_id=$1) AS fields,(SELECT count(*) FROM person_tag WHERE organization_id=$1) AS links,(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1) AS cells").bind(f.org).fetch_one(&f.pool).await.unwrap();
    (
        r.get("tags"),
        r.get("fields"),
        r.get("links"),
        r.get("cells"),
    )
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_exact_inventory_concurrent_workers_and_duplicate_recovery(migrator: PgPool) {
    let (f, parent, id, p) = fixture(&migrator, true).await;
    inventory(&f, id).await;
    let parent_before: Value =
        sqlx::query_scalar("SELECT to_jsonb(i) FROM migration_import i WHERE id=$1")
            .bind(parent)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let calls = f.reader.calls();
    confirm(&f, id, &p).await;
    inventory(&f, id).await;
    let (a, b) = tokio::join!(drain(&f, &f.policy), drain(&f, &f.policy));
    let _ = (a, b);
    assert_eq!(detail(&f, id).await["state"], "completed");
    assert_eq!(native(&f).await, (1, 1, 1, 1));
    inventory(&f, id).await;
    drain(&f, &f.policy).await;
    inventory(&f, id).await;
    assert_eq!(detail(&f, id).await["reserved_bytes"], "0");
    assert_eq!(f.reader.calls(), calls);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(i) FROM migration_import i WHERE id=$1")
        .bind(parent)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(after, parent_before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_result_failure_rolls_back_native_identity_cursor_and_bytes(
    migrator: PgPool,
) {
    let (f, _, id, p) = fixture(&migrator, false).await;
    confirm(&f, id, &p).await;
    let before = detail(&f, id).await;
    inventory(&f, id).await;
    sqlx::query("CREATE FUNCTION metadata_test_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='P0199',MESSAGE='synthetic_result_failure'; END $$").execute(&migrator).await.unwrap();
    sqlx::query("CREATE TRIGGER metadata_test_fail BEFORE INSERT ON migration_metadata_result FOR EACH ROW EXECUTE FUNCTION metadata_test_fail()").execute(&migrator).await.unwrap();
    assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    assert_eq!(detail(&f, id).await["state"], "paused");
    assert_eq!(native(&f).await, (0, 0, 0, 0));
    inventory(&f, id).await;
    assert_eq!(
        detail(&f, id).await["retained_bytes"],
        before["retained_bytes"]
    );
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_metadata_identity WHERE import_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(n, 0);
    sqlx::query("DROP TRIGGER metadata_test_fail ON migration_metadata_result")
        .execute(&migrator)
        .await
        .unwrap();
    retry(&f, id).await;
    drain(&f, &f.policy).await;
    assert_eq!(native(&f).await, (1, 1, 1, 1));
    inventory(&f, id).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_current_ceiling_pause_explicit_retry_and_cancel_at_exhaustion(
    migrator: PgPool,
) {
    let (f, _, id, p) = fixture(&migrator, true).await;
    confirm(&f, id, &p).await;
    let mut low = f.policy.clone();
    let r=sqlx::query("SELECT retained_bytes,reserved_bytes FROM migration_snapshot WHERE id=$1 AND organization_id=$2").bind(f.snapshot).bind(f.org).fetch_one(&f.pool).await.unwrap();
    low.run_ceiling_bytes = r.get::<i64, _>("retained_bytes") + r.get::<i64, _>("reserved_bytes");
    low.org_ceiling_bytes = low.run_ceiling_bytes;
    drain(&f, &low).await;
    assert_eq!(detail(&f, id).await["pause_reason"], "storage_limit");
    assert_eq!(native(&f).await, (0, 0, 0, 0));
    inventory(&f, id).await;
    drain(&f, &f.policy).await;
    assert_eq!(native(&f).await, (0, 0, 0, 0));
    let request = metadata::ImportRequest {
        request_id: Uuid::new_v4(),
    };
    let response = metadata::action(&f.pool, &f.key, &f.ctx, id, request, false, &low)
        .await
        .unwrap();
    assert_eq!(response["import"]["reserved_bytes"], "0");
    assert_eq!(detail(&f, id).await["state"], "cancelled");
    inventory(&f, id).await;
    assert_eq!(native(&f).await, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_expired_lease_reclaim_and_partial_cancel_preserve_units(migrator: PgPool) {
    let (f, _, id, p) = fixture(&migrator, false).await;
    confirm(&f, id, &p).await;
    sqlx::query("UPDATE migration_metadata_import SET state='running',lease_token=$2,lease_expires_at=now()-interval '1 second' WHERE id=$1").bind(id).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
    assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let counts = native(&f).await;
    assert_eq!(counts.1, 1);
    inventory(&f, id).await;
    metadata::action(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        metadata::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        false,
        &f.policy,
    )
    .await
    .unwrap();
    drain(&f, &f.policy).await;
    assert_eq!(native(&f).await, counts);
    inventory(&f, id).await;
    assert_eq!(detail(&f, id).await["reserved_bytes"], "0");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_private_permit_has_no_unrelated_insert_update_delete_or_parent_bypass(
    migrator: PgPool,
) {
    let (f, _, id, p) = fixture(&migrator, false).await;
    confirm(&f, id, &p).await;
    assert!(tag::create_tag(
        &f.pool,
        &f.ctx,
        CreateTag {
            name: "Ordinary blocked".into()
        }
    )
    .await
    .is_err());
    let plan = Uuid::parse_str(p["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let row = sqlx::query(
        "SELECT id,target_id FROM migration_metadata_mapping WHERE plan_id=$1 AND kind='tag'",
    )
    .bind(plan)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let token = Uuid::new_v4();
    let unit: Uuid = row.get("id");
    let target: Uuid = row.get("target_id");
    sqlx::query("UPDATE migration_metadata_import SET state='running',lease_token=$2,lease_expires_at=now()+interval '1 minute' WHERE id=$1").bind(id).bind(token).execute(&migrator).await.unwrap();
    for (permit, chosen) in [
        ("crm.metadata_token", Uuid::new_v4()),
        ("crm.metadata_token", unit),
        ("crm.import_token", unit),
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config($1,$2,true),set_config('crm.metadata_unit',$3,true)")
            .bind(permit)
            .bind(token.to_string())
            .bind(chosen.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let error=sqlx::query("INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,'Wrong target')").bind(Uuid::new_v4()).bind(f.org).bind(f.actor).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("P010C")
        );
        tx.rollback().await.unwrap();
    }
    // Even the right target is not usable after lease expiry.
    sqlx::query("UPDATE migration_metadata_import SET lease_expires_at=now()-interval '1 second' WHERE id=$1").bind(id).execute(&migrator).await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('crm.metadata_token',$1,true),set_config('crm.metadata_unit',$2,true)",
    )
    .bind(token.to_string())
    .bind(unit.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(sqlx::query(
        "INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,'Atomic Tag')"
    )
    .bind(target)
    .bind(f.org)
    .bind(f.actor)
    .execute(&mut *tx)
    .await
    .is_err());
    tx.rollback().await.unwrap();
    drain(&f, &f.policy).await;
    assert_eq!(detail(&f, id).await["state"], "completed");
    for statement in [
        "UPDATE tag SET name='Changed' WHERE id=$1 AND organization_id=$2",
        "DELETE FROM tag WHERE id=$1 AND organization_id=$2",
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.metadata_token',$1,true),set_config('crm.metadata_unit',$2,true)").bind(token.to_string()).bind(unit.to_string()).execute(&mut *tx).await.unwrap();
        let e = sqlx::query(statement)
            .bind(target)
            .bind(f.org)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("P010C")
        );
        tx.rollback().await.unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_gate_catalog_continues_in_fifty_descriptor_units(migrator: PgPool) {
    let (f, _, id, p) = fixture_book(&migrator, false, true).await;
    let response=metadata::replan(&f.pool,&f.key,&f.ctx,id,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":p["latest_plan"]["revision"],"mappings":[]})).unwrap(),&f.policy).await.unwrap();
    let plan = Uuid::parse_str(response["import"]["latest_plan"]["id"].as_str().unwrap()).unwrap();
    let mut previous = (0i64, 0i64);
    let mut continued = false;
    for _ in 0..1200 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            break;
        }
        let r=sqlx::query("SELECT checkpoint_element,(SELECT count(*) FROM migration_metadata_mapping WHERE plan_id=$1) AS mappings,(SELECT count(*) FROM migration_metadata_alias WHERE plan_id=$1) AS aliases FROM migration_metadata_plan WHERE id=$1").bind(plan).fetch_one(&f.pool).await.unwrap();
        let counts = (r.get::<i64, _>("mappings"), r.get::<i64, _>("aliases"));
        assert!(counts.0 - previous.0 <= 50);
        assert!(counts.1 - previous.1 <= 50);
        continued |= r.get::<i32, _>("checkpoint_element") > 0;
        previous = counts;
    }
    assert!(continued);
    assert_eq!(previous, (244, 121));
    assert_eq!(detail(&f, id).await["latest_plan"]["state"], "ready");
    assert_eq!(native(&f).await, (0, 0, 0, 0));
    inventory(&f, id).await;
}

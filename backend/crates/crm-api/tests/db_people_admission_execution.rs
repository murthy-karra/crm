//! D-078 real retained-capture admission acceptance.  This drives the existing
//! 010c parent and 010e1 report; no plan, result, identity, or provenance rows
//! are fabricated by the test.
use crate::{db_activity_source, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, core_change_worker, people_admission, people_admission_worker,
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_source::Stream,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
fn number(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}

pub(super) async fn report(f: &import_support::Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    f.reader.set_records(Stream::People, people);
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let snapshot = snapshot::propose(
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
    let newer = uuid(&snapshot["snapshot"]["id"]);
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        newer,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    let report = core_change_reports::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        core_change_reports::PrepareCoreChangeReport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            newer_snapshot_id: newer,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = uuid(&report["report_id"]);
    for _ in 0..100 {
        if !core_change_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    assert_eq!(
        core_change_reports::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"],
        "completed"
    );
    id
}

pub(super) async fn ready(f: &import_support::Fixture, parent: Uuid, people: Vec<Value>) -> Uuid {
    let report = report(f, parent, people).await;
    let value = people_admission::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_admission::PreparePeopleAdmission {
            request_id: Uuid::new_v4(),
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = uuid(&value["admission_id"]);
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"]
            == "ready"
        {
            return id;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("admission preview did not seal")
}

async fn drain(f: &import_support::Fixture, id: Uuid) {
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"]
            == "completed"
        {
            return;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    panic!("admission execution did not complete")
}

async fn confirm_ready(f: &import_support::Fixture, id: Uuid) {
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let plan = &detail["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&plan["id"]),
            plan_revision: number(&plan["revision"]),
            plan_digest: plan["digest"].as_str().unwrap().into(),
            eligible_count: number(&plan["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn retained_new_people_admit_with_native_identity_provenance_and_facts(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id=ready(&f,parent,vec![
        json!({"id":101,"firstName":"Synthetic One","stage":"Lead","assignedUserId":3,"phones":[{"value":"4155550100"}]}),
        json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","phones":[{"value":"(415)555-0100"}]}),
        json!({"id":103,"firstName":"Synthetic held","stage":"Lead","isTrash":true}),
        json!({"id":104,"firstName":"New Person","lastName":"Unicode λ","stage":"Lead","emails":[{"value":"new@example.test"}],"phones":[{"value":"4155550104"}]})
    ]).await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let mappings: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('kind',kind,'source_key',source_key,'disposition',disposition,'qualified',qualified,'target_id',target_id)),'[]'::jsonb) FROM migration_import_mapping WHERE plan_id=(SELECT parent_plan_id FROM migration_people_admission WHERE id=$1)").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        number(&detail["plan"]["counts"]["eligible"]),
        1,
        "{detail} {mappings}"
    );
    let items = people_admission::items(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::Page {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let new = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["source_id"] == "104")
        .unwrap();
    assert_eq!(new["disposition"], "eligible");
    let plan = &detail["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&plan["id"]),
            plan_revision: number(&plan["revision"]),
            plan_digest: plan["digest"].as_str().unwrap().into(),
            eligible_count: number(&plan["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    drain(&f, id).await;
    let retained: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_people_admission WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        retained,
        people_admission::retained_byte_audit(&f.pool, &f.ctx, id)
            .await
            .unwrap(),
        "live admission ledger must equal the independent persisted-byte audit"
    );
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='104' AND disposition='settled'").bind(id).fetch_one(&f.pool).await.unwrap();
    let contacts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contact_method WHERE organization_id=$1 AND person_id=$2",
    )
    .bind(f.org)
    .bind(person)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(contacts, 2);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=17 AND family='people' AND source_id='104' AND admission_id=$2 AND target_id=$3").bind(f.org).bind(id).bind(person).fetch_one(&f.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_admitted WHERE organization_id=$1 AND person_id=$2 AND admission_id=$3").bind(f.org).bind(person).bind(id).fetch_one(&f.pool).await.unwrap(),1);
    let provenance = people_admission::admission_provenance(&f.pool, &f.key, &f.ctx, person)
        .await
        .unwrap();
    assert_eq!(provenance["provenance"]["source_id"], "104");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inquiry WHERE organization_id=$1 AND person_id=$2"
        )
        .bind(f.org)
        .bind(person)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    let direct = sqlx::query(
        "INSERT INTO person(id,organization_id,first_name,stage_id) VALUES($1,$2,'forbidden',$3)",
    )
    .bind(Uuid::new_v4())
    .bind(f.org)
    .bind(f.lead_stage)
    .execute(&f.pool)
    .await;
    assert!(direct.is_err(), "an app connection without an exact admission permit must not write review-workspace People");
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn original_presence_and_unmapped_stage_are_held_before_confirmation(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id = ready(
        &f,
        parent,
        vec![
            json!({"id":101,"firstName":"Changed original","stage":"Lead","phones":[{"value":"4155550100"}]}),
            json!({"id":104,"firstName":"Unmapped","stage":"New stage","phones":[{"value":"4155550104"}]}),
            json!({"id":105,"firstName":"Qualified","stage":"Lead","phones":[{"value":"4155550105"}]}),
        ],
    )
    .await;
    let items = people_admission::items(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::Page {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let item = |source: &str| {
        items["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["source_id"] == source)
            .unwrap()["disposition"]
            .clone()
    };
    assert_eq!(item("101"), "already_imported");
    assert_eq!(item("104"), "held_mapping_gap");
    assert_eq!(item("105"), "eligible");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person WHERE organization_id=$1 AND first_name IN ('Changed original','Unmapped','Qualified')").bind(f.org).fetch_one(&f.pool).await.unwrap(), 0);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn cancelled_run_allows_same_report_remainder_preview(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let people = vec![
        json!({"id":104,"firstName":"Remainder","stage":"Lead","phones":[{"value":"4155550104"}]}),
    ];
    let first = ready(&f, parent, people).await;
    let first_detail = people_admission::detail(&f.pool, &f.key, &f.ctx, first)
        .await
        .unwrap();
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        first,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: number(&first_detail["lifecycle_revision"]),
        },
    )
    .await
    .unwrap();
    let report: Uuid =
        sqlx::query_scalar("SELECT report_id FROM migration_people_admission WHERE id=$1")
            .bind(first)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let value = people_admission::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_admission::PreparePeopleAdmission {
            request_id: Uuid::new_v4(),
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let second = uuid(&value["admission_id"]);
    for _ in 0..100 {
        if people_admission::detail(&f.pool, &f.key, &f.ctx, second)
            .await
            .unwrap()["state"]
            == "ready"
        {
            break;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, second)
        .await
        .unwrap();
    assert_eq!(detail["state"], "ready");
    assert_eq!(number(&detail["plan"]["counts"]["eligible"]), 1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=17 AND source_id='104'").bind(f.org).fetch_one(&f.pool).await.unwrap(),0);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn lowered_policy_fences_before_native_write_and_preserves_cancel_capacity(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id = ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Policy Fence","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let plan = &detail["plan"];
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: uuid(&plan["id"]),
            plan_revision: number(&plan["revision"]),
            plan_digest: plan["digest"].as_str().unwrap().into(),
            eligible_count: number(&plan["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let tiny = snapshot::SnapshotPolicy {
        run_ceiling_bytes: 1,
        org_ceiling_bytes: 1,
    };
    assert!(matches!(
        people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &tiny,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::StorageLimit)
    ));
    let paused = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "storage_limit");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Policy Fence'"
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    people_admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: number(&paused["lifecycle_revision"]),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"],
        "cancelled"
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn expired_lease_and_initiator_demotion_fence_execution(migrator: PgPool) {
    let stale = import_support::fixture(&migrator, import_support::default_people()).await;
    let stale_parent = db_activity_source::completed_parent(&stale).await;
    let stale_id = ready(
        &stale,
        stale_parent,
        vec![json!({"id":106,"firstName":"Expired Lease","stage":"Lead"})],
    )
    .await;
    confirm_ready(&stale, stale_id).await;
    sqlx::query("UPDATE migration_people_admission SET state='running',lease_token=$2,lease_epoch=7,lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(stale_id).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
    assert!(matches!(
        people_admission_worker::run_once(
            &stale.pool,
            &stale.key,
            &stale.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let stale_detail = people_admission::detail(&stale.pool, &stale.key, &stale.ctx, stale_id)
        .await
        .unwrap();
    assert_eq!(stale_detail["state"], "paused");
    assert_eq!(stale_detail["pause_reason"], "lease_conflict");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Expired Lease'"
        )
        .bind(stale.org)
        .fetch_one(&stale.pool)
        .await
        .unwrap(),
        0
    );

    let demoted = import_support::fixture(&migrator, import_support::default_people()).await;
    let demoted_parent = db_activity_source::completed_parent(&demoted).await;
    let demoted_id = ready(
        &demoted,
        demoted_parent,
        vec![json!({"id":106,"firstName":"Demoted Initiator","stage":"Lead"})],
    )
    .await;
    confirm_ready(&demoted, demoted_id).await;
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2")
        .bind(demoted.org).bind(demoted.actor).execute(&migrator).await.unwrap();
    assert!(matches!(
        people_admission_worker::run_once(
            &demoted.pool,
            &demoted.key,
            &demoted.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    let demoted_state: (String, String) =
        sqlx::query_as("SELECT state,pause_reason FROM migration_people_admission WHERE id=$1")
            .bind(demoted_id)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(demoted_state.0, "paused");
    assert_eq!(demoted_state.1, "authority_changed");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Demoted Initiator'").bind(demoted.org).fetch_one(&demoted.pool).await.unwrap(), 0);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn identity_tombstone_and_sealed_plan_items_are_immutable(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let parent_plan: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_import_plan WHERE import_id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id) VALUES($1,17,'people','106',$2,$3,$4)")
        .bind(f.org).bind(Uuid::new_v4()).bind(parent).bind(parent_plan).execute(&migrator).await.unwrap();
    let id = ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Erased Identity","stage":"Lead"})],
    )
    .await;
    let item: (Uuid, Uuid) = sqlx::query_as(
        "SELECT i.id,i.plan_id FROM migration_people_admission_item i WHERE i.admission_id=$1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT disposition FROM migration_people_admission_item WHERE id=$1"
        )
        .bind(item.0)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "held_identity"
    );
    assert!(
        sqlx::query("UPDATE migration_people_admission_plan SET state='building' WHERE id=$1")
            .bind(item.1)
            .execute(&migrator)
            .await
            .is_err()
    );
    assert!(sqlx::query(
        "UPDATE migration_people_admission_item SET source_key='rewritten' WHERE id=$1"
    )
    .bind(item.0)
    .execute(&migrator)
    .await
    .is_err());
    assert!(sqlx::query("INSERT INTO migration_people_admission_contact(id,item_id,admission_id,organization_id,kind,import_order,value_nonce,value_ciphertext) VALUES($1,$2,$3,$4,'email',99,$5,$6)")
        .bind(Uuid::new_v4()).bind(item.0).bind(id).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).execute(&migrator).await.is_err());
}

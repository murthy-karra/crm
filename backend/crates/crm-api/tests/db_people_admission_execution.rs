//! D-078 real retained-capture admission acceptance.  This drives the existing
//! 010c parent and 010e1 report; no plan, result, identity, or provenance rows
//! are fabricated by the lifecycle tests. The cfg(perf-harness) relation-volume
//! fixture below is explicitly inert synthetic query-plan setup.
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

#[cfg(feature = "perf-harness")]
const ADMISSION_WORKER_SOURCE: &str =
    include_str!("../../crm-app/src/domain/migration/people_admission_worker.rs");
#[cfg(feature = "perf-harness")]
const ADMISSION_SOURCE_SOURCE: &str =
    include_str!("../../crm-app/src/domain/migration/people_admission_source.rs");
#[cfg(feature = "perf-harness")]
const ADMISSION_QUERIES_SOURCE: &str =
    include_str!("../../crm-app/src/domain/migration/people_admission_queries.rs");
#[cfg(feature = "perf-harness")]
const HOT_PEOPLE: i64 = 25_000;

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
fn number(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}

#[cfg(feature = "perf-harness")]
fn production_statement(source: &str, prefix: &str) -> String {
    let marker = format!("\"{prefix}");
    let start = source
        .find(&marker)
        .expect("production SQL prefix must remain present");
    serde_json::Deserializer::from_str(&source[start..])
        .into_iter::<String>()
        .next()
        .unwrap()
        .unwrap()
}

#[cfg(feature = "perf-harness")]
macro_rules! explain {
    ($fixture:expr, $source:expr, $prefix:expr $(, $bind:expr)* $(,)?) => {{
        let sql = production_statement($source, $prefix);
        let statement = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        sqlx::query_scalar::<_, Value>(&statement)
            $(.bind($bind))*
            .fetch_one(&$fixture.pool)
            .await
            .unwrap()
    }};
}

#[cfg(feature = "perf-harness")]
fn plan_nodes<'a>(value: &'a Value, nodes: &mut Vec<&'a Value>) {
    match value {
        Value::Object(values) => {
            if values.contains_key("Node Type") {
                nodes.push(value);
            }
            for value in values.values() {
                plan_nodes(value, nodes);
            }
        }
        Value::Array(values) => {
            for value in values {
                plan_nodes(value, nodes);
            }
        }
        _ => {}
    }
}

#[cfg(feature = "perf-harness")]
fn uses_index(plan: &Value, table: &str, index: &str) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    assert!(
        nodes.iter().any(|node| {
            node["Relation Name"] == table && node["Index Name"].as_str() == Some(index)
        }),
        "expected {index} for {table}: {plan}"
    );
}

#[cfg(feature = "perf-harness")]
fn uses_any_index(plan: &Value, table: &str, indexes: &[&str]) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    assert!(
        nodes.iter().any(|node| {
            node["Relation Name"] == table
                && indexes.contains(&node["Index Name"].as_str().unwrap_or_default())
        }),
        "expected one of {indexes:?} for {table}: {plan}"
    );
}

#[cfg(feature = "perf-harness")]
fn bounded_work(plan: &Value, table: &str, max_rows: f64) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    let examined: f64 = nodes
        .iter()
        .filter(|node| node["Relation Name"] == table)
        .map(|node| {
            (node["Actual Rows"].as_f64().unwrap_or_default()
                + node["Rows Removed by Filter"].as_f64().unwrap_or_default())
                * node["Actual Loops"].as_f64().unwrap_or(1.0)
        })
        .sum();
    assert!(
        examined <= max_rows,
        "{table} examined {examined} rows for a bounded lookup: {plan}"
    );
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
        "SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id) VALUES($1,17,'people','106',$2,$3,$4)")
        .bind(f.org).bind(Uuid::nil()).bind(parent).bind(parent_plan).execute(&migrator).await.unwrap();
    let id = ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Erased Identity","stage":"Lead"})],
    )
    .await;
    let item: (Uuid, Uuid) = sqlx::query_as(
        "SELECT i.id,i.plan_id FROM migration_people_admission_item i WHERE i.admission_id=$1 AND i.source_id='106'",
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

#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn hot_plan_25k_people_has_sparse_pages_and_dense_shared_contacts(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let report_id = report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Hot seed","stage":"Lead"})],
    )
    .await;
    let prepared = people_admission::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_admission::PreparePeopleAdmission {
            request_id: Uuid::new_v4(),
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let admission_id = uuid(&prepared["admission_id"]);
    let scale_report = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_core_change_report(id,organization_id,parent_import_id,parent_plan_id,baseline_snapshot_id,newer_snapshot_id,baseline_sequence,newer_sequence,source_account_id,workspace_revision,initiated_by_user_id,engine_version,tuple_hmac,inputs_nonce,inputs_ciphertext,state,phase) SELECT $1,organization_id,parent_import_id,parent_plan_id,baseline_snapshot_id,newer_snapshot_id,baseline_sequence+1,newer_sequence,source_account_id,workspace_revision,initiated_by_user_id,engine_version,decode(repeat('7a',32),'hex'),inputs_nonce,inputs_ciphertext,'running','compare' FROM migration_core_change_report WHERE id=$2 AND organization_id=$3")
        .bind(scale_report).bind(report_id).bind(f.org).execute(&migrator).await.unwrap();
    // This query-plan fixture is a scale relation setup, not a claim that a
    // synthetic bulk insert qualified source captures or timed a full worker.
    // Its rows use the same report/admission foreign keys as preparation.
    sqlx::query(
        "UPDATE migration_people_admission SET report_id=$3 WHERE id=$1 AND organization_id=$2",
    )
    .bind(admission_id)
    .bind(f.org)
    .bind(scale_report)
    .execute(&migrator)
    .await
    .unwrap();
    for first in (1..=HOT_PEOPLE).step_by(250) {
        let last = (first + 249).min(HOT_PEOPLE);
        sqlx::query("INSERT INTO migration_core_change_group(id,report_id,organization_id,family,source_key,source_id,nonce,ciphertext) SELECT gen_random_uuid(),$1,$2,'people',(1000000+g)::text,(1000000+g)::text,$3,$4 FROM generate_series($5,$6) g")
            .bind(scale_report).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(first).bind(last).execute(&migrator).await.unwrap();
    }
    // The operational side of the scale fixture is deliberately native, with
    // 50 active members and one shared contact value across 25k People.
    sqlx::query("WITH members AS (SELECT gen_random_uuid() id FROM generate_series(1,48)) INSERT INTO app_user(id,email,display_name) SELECT id,id::text||'@admission-scale.synthetic.test','Admission scale member' FROM members")
        .execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM app_user WHERE email LIKE '%@admission-scale.synthetic.test'")
        .bind(f.org).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO person(organization_id,stage_id,assigned_user_id,first_name,last_name) SELECT $1,$2,$3,'Admission scale',g::text FROM generate_series(1,$4) g")
        .bind(f.org).bind(f.lead_stage).bind(f.actor).bind(HOT_PEOPLE).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) SELECT organization_id,id,'email','shared-hot-plan@synthetic.test','shared-hot-plan@synthetic.test' FROM person WHERE organization_id=$1 AND first_name='Admission scale'")
        .bind(f.org).execute(&migrator).await.unwrap();
    let member_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    let native_people: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Admission scale'",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    let shared_native_contacts: i64 = sqlx::query_scalar("SELECT count(*) FROM contact_method WHERE organization_id=$1 AND normalized_value='shared-hot-plan@synthetic.test'")
        .bind(f.org).fetch_one(&migrator).await.unwrap();
    assert_eq!(member_count, 50);
    assert_eq!(native_people, HOT_PEOPLE);
    assert_eq!(shared_native_contacts, HOT_PEOPLE);
    let plan_id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_admission_plan(id,admission_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) VALUES($1,$2,$3,1,'building',$4,$5)")
        .bind(plan_id).bind(admission_id).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_people_admission_item(id,admission_id,plan_id,organization_id,source_key,source_id,prospective_person_id,disposition,projection_nonce,projection_ciphertext,provenance_nonce,provenance_ciphertext,item_byte_bound) SELECT ('00000000-0000-0000-0000-'||lpad(g::text,12,'0'))::uuid,$1,$2,$3,(1000000+g)::text,(1000000+g)::text,('00000000-0000-0000-0001-'||lpad(g::text,12,'0'))::uuid,CASE WHEN g%500=0 THEN 'eligible' ELSE 'held_evidence_gap' END,$4,$5,$4,$5,1 FROM generate_series(1,$6) g")
        .bind(admission_id).bind(plan_id).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(HOT_PEOPLE).execute(&migrator).await.unwrap();
    let dense_item = Uuid::parse_str("00000000-0000-0000-0000-000000000500").unwrap();
    sqlx::query("INSERT INTO migration_people_admission_contact(id,item_id,admission_id,organization_id,kind,import_order,value_nonce,value_ciphertext,primary_contact) SELECT gen_random_uuid(),i.id,$1,$2,'email',0,$3,$4,true FROM migration_people_admission_item i WHERE i.id<>$5 AND i.admission_id=$1")
        .bind(admission_id).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(dense_item).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_people_admission_contact(id,item_id,admission_id,organization_id,kind,import_order,value_nonce,value_ciphertext,primary_contact) SELECT gen_random_uuid(),$1,$2,$3,'email',g,$4,$5,g=0 FROM generate_series(0,49) g")
        .bind(dense_item).bind(admission_id).bind(f.org).bind(vec![0u8;24]).bind(vec![0u8;16]).execute(&migrator).await.unwrap();
    let parent_plan: Uuid = sqlx::query_scalar(
        "SELECT confirmed_plan_id FROM migration_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    let original_sequence: i64 = sqlx::query_scalar(
        "SELECT original_sequence FROM migration_people_admission WHERE id=$1 AND organization_id=$2",
    )
    .bind(admission_id)
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    let original_capture: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people' ORDER BY sequence LIMIT 1",
    )
    .bind(f.snapshot)
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    // These source-index rows only provide a 25k relation for EXPLAIN. They
    // intentionally do not claim qualified raw capture coverage.
    for first in (1..=HOT_PEOPLE).step_by(250) {
        let last = (first + 249).min(HOT_PEOPLE);
        sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT gen_random_uuid(),$1,$2,$3,$4,g+100,'people',(1000000+g)::text,$5,decode(repeat('6b',32),'hex'),$6,$7,false FROM generate_series($8,$9) g")
            .bind(f.snapshot).bind(f.org).bind(original_capture).bind(original_sequence).bind(Stream::People.representation()).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(first).bind(last).execute(&migrator).await.unwrap();
    }
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id) SELECT $1,17,'people',(1000000+g)::text,('00000000-0000-0000-0001-'||lpad(g::text,12,'0'))::uuid,$2,$3 FROM generate_series(1,$4) g")
        .bind(f.org).bind(parent).bind(parent_plan).bind(HOT_PEOPLE).execute(&migrator).await.unwrap();
    sqlx::query("UPDATE migration_people_admission_plan SET state='ready',digest=decode(repeat('5a',32),'hex'),sealed_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1")
        .bind(plan_id).execute(&migrator).await.unwrap();
    for table in [
        "migration_core_change_group",
        "migration_people_admission_item",
        "migration_people_admission_contact",
        "migration_import_identity",
        "migration_snapshot_record",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&migrator)
            .await
            .unwrap();
    }

    let preparation = explain!(
        f,
        ADMISSION_WORKER_SOURCE,
        "SELECT source_key,source_id FROM migration_core_change_group WHERE",
        scale_report,
        f.org,
        "1012500"
    );
    let identity = explain!(
        f,
        ADMISSION_WORKER_SOURCE,
        "SELECT i.admission_id,EXISTS(SELECT 1 FROM person p",
        f.org,
        17_i64,
        "1012500"
    );
    let absence = explain!(
        f,
        ADMISSION_SOURCE_SOURCE,
        "SELECT EXISTS(SELECT 1 FROM migration_snapshot_record WHERE",
        f.snapshot,
        f.org,
        "1012500",
        original_sequence
    );
    let item_page = explain!(
        f,
        ADMISSION_QUERIES_SOURCE,
        "SELECT i.id FROM migration_people_admission_item i WHERE",
        admission_id,
        f.org,
        plan_id,
        "eligible",
        Option::<Uuid>::None,
        51_i64,
        Option::<bool>::None
    );
    let all_items = explain!(
        f,
        ADMISSION_QUERIES_SOURCE,
        "SELECT i.id FROM migration_people_admission_item i WHERE i.admission_id=$1 AND i.organization_id=$2 AND i.plan_id=$3 AND ($4::uuid",
        admission_id,
        f.org,
        plan_id,
        Option::<Uuid>::None,
        51_i64
    );
    let cancelled_stored = explain!(
        f,
        ADMISSION_QUERIES_SOURCE,
        "SELECT i.id FROM migration_people_admission_item i WHERE",
        admission_id,
        f.org,
        plan_id,
        "cancelled",
        Option::<Uuid>::None,
        51_i64,
        Option::<bool>::None
    );
    let cancelled_virtual = explain!(
        f,
        ADMISSION_QUERIES_SOURCE,
        "SELECT i.id FROM migration_people_admission_item i WHERE",
        admission_id,
        f.org,
        plan_id,
        "eligible",
        Option::<Uuid>::None,
        51_i64,
        Some(true)
    );
    let contact_page = explain!(
        f,
        ADMISSION_QUERIES_SOURCE,
        "SELECT id,kind,import_order,primary_contact,octet_length(value_nonce)",
        admission_id,
        f.org,
        dense_item,
        Option::<Uuid>::None,
        51_i64
    );
    let claim = explain!(
        f,
        ADMISSION_WORKER_SOURCE,
        "SELECT * FROM migration_people_admission_item WHERE admission_id=",
        admission_id,
        f.org,
        plan_id
    );

    uses_index(
        &preparation,
        "migration_core_change_group",
        "migration_core_change_group_admission_people_keyset",
    );
    bounded_work(&preparation, "migration_core_change_group", 1.0);
    uses_index(
        &identity,
        "migration_import_identity",
        "migration_import_identity_pkey",
    );
    bounded_work(&identity, "migration_import_identity", 1.0);
    uses_index(
        &absence,
        "migration_snapshot_record",
        "migration_snapshot_record_page",
    );
    bounded_work(&absence, "migration_snapshot_record", 1.0);
    uses_index(
        &item_page,
        "migration_people_admission_item",
        "migration_people_admission_item_page",
    );
    bounded_work(&item_page, "migration_people_admission_item", 51.0);
    uses_any_index(
        &all_items,
        "migration_people_admission_item",
        &[
            "migration_people_admission_item_all_page",
            "migration_people_admission_item_pkey",
        ],
    );
    bounded_work(&all_items, "migration_people_admission_item", 51.0);
    for plan in [&cancelled_stored, &cancelled_virtual] {
        uses_index(
            plan,
            "migration_people_admission_item",
            "migration_people_admission_item_page",
        );
        bounded_work(plan, "migration_people_admission_item", 51.0);
    }
    uses_any_index(
        &contact_page,
        "migration_people_admission_contact",
        &[
            "migration_people_admission_contact_page",
            "migration_people_admission_contac_item_id_kind_import_order_key",
        ],
    );
    bounded_work(&contact_page, "migration_people_admission_contact", 50.0);
    uses_index(
        &claim,
        "migration_people_admission_item",
        "migration_people_admission_item_eligible_claim",
    );
    bounded_work(&claim, "migration_people_admission_item", 1.0);
}

//! Remaining 010f1 acceptance boundaries, using real source captures and typed
//! parent/child commands. Deliberate expiry, conflicting FUB binding and erasure
//! states are migrator-only negative fixtures, never application bypass paths.
//! Child preparation, review, confirmation and execution must make no FUB calls.
use crate::import_support::{self as support, Book, Fixture};
use crm_api::domain::{
    custom_field::{self, CreateCustomField, FieldType},
    migration::{
        crypto, import_worker,
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        metadata, metadata_worker,
        snapshot_source::Stream,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

fn person(id: u64) -> Value {
    json!({"id":id,"firstName":"Synthetic acceptance Person","stage":"Lead","assignedUserId":3})
}
fn field(id: u64, name: &str, label: &str) -> Value {
    json!({"id":id,"name":name,"label":label,"type":"text","isRecurring":false})
}
async fn source_fixture(migrator: &PgPool, people: Vec<Value>, fields: Vec<Value>) -> Fixture {
    let book = Arc::new(Book::new(people));
    book.set_records(Stream::CustomFields, fields);
    support::fixture_with_book(migrator, book).await
}
async fn parent_ready(f: &Fixture) -> (Uuid, Value) {
    let (parent, _) = support::propose(f).await;
    support::drain_import(f).await;
    support::replan(
        f,
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
    support::drain_import(f).await;
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    assert_eq!(ready["plan"]["state"], "ready");
    (parent, ready)
}
async fn parent_confirm(f: &Fixture, parent: Uuid, ready: &Value) {
    imports::confirm(&f.pool,&f.key,&f.ctx,parent,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],"plan_revision":ready["plan"]["revision"],"confirmation_digest":ready["plan"]["confirmation_digest"],"acknowledgments":{"held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}})).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
}
async fn completed_parent(f: &Fixture) -> Uuid {
    let (parent, ready) = parent_ready(f).await;
    parent_confirm(f, parent, &ready).await;
    support::drain_import(f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    parent
}
async fn drain(f: &Fixture) {
    let calls = f.reader.calls();
    for _ in 0..2000 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            assert_eq!(
                f.reader.calls(),
                calls,
                "metadata worker must remain source-free"
            );
            return;
        }
    }
    panic!("bounded synthetic child did not settle");
}
async fn detail(f: &Fixture, child: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, child, &f.policy)
        .await
        .unwrap()
}
fn plan(ready: &Value) -> Uuid {
    Uuid::parse_str(ready["latest_plan"]["id"].as_str().unwrap()).unwrap()
}
async fn propose(f: &Fixture, parent: Uuid) -> (Uuid, Value) {
    let value = metadata::propose(
        &f.pool,
        &f.key,
        &f.ctx,
        serde_json::from_value(json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}))
            .unwrap(),
        &f.policy,
    )
    .await
    .unwrap();
    let child = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    drain(f).await;
    let ready = detail(f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    (child, ready)
}
async fn replan(
    f: &Fixture,
    child: Uuid,
    ready: &Value,
    mappings: Vec<Value>,
) -> Result<Value, MigrationError> {
    metadata::replan(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":ready["latest_plan"]["revision"],"mappings":mappings})).unwrap(),&f.policy).await
}
fn confirmation(ready: &Value) -> metadata::ConfirmMetadataImport {
    serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":ready["latest_plan"]["id"],"plan_revision":ready["latest_plan"]["revision"],"confirmation_digest":ready["latest_plan"]["confirmation_digest"],"workspace_revision":ready["workspace_revision"],"acknowledgments":{"held_count":ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap()
}
async fn confirm(f: &Fixture, child: Uuid, ready: &Value) -> Result<Value, MigrationError> {
    metadata::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        confirmation(ready),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
}
async fn execute(f: &Fixture, child: Uuid, ready: &Value) {
    confirm(f, child, ready).await.unwrap();
    drain(f).await;
    assert_eq!(detail(f, child).await["state"], "completed");
}
fn open(f: &Fixture, plan: Uuid, row: &PgRow, purpose: &str) -> Value {
    let bytes = crypto::open_snapshot(
        &f.key,
        f.ctx.organization_id,
        f.snapshot,
        row.get("id"),
        &format!("metadata-v1:{plan}:{purpose}"),
        row.get("nonce"),
        row.get("ciphertext"),
    )
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
async fn field_mapping(f: &Fixture, plan: Uuid, source_id: &str) -> PgRow {
    sqlx::query("SELECT m.* FROM migration_metadata_mapping m JOIN migration_metadata_source s ON s.id=m.source_row_id AND s.plan_id=m.plan_id AND s.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind='field' AND s.source_id=$3").bind(plan).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap()
}
fn patch(row: &PgRow, choice: Value) -> Value {
    json!({"mapping_id":row.get::<Uuid,_>("id"),"choice":choice})
}
async fn approve_all(f: &Fixture, child: Uuid, ready: &Value) -> Value {
    let rows=sqlx::query("SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 ORDER BY id").bind(plan(ready)).bind(f.org).fetch_all(&f.pool).await.unwrap();
    let patches = rows
        .iter()
        .map(|row| patch(row, json!({"kind":"create_matching"})))
        .collect();
    replan(f, child, ready, patches).await.unwrap();
    drain(f).await;
    let next = detail(f, child).await;
    assert_eq!(next["latest_plan"]["state"], "ready");
    next
}
async fn native_field(f: &Fixture, label: &str) -> Uuid {
    custom_field::create_custom_field(
        &f.pool,
        &f.ctx,
        CreateCustomField {
            label: label.into(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field
    .id
    .0
}
async fn native(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('people',(SELECT count(*) FROM person WHERE organization_id=$1),'tags',(SELECT count(*) FROM tag WHERE organization_id=$1),'fields',(SELECT count(*) FROM custom_field WHERE organization_id=$1),'links',(SELECT count(*) FROM person_tag WHERE organization_id=$1),'values',(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1))").bind(f.org).fetch_one(&f.pool).await.unwrap()
}
async fn imported_person(f: &Fixture, parent: Uuid, source: &str) -> Uuid {
    sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE import_id=$1 AND organization_id=$2 AND source_id=$3 AND disposition='imported'").bind(parent).bind(f.org).bind(source).fetch_one(&f.pool).await.unwrap()
}
fn reason(value: &Value, code: &str) {
    assert!(
        value["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == code),
        "missing closed reason {code}"
    );
}
/// Complete rows, including timestamps and payload bytes, make an accidental
/// Person rewrite or history insertion observable. A real 010c import has no
/// Inquiry, and the empty set explicitly proves this child synthesizes none.
async fn native_history(f: &Fixture, parent: Uuid) -> Value {
    let mut out = serde_json::Map::new();
    for table in [
        "person",
        "contact_method",
        "inquiry",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "person_imported",
    ] {
        let rows:Value=sqlx::query_scalar(&format!("SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]'::jsonb) FROM {table} r WHERE organization_id=$1")).bind(f.org).fetch_one(&f.pool).await.unwrap();
        out.insert(table.into(), rows);
    }
    let parent:Value=sqlx::query_scalar("SELECT jsonb_build_object('import',to_jsonb(i),'workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE w.organization_id=i.organization_id),'results',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM migration_import_result r WHERE r.import_id=i.id AND r.organization_id=i.organization_id),'identities',(SELECT jsonb_agg(to_jsonb(k) ORDER BY k.family,k.source_id) FROM migration_import_identity k WHERE k.import_id=i.id AND k.organization_id=i.organization_id)) FROM migration_import i WHERE i.id=$1 AND i.organization_id=$2").bind(parent).bind(f.org).fetch_one(&f.pool).await.unwrap();
    out.insert("parent".into(), parent);
    Value::Object(out)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_acceptance_cancelled_and_running_parents_are_ineligible(migrator: PgPool) {
    for state in ["cancelled", "running"] {
        let f = source_fixture(&migrator, vec![person(101), person(102)], vec![]).await;
        let (parent, ready) = parent_ready(&f).await;
        if state == "cancelled" {
            imports::action(
                &f.pool,
                &f.key,
                &f.ctx,
                parent,
                imports::ImportRequest {
                    request_id: Uuid::new_v4(),
                },
                false,
                &f.policy,
            )
            .await
            .unwrap();
        } else {
            parent_confirm(&f, parent, &ready).await;
            assert!(import_worker::run_once(&f.pool, &f.key, &f.policy)
                .await
                .unwrap());
        }
        let before = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap();
        assert_eq!(
            before["state"], state,
            "negative parent state is reached by typed commands"
        );
        let calls = f.reader.calls();
        let rejected = metadata::propose(
            &f.pool,
            &f.key,
            &f.ctx,
            serde_json::from_value(json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}))
                .unwrap(),
            &f.policy,
        )
        .await;
        assert!(matches!(rejected, Err(MigrationError::SourceNotEligible)));
        assert_eq!(
            imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
                .await
                .unwrap(),
            before
        );
        let children: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM migration_metadata_import WHERE organization_id=$1",
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(children, 0);
        assert_eq!(f.reader.calls(), calls);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_acceptance_expired_ten_minute_plan_requires_explicit_replan(migrator: PgPool) {
    let (f, _, child, ready) = crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let calls = f.reader.calls();
    let ttl:i64=sqlx::query_scalar("SELECT extract(epoch FROM (expires_at-completed_at))::bigint FROM migration_metadata_plan WHERE id=$1 AND organization_id=$2").bind(plan(&ready)).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(ttl, 600, "ready confirmation lifetime is ten minutes");
    // Time-advance negative fixture: no sleeps or altered command semantics.
    sqlx::query("UPDATE migration_metadata_plan SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1 AND organization_id=$2").bind(plan(&ready)).bind(f.org).execute(&migrator).await.unwrap();
    let before = native(&f).await;
    assert!(matches!(
        confirm(&f, child, &ready).await,
        Err(MigrationError::ImportExpired)
    ));
    let state=sqlx::query("SELECT state,confirmed_plan_id FROM migration_metadata_import WHERE id=$1 AND organization_id=$2").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(state.get::<String, _>("state"), "proposed");
    assert!(state.get::<Option<Uuid>, _>("confirmed_plan_id").is_none());
    let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM migration_metadata_receipt WHERE import_id=$1 AND organization_id=$2 AND action='confirm'").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(receipts, 0);
    assert_eq!(native(&f).await, before);
    drain(&f).await;
    assert_eq!(
        native(&f).await,
        before,
        "expiry does not auto-confirm or replan"
    );
    replan(&f, child, &ready, vec![]).await.unwrap();
    drain(&f).await;
    let fresh = detail(&f, child).await;
    assert_ne!(plan(&fresh), plan(&ready));
    assert_eq!(fresh["latest_plan"]["state"], "ready");
    let old_revision = ready["latest_plan"]["revision"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert_eq!(
        fresh["latest_plan"]["revision"],
        (old_revision + 1).to_string()
    );
    assert_ne!(
        fresh["latest_plan"]["confirmation_digest"],
        ready["latest_plan"]["confirmation_digest"]
    );
    let old: String = sqlx::query_scalar(
        "SELECT state FROM migration_metadata_plan WHERE id=$1 AND organization_id=$2",
    )
    .bind(plan(&ready))
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(old, "superseded");
    let ttl:i64=sqlx::query_scalar("SELECT extract(epoch FROM (expires_at-completed_at))::bigint FROM migration_metadata_plan WHERE id=$1 AND organization_id=$2").bind(plan(&fresh)).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(ttl, 600);
    execute(&f, child, &fresh).await;
    assert_eq!(
        native(&f).await,
        json!({"people":1,"tags":1,"fields":1,"links":1,"values":1})
    );
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_acceptance_two_valid_fields_cannot_merge_into_one_native_target(
    migrator: PgPool,
) {
    let mut p = person(101);
    p["customA"] = json!("first value");
    p["customB"] = json!("second value");
    p["customSafe"] = json!("independent value");
    let f = source_fixture(
        &migrator,
        vec![p],
        vec![
            field(21, "customA", "A"),
            field(22, "customB", "B"),
            field(23, "customSafe", "Safe"),
        ],
    )
    .await;
    let target = native_field(&f, "Shared native target").await;
    let native_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(f) FROM custom_field f WHERE id=$1 AND organization_id=$2",
    )
    .bind(target)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let parent = completed_parent(&f).await;
    let before = native_history(&f, parent).await;
    let calls = f.reader.calls();
    let (child, first) = propose(&f, parent).await;
    let a = field_mapping(&f, plan(&first), "21").await;
    let b = field_mapping(&f, plan(&first), "22").await;
    let safe = field_mapping(&f, plan(&first), "23").await;
    assert!(a.get::<bool, _>("qualified") && b.get::<bool, _>("qualified"));
    replan(
        &f,
        child,
        &first,
        vec![
            patch(&a, json!({"kind":"map_existing","target_id":target})),
            patch(&b, json!({"kind":"map_existing","target_id":target})),
            patch(&safe, json!({"kind":"create_matching"})),
        ],
    )
    .await
    .unwrap();
    drain(&f).await;
    let ready = detail(&f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    for source in ["21", "22"] {
        let mapping = field_mapping(&f, plan(&ready), source).await;
        assert_eq!(mapping.get::<String, _>("disposition"), "hold");
        reason(
            &open(&f, plan(&ready), &mapping, "mapping"),
            "mapping_target_collision",
        );
    }
    let manifest=sqlx::query("SELECT * FROM migration_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id='101'").bind(plan(&ready)).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let manifest = open(&f, plan(&ready), &manifest, "manifest");
    for source in ["customA", "customB"] {
        let op = manifest["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["source_field"] == source)
            .unwrap();
        assert_eq!(op["disposition"], "held");
        reason(op, "field_mapping_held");
    }
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "4");
    execute(&f, child, &ready).await;
    let applied:Vec<(String,String)>=sqlx::query_as("SELECT f.label,v.text_value FROM person_custom_field_value v JOIN custom_field f ON f.id=v.field_id AND f.organization_id=v.organization_id WHERE v.organization_id=$1").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(applied, vec![("Safe".into(), "independent value".into())]);
    let native_after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(f) FROM custom_field f WHERE id=$1 AND organization_id=$2",
    )
    .bind(target)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(native_after, native_before);
    assert_eq!(native_history(&f, parent).await, before);
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_acceptance_existing_different_fub_binding_cannot_be_reassigned(migrator: PgPool) {
    let mut p = person(101);
    p["customIncoming"] = json!("must not replace binding");
    p["customSafe"] = json!("independent value");
    let f = source_fixture(
        &migrator,
        vec![p],
        vec![
            field(21, "customIncoming", "Bound field"),
            field(22, "customSafe", "Safe"),
        ],
    )
    .await;
    let target = native_field(&f, "Bound field").await;
    // Native create commands do not grant source provenance. This explicit
    // migrator-only legacy fixture models a field already bound elsewhere.
    sqlx::query("UPDATE custom_field SET source='fub',external_key='customOther' WHERE id=$1 AND organization_id=$2").bind(target).bind(f.org).execute(&migrator).await.unwrap();
    let bound: Value = sqlx::query_scalar(
        "SELECT to_jsonb(f) FROM custom_field f WHERE id=$1 AND organization_id=$2",
    )
    .bind(target)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let parent = completed_parent(&f).await;
    let calls = f.reader.calls();
    let (child, first) = propose(&f, parent).await;
    let incoming = field_mapping(&f, plan(&first), "21").await;
    assert!(incoming.get::<bool, _>("qualified"));
    let rejected = replan(
        &f,
        child,
        &first,
        vec![patch(
            &incoming,
            json!({"kind":"map_existing","target_id":target}),
        )],
    )
    .await;
    assert!(matches!(rejected, Err(MigrationError::InvalidImportChoice)));
    assert_eq!(
        detail(&f, child).await["latest_plan"]["revision"],
        first["latest_plan"]["revision"]
    );
    let safe = field_mapping(&f, plan(&first), "22").await;
    replan(
        &f,
        child,
        &first,
        vec![patch(&safe, json!({"kind":"create_matching"}))],
    )
    .await
    .unwrap();
    drain(&f).await;
    let ready = detail(&f, child).await;
    execute(&f, child, &ready).await;
    let after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(f) FROM custom_field f WHERE id=$1 AND organization_id=$2",
    )
    .bind(target)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(after, bound);
    let rows:Vec<(String,String)>=sqlx::query_as("SELECT f.external_key,v.text_value FROM person_custom_field_value v JOIN custom_field f ON f.id=v.field_id AND f.organization_id=v.organization_id WHERE v.organization_id=$1").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        rows,
        vec![("customSafe".into(), "independent value".into())]
    );
    assert_eq!(
        native(&f).await,
        json!({"people":1,"tags":0,"fields":2,"links":0,"values":1})
    );
    assert_eq!(f.reader.calls(), calls);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_acceptance_tombstoned_parent_person_cannot_regain_metadata_or_history(
    migrator: PgPool,
) {
    let mut erased = person(101);
    erased["tags"] = json!(["Shared"]);
    erased["customKeep"] = json!("erased source value");
    let mut live = person(102);
    live["tags"] = json!(["Shared"]);
    live["customKeep"] = json!("live source value");
    let f = source_fixture(
        &migrator,
        vec![erased, live],
        vec![field(21, "customKeep", "Keep")],
    )
    .await;
    let parent = completed_parent(&f).await;
    let erased = imported_person(&f, parent, "101").await;
    let live = imported_person(&f, parent, "102").await;
    // Deliberate erasure fixture. Parent results/identity tombstones are retained;
    // application mutation commands cannot erase a Person in migration review.
    assert_eq!(
        sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
            .bind(erased)
            .bind(f.org)
            .execute(&migrator)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
    let before = native_history(&f, parent).await;
    assert_eq!(before["person"].as_array().unwrap().len(), 1);
    assert_eq!(before["inquiry"], json!([]));
    assert_eq!(before["person_imported"].as_array().unwrap().len(), 2);
    let calls = f.reader.calls();
    let (child, first) = propose(&f, parent).await;
    let ready = approve_all(&f, child, &first).await;
    let held=sqlx::query("SELECT * FROM migration_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id='101'").bind(plan(&ready)).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(held.get::<String, _>("disposition"), "held");
    assert!(held.get::<Option<Uuid>, _>("person_id").is_none());
    let evidence = open(&f, plan(&ready), &held, "manifest");
    reason(&evidence, "parent_person_unavailable");
    assert_eq!(evidence["operations"], json!([]));
    assert_eq!(ready["latest_plan"]["counts"]["people"]["excluded"], "1");
    execute(&f, child, &ready).await;
    assert_eq!(
        native_history(&f, parent).await,
        before,
        "child must not rewrite People, synthesize Inquiry or touch immutable history/parent state"
    );
    let result=sqlx::query("SELECT * FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 AND kind='people' AND source_id='101'").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(result.get::<String, _>("disposition"), "held");
    assert!(result.get::<Option<Uuid>, _>("person_id").is_none());
    reason(
        &open(&f, plan(&ready), &result, "result"),
        "parent_person_unavailable",
    );
    let destinations:Vec<Uuid>=sqlx::query_scalar("SELECT person_id FROM person_tag WHERE organization_id=$1 UNION ALL SELECT person_id FROM person_custom_field_value WHERE organization_id=$1").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(destinations, vec![live, live]);
    let target:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND import_id=$2 AND family='people' AND source_id='101'").bind(f.org).bind(parent).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        target, erased,
        "identity tombstone stays bound to the erased target"
    );
    assert!(matches!(
        metadata::provenance(
            &f.pool,
            &f.key,
            &f.ctx,
            erased,
            metadata::MetadataPage::default()
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    assert_eq!(
        native(&f).await,
        json!({"people":1,"tags":1,"fields":1,"links":1,"values":1})
    );
    assert_eq!(f.reader.calls(), calls);
}

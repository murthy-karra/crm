//! Retained-source metadata acceptance through completed 010c workspaces.
//! TRUST/BOUNDARY: fixtures capture through the real source pipeline. Migrator
//! writes below explicitly corrupt evidence or install otherwise unreachable
//! conflicting local data; normal commands never bypass a held workspace.
use std::sync::Arc;

use crate::import_support::{self as support, Book, Fixture};
use crm_api::domain::{
    custom_field::{self, CreateCustomField, FieldType},
    migration::{
        crypto,
        imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        metadata, metadata_worker,
        snapshot_source::Stream,
        MigrationError,
    },
    tag::{self, CreateTag},
};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

fn person(id: u64) -> Value {
    json!({"id":id,"firstName":"Synthetic metadata Person","stage":"Lead","assignedUserId":3})
}

fn field(id: u64, name: &str, label: &str, kind: &str) -> Value {
    json!({"id":id,"name":name,"label":label,"type":kind,"isRecurring":false})
}

fn book(people: Vec<Value>, fields: Vec<Value>) -> Arc<Book> {
    let reader = Arc::new(Book::new(people));
    reader.set_records(Stream::CustomFields, fields);
    reader
}

async fn complete_parent(f: &Fixture) -> Uuid {
    let calls = f.reader.calls();
    let (id, _) = support::propose(f).await;
    support::drain_import(f).await;
    support::replan(
        f,
        id,
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
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap();
    let command = json!({"request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],
        "plan_revision":ready["plan"]["revision"],"confirmation_digest":ready["plan"]["confirmation_digest"],
        "acknowledgments":{"held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}});
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(command).unwrap(),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    support::drain_import(f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(f.reader.calls(), calls);
    id
}

async fn drain(f: &Fixture) {
    let calls = f.reader.calls();
    for _ in 0..2_000 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            assert_eq!(
                f.reader.calls(),
                calls,
                "metadata work must remain source-free"
            );
            return;
        }
    }
    panic!("synthetic metadata exceeded bounded fixture work");
}

async fn detail(f: &Fixture, id: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap()
}

fn plan_id(detail: &Value) -> Uuid {
    Uuid::parse_str(detail["latest_plan"]["id"].as_str().unwrap()).unwrap()
}

async fn propose(f: &Fixture, parent: Uuid) -> (Uuid, Value) {
    let calls = f.reader.calls();
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
    let id = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    drain(f).await;
    let value = detail(f, id).await;
    assert_eq!(value["latest_plan"]["state"], "ready");
    assert_eq!(value["parent_import_id"], parent.to_string());
    assert_eq!(f.reader.calls(), calls);
    (id, value)
}

async fn patch(
    f: &Fixture,
    id: Uuid,
    ready: &Value,
    mappings: Vec<Value>,
) -> Result<Value, MigrationError> {
    metadata::replan(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(json!({"request_id":Uuid::new_v4(),
            "expected_plan_revision":ready["latest_plan"]["revision"],"mappings":mappings}))
        .unwrap(),
        &f.policy,
    )
    .await
}

async fn replan(f: &Fixture, id: Uuid, ready: &Value, mappings: Vec<Value>) -> Value {
    patch(f, id, ready, mappings).await.unwrap();
    drain(f).await;
    let value = detail(f, id).await;
    assert_eq!(value["latest_plan"]["state"], "ready");
    value
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

async fn mappings(f: &Fixture, plan: Uuid) -> Vec<(PgRow, Value)> {
    sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 ORDER BY source_sequence,source_ordinal,element_ordinal,id")
        .bind(plan).bind(f.org).fetch_all(&f.pool).await.unwrap().into_iter()
        .map(|row| { let value = open(f, plan, &row, "mapping"); (row, value) }).collect()
}

fn choice_patch(row: &PgRow, choice: Value) -> Value {
    json!({"mapping_id":row.get::<Uuid,_>("id"),"choice":choice})
}

/// A suggestion never grants permission: enumerate and explicitly approve only
/// source-qualified, representable catalog items in these synthetic books.
async fn matching_choices(f: &Fixture, plan: Uuid) -> Vec<Value> {
    mappings(f, plan)
        .await
        .iter()
        .filter(|(row, data)| {
            row.get::<bool, _>("qualified")
                && data["label"].is_string()
                && (!data["field"].is_object()
                    || data["field"]["creation_reasons"]
                        .as_array()
                        .is_some_and(Vec::is_empty))
        })
        .map(|(row, _)| choice_patch(row, json!({"kind":"create_matching"})))
        .collect()
}

async fn field_mapping(f: &Fixture, plan: Uuid, source_id: &str) -> PgRow {
    sqlx::query("SELECT m.* FROM migration_metadata_mapping m JOIN migration_metadata_source s ON s.id=m.source_row_id AND s.organization_id=m.organization_id AND s.plan_id=m.plan_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind='field' AND s.source_id=$3")
        .bind(plan).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap()
}

async fn source(f: &Fixture, plan: Uuid, family: &str, source_id: &str) -> PgRow {
    sqlx::query("SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4")
        .bind(plan).bind(f.org).bind(family).bind(source_id).fetch_one(&f.pool).await.unwrap()
}

async fn manifest(f: &Fixture, plan: Uuid, source_id: &str) -> Value {
    let row = sqlx::query("SELECT * FROM migration_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id=$3")
        .bind(plan).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap();
    open(f, plan, &row, "manifest")
}

fn operation<'a>(manifest: &'a Value, source_field: &str) -> &'a Value {
    manifest["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["source_field"] == source_field)
        .expect("field operation must reconcile")
}

async fn parent_state(f: &Fixture, parent: Uuid) -> Value {
    // Includes original plan/binding, byte counters, identities and exact result
    // ciphertext. All fixtures are synthetic; these values never leave tests.
    sqlx::query_scalar("SELECT jsonb_build_object('import',to_jsonb(i),'workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE w.organization_id=i.organization_id),'results',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM migration_import_result r WHERE r.import_id=i.id AND r.organization_id=i.organization_id),'identities',(SELECT jsonb_agg(to_jsonb(k) ORDER BY k.family,k.source_id) FROM migration_import_identity k WHERE k.import_id=i.id AND k.organization_id=i.organization_id)) FROM migration_import i WHERE i.id=$1 AND i.organization_id=$2")
        .bind(parent).bind(f.org).fetch_one(&f.pool).await.unwrap()
}

async fn execute(f: &Fixture, id: Uuid, ready: &Value, parent: Uuid) -> Value {
    let calls = f.reader.calls();
    let before = parent_state(f, parent).await;
    let command = json!({"request_id":Uuid::new_v4(),"plan_id":ready["latest_plan"]["id"],
        "plan_revision":ready["latest_plan"]["revision"],"confirmation_digest":ready["latest_plan"]["confirmation_digest"],
        "workspace_revision":ready["workspace_revision"],"acknowledgments":{
            "held_count":ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}});
    metadata::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(command).unwrap(),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    drain(f).await;
    let value = detail(f, id).await;
    assert_eq!(value["state"], "completed");
    assert_eq!(
        parent_state(f, parent).await,
        before,
        "child must not mutate the parent or workspace binding"
    );
    assert_eq!(f.reader.calls(), calls);
    value
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

async fn imported_person(f: &Fixture, parent: Uuid, source_id: &str) -> Uuid {
    sqlx::query_scalar("SELECT person_id FROM migration_import_result WHERE import_id=$1 AND organization_id=$2 AND source_id=$3 AND disposition='imported'")
        .bind(parent).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_requalifies_people_and_definitions_after_completed_parent(
    migrator: PgPool,
) {
    for family in ["people", "custom_fields"] {
        let mut p = person(101);
        p["tags"] = json!(["Qualified Tag"]);
        p["customA"] = json!("source");
        let f = support::fixture_with_book(
            &migrator,
            book(vec![p], vec![field(10, "customA", "A", "text")]),
        )
        .await;
        let parent = complete_parent(&f).await;
        let frozen = parent_state(&f, parent).await;
        let (child, mut ready) = propose(&f, parent).await;
        assert_eq!(ready["coverage"]["metadata_excluded_people"], "0");
        assert!(ready["coverage"].get("parent_excluded_people").is_none());
        let observation = sqlx::query("SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family=$3")
            .bind(f.snapshot).bind(f.org).bind(family).fetch_one(&migrator).await.unwrap();
        let record_id: Uuid = observation.get("id");
        let capture_id: Uuid = observation.get("capture_id");
        let capture = sqlx::query("SELECT * FROM migration_snapshot_capture WHERE id=$1")
            .bind(capture_id)
            .fetch_one(&migrator)
            .await
            .unwrap();
        for case in [
            "ordinal",
            "semantic_hmac",
            "record_representation",
            "capture_representation",
            "status",
            "truncated",
            "accepted",
            "raw_length",
            "raw_hmac",
        ] {
            sqlx::query("UPDATE migration_snapshot_record SET ordinal=$2,representation=$3,semantic_hmac=$4 WHERE id=$1")
                .bind(record_id).bind(observation.get::<i32,_>("ordinal")).bind(observation.get::<String,_>("representation")).bind(observation.get::<Vec<u8>,_>("semantic_hmac")).execute(&migrator).await.unwrap();
            sqlx::query("UPDATE migration_snapshot_capture SET representation=$2,http_status=$3,truncated=$4,accepted=$5,classification=$6,raw_byte_len=$7,nonce=$8,ciphertext=$9 WHERE id=$1")
                .bind(capture_id).bind(capture.get::<String,_>("representation")).bind(capture.get::<i32,_>("http_status"))
                .bind(capture.get::<bool,_>("truncated")).bind(capture.get::<bool,_>("accepted")).bind(capture.get::<String,_>("classification"))
                .bind(capture.get::<i64,_>("raw_byte_len")).bind(capture.get::<Vec<u8>,_>("nonce")).bind(capture.get::<Vec<u8>,_>("ciphertext")).execute(&migrator).await.unwrap();
            let mutation = match case {
                "ordinal" => Some(("UPDATE migration_snapshot_record SET ordinal=99 WHERE id=$1",record_id)),
                "semantic_hmac" => Some(("UPDATE migration_snapshot_record SET semantic_hmac=decode(repeat('00',32),'hex') WHERE id=$1",record_id)),
                "record_representation" => Some(("UPDATE migration_snapshot_record SET representation='unqualified' WHERE id=$1",record_id)),
                "capture_representation" => Some(("UPDATE migration_snapshot_capture SET representation='unqualified' WHERE id=$1",capture_id)),
                "status" => Some(("UPDATE migration_snapshot_capture SET http_status=403 WHERE id=$1",capture_id)),
                "truncated" => Some(("UPDATE migration_snapshot_capture SET truncated=true WHERE id=$1",capture_id)),
                "accepted" => Some(("UPDATE migration_snapshot_capture SET accepted=false,classification='no_progress' WHERE id=$1",capture_id)),
                "raw_length" => Some(("UPDATE migration_snapshot_capture SET raw_byte_len=raw_byte_len+1 WHERE id=$1",capture_id)),
                _ => None,
            };
            if let Some((sql, id)) = mutation {
                sqlx::query(sql).bind(id).execute(&migrator).await.unwrap();
            } else {
                let raw = if family == "people" {
                    br#"{"people":[{"id":101,"firstName":"Changed retained data","stage":"Lead","tags":["Qualified Tag"]}]}"#.as_slice()
                } else {
                    br#"{"customfields":[{"id":10,"name":"customA","label":"A","type":"text","unknown":true}]}"#.as_slice()
                };
                let sealed = crypto::seal_snapshot(
                    &f.key,
                    f.ctx.organization_id,
                    f.snapshot,
                    capture_id,
                    "capture",
                    raw,
                )
                .unwrap();
                sqlx::query("UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3,raw_byte_len=$4 WHERE id=$1")
                    .bind(capture_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(raw.len() as i64).execute(&migrator).await.unwrap();
            }
            ready = replan(&f, child, &ready, vec![]).await;
            assert_eq!(
                ready["coverage"]["metadata_excluded_people"],
                ready["latest_plan"]["counts"]["people"]["excluded"]
            );
            assert_eq!(ready["coverage"]["metadata_excluded_people"], if family == "people" { "1" } else { "0" }, "child source requalification is included, definition-only holds are not Person exclusions");
            assert!(ready["coverage"].get("parent_excluded_people").is_none());
            let source = source(
                &f,
                plan_id(&ready),
                family,
                if family == "people" { "101" } else { "10" },
            )
            .await;
            assert!(
                !source.get::<bool, _>("qualified") || source.get::<bool, _>("conflict"),
                "{family}/{case} cannot qualify"
            );
            let writes: i64 =
                sqlx::query_scalar("SELECT count(*) FROM person_tag WHERE organization_id=$1")
                    .bind(f.org)
                    .fetch_one(&f.pool)
                    .await
                    .unwrap();
            assert_eq!(writes, 0);
            assert_eq!(parent_state(&f, parent).await, frozen);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_all_variants_and_exact_raw_identity_override_preview(migrator: PgPool) {
    let reader = Arc::new(Book::new(vec![]));
    reader.set_raw(Stream::People,0,200,br#"{"_metadata":{"collection":"people","limit":100,"offset":0,"total":2},"people":[{"id":9007199254740993,"firstName":"Exact","stage":"Lead","assignedUserId":3,"tags":["Shared"],"customExact":"from raw","unknown":1},{"unknown":1.00,"customExact":"from raw","tags":["Shared"],"assignedUserId":3,"stage":"Lead","firstName":"Exact","id":9007199254740993}]}"#.to_vec(),false);
    reader.set_raw(Stream::CustomFields,0,200,br#"{"_metadata":{"collection":"customfields","limit":100,"offset":0,"total":5},"customfields":[{"id":18446744073709551617,"name":"customExact","label":"Exact","type":"text","unknown":123456789012345678901234567890},{"id":18446744073709551617,"name":"customExact","label":"Exact","type":"text","unknown":123456789012345678901234567890.0},{"id":20,"name":"customConflict","label":"Conflict","type":"text","unknown":false},{"id":20,"name":"customConflict","label":"Conflict","type":"text","unknown":false},{"id":20,"name":"customConflict","label":"Conflict","type":"text","unknown":true}]}"#.to_vec(),false);
    let f = support::fixture_with_book(&migrator, reader).await;
    let parent = complete_parent(&f).await;
    // A clipped projection is display-only, including its apparent source key.
    let rows = sqlx::query("SELECT id FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family='custom_fields'").bind(f.snapshot).bind(f.org).fetch_all(&migrator).await.unwrap();
    for row in rows {
        let id: Uuid = row.get("id");
        let sealed = crypto::seal_snapshot(
            &f.key,
            f.ctx.organization_id,
            f.snapshot,
            id,
            "record",
            br#"{"name":"WRONG","type":"number"}"#,
        )
        .unwrap();
        sqlx::query("UPDATE migration_snapshot_record SET projection_nonce=$2,projection_ciphertext=$3 WHERE id=$1").bind(id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&migrator).await.unwrap();
    }
    let (child, first) = propose(&f, parent).await;
    let exact = source(&f, plan_id(&first), "custom_fields", "18446744073709551617").await;
    assert!(exact.get::<bool, _>("qualified"));
    assert!(!exact.get::<bool, _>("conflict"));
    assert_eq!(exact.get::<i64, _>("observations"), 2);
    let evidence = open(&f, plan_id(&first), &exact, "source");
    assert_eq!(
        evidence["provenance"]["unknown"],
        "12345678901234567890123456789e1"
    );
    assert_eq!(evidence["entity"]["value"]["name"], "customExact");
    assert_eq!(evidence["canonical"], json!([]));
    let conflict = source(&f, plan_id(&first), "custom_fields", "20").await;
    assert_eq!(conflict.get::<i64, _>("observations"), 3);
    assert!(conflict.get::<bool, _>("conflict"));
    let ready = replan(
        &f,
        child,
        &first,
        matching_choices(&f, plan_id(&first)).await,
    )
    .await;
    execute(&f, child, &ready, parent).await;
    let stored: String = sqlx::query_scalar("SELECT v.text_value FROM person_custom_field_value v JOIN custom_field c ON c.id=v.field_id AND c.organization_id=v.organization_id WHERE v.organization_id=$1 AND c.external_key='customExact'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(stored, "from raw");
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT external_key FROM custom_field WHERE organization_id=$1 AND source='fub'",
    )
    .bind(f.org)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(names, vec!["customExact"]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_machine_key_collisions_hold_while_equal_labels_map_explicitly(
    migrator: PgPool,
) {
    let mut p = person(101);
    p["customCollision"] = json!("ambiguous");
    p["customLeft"] = json!("left");
    p["customRight"] = json!("right");
    p["customUndeclared"] = json!("retained only");
    let f = support::fixture_with_book(
        &migrator,
        book(
            vec![p],
            vec![
                field(10, "customCollision", "Different label A", "text"),
                field(11, "customCollision", "Different label B", "text"),
                field(12, "customLeft", "Same label", "text"),
                field(13, "customRight", "Same label", "text"),
            ],
        ),
    )
    .await;
    let left = native_field(&f, "Native left").await;
    let right = native_field(&f, "Native right").await;
    let parent = complete_parent(&f).await;
    let (child, first) = propose(&f, parent).await;
    for id in ["10", "11"] {
        let row = field_mapping(&f, plan_id(&first), id).await;
        let invalid = choice_patch(&row, json!({"kind":"map_existing","target_id":left}));
        assert!(matches!(
            patch(&f, child, &first, vec![invalid]).await,
            Err(MigrationError::InvalidImportChoice | MigrationError::SourceNotEligible)
        ));
    }
    let patches = vec![
        choice_patch(
            &field_mapping(&f, plan_id(&first), "12").await,
            json!({"kind":"map_existing","target_id":left}),
        ),
        choice_patch(
            &field_mapping(&f, plan_id(&first), "13").await,
            json!({"kind":"map_existing","target_id":right}),
        ),
    ];
    let ready = replan(&f, child, &first, patches).await;
    let items = manifest(&f, plan_id(&ready), "101").await;
    assert!(items["operations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|op| op["source_field"] == "customCollision" && op["disposition"] == "held"));
    execute(&f, child, &ready, parent).await;
    let rows = sqlx::query(
        "SELECT field_id,text_value FROM person_custom_field_value WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(
        |r| r.get::<Uuid, _>("field_id") == left && r.get::<String, _>("text_value") == "left"
    ));
    assert!(rows
        .iter()
        .any(|r| r.get::<Uuid, _>("field_id") == right
            && r.get::<String, _>("text_value") == "right"));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM custom_field WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_large_and_nul_keys_can_map_without_native_key_coercion(migrator: PgPool) {
    let long = "é".repeat(1025);
    let nul = "custom\0Value";
    let mut p = person(101);
    p[&long] = json!("large-key value");
    p[nul] = json!("nul-key value");
    let f = support::fixture_with_book(
        &migrator,
        book(
            vec![p],
            vec![
                field(10, &long, &"x".repeat(61), "text"),
                field(11, nul, "NUL key", "text"),
            ],
        ),
    )
    .await;
    let left = native_field(&f, "Explicit large-key destination").await;
    let right = native_field(&f, "Explicit NUL-key destination").await;
    let parent = complete_parent(&f).await;
    let (child, first) = propose(&f, parent).await;
    for id in ["10", "11"] {
        let row = field_mapping(&f, plan_id(&first), id).await;
        assert!(
            row.get::<bool, _>("qualified"),
            "native key fit does not invalidate source identity"
        );
        let response = patch(
            &f,
            child,
            &first,
            vec![choice_patch(&row, json!({"kind":"create_matching"}))],
        )
        .await;
        assert!(matches!(
            response,
            Err(MigrationError::InvalidImportChoice | MigrationError::SourceNotEligible)
        ));
        assert_eq!(
            detail(&f, child).await["latest_plan"]["revision"],
            first["latest_plan"]["revision"]
        );
    }
    let ready = replan(
        &f,
        child,
        &first,
        vec![
            choice_patch(
                &field_mapping(&f, plan_id(&first), "10").await,
                json!({"kind":"map_existing","target_id":left}),
            ),
            choice_patch(
                &field_mapping(&f, plan_id(&first), "11").await,
                json!({"kind":"map_existing","target_id":right}),
            ),
        ],
    )
    .await;
    execute(&f, child, &ready, parent).await;
    let values: Vec<String> = sqlx::query_scalar("SELECT text_value FROM person_custom_field_value WHERE organization_id=$1 ORDER BY text_value").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(values, vec!["large-key value", "nul-key value"]);
    let bound: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM custom_field WHERE organization_id=$1 AND source IS NOT NULL",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(bound, 0, "mapping preserves existing provenance pairs");
    let evidence = source(&f, plan_id(&ready), "people", "101").await;
    let evidence = open(&f, plan_id(&ready), &evidence, "source");
    assert_eq!(evidence["provenance"][&long], "\"large-key value\"");
    assert_eq!(evidence["provenance"][nul], "\"nul-key value\"");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_native_values_preserve_precision_absence_literals_and_recurrence_holds(
    migrator: PgPool,
) {
    let reader = Arc::new(Book::new(vec![]));
    reader.set_raw(Stream::People,0,200,br#"{"_metadata":{"collection":"people","limit":100,"offset":0,"total":5},"people":[{"id":1,"firstName":"Exact decimal","stage":"Lead","assignedUserId":3,"customNumber":999999999999999.9999,"customChoice":"None","customDate":"2000-01-01"},{"id":2,"firstName":"Numeric string","stage":"Lead","assignedUserId":3,"customNumber":"12.34","customChoice":"null"},{"id":3,"firstName":"Source null","stage":"Lead","assignedUserId":3,"customNumber":null,"customChoice":"N/A"},{"id":4,"firstName":"Missing number","stage":"Lead","assignedUserId":3,"customChoice":"None"},{"id":5,"firstName":"Negative zero","stage":"Lead","assignedUserId":3,"customNumber":-0.0000,"customChoice":"unknown"}]}"#.to_vec(),false);
    reader.set_records(Stream::CustomFields,vec![field(10,"customNumber","Number","number"),
        json!({"id":11,"name":"customChoice","label":"Choice","type":"dropdown","choices":["None","null","N/A"]}),
        json!({"id":12,"name":"customDate","label":"Recurring date","type":"date","isRecurring":true})]);
    let f = support::fixture_with_book(&migrator, reader).await;
    let parent = complete_parent(&f).await;
    let (child, first) = propose(&f, parent).await;
    let ready = replan(
        &f,
        child,
        &first,
        matching_choices(&f, plan_id(&first)).await,
    )
    .await;
    for (person, expected) in [("2", "held"), ("3", "source_null"), ("4", "not_supplied")] {
        assert_eq!(
            operation(&manifest(&f, plan_id(&ready), person).await, "customNumber")["disposition"],
            expected
        );
    }
    assert_eq!(
        operation(&manifest(&f, plan_id(&ready), "1").await, "customDate")["disposition"],
        "held"
    );
    execute(&f, child, &ready, parent).await;
    let numbers: Vec<String> = sqlx::query_scalar("SELECT v.number_value::text FROM person_custom_field_value v JOIN custom_field c ON c.id=v.field_id AND c.organization_id=v.organization_id WHERE v.organization_id=$1 AND c.external_key='customNumber' ORDER BY number_value").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(numbers, vec!["0.0000", "999999999999999.9999"]);
    let labels: Vec<String> = sqlx::query_scalar("SELECT o.label FROM person_custom_field_value v JOIN custom_field_option o ON o.id=v.option_id AND o.field_id=v.field_id AND o.organization_id=v.organization_id WHERE v.organization_id=$1 ORDER BY o.label").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(labels, vec!["N/A", "None", "None", "null"]);
    let dates: i64 = sqlx::query_scalar("SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1 AND date_value IS NOT NULL").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(dates, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_shared_tags_do_not_merge_people_and_overflow_holds_the_whole_link_set(
    migrator: PgPool,
) {
    let mut people = vec![person(1), person(2), person(3)];
    for (index, p) in people.iter_mut().enumerate() {
        p["customText"] = json!(format!("value {index}"));
        p["emails"] = json!([{"value":"shared@metadata.synthetic"}]);
    }
    people[0]["tags"] = json!([" Shared ", "Shared", false]);
    people[1]["tags"] = json!(["shared"]);
    people[2]["tags"] = json!((0..21).map(|i| format!("Overflow {i}")).collect::<Vec<_>>());
    let f = support::fixture_with_book(
        &migrator,
        book(people, vec![field(10, "customText", "Text", "text")]),
    )
    .await;
    let parent = complete_parent(&f).await;
    let (child, first) = propose(&f, parent).await;
    let ready = replan(
        &f,
        child,
        &first,
        matching_choices(&f, plan_id(&first)).await,
    )
    .await;
    let overflow = manifest(&f, plan_id(&ready), "3").await;
    let links: Vec<_> = overflow["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["kind"] == "tag_link")
        .collect();
    assert_eq!(links.len(), 21);
    assert!(links.iter().all(|op| op["disposition"] == "held"));
    assert_eq!(
        operation(&overflow, "customText")["disposition"],
        "eligible"
    );
    execute(&f, child, &ready, parent).await;
    let first_person = imported_person(&f, parent, "1").await;
    let second_person = imported_person(&f, parent, "2").await;
    assert_ne!(first_person, second_person);
    let rows=sqlx::query("SELECT p.person_id,t.id,t.name FROM person_tag p JOIN tag t ON t.id=p.tag_id AND t.organization_id=p.organization_id WHERE p.organization_id=$1").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<Uuid, _>("id"), rows[1].get::<Uuid, _>("id"));
    assert!(rows
        .iter()
        .all(|row| row.get::<String, _>("name") == "Shared"));
    let values: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(values, 3);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_capacity_archival_and_local_conflicts_preserve_native_rows(
    migrator: PgPool,
) {
    let mut p = person(101);
    p["tags"] = json!(["Existing tag 0", "Overflow tag"]);
    p["customConflict"] = json!("source wants replacement");
    p["customEqual"] = json!("same");
    p["customAbsent"] = json!("inserted");
    p["customNew"] = json!("over quota");
    p["customArchived"] = json!("do not revive");
    let f = support::fixture_with_book(
        &migrator,
        book(
            vec![p],
            vec![
                field(10, "customConflict", "Existing field 0", "text"),
                field(11, "customEqual", "Existing field 1", "text"),
                field(12, "customAbsent", "Existing field 2", "text"),
                field(13, "customNew", "Overflow field", "text"),
                field(14, "customArchived", "Archived field", "text"),
            ],
        ),
    )
    .await;
    let mut fields = vec![];
    for n in 0..50 {
        fields.push(native_field(&f, &format!("Existing field {n}")).await);
    }
    for n in 0..200 {
        tag::create_tag(
            &f.pool,
            &f.ctx,
            CreateTag {
                name: format!("Existing tag {n}"),
            },
        )
        .await
        .unwrap();
    }
    let parent = complete_parent(&f).await;
    let person = imported_person(&f, parent, "101").await;
    // Explicit negative fixtures: ordinary app commands cannot perform these
    // writes after review begins. They model legacy/conflicting native state.
    let archived = Uuid::new_v4();
    sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id,archived_at) VALUES($1,$2,'Archived field','text',51,$3,now())").bind(archived).bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
    for (field, value) in [(fields[0], "different local value"), (fields[1], "same")] {
        sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'text',$4,$5,'web_session',$6)").bind(f.org).bind(person).bind(field).bind(value).bind(f.actor).bind(Uuid::new_v4()).execute(&migrator).await.unwrap();
    }
    let before:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(v) ORDER BY field_id) FROM person_custom_field_value v WHERE organization_id=$1").bind(f.org).fetch_one(&f.pool).await.unwrap();
    let (child, first) = propose(&f, parent).await;
    let invalid = choice_patch(
        &field_mapping(&f, plan_id(&first), "14").await,
        json!({"kind":"map_existing","target_id":archived}),
    );
    assert!(matches!(
        patch(&f, child, &first, vec![invalid]).await,
        Err(MigrationError::InvalidImportChoice | MigrationError::SourceNotEligible)
    ));
    let mut patches = matching_choices(&f, plan_id(&first)).await;
    for (id, target) in [("10", fields[0]), ("11", fields[1]), ("12", fields[2])] {
        let row = field_mapping(&f, plan_id(&first), id).await;
        let mapping = row.get::<Uuid, _>("id").to_string();
        patches.retain(|patch| patch["mapping_id"] != mapping);
        patches.push(choice_patch(
            &row,
            json!({"kind":"map_existing","target_id":target}),
        ));
    }
    let ready = replan(&f, child, &first, patches).await;
    let operations = manifest(&f, plan_id(&ready), "101").await;
    assert_eq!(
        operation(&operations, "customConflict")["disposition"],
        "held"
    );
    assert_eq!(
        operation(&operations, "customEqual")["disposition"],
        "already_present"
    );
    assert_eq!(
        operation(&operations, "customAbsent")["disposition"],
        "eligible"
    );
    assert_eq!(operation(&operations, "customNew")["disposition"], "held");
    execute(&f, child, &ready, parent).await;
    let after:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(v) ORDER BY field_id) FROM person_custom_field_value v WHERE organization_id=$1 AND field_id=ANY($2)").bind(f.org).bind(&fields[..2]).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        after, before,
        "equal/differing existing values are never updated"
    );
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM tag WHERE organization_id=$1),(SELECT count(*) FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL),(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts, (200, 50, 3));
    let remains_archived: bool = sqlx::query_scalar(
        "SELECT archived_at IS NOT NULL FROM custom_field WHERE id=$1 AND organization_id=$2",
    )
    .bind(archived)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(remains_archived);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_source_full_multimegabyte_evidence_survives_execution_without_truncation(
    migrator: PgPool,
) {
    let original = format!("{}🙂 exact end", "x".repeat(2 * 1024 * 1024 + 17));
    let mut p = person(101);
    p["unknownLarge"] = json!(original);
    p["customText"] = json!("native text");
    p["tags"] = json!(["Evidence"]);
    let f = support::fixture_with_book(
        &migrator,
        book(vec![p], vec![field(10, "customText", "Text", "text")]),
    )
    .await;
    let parent = complete_parent(&f).await;
    let (child, first) = propose(&f, parent).await;
    let ready = replan(
        &f,
        child,
        &first,
        matching_choices(&f, plan_id(&first)).await,
    )
    .await;
    let record = source(&f, plan_id(&ready), "people", "101").await;
    assert!(record.get::<Vec<u8>, _>("ciphertext").len() > 2 * 1024 * 1024);
    let raw = open(&f, plan_id(&ready), &record, "source");
    assert_eq!(
        raw["provenance"]["unknownLarge"],
        serde_json::to_string(&original).unwrap()
    );
    assert_eq!(raw["canonical"], json!([]));
    execute(&f, child, &ready, parent).await;
    let row=sqlx::query("SELECT * FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 AND kind='people' AND source_id='101'").bind(child).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let result = open(&f, plan_id(&ready), &row, "result");
    assert_eq!(
        result["source"]["unknownLarge"],
        serde_json::to_string(&original).unwrap()
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1 AND text_value='native text'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, 1);
}

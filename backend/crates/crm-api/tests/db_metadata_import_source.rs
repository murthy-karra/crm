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
    crate::db_family_refresh::assert_metadata_after_states(f, id, false).await;
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

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn family_refresh_metadata_original_discovery_is_bound_and_conservative(migrator: PgPool) {
    use crm_api::domain::migration::family_refresh::{
        metadata_discovery::{self, Discovery},
        model::Hold,
    };
    let mut p = person(101);
    p["tags"] = json!(["Imported"]);
    p["customText"] = json!("Native baseline");
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
    execute(&f, child, &ready, parent).await;
    let claim =
        crate::db_family_refresh::prepared_family_refresh(&migrator, &f, parent, "metadata").await;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&f.pool).await.unwrap();
    let proven = match metadata_discovery::discover(&f.pool, &f.key, &claim, cohort)
        .await
        .unwrap()
    {
        Discovery::Proven(p) => p,
        Discovery::Held(h) => panic!("unexpected hold: {h:?}"),
    };
    assert_eq!(proven.ownership.tags.len(), 1);
    assert_eq!(proven.ownership.fields.len(), 1);
    assert!(
        metadata_discovery::discover(&f.pool, &f.key, &claim, Uuid::new_v4())
            .await
            .is_err()
    );
    // Migrator simulates a forbidden local edit and revert, preserving row value
    // but advancing the database-owned metadata revision twice.
    let target = *proven.ownership.fields.iter().next().unwrap();
    let mut tx = migrator.begin().await.unwrap();
    sqlx::query("UPDATE person_custom_field_value SET text_value='local edit' WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(f.org).bind(proven.baseline.person).bind(target).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE person_custom_field_value SET text_value='Native baseline' WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(f.org).bind(proven.baseline.person).bind(target).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        metadata_discovery::discover(&f.pool, &f.key, &claim, cohort)
            .await
            .unwrap(),
        Discovery::Held(Hold::LocalChange)
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn family_refresh_metadata_prior_result_preserves_ownership_and_source_boundary(
    migrator: PgPool,
) {
    use crm_api::domain::migration::family_refresh::{
        metadata_discovery::{self, Discovery},
        model::Hold,
        native_baseline::{AfterState, State},
    };
    let mut p = person(101);
    p["tags"] = json!(["Imported"]);
    p["customText"] = json!("Native baseline");
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
    execute(&f, child, &ready, parent).await;
    let claim =
        crate::db_family_refresh::prepared_family_refresh(&migrator, &f, parent, "metadata").await;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&f.pool).await.unwrap();
    let first = match metadata_discovery::discover(&f.pool, &f.key, &claim, cohort)
        .await
        .unwrap()
    {
        Discovery::Proven(p) => p,
        Discovery::Held(h) => panic!("{h:?}"),
    };
    let person = first.baseline.person;
    let field = *first.ownership.fields.iter().next().unwrap();
    let proof = AfterState {
        version: 1,
        manifest: Uuid::new_v4(),
        source_id: "101".into(),
        person,
        target: person,
        state: State::Metadata {
            snapshot: first.baseline.clone(),
            ownership: first.ownership.clone(),
        },
    };
    let (next, cohort, results) =
        crate::db_family_refresh::prior_native_fixture(&migrator, &f, parent, &claim, vec![proof])
            .await;
    for _ in 0..2 {
        let found = match metadata_discovery::discover(&f.pool, &f.key, &next, cohort)
            .await
            .unwrap()
        {
            Discovery::Proven(p) => p,
            Discovery::Held(h) => panic!("{h:?}"),
        };
        assert_eq!(found.result, results[0]);
        assert_eq!(found.baseline.head, Some(results[0]));
        assert_ne!(found.source_snapshot, first.source_snapshot);
        assert_eq!(found.ownership.tags, first.ownership.tags);
        assert_eq!(found.ownership.fields, first.ownership.fields);
        assert_eq!(found.baseline.revision, first.baseline.revision);
    }
    assert!(
        metadata_discovery::discover(
            &f.pool,
            &crm_api::config::RawPayloadKey::new([82; 32]),
            &next,
            cohort
        )
        .await
        .is_err(),
        "result AEAD must authenticate before using its baseline"
    );
    let interval=sqlx::query("SELECT n.id,n.started_at,o.completed_at FROM migration_family_refresh_plan p JOIN migration_snapshot n ON n.id=p.source_snapshot_id JOIN migration_family_refresh_plan old ON old.id=$2 JOIN migration_snapshot o ON o.id=old.source_snapshot_id WHERE p.id=$1").bind(next.plan).bind(claim.plan).fetch_one(&migrator).await.unwrap();
    sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
        .bind(interval.get::<Uuid, _>("id"))
        .bind(interval.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"))
        .execute(&migrator)
        .await
        .unwrap();
    assert!(
        matches!(
            metadata_discovery::discover(&f.pool, &f.key, &next, cohort)
                .await
                .unwrap(),
            Discovery::Held(Hold::SourceNotNewer)
        ),
        "must compare against prior refresh capture, not older first import"
    );
    sqlx::query("UPDATE migration_snapshot SET started_at=$2 WHERE id=$1")
        .bind(interval.get::<Uuid, _>("id"))
        .bind(interval.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"))
        .execute(&migrator)
        .await
        .unwrap();
    let mut tx = migrator.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE person_custom_field_value SET text_value='local edit' WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(f.org).bind(person).bind(field).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE person_custom_field_value SET text_value='Native baseline' WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(f.org).bind(person).bind(field).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        metadata_discovery::discover(&f.pool, &f.key, &next, cohort)
            .await
            .unwrap(),
        Discovery::Held(Hold::LocalChange)
    ));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_result WHERE bundle_id=$1",
    )
    .bind(next.bundle)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_catalog_handover_preserves_original_owners_and_charges(migrator: PgPool) {
    use crm_api::{
        auth::workspace::ReleaseReadiness,
        domain::migration::family_refresh::{commands, model::Family},
    };
    let mut p = person(101);
    p["tags"] = json!(["Imported"]);
    p["customText"] = json!("Native baseline");
    let f = support::fixture_with_book(
        &migrator,
        book(
            vec![p.clone()],
            vec![field(10, "customText", "Text", "text")],
        ),
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
    execute(&f, child, &ready, parent).await;
    let report = crate::db_people_admission_execution::report(&f, parent, vec![p]).await;
    let ledger_sql = "SELECT jsonb_build_object('import',i.retained_bytes,'snapshot',s.retained_bytes,'identities',(SELECT jsonb_agg(to_jsonb(x) ORDER BY kind,source_key) FROM migration_metadata_identity x WHERE x.import_id=i.id),'claims',(SELECT count(*) FROM migration_metadata_catalog_claim WHERE organization_id=i.organization_id),'readiness',(SELECT count(*) FROM migration_metadata_catalog_readiness WHERE organization_id=i.organization_id)) FROM migration_metadata_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id WHERE i.id=$1";
    let before: Value = sqlx::query_scalar(ledger_sql)
        .bind(child)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(before["readiness"], 0);
    assert_eq!(before["claims"], 0);
    let request = Uuid::new_v4();
    let command = || commands::PrepareFamilyRefresh {
        request_id: request,
        parent_import_id: parent,
        core_report_id: Some(report),
        history_capture_id: None,
        families: vec![Family::Metadata],
    };
    // Fail after claim insertion, while publishing readiness, to prove both the
    // owner backfill and its original-payer charges roll back with admission.
    sqlx::raw_sql("CREATE FUNCTION test_handover_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic catalog handover fault'; END $$; CREATE TRIGGER test_handover_fault BEFORE INSERT ON migration_metadata_catalog_readiness FOR EACH ROW EXECUTE FUNCTION test_handover_fault()").execute(&migrator).await.unwrap();
    assert!(commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        command()
    )
    .await
    .is_err());
    sqlx::raw_sql("DROP TRIGGER test_handover_fault ON migration_metadata_catalog_readiness; DROP FUNCTION test_handover_fault()").execute(&migrator).await.unwrap();
    let failed: Value = sqlx::query_scalar(ledger_sql)
        .bind(child)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(failed, before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_bundle WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap(),
        0
    );
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        command(),
    )
    .await
    .unwrap();
    let after: Value = sqlx::query_scalar(ledger_sql)
        .bind(child)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let bytes: i64 = sqlx::query_scalar("SELECT sum(32+octet_length(evidence_nonce)+octet_length(evidence_ciphertext))::bigint FROM migration_metadata_catalog_claim WHERE original_import_id=$1").bind(child).fetch_one(&migrator).await.unwrap();
    assert!(bytes > 0);
    assert_eq!(
        after["import"].as_i64().unwrap() - before["import"].as_i64().unwrap(),
        bytes
    );
    assert_eq!(
        after["snapshot"].as_i64().unwrap() - before["snapshot"].as_i64().unwrap(),
        bytes
    );
    assert_eq!(after["identities"], before["identities"]);
    assert_eq!(
        after["claims"].as_u64().unwrap(),
        before["identities"].as_array().unwrap().len() as u64
    );
    assert!(sqlx::query_scalar::<_,bool>("SELECT bool_and(c.original_import_id=x.import_id AND c.original_plan_id=x.plan_id AND c.original_mapping_id=x.mapping_id AND c.target_id=x.target_id) FROM migration_metadata_identity x JOIN migration_metadata_catalog_claim c USING(organization_id,source_account_id,kind,source_key) WHERE x.import_id=$1").bind(child).fetch_one(&migrator).await.unwrap());
    let replay = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        command(),
    )
    .await
    .unwrap();
    assert_eq!(replay.bundle_id, prepared.bundle_id);
    assert_eq!(
        sqlx::query_scalar::<_, Value>(ledger_sql)
            .bind(child)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        after,
        "receipt replay cannot backfill or charge again"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_person_metadata_proposal_is_atomic_and_preserves_null_gaps(
    migrator: PgPool,
) {
    metadata_execution_scenario(migrator, 0).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_metadata_execution_completes_and_reconciles(migrator: PgPool) {
    metadata_execution_scenario(migrator, 1).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_metadata_execution_holds_changed_catalog(migrator: PgPool) {
    metadata_execution_scenario(migrator, 2).await;
}

async fn metadata_execution_scenario(migrator: PgPool, mode: u8) {
    use crm_api::{
        auth::workspace::ReleaseReadiness,
        domain::migration::family_refresh::{
            commands,
            evidence::{Purpose, Scope},
            mapping_selection::{MappingPatch, Selection},
            metadata_delta::{Change, Gap},
            metadata_plan::{self, Decision, Prepared},
            model::Family,
            plan_commands::{self, PlanFamilyRefresh},
            preparation_worker::{self, Progress},
        },
    };
    let mut p = person(101);
    p["tags"] = json!(["Owned", "Keep"]);
    p["customText"] = json!("Before");
    p["customNull"] = json!("Keep null");
    p["customMissing"] = json!("Keep missing");
    p["customNumber"] = json!(1.25);
    let mut missing = p.clone();
    missing["id"] = json!(102);
    let f = support::fixture_with_book(
        &migrator,
        book(
            vec![p.clone(), missing],
            vec![
                field(10, "customText", "Text", "text"),
                field(11, "customNull", "Null", "text"),
                field(12, "customMissing", "Missing", "text"),
                field(13, "customNumber", "Number", "number"),
            ],
        ),
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
    execute(&f, child, &ready, parent).await;
    p["tags"] = json!(["Keep", "New"]);
    p["customText"] = json!("After");
    p["customNull"] = Value::Null;
    p.as_object_mut().unwrap().remove("customMissing");
    let report =
        crate::db_people_admission_execution::report(&f, parent, vec![p, person(999)]).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &ReleaseReadiness::for_tests(),
        &f.ctx,
        commands::PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata],
        },
    )
    .await
    .unwrap();
    for step in 0..60 {
        if preparation_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
            == Progress::Idle
        {
            break;
        }
        assert!(step < 59);
    }
    let rows=sqlx::query("SELECT m.id,c.target_id FROM migration_family_refresh_mapping m LEFT JOIN migration_metadata_catalog_claim c ON c.organization_id=m.organization_id AND c.source_key=m.source_key_hmac AND c.kind=CASE m.kind WHEN 'tag' THEN 'tag' ELSE m.kind END WHERE m.plan_id=$1 ORDER BY m.id").bind(prepared.families[0].plan_id).fetch_all(&migrator).await.unwrap();
    let patches = rows
        .iter()
        .map(|r| MappingPatch {
            mapping_id: r.get("id"),
            choice: match r.get::<Option<Uuid>, _>("target_id") {
                Some(target_id) => Selection::Existing { target_id },
                None => Selection::CreateMatching,
            },
        })
        .collect();
    let next = plan_commands::plan(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        prepared.bundle_id,
        PlanFamilyRefresh {
            request_id: Uuid::new_v4(),
            expected_revision: prepared.revision,
            family: Family::Metadata,
            patches,
            source_timezone: None,
        },
    )
    .await
    .unwrap();
    for step in 0..60 {
        preparation_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap();
        if sqlx::query_scalar::<_, bool>(
            "SELECT catalog_walk_complete FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(next.families[0].plan_id)
        .fetch_one(&migrator)
        .await
        .unwrap()
        {
            break;
        }
        assert!(step < 59);
    }
    let claim = crate::db_family_refresh_commands::claim_metadata_inspection(
        &f,
        next.bundle_id,
        next.families[0].plan_id,
    )
    .await;
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(next.bundle_id).fetch_one(&migrator).await.unwrap();
    let checkpoint="SELECT jsonb_build_object('counts',counts,'position',position,'retained',retained_bytes,'measured',measured_bytes,'reserved',reserved_bytes,'cursor',checkpoint_id,'source_complete',source_walk_complete,'owned_cursor',owned_after,'owned_complete',owned_walk_complete) FROM migration_family_refresh_plan WHERE id=$1";
    let before: Value = sqlx::query_scalar(checkpoint)
        .bind(claim.plan)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let native_sql="SELECT jsonb_build_object('tags',(SELECT jsonb_agg(to_jsonb(x) ORDER BY tag_id) FROM person_tag x WHERE organization_id=$1),'values',(SELECT jsonb_agg(to_jsonb(x) ORDER BY field_id) FROM person_custom_field_value x WHERE organization_id=$1))";
    let native_before: Value = sqlx::query_scalar(native_sql)
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        metadata_plan::prepare_unit(&f.pool, &f.key, &tiny, &claim, cohort)
            .await
            .unwrap(),
        Prepared::Capacity
    ));
    sqlx::raw_sql("CREATE FUNCTION test_person_metadata_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.position<>OLD.position THEN RAISE EXCEPTION 'synthetic Person metadata fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_person_metadata_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_person_metadata_fault()").execute(&migrator).await.unwrap();
    assert!(
        metadata_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, cohort)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_person_metadata_fault ON migration_family_refresh_plan; DROP FUNCTION test_person_metadata_fault()").execute(&migrator).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Value>(checkpoint)
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        before
    );
    let id = match metadata_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, cohort)
        .await
        .unwrap()
    {
        Prepared::Unit(id) => id,
        Prepared::Capacity => panic!("unexpected capacity"),
    };
    let row = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE id=$1")
        .bind(id)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM migration_family_refresh_plan WHERE id=$1")
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap();
    let evidence: metadata_plan::Evidence = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision,
    }
    .open(
        &f.key,
        id,
        Purpose::Manifest,
        row.get("nonce"),
        row.get("ciphertext"),
    )
    .unwrap();
    let Decision::Ready { native, .. } = evidence.decision else {
        panic!(
            "unexpected held proposal: {:?}",
            row.get::<Option<String>, _>("reason")
        );
    };
    assert_eq!(native.counts.updates, 1);
    assert_eq!(native.counts.tag_removals, 1);
    assert_eq!(native.counts.field_clears, 0);
    assert_eq!(
        native
            .changes
            .iter()
            .filter(|c| matches!(c, Change::SetField(..)))
            .count(),
        1
    );
    assert_eq!(
        native
            .changes
            .iter()
            .filter(|c| matches!(c, Change::AddTag(..)))
            .count(),
        1
    );
    assert!(native
        .gaps
        .iter()
        .any(|g| matches!(g, Gap::UnqualifiedNull(_))));
    assert!(native
        .gaps
        .iter()
        .any(|g| matches!(g, Gap::MissingField(_))));
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        native_before,
        "preparation cannot change native state"
    );
    let after: Value = sqlx::query_scalar(checkpoint)
        .bind(claim.plan)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert!(
        matches!(metadata_plan::prepare_unit(&f.pool,&f.key,&tiny,&claim,cohort).await.unwrap(),Prepared::Unit(replayed) if replayed==id)
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(checkpoint)
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        after
    );
    use crm_api::domain::migration::family_refresh::metadata_walk::{self, Progress as Walk};
    sqlx::raw_sql("CREATE FUNCTION test_person_walk_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id THEN RAISE EXCEPTION 'synthetic Person walk fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_person_walk_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_person_walk_fault()").execute(&migrator).await.unwrap();
    assert!(metadata_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
        .await
        .is_err());
    sqlx::raw_sql("DROP TRIGGER test_person_walk_fault ON migration_family_refresh_plan; DROP FUNCTION test_person_walk_fault()").execute(&migrator).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Value>(checkpoint)
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        after
    );
    for turn in 0..8 {
        if metadata_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
            == Walk::Finished
        {
            break;
        }
        assert!(turn < 7);
    }
    for turn in 0..8 {
        if metadata_walk::missing_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
            == Walk::Finished
        {
            break;
        }
        assert!(turn < 7);
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND kind='metadata'").bind(claim.plan).fetch_one(&migrator).await.unwrap(),3,"changed, missing, and outside-cohort Persons all receive outcomes");
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT reason FROM migration_family_refresh_manifest WHERE plan_id=$1 AND kind='metadata' AND source_id='102'").bind(claim.plan).fetch_one(&migrator).await.unwrap(),"source_not_observed");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND kind='metadata' AND disposition='excluded'").bind(claim.plan).fetch_one(&migrator).await.unwrap(),1);
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        native_before
    );
    use crm_api::domain::migration::family_refresh::sealing::{self, Progress as Seal};
    // Advance fixed-width held/current checkpoints until the first real recipe.
    for turn in 0..20 {
        let writes:Option<bool>=sqlx::query_scalar("SELECT u.disposition IN ('insert','update') FROM migration_family_refresh_plan p JOIN migration_family_refresh_manifest u ON u.plan_id=p.id AND u.organization_id=p.organization_id AND u.position=p.proof_after+1 WHERE p.id=$1").bind(claim.plan).fetch_optional(&migrator).await.unwrap();
        if writes == Some(true) {
            break;
        }
        assert_eq!(
            sealing::run_once(&f.pool, &f.key, &tiny, &claim)
                .await
                .unwrap(),
            Seal::Advanced
        );
        assert!(turn < 19);
    }
    let proof_checkpoint="SELECT jsonb_build_object('cursor',proof_after,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes,'proofs',(SELECT count(*) FROM migration_family_refresh_write_proof WHERE plan_id=p.id)) FROM migration_family_refresh_plan p WHERE id=$1";
    let proof_before: Value = sqlx::query_scalar(proof_checkpoint)
        .bind(claim.plan)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(
        sealing::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        Seal::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_family_proof_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.proof_after IS DISTINCT FROM OLD.proof_after THEN RAISE EXCEPTION 'synthetic proof fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_family_proof_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_family_proof_fault()").execute(&migrator).await.unwrap();
    assert!(sealing::run_once(&f.pool, &f.key, &f.policy, &claim)
        .await
        .is_err());
    sqlx::raw_sql("DROP TRIGGER test_family_proof_fault ON migration_family_refresh_plan; DROP FUNCTION test_family_proof_fault()").execute(&migrator).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Value>(proof_checkpoint)
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        proof_before
    );
    for turn in 0..40 {
        if sealing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
            == Seal::Finished
        {
            break;
        }
        assert!(turn < 39);
    }
    let sealed=sqlx::query("SELECT p.*,b.digest AS bundle_digest,b.state AS bundle_state FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id WHERE p.id=$1").bind(claim.plan).fetch_one(&migrator).await.unwrap();
    assert_eq!(sealed.get::<String, _>("state"), "ready");
    assert_eq!(sealed.get::<String, _>("bundle_state"), "ready");
    assert_eq!(sealed.get::<Vec<u8>, _>("digest").len(), 32);
    assert_eq!(sealed.get::<Vec<u8>, _>("bundle_digest").len(), 32);
    assert_eq!(
        sealed.get::<i64, _>("seal_after"),
        sealed.get::<i64, _>("position")
    );
    assert_eq!(
        sealed.get::<Value, _>("seal_counts"),
        sealed.get::<Value, _>("counts")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_write_proof WHERE plan_id=$1"
        )
        .bind(claim.plan)
        .fetch_one(&migrator)
        .await
        .unwrap(),
        4,
        "new catalog tag plus three exact Person mutations"
    );
    assert!(sqlx::query_scalar::<_,bool>("SELECT bool_and(nonce IS NOT NULL AND ciphertext IS NOT NULL) FROM migration_family_refresh_write_proof WHERE plan_id=$1").bind(claim.plan).fetch_one(&migrator).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        native_before,
        "proof preparation and sealing cannot perform native writes"
    );
    preparation_worker::release(&f.pool, &claim).await.unwrap();
    use crm_api::domain::migration::family_refresh::confirmation::{
        self, ConfirmFamilyRefresh, SelectedPlan,
    };
    let request_id = Uuid::new_v4();
    let make_command = || ConfirmFamilyRefresh {
        request_id,
        expected_revision: next.revision.clone(),
        bundle_digest: sealed
            .get::<Vec<u8>, _>("bundle_digest")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        families: vec![SelectedPlan {
            family: Family::Metadata,
            plan_id: claim.plan,
            plan_revision: sealed.get::<i64, _>("revision").to_string(),
            plan_digest: sealed
                .get::<Vec<u8>, _>("digest")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            expected_counts: serde_json::from_value(sealed.get("counts")).unwrap(),
        }],
        acknowledged_exclusions: true,
    };
    let release = crm_api::auth::workspace::ReleaseReadiness::for_tests();
    let mut wrong = make_command();
    wrong.families[0].expected_counts.tag_removals += 1;
    assert!(
        confirmation::confirm(&f.pool, &f.key, &release, &f.ctx, claim.bundle, wrong)
            .await
            .is_err()
    );
    let mut unacknowledged = make_command();
    unacknowledged.acknowledged_exclusions = false;
    assert!(confirmation::confirm(
        &f.pool,
        &f.key,
        &release,
        &f.ctx,
        claim.bundle,
        unacknowledged
    )
    .await
    .is_err());
    sqlx::raw_sql("CREATE FUNCTION test_confirm_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='confirm' THEN RAISE EXCEPTION 'synthetic confirm fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_confirm_fault BEFORE INSERT ON migration_family_refresh_receipt FOR EACH ROW EXECUTE FUNCTION test_confirm_fault()").execute(&migrator).await.unwrap();
    assert!(confirmation::confirm(
        &f.pool,
        &f.key,
        &release,
        &f.ctx,
        claim.bundle,
        make_command()
    )
    .await
    .is_err());
    sqlx::raw_sql("DROP TRIGGER test_confirm_fault ON migration_family_refresh_receipt; DROP FUNCTION test_confirm_fault()").execute(&migrator).await.unwrap();
    assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=$1)").bind(f.org).fetch_one(&migrator).await.unwrap());
    let admitted = confirmation::confirm(
        &f.pool,
        &f.key,
        &release,
        &f.ctx,
        claim.bundle,
        make_command(),
    )
    .await
    .unwrap();
    assert_eq!(admitted.state, "queued");
    let replay = confirmation::confirm(
        &f.pool,
        &f.key,
        &release,
        &f.ctx,
        claim.bundle,
        make_command(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&admitted).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        native_before,
        "confirmation queues work without mutating native rows"
    );

    use crm_api::domain::migration::family_refresh::{
        execution, native_baseline::ResultData, result_queries,
    };
    let executing = execution::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(executing.plan, claim.plan);
    for turn in 0..30 {
        let next=sqlx::query("SELECT u.kind,u.disposition,u.source_id FROM migration_family_refresh_plan p JOIN migration_family_refresh_manifest u ON u.plan_id=p.id AND u.organization_id=p.organization_id AND u.position=p.apply_position+1 WHERE p.id=$1").bind(claim.plan).fetch_one(&migrator).await.unwrap();
        if next.get::<String, _>("kind") == "metadata"
            && next.get::<String, _>("disposition") == "update"
        {
            break;
        }
        assert_eq!(
            execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
                .await
                .unwrap(),
            execution::Progress::Advanced
        );
        assert!(turn < 29);
    }
    let execution_checkpoint="SELECT jsonb_build_object('position',apply_position,'results',results,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes,'heads',(SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=p.organization_id),'receipts',(SELECT count(*) FROM migration_family_refresh_result WHERE plan_id=p.id)) FROM migration_family_refresh_plan p WHERE id=$1";
    let before_execution: Value = sqlx::query_scalar(execution_checkpoint)
        .bind(claim.plan)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let before_person: Value = sqlx::query_scalar(native_sql)
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    if mode == 2 {
        sqlx::query("UPDATE tag SET name='Locally renamed',updated_at=clock_timestamp() WHERE organization_id=$1 AND id IN (SELECT target_id FROM migration_family_refresh_manifest WHERE plan_id=$2 AND kind='catalog' AND disposition='insert')").bind(f.org).bind(claim.plan).execute(&migrator).await.unwrap();
        assert_eq!(
            execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
                .await
                .unwrap(),
            execution::Progress::Advanced
        );
        let reason:String=sqlx::query_scalar("SELECT r.reason FROM migration_family_refresh_result r JOIN migration_family_refresh_manifest u ON u.id=r.manifest_id AND u.organization_id=r.organization_id WHERE r.plan_id=$1 AND u.kind='metadata' AND u.source_id='101' AND r.disposition='held'").bind(claim.plan).fetch_one(&migrator).await.unwrap();
        assert_eq!(reason, "target_unavailable");
        assert!(!sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM migration_family_refresh_head WHERE organization_id=$1)"
        )
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap());
        assert_eq!(
            sqlx::query_scalar::<_, Value>(native_sql)
                .bind(f.org)
                .fetch_one(&migrator)
                .await
                .unwrap(),
            before_person
        );
        return;
    }
    assert_eq!(
        execution::apply_once(&f.pool, &f.key, &tiny, &release, &executing)
            .await
            .unwrap(),
        execution::Progress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_native_result_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic native result fault'; END $$; CREATE TRIGGER test_native_result_fault BEFORE INSERT ON migration_family_refresh_result FOR EACH ROW EXECUTE FUNCTION test_native_result_fault()").execute(&migrator).await.unwrap();
    assert!(
        execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_native_result_fault ON migration_family_refresh_result; DROP FUNCTION test_native_result_fault()").execute(&migrator).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        before_person,
        "a fault after native SQL rolls back every Person mutation"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(execution_checkpoint)
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        before_execution
    );
    execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
        .await
        .unwrap();
    let successful=sqlx::query("SELECT r.* FROM migration_family_refresh_result r JOIN migration_family_refresh_manifest u ON u.id=r.manifest_id AND u.organization_id=r.organization_id WHERE r.plan_id=$1 AND u.kind='metadata' AND u.source_id='101'").bind(claim.plan).fetch_one(&migrator).await.unwrap();
    assert_eq!(successful.get::<String, _>("disposition"), "applied");
    let proof: ResultData = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: sealed.get("revision"),
    }
    .open(
        &f.key,
        successful.get("id"),
        Purpose::Result,
        successful.get("nonce"),
        successful.get("ciphertext"),
    )
    .unwrap();
    let proof = proof.after_state.unwrap();
    let crm_api::domain::migration::family_refresh::native_baseline::State::Metadata {
        snapshot,
        ..
    } = proof.state
    else {
        panic!("metadata after-state")
    };
    assert_eq!(snapshot.head, Some(successful.get::<Uuid, _>("id")));
    assert_eq!(snapshot.revision, native.expected_revision + 3);
    assert_eq!(sqlx::query_scalar::<_,Uuid>("SELECT result_id FROM migration_family_refresh_head WHERE organization_id=$1 AND kind='metadata'").bind(f.org).fetch_one(&migrator).await.unwrap(),successful.get::<Uuid,_>("id"));
    let native_after: Value = sqlx::query_scalar(native_sql)
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_ne!(native_after, before_person);
    let page = result_queries::results(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        result_queries::ResultPage {
            family: Family::Metadata,
            plan_id: claim.plan,
            cohort_id: None,
            outcome: None,
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some());
    let mut foreign = f.ctx.clone();
    foreign.organization_id = crm_api::ids::OrganizationId::new(Uuid::new_v4());
    assert!(result_queries::results(
        &f.pool,
        &f.key,
        &foreign,
        claim.bundle,
        result_queries::ResultPage {
            family: Family::Metadata,
            plan_id: claim.plan,
            cohort_id: None,
            outcome: None,
            limit: Some(1),
            cursor: page.next_cursor.clone()
        }
    )
    .await
    .is_err());
    if mode == 1 {
        preparation_worker::release(&f.pool, &executing)
            .await
            .unwrap();
        assert_eq!(
            execution::run_once(&f.pool, &f.key, &f.policy, None)
                .await
                .unwrap(),
            Progress::Paused
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT pause_reason FROM migration_family_refresh_plan WHERE id=$1"
            )
            .bind(claim.plan)
            .fetch_one(&migrator)
            .await
            .unwrap(),
            "release_not_ready"
        );
        assert_eq!(
            sqlx::query_scalar::<_, Value>(native_sql)
                .bind(f.org)
                .fetch_one(&migrator)
                .await
                .unwrap(),
            native_after
        );
        use crm_api::domain::migration::family_refresh::{lifecycle::FamilyControl, resume};
        let resumed = resume::resume(
            &f.pool,
            &f.key,
            &f.policy,
            &release,
            &f.ctx,
            claim.bundle,
            FamilyControl {
                request_id: Uuid::new_v4(),
                expected_revision: admitted.revision.clone(),
                families: vec![Family::Metadata],
            },
        )
        .await
        .unwrap();
        assert_eq!(resumed.state, "queued");
        for turn in 0..30 {
            match execution::run_once(&f.pool, &f.key, &f.policy, Some(&release))
                .await
                .unwrap()
            {
                Progress::Advanced => assert!(turn < 29),
                Progress::Idle => break,
                Progress::Paused => panic!("unexpected pause"),
            }
        }
        let completed = sqlx::query("SELECT p.state,p.position,p.apply_position,p.results,b.state AS bundle_state FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id WHERE p.id=$1").bind(claim.plan).fetch_one(&migrator).await.unwrap();
        assert_eq!(completed.get::<String, _>("state"), "completed");
        assert_eq!(completed.get::<String, _>("bundle_state"), "completed");
        assert_eq!(
            completed.get::<i64, _>("position"),
            completed.get::<i64, _>("apply_position")
        );
        let totals: crm_api::domain::migration::family_refresh::model::Counts =
            serde_json::from_value(completed.get("results")).unwrap();
        assert!(totals.reconciles());
        assert_eq!(totals.units, completed.get::<i64, _>("position") as u64);
        assert!(execution::claim_next(&f.pool).await.unwrap().is_none());
        return;
    }
    use crm_api::domain::migration::family_refresh::lifecycle::{self, FamilyControl};
    let cancel_id = Uuid::new_v4();
    let cancel_command = || FamilyControl {
        request_id: cancel_id,
        expected_revision: admitted.revision.clone(),
        families: vec![Family::Metadata],
    };
    let cancelled = lifecycle::cancel(&f.pool, &f.key, &f.ctx, claim.bundle, cancel_command())
        .await
        .unwrap();
    assert_eq!(cancelled.state, "cancelled");
    let replay = lifecycle::cancel(&f.pool, &f.key, &f.ctx, claim.bundle, cancel_command())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&cancelled).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(native_sql)
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap(),
        native_after
    );
}

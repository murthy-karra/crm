//! R1 regressions through real retained captures, completed parent imports and
//! typed child commands. The only migrator mutation is an explicit executor
//! demotion/adoption authorization fixture; no review-write bypass is used.
use crate::import_support::{self as support, Book, Fixture};
use crm_api::{
    domain::{
        envelope::CommandContext,
        migration::{
            crypto,
            imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
            metadata, metadata_worker,
            snapshot_source::Stream,
            MigrationError,
        },
    },
    ids::UserId,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

fn person(id: u64) -> Value {
    json!({"id":id,"firstName":"Synthetic R1 Person","stage":"Lead","assignedUserId":3})
}
fn field(id: u64, name: &str, kind: &str) -> Value {
    json!({"id":id,"name":name,"label":name,"type":kind,"isRecurring":false})
}
async fn drain(f: &Fixture) {
    let calls = f.reader.calls();
    for _ in 0..2_000 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            assert_eq!(f.reader.calls(), calls, "child work is source-free");
            return;
        }
    }
    panic!("bounded synthetic metadata work did not settle");
}
async fn detail(f: &Fixture, child: Uuid) -> Value {
    metadata::detail(&f.pool, &f.key, &f.ctx, child, &f.policy)
        .await
        .unwrap()
}
fn plan(value: &Value) -> Uuid {
    Uuid::parse_str(value["latest_plan"]["id"].as_str().unwrap()).unwrap()
}
async fn fixture(
    migrator: &PgPool,
    people: Vec<Value>,
    fields: Vec<Value>,
) -> (Fixture, Uuid, Value) {
    let book = Arc::new(Book::new(people));
    book.set_records(Stream::CustomFields, fields);
    let f = support::fixture_with_book(migrator, book).await;
    let calls = f.reader.calls();
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
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool,&f.key,&f.ctx,parent,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],"plan_revision":ready["plan"]["revision"],"confirmation_digest":ready["plan"]["confirmation_digest"],"acknowledgments":{"held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}})).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    let proposed = metadata::propose(
        &f.pool,
        &f.key,
        &f.ctx,
        serde_json::from_value(json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}))
            .unwrap(),
        &f.policy,
    )
    .await
    .unwrap();
    let child = Uuid::parse_str(proposed["import"]["id"].as_str().unwrap()).unwrap();
    drain(&f).await;
    let ready = detail(&f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    assert_eq!(ready["latest_plan"]["revision"], "1");
    assert_eq!(f.reader.calls(), calls);
    (f, child, ready)
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
    sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 ORDER BY kind,id").bind(plan).bind(f.org).fetch_all(&f.pool).await.unwrap().into_iter().map(|row|{let data=open(f,plan,&row,"mapping");(row,data)}).collect()
}
async fn patches(f: &Fixture, plan: Uuid) -> Vec<Value> {
    mappings(f,plan).await.into_iter().filter(|(row,data)|row.get::<bool,_>("qualified") && data["label"].is_string() && (!data["field"].is_object() || data["field"]["creation_reasons"].as_array().is_some_and(Vec::is_empty))).map(|(row,_)|json!({"mapping_id":row.get::<Uuid,_>("id"),"choice":{"kind":"create_matching"}})).collect()
}
async fn replan(f: &Fixture, child: Uuid, revision: &Value, mappings: Vec<Value>) -> Value {
    metadata::replan(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":revision,"mappings":mappings})).unwrap(),&f.policy).await.unwrap()
}
async fn approve(f: &Fixture, child: Uuid, ready: &Value) -> Value {
    replan(
        f,
        child,
        &ready["latest_plan"]["revision"],
        patches(f, plan(ready)).await,
    )
    .await;
    drain(f).await;
    let ready = detail(f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    ready
}
async fn confirm(f: &Fixture, child: Uuid, ready: &Value) {
    metadata::confirm(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":ready["latest_plan"]["id"],"plan_revision":ready["latest_plan"]["revision"],"confirmation_digest":ready["latest_plan"]["confirmation_digest"],"workspace_revision":ready["workspace_revision"],"acknowledgments":{"held_count":ready["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})).unwrap(),&crm_app::auth::workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
}
async fn execute(f: &Fixture, child: Uuid, ready: &Value) -> Value {
    confirm(f, child, ready).await;
    drain(f).await;
    let value = detail(f, child).await;
    assert_eq!(value["state"], "completed");
    value
}
// Full field evidence is fetched through the public query seam, including
// segmented continuation. Operations are ordinal -> canonical JSON strings.
async fn full_fields(
    f: &Fixture,
    ctx: &CommandContext,
    child: Uuid,
    plan: Uuid,
    entity: Uuid,
    committed: bool,
) -> Value {
    let mut cursor = None;
    let mut text = String::new();
    for _ in 0..100 {
        let q = metadata::FieldQuery {
            cursor,
            limit: Some(512),
        };
        let segment = if committed {
            metadata::result_field(&f.pool, &f.key, ctx, child, entity, "all", q)
                .await
                .unwrap()
        } else {
            metadata::field(&f.pool, &f.key, ctx, child, plan, entity, "all", q)
                .await
                .unwrap()
        };
        assert_eq!(number(&segment["offset_bytes"]), text.len() as i64);
        text.push_str(segment["text"].as_str().unwrap());
        if segment["complete"] == true {
            assert!(segment["next_cursor"].is_null());
            assert_eq!(number(&segment["full_utf8_bytes"]), text.len() as i64);
            let mut value: Value = serde_json::from_str(&text).unwrap();
            let mut operations = value["operations"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(i, v)| {
                    (
                        i.parse::<usize>().unwrap(),
                        serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            operations.sort_by_key(|(i, _)| *i);
            value["operations"] = json!(operations.into_iter().map(|(_, v)| v).collect::<Vec<_>>());
            return value;
        }
        cursor = Some(segment["next_cursor"].as_str().unwrap().to_owned());
    }
    panic!("bounded synthetic field evidence did not complete");
}
async fn manifest(f: &Fixture, plan: Uuid, source_id: &str) -> (PgRow, Value) {
    let row=sqlx::query("SELECT * FROM migration_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id=$3").bind(plan).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap();
    let value = open(f, plan, &row, "manifest");
    let full = full_fields(f, &f.ctx, row.get("import_id"), plan, row.get("id"), false).await;
    assert_eq!(full["operations"], value["operations"]);
    (row, value)
}
async fn result(f: &Fixture, child: Uuid, source_id: &str) -> (PgRow, Value) {
    let row=sqlx::query("SELECT * FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 AND kind='people' AND source_id=$3").bind(child).bind(f.org).bind(source_id).fetch_one(&f.pool).await.unwrap();
    let value = open(f, row.get("plan_id"), &row, "result");
    let full = full_fields(f, &f.ctx, child, row.get("plan_id"), row.get("id"), true).await;
    assert_eq!(full["source"], value["source"]);
    assert_eq!(full["operations"], value["operations"]);
    (row, value)
}
fn operation<'a>(value: &'a Value, field: &str) -> &'a Value {
    value["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["source_field"] == field)
        .expect("retained field operation")
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
async fn issue(f: &Fixture, plan: Uuid, code: &str) -> i64 {
    sqlx::query_scalar("SELECT record_count FROM migration_metadata_issue WHERE plan_id=$1 AND organization_id=$2 AND code=$3").bind(plan).bind(f.org).bind(code).fetch_one(&f.pool).await.unwrap()
}
async fn native(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('tags',(SELECT count(*) FROM tag WHERE organization_id=$1),'fields',(SELECT count(*) FROM custom_field WHERE organization_id=$1),'options',(SELECT count(*) FROM custom_field_option WHERE organization_id=$1),'links',(SELECT count(*) FROM person_tag WHERE organization_id=$1),'values',(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1))").bind(f.org).fetch_one(&f.pool).await.unwrap()
}
fn number(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}
/// Compare each committed result's public counters against its committed exact
/// operations and the immutable planned operation IDs, not against root totals.
async fn result_counts(f: &Fixture, child: Uuid, ctx: &CommandContext) {
    for row in sqlx::query("SELECT * FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 ORDER BY id").bind(child).bind(f.org).fetch_all(&f.pool).await.unwrap() {
        let counts:Value=row.get("counts");
        let kind:String=row.get("kind");
        let mut held=0;
        for family in ["tags","fields","options","tag_links","values"] {
            assert_eq!(number(&counts[family]["pending"]),0);
            held+=number(&counts[family]["held"]);
        }
        assert_eq!(number(&counts["held_count"]),held);
        if kind=="people" {
            let data=open(f,row.get("plan_id"),&row,"result");
            let full=full_fields(f,ctx,child,row.get("plan_id"),row.get("id"),true).await;
            assert_eq!(full["source"],data["source"]);
            assert_eq!(full["operations"],data["operations"]);
            let ops=data["operations"].as_array().unwrap();
            let planned:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM migration_metadata_operation WHERE manifest_id=$1 AND organization_id=$2 ORDER BY id").bind(row.get::<Uuid,_>("manifest_id")).bind(f.org).fetch_all(&f.pool).await.unwrap();
            let mut actual:Vec<Uuid>=ops.iter().map(|op|Uuid::parse_str(op["id"].as_str().unwrap()).unwrap()).collect();
            actual.sort();
            assert_eq!(actual,planned,"result retains every planned operation once");
            for (kind,family) in [("tag_link","tag_links"),("value","values")] {
                let count=ops.iter().filter(|op|op["kind"]==kind).count() as i64;
                assert_eq!(number(&counts[family]["planned"]),count);
                for disposition in ["eligible","created","applied","already_present","held","not_supplied","source_null"] {
                    assert_eq!(number(&counts[family][disposition]),ops.iter().filter(|op|op["kind"]==kind && op["disposition"]==disposition).count() as i64);
                }
            }
            for family in ["tags","fields","options"] {assert_eq!(number(&counts[family]["planned"]),0);}
        } else {
            let family=match kind.as_str(){"tag"=>"tags","field"=>"fields","option"=>"options",_=>panic!("closed result kind")};
            assert_eq!(number(&counts[family]["planned"]),1);
            assert_eq!(number(&counts[family][row.get::<String,_>("disposition").as_str()]),1);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_rapid_replans_preserve_unprepared_approval(migrator: PgPool) {
    let mut p = person(101);
    p["tags"] = json!(["Retained approval"]);
    p["customSafe"] = json!("approved value");
    let (f, child, v1) = fixture(&migrator, vec![p], vec![field(21, "customSafe", "text")]).await;
    let v2 = replan(
        &f,
        child,
        &v1["latest_plan"]["revision"],
        patches(&f, plan(&v1)).await,
    )
    .await;
    assert_eq!(v2["import"]["latest_plan"]["revision"], "2");
    assert_eq!(v2["import"]["latest_plan"]["state"], "building");
    // No worker call between these two commands: v2's encrypted patch, not a
    // partially materialized choice table, must be inherited by empty v3.
    let v3 = replan(&f, child, &v2["import"]["latest_plan"]["revision"], vec![]).await;
    assert_eq!(v3["import"]["latest_plan"]["revision"], "3");
    let v4 = replan(&f, child, &v3["import"]["latest_plan"]["revision"], vec![]).await;
    assert_eq!(v4["import"]["latest_plan"]["revision"], "4");
    // Both ancestors were interrupted during choice copying.
    drain(&f).await;
    let ready = detail(&f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "0");
    let mappings = mappings(&f, plan(&ready)).await;
    assert_eq!(mappings.len(), 2);
    for (row, data) in mappings {
        assert_eq!(row.get::<String, _>("disposition"), "create_matching");
        assert_eq!(data["choice"]["kind"], "create_matching");
    }
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":0,"options":0,"links":0,"values":0})
    );
    execute(&f, child, &ready).await;
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"options":0,"links":1,"values":1})
    );
    result_counts(&f, child, &f.ctx).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_native_choice_case_collision_holds_dependencies_before_ready(
    migrator: PgPool,
) {
    let native_collision: bool = sqlx::query_scalar("SELECT lower($1::text)=lower($2::text)")
        .bind("İ")
        .bind("i")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert!(
        native_collision,
        "fixture requires the configured PostgreSQL native İ/i fold"
    );
    let mut p = person(101);
    p["customChoice"] = json!("İ");
    p["customSafe"] = json!("independent value");
    let mut choices = field(21, "customChoice", "dropdown");
    choices["choices"] = json!(["İ", "i"]);
    let (f, child, v1) = fixture(
        &migrator,
        vec![p],
        vec![choices, field(22, "customSafe", "text")],
    )
    .await;
    let catalog = mappings(&f, plan(&v1)).await;
    let (_, bad) = catalog
        .iter()
        .find(|(row, data)| {
            row.get::<String, _>("kind") == "field" && data["source_name"] == "customChoice"
        })
        .unwrap();
    reason(bad, "colliding_source_choices");
    let wire = metadata::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        plan(&v1),
        metadata::MetadataPage::default(),
    )
    .await
    .unwrap();
    let bad_wire = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["source_id"] == "21")
        .unwrap();
    let safe_wire = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["source_id"] == "22")
        .unwrap();
    assert_eq!(bad_wire["qualified"], true);
    assert_eq!(bad_wire["create_matching_available"], false);
    assert_eq!(safe_wire["qualified"], true);
    assert_eq!(safe_wire["create_matching_available"], true);
    let options = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "option")
        .collect::<Vec<_>>();
    assert_eq!(options.len(), 2);
    for option in options {
        assert_eq!(option["create_matching_available"], false);
    }

    let rejected=metadata::replan(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":v1["latest_plan"]["revision"],"mappings":[{"mapping_id":bad_wire["id"],"choice":{"kind":"create_matching"}}]})).unwrap(),&f.policy).await;
    assert!(matches!(
        rejected,
        Err(MigrationError::SourceNotEligible | MigrationError::InvalidImportChoice)
    ));
    assert_eq!(detail(&f, child).await["latest_plan"]["revision"], "1");
    let safe = catalog
        .iter()
        .find(|(row, data)| {
            row.get::<String, _>("kind") == "field" && data["source_name"] == "customSafe"
        })
        .unwrap()
        .0
        .get::<Uuid, _>("id");
    replan(
        &f,
        child,
        &v1["latest_plan"]["revision"],
        vec![json!({"mapping_id":safe,"choice":{"kind":"create_matching"}})],
    )
    .await;
    drain(&f).await;
    let ready = detail(&f, child).await;
    assert_eq!(ready["latest_plan"]["state"], "ready");
    let catalog = mappings(&f, plan(&ready)).await;
    let mut held = 0;
    for (row, data) in &catalog {
        if row.get::<String, _>("kind") == "option" {
            assert_eq!(row.get::<String, _>("disposition"), "hold");
            reason(data, "field_mapping_held");
            held += 1;
        } else if data["source_name"] == "customChoice" {
            assert_eq!(row.get::<String, _>("disposition"), "hold");
            reason(data, "colliding_source_choices");
            held += 1;
        }
    }
    assert_eq!(held, 3);
    let (_, planned) = manifest(&f, plan(&ready), "101").await;
    assert_eq!(operation(&planned, "customChoice")["disposition"], "held");
    reason(operation(&planned, "customChoice"), "field_mapping_held");
    assert_eq!(operation(&planned, "customSafe")["disposition"], "eligible");
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "4");
    assert!(issue(&f, plan(&ready), "colliding_source_choices").await >= 1);
    execute(&f, child, &ready).await;
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":1,"options":0,"links":0,"values":1})
    );
    let label: String =
        sqlx::query_scalar("SELECT label FROM custom_field WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(label, "customSafe");
    result_counts(&f, child, &f.ctx).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_undeclared_custom_value_is_held_with_exact_source_evidence(migrator: PgPool) {
    let orphan = json!({"nested":[1,null,"N/A"],"text":"raw orphan"});
    let canonical = r#"{"nested":[1e0,null,"N/A"],"text":"raw orphan"}"#;
    let mut p = person(101);
    p["customOrphan"] = orphan.clone();
    p["customSafe"] = json!("independent");
    let (f, child, v1) = fixture(&migrator, vec![p], vec![field(21, "customSafe", "text")]).await;
    let ready = approve(&f, child, &v1).await;
    let (manifest_row, planned) = manifest(&f, plan(&ready), "101").await;
    let full = full_fields(
        &f,
        &f.ctx,
        child,
        plan(&ready),
        manifest_row.get("id"),
        false,
    )
    .await;
    assert_eq!(full["source"]["customOrphan"], canonical);
    let held = operation(&planned, "customOrphan");
    assert_eq!(held["kind"], "value");
    assert_eq!(held["disposition"], "held");
    assert!(held["mapping_id"].is_null());
    assert!(held["target_id"].is_null());
    reason(held, "source_definition_unavailable");
    assert_eq!(ready["latest_plan"]["counts"]["values"]["planned"], "2");
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "1");
    assert_eq!(
        issue(&f, plan(&ready), "source_definition_unavailable").await,
        1
    );
    let source=sqlx::query("SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id='101'").bind(plan(&ready)).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        open(&f, plan(&ready), &source, "source")["provenance"]["customOrphan"],
        canonical
    );
    execute(&f, child, &ready).await;
    let (_, committed) = result(&f, child, "101").await;
    assert_eq!(committed["source"]["customOrphan"], canonical);
    assert_eq!(operation(&committed, "customOrphan")["disposition"], "held");
    reason(
        operation(&committed, "customOrphan"),
        "source_definition_unavailable",
    );
    assert_eq!(native(&f).await["values"], 1);
    result_counts(&f, child, &f.ctx).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_malformed_tags_hold_person_without_fabricating_counted_operations(
    migrator: PgPool,
) {
    let mut p = person(101);
    p["tags"] = json!({});
    p["customSafe"] = json!("independent value");
    let mut no_write = person(102);
    no_write["tags"] = json!({});
    let (f, child, v1) = fixture(
        &migrator,
        vec![p, no_write],
        vec![field(21, "customSafe", "text")],
    )
    .await;
    let ready = approve(&f, child, &v1).await;
    let (row, planned) = manifest(&f, plan(&ready), "101").await;
    assert_eq!(row.get::<String, _>("disposition"), "eligible");
    reason(&planned, "tags_shape_unqualified");
    assert_eq!(planned["operations"].as_array().unwrap().len(), 1);
    assert_eq!(operation(&planned, "customSafe")["disposition"], "eligible");
    for family in ["tags", "tag_links"] {
        assert_eq!(ready["latest_plan"]["counts"][family]["planned"], "0");
    }
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "0");
    assert_eq!(issue(&f, plan(&ready), "tags_shape_unqualified").await, 2);
    let (no_write_row, no_write_plan) = manifest(&f, plan(&ready), "102").await;
    assert_eq!(no_write_row.get::<String, _>("disposition"), "eligible");
    reason(&no_write_plan, "tags_shape_unqualified");
    assert_eq!(no_write_plan["operations"].as_array().unwrap().len(), 1);
    assert_eq!(
        operation(&no_write_plan, "customSafe")["disposition"],
        "not_supplied"
    );
    let completed = execute(&f, child, &ready).await;
    assert_eq!(completed["counts"]["held_count"], "0");
    let (row, committed) = result(&f, child, "101").await;
    assert_eq!(row.get::<String, _>("disposition"), "held");
    assert_eq!(committed["source"]["tags"], "{}");
    reason(&committed, "tags_shape_unqualified");
    assert_eq!(
        operation(&committed, "customSafe")["disposition"],
        "applied"
    );
    let (no_write_row, no_write_result) = result(&f, child, "102").await;
    assert_eq!(no_write_row.get::<String, _>("disposition"), "held");
    assert_eq!(no_write_result["source"]["tags"], "{}");
    assert!(no_write_result["source"].get("customSafe").is_none());
    reason(&no_write_result, "tags_shape_unqualified");
    assert_eq!(
        operation(&no_write_result, "customSafe")["disposition"],
        "not_supplied"
    );
    assert_eq!(no_write_row.get::<Value, _>("counts")["held_count"], "0");
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":1,"options":0,"links":0,"values":1})
    );
    result_counts(&f, child, &f.ctx).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_result_counts_match_operations_including_held_null_and_absent(
    migrator: PgPool,
) {
    let mut good = person(101);
    good["tags"] = json!(["Shared"]);
    good["customText"] = json!("text");
    good["customNumber"] = json!(12);
    let mut held = person(102);
    held["tags"] = json!([42]);
    held["customNumber"] = json!("12");
    let mut null = person(103);
    null["tags"] = Value::Null;
    null["customText"] = Value::Null;
    null["customNumber"] = Value::Null;
    let mut absent = person(104);
    absent["tags"] = json!([]);
    let (f, child, v1) = fixture(
        &migrator,
        vec![good, held, null, absent],
        vec![
            field(21, "customText", "text"),
            field(22, "customNumber", "number"),
        ],
    )
    .await;
    let ready = approve(&f, child, &v1).await;
    assert_eq!(ready["latest_plan"]["counts"]["values"]["planned"], "8");
    assert_eq!(ready["latest_plan"]["counts"]["tag_links"]["planned"], "2");
    assert_eq!(ready["latest_plan"]["counts"]["held_count"], "2");
    execute(&f, child, &ready).await;
    result_counts(&f, child, &f.ctx).await;
    let (held, committed) = result(&f, child, "102").await;
    assert_eq!(held.get::<String, _>("disposition"), "held");
    assert_eq!(held.get::<Value, _>("counts")["held_count"], "2");
    assert_eq!(committed["operations"].as_array().unwrap().len(), 3);
    let (null, source_null) = result(&f, child, "103").await;
    assert_eq!(null.get::<String, _>("disposition"), "source_null");
    assert_eq!(source_null["source"]["tags"], "null");
    let (absent, not_supplied) = result(&f, child, "104").await;
    assert_eq!(absent.get::<String, _>("disposition"), "not_supplied");
    assert_eq!(not_supplied["source"]["tags"], "[]");
    assert!(not_supplied["source"].get("customText").is_none());
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":2,"options":0,"links":1,"values":2})
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_executor_demotion_requires_explicit_current_admin_adoption(migrator: PgPool) {
    let (f, parent, child, ready) = crate::db_metadata_import_gate::fixture(&migrator, false).await;
    let calls = f.reader.calls();
    confirm(&f, child, &ready).await;
    // Adversarial fixture: switch authority only after durable confirmation.
    sqlx::query("UPDATE organization_membership SET role=CASE WHEN user_id=$2 THEN 'member' ELSE 'admin' END WHERE organization_id=$1 AND user_id IN ($2,$3)").bind(f.org).bind(f.actor).bind(f.member).execute(&migrator).await.unwrap();
    let current = CommandContext {
        actor_user_id: UserId::new(f.member),
        ..f.ctx.clone()
    };
    assert!(metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let paused = metadata::detail(&f.pool, &f.key, &current, child, &f.policy)
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "authority_changed");
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":0,"options":0,"links":0,"values":0})
    );
    drain(&f).await;
    assert_eq!(
        metadata::detail(&f.pool, &f.key, &current, child, &f.policy)
            .await
            .unwrap()["state"],
        "paused"
    );
    assert!(matches!(
        metadata::action(
            &f.pool,
            &f.key,
            &f.ctx,
            child,
            metadata::ImportRequest {
                request_id: Uuid::new_v4()
            },
            true,
            &f.policy
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    let retry = metadata::action(
        &f.pool,
        &f.key,
        &current,
        child,
        metadata::ImportRequest {
            request_id: Uuid::new_v4(),
        },
        true,
        &f.policy,
    )
    .await
    .unwrap();
    assert_eq!(retry["import"]["state"], "queued");
    assert_eq!(
        retry["import"]["latest_plan"]["id"],
        ready["latest_plan"]["id"]
    );
    assert_eq!(
        retry["import"]["latest_plan"]["confirmation_digest"],
        ready["latest_plan"]["confirmation_digest"]
    );
    let actor: Uuid = sqlx::query_scalar(
        "SELECT executor_user_id FROM migration_metadata_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(child)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(actor, f.member);
    drain(&f).await;
    let completed = metadata::detail(&f.pool, &f.key, &current, child, &f.policy)
        .await
        .unwrap();
    assert_eq!(completed["state"], "completed");
    assert_eq!(
        native(&f).await,
        json!({"tags":1,"fields":1,"options":0,"links":1,"values":1})
    );
    let actors:Vec<Uuid>=sqlx::query_scalar("SELECT created_by_user_id FROM tag WHERE organization_id=$1 UNION ALL SELECT created_by_user_id FROM custom_field WHERE organization_id=$1 UNION ALL SELECT added_by_user_id FROM person_tag WHERE organization_id=$1 UNION ALL SELECT updated_by_user_id FROM person_custom_field_value WHERE organization_id=$1").bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(actors.len(), 4);
    assert!(actors.iter().all(|actor| *actor == f.member));
    let parent_actor: Uuid = sqlx::query_scalar(
        "SELECT executor_user_id FROM migration_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(parent_actor, f.actor);
    assert_eq!(f.reader.calls(), calls);
    result_counts(&f, child, &current).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn metadata_r1_qualified_oversized_machine_key_does_not_offer_creation(migrator: PgPool) {
    let key = format!("custom{}", "x".repeat(2049));
    let mut p = person(101);
    p[&key] = json!("retained oversized-key value");
    p["customSafe"] = json!("independent value");
    let large =
        json!({"id":21,"name":key,"label":"Representable label","type":"text","isRecurring":false});
    let (f, child, v1) = fixture(
        &migrator,
        vec![p],
        vec![large, field(22, "customSafe", "text")],
    )
    .await;
    let wire = metadata::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        child,
        plan(&v1),
        metadata::MetadataPage::default(),
    )
    .await
    .unwrap();
    let large = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["source_id"] == "21")
        .unwrap();
    let safe = wire["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["source_id"] == "22")
        .unwrap();
    assert_eq!(large["qualified"], true);
    assert_eq!(large["create_matching_available"], false);
    assert_eq!(safe["create_matching_available"], true);
    let rejected=metadata::replan(&f.pool,&f.key,&f.ctx,child,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_plan_revision":v1["latest_plan"]["revision"],"mappings":[{"mapping_id":large["id"],"choice":{"kind":"create_matching"}}]})).unwrap(),&f.policy).await;
    assert!(matches!(
        rejected,
        Err(MigrationError::SourceNotEligible | MigrationError::InvalidImportChoice)
    ));
    assert_eq!(detail(&f, child).await["latest_plan"]["revision"], "1");
    let ready = approve(&f, child, &v1).await;
    let (row, _) = manifest(&f, plan(&ready), "101").await;
    let full = full_fields(&f, &f.ctx, child, plan(&ready), row.get("id"), false).await;
    assert_eq!(full["source"][&key], "\"retained oversized-key value\"");
    execute(&f, child, &ready).await;
    assert_eq!(
        native(&f).await,
        json!({"tags":0,"fields":1,"options":0,"links":0,"values":1})
    );
    result_counts(&f, child, &f.ctx).await;
}

//! Synthetic retained-evidence qualification through the real import worker.
//! Corruptions below use the migrator only to damage already captured evidence;
//! all initial source rows and all preparation use the normal typed pipelines.
use std::sync::Arc;

use crate::import_support::{
    drain_import, fixture, fixture_with_book, propose, replan, Book, Fixture,
};
use crm_api::domain::migration::{
    crypto,
    imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
    snapshot_source::Stream,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

fn person(id: u64) -> Value {
    json!({"id":id,"firstName":"Synthetic Person","stage":"Lead","assignedUserId":3})
}

async fn ready(f: &Fixture, plan: Uuid) {
    drain_import(f).await;
    let state: String = sqlx::query_scalar(
        "SELECT state FROM migration_import_plan WHERE id=$1 AND organization_id=$2",
    )
    .bind(plan)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        state, "ready",
        "preparation must settle with held records, not stall"
    );
}

/// Approve every emitted mapping explicitly. These choices must never cure bad
/// source evidence; mapping/manifest eligibility is re-evaluated by production.
async fn mapped_plan(f: &Fixture) -> (Uuid, Uuid) {
    let calls = f.reader.calls();
    let (run, first) = propose(f).await;
    ready(f, first).await;
    let rows = sqlx::query("SELECT kind,source_key FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 ORDER BY kind,source_key")
        .bind(first).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert!(rows.len() <= 50);
    let mut stages = vec![];
    let mut assignees = vec![];
    for row in rows {
        let source_key = row.get("source_key");
        match row.get::<String, _>("kind").as_str() {
            "stage" => stages.push(StagePatch {
                source_key,
                choice: StageChoice::Existing {
                    stage_id: f.lead_stage,
                },
            }),
            "assignee" => assignees.push(AssigneePatch {
                source_key,
                choice: AssigneeChoice::Member { user_id: f.actor },
            }),
            _ => panic!("unknown mapping kind"),
        }
    }
    let (_, plan) = replan(f, run, "1", &stages, &assignees).await;
    ready(f, plan).await;
    assert_eq!(f.reader.calls(), calls, "preparation must not call FUB");
    (run, plan)
}

async fn manifest(f: &Fixture, plan: Uuid, source: &str) -> PgRow {
    sqlx::query("SELECT * FROM migration_import_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id=$3")
        .bind(plan).bind(f.org).bind(source).fetch_one(&f.pool).await.unwrap()
}

fn open(f: &Fixture, plan: Uuid, row: &PgRow, purpose: &str) -> Value {
    let bytes = crypto::open_snapshot(
        &f.key,
        f.ctx.organization_id,
        f.snapshot,
        row.get("id"),
        &format!("import-v1:{plan}:{purpose}"),
        row.get("nonce"),
        row.get("ciphertext"),
    )
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn contains(values: &Value, code: &str) -> bool {
    values
        .as_array()
        .is_some_and(|items| items.iter().any(|value| value == code))
}

async fn assert_no_people(f: &Fixture) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "preparation cannot create business rows");
}

async fn execute(f: &Fixture, run: Uuid, plan: Uuid) {
    let before = f.reader.calls();
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
        .await
        .unwrap();
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        imports::ConfirmPeopleImport {
            request_id: Uuid::new_v4(),
            plan_id: plan,
            plan_revision: detail["plan"]["revision"].as_str().unwrap().into(),
            confirmation_digest: detail["plan"]["confirmation_digest"]
                .as_str()
                .unwrap()
                .into(),
            acknowledgments: imports::Acknowledgments {
                held_count: detail["counts"]["held_people"].as_str().unwrap().into(),
                review_only: true,
                remaining_data: true,
            },
        },
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    drain_import(f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
        .await
        .unwrap();
    assert_eq!(detail["state"], "completed", "{detail}");
    assert_eq!(f.reader.calls(), before, "execution must not call FUB");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_exact_ids_and_unknown_numeric_metadata_come_from_raw(migrator: PgPool) {
    let large_id = "9".repeat(128);
    let book = Arc::new(Book::new(vec![]));
    book.set_raw(Stream::People, 0, 200, format!(r#"{{"_metadata":{{"collection":"people","limit":100,"offset":0,"total":2}},"people":[{{"id":{large_id},"firstName":" Exact name ","stage":"Lead","assignedUserId":3,"arbitrary":123456789012345678901234567890.123456789,"unknown":{{"consent":false}},"created":null}},{{"id":9007199254740993,"firstName":"Other","stage":"Lead"}}]}}"#).into_bytes(), false);
    let f = fixture_with_book(&migrator, book).await;
    // Tamper only with the bounded preview projection: executable fields must
    // still be extracted from the original raw capture and its semantic HMAC.
    let observation = sqlx::query("SELECT id FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family='people' AND source_id=$3")
        .bind(f.snapshot).bind(f.org).bind(&large_id).fetch_one(&migrator).await.unwrap();
    let observation_id: Uuid = observation.get("id");
    let projection = crypto::seal_snapshot(
        &f.key,
        f.ctx.organization_id,
        f.snapshot,
        observation_id,
        "record",
        br#"{"firstName":"WRONG DISPLAY VALUE"}"#,
    )
    .unwrap();
    sqlx::query("UPDATE migration_snapshot_record SET projection_nonce=$2,projection_ciphertext=$3 WHERE id=$1")
        .bind(observation_id).bind(projection.nonce.as_slice()).bind(projection.ciphertext).execute(&migrator).await.unwrap();
    let (_, plan) = mapped_plan(&f).await;
    for id in [large_id.as_str(), "9007199254740993"] {
        assert_eq!(
            manifest(&f, plan, id).await.get::<String, _>("disposition"),
            "eligible"
        );
    }
    let source = sqlx::query("SELECT * FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id=$3")
        .bind(plan).bind(f.org).bind(&large_id).fetch_one(&f.pool).await.unwrap();
    assert!(source.get::<bool, _>("qualified"));
    let retained = open(&f, plan, &source, "source");
    assert_eq!(retained["source_id"], large_id);
    assert_eq!(retained["entity"]["value"]["first_name"], " Exact name ");
    assert_eq!(
        retained["provenance"]["arbitrary"],
        "123456789012345678901234567890123456789e-9"
    );
    assert_eq!(retained["provenance"]["unknown"], r#"{"consent":false}"#);
    assert_eq!(retained["provenance"]["created"], "null");
    assert!(retained["provenance"].get("updated").is_none());
    assert_no_people(&f).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_capture_and_observation_qualification_cannot_be_fixed_by_mappings(
    migrator: PgPool,
) {
    let f = fixture(&migrator, vec![person(101)]).await;
    let record = sqlx::query("SELECT * FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family='people'")
        .bind(f.snapshot).bind(f.org).fetch_one(&migrator).await.unwrap();
    let record_id: Uuid = record.get("id");
    let capture_id: Uuid = record.get("capture_id");
    let capture = sqlx::query("SELECT * FROM migration_snapshot_capture WHERE id=$1")
        .bind(capture_id)
        .fetch_one(&migrator)
        .await
        .unwrap();
    for case in [
        "ordinal",
        "source_id",
        "semantic_hmac",
        "record_representation",
        "record_family",
        "capture_representation",
        "status",
        "truncated",
        "accepted",
        "classification",
        "raw_length",
        "duplicate_keys",
        "raw_id",
    ] {
        // Restore the exact original captured row between independent plans.
        sqlx::query("UPDATE migration_snapshot_record SET ordinal=$2,source_id=$3,family=$4,representation=$5,semantic_hmac=$6 WHERE id=$1")
            .bind(record_id).bind(record.get::<i32,_>("ordinal")).bind(record.get::<Option<String>,_>("source_id"))
            .bind(record.get::<String,_>("family")).bind(record.get::<String,_>("representation")).bind(record.get::<Vec<u8>,_>("semantic_hmac")).execute(&migrator).await.unwrap();
        sqlx::query("UPDATE migration_snapshot_capture SET representation=$2,http_status=$3,truncated=$4,accepted=$5,classification=$6,raw_byte_len=$7,nonce=$8,ciphertext=$9 WHERE id=$1")
            .bind(capture_id).bind(capture.get::<String,_>("representation")).bind(capture.get::<i32,_>("http_status"))
            .bind(capture.get::<bool,_>("truncated")).bind(capture.get::<bool,_>("accepted")).bind(capture.get::<String,_>("classification"))
            .bind(capture.get::<i64,_>("raw_byte_len")).bind(capture.get::<Vec<u8>,_>("nonce")).bind(capture.get::<Vec<u8>,_>("ciphertext")).execute(&migrator).await.unwrap();
        let change = match case {
            "ordinal" => Some(("UPDATE migration_snapshot_record SET ordinal=99 WHERE id=$1", record_id)),
            "source_id" => Some(("UPDATE migration_snapshot_record SET source_id='999' WHERE id=$1", record_id)),
            "semantic_hmac" => Some(("UPDATE migration_snapshot_record SET semantic_hmac=decode(repeat('00',32),'hex') WHERE id=$1", record_id)),
            "record_representation" => Some(("UPDATE migration_snapshot_record SET representation='unqualified' WHERE id=$1", record_id)),
            "record_family" => Some(("UPDATE migration_snapshot_record SET family='tasks' WHERE id=$1", record_id)),
            "capture_representation" => Some(("UPDATE migration_snapshot_capture SET representation='unqualified' WHERE id=$1", capture_id)),
            "status" => Some(("UPDATE migration_snapshot_capture SET http_status=403 WHERE id=$1", capture_id)),
            "truncated" => Some(("UPDATE migration_snapshot_capture SET truncated=true WHERE id=$1", capture_id)),
            "accepted" => Some(("UPDATE migration_snapshot_capture SET accepted=false WHERE id=$1", capture_id)),
            "classification" => Some(("UPDATE migration_snapshot_capture SET classification='no_progress' WHERE id=$1", capture_id)),
            "raw_length" => Some(("UPDATE migration_snapshot_capture SET raw_byte_len=raw_byte_len+1 WHERE id=$1", capture_id)),
            _ => None,
        };
        if let Some((sql, id)) = change {
            sqlx::query(sql).bind(id).execute(&migrator).await.unwrap();
        } else {
            let body = if case == "duplicate_keys" {
                br#"{"people":[{"id":101,"firstName":"Ada","firstName":"Other","stage":"Lead","assignedUserId":3}]}"#.as_slice()
            } else {
                br#"{"people":[{"id":999,"firstName":"Synthetic Person","stage":"Lead","assignedUserId":3}]}"#.as_slice()
            };
            let sealed = crypto::seal_snapshot(
                &f.key,
                f.ctx.organization_id,
                f.snapshot,
                capture_id,
                "capture",
                body,
            )
            .unwrap();
            sqlx::query("UPDATE migration_snapshot_capture SET nonce=$2,ciphertext=$3,raw_byte_len=$4 WHERE id=$1")
                .bind(capture_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(body.len() as i64).execute(&migrator).await.unwrap();
        }
        let (_, plan) = mapped_plan(&f).await;
        let source_id = if case == "source_id" { "999" } else { "101" };
        let item = manifest(&f, plan, source_id).await;
        assert_eq!(item.get::<String, _>("disposition"), "held", "{case}");
        let body = open(&f, plan, &item, "manifest");
        assert!(
            contains(&body["reasons"], "source_unqualified")
                || contains(&body["reasons"], "source_conflict"),
            "{case}: {body}"
        );
    }
    assert_no_people(&f).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_rejected_equal_observation_never_wins_but_disagreement_holds(
    migrator: PgPool,
) {
    for disagree in [false, true] {
        let mut people: Vec<_> = (1..=100).map(person).collect();
        let mut repeated = person(1);
        if disagree {
            repeated["unknown_only_change"] = json!({"decimal":"still source evidence"});
        }
        people.extend([repeated, person(101)]);
        let f = fixture(&migrator, people).await;
        let rejected: Uuid = sqlx::query_scalar("SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people' ORDER BY sequence DESC LIMIT 1")
            .bind(f.snapshot).bind(f.org).fetch_one(&migrator).await.unwrap();
        sqlx::query("UPDATE migration_snapshot_capture SET accepted=false,classification='no_progress' WHERE id=$1").bind(rejected).execute(&migrator).await.unwrap();
        let (_, plan) = mapped_plan(&f).await;
        let source = sqlx::query("SELECT qualified,conflict,observations,capture_id FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id='1'")
            .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
        assert!(source.get::<bool, _>("qualified"));
        assert_eq!(source.get::<bool, _>("conflict"), disagree);
        assert_eq!(source.get::<i64, _>("observations"), 2);
        assert_ne!(source.get::<Uuid, _>("capture_id"), rejected);
        assert_eq!(
            manifest(&f, plan, "1")
                .await
                .get::<String, _>("disposition"),
            if disagree { "held" } else { "eligible" }
        );
        assert_eq!(
            manifest(&f, plan, "101")
                .await
                .get::<String, _>("disposition"),
            "held"
        );
        assert_no_people(&f).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_accepted_semantic_duplicates_collapse_and_unknown_variants_hold(
    migrator: PgPool,
) {
    let book = Arc::new(Book::new(vec![]));
    book.set_raw(Stream::People, 0, 200, br#"{"_metadata":{"collection":"people","limit":100,"offset":0,"total":4},"people":[{"id":1,"firstName":"Equal","stage":"Lead","unknown":1},{"unknown":1.00,"stage":"Lead","firstName":"Equal","id":1.0},{"id":2,"firstName":"Variant","stage":"Lead","unknown":false},{"id":2,"firstName":"Variant","stage":"Lead","unknown":true}]}"#.to_vec(), false);
    let f = fixture_with_book(&migrator, book).await;
    let (_, plan) = mapped_plan(&f).await;
    assert_eq!(
        manifest(&f, plan, "1")
            .await
            .get::<String, _>("disposition"),
        "eligible"
    );
    let conflict = manifest(&f, plan, "2").await;
    assert_eq!(conflict.get::<String, _>("disposition"), "held");
    assert!(contains(
        &open(&f, plan, &conflict, "manifest")["reasons"],
        "source_conflict"
    ));
    let sources = sqlx::query("SELECT source_id,observations,conflict FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' ORDER BY source_id")
        .bind(plan).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(sources.len(), 2);
    for source in sources {
        assert_eq!(source.get::<i64, _>("observations"), 2);
        assert_eq!(
            source.get::<bool, _>("conflict"),
            source.get::<String, _>("source_id") == "2"
        );
    }
    assert_no_people(&f).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_missing_observation_identity_is_counted_without_fabricating_person(
    migrator: PgPool,
) {
    let f = fixture(&migrator, vec![person(101), person(102)]).await;
    // Damage only the retained observation's identity. The raw item must not be
    // promoted into a new import identity by trusting its ID over the observation.
    sqlx::query("UPDATE migration_snapshot_record SET source_id=NULL WHERE snapshot_id=$1 AND organization_id=$2 AND family='people' AND source_id='101'")
        .bind(f.snapshot).bind(f.org).execute(&migrator).await.unwrap();
    let (_, plan) = mapped_plan(&f).await;
    let counts = sqlx::query("SELECT invalid_ids,source_people FROM migration_import_plan WHERE id=$1 AND organization_id=$2")
        .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts.get::<i64, _>("invalid_ids"), 1);
    assert_eq!(counts.get::<i64, _>("source_people"), 1);
    let source_ids: Vec<String> = sqlx::query_scalar("SELECT source_id FROM migration_import_manifest WHERE plan_id=$1 AND organization_id=$2 ORDER BY source_id")
        .bind(plan).bind(f.org).fetch_all(&f.pool).await.unwrap();
    assert_eq!(source_ids, vec!["102"]);
    assert_no_people(&f).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_supporting_stage_and_user_variants_hold_only_dependents(migrator: PgPool) {
    for family in [Stream::Stages, Stream::Users] {
        let book = Arc::new(Book::new(vec![
            person(101),
            json!({"id":102,"firstName":"Independent","stage":"Sphere"}),
        ]));
        book.set_records(
            Stream::Stages,
            vec![
                json!({"id":4,"name":"Lead"}),
                json!({"id":5,"name":"Sphere"}),
            ],
        );
        let (id, kind) = if family == Stream::Stages {
            book.set_records(
                family,
                vec![
                    json!({"id":4,"name":"Lead","unknown":false}),
                    json!({"id":4,"name":"Lead","unknown":true}),
                    json!({"id":5,"name":"Sphere"}),
                ],
            );
            ("4", "stage")
        } else {
            book.set_records(
                family,
                vec![
                    json!({"id":3,"name":"Source User","unknown":false}),
                    json!({"id":3,"name":"Source User","unknown":true}),
                ],
            );
            ("3", "assignee")
        };
        let f = fixture_with_book(&migrator, book).await;
        let (_, plan) = mapped_plan(&f).await;
        let mapping = sqlx::query("SELECT qualified,disposition FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4")
            .bind(plan).bind(f.org).bind(kind).bind(id).fetch_one(&f.pool).await.unwrap();
        assert!(!mapping.get::<bool, _>("qualified"));
        assert_eq!(mapping.get::<String, _>("disposition"), "hold");
        assert_eq!(
            manifest(&f, plan, "101")
                .await
                .get::<String, _>("disposition"),
            "held"
        );
        assert_eq!(
            manifest(&f, plan, "102")
                .await
                .get::<String, _>("disposition"),
            "eligible"
        );
        assert_no_people(&f).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_native_holds_primary_order_and_cross_person_overlap_survive_manifest(
    migrator: PgPool,
) {
    let limit = format!("{}@x", "é".repeat(1023));
    assert_eq!(limit.len(), 2048);
    let people = vec![
        json!({"id":1,"firstName":"Ordered","stage":"Lead","emails":[{"value":" shared@example.test ","isPrimary":0,"label":"original"},{"value":"other@example.test","isPrimary":0},{"value":"SHARED@example.test","isPrimary":1,"unknownStatus":false}]}),
        json!({"id":2,"firstName":"Separate","stage":"Lead","emails":[{"value":"shared@example.test"}]}),
        json!({"id":3,"firstName":"Boundary","stage":"Lead","emails":[{"value":limit}]}),
        json!({"id":4,"firstName":"Oversize","stage":"Lead","emails":[{"value":format!("{limit}a")}]}),
        json!({"id":5,"firstName":"Contains\u{0000}NUL","stage":"Lead"}),
        json!({"id":6,"firstName":"Trash flag","stage":"Lead","isTrash":true}),
        json!({"id":7,"firstName":"Unknown preference","stage":"Lead","emails":[{"value":"valid@example.test","isPrimary":true}],"unknownCommunicationFlag":false}),
    ];
    let f = fixture(&migrator, people).await;
    let (run, plan) = mapped_plan(&f).await;
    for id in ["1", "2", "3", "7"] {
        assert_eq!(
            manifest(&f, plan, id).await.get::<String, _>("disposition"),
            "eligible"
        );
    }
    for (id, reason) in [
        ("4", "email_normalized_value_too_large"),
        ("5", "native_text_contains_nul"),
        ("6", "trash_person"),
    ] {
        let item = manifest(&f, plan, id).await;
        assert_eq!(item.get::<String, _>("disposition"), "held");
        assert!(contains(
            &open(&f, plan, &item, "manifest")["reasons"],
            reason
        ));
    }
    for id in ["1", "2"] {
        assert!(manifest(&f, plan, id).await.get::<i64, _>("overlap_count") > 0);
    }
    let source = sqlx::query("SELECT * FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id='1'")
        .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let body = open(&f, plan, &source, "source");
    let contacts = body["entity"]["value"]["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 2);
    assert_eq!(contacts[0]["value"], "SHARED@example.test");
    assert_eq!(contacts[0]["import_order"], 0);
    assert_eq!(contacts[1]["value"], "other@example.test");
    assert!(body["provenance"]["emails"]
        .as_str()
        .unwrap()
        .contains("unknownStatus"));
    let uncertain = manifest(&f, plan, "7").await;
    assert!(contains(
        &open(&f, plan, &uncertain, "manifest")["transformations"],
        "email_primary_unqualified"
    ));
    assert_no_people(&f).await;
    execute(&f, run, plan).await;
    let rows = sqlx::query("SELECT i.source_id,c.person_id,c.kind,c.value,c.normalized_value,c.import_order FROM migration_import_identity i JOIN contact_method c ON c.person_id=i.target_id AND c.organization_id=i.organization_id WHERE i.import_id=$1 AND i.organization_id=$2 AND i.family='people' ORDER BY i.source_id,c.import_order")
        .bind(run).bind(f.org).fetch_all(&migrator).await.unwrap();
    let first = rows
        .iter()
        .find(|row| row.get::<String, _>("source_id") == "1")
        .unwrap();
    let separate = rows
        .iter()
        .find(|row| row.get::<String, _>("source_id") == "2")
        .unwrap();
    assert_eq!(first.get::<String, _>("value"), "SHARED@example.test");
    assert_eq!(first.get::<Option<i32>, _>("import_order"), Some(0));
    assert_ne!(
        first.get::<Uuid, _>("person_id"),
        separate.get::<Uuid, _>("person_id")
    );
    let boundary = rows
        .iter()
        .find(|row| row.get::<String, _>("source_id") == "3")
        .unwrap();
    assert_eq!(boundary.get::<String, _>("normalized_value").len(), 2048);
    let imported: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(imported, 4);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_large_provenance_and_oversized_stage_remain_exact_and_mappable(
    migrator: PgPool,
) {
    let source_url = format!(
        "https://synthetic.invalid/{}",
        "x".repeat(2 * 1024 * 1024 + 17)
    );
    let stage_label = format!("{}x", "é".repeat(1024));
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Full provenance","stage":stage_label,"sourceUrl":source_url,"source":"Original source","created":"2020-01-02T03:04:05Z"}),
    ]));
    book.set_records(Stream::Stages, vec![json!({"id":4,"name":stage_label})]);
    let f = fixture_with_book(&migrator, book).await;
    let (run, plan) = mapped_plan(&f).await;
    let item = manifest(&f, plan, "101").await;
    assert_eq!(item.get::<String, _>("disposition"), "eligible");
    assert!(item.get::<i64, _>("added_byte_bound") > 2 * 1024 * 1024);
    assert!(item.get::<i64, _>("added_byte_bound") <= imports::UNIT);
    let source = sqlx::query("SELECT * FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id='101'")
        .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let body = open(&f, plan, &source, "source");
    assert_eq!(
        body["provenance"]["sourceUrl"],
        serde_json::to_string(&source_url).unwrap()
    );
    assert_eq!(body["entity"]["value"]["stage_label"], stage_label);
    let mapping = sqlx::query("SELECT * FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='stage' AND source_key='4'")
        .bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert!(mapping.get::<bool, _>("qualified"));
    assert_eq!(mapping.get::<String, _>("disposition"), "existing");
    assert_no_people(&f).await;
    execute(&f, run, plan).await;
    let result = sqlx::query("SELECT id,provenance_nonce,provenance_ciphertext FROM migration_import_result WHERE import_id=$1 AND organization_id=$2 AND source_id='101' AND disposition='imported'")
        .bind(run).bind(f.org).fetch_one(&migrator).await.unwrap();
    let plaintext = crypto::open_snapshot(
        &f.key,
        f.ctx.organization_id,
        f.snapshot,
        result.get("id"),
        &format!("import-v1:{plan}:provenance"),
        result.get("provenance_nonce"),
        result.get("provenance_ciphertext"),
    )
    .unwrap();
    assert!(plaintext.len() > 2 * 1024 * 1024);
    let provenance: Value = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(
        provenance["provenance"]["sourceUrl"],
        serde_json::to_string(&source_url).unwrap()
    );
    assert_eq!(
        provenance["provenance"]["created"],
        r#""2020-01-02T03:04:05Z""#
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_nul_supporting_fields_skip_suggestions_and_allow_explicit_mapping(
    migrator: PgPool,
) {
    let stage = "Synthetic\0captured stage";
    let email = "source\0user@synthetic.test";
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Synthetic","stage":stage,"assignedUserId":3}),
    ]));
    book.set_records(Stream::Stages, vec![json!({"id":4,"name":stage})]);
    book.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":"Synthetic source","email":email})],
    );
    let f = fixture_with_book(&migrator, book).await;
    let (run, plan) = mapped_plan(&f).await;
    let detail = imports::detail(&f.pool, &f.key, &f.ctx, run, &f.policy)
        .await
        .unwrap();
    assert_eq!(detail["counts"]["eligible_people"], "1");
    for (kind, expected) in [("stage", stage), ("assignee", email)] {
        let row = sqlx::query("SELECT * FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3")
            .bind(plan).bind(f.org).bind(kind).fetch_one(&f.pool).await.unwrap();
        let body = open(&f, plan, &row, "mapping");
        assert_eq!(body["suggestions"], json!([]));
        let field = if kind == "stage" { "name" } else { "email" };
        assert_eq!(
            body["source"]["provenance"][field],
            serde_json::to_string(expected).unwrap()
        );
        assert!(row.get::<bool, _>("qualified"));
        assert_eq!(
            row.get::<String, _>("disposition"),
            if kind == "stage" {
                "existing"
            } else {
                "member"
            }
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn import_source_mapping_fields_are_exact_bounded_and_http_scoped(migrator: PgPool) {
    use crate::common::{body_json, get_with_cookie};
    use axum::http::StatusCode;
    let label = "😀".repeat(400);
    let user_name = "Synthetic λ".repeat(140);
    let book = Arc::new(Book::new(vec![
        json!({"id":101,"firstName":"Synthetic","stage":label,"assignedUserId":3}),
    ]));
    book.set_records(
        Stream::Stages,
        vec![json!({"id":4,"name":label,"sourceDetail":{"exact":9007199254740993u64}})],
    );
    book.set_records(
        Stream::Users,
        vec![json!({"id":3,"name":user_name,"email":"synthetic@invalid.test"})],
    );
    let f = fixture_with_book(&migrator, book).await;
    let (run, plan) = mapped_plan(&f).await;
    let calls = f.reader.calls();
    let base = format!("/api/migrations/fub/imports/{run}/plans/{plan}");
    let mut first_cursor = None;
    let mut first_url = None;
    for (kind, value_name, expected) in
        [("stage", "label", &label), ("assignee", "name", &user_name)]
    {
        let response =
            get_with_cookie(&f.app, &format!("{base}/mappings?kind={kind}"), &f.cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        let page = body_json(response).await;
        let mapping = &page["mappings"][0];
        assert_eq!(mapping["source"][value_name]["abbreviated"], true);
        let mapping_id = mapping["id"].as_str().unwrap();
        let field = mapping["source"][value_name]["field_key"].as_str().unwrap();
        let url = format!("{base}/mappings/{mapping_id}/fields/{field}");
        let mut cursor = None::<String>;
        let mut text = String::new();
        for _ in 0..64 {
            let query = match &cursor {
                Some(cursor) => format!("?limit=64&cursor={cursor}"),
                None => "?limit=64".into(),
            };
            let response = get_with_cookie(&f.app, &format!("{url}{query}"), &f.cookie).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let value = body_json(response).await;
            assert_eq!(value["offset"], text.len().to_string());
            let part = value["text"].as_str().unwrap();
            assert!(!part.is_empty() && part.len() <= 64);
            text.push_str(part);
            cursor = value["next_cursor"].as_str().map(str::to_owned);
            if first_cursor.is_none() {
                first_cursor = cursor.clone();
                first_url = Some(url.clone());
            }
            if cursor.is_none() {
                break;
            }
        }
        assert!(cursor.is_none(), "bounded fixture must finish");
        assert_eq!(text, serde_json::to_string(expected).unwrap());
        for (bad, status) in [
            (format!("{url}?limit=1"), StatusCode::BAD_REQUEST),
            (
                format!("{base}/mappings/{mapping_id}/fields/not_a_field"),
                StatusCode::NOT_FOUND,
            ),
            (
                format!("{base}/mappings/{}/fields/{field}", Uuid::new_v4()),
                StatusCode::NOT_FOUND,
            ),
        ] {
            assert_eq!(
                get_with_cookie(&f.app, &bad, &f.cookie).await.status(),
                status
            );
        }
        assert_eq!(
            get_with_cookie(&f.app, &url, &f.member_cookie)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        let wrong_scope = format!(
            "{base}/mappings/{mapping_id}/fields/provenance?cursor={}",
            first_cursor.as_ref().unwrap()
        );
        assert_eq!(
            get_with_cookie(&f.app, &wrong_scope, &f.cookie)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        let exact = get_with_cookie(
            &f.app,
            &format!("{base}/mappings/{mapping_id}/fields/provenance"),
            &f.cookie,
        )
        .await;
        assert_eq!(exact.status(), StatusCode::OK);
        let value = body_json(exact).await;
        assert!(value["text"].as_str().unwrap().contains(
            &serde_json::to_string(expected)
                .unwrap()
                .replace('"', "\\\"")
        ));
    }
    let foreign = fixture(&migrator, vec![person(201)]).await;
    assert_eq!(
        get_with_cookie(&f.app, first_url.as_ref().unwrap(), &foreign.cookie)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let manifest_id: Uuid =
        sqlx::query_scalar("SELECT id FROM migration_import_manifest WHERE plan_id=$1")
            .bind(plan)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let swapped = format!(
        "{base}/records/{manifest_id}/fields/provenance?cursor={}",
        first_cursor.unwrap()
    );
    assert_eq!(
        get_with_cookie(&f.app, &swapped, &f.cookie).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.reader.calls(), calls);
}

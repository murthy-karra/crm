//! D-050: one 25,000-source-Person book, not a production capacity benchmark.
//!
//! Migration rows below are deliberately seeded by the migrator for query-plan
//! and comparison scale evidence. They are NOT evidence of HTTP capture fidelity
//! or source enumeration. Every projection the real preview engine decrypts has
//! the normal tenant/run/row/purpose AAD. Business setup uses sanctioned helpers.
//! EXPLAIN statements are extracted from the compiled production source, avoiding
//! a hand-maintained SQL copy that can silently stop measuring the actual query.
use std::collections::HashSet;

use crm_api::{
    config::RawPayloadKey,
    domain::{
        admin::{MembershipStatus, Role},
        envelope::{CommandContext, Origin},
        migration::{
            commands, crypto,
            reader::{FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::{self, PageQuery, SnapshotPolicy, SnapshotRequest, SourceAction},
            snapshot_preview,
            snapshot_source::Stream,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const PEOPLE: usize = 25_000;
const DUPLICATE_OBSERVATIONS: usize = 100;
const SNAPSHOT_SOURCE: &str = include_str!("../../crm-app/src/domain/migration/snapshot.rs");
const WORKER_SOURCE: &str = include_str!("../../crm-app/src/domain/migration/snapshot_worker.rs");
const PREVIEW_SOURCE: &str = include_str!("../../crm-app/src/domain/migration/snapshot_preview.rs");

struct IdentityOnly;

#[async_trait::async_trait]
impl FubReader for IdentityOnly {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        Ok((
            Identity {
                account_id: 250,
                user_id: Some(1),
                account_domain: None,
                display_name: None,
            },
            br#"{"account":{"id":250},"user":{"id":1}}"#.to_vec(),
        ))
    }

    async fn probe(&self, _: &str, _: Probe) -> Result<ProbeResult, ReaderError> {
        panic!("the scale fixture must not execute source requests")
    }
}

struct Fixture {
    pool: PgPool,
    ctx: CommandContext,
    key: RawPayloadKey,
    policy: SnapshotPolicy,
    run: Uuid,
    phone_hmac: [u8; 32],
}

async fn fixture(migrator: &PgPool) -> Fixture {
    let org = crate::common::create_org(migrator, "Synthetic snapshot scale").await;
    let actor = crate::common::create_user(
        migrator,
        "snapshot-scale@synthetic.test",
        "Synthetic snapshot scale admin",
        "synthetic scale fixture password",
    )
    .await;
    crate::common::add_membership_with(migrator, org, actor, Role::Admin, MembershipStatus::Active)
        .await;
    let pool = crate::common::connect_as_app(migrator).await;
    let ctx = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(actor),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let key = RawPayloadKey::new([25; 32]);
    let policy = SnapshotPolicy::default();
    let connection = commands::connect_fub(
        &pool,
        &key,
        &IdentityOnly,
        &ctx,
        commands::ConnectFub {
            request_id: Uuid::new_v4(),
            api_key: "synthetic scale reader only".into(),
        },
    )
    .await
    .unwrap()
    .value;
    let proposed = snapshot::propose(
        &pool,
        &key,
        &policy,
        &ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection.id,
            expected_revision: connection.revision,
        },
    )
    .await
    .unwrap();
    let run = Uuid::parse_str(proposed["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &pool,
        &key,
        &policy,
        &ctx,
        run,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    Fixture {
        phone_hmac: crypto::snapshot_hmac(
            &key,
            ctx.organization_id,
            "contact:phone",
            b"+12025550100",
        ),
        pool,
        ctx,
        key,
        policy,
        run,
    }
}

async fn seed_observations(
    migrator: &PgPool,
    f: &Fixture,
    sequence: i64,
    first: usize,
    count: usize,
) {
    let capture_id = Uuid::new_v4();
    let capture = crypto::seal_snapshot(
        &f.key,
        f.ctx.organization_id,
        f.run,
        capture_id,
        "capture",
        br#"{"synthetic_plan_fixture":true}"#,
    )
    .unwrap();
    sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,classification,truncated,accepted) VALUES($1,$2,$3,'people',$4,$4,$5,$6,200,31,$7,$8,'synthetic_plan_fixture',false,true)")
        .bind(capture_id).bind(f.run).bind(f.ctx.organization_id.0).bind(sequence)
        .bind(vec![sequence as u8; 32]).bind(Stream::People.representation())
        .bind(capture.nonce.as_slice()).bind(capture.ciphertext)
        .execute(migrator).await.unwrap();

    // Bounded bind arrays, with fresh AEAD nonces and each real row's AAD.
    for chunk_first in (first..first + count).step_by(500) {
        let chunk_end = (chunk_first + 500).min(first + count);
        let mut ids = Vec::new();
        let mut source_ids = Vec::new();
        let mut ordinals = Vec::new();
        let mut semantics = Vec::new();
        let mut nonces = Vec::new();
        let mut ciphertexts = Vec::new();
        for source in chunk_first..chunk_end {
            let id = Uuid::new_v4();
            let plaintext = serde_json::to_vec(&json!({
                "id": source.to_string(), "firstName": "Synthetic", "lastName": "Scale",
                "stage": "Lead", "phones": [{"value": "+12025550100"}]
            }))
            .unwrap();
            let sealed = crypto::seal_snapshot(
                &f.key,
                f.ctx.organization_id,
                f.run,
                id,
                "record",
                &plaintext,
            )
            .unwrap();
            ids.push(id);
            source_ids.push(source.to_string());
            ordinals.push((source - first) as i32);
            semantics.push(
                crypto::snapshot_hmac(
                    &f.key,
                    f.ctx.organization_id,
                    &format!("semantic:{}", Stream::People.representation()),
                    &plaintext,
                )
                .to_vec(),
            );
            nonces.push(sealed.nonce.to_vec());
            ciphertexts.push(sealed.ciphertext);
        }
        sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) SELECT b.id,$1,$2,$3,$4,b.ordinal,'people',b.source_id,$5,b.semantic,b.nonce,b.ciphertext,false FROM unnest($6::uuid[],$7::text[],$8::integer[],$9::bytea[],$10::bytea[],$11::bytea[]) AS b(id,source_id,ordinal,semantic,nonce,ciphertext)")
            .bind(f.run).bind(f.ctx.organization_id.0).bind(capture_id).bind(sequence)
            .bind(Stream::People.representation()).bind(&ids).bind(&source_ids).bind(&ordinals)
            .bind(&semantics).bind(&nonces).bind(&ciphertexts)
            .execute(migrator).await.unwrap();
        sqlx::query("INSERT INTO migration_snapshot_contact_key(snapshot_id,organization_id,record_id,capture_sequence,source_id,kind,key_hmac) SELECT $1,$2,b.id,$3,b.source_id,'phone',$4 FROM unnest($5::uuid[],$6::text[]) AS b(id,source_id)")
            .bind(f.run).bind(f.ctx.organization_id.0).bind(sequence).bind(f.phone_hmac.as_slice())
            .bind(&ids).bind(&source_ids).execute(migrator).await.unwrap();
    }
}

async fn seed_history_and_statistics(migrator: &PgPool, f: &Fixture) {
    // Historical runs make latest/claim/stream index selection meaningful. One
    // queued history row is never executed; only EXPLAIN reads it.
    sqlx::query("INSERT INTO migration_snapshot(id,organization_id,connection_id,connection_revision,source_account_id,profile_version,schema_version,initiated_by_user_id,state,proposal_expires_at,original_run_byte_limit,run_byte_limit,created_at,next_attempt_at) SELECT gen_random_uuid(),s.organization_id,s.connection_id,s.connection_revision,s.source_account_id,s.profile_version,s.schema_version,s.initiated_by_user_id,CASE WHEN n=1 THEN 'queued' ELSE 'completed' END,now()-interval '1 day',s.original_run_byte_limit,s.run_byte_limit,now()-n*interval '1 minute',CASE WHEN n=1 THEN now()-interval '1 second' ELSE NULL END FROM migration_snapshot s CROSS JOIN generate_series(1,2000) n WHERE s.id=$1")
        .bind(f.run).execute(migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_snapshot_stream(snapshot_id,organization_id,stream,family,state) SELECT s.id,s.organization_id,b.stream,b.family,'completed' FROM migration_snapshot s CROSS JOIN (VALUES ('users','users'),('stages','stages'),('custom_fields','custom_fields'),('people','people'),('notes','notes'),('note_detail','notes'),('tasks_open','tasks'),('tasks_completed','tasks')) b(stream,family) WHERE s.organization_id=$1 AND s.id<>$2")
        .bind(f.ctx.organization_id.0).bind(f.run).execute(migrator).await.unwrap();
    for table in [
        "migration_snapshot",
        "migration_snapshot_stream",
        "migration_snapshot_capture",
        "migration_snapshot_record",
        "migration_snapshot_contact_key",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(migrator)
            .await
            .unwrap();
    }
}

/// Decode the Rust string literal directly (all measured SQL uses JSON-compatible
/// string escapes). A renamed/replaced production query fails here instead of
/// quietly measuring an obsolete test-only statement.
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

fn plan_nodes<'a>(value: &'a Value, result: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if object.contains_key("Node Type") {
                result.push(value);
            }
            for child in object.values() {
                plan_nodes(child, result);
            }
        }
        Value::Array(values) => {
            for child in values {
                plan_nodes(child, result);
            }
        }
        _ => {}
    }
}

fn indexed(plan: &Value, table: &str) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    assert!(
        nodes.iter().any(|node| {
            node["Relation Name"] == table
                && node["Node Type"]
                    .as_str()
                    .is_some_and(|kind| kind.contains("Index") || kind == "Bitmap Heap Scan")
        }),
        "expected indexed access for {table}: {plan}"
    );
}

fn bounded_relation_work(plan: &Value, table: &str, max_rows: f64) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    let examined: f64 = nodes
        .iter()
        .filter(|node| node["Relation Name"] == table)
        .map(|node| {
            let rows = node["Actual Rows"].as_f64().unwrap_or_default()
                + node["Rows Removed by Filter"].as_f64().unwrap_or_default();
            rows * node["Actual Loops"].as_f64().unwrap_or(1.0)
        })
        .sum();
    assert!(
        examined <= max_rows,
        "{table} examined {examined} rows for a bounded lookup: {plan}"
    );
}

macro_rules! explain {
    ($f:expr, $label:expr, $source:expr, $prefix:expr $(, $bind:expr)* $(,)?) => {{
        let sql = production_statement($source, $prefix);
        let statement = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan: Value = sqlx::query_scalar(&statement)$(.bind($bind))*
            .fetch_one(&$f.pool).await.unwrap();
        // Safe synthetic IDs/aggregates/plans only; no projection plaintext.
        eprintln!("SNAPSHOT_SCALE_PLAN {} {}", $label, plan);
        plan
    }};
}

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn shared_phone_25000_people_has_bounded_frozen_groups_and_indexed_queries(migrator: PgPool) {
    let f = fixture(&migrator).await;
    seed_observations(&migrator, &f, 1, 1, PEOPLE).await;
    seed_observations(&migrator, &f, 2, 1, DUPLICATE_OBSERVATIONS).await;
    sqlx::query("UPDATE migration_snapshot SET state='paused',pause_reason='synthetic_plan_fixture',capture_sequence=2,accepted_captures=2,identity_required=false,lease_token=NULL,lease_expires_at=NULL WHERE id=$1")
        .bind(f.run).execute(&migrator).await.unwrap();
    sqlx::query("UPDATE migration_snapshot_stream SET state='completed',checkpoint=2,accepted_captures=2,returned_items=$3,reported_total=$4 WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people'")
        .bind(f.run).bind(f.ctx.organization_id.0).bind((PEOPLE+DUPLICATE_OBSERVATIONS) as i64)
        .bind(PEOPLE.to_string()).execute(&migrator).await.unwrap();
    seed_history_and_statistics(&migrator, &f).await;

    let claim = explain!(
        f,
        "source_claim",
        WORKER_SOURCE,
        "SELECT id,organization_id FROM migration_snapshot WHERE"
    );
    indexed(&claim, "migration_snapshot");
    let latest = explain!(
        f,
        "latest_completed",
        SNAPSHOT_SOURCE,
        "SELECT id FROM migration_snapshot WHERE organization_id=$1 AND state IN ('completed'",
        f.ctx.organization_id.0
    );
    indexed(&latest, "migration_snapshot");
    let stream = explain!(
        f,
        "stream_dispatch",
        WORKER_SOURCE,
        "SELECT * FROM migration_snapshot_stream WHERE",
        f.run,
        f.ctx.organization_id.0
    );
    indexed(&stream, "migration_snapshot_stream");
    let summary = explain!(
        f,
        "stream_summary",
        SNAPSHOT_SOURCE,
        "SELECT s.*,(SELECT count(DISTINCT r.source_id)",
        f.run,
        f.ctx.organization_id.0
    );
    indexed(&summary, "migration_snapshot_stream");
    let checkpoint = explain!(
        f,
        "settled_checkpoint_evidence",
        WORKER_SOURCE,
        "SELECT 1 FROM migration_snapshot_capture WHERE",
        f.run,
        f.ctx.organization_id.0,
        "people",
        vec![2_u8; 32]
    );
    bounded_relation_work(&checkpoint, "migration_snapshot_capture", 2.0);
    let versions = explain!(
        f,
        "record_variants",
        PREVIEW_SOURCE,
        "SELECT representation,count(DISTINCT semantic_hmac)",
        f.run,
        f.ctx.organization_id.0,
        2_i64,
        "people",
        "12345"
    );
    indexed(&versions, "migration_snapshot_record");
    bounded_relation_work(&versions, "migration_snapshot_record", 2.0);
    let source_page = explain!(
        f,
        "source_record_page",
        PREVIEW_SOURCE,
        "SELECT family,source_id FROM migration_snapshot_record WHERE",
        f.run,
        f.ctx.organization_id.0,
        2_i64,
        "people",
        "12000"
    );
    indexed(&source_page, "migration_snapshot_record");
    bounded_relation_work(&source_page, "migration_snapshot_record", 120.0);
    let grouping = explain!(
        f,
        "contact_group_aggregation",
        PREVIEW_SOURCE,
        "SELECT kind,key_hmac,count(DISTINCT source_id)",
        f.run,
        f.ctx.organization_id.0,
        2_i64,
        "",
        Vec::<u8>::new()
    );
    // One intentional complete pass computes one group, never pair expansion.
    bounded_relation_work(
        &grouping,
        "migration_snapshot_contact_key",
        (PEOPLE + DUPLICATE_OBSERVATIONS) as f64,
    );

    let generated = snapshot_preview::generate(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        f.run,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
    )
    .await
    .unwrap();
    let preview = Uuid::parse_str(generated["preview_id"].as_str().unwrap()).unwrap();
    let preview_claim = explain!(
        f,
        "preview_claim",
        PREVIEW_SOURCE,
        "SELECT id,snapshot_id,organization_id FROM migration_snapshot_preview WHERE"
    );
    bounded_relation_work(&preview_claim, "migration_snapshot_preview", 1.0);

    // This future observation must not enter the already-frozen report, even
    // though group construction and record generation have not begun yet.
    seed_observations(&migrator, &f, 3, PEOPLE + 1, 1).await;
    sqlx::query("UPDATE migration_snapshot SET capture_sequence=3,accepted_captures=3 WHERE id=$1")
        .bind(f.run)
        .execute(&migrator)
        .await
        .unwrap();
    assert!(snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let overlap_count = explain!(
        f,
        "per_record_overlap_count",
        PREVIEW_SOURCE,
        "SELECT count(DISTINCT g.id) FROM migration_snapshot_contact_key",
        f.run,
        f.ctx.organization_id.0,
        2_i64,
        preview,
        "12345"
    );
    indexed(&overlap_count, "migration_snapshot_contact_key");
    bounded_relation_work(&overlap_count, "migration_snapshot_contact_key", 2.0);

    let mut batches = 1;
    loop {
        let more = snapshot_preview::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap();
        if !more {
            break;
        }
        batches += 1;
        assert!(
            batches <= PEOPLE / 50 + 3,
            "preview failed to progress in bounded batches"
        );
    }
    let report = snapshot_preview::detail(&f.pool, &f.key, &f.ctx, f.run, preview)
        .await
        .unwrap();
    assert_eq!(report["preview"]["state"], "completed");
    assert_eq!(report["preview"]["capture_sequence"], "2");
    let groups = snapshot_preview::groups(
        &f.pool,
        &f.key,
        &f.ctx,
        f.run,
        preview,
        PageQuery::default(),
    )
    .await
    .unwrap();
    assert_eq!(groups["groups"].as_array().unwrap().len(), 1);
    assert!(groups["next_cursor"].is_null());
    assert_eq!(groups["groups"][0]["member_count"], PEOPLE.to_string());
    let group = Uuid::parse_str(groups["groups"][0]["id"].as_str().unwrap()).unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_preview_record WHERE preview_id=$1 AND organization_id=$2")
        .bind(preview).bind(f.ctx.organization_id.0).fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, PEOPLE as i64);
    let wrong_group_counts: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_preview_record WHERE preview_id=$1 AND organization_id=$2 AND overlap_group_count<>1")
        .bind(preview).bind(f.ctx.organization_id.0).fetch_one(&f.pool).await.unwrap();
    assert_eq!(wrong_group_counts, 0);
    sqlx::query("ANALYZE migration_snapshot_preview_record")
        .execute(&migrator)
        .await
        .unwrap();

    let preview_page = explain!(
        f,
        "preview_record_page",
        PREVIEW_SOURCE,
        "SELECT * FROM migration_snapshot_preview_record WHERE preview_id=",
        preview,
        f.run,
        f.ctx.organization_id.0,
        "people",
        Option::<String>::None,
        "12000",
        51_i64
    );
    indexed(&preview_page, "migration_snapshot_preview_record");
    bounded_relation_work(&preview_page, "migration_snapshot_preview_record", 51.0);
    let group_page = explain!(
        f,
        "overlap_group_page",
        PREVIEW_SOURCE,
        "SELECT g.id,g.kind,g.member_count FROM migration_snapshot_preview_group",
        preview,
        f.run,
        f.ctx.organization_id.0,
        Uuid::nil(),
        Option::<String>::None,
        2_i64,
        51_i64
    );
    bounded_relation_work(&group_page, "migration_snapshot_preview_group", 1.0);
    let member_page = explain!(
        f,
        "overlap_member_page",
        PREVIEW_SOURCE,
        "SELECT DISTINCT k.source_id,r.id AS record_id FROM migration_snapshot_contact_key",
        f.run,
        f.ctx.organization_id.0,
        2_i64,
        preview,
        "phone",
        f.phone_hmac.as_slice(),
        "12000",
        51_i64
    );
    indexed(&member_page, "migration_snapshot_contact_key");
    indexed(&member_page, "migration_snapshot_preview_record");
    bounded_relation_work(&member_page, "migration_snapshot_contact_key", 120.0);
    bounded_relation_work(&member_page, "migration_snapshot_preview_record", 120.0);

    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut pages = 0;
    loop {
        let page = snapshot_preview::members(
            &f.pool,
            &f.key,
            &f.ctx,
            f.run,
            preview,
            group,
            PageQuery {
                limit: Some(50),
                cursor,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let members = page["members"].as_array().unwrap();
        assert!(!members.is_empty() && members.len() <= 50);
        assert!(serde_json::to_vec(&page).unwrap().len() < 16 * 1024);
        for member in members {
            assert!(
                seen.insert(member["source_id"].as_str().unwrap().to_owned()),
                "a source ID repeated across member pages"
            );
        }
        pages += 1;
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
        assert!(pages < PEOPLE / 50 + 1);
    }
    assert_eq!(pages, PEOPLE / 50);
    assert_eq!(seen, (1..=PEOPLE).map(|id| id.to_string()).collect());
    assert!(!seen.contains(&(PEOPLE + 1).to_string()));
    let counts = sqlx::query("SELECT (SELECT count(*) FROM migration_snapshot_preview_group WHERE preview_id=$1) AS groups,(SELECT count(*) FROM migration_snapshot_record WHERE snapshot_id=$2) AS observations,(SELECT count(*) FROM person WHERE organization_id=$3) AS business_people")
        .bind(preview).bind(f.run).bind(f.ctx.organization_id.0).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts.get::<i64, _>("groups"), 1);
    assert_eq!(
        counts.get::<i64, _>("observations"),
        (PEOPLE + DUPLICATE_OBSERVATIONS + 1) as i64
    );
    assert_eq!(counts.get::<i64, _>("business_people"), 0);
    eprintln!(
        "SNAPSHOT_SCALE_RESULT {}",
        json!({"selected_people":PEOPLE,"duplicate_observations":DUPLICATE_OBSERVATIONS,"excluded_later_observations":1,"groups":1,"member_pages":pages,"preview_batches":batches,"business_people_written":0,"capture_fidelity_test":false})
    );
}

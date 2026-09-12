//! One paired, serial operational Person-detail comparison for D-068 / 010f2.
//!
//! Both arms run in one executable against the same app-role pool and immutable
//! synthetic 25k-People/50-member fixture. Three affected reader bodies are frozen
//! at cd3b010; authentication, middleware and unchanged helpers are shared. No
//! request can select that arm. This is not import qualification or a full old
//! binary, and does not replace the separate concentrated activity plan checks.
//!
//! Run only in the coordinator's isolated SQLx performance slot, with absolute
//! CRM_010F2_PERSON_PERF_OUTPUT, --features perf-harness, --ignored --exact and
//! --test-threads=1. No provider, worker, live workspace or external server runs.

use crate::db_import_contact_perf::driver;

use chrono::{DateTime, TimeZone, Utc};
use crm_api::{
    domain::admin::{queries as admin_queries, MembershipStatus, Role},
    ids::{OrganizationId, UserId},
    realtime::Publisher,
    routes::people::perf_cd3b010_person as frozen,
    state::AppState,
};
use driver::{
    AttemptContext, AttemptIds, AttemptOutcome, AttemptPhase, ClientFailure, ClientFailureKind,
    CompletedHttpResponse, HttpBodyResult, QueryArm, TimingCapture, WaveCapture,
};
use reqwest::header::{COOKIE, SET_COOKIE};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use uuid::Uuid;

const WARMUPS: usize = 5;
const SAMPLES: usize = 40;
const PASSWORD: &str = "synthetic-detail-performance-password";
const EMAIL: &str = "detail-perf@example.test";
const SOURCE_MANIFEST: &str = include_str!("fixtures/activity_detail_cd3b010/manifest.json");

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn executable_sha256() -> String {
    let mut file = std::fs::File::open(std::env::current_exe().expect("test executable path"))
        .expect("read the common test executable");
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let length = file.read(&mut buffer).expect("hash common executable");
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// Top-level source functions end at a column-zero closing brace. This deliberately
// rejects a missing/renamed body instead of silently accepting another function.
fn source_function<'a>(source: &'a str, name: &str) -> &'a str {
    let needle = format!("fn {name}");
    let mut offset = 0;
    let start = source
        .split_inclusive('\n')
        .find_map(|line| {
            let at = offset;
            offset += line.len();
            let declaration = line
                .strip_prefix("pub(crate) ")
                .or_else(|| line.strip_prefix("pub(super) "))
                .or_else(|| line.strip_prefix("pub "))
                .unwrap_or(line);
            let declaration = declaration.strip_prefix("async ").unwrap_or(declaration);
            let rest = declaration.strip_prefix(&needle)?;
            (rest.starts_with('(') || rest.starts_with('<')).then_some(at)
        })
        .expect("named top-level function in retained source");
    let end = source[start..]
        .find("\n}\n")
        .expect("complete top-level function")
        + start
        + 3;
    &source[start..end]
}

fn verify_sources() -> Value {
    let manifest: Value = serde_json::from_str(SOURCE_MANIFEST).expect("frozen source manifest");
    let sources = BTreeMap::from([
        (
            "backend/crates/crm-app/src/domain/person/queries.rs",
            include_str!("../../crm-app/src/domain/person/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/domain/task/queries.rs",
            include_str!("../../crm-app/src/domain/task/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/domain/note/queries.rs",
            include_str!("../../crm-app/src/domain/note/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/domain/inquiry/queries.rs",
            include_str!("../../crm-app/src/domain/inquiry/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/domain/tag/queries.rs",
            include_str!("../../crm-app/src/domain/tag/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/domain/custom_field/queries.rs",
            include_str!("../../crm-app/src/domain/custom_field/queries.rs"),
        ),
        (
            "backend/crates/crm-app/src/auth/workspace.rs",
            include_str!("../../crm-app/src/auth/workspace.rs"),
        ),
    ]);
    for helper in manifest["unchanged_helpers"].as_array().unwrap() {
        let source = sources[helper["path"].as_str().unwrap()];
        let body = source_function(source, helper["function"].as_str().unwrap());
        assert_eq!(
            sha256(body.as_bytes()),
            helper["sha256"].as_str().unwrap(),
            "shared helper drift"
        );
    }
    let fragments = BTreeMap::from([
        (
            "history_for_person",
            include_str!("../../crm-app/src/domain/person/perf_cd3b010_history.rs"),
        ),
        (
            "open_for_person",
            include_str!("../../crm-app/src/domain/task/perf_cd3b010_open.rs"),
        ),
        (
            "get_person",
            include_str!("../src/routes/perf_cd3b010_person.rs"),
        ),
    ]);
    for fragment in manifest["frozen"].as_array().unwrap() {
        let source = fragments[fragment["function"].as_str().unwrap()];
        let body = source
            .split_once("// BEGIN FROZEN CD3B010\n")
            .unwrap()
            .1
            .split_once("// END FROZEN CD3B010\n")
            .unwrap()
            .0;
        assert_eq!(body.len() as u64, fragment["byte_length"].as_u64().unwrap());
        assert_eq!(
            sha256(body.as_bytes()),
            fragment["sha256"].as_str().unwrap(),
            "frozen body drift"
        );
    }
    manifest
}

struct Fixture {
    org: Uuid,
    person: Uuid,
    clock: DateTime<Utc>,
    counts: BTreeMap<String, i64>,
}

async fn counts(pool: &PgPool, org: Uuid) -> BTreeMap<String, i64> {
    let mut result = BTreeMap::new();
    // This is a closed fixture-table list, not a caller-supplied SQL identifier.
    for table in [
        "organization_membership",
        "person",
        "contact_method",
        "inquiry",
        "inquiry_received",
        "note",
        "task",
        "tag",
        "person_tag",
        "custom_field",
        "custom_field_option",
        "person_custom_field_value",
        "migration_activity_import",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(org)
        .fetch_one(pool)
        .await
        .expect("fixture cardinality");
        result.insert(table.into(), count);
    }
    result
}

async fn seed(migrator: &PgPool, pool: &PgPool) -> Fixture {
    let clock = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
    let org = crate::common::create_org(migrator, "Operational detail performance").await;
    let viewer = crate::common::create_user(migrator, EMAIL, "Detail viewer", PASSWORD).await;
    let mut conn = pool.acquire().await.expect("fixture app connection");
    admin_queries::insert_membership(
        &mut conn,
        OrganizationId::new(org),
        UserId::new(viewer),
        Role::Member,
        MembershipStatus::Active,
    )
    .await
    .unwrap();
    let mut others = Vec::new();
    for ordinal in 1..50 {
        let user = admin_queries::insert_app_user(
            &mut conn,
            &format!("detail-{ordinal}@example.test"),
            &format!("Detail member {ordinal}"),
        )
        .await
        .unwrap();
        admin_queries::insert_membership(
            &mut conn,
            OrganizationId::new(org),
            user,
            if ordinal == 1 {
                Role::Admin
            } else {
                Role::Member
            },
            MembershipStatus::Active,
        )
        .await
        .unwrap();
        others.push(user.as_uuid());
    }
    drop(conn);
    let other = others[0];
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(pool)
    .await
    .expect("default stage");
    // Synthetic app-role bulk data, matching existing scale-fixture precedent;
    // it does not claim typed-command/import qualification or storage capacity.
    sqlx::query("INSERT INTO person (organization_id,stage_id,assigned_user_id,first_name,last_name,created_at,updated_at) SELECT $1,$2,$3,'Detail',gs::text,$4,$4 FROM generate_series(1,25000) gs")
        .bind(org).bind(stage).bind(viewer).bind(clock).execute(pool).await.unwrap();
    let person: Uuid =
        sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 ORDER BY id LIMIT 1")
            .bind(org)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(r#"INSERT INTO contact_method (organization_id,person_id,kind,value,normalized_value,created_at)
        SELECT p.organization_id,p.id,c.kind,c.value,c.value,$2 + c.ordinal * interval '1 second'
        FROM person p CROSS JOIN LATERAL (VALUES
        ('email','detail-' || p.id::text || '@example.test',1),
        ('phone','+1555' || lpad(p.last_name,7,'0'),2)) c(kind,value,ordinal)
        WHERE p.organization_id=$1"#).bind(org).bind(clock).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,message,received_at,created_at) SELECT $1,id,gen_random_uuid(),'detail_perf','Synthetic inquiry',$2,$2 FROM person WHERE organization_id=$1")
        .bind(org).bind(clock).execute(pool).await.unwrap();
    sqlx::query(r#"INSERT INTO inquiry_received
        (organization_id,actor_kind,origin,occurred_at,recorded_at,correlation_id,inquiry_id,person_id,raw_payload_id,content_hmac,source,person_created)
        SELECT organization_id,'system','migration',$2,$2,gen_random_uuid(),id,person_id,raw_payload_id,decode(repeat('ab',32),'hex'),source,true
        FROM inquiry WHERE organization_id=$1"#).bind(org).bind(clock).execute(pool).await.unwrap();
    // Three notes / four tasks per Person. The selected detail has 20 live
    // notes, 10 open tasks and 10 completed tasks, plus one excluded tombstone
    // of each. Equal times and mixed viewer/other/null actors test exact order
    // and all viewer-relative can_manage branches. Nullable actors are legal
    // native migration-origin fixture rows, with no source binding/child.
    for (only_selected, limit) in [(false, 3_i32), (true, 17_i32)] {
        sqlx::query(r#"INSERT INTO note (organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at)
            SELECT $1,p.id,CASE gs%3 WHEN 0 THEN NULL WHEN 1 THEN $3::uuid ELSE $4::uuid END,
            'Synthetic note ' || gs || ': ' || repeat('A useful relationship detail. ',20) || 'fin.','migration',gen_random_uuid(),$5,$5
            FROM person p CROSS JOIN generate_series(1,$6) gs
            WHERE p.organization_id=$1 AND (NOT $7 OR p.id=$2)"#)
            .bind(org).bind(person).bind(viewer).bind(other).bind(clock).bind(limit).bind(only_selected).execute(pool).await.unwrap();
    }
    for (only_selected, limit) in [(false, 4_i32), (true, 16_i32)] {
        sqlx::query(r#"INSERT INTO task (organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,completed_at,completed_by_user_id,origin,correlation_id,created_at,updated_at)
            SELECT $1,p.id,'Synthetic task ' || gs || ': ' || repeat('Relationship follow-up. ',4) || 'end',
            CASE gs%5 WHEN 0 THEN 'call' WHEN 1 THEN 'email' WHEN 2 THEN 'text' WHEN 3 THEN 'follow_up' ELSE 'other' END,
            CASE WHEN gs%3=0 THEN NULL ELSE $5::timestamptz + interval '1 day' END,
            CASE gs%3 WHEN 0 THEN NULL WHEN 1 THEN $3::uuid ELSE $4::uuid END,
            CASE gs%4 WHEN 0 THEN NULL WHEN 1 THEN $3::uuid ELSE $4::uuid END,
            CASE WHEN gs%2=0 THEN $5::timestamptz ELSE NULL END,
            CASE WHEN gs%2=0 AND gs%3<>0 THEN $4::uuid ELSE NULL END,
            'migration',gen_random_uuid(),$5,$5
            FROM person p CROSS JOIN generate_series(1,$6) gs
            WHERE p.organization_id=$1 AND (NOT $7 OR p.id=$2)"#)
            .bind(org).bind(person).bind(viewer).bind(other).bind(clock).bind(limit).bind(only_selected).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO note (organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at,deleted_at,deleted_by_user_id) VALUES ($1,$2,$3,'','web_session',gen_random_uuid(),$4,$4,$4,$3)")
        .bind(org).bind(person).bind(viewer).bind(clock).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO task (organization_id,person_id,title,assignee_user_id,created_by_user_id,origin,correlation_id,created_at,updated_at,deleted_at,deleted_by_user_id) VALUES ($1,$2,'',$3,$3,'web_session',gen_random_uuid(),$4,$4,$4,$3)")
        .bind(org).bind(person).bind(viewer).bind(clock).execute(pool).await.unwrap();
    for ordinal in 0..3 {
        let tag: Uuid = sqlx::query_scalar("INSERT INTO tag (organization_id,name,created_by_user_id,created_at,updated_at) VALUES ($1,$2,$3,$4,$4) RETURNING id")
            .bind(org).bind(format!("Detail tag {ordinal}")).bind(viewer).bind(clock).fetch_one(pool).await.unwrap();
        sqlx::query("INSERT INTO person_tag (organization_id,person_id,tag_id,added_by_user_id,created_at) SELECT $1,id,$2,$3,$4 FROM person WHERE organization_id=$1")
            .bind(org).bind(tag).bind(viewer).bind(clock).execute(pool).await.unwrap();
    }
    for (position, kind) in ["text", "number", "date", "choice"].into_iter().enumerate() {
        let field: Uuid = sqlx::query_scalar("INSERT INTO custom_field (organization_id,label,field_type,position,created_by_user_id,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$6) RETURNING id")
            .bind(org).bind(format!("Detail {kind}")).bind(kind).bind(position as i32).bind(viewer).bind(clock).fetch_one(pool).await.unwrap();
        let option: Option<Uuid> = if kind == "choice" {
            Some(sqlx::query_scalar("INSERT INTO custom_field_option (organization_id,field_id,label,position,created_at,updated_at) VALUES ($1,$2,'Chosen',0,$3,$3) RETURNING id")
                .bind(org).bind(field).bind(clock).fetch_one(pool).await.unwrap())
        } else {
            None
        };
        sqlx::query(r#"INSERT INTO person_custom_field_value
            (organization_id,person_id,field_id,field_type,text_value,number_value,date_value,option_id,updated_by_user_id,origin,correlation_id,created_at,updated_at)
            SELECT $1,id,$2,$3,CASE WHEN $3='text' THEN repeat('Native value. ',15) || 'end' END,
            CASE WHEN $3='number' THEN 1234567.1250 END,CASE WHEN $3='date' THEN DATE '2026-09-01' END,$4,$5,'web_session',gen_random_uuid(),$6,$6
            FROM person WHERE organization_id=$1"#).bind(org).bind(field).bind(kind).bind(option).bind(viewer).bind(clock).execute(pool).await.unwrap();
    }
    for table in [
        "person",
        "contact_method",
        "inquiry",
        "inquiry_received",
        "note",
        "task",
        "tag",
        "person_tag",
        "custom_field",
        "custom_field_option",
        "person_custom_field_value",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(migrator)
            .await
            .unwrap();
    }
    let counts = counts(pool, org).await;
    for (name, expected) in [
        ("organization_membership", 50),
        ("person", 25_000),
        ("contact_method", 50_000),
        ("inquiry", 25_000),
        ("inquiry_received", 25_000),
        ("note", 75_018),
        ("task", 100_017),
        ("tag", 3),
        ("person_tag", 75_000),
        ("custom_field", 4),
        ("custom_field_option", 1),
        ("person_custom_field_value", 100_000),
        ("migration_activity_import", 0),
    ] {
        assert_eq!(counts[name], expected, "fixture {name}");
    }
    Fixture {
        org,
        person,
        clock,
        counts,
    }
}

struct Server {
    base: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}
impl Server {
    async fn start(app: axum::Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("isolated loopback");
        let address = listener.local_addr().unwrap();
        let (stop, shutdown) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown.await;
                })
                .await
                .expect("serve paired HTTP");
        });
        Self {
            base: format!("http://{address}"),
            stop,
            task,
        }
    }
    async fn close(self) {
        let _ = self.stop.send(());
        self.task.await.expect("stop paired HTTP");
    }
}

#[derive(Clone)]
struct Session {
    client: reqwest::Client,
    cookie: String,
    url: String,
}
async fn login(server: &Server, person: Uuid) -> Session {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let response = client
        .post(format!("{}/api/session", server.base))
        .json(&json!({"email":EMAIL,"password":PASSWORD}))
        .send()
        .await
        .expect("fixture authentication");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .expect("fixture session cookie")
        .to_owned();
    let _: Value = response.json().await.expect("complete session response");
    Session {
        client,
        cookie,
        url: format!("{}/api/people/{person}", server.base),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AttemptMeta {
    body_bytes: usize,
    body_sha256: Option<String>,
    json_equal: bool,
    bytes_equal: bool,
    entry_hits: [usize; 3],
}
fn hits_since(before: [usize; 3]) -> [usize; 3] {
    let after = frozen::entry_hits();
    std::array::from_fn(|i| after[i].saturating_sub(before[i]))
}

async fn request(
    session: Session,
    expected: Arc<Vec<u8>>,
    expected_json: Arc<Value>,
) -> Result<CompletedHttpResponse<AttemptMeta>, ClientFailure<AttemptMeta>> {
    let before = frozen::entry_hits();
    let response = session
        .client
        .get(&session.url)
        .header(COOKIE, &session.cookie)
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return Err(ClientFailure {
                request_completed_at: Instant::now(),
                kind: if error.is_timeout() {
                    ClientFailureKind::RequestTimeout
                } else {
                    ClientFailureKind::Transport
                },
                metadata: AttemptMeta {
                    body_bytes: 0,
                    body_sha256: None,
                    json_equal: false,
                    bytes_equal: false,
                    entry_hits: hits_since(before),
                },
            })
        }
    };
    let status = response.status().as_u16();
    let body = response.bytes().await;
    // Full-body HTTP completion precedes JSON parsing, hashing and comparisons.
    let request_completed_at = Instant::now();
    let body = match body {
        Ok(body) => body,
        Err(_) => {
            return Err(ClientFailure {
                request_completed_at,
                kind: ClientFailureKind::BodyRead,
                metadata: AttemptMeta {
                    body_bytes: 0,
                    body_sha256: None,
                    json_equal: false,
                    bytes_equal: false,
                    entry_hits: hits_since(before),
                },
            })
        }
    };
    let parsed = serde_json::from_slice::<Value>(&body).ok();
    let complete = parsed.as_ref().is_some_and(complete_shape);
    Ok(CompletedHttpResponse {
        request_completed_at,
        status,
        result: if complete {
            HttpBodyResult::Complete
        } else {
            HttpBodyResult::InvalidEnvelope
        },
        whole_source_evaluations: 0,
        metadata: AttemptMeta {
            body_bytes: body.len(),
            body_sha256: Some(sha256(&body)),
            json_equal: parsed.as_ref() == Some(expected_json.as_ref()),
            bytes_equal: body.as_ref() == expected.as_slice(),
            entry_hits: hits_since(before),
        },
    })
}

fn complete_shape(body: &Value) -> bool {
    body.as_object().is_some_and(|o| o.len() == 7)
        && body["person"].is_object()
        && [
            ("contact_methods", 2),
            ("inquiries", 1),
            ("history", 31),
            ("tags", 3),
            ("tasks", 10),
            ("custom_fields", 4),
        ]
        .into_iter()
        .all(|(field, count)| body[field].as_array().is_some_and(|a| a.len() == count))
        && body["history"].as_array().is_some_and(|a| {
            ["note", "task_completed"].into_iter().all(|kind| {
                a.iter()
                    .any(|v| v["kind"] == kind && v["detail"]["can_manage"] == true)
                    && a.iter()
                        .any(|v| v["kind"] == kind && v["detail"]["can_manage"] == false)
            })
        })
        && body["tasks"].as_array().is_some_and(|a| {
            a.iter().any(|v| v["can_manage"] == true) && a.iter().any(|v| v["can_manage"] == false)
        })
}

fn accepted(wave: &WaveCapture<AttemptMeta>) -> bool {
    let expected_hits = if wave.arm == QueryArm::FrozenOriginal {
        [1; 3]
    } else {
        [0; 3]
    };
    wave.concurrency == 1 && wave.attempts.len()==1 && wave.attempts.iter().all(|a| {
        matches!(&a.outcome, AttemptOutcome::Response(r) if r.status==200 && r.result==HttpBodyResult::Complete && r.metadata.json_equal && r.metadata.bytes_equal && r.metadata.entry_hits==expected_hits)
        && matches!(a.timing,TimingCapture::Exact(_))
    })
}

#[sqlx::test]
#[ignore = "isolated paired HTTP performance slot, absolute CRM_010F2_PERSON_PERF_OUTPUT"]
async fn operational_person_detail_matches_cd3b010(migrator: PgPool) {
    let output = PathBuf::from(
        std::env::var_os("CRM_010F2_PERSON_PERF_OUTPUT").expect("explicit evidence output"),
    );
    assert!(output.is_absolute(), "absolute evidence output");
    std::fs::create_dir_all(&output).expect("evidence directory");
    let source_manifest = verify_sources();
    let binary_sha256 = executable_sha256();
    let pool = crate::common::connect_as_app(&migrator).await;
    let fixture = seed(&migrator, &pool).await;
    let config = crate::common::test_config();
    let state = || AppState::for_tests(pool.clone(), &config, Publisher::recording());
    let baseline_server = Server::start(crm_api::build_app_with_people_router(
        state(),
        frozen::router(),
    ))
    .await;
    let current_server = Server::start(crm_api::build_app(state())).await;
    let baseline_session = login(&baseline_server, fixture.person).await;
    let current_session = login(&current_server, fixture.person).await;
    let before = frozen::entry_hits();
    let baseline_response = baseline_session
        .client
        .get(&baseline_session.url)
        .header(COOKIE, &baseline_session.cookie)
        .send()
        .await;
    let (reference_status, reference, reference_failure) = match baseline_response {
        Ok(response) => {
            let status = response.status().as_u16();
            match response.bytes().await {
                Ok(body) => (Some(status), body.to_vec(), None),
                Err(_) => (Some(status), Vec::new(), Some("body_read")),
            }
        }
        Err(error) => (
            None,
            Vec::new(),
            Some(if error.is_timeout() {
                "request_timeout"
            } else {
                "transport"
            }),
        ),
    };
    let reference = Arc::new(reference);
    let reference_json =
        Arc::new(serde_json::from_slice::<Value>(&reference).unwrap_or(Value::Null));
    let reference_hits = hits_since(before);
    let preflight = json!({"status":reference_status,"failure":reference_failure,"body_bytes":reference.len(),"body_sha256":sha256(&reference),"complete_shape":complete_shape(&reference_json),"entry_hits":reference_hits});
    let baseline_reference_valid = reference_failure.is_none()
        && reference_status == Some(200)
        && complete_shape(&reference_json)
        && reference_hits == [1; 3];
    let sessions = [baseline_session, current_session];
    let dispatch = Arc::new(move |context: AttemptContext| {
        let session = sessions[usize::from(context.arm == QueryArm::Final)].clone();
        request(session, reference.clone(), reference_json.clone())
    });
    let mut ids = AttemptIds::default();
    let mut waves = Vec::new();
    // Alternate AB/BA within every serial pair; no arm overlaps the other.
    for (phase, pairs) in [
        (AttemptPhase::Warmup, WARMUPS),
        (AttemptPhase::Measured, SAMPLES),
    ] {
        for pair in 0..pairs {
            let arms = if pair % 2 == 0 {
                [QueryArm::FrozenOriginal, QueryArm::Final]
            } else {
                [QueryArm::Final, QueryArm::FrozenOriginal]
            };
            for arm in arms {
                waves.extend(
                    driver::run_serial(&mut ids, arm, phase, waves.len(), 1, dispatch.clone())
                        .await
                        .expect("bounded serial schedule"),
                );
            }
        }
    }
    let after_counts = counts(&pool, fixture.org).await;
    let workspace: String =
        sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
            .bind(fixture.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    let measurements = |arm| {
        driver::summarize_timings(
            waves
                .iter()
                .filter(|w| w.arm == arm && w.phase == AttemptPhase::Measured)
                .flat_map(|w| w.attempts.iter().map(|a| a.timing)),
        )
    };
    let baseline = measurements(QueryArm::FrozenOriginal);
    let current = measurements(QueryArm::Final);
    let budget = baseline
        .p95
        .map(|old| old + old.mul_f64(0.10).max(Duration::from_millis(25)));
    let latency_passed = current
        .p95
        .zip(budget)
        .is_some_and(|(current, budget)| current <= budget);
    let normal_passed = baseline_reference_valid
        && waves.len() == 2 * (WARMUPS + SAMPLES)
        && waves.iter().all(accepted);
    let counts_unchanged = fixture.counts == after_counts;
    let evidence = json!({
        "protocol":"slice-010f2-one-paired-operational-person-detail-v1",
        "baseline_commit":source_manifest["baseline_commit"],"source_manifest":source_manifest,
        "source_manifest_sha256":sha256(SOURCE_MANIFEST.as_bytes()),"common_executable_sha256":binary_sha256,
        "clock":{"fixture_utc":fixture.clock,"response_clock":"All returned timestamps are fixed fixture values; this endpoint computes no wall-clock-relative fields. Both arms share the ordinary session clock."},
        "fixture":{"kind":"synthetic moderate native activity book; not import qualification or maximum capacity","before_counts":fixture.counts,"after_counts":after_counts,"counts_unchanged":counts_unchanged,"workspace_mode":workspace,"selected_live_notes":20,"selected_open_tasks":10,"selected_completed_tasks":10,"selected_core_history":1,"selected_note_tombstones":1,"selected_task_tombstones":1},
        "request_protocol":{"concurrency":1,"warmups_per_arm":WARMUPS,"measured_per_arm":SAMPLES,"pair_order":"alternating AB/BA","timing":"dispatch through complete HTTP body, before parse/hash/compare","comparison":"complete JSON and byte equality","percentile":"nearest rank ceil(0.95*n)","budget":"baseline p95 + max(25ms, 10% of baseline p95)"},
        "reference":preflight,"raw_waves":waves,
        "comparison":{"baseline":baseline,"current":current,"current_p95_limit":budget,"latency_passed":latency_passed,"all_responses_equal_and_entrypoints_verified":normal_passed}
    });
    // Retain every warmup, measured response and client/join failure before
    // any acceptance assertion. Only counts, hashes and safe timing metadata.
    std::fs::write(
        output.join("person-detail-paired.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .expect("write paired evidence");
    baseline_server.close().await;
    current_server.close().await;
    pool.close().await;
    assert!(
        normal_passed,
        "response equality/entrypoint/completion gate; see retained evidence"
    );
    assert!(
        counts_unchanged && workspace == "operational",
        "fixture must remain operational and unchanged"
    );
    assert_eq!(baseline.exact_observations, SAMPLES);
    assert_eq!(current.exact_observations, SAMPLES);
    assert!(
        latency_passed,
        "paired p95 regression budget; see retained evidence"
    );
}

use crm_app::{
    config::RawPayloadKey,
    domain::{
        envelope::{CommandContext, Origin},
        migration::{
            commands,
            reader::{Capture, FubReader, Identity, Probe, ProbeResult, ReaderError},
            snapshot::{self, SnapshotPolicy, SnapshotRequest, SourceAction},
            snapshot_source::{Request, Stream},
            snapshot_worker,
        },
    },
    ids::{CorrelationId, OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, PgPool, Row};
use std::{path::Path, str::FromStr};
use uuid::Uuid;
struct Book;
#[async_trait::async_trait]
impl FubReader for Book {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        Ok((
            Identity {
                account_id: 17,
                user_id: Some(3),
                account_domain: None,
                display_name: None,
            },
            br#"{"account":{"id":17},"user":{"id":3}}"#.to_vec(),
        ))
    }
    async fn probe(&self, _: &str, _: Probe) -> Result<ProbeResult, ReaderError> {
        Err(ReaderError::Unavailable)
    }
    async fn snapshot(&self, _: &str, r: &Request) -> Result<Capture, ReaderError> {
        let collection = match r.stream {
            Stream::Users => "users",
            Stream::Stages => "stages",
            Stream::CustomFields => "customfields",
            Stream::People => "people",
            Stream::Notes => "notes",
            Stream::TasksOpen | Stream::TasksCompleted => "tasks",
            Stream::NoteDetail => panic!("no synthetic notes"),
        };
        let records = match r.stream {
            Stream::Users => json!([{"id":3,"name":"Synthetic handoff user"}]),
            Stream::Stages => json!([{"id":4,"name":"Lead"}]),
            Stream::People => json!([{"id":101,"firstName":"Synthetic handoff person"}]),
            _ => json!([]),
        };
        let value = json!({"_metadata":{"collection":collection,"limit":100,"offset":0,"total":records.as_array().unwrap().len()},collection:records});
        Ok(Capture {
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
            truncated: false,
            source_version: Some("synthetic-handoff-v1".into()),
        })
    }
}
fn options(name: &str, db: &str) -> PgConnectOptions {
    let raw = std::env::var(name).expect("private environment value missing");
    let option = PgConnectOptions::from_str(&raw).expect("invalid private database settings");
    assert!(matches!(
        option.get_host(),
        "localhost" | "127.0.0.1" | "::1"
    ));
    option.database(db)
}
async fn evidence(pool: &PgPool, label: &str) -> Value {
    let r = sqlx::query("SELECT state,pause_reason,capture_sequence FROM migration_snapshot")
        .fetch_one(pool)
        .await
        .unwrap();
    let caps = sqlx::query(
        "SELECT stream,checkpoint,accepted FROM migration_snapshot_capture ORDER BY sequence",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let distinct:i64=sqlx::query_scalar("SELECT count(DISTINCT request_fingerprint) FROM migration_snapshot_capture WHERE stream='identity' AND accepted").fetch_one(pool).await.unwrap();
    let checkpoint: i64 = sqlx::query_scalar(
        "SELECT checkpoint FROM migration_snapshot_stream WHERE stream='identity'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    json!({"label":label,"process_id":std::process::id(),"state":r.get::<String,_>("state"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"sequence":r.get::<i64,_>("capture_sequence"),"identity_next_checkpoint":checkpoint,"distinct_accepted_identity_fingerprints":distinct,"captures":caps.iter().map(|c|json!({"stream":c.get::<String,_>("stream"),"checkpoint":c.get::<i64,_>("checkpoint"),"accepted":c.get::<bool,_>("accepted")})).collect::<Vec<_>>()})
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let env_path = args.get(1).expect("explicit fixture env path required");
    let repo = args.get(2).expect("repository root required");
    for entry in
        dotenvy::from_path_iter(env_path).expect("load explicit private fixture environment")
    {
        let (key, value) = entry.expect("valid environment entry");
        if matches!(key.as_str(), "DATABASE_URL" | "MIGRATION_DATABASE_URL") {
            std::env::set_var(key, value);
        }
    }
    if args.get(3).is_some_and(|v| v == "worker") {
        let db = &args[4];
        assert!(db.starts_with("crm_handoff_repro_"));
        let pool = PgPool::connect_with(options("DATABASE_URL", db))
            .await
            .expect("connect isolated app database");
        let iterations: usize = args[5].parse().unwrap();
        for _ in 0..iterations {
            match snapshot_worker::run_once(
                &pool,
                &RawPayloadKey::new([0x11; 32]),
                &Book,
                &SnapshotPolicy::default(),
            )
            .await
            {
                Ok(false) => break,
                Ok(true) => {}
                Err(e) => panic!("collector outcome: {e}"),
            }
        }
        println!("{}", evidence(&pool, &args[6]).await);
        pool.close().await;
        return;
    }
    let db = format!("crm_handoff_repro_{}", std::process::id());
    let base =
        PgConnectOptions::from_str(&std::env::var("MIGRATION_DATABASE_URL").unwrap()).unwrap();
    assert_eq!(base.get_database(), Some("crm_migration_010e1"));
    assert!(matches!(base.get_host(), "localhost" | "127.0.0.1" | "::1"));
    let owner = PgPool::connect_with(base)
        .await
        .expect("connect isolated migration base");
    sqlx::query(&format!("CREATE DATABASE {db}"))
        .execute(&owner)
        .await
        .expect("create uniquely owned reproduction database");
    let migrator = PgPool::connect_with(options("MIGRATION_DATABASE_URL", &db))
        .await
        .unwrap();
    let migration = sqlx::migrate::Migrator::new(
        Path::new(repo)
            .join("backend/crates/crm-api/migrations")
            .as_path(),
    )
    .await
    .unwrap();
    migration
        .run(&migrator)
        .await
        .expect("apply current migrations");
    let org:Uuid=sqlx::query_scalar("INSERT INTO organization(name,intake_slug,intake_token) VALUES('SYNTHETIC handoff reproduction','synthetic-handoff','syntha11') RETURNING id").fetch_one(&migrator).await.unwrap();
    let user:Uuid=sqlx::query_scalar("INSERT INTO app_user(email,display_name) VALUES('handoff@synthetic.invalid','SYNTHETIC handoff admin') RETURNING id").fetch_one(&migrator).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) VALUES($1,$2,'admin','active')").bind(org).bind(user).execute(&migrator).await.unwrap();
    let pool = PgPool::connect_with(options("DATABASE_URL", &db))
        .await
        .unwrap();
    let ctx = CommandContext {
        organization_id: OrganizationId::new(org),
        actor_user_id: UserId::new(user),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let key = RawPayloadKey::new([0x11; 32]);
    let policy = SnapshotPolicy::default();
    let c = commands::connect_fub(
        &pool,
        &key,
        &Book,
        &ctx,
        commands::ConnectFub {
            request_id: Uuid::new_v4(),
            api_key: "synthetic-reproduction-only".into(),
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
            connection_id: c.id,
            expected_revision: c.revision,
        },
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(proposed["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &pool,
        &key,
        &policy,
        &ctx,
        id,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    let mut results = Vec::new();
    for (count, label) in [
        (1, "process_A_one_claim"),
        (1, "process_B_one_claim"),
        (1, "process_C_one_claim"),
        (2, "process_D_two_claims"),
        (20, "process_E_drain"),
    ] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([env_path, repo, "worker", &db, &count.to_string(), label])
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "worker failed: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let value: Value = serde_json::from_slice(&child.stdout).unwrap();
        println!("{value}");
        results.push(value);
    }
    assert_eq!(results[2]["identity_next_checkpoint"], 3);
    assert_eq!(results[2]["distinct_accepted_identity_fingerprints"], 3);
    assert_eq!(results[2]["sequence"], 3);
    assert_eq!(results[4]["state"], "completed");
    pool.close().await;
    migrator.close().await;
    sqlx::query(&format!("DROP DATABASE {db}"))
        .execute(&owner)
        .await
        .expect("drop only this helper's database");
    println!(
        "{}",
        json!({"isolated_database_cleaned":true,"separate_worker_processes":5,"assertions_passed":true})
    );
}

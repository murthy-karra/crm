//! Bounded retained-only metadata preparation and execution. No FUB reader exists here.
use super::{
    crypto,
    metadata::{self, Choice, Patch},
    metadata_model::{self as m, Counts, Manifest, Mapping, Operation, ResultData, Target},
    metadata_source::{self as source, Entity, Record},
    metadata_store as s,
    snapshot::SnapshotPolicy,
    snapshot_source::Stream,
    MigrationError,
};
use crate::{
    auth::workspace,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone)]
struct Job {
    id: Uuid,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    token: Uuid,
    actor: UserId,
    boundary: i64,
    account: i64,
    parent: Uuid,
    parent_plan: Uuid,
}
impl Job {
    fn ctx(&self) -> CommandContext {
        CommandContext {
            organization_id: self.org,
            actor_user_id: self.actor,
            origin: Origin::Migration,
            correlation_id: CorrelationId::new(self.id),
        }
    }
}
pub fn spawn(
    pool: PgPool,
    key: RawPayloadKey,
    policy: SnapshotPolicy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            for _ in 0..32 {
                match run_once(&pool, &key, &policy).await {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(e) => {
                        tracing::warn!(outcome=%e,"Metadata import unit failed");
                        break;
                    }
                }
            }
        }
    })
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
) -> Result<bool, MigrationError> {
    s::with_policy(policy, run_once_inner(pool, key)).await
}
async fn worker_tx(
    pool: &PgPool,
    org: OrganizationId,
    actor: UserId,
) -> Result<(Transaction<'_, Postgres>, bool), MigrationError> {
    let mut tx = tokio::time::timeout(workspace::WAIT, pool.begin())
        .await
        .map_err(|_| MigrationError::Database(sqlx::Error::PoolTimedOut))??;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, org).await?;
    let member=sqlx::query("SELECT role,status FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR SHARE").bind(org.0).bind(actor.0).fetch_optional(&mut *tx).await?;
    let active = member.is_some_and(|m| {
        m.get::<String, _>("role") == "admin" && m.get::<String, _>("status") == "active"
    });
    super::store::lock_org(&mut tx, org).await?;
    Ok((tx, active))
}
#[tracing::instrument(name = "migration.metadata_import.unit", skip_all)]
async fn run_once_inner(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    let c=sqlx::query("SELECT i.id,i.organization_id,i.executor_user_id FROM migration_metadata_import i JOIN migration_metadata_plan p ON p.id=i.latest_plan_id AND p.organization_id=i.organization_id WHERE i.state IN ('proposed','queued','running') AND ((i.state='proposed' AND p.state='building') OR i.state IN ('queued','running')) AND (i.lease_expires_at IS NULL OR i.lease_expires_at<=now()) ORDER BY i.created_at,i.id LIMIT 1").fetch_optional(pool).await?;
    let Some(c) = c else { return Ok(false) };
    let org = OrganizationId::new(c.get("organization_id"));
    let id: Uuid = c.get("id");
    let actor = UserId::new(c.get("executor_user_id"));
    let (mut tx, active) = worker_tx(pool, org, actor).await?;
    let r = s::run(&mut tx, org, id).await?;
    if r.get::<Uuid, _>("executor_user_id") != actor.0
        || r.get::<Option<chrono::DateTime<Utc>>, _>("lease_expires_at")
            .is_some_and(|v| v > Utc::now())
        || !matches!(
            r.get::<String, _>("state").as_str(),
            "proposed" | "queued" | "running"
        )
    {
        return Ok(false);
    }
    let j = Job {
        id,
        org,
        snapshot: r.get("snapshot_id"),
        plan: r.get("latest_plan_id"),
        token: Uuid::new_v4(),
        actor,
        boundary: r.get("capture_sequence"),
        account: r.get("source_account_id"),
        parent: r.get("parent_import_id"),
        parent_plan: r.get("parent_plan_id"),
    };
    s::release(&mut tx, org, id, "work", 0).await?;
    if !active {
        pause(&mut tx, &j, "authority_changed").await?;
        tx.commit().await?;
        return Ok(true);
    }
    if metadata::validate_binding(&mut tx, &r).await.is_err() {
        pause(&mut tx, &j, "source_evidence_unavailable").await?;
        tx.commit().await?;
        return Ok(true);
    }
    sqlx::query("UPDATE migration_metadata_import SET lease_token=$3,lease_expires_at=now()+interval '60 seconds',updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut tx = s::begin(pool, &j.ctx(), false).await?;
    let r = s::run(&mut tx, org, id).await?;
    if r.get::<Option<Uuid>, _>("lease_token") != Some(j.token)
        || r.get::<Uuid, _>("latest_plan_id") != j.plan
    {
        return Ok(true);
    }
    let p = s::plan(&mut tx, org, id, j.plan).await?;
    let result = if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some() {
        execute(&mut tx, key, &j, &r, &p).await
    } else {
        prepare(&mut tx, key, &j, &p).await
    };
    match result {
        Ok(()) => {
            let count=sqlx::query("UPDATE migration_metadata_import SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_expires_at>clock_timestamp()").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?.rows_affected();
            if count != 1 {
                return Err(MigrationError::ImportBusy);
            }
            tx.commit().await?;
        }
        Err(error) => {
            if let MigrationError::Database(ref e) = error {
                tracing::warn!(
                    sqlstate = e.as_database_error().and_then(|e| e.code()).as_deref(),
                    "Metadata database unit failed"
                );
            }
            tx.rollback().await?;
            let (mut tx, _) = worker_tx(pool, org, actor).await?;
            let r = s::run(&mut tx, org, id).await?;
            if r.get::<Option<Uuid>, _>("lease_token") == Some(j.token) {
                s::release(&mut tx, org, id, "work", 0).await?;
                let reason = match error {
                    MigrationError::StorageLimit => "storage_limit",
                    MigrationError::Crypto => "retained_key_unavailable",
                    MigrationError::SourceNotEligible => "source_integrity",
                    MigrationError::InvalidImportChoice => "mapping_target_changed",
                    MigrationError::Database(_) => "storage_unavailable",
                    _ => "target_unavailable",
                };
                pause(&mut tx, &j, reason).await?;
                tx.commit().await?;
            }
        }
    }
    Ok(true)
}
async fn pause(conn: &mut PgConnection, j: &Job, reason: &str) -> Result<(), MigrationError> {
    tracing::info!(organization_id=%j.org,import_id=%j.id,plan_id=%j.plan,pause_reason=reason,"Metadata import paused");
    sqlx::query("UPDATE migration_metadata_import SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(reason).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_metadata_plan SET state=CASE WHEN state='building' THEN 'paused' ELSE state END,pause_reason=CASE WHEN state='building' THEN $3 ELSE pause_reason END WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(reason).execute(conn).await?;
    Ok(())
}
async fn admission(conn: &mut PgConnection, j: &Job, amount: i64) -> Result<Uuid, MigrationError> {
    let t = Uuid::new_v4();
    if !s::reserve(
        conn, j.org, j.id, j.snapshot, j.plan, j.token, t, amount, "work",
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(t)
}
async fn phase(conn: &mut PgConnection, j: &Job, next: &str) -> Result<(), MigrationError> {
    let previous: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT checkpoint_key FROM migration_metadata_plan WHERE id=$1 AND organization_id=$2",
    )
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE migration_metadata_plan SET phase=$3,checkpoint_kind='',checkpoint_id=NULL,checkpoint_key=NULL,checkpoint_element=0 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(next).execute(&mut *conn).await?;
    s::charge(
        conn,
        j.org,
        j.id,
        j.snapshot,
        j.plan,
        -(previous.map_or(0, |v| v.len()) as i64),
    )
    .await
}
async fn save_counts(
    conn: &mut PgConnection,
    j: &Job,
    counts: &Counts,
    execution: bool,
) -> Result<(), MigrationError> {
    let sql = if execution {
        "UPDATE migration_metadata_import SET counts=$3 WHERE id=$1 AND organization_id=$2"
    } else {
        "UPDATE migration_metadata_plan SET counts=$3 WHERE id=$1 AND organization_id=$2"
    };
    sqlx::query(sql)
        .bind(if execution { j.id } else { j.plan })
        .bind(j.org.0)
        .bind(serde_json::to_value(counts).map_err(|_| MigrationError::Crypto)?)
        .execute(conn)
        .await?;
    Ok(())
}
async fn issues(conn: &mut PgConnection, j: &Job, codes: &[String]) -> Result<(), MigrationError> {
    for code in codes.iter().collect::<BTreeSet<_>>() {
        sqlx::query("INSERT INTO migration_metadata_issue(plan_id,import_id,organization_id,code,record_count) VALUES($1,$2,$3,$4,1) ON CONFLICT(plan_id,organization_id,code) DO UPDATE SET record_count=migration_metadata_issue.record_count+1").bind(j.plan).bind(j.id).bind(j.org.0).bind(code).execute(&mut *conn).await?;
    }
    Ok(())
}
async fn prepare(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if p.get::<String, _>("state") != "building" {
        return Ok(());
    }
    match p.get::<String, _>("phase").as_str() {
        "copying_choices" => copy_choices(conn, key, j, p).await,
        "captures" => capture(conn, key, j, p).await,
        "catalog" => catalog(conn, key, j, p).await,
        "mappings" => mappings(conn, key, j, p).await,
        "people" => people(conn, key, j, p).await,
        "ready" => Ok(()),
        _ => Err(MigrationError::SourceNotEligible),
    }
}
async fn copy_choices(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let token = admission(conn, j, s::UNIT).await?;
    let old: Option<Uuid> = p.get("inherit_plan_id");
    let mut added = 0;
    let rows = if let Some(old) = old {
        sqlx::query("SELECT id,kind,source_key,nonce,ciphertext FROM migration_metadata_choice WHERE plan_id=$1 AND organization_id=$2 AND (kind,source_key)>($3,$4) ORDER BY kind,source_key LIMIT 50").bind(old).bind(j.org.0).bind(p.get::<String,_>("checkpoint_kind")).bind(p.get::<Option<Vec<u8>>,_>("checkpoint_key").unwrap_or_default()).fetch_all(&mut *conn).await?
    } else {
        vec![]
    };
    if rows.is_empty() {
        let predecessor = if let Some(old) = old {
            // An interrupted ancestor may not have copied its older choices.
            // Its own patch is already materialized. Visit one ancestor per
            // unit, newest first; never load the complete lineage at once.
            let ancestor = s::plan(conn, j.org, j.id, old).await?;
            if ancestor.get::<String, _>("phase") == "copying_choices" {
                ancestor.get::<Option<Uuid>, _>("parent_plan_id")
            } else {
                None
            }
        } else {
            None
        };
        if let Some(predecessor) = predecessor {
            added -= p
                .get::<Option<Vec<u8>>, _>("checkpoint_key")
                .map_or(0, |v| v.len() as i64);
            sqlx::query("UPDATE migration_metadata_plan SET inherit_plan_id=$3,checkpoint_kind='',checkpoint_key=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(predecessor).execute(&mut *conn).await?;
        } else {
            phase(conn, j, "captures").await?;
        }
    } else {
        for r in &rows {
            let kind: String = r.get("kind");
            let source_key: Vec<u8> = r.get("source_key");
            let exists = sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_metadata_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4)").bind(j.plan).bind(j.org.0).bind(&kind).bind(&source_key).fetch_one(&mut *conn).await?;
            if exists {
                continue;
            }
            let choice = s::open(
                key,
                j.org,
                j.snapshot,
                old.unwrap(),
                r.get("id"),
                "choice",
                &r.get::<Vec<u8>, _>("nonce"),
                &r.get::<Vec<u8>, _>("ciphertext"),
            )?;
            added += metadata::insert_choice(
                conn,
                key,
                j.org,
                j.id,
                j.snapshot,
                j.plan,
                &Patch {
                    kind,
                    source_key,
                    choice,
                },
            )
            .await?;
        }
        let last = rows.last().unwrap();
        added += 32
            - p.get::<Option<Vec<u8>>, _>("checkpoint_key")
                .map_or(0, |v| v.len() as i64);
        sqlx::query("UPDATE migration_metadata_plan SET checkpoint_kind=$3,checkpoint_key=$4 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(last.get::<String,_>("kind")).bind(last.get::<Vec<u8>,_>("source_key")).execute(&mut *conn).await?;
    }
    s::settle(conn, j.org, j.id, token, added).await
}
fn source_record(
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
) -> Result<Record, MigrationError> {
    s::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        r.get("id"),
        "source",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
}
async fn capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let next=sqlx::query("SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 AND stream IN ('people','custom_fields') ORDER BY sequence LIMIT 1").bind(j.snapshot).bind(j.org.0).bind(p.get::<i64,_>("checkpoint_capture")).bind(j.boundary).fetch_optional(&mut *conn).await?;
    let Some(next) = next else {
        return phase(conn, j, "catalog").await;
    };
    let token = admission(conn, j, s::UNIT).await?;
    let c=sqlx::query("SELECT * FROM migration_snapshot_capture WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(next.get::<Uuid,_>("id")).bind(j.snapshot).bind(j.org.0).fetch_one(&mut *conn).await?;
    let capture: Uuid = c.get("id");
    let family: String = c.get("stream");
    let stream = Stream::parse(&family).ok_or(MigrationError::SourceNotEligible)?;
    let raw = crypto::open_snapshot(
        key,
        j.org,
        j.snapshot,
        capture,
        "capture",
        &c.get::<Vec<u8>, _>("nonce"),
        &c.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let parsed = source::extract_page(stream, &raw);
    let records=sqlx::query("SELECT id,source_id,ordinal,family,representation,semantic_hmac FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101").bind(capture).bind(j.snapshot).bind(j.org.0).fetch_all(&mut *conn).await?;
    if records.len() > 100 {
        return Err(MigrationError::SourceNotEligible);
    }
    let accepted = c.get::<bool, _>("accepted")
        && !c.get::<bool, _>("truncated")
        && (200..300).contains(&c.get::<i32, _>("http_status"))
        && c.get::<String, _>("classification") == "success"
        && c.get::<String, _>("representation") == stream.representation()
        && c.get::<i64, _>("raw_byte_len") == raw.len() as i64;
    let mut added = 0;
    let mut counts = Counts::load(p.get("counts"))?;
    for r in &records {
        let Some(source_id) = r.get::<Option<String>, _>("source_id") else {
            counts.invalid_source_ids += 1;
            continue;
        };
        let ordinal: i32 = r.get("ordinal");
        let mut item = usize::try_from(ordinal)
            .ok()
            .and_then(|i| parsed.as_ref().ok().and_then(|p| p.get(i)))
            .cloned()
            .unwrap_or(Record {
                source_id: Some(source_id.clone()),
                canonical: vec![],
                entity: Entity::Invalid,
                reasons: vec!["source_integrity".into()],
                transformations: vec![],
                provenance: Default::default(),
            });
        let semantic = crypto::snapshot_hmac(
            key,
            j.org,
            &format!("semantic:{}", stream.representation()),
            &item.canonical,
        );
        let integrity = item.source_id.as_deref() == Some(source_id.as_str())
            && r.get::<String, _>("family") == family
            && r.get::<String, _>("representation") == stream.representation()
            && r.get::<Vec<u8>, _>("semantic_hmac") == semantic;
        let qualified = accepted && integrity;
        item.canonical.clear();
        let old=sqlx::query("SELECT id,qualified,conflict,semantic_hmac,nonce,ciphertext FROM migration_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4 FOR UPDATE").bind(j.plan).bind(j.org.0).bind(&family).bind(&source_id).fetch_optional(&mut *conn).await?;
        let id = old.as_ref().map_or_else(Uuid::new_v4, |v| v.get("id"));
        let conflict = !integrity
            || old
                .as_ref()
                .is_some_and(|v| v.get::<Vec<u8>, _>("semantic_hmac") != semantic);
        if old
            .as_ref()
            .is_some_and(|v| v.get::<bool, _>("qualified") || !qualified)
        {
            sqlx::query("UPDATE migration_metadata_source SET conflict=conflict OR $3,observations=observations+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(j.org.0).bind(conflict).execute(&mut *conn).await?;
            continue;
        }
        let a = s::seal(key, j.org, j.snapshot, j.plan, id, "source", &item)?;
        let size = s::sealed_bytes(&a)
            + source_id.len() as i64
            + stream.representation().len() as i64
            + 32;
        if let Some(old) = old {
            added += size
                - (old.get::<Vec<u8>, _>("nonce").len()
                    + old.get::<Vec<u8>, _>("ciphertext").len()
                    + source_id.len()
                    + stream.representation().len()
                    + 32) as i64;
            sqlx::query("UPDATE migration_metadata_source SET record_id=$3,capture_id=$4,capture_sequence=$5,ordinal=$6,representation=$7,semantic_hmac=$8,qualified=$9,conflict=conflict OR $10,observations=observations+1,nonce=$11,ciphertext=$12 WHERE id=$1 AND organization_id=$2").bind(id).bind(j.org.0).bind(r.get::<Uuid,_>("id")).bind(capture).bind(c.get::<i64,_>("sequence")).bind(ordinal).bind(stream.representation()).bind(semantic.as_slice()).bind(qualified).bind(conflict).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(&mut *conn).await?;
        } else {
            added += size;
            sqlx::query("INSERT INTO migration_metadata_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,representation,semantic_hmac,qualified,conflict,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)").bind(id).bind(j.plan).bind(j.id).bind(j.snapshot).bind(j.org.0).bind(&family).bind(&source_id).bind(r.get::<Uuid,_>("id")).bind(capture).bind(c.get::<i64,_>("sequence")).bind(ordinal).bind(stream.representation()).bind(semantic.as_slice()).bind(qualified).bind(conflict).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(&mut *conn).await?;
        }
    }
    if accepted && parsed.as_ref().is_ok_and(|v| v.len() != records.len()) {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("UPDATE migration_metadata_plan SET checkpoint_capture=$3 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(c.get::<i64,_>("sequence")).execute(&mut *conn).await?;
    save_counts(conn, j, &counts, false).await?;
    s::settle(conn, j.org, j.id, token, added).await
}

async fn next_source(
    conn: &mut PgConnection,
    j: &Job,
    family: &str,
    last: Option<Uuid>,
) -> Result<Option<sqlx::postgres::PgRow>, MigrationError> {
    let (sequence, ordinal) = if let Some(last) = last {
        let r=sqlx::query("SELECT capture_sequence,ordinal FROM migration_metadata_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(last).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
        (
            r.get::<i64, _>("capture_sequence"),
            r.get::<i32, _>("ordinal"),
        )
    } else {
        (0, -1)
    };
    Ok(sqlx::query("SELECT * FROM migration_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND (capture_sequence,ordinal,id)>($4,$5,$6) ORDER BY capture_sequence,ordinal,id LIMIT 1").bind(j.plan).bind(j.org.0).bind(family).bind(sequence).bind(ordinal).bind(last.unwrap_or(Uuid::nil())).fetch_optional(conn).await?)
}
fn decoded_mapping(
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
) -> Result<Mapping, MigrationError> {
    s::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        r.get("id"),
        "mapping",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
async fn insert_mapping(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    source: &sqlx::postgres::PgRow,
    id: Uuid,
    kind: &str,
    source_key: &[u8],
    name_hmac: Option<&[u8]>,
    parent: Option<Uuid>,
    ordinal: i32,
    qualified: bool,
    value: &Mapping,
) -> Result<i64, MigrationError> {
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "mapping", value)?;
    let size = s::sealed_bytes(&a) + 32 + name_hmac.map_or(0, |v| v.len() as i64);
    sqlx::query("INSERT INTO migration_metadata_mapping(id,plan_id,import_id,organization_id,kind,source_key,source_row_id,parent_mapping_id,source_sequence,source_ordinal,element_ordinal,name_hmac,qualified,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(kind).bind(source_key).bind(source.get::<Uuid,_>("id")).bind(parent).bind(source.get::<i64,_>("capture_sequence")).bind(source.get::<i32,_>("ordinal")).bind(ordinal).bind(name_hmac).bind(qualified).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(conn).await?;
    Ok(size)
}
async fn catalog(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let family = if p.get::<String, _>("checkpoint_kind") == "people" {
        "people"
    } else {
        "custom_fields"
    };
    let r = next_source(conn, j, family, p.get("checkpoint_id")).await?;
    let Some(r) = r else {
        if family == "custom_fields" {
            sqlx::query("UPDATE migration_metadata_plan SET checkpoint_kind='people',checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).execute(conn).await?;
            return Ok(());
        }
        return phase(conn, j, "mappings").await;
    };
    let token = admission(conn, j, s::UNIT).await?;
    let record = source_record(key, j, &r)?;
    let qualified =
        r.get::<bool, _>("qualified") && !r.get::<bool, _>("conflict") && record.reasons.is_empty();
    let mut added = 0;
    let element = p.get::<i32, _>("checkpoint_element") as usize;
    let mut next_element = 0usize;
    let mut more = false;
    match &record.entity {
        Entity::Field(field) => {
            let source_id = record
                .source_id
                .as_deref()
                .ok_or(MigrationError::SourceNotEligible)?;
            let source_key = s::source_key(key, j.org, j.account, "field", source_id.as_bytes());
            let name = field
                .name
                .as_ref()
                .map(|n| s::source_key(key, j.org, j.account, "field-name", n.as_bytes()));
            let id = if element == 0 {
                Uuid::new_v4()
            } else {
                sqlx::query_scalar("SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND source_row_id=$3").bind(j.plan).bind(j.org.0).bind(r.get::<Uuid,_>("id")).fetch_one(&mut *conn).await?
            };
            let mut reasons = record.reasons.clone();
            if !qualified {
                m::mark(&mut reasons, "source_integrity")
            }
            for reason in &field.reasons {
                m::mark(&mut reasons, reason)
            }
            let data = Mapping {
                source: record.provenance.clone(),
                label: field.label.clone(),
                group: None,
                source_name: field.name.clone(),
                field: Some(field.clone()),
                raw_choice: None,
                choice: Choice::Hold,
                reasons,
                transformations: record.transformations.clone(),
                suggestions: vec![],
                target: None,
            };
            let field_qualified = qualified && field.reasons.is_empty();
            if element == 0 {
                added += insert_mapping(
                    conn,
                    key,
                    j,
                    &r,
                    id,
                    "field",
                    &source_key,
                    name.as_deref(),
                    None,
                    0,
                    field_qualified,
                    &data,
                )
                .await?;
            }
            let start = element.saturating_sub(1);
            let take = if element == 0 { 49 } else { 50 };
            next_element = 1 + (start + take).min(field.choices.len());
            more = next_element < 1 + field.choices.len();
            for choice in field.choices.iter().skip(start).take(take) {
                let Some(raw) = &choice.raw else { continue };
                let option_id = Uuid::new_v4();
                let key_material = s::bytes(&(source_id, raw))?;
                let source_key = s::source_key(key, j.org, j.account, "option", &key_material);
                // Exact repeated choices are one mapping with the entire field held;
                // original duplicate ordinals remain in the field's exact evidence.
                if let Some(existing)=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='option' AND source_key=$3").bind(j.plan).bind(j.org.0).bind(&source_key).fetch_optional(&mut *conn).await? {let prior=decoded_mapping(key,j,&existing)?;if prior.raw_choice.as_ref()!=Some(raw) || existing.get::<Option<Uuid>,_>("parent_mapping_id")!=Some(id){return Err(MigrationError::SourceNotEligible)}continue}

                let group = if let Some(label) = &choice.label {
                    Some(
                        sqlx::query_scalar::<_, String>("SELECT lower($1::text)")
                            .bind(label)
                            .fetch_one(&mut *conn)
                            .await?,
                    )
                } else {
                    None
                };
                let folded = group.as_ref().map(|g| {
                    s::source_key(
                        key,
                        j.org,
                        j.account,
                        &format!("option-label:{source_id}"),
                        g.as_bytes(),
                    )
                });
                let data = Mapping {
                    source: BTreeMap::from([
                        (
                            "field_id".into(),
                            s::bytes(&source_id).map(|v| String::from_utf8(v).unwrap())?,
                        ),
                        ("choice".into(), serde_json::to_string(raw).unwrap()),
                        ("ordinal".into(), choice.ordinal.to_string()),
                    ]),
                    label: choice.label.clone(),
                    group,
                    source_name: field.name.clone(),
                    field: None,
                    raw_choice: Some(raw.clone()),
                    choice: Choice::Hold,
                    reasons: data.reasons.clone(),
                    transformations: vec![],
                    suggestions: vec![],
                    target: None,
                };
                added += insert_mapping(
                    conn,
                    key,
                    j,
                    &r,
                    option_id,
                    "option",
                    &source_key,
                    folded.as_deref(),
                    Some(id),
                    choice.ordinal as i32,
                    field_qualified,
                    &data,
                )
                .await?;
            }
        }
        Entity::Person(person) => {
            // Tag catalogs originate only from committed, live parent People.
            if qualified && parent_person(conn, j, r.get("source_id")).await?.is_some() {
                next_element = (element + 50).min(person.tags.len());
                more = next_element < person.tags.len();
                for tag in person.tags.iter().skip(element).take(50) {
                    let Some(raw) = &tag.raw else { continue };
                    let group = if let Some(label) = &tag.label {
                        Some(
                            sqlx::query_scalar::<_, String>("SELECT lower($1::text)")
                                .bind(label)
                                .fetch_one(&mut *conn)
                                .await?,
                        )
                    } else {
                        None
                    };
                    let source_key = s::source_key(
                        key,
                        j.org,
                        j.account,
                        if group.is_some() {
                            "tag-group"
                        } else {
                            "tag-raw"
                        },
                        group.as_deref().unwrap_or(raw).as_bytes(),
                    );
                    let old=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='tag' AND source_key=$3 FOR UPDATE").bind(j.plan).bind(j.org.0).bind(&source_key).fetch_optional(&mut *conn).await?;
                    let id = if let Some(old) = old {
                        let existing = decoded_mapping(key, j, &old)?;
                        if existing.group != group
                            || (group.is_none()
                                && existing.raw_choice.as_deref() != Some(raw.as_str()))
                        {
                            return Err(MigrationError::SourceNotEligible);
                        }
                        old.get("id")
                    } else {
                        let id = Uuid::new_v4();
                        let data = Mapping {
                            source: BTreeMap::from([(
                                "tag".into(),
                                serde_json::to_string(raw).unwrap(),
                            )]),
                            label: tag.label.clone(),
                            group: group.clone(),
                            source_name: None,
                            field: None,
                            raw_choice: Some(raw.clone()),
                            choice: Choice::Hold,
                            reasons: vec![],
                            transformations: if tag.label.as_deref().is_some_and(|v| v != raw) {
                                vec!["tag_label_trimmed".into()]
                            } else {
                                vec![]
                            },
                            suggestions: vec![],
                            target: None,
                        };
                        added += insert_mapping(
                            conn,
                            key,
                            j,
                            &r,
                            id,
                            "tag",
                            &source_key,
                            None,
                            None,
                            tag.ordinal as i32,
                            true,
                            &data,
                        )
                        .await?;
                        id
                    };
                    let alias_key =
                        s::source_key(key, j.org, j.account, "tag-alias", raw.as_bytes());
                    if let Some(alias)=sqlx::query("SELECT a.element_ordinal,s.* FROM migration_metadata_alias a JOIN migration_metadata_source s ON s.id=a.source_row_id AND s.organization_id=a.organization_id WHERE a.mapping_id=$1 AND a.organization_id=$2 AND a.alias_key=$3 ORDER BY a.id LIMIT 1").bind(id).bind(j.org.0).bind(&alias_key).fetch_optional(&mut *conn).await?{let old=source_record(key,j,&alias)?;let same=matches!(old.entity,Entity::Person(ref p) if p.tags.get(alias.get::<i32,_>("element_ordinal") as usize).and_then(|v|v.raw.as_ref())==Some(raw));if !same{return Err(MigrationError::SourceNotEligible)}}
                    sqlx::query("INSERT INTO migration_metadata_alias(id,plan_id,import_id,organization_id,mapping_id,source_row_id,element_ordinal,alias_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::new_v4()).bind(j.plan).bind(j.id).bind(j.org.0).bind(id).bind(r.get::<Uuid,_>("id")).bind(tag.ordinal as i32).bind(alias_key).execute(&mut *conn).await?;
                    added += 32;
                }
            }
        }
        Entity::Invalid => {}
    }
    if more {
        sqlx::query("UPDATE migration_metadata_plan SET checkpoint_element=$3 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(next_element as i32).execute(&mut *conn).await?;
    } else {
        sqlx::query("UPDATE migration_metadata_plan SET checkpoint_id=$3,checkpoint_element=0 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(r.get::<Uuid,_>("id")).execute(&mut *conn).await?;
    }
    s::settle(conn, j.org, j.id, token, added).await
}
async fn parent_person(
    conn: &mut PgConnection,
    j: &Job,
    source_id: &str,
) -> Result<Option<(Uuid, Uuid)>, MigrationError> {
    let row=sqlx::query("SELECT r.id,r.person_id FROM migration_import_identity i JOIN migration_import_result r ON r.import_id=i.import_id AND r.plan_id=i.plan_id AND r.organization_id=i.organization_id AND r.source_id=i.source_id AND r.person_id=i.target_id JOIN person p ON p.id=r.person_id AND p.organization_id=r.organization_id WHERE i.organization_id=$1 AND i.source_account_id=$2 AND i.family='people' AND i.source_id=$3 AND i.import_id=$4 AND i.plan_id=$5 AND r.disposition IN ('imported','already_imported')").bind(j.org.0).bind(j.account).bind(source_id).bind(j.parent).bind(j.parent_plan).fetch_optional(conn).await?;
    Ok(row.map(|r| (r.get("id"), r.get("person_id"))))
}
async fn frozen_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
) -> Result<Choice, MigrationError> {
    let c=sqlx::query("SELECT id,nonce,ciphertext FROM migration_metadata_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(j.plan).bind(j.org.0).bind(r.get::<String,_>("kind")).bind(r.get::<Vec<u8>,_>("source_key")).fetch_optional(conn).await?;
    match c {
        Some(c) => s::open(
            key,
            j.org,
            j.snapshot,
            j.plan,
            c.get("id"),
            "choice",
            &c.get::<Vec<u8>, _>("nonce"),
            &c.get::<Vec<u8>, _>("ciphertext"),
        ),
        None => Ok(Choice::Hold),
    }
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
async fn update_mapping(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
    data: &Mapping,
    disposition: &str,
    target: Option<Uuid>,
    field: Option<Uuid>,
    bound: i64,
) -> Result<i64, MigrationError> {
    let a = s::seal(key, j.org, j.snapshot, j.plan, r.get("id"), "mapping", data)?;
    let delta = s::sealed_bytes(&a)
        - (r.get::<Vec<u8>, _>("nonce").len() + r.get::<Vec<u8>, _>("ciphertext").len()) as i64;
    sqlx::query("UPDATE migration_metadata_mapping SET disposition=$3,target_id=$4,target_field_id=$5,added_byte_bound=$6,nonce=$7,ciphertext=$8 WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("id")).bind(j.org.0).bind(disposition).bind(target).bind(field).bind(bound).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(conn).await?;
    Ok(delta)
}
async fn mappings(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let stage = p.get::<String, _>("checkpoint_kind");
    let finalize = stage.starts_with("final_");
    let kind = if stage.is_empty() {
        "field"
    } else {
        stage.strip_prefix("final_").unwrap_or(stage.as_str())
    };
    let r=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND id>$4 ORDER BY id LIMIT 1").bind(j.plan).bind(j.org.0).bind(kind).bind(p.get::<Option<Uuid>,_>("checkpoint_id").unwrap_or(Uuid::nil())).fetch_optional(&mut *conn).await?;
    let Some(r) = r else {
        let next = match (finalize, kind) {
            (false, "field") => "option",
            (false, "option") => "tag",
            (false, _) => "final_field",
            (true, "field") => "final_option",
            (true, "option") => "final_tag",
            _ => return phase(conn, j, "people").await,
        };
        sqlx::query("UPDATE migration_metadata_plan SET checkpoint_kind=$3,checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(next).execute(conn).await?;
        return Ok(());
    };
    let token = admission(conn, j, s::UNIT).await?;
    let dst = metadata::decode_destination(key, &s::run(conn, j.org, j.id).await?, p)?;
    let mut data = decoded_mapping(key, j, &r)?;
    let kind: String = r.get("kind");
    let mut disposition = r.get::<String, _>("disposition");
    let mut target: Option<Uuid> = r.get("target_id");
    let mut target_field: Option<Uuid> = r.get("target_field_id");
    if !finalize {
        data.choice = frozen_choice(conn, key, j, &r).await?;
        data.reasons.retain(|x| x != "mapping_held");
        if !r.get::<bool, _>("qualified") {
            m::mark(&mut data.reasons, "source_integrity")
        }
        if kind == "field" && r.get::<Option<Vec<u8>>, _>("name_hmac").is_some() {
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND name_hmac=$3 LIMIT 2) bounded").bind(j.plan).bind(j.org.0).bind(r.get::<Vec<u8>,_>("name_hmac")).fetch_one(&mut *conn).await?;
            if n > 1 {
                m::mark(&mut data.reasons, "source_field_key_collision")
            }
        }
        if kind == "field" {
            if let Some(field) = data
                .field
                .as_ref()
                .filter(|f| f.field_type.as_deref() == Some("choice"))
            {
                // Rust folding identifies definite collisions; destination
                // collation can equate additional labels (for example İ/i).
                // This array is bounded by the qualified raw source record.
                let labels: Vec<&str> = field
                    .choices
                    .iter()
                    .filter_map(|c| c.label.as_deref())
                    .collect();
                let collision: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM unnest($1::text[]) AS labels(label) GROUP BY lower(label) HAVING count(*)>1)").bind(labels).fetch_one(&mut *conn).await?;
                if collision {
                    m::mark(&mut data.reasons, "colliding_source_choices");
                }
            }
        }
        let parent = if kind == "option" {
            Some(sqlx::query("SELECT * FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(r.get::<Uuid,_>("parent_mapping_id")).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?)
        } else {
            None
        };
        if let Some(parent) = &parent {
            target_field = parent.get("target_id");
            let parent_data = decoded_mapping(key, j, parent)?;
            for reason in &parent_data.reasons {
                if matches!(
                    reason.as_str(),
                    "source_integrity"
                        | "source_field_key_collision"
                        | "colliding_source_choices"
                        | "duplicate_source_choice"
                ) {
                    m::mark(&mut data.reasons, reason);
                }
            }
            if parent.get::<String, _>("disposition") == "hold" {
                m::mark(&mut data.reasons, "field_mapping_held")
            }
        }
        let source_type = data.field.as_ref().and_then(|f| f.field_type.as_deref());
        let candidates = dst
            .all()
            .filter(|v| {
                v.kind == kind
                    && (kind != "field" || v.field_type.as_deref() == source_type)
                    && (kind != "option" || v.field_id == target_field)
            })
            .filter(|v| {
                if kind == "field" {
                    (v.source.as_deref() == Some("fub") && v.external_key == data.source_name)
                        || data
                            .label
                            .as_ref()
                            .is_some_and(|l| l.eq_ignore_ascii_case(&v.label))
                } else {
                    data.label
                        .as_ref()
                        .is_some_and(|l| l.eq_ignore_ascii_case(&v.label))
                }
            })
            .take(10)
            .cloned()
            .collect::<Vec<_>>();
        data.suggestions = candidates;
        match &data.choice {
            Choice::Hold => m::mark(&mut data.reasons, "mapping_held"),
            Choice::MapExisting { target_id } => {
                if let Some(t) = dst.by_id(*target_id).filter(|v| {
                    v.kind == kind
                        && (kind != "field" || v.field_type.as_deref() == source_type)
                        && (kind != "option" || v.field_id == target_field)
                }) {
                    if kind == "field"
                        && t.source.is_some()
                        && (t.source.as_deref() != Some("fub")
                            || t.external_key != data.source_name)
                    {
                        m::mark(&mut data.reasons, "source_binding_conflict")
                    }
                    target = Some(t.id);
                    data.target = Some(t.clone());
                    disposition = "map_existing".into();
                } else {
                    m::mark(&mut data.reasons, "mapping_target_unavailable")
                }
            }
            Choice::CreateMatching => {
                if data.label.is_none() {
                    m::mark(&mut data.reasons, "native_label_unavailable")
                }
                if let Some(f) = &data.field {
                    for reason in &f.creation_reasons {
                        m::mark(&mut data.reasons, reason)
                    }
                }
                let label = data.label.clone().unwrap_or_default();
                let same = if !label.is_empty() && !label.contains('\0') && label.len() <= 2048 {
                    let folded: String = sqlx::query_scalar("SELECT lower($1::text)")
                        .bind(&label)
                        .fetch_one(&mut *conn)
                        .await?;
                    let ids = if kind == "tag" {
                        sqlx::query_scalar::<_, Uuid>(
                            "SELECT id FROM tag WHERE organization_id=$1 AND lower(name)=$2",
                        )
                        .bind(j.org.0)
                        .bind(folded)
                        .fetch_all(&mut *conn)
                        .await?
                    } else if kind == "field" {
                        sqlx::query_scalar::<_,Uuid>("SELECT id FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL AND lower(label)=$2").bind(j.org.0).bind(folded).fetch_all(&mut *conn).await?
                    } else {
                        sqlx::query_scalar::<_,Uuid>("SELECT id FROM custom_field_option WHERE organization_id=$1 AND field_id=$2 AND archived_at IS NULL AND lower(label)=$3").bind(j.org.0).bind(target_field).bind(folded).fetch_all(&mut *conn).await?
                    };
                    ids.first().copied()
                } else {
                    None
                };
                if let Some(same) = same {
                    if kind == "tag" {
                        target = Some(same);
                        data.target = dst.by_id(same).cloned();
                        disposition = "map_existing".into();
                    } else {
                        m::mark(&mut data.reasons, "native_label_conflict")
                    }
                } else {
                    let planned = Uuid::new_v4();
                    target = Some(planned);
                    disposition = "create_matching".into();
                    data.target = Some(Target {
                        id: planned,
                        kind: kind.clone(),
                        field_id: target_field,
                        label,
                        field_type: source_type.map(str::to_owned),
                        source: if kind == "field" {
                            Some("fub".into())
                        } else {
                            None
                        },
                        external_key: if kind == "field" {
                            data.source_name.clone()
                        } else {
                            None
                        },
                        position: 0,
                    });
                }
                if kind == "field" {
                    if let Some(name) = data
                        .source_name
                        .as_ref()
                        .filter(|v| !v.contains('\0') && v.len() <= 2048)
                    {
                        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_field WHERE organization_id=$1 AND source='fub' AND external_key=$2)").bind(j.org.0).bind(name).fetch_one(&mut *conn).await?;
                        if exists {
                            m::mark(&mut data.reasons, "source_binding_conflict")
                        }
                    }
                }
            }
        }
    } else {
        if disposition != "hold" {
            if matches!(kind.as_str(), "field" | "option") && target.is_some() {
                let n:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_target_id=$4 AND candidate_disposition<>'hold' LIMIT 2) bounded").bind(j.plan).bind(j.org.0).bind(&kind).bind(target).fetch_one(&mut *conn).await?;
                if n > 1 {
                    m::mark(&mut data.reasons, "mapping_target_collision")
                }
            }
            if kind == "option" {
                let parent=sqlx::query("SELECT disposition FROM migration_metadata_mapping WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("parent_mapping_id")).bind(j.org.0).fetch_one(&mut *conn).await?;
                if parent.get::<String, _>("disposition") == "hold" {
                    m::mark(&mut data.reasons, "field_mapping_held")
                }
            }
            if disposition == "create_matching" {
                if r.get::<Option<Vec<u8>>, _>("target_label_hmac").is_some() {
                    let n:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_disposition='create_matching' AND target_label_hmac=$4 AND candidate_field_id IS NOT DISTINCT FROM $5 LIMIT 2) bounded").bind(j.plan).bind(j.org.0).bind(&kind).bind(r.get::<Vec<u8>,_>("target_label_hmac")).bind(target_field).fetch_one(&mut *conn).await?;
                    if n > 1 {
                        m::mark(&mut data.reasons, "native_label_conflict")
                    }
                }

                let new_count: i64 = if kind == "option" {
                    sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='option' AND candidate_field_id=$3 AND candidate_disposition='create_matching' LIMIT 51) bounded").bind(j.plan).bind(j.org.0).bind(target_field).fetch_one(&mut *conn).await?
                } else {
                    sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND candidate_disposition='create_matching' LIMIT 201) bounded").bind(j.plan).bind(j.org.0).bind(&kind).fetch_one(&mut *conn).await?
                };
                let (existing, limit) = match kind.as_str() {
                    "tag" => (dst.tags.len() as i64, 200),
                    "field" => (dst.fields.len() as i64, 50),
                    _ => (
                        dst.options
                            .iter()
                            .filter(|o| o.field_id == target_field)
                            .count() as i64,
                        50,
                    ),
                };
                if existing + new_count > limit {
                    m::mark(&mut data.reasons, "native_capacity")
                }
                if kind == "field"
                    && data
                        .field
                        .as_ref()
                        .is_some_and(|f| f.field_type.as_deref() == Some("choice"))
                {
                    let n:i64=sqlx::query_scalar("SELECT count(*) FROM migration_metadata_mapping WHERE parent_mapping_id=$1 AND organization_id=$2 AND candidate_disposition='create_matching'").bind(r.get::<Uuid,_>("id")).bind(j.org.0).fetch_one(&mut *conn).await?;
                    if n as usize != data.field.as_ref().unwrap().choices.len() {
                        m::mark(&mut data.reasons, "choice_creation_incomplete")
                    }
                }
            }
            if let Some(identity)=sqlx::query("SELECT target_id FROM migration_metadata_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4").bind(j.org.0).bind(j.account).bind(&kind).bind(r.get::<Vec<u8>,_>("source_key")).fetch_optional(&mut *conn).await?{if Some(identity.get::<Uuid,_>("target_id"))!=target{m::mark(&mut data.reasons,"identity_target_unavailable")}}
        }
    }
    if !data.reasons.is_empty() {
        disposition = "hold".into();
        target = None;
        data.target = None;
    }
    let mut bound = (s::bytes(&data)?.len() as i64)
        .saturating_mul(3)
        .saturating_add(256 * 1024);
    if finalize
        && kind == "field"
        && disposition == "create_matching"
        && data
            .field
            .as_ref()
            .is_some_and(|f| f.field_type.as_deref() == Some("choice"))
    {
        let children:i64=sqlx::query_scalar("SELECT COALESCE(sum(added_byte_bound),0)::bigint FROM migration_metadata_mapping WHERE parent_mapping_id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("id")).bind(j.org.0).fetch_one(&mut *conn).await?;
        bound = bound.saturating_add(children);
    }

    if bound > s::UNIT {
        m::mark(&mut data.reasons, "import_item_byte_limit");
        disposition = "hold".into();
        target = None;
        data.target = None;
    }
    let mut extra = 0;
    if !finalize {
        let label_hash = if let Some(t) = &data.target {
            let folded: String = sqlx::query_scalar("SELECT lower($1::text)")
                .bind(&t.label)
                .fetch_one(&mut *conn)
                .await?;
            Some(s::source_key(
                key,
                j.org,
                j.account,
                "target-label",
                folded.as_bytes(),
            ))
        } else {
            None
        };
        extra = label_hash.as_ref().map_or(0, |v| v.len() as i64);
        sqlx::query("UPDATE migration_metadata_mapping SET candidate_disposition=$3,candidate_target_id=$4,candidate_field_id=$5,target_label_hmac=$6 WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("id")).bind(j.org.0).bind(&disposition).bind(target).bind(target_field).bind(label_hash).execute(&mut *conn).await?;
    }
    let added = extra
        + update_mapping(
            conn,
            key,
            j,
            &r,
            &data,
            &disposition,
            target,
            target_field,
            bound.min(s::UNIT),
        )
        .await?;
    if finalize {
        let mut counts = Counts::load(p.get("counts"))?;
        counts.planned(
            &kind,
            if disposition == "hold" {
                "held"
            } else if disposition == "create_matching" {
                "eligible"
            } else {
                "already_present"
            },
        );
        save_counts(conn, j, &counts, false).await?;
        issues(conn, j, &data.reasons).await?;
    }
    sqlx::query("UPDATE migration_metadata_plan SET checkpoint_id=$3,max_added_byte_bound=GREATEST(max_added_byte_bound,$4) WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(r.get::<Uuid,_>("id")).bind(bound.min(s::UNIT)).execute(&mut *conn).await?;
    s::settle(conn, j.org, j.id, token, added).await
}

async fn value_state(
    conn: &mut PgConnection,
    org: OrganizationId,
    person: Uuid,
    field: Uuid,
    value: &source::NativeValue,
) -> Result<Option<bool>, MigrationError> {
    let (text, number, date, option) = native_parts(value)?;
    Ok(sqlx::query_scalar("SELECT text_value IS NOT DISTINCT FROM $4 AND number_value IS NOT DISTINCT FROM CAST($5::text AS numeric) AND date_value IS NOT DISTINCT FROM $6 AND option_id IS NOT DISTINCT FROM $7 FROM person_custom_field_value WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(org.0).bind(person).bind(field).bind(text).bind(number).bind(date).bind(option).fetch_optional(conn).await?)
}
type NativeParts<'a> = (
    Option<&'a str>,
    Option<&'a str>,
    Option<chrono::NaiveDate>,
    Option<Uuid>,
);
fn native_parts(value: &source::NativeValue) -> Result<NativeParts<'_>, MigrationError> {
    Ok(match value {
        source::NativeValue::Text(v) => (Some(v.as_str()), None, None, None),
        source::NativeValue::Number(v) => (None, Some(v.as_str()), None, None),
        source::NativeValue::Date(v) => (
            None,
            None,
            Some(v.parse().map_err(|_| MigrationError::SourceNotEligible)?),
            None,
        ),
        source::NativeValue::Choice(v) => (
            None,
            None,
            None,
            Some(v.parse().map_err(|_| MigrationError::SourceNotEligible)?),
        ),
    })
}
async fn people(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let Some(r) = next_source(conn, j, "people", p.get("checkpoint_id")).await? else {
        let token = admission(conn, j, 4096).await?;
        let digest = crypto::snapshot_hmac(
            key,
            j.org,
            "metadata-confirmation",
            &s::bytes(
                &json!({"engine":metadata::ENGINE,"import":j.id,"plan":j.plan,"snapshot":j.snapshot,"boundary":j.boundary,"counts":p.get::<Value,_>("counts"),"destination":p.get::<Vec<u8>,_>("destination_ciphertext"),"patch":p.get::<Vec<u8>,_>("patch_ciphertext")}),
            )?,
        );
        sqlx::query("UPDATE migration_metadata_plan SET state='ready',phase='ready',confirmation_digest=$3,expires_at=now()+interval '10 minutes',completed_at=now(),checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(digest.as_slice()).execute(&mut *conn).await?;
        s::settle(conn, j.org, j.id, token, 32).await?;
        return Ok(());
    };
    let token = admission(conn, j, s::UNIT).await?;
    let record = source_record(key, j, &r)?;
    let source_id: String = r.get("source_id");
    let parent = parent_person(conn, j, &source_id).await?;
    let mut manifest = Manifest {
        reasons: record.reasons.clone(),
        operations: vec![],
    };
    if !r.get::<bool, _>("qualified") || r.get::<bool, _>("conflict") {
        m::mark(&mut manifest.reasons, "source_integrity")
    }
    if parent.is_none() {
        m::mark(&mut manifest.reasons, "parent_person_unavailable")
    }
    let mut counts = Counts::load(p.get("counts"))?;
    counts.people.source += 1;
    let qualified = manifest.reasons.is_empty() && matches!(&record.entity, Entity::Person(_));
    let mut seen_tags = BTreeSet::new();
    let mut operations_bytes = 0usize;
    if qualified {
        let person = parent.unwrap().1;
        if let Entity::Person(person_source) = &record.entity {
            if person_source.tags_state == "held" {
                m::mark(&mut manifest.reasons, "tags_shape_unqualified")
            }
            for tag in &person_source.tags {
                let a=sqlx::query("SELECT m.* FROM migration_metadata_alias a JOIN migration_metadata_mapping m ON m.id=a.mapping_id AND m.plan_id=a.plan_id AND m.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.source_row_id=$3 AND a.element_ordinal=$4").bind(j.plan).bind(j.org.0).bind(r.get::<Uuid,_>("id")).bind(tag.ordinal as i32).fetch_optional(&mut *conn).await?;
                let (mapping_id, target, source_key, reasons) = if let Some(a) = a {
                    let data = decoded_mapping(key, j, &a)?;
                    (
                        Some(a.get::<Uuid, _>("id")),
                        a.get::<Option<Uuid>, _>("target_id"),
                        a.get::<Vec<u8>, _>("source_key"),
                        data.reasons,
                    )
                } else {
                    (
                        None,
                        None,
                        s::source_key(
                            key,
                            j.org,
                            j.account,
                            "unqualified-tag",
                            &s::bytes(&json!([source_id, tag.ordinal]))?,
                        ),
                        tag.reasons.clone(),
                    )
                };
                if !seen_tags.insert(source_key.clone()) {
                    continue;
                }
                let mut op = Operation {
                    id: Uuid::new_v4(),
                    kind: "tag_link".into(),
                    source_key,
                    source_field: None,
                    mapping_id,
                    target_id: target,
                    disposition: if target.is_some() { "eligible" } else { "held" }.into(),
                    value: None,
                    reasons,
                    transformations: vec![],
                };
                if let Some(target) = target {
                    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_tag WHERE organization_id=$1 AND person_id=$2 AND tag_id=$3)").bind(j.org.0).bind(person).bind(target).fetch_one(&mut *conn).await?;
                    if exists {
                        op.disposition = "already_present".into();
                    }
                }
                operations_bytes += s::bytes(&op)?.len();
                manifest.operations.push(op);
            }
            let existing: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM person_tag WHERE organization_id=$1 AND person_id=$2",
            )
            .bind(j.org.0)
            .bind(person)
            .fetch_one(&mut *conn)
            .await?;
            let new_tags = manifest
                .operations
                .iter()
                .filter(|o| o.kind == "tag_link" && o.disposition == "eligible")
                .filter_map(|o| o.target_id)
                .collect::<BTreeSet<_>>()
                .len() as i64;
            if existing + new_tags > 20 {
                for op in manifest
                    .operations
                    .iter_mut()
                    .filter(|o| o.kind == "tag_link" && o.disposition == "eligible")
                {
                    op.disposition = "held".into();
                    m::mark(&mut op.reasons, "person_tag_capacity")
                }
            }
        }
        let mut declared_fields = BTreeSet::new();
        let mut last = Uuid::nil();
        loop {
            let fields=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND id>$3 ORDER BY id LIMIT 50").bind(j.plan).bind(j.org.0).bind(last).fetch_all(&mut *conn).await?;
            if fields.is_empty() {
                break;
            }
            for field_row in fields {
                last = field_row.get("id");
                let data = decoded_mapping(key, j, &field_row)?;
                let Some(field) = data.field.as_ref() else {
                    continue;
                };
                if let Some(name) = &field.name {
                    declared_fields.insert(name.as_str().to_owned());
                }
                let mut value = source::extract_value(&record, field);
                let target: Option<Uuid> = field_row.get("target_id");
                if value.disposition == "eligible"
                    && (target.is_none() || field_row.get::<String, _>("disposition") == "hold")
                {
                    value.disposition = "held".into();
                    value.reasons.extend(data.reasons.clone());
                    m::mark(&mut value.reasons, "field_mapping_held")
                }
                if value.disposition == "eligible" {
                    if let Some(source::NativeValue::Choice(raw)) = &value.value {
                        let raw = raw.clone();
                        let field_source=sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_metadata_source WHERE id=$1 AND organization_id=$2").bind(field_row.get::<Uuid,_>("source_row_id")).bind(j.org.0).fetch_one(&mut *conn).await?;
                        let sk = s::source_key(
                            key,
                            j.org,
                            j.account,
                            "option",
                            &s::bytes(&json!([field_source, raw]))?,
                        );
                        let option=sqlx::query("SELECT target_id,disposition,target_field_id FROM migration_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='option' AND source_key=$3 AND parent_mapping_id=$4").bind(j.plan).bind(j.org.0).bind(sk).bind(last).fetch_optional(&mut *conn).await?;
                        if let Some(option) = option.filter(|o| {
                            o.get::<String, _>("disposition") != "hold"
                                && o.get::<Option<Uuid>, _>("target_field_id") == target
                        }) {
                            value.value = Some(source::NativeValue::Choice(
                                option.get::<Uuid, _>("target_id").to_string(),
                            ));
                        } else {
                            value.disposition = "held".into();
                            m::mark(&mut value.reasons, "option_mapping_held")
                        }
                    }
                }
                if value.disposition == "eligible" {
                    match value_state(
                        conn,
                        j.org,
                        person,
                        target.unwrap(),
                        value
                            .value
                            .as_ref()
                            .ok_or(MigrationError::SourceNotEligible)?,
                    )
                    .await?
                    {
                        Some(true) => value.disposition = "already_present".into(),
                        Some(false) => {
                            value.disposition = "held".into();
                            m::mark(&mut value.reasons, "local_value_conflict")
                        }
                        None => {}
                    }
                }
                let op = Operation {
                    id: Uuid::new_v4(),
                    kind: "value".into(),
                    source_key: field_row.get("source_key"),
                    source_field: field.name.clone(),
                    mapping_id: Some(last),
                    target_id: target,
                    disposition: value.disposition,
                    value: value.value,
                    reasons: value.reasons,
                    transformations: value.transformations,
                };
                operations_bytes += s::bytes(&op)?.len();
                manifest.operations.push(op);
                if operations_bytes > 16 * 1024 * 1024 {
                    return Err(MigrationError::SourceNotEligible);
                }
            }
        }
        for name in record
            .provenance
            .keys()
            .filter(|name| name.starts_with("custom") && !declared_fields.contains(*name))
        {
            // Keep exact original values in source evidence without guessing a
            // type or promoting a custom-looking key into a field definition.
            let op = Operation {
                id: Uuid::new_v4(),
                kind: "value".into(),
                source_key: s::source_key(
                    key,
                    j.org,
                    j.account,
                    "unresolved-field",
                    name.as_bytes(),
                ),
                source_field: Some(name.clone()),
                mapping_id: None,
                target_id: None,
                disposition: "held".into(),
                value: None,
                reasons: vec!["source_definition_unavailable".into()],
                transformations: vec![],
            };
            operations_bytes += s::bytes(&op)?.len();
            if operations_bytes > 16 * 1024 * 1024 {
                return Err(MigrationError::SourceNotEligible);
            }
            manifest.operations.push(op);
        }
    }
    if qualified {
        counts.people.eligible += 1
    } else {
        counts.people.excluded += 1
    }
    let planned = s::bytes(&manifest)?.len() as i64;
    let source_bytes = s::bytes(&record.provenance)?.len() as i64;
    // JSON result encryption, exact source, all outcomes, native variable-width
    // cells, operation identity keys, and fixed closed receipt/counter allowance.
    let bound = source_bytes
        .saturating_add(planned.saturating_mul(3))
        .saturating_add(256 * 1024);
    if bound > s::UNIT {
        for op in &mut manifest.operations {
            op.disposition = "held".into();
            m::mark(&mut op.reasons, "import_item_byte_limit")
        }
        m::mark(&mut manifest.reasons, "import_item_byte_limit")
    }
    let id = Uuid::new_v4();
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "manifest", &manifest)?;
    let mut added = s::sealed_bytes(&a) + source_id.len() as i64;
    sqlx::query("INSERT INTO migration_metadata_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,parent_result_id,person_id,disposition,added_byte_bound,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(&source_id).bind(r.get::<Uuid,_>("id")).bind(parent.map(|p|p.0)).bind(parent.map(|p|p.1)).bind(if qualified{"eligible"}else{"held"}).bind(bound.min(s::UNIT)).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(&mut *conn).await?;
    for op in &manifest.operations {
        counts.planned(&op.kind, &op.disposition);
        issues(conn, j, &op.reasons).await?;
        sqlx::query("INSERT INTO migration_metadata_operation(id,plan_id,import_id,organization_id,manifest_id,mapping_id,kind,source_key,target_id,disposition) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(op.id).bind(j.plan).bind(j.id).bind(j.org.0).bind(id).bind(op.mapping_id).bind(&op.kind).bind(&op.source_key).bind(op.target_id).bind(&op.disposition).execute(&mut *conn).await?;
        added += 32;
        if let Some(mapping) = op.mapping_id {
            sqlx::query("UPDATE migration_metadata_mapping SET dependent_count=dependent_count+1 WHERE id=$1 AND organization_id=$2").bind(mapping).bind(j.org.0).execute(&mut *conn).await?;
        }
    }
    issues(conn, j, &manifest.reasons).await?;
    save_counts(conn, j, &counts, false).await?;
    sqlx::query("UPDATE migration_metadata_plan SET checkpoint_id=$3,max_added_byte_bound=GREATEST(max_added_byte_bound,$4) WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(r.get::<Uuid,_>("id")).bind(bound.min(s::UNIT)).execute(&mut *conn).await?;
    s::settle(conn, j.org, j.id, token, added).await
}

pub(crate) async fn validate_catalog(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let frozen = metadata::decode_destination(key, r, p)?;
    let current =
        metadata::destination(conn, OrganizationId::new(r.get("organization_id"))).await?;
    // Before confirmation the catalog must still match the exact frozen local
    // comparison. Review-mode command/DB guards keep it stable thereafter.
    if s::bytes(&frozen)? != s::bytes(&current)? {
        return Err(MigrationError::InvalidImportChoice);
    }
    Ok(())
}
async fn permit(conn: &mut PgConnection, j: &Job, unit: Uuid) -> Result<(), MigrationError> {
    sqlx::query(
        "SELECT set_config('crm.metadata_token',$1,true),set_config('crm.metadata_unit',$2,true)",
    )
    .bind(j.token.to_string())
    .bind(unit.to_string())
    .execute(conn)
    .await?;
    Ok(())
}
async fn write_identity(
    conn: &mut PgConnection,
    j: &Job,
    row: &sqlx::postgres::PgRow,
    target: Uuid,
) -> Result<i64, MigrationError> {
    let old=sqlx::query("SELECT target_id FROM migration_metadata_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4").bind(j.org.0).bind(j.account).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).fetch_optional(&mut *conn).await?;
    if let Some(old) = old {
        if old.get::<Uuid, _>("target_id") != target {
            return Err(MigrationError::InvalidImportChoice);
        }
        return Ok(0);
    }
    sqlx::query("INSERT INTO migration_metadata_identity(organization_id,source_account_id,kind,source_key,target_id,import_id,plan_id,mapping_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(j.org.0).bind(j.account).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(target).bind(j.id).bind(j.plan).bind(row.get::<Uuid,_>("id")).execute(conn).await?;
    Ok(32)
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
async fn result(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    kind: &str,
    unit: Uuid,
    source_id: Option<&str>,
    person: Option<Uuid>,
    disposition: &str,
    counts: Value,
    data: &ResultData,
) -> Result<i64, MigrationError> {
    let id = Uuid::new_v4();
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "result", data)?;
    let size = s::sealed_bytes(&a) + source_id.map_or(0, |v| v.len() as i64);
    sqlx::query("INSERT INTO migration_metadata_result(id,import_id,plan_id,organization_id,kind,mapping_id,manifest_id,unit_id,source_id,person_id,disposition,counts,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(id).bind(j.id).bind(j.plan).bind(j.org.0).bind(kind).bind(if kind=="people"{None}else{Some(unit)}).bind(if kind=="people"{Some(unit)}else{None}).bind(unit).bind(source_id).bind(person).bind(disposition).bind(counts).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(conn).await?;
    Ok(size)
}
async fn target_live(
    conn: &mut PgConnection,
    org: OrganizationId,
    t: &Target,
) -> Result<bool, MigrationError> {
    Ok(match t.kind.as_str(){"tag"=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tag WHERE id=$1 AND organization_id=$2 AND name=$3)").bind(t.id).bind(org.0).bind(&t.label).fetch_one(conn).await?,"field"=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_field WHERE id=$1 AND organization_id=$2 AND label=$3 AND field_type=$4 AND source IS NOT DISTINCT FROM $5 AND external_key IS NOT DISTINCT FROM $6 AND archived_at IS NULL)").bind(t.id).bind(org.0).bind(&t.label).bind(&t.field_type).bind(&t.source).bind(&t.external_key).fetch_one(conn).await?,"option"=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.id=$1 AND o.organization_id=$2 AND o.label=$3 AND o.field_id=$4 AND o.archived_at IS NULL AND f.archived_at IS NULL AND f.field_type='choice')").bind(t.id).bind(org.0).bind(&t.label).bind(t.field_id).fetch_one(conn).await?,_=>false})
}
async fn insert_native(
    conn: &mut PgConnection,
    j: &Job,
    t: &Target,
) -> Result<i64, MigrationError> {
    match t.kind.as_str() {
        "tag" => {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tag WHERE organization_id=$1")
                .bind(j.org.0)
                .fetch_one(&mut *conn)
                .await?;
            if n >= 200 {
                return Err(MigrationError::InvalidImportChoice);
            }
            sqlx::query(
                "INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,$4)",
            )
            .bind(t.id)
            .bind(j.org.0)
            .bind(j.actor.0)
            .bind(&t.label)
            .execute(&mut *conn)
            .await?;
        }
        "field" => {
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL").bind(j.org.0).fetch_one(&mut *conn).await?;
            if n >= 50 {
                return Err(MigrationError::InvalidImportChoice);
            }
            sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id,source,external_key) VALUES($1,$2,$3,$4,(SELECT COALESCE(max(position),-1)+1 FROM custom_field WHERE organization_id=$2 AND archived_at IS NULL),$5,'fub',$6)").bind(t.id).bind(j.org.0).bind(&t.label).bind(&t.field_type).bind(j.actor.0).bind(&t.external_key).execute(&mut *conn).await?;
        }
        "option" => {
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM custom_field_option WHERE organization_id=$1 AND field_id=$2 AND archived_at IS NULL").bind(j.org.0).bind(t.field_id).fetch_one(&mut *conn).await?;
            if n >= 50 {
                return Err(MigrationError::InvalidImportChoice);
            }
            sqlx::query("INSERT INTO custom_field_option(id,organization_id,field_id,label,position) VALUES($1,$2,$3,$4,(SELECT COALESCE(max(position),-1)+1 FROM custom_field_option WHERE organization_id=$2 AND field_id=$3 AND archived_at IS NULL))").bind(t.id).bind(j.org.0).bind(t.field_id).bind(&t.label).execute(&mut *conn).await?;
        }
        _ => return Err(MigrationError::SourceNotEligible),
    }
    Ok(0)
}
async fn catalog_result(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
    data: &Mapping,
    counts: &mut Counts,
) -> Result<i64, MigrationError> {
    let kind: String = r.get("kind");
    let disposition: String = r.get("disposition");
    let mut added = 0;
    let outcome = match disposition.as_str() {
        "hold" => "held",
        "map_existing" => {
            let t = data
                .target
                .as_ref()
                .ok_or(MigrationError::SourceNotEligible)?;
            if !target_live(conn, j.org, t).await? {
                return Err(MigrationError::InvalidImportChoice);
            }
            added += write_identity(conn, j, r, t.id).await?;
            "already_present"
        }
        "create_matching" => {
            let t = data
                .target
                .as_ref()
                .ok_or(MigrationError::SourceNotEligible)?;
            added += insert_native(conn, j, t).await?;
            added += write_identity(conn, j, r, t.id).await?;
            "created"
        }
        _ => return Err(MigrationError::SourceNotEligible),
    };
    let old = if disposition == "hold" {
        "held"
    } else if disposition == "map_existing" {
        "already_present"
    } else {
        "eligible"
    };
    counts.settled(&kind, old, outcome);
    let source_id: Option<String> = if let Some(source) = r
        .get::<Option<Uuid>, _>("source_row_id")
        .filter(|_| kind == "field")
    {
        sqlx::query_scalar(
            "SELECT source_id FROM migration_metadata_source WHERE id=$1 AND organization_id=$2",
        )
        .bind(source)
        .bind(j.org.0)
        .fetch_optional(&mut *conn)
        .await?
    } else {
        None
    };
    let mut local = Counts::default();
    local.outcome(&kind, outcome);
    let operation = json!({"choice":data.choice,"target":data.target.as_ref().map(Target::wire),"outcome":outcome,"transformations":data.transformations});
    added += result(
        conn,
        key,
        j,
        &kind,
        r.get("id"),
        source_id.as_deref(),
        None,
        outcome,
        local.wire(),
        &ResultData {
            source: data.source.clone(),
            operations: operation,
            reasons: data.reasons.clone(),
        },
    )
    .await?;
    Ok(added)
}
async fn execute(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if r.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(j.plan)
        || p.get::<String, _>("state") != "ready"
    {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("UPDATE migration_metadata_import SET state='running',updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(&mut *conn).await?;
    if r.get::<String, _>("phase") == "catalog" {
        // Indexed anti-probes skip only committed units. Kind order puts fields
        // before their options; new choice field and its options commit together.
        let cursor = if let Some(last) = r.get::<Option<Uuid>, _>("checkpoint_id") {
            let c=sqlx::query("SELECT kind,source_sequence,source_ordinal,element_ordinal,id FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(last).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
            (
                c.get::<String, _>("kind"),
                c.get::<i64, _>("source_sequence"),
                c.get::<i32, _>("source_ordinal"),
                c.get::<i32, _>("element_ordinal"),
                last,
            )
        } else {
            (String::new(), 0, -1, -1, Uuid::nil())
        };
        let row=sqlx::query("SELECT m.* FROM migration_metadata_mapping m WHERE m.plan_id=$1 AND m.organization_id=$2 AND (m.kind,m.source_sequence,m.source_ordinal,m.element_ordinal,m.id)>($4,$5,$6,$7,$8) AND NOT EXISTS(SELECT 1 FROM migration_metadata_result x WHERE x.import_id=$3 AND x.organization_id=$2 AND x.unit_id=m.id) ORDER BY m.kind,m.source_sequence,m.source_ordinal,m.element_ordinal,m.id LIMIT 1").bind(j.plan).bind(j.org.0).bind(j.id).bind(cursor.0).bind(cursor.1).bind(cursor.2).bind(cursor.3).bind(cursor.4).fetch_optional(&mut *conn).await?;
        let Some(row) = row else {
            sqlx::query("UPDATE migration_metadata_import SET phase='people',checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(conn).await?;
            return Ok(());
        };
        let data = decoded_mapping(key, j, &row)?;
        let children = if row.get::<String, _>("kind") == "field"
            && row.get::<String, _>("disposition") == "create_matching"
            && data
                .field
                .as_ref()
                .is_some_and(|f| f.field_type.as_deref() == Some("choice"))
        {
            sqlx::query("SELECT * FROM migration_metadata_mapping WHERE parent_mapping_id=$1 AND organization_id=$2 ORDER BY element_ordinal,id LIMIT 51").bind(row.get::<Uuid,_>("id")).bind(j.org.0).fetch_all(&mut *conn).await?
        } else {
            vec![]
        };
        if children.len() > 50 {
            return Err(MigrationError::SourceNotEligible);
        }
        let bound = row.get::<i64, _>("added_byte_bound");
        if bound > s::UNIT {
            return Err(MigrationError::StorageLimit);
        }
        let token = admission(conn, j, bound.max(4096)).await?;
        permit(conn, j, row.get("id")).await?;
        let mut counts = Counts::load(r.get("counts"))?;
        let mut added = catalog_result(conn, key, j, &row, &data, &mut counts).await?;
        for child in &children {
            if child.get::<String, _>("disposition") != "create_matching" {
                return Err(MigrationError::InvalidImportChoice);
            }
            let child_data = decoded_mapping(key, j, child)?;
            added += catalog_result(conn, key, j, child, &child_data, &mut counts).await?;
        }
        sqlx::query("UPDATE migration_metadata_import SET checkpoint_id=$3 WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(row.get::<Uuid,_>("id")).execute(&mut *conn).await?;
        save_counts(conn, j, &counts, true).await?;
        return s::settle(conn, j.org, j.id, token, added).await;
    }
    let row=sqlx::query("SELECT m.* FROM migration_metadata_manifest m WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.id>$3 ORDER BY m.id LIMIT 1").bind(j.plan).bind(j.org.0).bind(r.get::<Option<Uuid>,_>("checkpoint_id").unwrap_or(Uuid::nil())).fetch_optional(&mut *conn).await?;
    let Some(row) = row else {
        sqlx::query("UPDATE migration_metadata_import SET state='completed',phase='complete',completed_at=now(),updated_at=now(),cancel_reservation_token=NULL WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(&mut *conn).await?;
        s::release(conn, j.org, j.id, "cancel", 0).await?;
        return Ok(());
    };
    let unit: Uuid = row.get("id");
    let token = admission(conn, j, row.get::<i64, _>("added_byte_bound").max(4096)).await?;
    permit(conn, j, unit).await?;
    let mut manifest: Manifest = s::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        unit,
        "manifest",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let source_row = sqlx::query(
        "SELECT * FROM migration_metadata_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
    )
    .bind(row.get::<Uuid, _>("source_row_id"))
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    let source = source_record(key, j, &source_row)?;
    let source_id: String = row.get("source_id");
    let person: Option<Uuid> = row.get("person_id");
    let mut added = 0;
    if person.is_some() && parent_person(conn, j, &source_id).await?.map(|p| p.1) != person {
        return Err(MigrationError::InvalidImportChoice);
    }
    let mut counts = Counts::load(r.get("counts"))?;
    let mut local = Counts::default();
    for op in &mut manifest.operations {
        let old = op.disposition.clone();
        if matches!(old.as_str(), "eligible" | "already_present") {
            let person = person.ok_or(MigrationError::InvalidImportChoice)?;
            let mapping=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(op.mapping_id).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
            let mapping_data = decoded_mapping(key, j, &mapping)?;
            let t = mapping_data
                .target
                .as_ref()
                .ok_or(MigrationError::InvalidImportChoice)?;
            if !target_live(conn, j.org, t).await? {
                return Err(MigrationError::InvalidImportChoice);
            }
            if op.kind == "tag_link" {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_tag WHERE organization_id=$1 AND person_id=$2 AND tag_id=$3)").bind(j.org.0).bind(person).bind(op.target_id).fetch_one(&mut *conn).await?;
                if exists {
                    op.disposition = "already_present".into()
                } else if old == "already_present" {
                    return Err(MigrationError::InvalidImportChoice);
                } else {
                    let n: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM person_tag WHERE organization_id=$1 AND person_id=$2",
                    )
                    .bind(j.org.0)
                    .bind(person)
                    .fetch_one(&mut *conn)
                    .await?;
                    if n >= 20 {
                        return Err(MigrationError::InvalidImportChoice);
                    }
                    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) VALUES($1,$2,$3,$4)").bind(j.org.0).bind(person).bind(op.target_id).bind(j.actor.0).execute(&mut *conn).await?;
                    op.disposition = "applied".into();
                }
            } else {
                let v = op.value.as_ref().ok_or(MigrationError::SourceNotEligible)?;
                match value_state(
                    conn,
                    j.org,
                    person,
                    op.target_id.ok_or(MigrationError::SourceNotEligible)?,
                    v,
                )
                .await?
                {
                    Some(true) => op.disposition = "already_present".into(),
                    Some(false) => {
                        op.disposition = "held".into();
                        m::mark(&mut op.reasons, "local_value_conflict");
                        issues(conn, j, &["local_value_conflict".into()]).await?;
                    }
                    None => {
                        if old == "already_present" {
                            return Err(MigrationError::InvalidImportChoice);
                        }
                        let (text, number, date, option) = native_parts(v)?;
                        if let Some(option) = option {
                            let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_field_option WHERE id=$1 AND organization_id=$2 AND field_id=$3 AND archived_at IS NULL)").bind(option).bind(j.org.0).bind(op.target_id).fetch_one(&mut *conn).await?;
                            if !live {
                                return Err(MigrationError::InvalidImportChoice);
                            }
                        }
                        sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,number_value,date_value,option_id,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,$4,$5,CAST($6::text AS numeric),$7,$8,$9,'migration',$10)").bind(j.org.0).bind(person).bind(op.target_id).bind(&t.field_type).bind(text).bind(number).bind(date).bind(option).bind(j.actor.0).bind(j.id).execute(&mut *conn).await?;
                        op.disposition = "applied".into();
                    }
                }
            }
        }
        counts.settled(&op.kind, &old, &op.disposition);
        local.outcome(&op.kind, &op.disposition);
    }
    if row.get::<String, _>("disposition") == "eligible" {
        counts.people.settled += 1;
        local.people.eligible = 1;
        local.people.settled = 1;
    } else {
        local.people.excluded = 1;
    }
    local.people.source = 1;
    let outcome = if row.get::<String, _>("disposition") == "held"
        || !manifest.reasons.is_empty()
        || manifest.operations.iter().any(|o| o.disposition == "held")
    {
        "held"
    } else if manifest
        .operations
        .iter()
        .any(|o| o.disposition == "applied")
    {
        "applied"
    } else if manifest
        .operations
        .iter()
        .any(|o| o.disposition == "already_present")
    {
        "already_present"
    } else if manifest
        .operations
        .iter()
        .any(|o| o.disposition == "source_null")
        || matches!(&source.entity, Entity::Person(p) if p.tags_state == "source_null")
    {
        "source_null"
    } else {
        "not_supplied"
    };
    added += result(
        conn,
        key,
        j,
        "people",
        unit,
        Some(&source_id),
        person,
        outcome,
        local.wire(),
        &ResultData {
            source: source.provenance,
            operations: serde_json::to_value(&manifest.operations)
                .map_err(|_| MigrationError::Crypto)?,
            reasons: manifest.reasons,
        },
    )
    .await?;
    save_counts(conn, j, &counts, true).await?;
    sqlx::query(
        "UPDATE migration_metadata_import SET checkpoint_id=$3 WHERE id=$1 AND organization_id=$2",
    )
    .bind(j.id)
    .bind(j.org.0)
    .bind(unit)
    .execute(&mut *conn)
    .await?;
    s::settle(conn, j.org, j.id, token, added).await
}

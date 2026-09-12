//! Bounded retained-only activity preparation and execution. No FUB reader exists here.
use super::{
    activity::{self, Choice, Patch},
    activity_model::{self as m, CapturedRecord, Counts, Manifest, Mapping, ResultData},
    activity_source::{self as source, NativeActivity, Record},
    activity_store as s, crypto,
    snapshot::SnapshotPolicy,
    snapshot_source::{self, Cursor, Request, Stream},
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
                        tracing::warn!(outcome=%e,"Activity import unit failed");
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
#[tracing::instrument(name = "migration.activity_import.unit", skip_all)]
async fn run_once_inner(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    let c=sqlx::query("SELECT i.id,i.organization_id,i.executor_user_id FROM migration_activity_import i JOIN migration_activity_plan p ON p.id=i.latest_plan_id AND p.organization_id=i.organization_id WHERE i.state IN ('preparing','queued','running') AND ((i.state='preparing' AND p.state='building') OR i.state IN ('queued','running')) AND (i.lease_expires_at IS NULL OR i.lease_expires_at<=now()) ORDER BY i.created_at,i.id LIMIT 1").fetch_optional(pool).await?;
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
            "preparing" | "queued" | "running"
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
    if activity::validate_binding(&mut tx, &r).await.is_err() {
        pause(&mut tx, &j, "source_evidence_unavailable").await?;
        tx.commit().await?;
        return Ok(true);
    }
    sqlx::query("UPDATE migration_activity_import SET lease_token=$3,lease_expires_at=now()+interval '60 seconds',updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?;
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
            let count=sqlx::query("UPDATE migration_activity_import SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_expires_at>clock_timestamp()").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?.rows_affected();
            if count != 1 {
                return Err(MigrationError::ImportBusy);
            }
            tx.commit().await?;
            tracing::info!(organization_id=%j.org,import_id=%j.id,plan_id=%j.plan,phase=p.get::<String,_>("phase"),"Activity import unit committed");
        }
        Err(error) => {
            if let MigrationError::Database(ref e) = error {
                tracing::warn!(
                    sqlstate = e.as_database_error().and_then(|e| e.code()).as_deref(),
                    "Activity database unit failed"
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
    tracing::info!(organization_id=%j.org,import_id=%j.id,plan_id=%j.plan,pause_reason=reason,"Activity import paused");
    sqlx::query("UPDATE migration_activity_import SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(reason).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_activity_plan SET state=CASE WHEN state='building' THEN 'paused' ELSE state END,pause_reason=CASE WHEN state='building' THEN $3 ELSE pause_reason END WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(reason).execute(&mut *conn).await?;
    s::control(conn, j.org, j.id).await
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
    sqlx::query("UPDATE migration_activity_plan SET phase=$3,checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(next).execute(&mut *conn).await?;
    s::charge(conn, j.org, j.id, j.snapshot, j.plan, 0).await
}
async fn save_counts(
    conn: &mut PgConnection,
    j: &Job,
    counts: &Counts,
    execution: bool,
) -> Result<(), MigrationError> {
    sqlx::query(if execution {
        "UPDATE migration_activity_import SET counts=$3 WHERE id=$1 AND organization_id=$2"
    } else {
        "UPDATE migration_activity_plan SET counts=$3 WHERE id=$1 AND organization_id=$2"
    })
    .bind(if execution { j.id } else { j.plan })
    .bind(j.org.0)
    .bind(json!(counts))
    .execute(conn)
    .await?;
    Ok(())
}
async fn issues(conn: &mut PgConnection, j: &Job, codes: &[String]) -> Result<(), MigrationError> {
    for code in codes.iter().collect::<BTreeSet<_>>() {
        sqlx::query("INSERT INTO migration_activity_issue(plan_id,import_id,organization_id,code,record_count) VALUES($1,$2,$3,$4,1) ON CONFLICT(plan_id,organization_id,code) DO UPDATE SET record_count=migration_activity_issue.record_count+1").bind(j.plan).bind(j.id).bind(j.org.0).bind(code).execute(&mut *conn).await?;
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
        "manifests" => manifest(conn, key, j, p).await,
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
        sqlx::query("SELECT * FROM migration_activity_choice WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 50").bind(old).bind(j.org.0).bind(p.get::<Option<Uuid>,_>("checkpoint_id").unwrap_or(Uuid::nil())).fetch_all(&mut *conn).await?
    } else {
        vec![]
    };
    if rows.is_empty() {
        let predecessor = if let Some(old) = old {
            let ancestor = s::plan(conn, j.org, j.id, old).await?;
            if ancestor.get::<String, _>("phase") == "copying_choices" {
                ancestor.get::<Option<Uuid>, _>("parent_plan_id")
            } else {
                None
            }
        } else {
            None
        };
        if let Some(previous) = predecessor {
            sqlx::query("UPDATE migration_activity_plan SET inherit_plan_id=$3,checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(previous).execute(&mut *conn).await?;
        } else {
            phase(conn, j, "captures").await?
        }
    } else {
        for r in &rows {
            let kind: String = r.get("kind");
            let sk: Vec<u8> = r.get("source_key");
            let exists=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_activity_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4)").bind(j.plan).bind(j.org.0).bind(&kind).bind(&sk).fetch_one(&mut *conn).await?;
            if !exists {
                let choice = s::open(
                    key,
                    j.org,
                    j.snapshot,
                    old.unwrap(),
                    r.get("id"),
                    "choice",
                    r.get("nonce"),
                    r.get("ciphertext"),
                )?;
                added += activity::insert_choice(
                    conn,
                    key,
                    j.org,
                    j.id,
                    j.snapshot,
                    j.plan,
                    &Patch {
                        kind,
                        source_key: sk,
                        choice,
                    },
                )
                .await?;
            }
        }
        sqlx::query("UPDATE migration_activity_plan SET checkpoint_id=$3 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(rows.last().unwrap().get::<Uuid,_>("id")).execute(&mut *conn).await?;
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
        r.get("nonce"),
        r.get("ciphertext"),
    )
}

async fn collection_cursor(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    c: &sqlx::postgres::PgRow,
    stream: Stream,
) -> Result<Cursor, MigrationError> {
    let checkpoint: i64 = c.get("checkpoint");
    if checkpoint == 0 {
        return Ok(Cursor::default());
    }
    if checkpoint < 0 {
        return Err(MigrationError::SourceNotEligible);
    }
    // A successful nonterminal page always has at least one record. Read only
    // its first authenticated child observation, not the whole capture history.
    // Rejected attempts never advance the source checkpoint or this cursor.
    let previous=sqlx::query("SELECT a.* FROM migration_snapshot_capture c JOIN migration_activity_source a ON a.capture_id=c.id AND a.snapshot_id=c.snapshot_id AND a.organization_id=c.organization_id AND a.plan_id=$4 AND a.ordinal=0 WHERE c.snapshot_id=$1 AND c.organization_id=$2 AND c.stream=$3 AND c.checkpoint=$5 AND c.accepted AND c.sequence<$6 LIMIT 1").bind(j.snapshot).bind(j.org.0).bind(stream.as_str()).bind(j.plan).bind(checkpoint-1).bind(c.get::<i64,_>("sequence")).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let progress: CapturedRecord = s::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        previous.get("id"),
        "source",
        previous.get("nonce"),
        previous.get("ciphertext"),
    )?;
    if !progress.capture_accepted || progress.record.stream != stream {
        return Err(MigrationError::SourceNotEligible);
    }
    progress
        .next_cursor
        .ok_or(MigrationError::SourceNotEligible)
}

fn request_fingerprint(
    key: &RawPayloadKey,
    j: &Job,
    request: &Request,
) -> Result<[u8; 32], MigrationError> {
    request
        .path()
        .map_err(|_| MigrationError::SourceNotEligible)?;
    Ok(crypto::snapshot_hmac(
        key,
        j.org,
        "request",
        &s::bytes(&json!({
            "stream":request.stream.as_str(),"offset":request.cursor.offset,
            "next":request.cursor.next,"source_id":request.source_id,
        }))?,
    ))
}

async fn capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let c=sqlx::query("SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 AND stream IN ('notes','note_detail','tasks_open','tasks_completed','users') ORDER BY sequence LIMIT 1").bind(j.snapshot).bind(j.org.0).bind(p.get::<i64,_>("checkpoint_capture")).bind(j.boundary).fetch_optional(&mut *conn).await?;
    let Some(c) = c else {
        return phase(conn, j, "manifests").await;
    };
    let token = admission(conn, j, s::UNIT).await?;
    let capture: Uuid = c.get("id");
    let stream =
        Stream::parse(&c.get::<String, _>("stream")).ok_or(MigrationError::SourceNotEligible)?;
    let raw = crypto::open_snapshot(
        key,
        j.org,
        j.snapshot,
        capture,
        "capture",
        c.get("nonce"),
        c.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if c.get::<i64, _>("raw_byte_len") != raw.len() as i64
        || c.get::<String, _>("representation") != stream.representation()
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let mut added = 0;
    let accepted: bool = c.get("accepted");
    let cursor = if stream == Stream::NoteDetail {
        Cursor::default()
    } else {
        let cursor = collection_cursor(conn, key, j, &c, stream).await?;
        let request = Request {
            stream,
            cursor: cursor.clone(),
            source_id: None,
        };
        if c.get::<Vec<u8>, _>("request_fingerprint") != request_fingerprint(key, j, &request)? {
            return Err(MigrationError::SourceNotEligible);
        }
        cursor
    };
    let negative = stream == Stream::NoteDetail
        && c.get::<i32, _>("http_status") == 404
        && c.get::<String, _>("classification") == "content_inaccessible";
    let linked=sqlx::query("SELECT * FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101").bind(capture).bind(j.snapshot).bind(j.org.0).fetch_all(&mut *conn).await?;
    if negative {
        if !accepted || c.get::<bool, _>("truncated") || !linked.is_empty() {
            return Err(MigrationError::SourceNotEligible);
        }
        let record = Record {
            source_id: None,
            person_id: None,
            canonical: vec![],
            stream,
            roles: vec![],
            source_type: None,
            reasons: vec!["content_inaccessible".into()],
            source_only: BTreeMap::new(),
            provenance: BTreeMap::from([("restricted_capture".into(), capture.to_string())]),
        };
        added += insert_source(
            conn,
            key,
            j,
            &c,
            None,
            -1,
            &record,
            &c.get::<Vec<u8>, _>("request_fingerprint"),
            true,
            cursor,
            None,
        )
        .await?;
    } else if accepted || !linked.is_empty() {
        if c.get::<bool, _>("truncated") || !(200..300).contains(&c.get::<i32, _>("http_status")) {
            return Err(MigrationError::SourceNotEligible);
        }
        let parsed =
            source::extract_page(stream, &raw).map_err(|_| MigrationError::SourceNotEligible)?;
        if parsed.len() != linked.len() || linked.len() > 100 {
            return Err(MigrationError::SourceNotEligible);
        }
        let request = Request {
            stream,
            cursor: cursor.clone(),
            source_id: if stream == Stream::NoteDetail {
                parsed.first().and_then(|r| r.source_id.clone())
            } else {
                None
            },
        };
        if c.get::<Vec<u8>, _>("request_fingerprint") != request_fingerprint(key, j, &request)? {
            return Err(MigrationError::SourceNotEligible);
        }
        let qualified = snapshot_source::parse(&request, &raw)
            .map_err(|_| MigrationError::SourceNotEligible)?;
        let invalid = parsed.iter().any(|r| r.source_id.is_none());
        let classification: String = c.get("classification");
        let valid_classification = if accepted {
            classification == "success" && !invalid
        } else {
            match classification.as_str() {
                "invalid_source_id" => invalid,
                "pagination_no_progress" | "pagination_loop" => c.get::<i64, _>("checkpoint") > 0,
                _ => false,
            }
        };
        if !valid_classification || qualified.records.len() != parsed.len() {
            return Err(MigrationError::SourceNotEligible);
        }
        for (ordinal, (mut item, r)) in parsed.into_iter().zip(&linked).enumerate() {
            let semantic = crypto::snapshot_hmac(
                key,
                j.org,
                &format!("semantic:{}", stream.representation()),
                &item.canonical,
            );
            if r.get::<i32, _>("ordinal") != ordinal as i32
                || r.get::<Option<String>, _>("source_id") != item.source_id
                || r.get::<String, _>("family") != stream.family().as_str()
                || r.get::<String, _>("representation") != stream.representation()
                || r.get::<Vec<u8>, _>("semantic_hmac") != semantic
                || r.get::<i64, _>("capture_sequence") != c.get::<i64, _>("sequence")
                || qualified.records[ordinal].canonical != item.canonical
            {
                return Err(MigrationError::SourceNotEligible);
            }
            if !accepted {
                m::mark(&mut item.reasons, "capture_not_accepted");
            }
            item.canonical.clear();
            added += insert_source(
                conn,
                key,
                j,
                &c,
                Some(r.get("id")),
                ordinal as i32,
                &item,
                &semantic,
                false,
                cursor.clone(),
                if accepted {
                    qualified.next.clone()
                } else {
                    None
                },
            )
            .await?;
        }
    }
    sqlx::query("UPDATE migration_activity_plan SET checkpoint_capture=$3 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(c.get::<i64,_>("sequence")).execute(&mut *conn).await?;
    s::settle(conn, j.org, j.id, token, added).await
}
#[allow(clippy::too_many_arguments)]
async fn insert_source(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    c: &sqlx::postgres::PgRow,
    record_id: Option<Uuid>,
    ordinal: i32,
    item: &Record,
    semantic: &[u8],
    negative: bool,
    request_cursor: Cursor,
    next_cursor: Option<Cursor>,
) -> Result<i64, MigrationError> {
    let id = Uuid::new_v4();
    let counters = source::preview(item, None).source_only;
    let payload = CapturedRecord {
        record: item.clone(),
        request_cursor,
        next_cursor,
        capture_accepted: c.get("accepted"),
    };
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "source", &payload)?;
    let size = s::sealed_bytes(&a)
        + item.source_id.as_ref().map_or(0, |v| v.len() as i64)
        + item.stream.as_str().len() as i64
        + item.stream.representation().len() as i64
        + 32;
    sqlx::query("INSERT INTO migration_activity_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,negative,nonce,ciphertext,source_only_counts) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)").bind(id).bind(j.plan).bind(j.id).bind(j.snapshot).bind(j.org.0).bind(item.stream.family().as_str()).bind(&item.source_id).bind(record_id).bind(c.get::<Uuid,_>("id")).bind(c.get::<i64,_>("sequence")).bind(ordinal).bind(item.stream.as_str()).bind(item.stream.representation()).bind(semantic).bind(negative).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(json!(counters)).execute(conn).await?;
    Ok(size)
}
async fn parent_person(
    conn: &mut PgConnection,
    j: &Job,
    source_id: &str,
) -> Result<Option<(Uuid, Uuid)>, MigrationError> {
    let r=sqlx::query("SELECT r.id,r.person_id FROM migration_import_identity i JOIN migration_import_result r ON r.import_id=i.import_id AND r.plan_id=i.plan_id AND r.organization_id=i.organization_id AND r.source_id=i.source_id AND r.person_id=i.target_id JOIN person p ON p.id=r.person_id AND p.organization_id=r.organization_id WHERE i.organization_id=$1 AND i.source_account_id=$2 AND i.family='people' AND i.source_id=$3 AND i.import_id=$4 AND i.plan_id=$5 AND r.disposition IN ('imported','already_imported')").bind(j.org.0).bind(j.account).bind(source_id).bind(j.parent).bind(j.parent_plan).fetch_optional(conn).await?;
    Ok(r.map(|r| (r.get("id"), r.get("person_id"))))
}
async fn mapping(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    role: &str,
    value: &str,
) -> Result<(Choice, i64), MigrationError> {
    let sk = s::source_key(key, j.org, j.account, role, value.as_bytes());
    if let Some(r)=sqlx::query("SELECT * FROM migration_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(j.plan).bind(j.org.0).bind(role).bind(&sk).fetch_optional(&mut *conn).await?{let data:Mapping=s::open(key,j.org,j.snapshot,j.plan,r.get("id"),"mapping",r.get("nonce"),r.get("ciphertext"))?;sqlx::query("UPDATE migration_activity_mapping SET dependent_count=dependent_count+1 WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("id")).bind(j.org.0).execute(conn).await?;return Ok((data.choice,0))}
    let choice=if let Some(c)=sqlx::query("SELECT * FROM migration_activity_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(j.plan).bind(j.org.0).bind(role).bind(&sk).fetch_optional(&mut *conn).await?{s::open(key,j.org,j.snapshot,j.plan,c.get("id"),"choice",c.get("nonce"),c.get("ciphertext"))?}else{Choice::Hold};
    let id = Uuid::new_v4();
    let data = Mapping {
        source_value: value.into(),
        role: role.into(),
        choice: choice.clone(),
        suggestions: vec![],
        suggested_kind: if role == "task_kind" {
            source::suggested_kind(value).map(|k| k.as_str().into())
        } else {
            None
        },
        source: BTreeMap::from([("source_id_or_type".into(), value.into())]),
    };
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "mapping", &data)?;
    let size = s::sealed_bytes(&a) + 32;
    sqlx::query("INSERT INTO migration_activity_mapping(id,plan_id,import_id,organization_id,kind,source_key,dependent_count,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,1,$7,$8)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(role).bind(sk).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(conn).await?;
    Ok((choice, size))
}
async fn manifest(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let row=sqlx::query("SELECT * FROM migration_activity_source a WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND family IN ('notes','tasks') AND NOT negative ORDER BY id LIMIT 1").bind(j.plan).bind(j.org.0).bind(p.get::<Option<Uuid>,_>("checkpoint_id").unwrap_or(Uuid::nil())).fetch_optional(&mut *conn).await?;
    let Some(row) = row else {
        let digest = crypto::snapshot_hmac(
            key,
            j.org,
            "activity-plan",
            &s::bytes(
                &json!({"plan":j.plan,"counts":p.get::<Value,_>("counts"),"capture_sequence":j.boundary}),
            )?,
        );
        sqlx::query("UPDATE migration_activity_plan SET state='ready',phase='ready',confirmation_digest=$3,expires_at=now()+interval '30 minutes',completed_at=now() WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(digest.as_slice()).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_activity_import SET state='ready',revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(&mut *conn).await?;
        s::charge(conn, j.org, j.id, j.snapshot, j.plan, 32).await?;
        return Ok(());
    };
    let rowid: Uuid = row.get("id");
    let sid: Option<String> = row.get("source_id");
    let family: String = row.get("family");
    let kind = if family == "notes" { "note" } else { "task" };
    let exists = if let Some(sid) = &sid {
        sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_activity_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_id=$4)").bind(j.plan).bind(j.org.0).bind(kind).bind(sid).fetch_one(&mut *conn).await?
    } else {
        false
    };
    if exists {
        sqlx::query("UPDATE migration_activity_plan SET checkpoint_id=$3 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(rowid).execute(conn).await?;
        return Ok(());
    }
    let token = admission(conn, j, s::UNIT).await?;
    let mut added = 0;
    let mut chosen = row;
    let mut reasons = vec![];
    let mut observations = 1;
    let mut source_only = None;
    if let Some(sid) = &sid {
        let group=sqlx::query("SELECT representation,count(DISTINCT semantic_hmac) AS variants,count(DISTINCT stream) AS streams,count(*) AS observations FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4 GROUP BY representation ORDER BY representation LIMIT 3").bind(j.plan).bind(j.org.0).bind(&family).bind(sid).fetch_all(&mut *conn).await?;
        observations = group.iter().map(|r| r.get::<i64, _>("observations")).sum();
        if group.iter().any(|r| {
            r.get::<i64, _>("variants") > 1 || (kind == "task" && r.get::<i64, _>("streams") > 1)
        }) {
            m::mark(&mut reasons, "source_variants")
        }
        // Counts describe excluded components in distinct observations. Exact
        // duplicates collapse within a representation; complementary note list
        // and detail each contribute, even when both contain a similar property.
        // They do not assert a globally unique attachment/reply identity.
        let counters=sqlx::query("WITH observations AS (SELECT DISTINCT ON (representation,semantic_hmac) source_only_counts FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4 ORDER BY representation,semantic_hmac,id) SELECT e.key AS code,sum((e.value #>> '{}')::bigint)::bigint AS count FROM observations CROSS JOIN LATERAL jsonb_each(source_only_counts) e GROUP BY e.key ORDER BY e.key").bind(j.plan).bind(j.org.0).bind(&family).bind(sid).fetch_all(&mut *conn).await?;
        source_only = Some(
            counters
                .into_iter()
                .map(|r| {
                    let count = u64::try_from(r.get::<i64, _>("count"))
                        .map_err(|_| MigrationError::SourceNotEligible)?;
                    Ok((r.get::<String, _>("code"), count))
                })
                .collect::<Result<BTreeMap<_, _>, MigrationError>>()?,
        );
        if kind == "note" {
            let detail=sqlx::query("SELECT a.* FROM migration_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='notes' AND a.source_id=$3 AND a.stream='note_detail' AND NOT a.negative ORDER BY c.accepted DESC,a.id LIMIT 1").bind(j.plan).bind(j.org.0).bind(sid).fetch_optional(&mut *conn).await?;
            if let Some(detail) = detail {
                chosen = detail
            } else {
                let request = crypto::snapshot_hmac(
                    key,
                    j.org,
                    "request",
                    &s::bytes(
                        &json!({"stream":"note_detail","offset":0,"next":null,"source_id":sid}),
                    )?,
                );
                let inaccessible=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND negative AND semantic_hmac=$3)").bind(j.plan).bind(j.org.0).bind(request.as_slice()).fetch_one(&mut *conn).await?;
                m::mark(
                    &mut reasons,
                    if inaccessible {
                        "content_inaccessible"
                    } else {
                        "note_detail_unavailable"
                    },
                );
            }
        } else {
            // Rejected observations still participate in variant detection, but
            // only an accepted observation can supply an executable task.
            chosen=sqlx::query("SELECT a.* FROM migration_activity_source a JOIN migration_snapshot_capture c ON c.id=a.capture_id AND c.snapshot_id=a.snapshot_id AND c.organization_id=a.organization_id WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.family='tasks' AND a.source_id=$3 AND NOT a.negative ORDER BY c.accepted DESC,a.id LIMIT 1").bind(j.plan).bind(j.org.0).bind(sid).fetch_one(&mut *conn).await?;
        }
    }
    let record = source_record(key, j, &chosen)?;
    let r = s::run(conn, j.org, j.id).await?;
    let dst = activity::decode_destination(key, &r, p)?;
    let mut preview = source::preview(&record, dst.source_timezone.as_deref());
    if let Some(source_only) = source_only {
        preview.source_only = source_only;
    }
    for code in &preview.reasons {
        m::mark(&mut reasons, code)
    }
    if kind == "task"
        && dst.source_timezone.is_some()
        && (record
            .provenance
            .get("dueDate")
            .is_some_and(|v| v != "null"))
    {
        if let Some(assignee) = record.roles.iter().find(|r| r.role == "task_assignee") {
            let users=sqlx::query("SELECT * FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND family='users' AND source_id=$3 ORDER BY id LIMIT 101").bind(j.plan).bind(j.org.0).bind(&assignee.source_id).fetch_all(&mut *conn).await?;
            if users.len() > 100 {
                m::mark(&mut reasons, "source_user_evidence_ambiguous")
            }
            for u in users {
                let evidence = source_record(key, j, &u)?;
                for field in ["timezone", "timeZone"] {
                    if let Some(v) = evidence.provenance.get(field) {
                        let zone = serde_json::from_str::<String>(v).ok();
                        if zone
                            .as_deref()
                            .is_some_and(|z| Some(z) != dst.source_timezone.as_deref())
                        {
                            m::mark(&mut reasons, "source_timezone_conflict")
                        }
                    }
                }
            }
        }
    }
    let parent = if let Some(person) = &record.person_id {
        parent_person(conn, j, person).await?
    } else {
        None
    };
    if parent.is_none() {
        m::mark(&mut reasons, "parent_person_unavailable")
    }
    let mut author = None;
    let mut creator = None;
    let mut assignee = None;
    let mut native_kind = None;
    for role in &record.roles {
        let (choice, size) = mapping(conn, key, j, &role.role, &role.source_id).await?;
        added += size;
        let target = match &choice {
            Choice::LeaveUnmapped => None,
            Choice::MapExisting { target_id } => {
                if activity::validate_choice(conn, j.org, &role.role, &choice)
                    .await
                    .is_err()
                {
                    m::mark(&mut reasons, "mapping_target_changed");
                    None
                } else {
                    Some(*target_id)
                }
            }
            _ => {
                m::mark(&mut reasons, "user_mapping_required");
                None
            }
        };
        match role.role.as_str() {
            "note_author" => author = target,
            "task_creator" => creator = target,
            "task_assignee" => assignee = target,
            _ => return Err(MigrationError::SourceNotEligible),
        }
    }
    if let Some(t) = record.source_type.as_ref().filter(|_| kind == "task") {
        let (choice, size) = mapping(conn, key, j, "task_kind", t).await?;
        added += size;
        match choice {
            Choice::MapKind { native_kind: ref k }
                if crate::domain::task::TaskKind::from_db_str(k).is_some() =>
            {
                native_kind = Some(k.clone())
            }
            _ => m::mark(&mut reasons, "task_kind_mapping_required"),
        }
    }
    if kind == "task" && native_kind.is_none() {
        m::mark(&mut reasons, "task_kind_mapping_required")
    }
    let target = Uuid::new_v4();
    let native_key = sid.as_ref().map(|sid| format!("v1:{}:{sid}", j.account));
    let source_only_count = preview
        .source_only
        .values()
        .try_fold(0i64, |a, n| a.checked_add(i64::try_from(*n).ok()?))
        .ok_or(MigrationError::StorageLimit)?;
    preview.reasons = reasons.clone();
    let mut data = Manifest {
        record,
        preview: preview.clone(),
        native: preview.native.clone(),
        native_kind,
        reasons,
        source_only_count,
        source_observations: observations,
    };
    let mut disposition = if data.reasons.is_empty() && data.native.is_some() {
        "eligible"
    } else {
        "held"
    };
    let mut target_id = Some(target);
    if disposition == "eligible" {
        if let (Some((_, person)), Some(sid), Some(native_key)) =
            (parent, sid.as_deref(), native_key.as_deref())
        {
            let outcome = native_state(
                conn, j, kind, sid, native_key, person, target, &data, author, creator, assignee,
            )
            .await?;
            match outcome {
                NativeState::New => {}
                NativeState::Equal(id) => {
                    disposition = "already_present";
                    target_id = Some(id)
                }
                NativeState::Held(reason) => {
                    disposition = "held";
                    m::mark(&mut data.reasons, reason);
                    target_id = None
                }
            }
        }
    }
    let id = Uuid::new_v4();
    let a = s::seal(key, j.org, j.snapshot, j.plan, id, "manifest", &data)?;
    let bound = (s::sealed_bytes(&a) * 2 + 16384).min(s::UNIT);
    added += s::sealed_bytes(&a)
        + sid.as_ref().map_or(0, |v| v.len() as i64)
        + data.record.person_id.as_ref().map_or(0, |v| v.len() as i64)
        + native_key.as_ref().map_or(0, |v| v.len() as i64);
    sqlx::query("INSERT INTO migration_activity_manifest(id,plan_id,import_id,organization_id,kind,source_id,source_row_id,source_person_id,parent_result_id,person_id,target_id,native_source_key,author_user_id,creator_user_id,assignee_user_id,disposition,added_byte_bound,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(kind).bind(&sid).bind(chosen.get::<Uuid,_>("id")).bind(&data.record.person_id).bind(parent.map(|v|v.0)).bind(parent.map(|v|v.1)).bind(target_id).bind(&native_key).bind(author).bind(creator).bind(assignee).bind(disposition).bind(bound).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(&mut *conn).await?;
    let mut counts = Counts::load(p.get("counts"))?;
    counts.planned(kind, disposition, source_only_count);
    if sid.is_none() {
        counts.invalid_occurrences += 1
    }
    if data.reasons.iter().any(|v| {
        matches!(
            v.as_str(),
            "content_inaccessible" | "note_detail_unavailable"
        )
    }) {
        counts.unavailable_bodies += 1
    }
    save_counts(conn, j, &counts, false).await?;
    issues(conn, j, &data.reasons).await?;
    for code in &data.reasons {
        sqlx::query("INSERT INTO migration_activity_manifest_issue(manifest_id,plan_id,import_id,organization_id,code) VALUES($1,$2,$3,$4,$5)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(code).execute(&mut *conn).await?;
        added += code.len() as i64;
    }
    sqlx::query("UPDATE migration_activity_plan SET checkpoint_id=$3,max_added_byte_bound=GREATEST(max_added_byte_bound,$4) WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(rowid).bind(bound).execute(&mut *conn).await?;
    s::settle(conn, j.org, j.id, token, added).await
}
enum NativeState {
    New,
    Equal(Uuid),
    Held(&'static str),
}
#[allow(clippy::too_many_arguments)]
async fn native_state(
    conn: &mut PgConnection,
    j: &Job,
    kind: &str,
    sid: &str,
    native_key: &str,
    person: Uuid,
    target: Uuid,
    data: &Manifest,
    author: Option<Uuid>,
    creator: Option<Uuid>,
    assignee: Option<Uuid>,
) -> Result<NativeState, MigrationError> {
    let identity=sqlx::query("SELECT target_id FROM migration_activity_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_id=$4").bind(j.org.0).bind(j.account).bind(kind).bind(sid).fetch_optional(&mut *conn).await?;
    let (lookup,legacy)=match kind{"note"=>("SELECT * FROM note WHERE organization_id=$1 AND source='fub' AND source_external_id=$2 FOR UPDATE","SELECT EXISTS(SELECT 1 FROM note WHERE organization_id=$1 AND source='fub' AND source_external_id=$2)"),"task"=>("SELECT * FROM task WHERE organization_id=$1 AND source='fub' AND source_external_id=$2 FOR UPDATE","SELECT EXISTS(SELECT 1 FROM task WHERE organization_id=$1 AND source='fub' AND source_external_id=$2)"),_=>return Err(MigrationError::SourceNotEligible)};
    if sqlx::query_scalar::<_, bool>(legacy)
        .bind(j.org.0)
        .bind(sid)
        .fetch_one(&mut *conn)
        .await?
    {
        return Ok(NativeState::Held("legacy_source_binding"));
    }
    let row = sqlx::query(lookup)
        .bind(j.org.0)
        .bind(native_key)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(r) = row else {
        if identity.is_some() {
            return Ok(NativeState::Held("identity_target_missing"));
        }
        let collision = sqlx::query_scalar::<_, bool>(if kind == "note" {
            "SELECT EXISTS(SELECT 1 FROM note WHERE id=$1 AND organization_id=$2)"
        } else {
            "SELECT EXISTS(SELECT 1 FROM task WHERE id=$1 AND organization_id=$2)"
        })
        .bind(target)
        .bind(j.org.0)
        .fetch_one(conn)
        .await?;
        return Ok(if collision {
            NativeState::Held("native_target_collision")
        } else {
            NativeState::New
        });
    };
    let id: Uuid = r.get("id");
    if identity.is_some_and(|v| v.get::<Uuid, _>("target_id") != id) {
        return Ok(NativeState::Held("identity_target_changed"));
    }
    if r.get::<Option<chrono::DateTime<Utc>>, _>("deleted_at")
        .is_some()
    {
        return Ok(NativeState::Held("native_tombstone"));
    }
    if r.get::<Uuid, _>("person_id") != person || r.get::<String, _>("origin") != "migration" {
        return Ok(NativeState::Held("native_binding_conflict"));
    }
    let equal = match &data.native {
        Some(NativeActivity::Note {
            body,
            created_at,
            updated_at,
            ..
        }) if kind == "note" => {
            r.get::<String, _>("body") == *body
                && r.get::<Option<Uuid>, _>("author_user_id") == author
                && r.get::<chrono::DateTime<Utc>, _>("created_at") == *created_at
                && r.get::<chrono::DateTime<Utc>, _>("updated_at") == *updated_at
        }
        Some(NativeActivity::Task {
            title,
            created_at,
            updated_at,
            due_at,
            completed_at,
            ..
        }) if kind == "task" => {
            r.get::<String, _>("title") == *title
                && Some(r.get::<String, _>("kind")) == data.native_kind
                && r.get::<Option<Uuid>, _>("created_by_user_id") == creator
                && r.get::<Option<Uuid>, _>("assignee_user_id") == assignee
                && r.get::<Option<Uuid>, _>("completed_by_user_id").is_none()
                && r.get::<chrono::DateTime<Utc>, _>("created_at") == *created_at
                && r.get::<chrono::DateTime<Utc>, _>("updated_at") == *updated_at
                && r.get::<Option<chrono::DateTime<Utc>>, _>("due_at") == *due_at
                && r.get::<Option<chrono::DateTime<Utc>>, _>("completed_at") == *completed_at
        }
        _ => false,
    };
    Ok(if equal {
        NativeState::Equal(id)
    } else {
        NativeState::Held("native_local_changes")
    })
}
/// Only a confirmed frozen manifest constructs these private typed commands.
struct ImportRetainedNote<'a> {
    target: Uuid,
    person: Uuid,
    source_key: &'a str,
    author: Option<Uuid>,
    body: &'a str,
    created: chrono::DateTime<Utc>,
    updated: chrono::DateTime<Utc>,
}
struct ImportRetainedTask<'a> {
    target: Uuid,
    person: Uuid,
    source_key: &'a str,
    creator: Option<Uuid>,
    assignee: Option<Uuid>,
    title: &'a str,
    kind: &'a str,
    created: chrono::DateTime<Utc>,
    updated: chrono::DateTime<Utc>,
    due: Option<chrono::DateTime<Utc>>,
    completed: Option<chrono::DateTime<Utc>>,
}
async fn import_note(
    conn: &mut PgConnection,
    j: &Job,
    cmd: ImportRetainedNote<'_>,
) -> Result<i64, MigrationError> {
    if crate::domain::note::NoteBody::parse(cmd.body)
        .ok()
        .as_deref()
        != Some(cmd.body)
    {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("INSERT INTO note(id,organization_id,person_id,author_user_id,body,origin,correlation_id,source,source_external_id,created_at,updated_at) VALUES($1,$2,$3,$4,$5,'migration',$6,'fub',$7,$8,$9)").bind(cmd.target).bind(j.org.0).bind(cmd.person).bind(cmd.author).bind(cmd.body).bind(j.id).bind(cmd.source_key).bind(cmd.created).bind(cmd.updated).execute(&mut *conn).await?;
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT pg_column_size(n) FROM note n WHERE id=$1 AND organization_id=$2",
    )
    .bind(cmd.target)
    .bind(j.org.0)
    .fetch_one(conn)
    .await? as i64)
}
async fn import_task(
    conn: &mut PgConnection,
    j: &Job,
    cmd: ImportRetainedTask<'_>,
) -> Result<i64, MigrationError> {
    if crate::domain::task::TaskTitle::parse(cmd.title)
        .ok()
        .as_deref()
        != Some(cmd.title)
        || crate::domain::task::TaskKind::from_db_str(cmd.kind).is_none()
    {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,completed_at,completed_by_user_id,origin,correlation_id,source,source_external_id,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,NULL,'migration',$10,'fub',$11,$12,$13)").bind(cmd.target).bind(j.org.0).bind(cmd.person).bind(cmd.title).bind(cmd.kind).bind(cmd.due).bind(cmd.assignee).bind(cmd.creator).bind(cmd.completed).bind(j.id).bind(cmd.source_key).bind(cmd.created).bind(cmd.updated).execute(&mut *conn).await?;
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT pg_column_size(t) FROM task t WHERE id=$1 AND organization_id=$2",
    )
    .bind(cmd.target)
    .bind(j.org.0)
    .fetch_one(conn)
    .await? as i64)
}
pub(crate) async fn validate_choices(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let org = OrganizationId::new(r.get("organization_id"));
    let plan: Uuid = p.get("id");
    let mut after = Uuid::nil();
    loop {
        let rows=sqlx::query("SELECT * FROM migration_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 50").bind(plan).bind(org.0).bind(after).fetch_all(&mut *conn).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let m = activity::mapping_data(key, r, plan, &row)?;
            activity::validate_choice(conn, org, &m.role, &m.choice).await?;
            after = row.get("id");
        }
    }
    Ok(())
}
async fn execute(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let row=sqlx::query("SELECT * FROM migration_activity_manifest WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 1").bind(j.plan).bind(j.org.0).bind(r.get::<Option<Uuid>,_>("checkpoint_id").unwrap_or(Uuid::nil())).fetch_optional(&mut *conn).await?;
    let Some(row) = row else {
        sqlx::query("UPDATE migration_activity_import SET state='completed',phase='complete',completed_at=now(),updated_at=now(),revision=revision+1,cancel_reservation_token=NULL WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(&mut *conn).await?;
        s::release(conn, j.org, j.id, "cancel", 0).await?;
        return Ok(());
    };
    let unit: Uuid = row.get("id");
    let token = admission(conn, j, row.get("added_byte_bound")).await?;
    let mut data: Manifest = s::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        unit,
        "manifest",
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let kind: String = row.get("kind");
    let old: String = row.get("disposition");
    let mut outcome = "held";
    let mut target = None;
    let mut native_bytes = 0;
    sqlx::query(
        "UPDATE migration_activity_import SET state='running' WHERE id=$1 AND organization_id=$2",
    )
    .bind(j.id)
    .bind(j.org.0)
    .execute(&mut *conn)
    .await?;
    if old != "held" {
        let current_parent = if let Some(sid) = &data.record.person_id {
            parent_person(conn, j, sid).await?
        } else {
            None
        };
        let frozen_parent = row
            .get::<Option<Uuid>, _>("person_id")
            .zip(row.get::<Option<Uuid>, _>("parent_result_id"));
        if current_parent.map(|v| (v.1, v.0)) != frozen_parent || current_parent.is_none() {
            m::mark(&mut data.reasons, "parent_person_unavailable")
        } else {
            let person = current_parent.ok_or(MigrationError::SourceNotEligible)?.1;
            let author: Option<Uuid> = row.get("author_user_id");
            let creator: Option<Uuid> = row.get("creator_user_id");
            let assignee: Option<Uuid> = row.get("assignee_user_id");
            let mut actors = [author, creator, assignee]
                .into_iter()
                .flatten()
                .collect::<BTreeSet<_>>();
            let mut actors_valid = true;
            for actor in std::mem::take(&mut actors) {
                let member=sqlx::query("SELECT status FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR SHARE").bind(j.org.0).bind(actor).fetch_optional(&mut *conn).await?;
                if member.is_none()
                    || (assignee == Some(actor)
                        && member.is_some_and(|v| v.get::<String, _>("status") != "active"))
                {
                    actors_valid = false
                }
            }
            if !actors_valid {
                m::mark(&mut data.reasons, "mapping_target_changed")
            } else {
                sqlx::query("SELECT id FROM person WHERE id=$1 AND organization_id=$2 FOR UPDATE")
                    .bind(person)
                    .bind(j.org.0)
                    .fetch_optional(&mut *conn)
                    .await?
                    .ok_or(MigrationError::SourceNotEligible)?;
                let sid = data
                    .record
                    .source_id
                    .as_deref()
                    .ok_or(MigrationError::SourceNotEligible)?;
                let native_key: String = row.get("native_source_key");
                let planned: Uuid = row.get("target_id");
                match native_state(
                    conn,
                    j,
                    &kind,
                    sid,
                    &native_key,
                    person,
                    planned,
                    &data,
                    author,
                    creator,
                    assignee,
                )
                .await?
                {
                    NativeState::Held(reason) => m::mark(&mut data.reasons, reason),
                    NativeState::Equal(id) => {
                        outcome = "already_present";
                        target = Some(id)
                    }
                    NativeState::New if old == "eligible" => {
                        sqlx::query("SELECT set_config('crm.activity_token',$1,true),set_config('crm.activity_unit',$2,true)").bind(j.token.to_string()).bind(unit.to_string()).execute(&mut *conn).await?;
                        native_bytes = match data
                            .native
                            .as_ref()
                            .ok_or(MigrationError::SourceNotEligible)?
                        {
                            NativeActivity::Note {
                                body,
                                created_at,
                                updated_at,
                                ..
                            } => {
                                import_note(
                                    conn,
                                    j,
                                    ImportRetainedNote {
                                        target: planned,
                                        person,
                                        source_key: &native_key,
                                        author,
                                        body,
                                        created: *created_at,
                                        updated: *updated_at,
                                    },
                                )
                                .await?
                            }
                            NativeActivity::Task {
                                title,
                                created_at,
                                updated_at,
                                due_at,
                                completed_at,
                                ..
                            } => {
                                import_task(
                                    conn,
                                    j,
                                    ImportRetainedTask {
                                        target: planned,
                                        person,
                                        source_key: &native_key,
                                        creator,
                                        assignee,
                                        title,
                                        kind: data
                                            .native_kind
                                            .as_deref()
                                            .ok_or(MigrationError::SourceNotEligible)?,
                                        created: *created_at,
                                        updated: *updated_at,
                                        due: *due_at,
                                        completed: *completed_at,
                                    },
                                )
                                .await?
                            }
                        };
                        outcome = "applied";
                        target = Some(planned)
                    }
                    NativeState::New => m::mark(&mut data.reasons, "identity_target_missing"),
                }
                if let Some(target) = target {
                    let inserted=sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,import_id,plan_id,manifest_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(organization_id,source_account_id,kind,source_id) DO NOTHING").bind(j.org.0).bind(j.account).bind(&kind).bind(sid).bind(target).bind(j.id).bind(j.plan).bind(unit).execute(&mut *conn).await?.rows_affected();
                    let _ = inserted;
                }
            }
        }
    }
    let result = Uuid::new_v4();
    let payload = ResultData {
        source: data.record.provenance.clone(),
        native: json!({"target_id":target,"native_kind":data.native_kind,"preview":data.native}),
        reasons: data.reasons.clone(),
        transformations: data.preview.transformations.clone(),
        source_only: data.preview.source_only.clone(),
    };
    let a = s::seal(key, j.org, j.snapshot, j.plan, result, "result", &payload)?;
    let mut counts = Counts::load(r.get("counts"))?;
    counts.settled(&kind, &old, outcome);
    save_counts(conn, j, &counts, true).await?;
    sqlx::query("UPDATE migration_activity_import SET checkpoint_id=$3,revision=revision+1,activity_revision=activity_revision+1,native_bytes=native_bytes+$4,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(unit).bind(native_bytes).execute(&mut *conn).await?;
    let pending:i64=sqlx::query_scalar("SELECT measured_bytes-retained_bytes FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).fetch_one(&mut *conn).await?;
    let result_reasons: BTreeSet<_> = payload.reasons.iter().collect();
    let issue_bytes = result_reasons
        .iter()
        .map(|code| 256 + code.len() as i64)
        .sum::<i64>();
    let size = pending
        + s::sealed_bytes(&a)
        + 256
        + issue_bytes
        + kind.len() as i64
        + outcome.len() as i64
        + data.record.source_id.as_ref().map_or(0, |v| v.len() as i64);
    sqlx::query("INSERT INTO migration_activity_result(id,import_id,plan_id,organization_id,manifest_id,kind,source_id,person_id,target_id,disposition,nonce,ciphertext,actual_bytes,native_bytes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(result).bind(j.id).bind(j.plan).bind(j.org.0).bind(unit).bind(&kind).bind(&data.record.source_id).bind(row.get::<Option<Uuid>,_>("person_id")).bind(target).bind(outcome).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(size).bind(native_bytes).execute(&mut *conn).await?;
    for code in result_reasons {
        sqlx::query("INSERT INTO migration_activity_result_issue(result_id,plan_id,import_id,organization_id,code) VALUES($1,$2,$3,$4,$5)").bind(result).bind(j.plan).bind(j.id).bind(j.org.0).bind(code).execute(&mut *conn).await?;
    }
    let _ = p;
    s::settle(conn, j.org, j.id, token, size).await?;
    tracing::info!(organization_id=%j.org,import_id=%j.id,plan_id=%j.plan,entity_kind=%kind,outcome,native_bytes,retained_bytes=size,"Activity import unit settled");
    Ok(())
}

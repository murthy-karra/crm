//! Bounded retained-evidence worker for Slice 010d3.
//!
//! The worker never contacts FUB. It walks authenticated 010d1 capture pages,
//! resolves targets through terminal admission results, and commits each
//! bounded unit atomically under the admitted owner tuple.
use super::{
    admitted_history_source as source, admitted_history_store as s, crypto,
    history_capture_source as capture_source, history_import_source as history_source,
    history_import_store as history_store, MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::OnceCell;
use uuid::Uuid;

#[derive(Default)]
pub struct WorkerSession {
    started: OnceCell<DateTime<Utc>>,
    admitted: AtomicBool,
}

static SESSION: WorkerSession = WorkerSession {
    started: OnceCell::const_new(),
    admitted: AtomicBool::new(false),
};

struct Claim {
    root: Uuid,
    org: OrganizationId,
    attempt: Option<Uuid>,
    token: Uuid,
}

fn context(org: OrganizationId, row: &PgRow) -> CommandContext {
    CommandContext {
        organization_id: org,
        actor_user_id: UserId::new(row.get("executor_user_id")),
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &super::snapshot::SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    run_once_with_session(pool, key, policy, release, &SESSION).await
}

pub async fn run_once_with_session(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &super::snapshot::SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    session: &WorkerSession,
) -> Result<bool, MigrationError> {
    let boundary = *session
        .started
        .get_or_try_init(|| async {
            sqlx::query_scalar::<_, DateTime<Utc>>("SELECT clock_timestamp()")
                .fetch_one(pool)
                .await
        })
        .await?;
    let Some(claim) = claim(pool, key, policy, release, session, boundary).await? else {
        return Ok(false);
    };
    if let Err(error) = unit(pool, key, &claim).await {
        let reason = match error {
            MigrationError::Crypto => "retained_integrity_failed",
            MigrationError::StorageLimit => "interpretation_bound_exceeded",
            MigrationError::Forbidden => "executor_not_authorized",
            MigrationError::ReleaseNotReady => "release_not_ready",
            MigrationError::SourceNotEligible | MigrationError::Conflict => {
                "source_binding_changed"
            }
            other => return Err(other),
        };
        let mut tx = pool.begin().await?;
        crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
        crate::auth::workspace::shared(&mut tx, claim.org).await?;
        super::store::lock_org(&mut tx, claim.org).await?;
        let r = s::root(&mut tx, claim.org, claim.root).await?;
        if r.get::<Option<Uuid>, _>("lease_token") == Some(claim.token) {
            s::pause(&mut tx, claim.org, claim.root, reason).await?;
        }
        tx.commit().await?;
    }
    Ok(true)
}

async fn claim(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &super::snapshot::SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    session: &WorkerSession,
    boundary: DateTime<Utc>,
) -> Result<Option<Claim>, MigrationError> {
    let candidate = sqlx::query(CANDIDATE_SQL).fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let root_id: Uuid = candidate.get("id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let actor = UserId::new(candidate.get("executor_user_id"));
    let ctx = CommandContext {
        organization_id: org,
        actor_user_id: actor,
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let mut tx = match s::begin(pool, &ctx, false).await {
        Ok(tx) => tx,
        Err(MigrationError::Forbidden) => {
            pause_demoted(pool, org, root_id).await?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let r = s::root(&mut tx, org, root_id).await?;
    let state = r.get::<String, _>("state");
    let active = matches!(state.as_str(), "preparing" | "queued" | "running")
        && (state == "preparing" || r.get::<Option<Uuid>, _>("current_attempt_id").is_some());
    let lease_expired: bool = sqlx::query_scalar(
        "SELECT lease_expires_at IS NULL OR lease_expires_at<=clock_timestamp() FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2",
    )
    .bind(root_id)
    .bind(org.0)
    .fetch_one(&mut *tx)
    .await?;
    if !active || !lease_expired {
        tx.rollback().await?;
        return Ok(None);
    }
    if r.get::<Uuid, _>("executor_user_id") != actor.0 {
        s::pause(&mut tx, org, root_id, "executor_not_authorized").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let plan_id = r.get::<Uuid, _>("latest_plan_id");
    let p = s::plan(&mut tx, org, root_id, plan_id).await?;
    if let Err(error) = s::validate(&mut tx, key, org, &r, &p).await {
        let reason = match error {
            MigrationError::Crypto => "retained_integrity_failed",
            MigrationError::SourceNotEligible | MigrationError::Conflict => {
                "source_binding_changed"
            }
            other => return Err(other),
        };
        s::pause(&mut tx, org, root_id, reason).await?;
        tx.commit().await?;
        return Ok(None);
    }
    let fresh = !session.admitted.load(Ordering::Acquire)
        || r.get::<String, _>("state") == "running"
        || r.get::<Option<DateTime<Utc>>, _>("admitted_at")
            .is_none_or(|v| v < boundary);
    if fresh {
        let ready = match release {
            Some(release) => release.require_admitted_history(&mut tx).await.is_ok(),
            None => false,
        };
        if !ready {
            s::pause(&mut tx, org, root_id, "release_not_ready").await?;
            tx.commit().await?;
            return Ok(None);
        }
        session.admitted.store(true, Ordering::Release);
    }
    // A worker can die after claiming a lease and before its unit transaction
    // settles.  The expired root then still owns the unique `work`
    // reservation.  The root row is locked and its lease was revalidated
    // above, so releasing that stale reservation is safe before reserving the
    // next bounded unit.
    s::release(&mut tx, org, root_id, "work", 0).await?;
    match s::reserve(&mut tx, org, root_id, "work", s::UNIT, policy).await {
        Ok(_) => {}
        Err(MigrationError::StorageLimit) => {
            s::pause(&mut tx, org, root_id, "storage_limit").await?;
            tx.commit().await?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    }
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_admitted_history_root SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',admitted_at=CASE WHEN $4 THEN clock_timestamp() ELSE admitted_at END,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
        .bind(root_id).bind(org.0).bind(token).bind(fresh).execute(&mut *tx).await?;
    let attempt = r.get::<Option<Uuid>, _>("current_attempt_id");
    if let Some(attempt) = attempt {
        sqlx::query("UPDATE migration_admitted_history_attempt SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',executor_user_id=$4,revision=revision+1 WHERE id=$1 AND root_id=$2 AND organization_id=$5")
            .bind(attempt).bind(root_id).bind(token).bind(actor.0).bind(org.0).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(Claim {
        root: root_id,
        org,
        attempt,
        token,
    }))
}

async fn pause_demoted(
    pool: &PgPool,
    org: OrganizationId,
    root: Uuid,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    crate::auth::workspace::shared(&mut tx, org).await?;
    super::store::lock_org(&mut tx, org).await?;
    s::pause(&mut tx, org, root, "executor_not_authorized").await?;
    tx.commit().await?;
    Ok(())
}

async fn unit(pool: &PgPool, key: &RawPayloadKey, claim: &Claim) -> Result<(), MigrationError> {
    let observed = sqlx::query("SELECT executor_user_id FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2")
        .bind(claim.root).bind(claim.org.0).fetch_one(pool).await?;
    let ctx = context(claim.org, &observed);
    let mut tx = s::begin(pool, &ctx, false).await?;
    let r = s::root(&mut tx, claim.org, claim.root).await?;
    let retained_before = r.get::<i64, _>("retained_bytes");
    let valid: bool = sqlx::query_scalar("SELECT state='running' AND lease_token=$3 AND lease_expires_at>clock_timestamp() FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2")
        .bind(claim.root).bind(claim.org.0).bind(claim.token).fetch_one(&mut *tx).await?;
    if !valid {
        return Ok(());
    }
    let p_id = if r.get::<String, _>("phase") == "applying" {
        r.get::<Uuid, _>("confirmed_plan_id")
    } else {
        r.get::<Uuid, _>("latest_plan_id")
    };
    let p = s::plan(&mut tx, claim.org, claim.root, p_id).await?;
    s::validate(&mut tx, key, claim.org, &r, &p).await?;
    match r.get::<String, _>("phase").as_str() {
        "preparing" => index_page(&mut tx, key, claim, &p).await?,
        "classifying" => classify(&mut tx, key, claim, &p).await?,
        "applying" => {
            apply(&mut tx, key, claim, &ctx, &r, &p).await?;
        }
        _ => return Err(MigrationError::Conflict),
    }
    let valid: bool = sqlx::query_scalar("SELECT lease_token=$3 AND lease_expires_at>clock_timestamp() FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2")
        .bind(claim.root).bind(claim.org.0).bind(claim.token).fetch_one(&mut *tx).await?;
    if !valid {
        return Err(MigrationError::Conflict);
    }
    let updated = s::root(&mut tx, claim.org, claim.root).await?;
    let settled = s::root(&mut tx, claim.org, claim.root).await?;
    let actual = settled
        .get::<i64, _>("retained_bytes")
        .saturating_sub(retained_before);
    s::release(&mut tx, claim.org, claim.root, "work", actual).await?;
    if updated.get::<String, _>("phase") == "complete" {
        let attempt = claim.attempt.ok_or(MigrationError::Conflict)?;
        // The cancellation reserve protects a confirmed attempt while it is
        // active. Once every occurrence is settled, release it along with
        // the per-unit work reservation so the shared ledger is reconciled.
        s::release(&mut tx, claim.org, claim.root, "cancel", 0).await?;
        sqlx::query("UPDATE migration_admitted_history_attempt SET state='completed',lease_token=NULL,lease_expires_at=NULL,completed_at=clock_timestamp() WHERE id=$1 AND root_id=$2 AND organization_id=$3")
            .bind(attempt).bind(claim.root).bind(claim.org.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_history_root SET state='completed',lease_token=NULL,lease_expires_at=NULL,completed_at=clock_timestamp(),revision=revision+1 WHERE id=$1 AND organization_id=$2")
            .bind(claim.root).bind(claim.org.0).execute(&mut *tx).await?;
    } else if updated.get::<String, _>("phase") == "applying"
        && updated
            .get::<Option<Uuid>, _>("current_attempt_id")
            .is_none()
    {
        sqlx::query("UPDATE migration_admitted_history_root SET state='ready',lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(claim.root).bind(claim.org.0).execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE migration_admitted_history_root SET state=CASE WHEN phase='applying' THEN 'queued' ELSE 'preparing' END,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(claim.root).bind(claim.org.0).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

fn open_progress(
    key: &RawPayloadKey,
    c: &Claim,
    p: &PgRow,
    family: &str,
) -> Result<Value, MigrationError> {
    let progress = p.get::<Value, _>("stream_progress");
    let row = &progress[family];
    if row["nonce"].is_null() {
        let binding = p.get::<Value, _>("source_binding");
        let stream = binding["coverage"]["streams"]
            .as_array()
            .and_then(|v| v.iter().find(|s| s["family"] == family))
            .ok_or(MigrationError::Crypto)?;
        if *row
            != json!({"checkpoint":0,"finished":false,"reported_total":stream["reported_total"],"nonce":null,"ciphertext":null})
        {
            return Err(MigrationError::Crypto);
        }
        return Ok(
            json!({"cursor":capture_source::Cursor::default(),"checkpoint":0,"finished":false,"reported_total":stream["reported_total"]}),
        );
    }
    let nonce: Vec<u8> =
        serde_json::from_value(row["nonce"].clone()).map_err(|_| MigrationError::Crypto)?;
    let ciphertext: Vec<u8> =
        serde_json::from_value(row["ciphertext"].clone()).map_err(|_| MigrationError::Crypto)?;
    let bytes = crypto::open_history(
        key,
        c.org,
        c.root,
        p.get("id"),
        &format!("admitted-history-progress-v1:{family}"),
        &nonce,
        &ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 8192 {
        return Err(MigrationError::StorageLimit);
    }
    serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)
}

async fn index_page(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let capture_id: Uuid = sqlx::query_scalar("SELECT history_capture_id FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2")
        .bind(c.root).bind(c.org.0).fetch_one(&mut *conn).await?;
    let raw = sqlx::query("SELECT * FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2 AND classification='advancing' AND sequence>$3 ORDER BY sequence LIMIT 1")
        .bind(capture_id).bind(c.org.0).bind(p.get::<i64, _>("last_capture_sequence")).fetch_optional(&mut *conn).await?;
    let Some(raw) = raw else {
        let binding = p.get::<Value, _>("source_binding");
        let coverage = binding["coverage"]["streams"]
            .as_array()
            .ok_or(MigrationError::Crypto)?;
        let mut occurrences = 0_i64;
        for stream in coverage {
            let family = stream["family"].as_str().ok_or(MigrationError::Crypto)?;
            let state = open_progress(key, c, p, family)?;
            if state["finished"] != true
                || state["checkpoint"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .as_deref()
                    != stream["checkpoint"].as_str()
                || state["reported_total"] != stream["reported_total"]
            {
                return Err(MigrationError::Crypto);
            }
            occurrences += stream["occurrences"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or(MigrationError::Crypto)?;
        }
        if occurrences != p.get::<i64, _>("occurrences") {
            return Err(MigrationError::Crypto);
        }
        sqlx::query("UPDATE migration_admitted_history_root SET phase='classifying' WHERE id=$1 AND organization_id=$2").bind(c.root).bind(c.org.0).execute(conn).await?;
        return Ok(());
    };
    let family = raw.get::<String, _>("family");
    let stream = capture_source::Stream::parse(&family).ok_or(MigrationError::Crypto)?;
    let state = open_progress(key, c, p, &family)?;
    if state["finished"] != false {
        return Err(MigrationError::Crypto);
    }
    let cursor: capture_source::Cursor =
        serde_json::from_value(state["cursor"].clone()).map_err(|_| MigrationError::Crypto)?;
    let bytes = crypto::open_history(
        key,
        c.org,
        capture_id,
        raw.get("id"),
        "capture",
        raw.get("nonce"),
        raw.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() as i64 != raw.get::<i64, _>("raw_byte_len")
        || raw.get::<Vec<u8>, _>("content_hmac")
            != crypto::history_hmac(key, c.org, capture_id, "capture", &bytes)
    {
        return Err(MigrationError::Crypto);
    }
    let reported_total: String = sqlx::query_scalar("SELECT reported_total FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND family=$3")
        .bind(capture_id)
        .bind(c.org.0)
        .bind(&family)
        .fetch_one(&mut *conn)
        .await?;
    let stream_row = sqlx::query("SELECT checkpoint,reported_total FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND family=$3")
        .bind(capture_id)
        .bind(c.org.0)
        .bind(&family)
        .fetch_one(&mut *conn)
        .await?;
    if state["checkpoint"].as_i64() != Some(raw.get::<i64, _>("checkpoint"))
        || raw.get::<i64, _>("checkpoint") >= stream_row.get::<i64, _>("checkpoint")
        || raw.get::<bool, _>("truncated")
        || raw.get::<i32, _>("http_status") != 200
        || raw.get::<String, _>("representation") != stream.representation()
        || stream_row.get::<String, _>("reported_total") != reported_total
    {
        return Err(MigrationError::Crypto);
    }
    let parsed = capture_source::parse(
        &capture_source::Request {
            stream,
            cursor: cursor.clone(),
        },
        &bytes,
        Some(&reported_total),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let start =
        usize::try_from(p.get::<i32, _>("page_ordinal")).map_err(|_| MigrationError::Crypto)?;
    let end = (start + 50).min(parsed.records.len());
    if start > parsed.records.len() {
        return Err(MigrationError::Crypto);
    }
    let observations = sqlx::query("SELECT * FROM migration_history_observation WHERE capture_id=$1 AND run_id=$2 AND organization_id=$3 AND ordinal>=$4 AND ordinal<$5 ORDER BY ordinal LIMIT 51")
        .bind(raw.get::<Uuid, _>("id")).bind(capture_id).bind(c.org.0).bind(start as i32).bind(end as i32).fetch_all(&mut *conn).await?;
    if observations.len() != end - start {
        return Err(MigrationError::Crypto);
    }
    let root = c.root;
    let plan = p.get::<Uuid, _>("id");
    let binding = p.get::<Value, _>("source_binding");
    let admission = binding["admission_id"]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(MigrationError::Crypto)?;
    let admission_plan = binding["admission_plan_id"]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(MigrationError::Crypto)?;
    let account = binding["source_account_id"]
        .as_str()
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or(MigrationError::Crypto)?;
    for (index, (record, observation)) in parsed.records[start..end]
        .iter()
        .zip(observations)
        .enumerate()
    {
        let ordinal = start + index;
        if observation.get::<i32, _>("ordinal") != ordinal as i32
            || observation.get::<String, _>("family") != family
            || observation.get::<i64, _>("capture_sequence") != raw.get::<i64, _>("sequence")
        {
            return Err(MigrationError::Crypto);
        }
        let old_identity = record.source_id.as_ref().map(|id| {
            crypto::history_hmac(
                key,
                c.org,
                capture_id,
                &format!("identity:{family}"),
                id.as_bytes(),
            )
            .to_vec()
        });
        let old_person = record.primary_person_id.as_ref().map(|id| {
            crypto::history_hmac(key, c.org, capture_id, "person-reference", id.as_bytes()).to_vec()
        });
        let old_semantic = crypto::history_hmac(
            key,
            c.org,
            capture_id,
            &format!("semantic:{family}"),
            &record.canonical,
        )
        .to_vec();
        if observation.get::<Option<Vec<u8>>, _>("identity_hmac") != old_identity
            || observation.get::<Option<Vec<u8>>, _>("primary_person_hmac") != old_person
            || observation.get::<Vec<u8>, _>("semantic_hmac") != old_semantic
        {
            return Err(MigrationError::Crypto);
        }
        let stable = record.source_id.as_ref().map(|id| {
            crypto::snapshot_hmac(
                key,
                c.org,
                "timeline-import-identity-v1",
                &serde_json::to_vec(&(account, &family, stream.representation(), id))
                    .expect("tuple serializes"),
            )
        });
        let semantic = crypto::snapshot_hmac(
            key,
            c.org,
            &format!(
                "timeline-import-canonical-v1:{account}:{family}:{}",
                stream.representation()
            ),
            &record.canonical,
        );
        let (person, target_reason) = source::target(
            conn,
            c.org,
            admission,
            admission_plan,
            account,
            record.primary_person_id.as_deref(),
        )
        .await?;
        let mut reason = target_reason.map(str::to_owned);
        if stable.is_none() {
            reason = Some("invalid_identity".into());
        }
        if record.relationship_uncertain {
            reason = Some("ambiguous_relationship".into());
        }
        let interpreted = history_source::interpret(
            stream,
            &record.canonical,
            binding["source_access_user_id"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or(MigrationError::Crypto)?,
        )?;
        let id = Uuid::new_v4();
        let mut metadata = interpreted.metadata;
        metadata["source_representation"] = json!(stream.representation());
        let position = p.get::<i64, _>("occurrences") + index as i64 + 1;
        let display = serde_json::to_vec(&json!({"binding":s::manifest_binding(key,c.org,&sqlx::query("SELECT $1::uuid AS id,$2::uuid AS root_id,$3::uuid AS plan_id,$4::uuid AS capture_id,$5::uuid AS observation_id,$6::int AS ordinal,$7::text AS family,$8::text AS representation,$9::bytea AS identity_hmac,$10::bytea AS semantic_hmac,$11::uuid AS person_id,$12::bigint AS position,$13::timestamptz AS source_created_at").bind(id).bind(root).bind(plan).bind(raw.get::<Uuid,_>("id")).bind(observation.get::<Uuid,_>("id")).bind(ordinal as i32).bind(&family).bind(stream.representation()).bind(stable.as_ref().map(|v|v.as_slice())).bind(semantic.as_slice()).bind(person).bind(position).bind(interpreted.created).fetch_one(&mut *conn).await?)?.to_vec(),"metadata":metadata})).map_err(|_| MigrationError::Crypto)?;
        let sealed = crypto::seal_history(
            key,
            c.org,
            root,
            id,
            "admitted-history-manifest-v1",
            &display,
        )
        .map_err(|_| MigrationError::Crypto)?;
        sqlx::query("INSERT INTO migration_admitted_history_manifest(id,root_id,plan_id,organization_id,position,capture_id,observation_id,ordinal,family,representation,identity_hmac,semantic_hmac,person_id,source_created_at,disposition,reason,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'pending',$15,$16,$17)")
            .bind(id).bind(root).bind(plan).bind(c.org.0).bind(position).bind(raw.get::<Uuid,_>("id")).bind(observation.get::<Uuid,_>("id")).bind(ordinal as i32).bind(&family).bind(stream.representation()).bind(stable.as_ref().map(|v|v.as_slice())).bind(semantic.as_slice()).bind(person).bind(interpreted.created).bind(&reason).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        if let Some(stable) = stable {
            let group_held = reason
                .as_deref()
                .is_some_and(|value| value != "out_of_cohort");
            sqlx::query("INSERT INTO migration_admitted_history_candidate(root_id,plan_id,organization_id,identity_hmac,semantic_hmac,first_manifest_id,person_id,conflicting) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(plan_id,organization_id,identity_hmac) DO UPDATE SET occurrences=migration_admitted_history_candidate.occurrences+1,conflicting=migration_admitted_history_candidate.conflicting OR EXCLUDED.conflicting OR migration_admitted_history_candidate.semantic_hmac<>EXCLUDED.semantic_hmac OR migration_admitted_history_candidate.person_id IS DISTINCT FROM EXCLUDED.person_id")
                .bind(root).bind(plan).bind(c.org.0).bind(stable.as_slice()).bind(semantic.as_slice()).bind(id).bind(person).bind(group_held).execute(&mut *conn).await?;
        }
    }
    let complete = end == parsed.records.len();
    let mut progress = p.get::<Value, _>("stream_progress");
    if complete {
        let state = json!({"cursor":parsed.next,"reported_total":parsed.reported_total,"checkpoint":raw.get::<i64,_>("checkpoint")+1,"finished":parsed.next.is_none()});
        let sealed = crypto::seal_history(
            key,
            c.org,
            c.root,
            plan,
            &format!("admitted-history-progress-v1:{family}"),
            &serde_json::to_vec(&state).map_err(|_| MigrationError::Crypto)?,
        )
        .map_err(|_| MigrationError::Crypto)?;
        progress[&family] = json!({"nonce":sealed.nonce.to_vec(),"ciphertext":sealed.ciphertext});
        sqlx::query("UPDATE migration_admitted_history_plan SET last_capture_sequence=$3,page_ordinal=0,stream_progress=$4 WHERE id=$1 AND organization_id=$2")
            .bind(plan).bind(c.org.0).bind(raw.get::<i64,_>("sequence")).bind(progress).execute(&mut *conn).await?;
    } else {
        sqlx::query("UPDATE migration_admitted_history_plan SET page_ordinal=$3,stream_progress=$4 WHERE id=$1 AND organization_id=$2")
            .bind(plan).bind(c.org.0).bind(end as i32).bind(progress).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE migration_admitted_history_plan SET occurrences=occurrences+$3 WHERE id=$1 AND organization_id=$2")
        .bind(plan).bind(c.org.0).bind((end-start) as i64).execute(&mut *conn).await?;
    Ok(())
}

async fn classify(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let rows = sqlx::query("SELECT m.*,x.conflicting,x.first_manifest_id FROM migration_admitted_history_manifest m LEFT JOIN migration_admitted_history_candidate x ON x.plan_id=m.plan_id AND x.organization_id=m.organization_id AND x.identity_hmac=m.identity_hmac WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.position>$3 ORDER BY m.position LIMIT 50")
        .bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(p.get::<i64,_>("classified_position")).fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        let expected_sequence = p.get::<Value, _>("source_binding")["capture_sequence"]
            .as_str()
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or(MigrationError::Crypto)?;
        let binding = p.get::<Value, _>("source_binding");
        for stream in binding["coverage"]["streams"]
            .as_array()
            .ok_or(MigrationError::Crypto)?
        {
            let family = stream["family"].as_str().ok_or(MigrationError::Crypto)?;
            let progress = open_progress(key, c, p, family)?;
            if progress["finished"] != true
                || progress["checkpoint"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .as_deref()
                    != stream["checkpoint"].as_str()
                || progress["reported_total"] != stream["reported_total"]
            {
                return Err(MigrationError::Crypto);
            }
        }
        if p.get::<i64, _>("last_capture_sequence") != expected_sequence {
            return Err(MigrationError::Crypto);
        }
        sqlx::query("UPDATE migration_admitted_history_plan SET state='ready',expires_at=clock_timestamp()+interval '10 minutes',sealed_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(p.get::<Uuid,_>("id")).bind(c.org.0).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_admitted_history_root SET phase='applying' WHERE id=$1 AND organization_id=$2").bind(c.root).bind(c.org.0).execute(&mut *conn).await?;
        return Ok(());
    }
    let mut unique = 0_i64;
    let mut unique_held = 0_i64;
    let mut eligible = 0_i64;
    let mut repeats = 0_i64;
    let mut held = 0_i64;
    let mut excluded = 0_i64;
    let mut unknown = 0_i64;
    let mut last = 0_i64;
    for m in rows {
        let mut reason = m.get::<Option<String>, _>("reason");
        if m.get::<Option<bool>, _>("conflicting") == Some(true)
            && !matches!(
                reason.as_deref(),
                Some("ambiguous_relationship" | "invalid_identity")
            )
        {
            reason = Some("conflicting_variants".into());
        }
        if reason.is_none() {
            if let Some(identity) = m.get::<Option<Vec<u8>>, _>("identity_hmac") {
                if let Some(old)=sqlx::query("SELECT * FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2").bind(c.org.0).bind(identity).fetch_optional(&mut *conn).await? {
                    reason=if old.get::<Option<DateTime<Utc>>,_>("erased_at").is_some(){Some("identity_erased".into())}
                    else if old.get::<Vec<u8>,_>("semantic_hmac")!=m.get::<Vec<u8>,_>("semantic_hmac") || old.get::<Option<Uuid>,_>("person_id")!=m.get::<Option<Uuid>,_>("person_id") || old.get::<String,_>("family")!=m.get::<String,_>("family") {Some("identity_conflict".into())}
                    else if history_store::existing_fact(conn,key,c.org,&old,&m).await?.is_none(){Some("fact_or_display_missing".into())}else{None};
                }
            }
        }
        let disposition = if reason.as_deref() == Some("out_of_cohort") {
            excluded += 1;
            "excluded"
        } else if reason.is_some() {
            held += 1;
            "held"
        } else if m.get::<Option<Uuid>, _>("first_manifest_id") == Some(m.get("id")) {
            eligible += 1;
            "eligible"
        } else {
            repeats += 1;
            "equal_repeat"
        };
        let first = m.get::<Option<Vec<u8>>, _>("identity_hmac").is_none()
            || m.get::<Option<Uuid>, _>("first_manifest_id") == Some(m.get("id"));
        if first && matches!(disposition, "eligible" | "held") {
            unique += 1;
        }
        if first && disposition == "held" {
            unique_held += 1;
        }
        if m.get::<Option<DateTime<Utc>>, _>("source_created_at")
            .is_none()
        {
            unknown += 1;
        }
        sqlx::query("UPDATE migration_admitted_history_manifest SET disposition=$3,reason=$4 WHERE id=$1 AND organization_id=$2").bind(m.get::<Uuid,_>("id")).bind(c.org.0).bind(disposition).bind(&reason).execute(&mut *conn).await?;
        if let Some(reason) = reason {
            sqlx::query("INSERT INTO migration_admitted_history_issue(root_id,plan_id,organization_id,code,record_count) VALUES($1,$2,$3,$4,1) ON CONFLICT(plan_id,organization_id,code) DO UPDATE SET record_count=migration_admitted_history_issue.record_count+1").bind(c.root).bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(reason).execute(&mut *conn).await?;
        }
        last = m.get("position");
    }
    sqlx::query("UPDATE migration_admitted_history_plan SET classified_position=$3,eligible=eligible+$4,equal_repeats=equal_repeats+$5,held=held+$6,excluded=excluded+$7,unknown_dates=unknown_dates+$8 WHERE id=$1 AND organization_id=$2")
        .bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(last).bind(eligible).bind(repeats).bind(held).bind(excluded).bind(unknown).execute(&mut *conn).await?;
    let prior = p.get::<Value, _>("counts");
    let count = |name: &str| -> Result<i64, MigrationError> {
        match prior.get(name) {
            None => Ok(0),
            Some(v) => v
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| *v >= 0)
                .ok_or(MigrationError::Crypto),
        }
    };
    sqlx::query("UPDATE migration_admitted_history_plan SET counts=jsonb_build_object('unique_planned',$3::text,'unique_held',$4::text,'occurrences',occurrences::text,'eligible',eligible::text,'equal_repeats',equal_repeats::text,'held',held::text,'excluded',excluded::text,'unknown_dates',unknown_dates::text) WHERE id=$1 AND organization_id=$2")
        .bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind((count("unique_planned")?+unique).to_string()).bind((count("unique_held")?+unique_held).to_string()).execute(&mut *conn).await?;
    Ok(())
}

async fn apply(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    ctx: &CommandContext,
    _r: &PgRow,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let attempt = c.attempt.ok_or(MigrationError::Conflict)?;
    if p.get::<String, _>("state") != "ready" {
        return Err(MigrationError::Conflict);
    }
    let applied_position: i64 = sqlx::query_scalar("SELECT applied_position FROM migration_admitted_history_attempt WHERE id=$1 AND organization_id=$2")
        .bind(attempt).bind(c.org.0).fetch_one(&mut *conn).await?;
    let rows=sqlx::query("SELECT m.*,x.first_manifest_id FROM migration_admitted_history_manifest m LEFT JOIN migration_admitted_history_candidate x ON x.plan_id=m.plan_id AND x.organization_id=m.organization_id AND x.identity_hmac=m.identity_hmac WHERE m.root_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND m.position>$4 AND NOT EXISTS(SELECT 1 FROM migration_admitted_history_result z WHERE z.root_id=m.root_id AND z.organization_id=m.organization_id AND z.manifest_id=m.id) ORDER BY m.position LIMIT 50")
        .bind(c.root).bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(applied_position).fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        sqlx::query("UPDATE migration_admitted_history_root SET phase='complete' WHERE id=$1 AND organization_id=$2").bind(c.root).bind(c.org.0).execute(conn).await?;
        return Ok(());
    }
    for m in rows {
        let manifest = m.get::<Uuid, _>("id");
        sqlx::query("SELECT set_config('crm.admitted_history_import_permit',$1,true)")
            .bind(json!({"root":c.root,"plan":p.get::<Uuid, _>("id"),"attempt":attempt,"manifest":manifest,"token":c.token}).to_string())
            .execute(&mut *conn)
            .await?;
        let family = m.get::<String, _>("family");
        let disposition = m.get::<String, _>("disposition");
        let mut outcome = if disposition == "equal_repeat" {
            "equal_repeat"
        } else if disposition == "excluded" {
            "excluded"
        } else if disposition == "held" {
            "held"
        } else {
            "imported"
        };
        let mut reason = m.get::<Option<String>, _>("reason");
        let mut fact: Option<Uuid> = None;
        if disposition == "eligible" {
            if m.get::<Option<Vec<u8>>, _>("nonce").is_none()
                || m.get::<Option<Vec<u8>>, _>("ciphertext").is_none()
            {
                reason = Some("identity_erased".into());
                outcome = "held";
            }
            if outcome == "imported" {
                let metadata = s::open_metadata(key, c.org, &m)?;
                let source_person_id = metadata.get("source_person_id").and_then(Value::as_str);
                let binding = p.get::<Value, _>("source_binding");
                let admission = binding["admission_id"]
                    .as_str()
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .ok_or(MigrationError::Crypto)?;
                let admission_plan = binding["admission_plan_id"]
                    .as_str()
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .ok_or(MigrationError::Crypto)?;
                let account = binding["source_account_id"]
                    .as_str()
                    .and_then(|v| v.parse::<i64>().ok())
                    .ok_or(MigrationError::Crypto)?;
                let (target, target_reason) = source::target(
                    conn,
                    c.org,
                    admission,
                    admission_plan,
                    account,
                    source_person_id,
                )
                .await?;
                if target != m.get::<Option<Uuid>, _>("person_id") || target_reason.is_some() {
                    reason = target_reason
                        .map(str::to_owned)
                        .or(Some("target_identity_mismatch".into()));
                    outcome = "held";
                }
                if outcome == "imported" {
                    let identity = m.get::<Vec<u8>, _>("identity_hmac");
                    let old=sqlx::query("SELECT * FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2 FOR UPDATE").bind(c.org.0).bind(&identity).fetch_optional(&mut *conn).await?;
                    if let Some(old) = old {
                        let old_fact_id = old.get::<Option<Uuid>, _>("fact_id");
                        let fact_integrity = old_fact_id.is_some()
                            && history_store::existing_fact(&mut *conn, key, c.org, &old, &m)
                                .await?
                                == old_fact_id;
                        if old.get::<Option<DateTime<Utc>>, _>("erased_at").is_some() {
                            reason = Some("identity_erased".into());
                            outcome = "held";
                        } else if old.get::<Vec<u8>, _>("semantic_hmac")
                            != m.get::<Vec<u8>, _>("semantic_hmac")
                            || old.get::<Option<Uuid>, _>("person_id")
                                != m.get::<Option<Uuid>, _>("person_id")
                        {
                            reason = Some("identity_conflict".into());
                            outcome = "held";
                        } else if !fact_integrity {
                            reason = Some("fact_or_display_missing".into());
                            outcome = "held";
                        } else {
                            fact = old_fact_id;
                            outcome = "already_present";
                        }
                    } else {
                        let identity_id = Uuid::new_v4();
                        let fact_id = Uuid::new_v4();
                        fact = Some(fact_id);
                        sqlx::query("INSERT INTO migration_history_import_identity(id,organization_id,owner_run_id,identity_hmac,semantic_hmac,person_id,fact_id,family,admitted_root_id,admitted_plan_id,admitted_attempt_id,admitted_manifest_id) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(identity_id).bind(c.org.0).bind(&identity).bind(m.get::<Vec<u8>,_>("semantic_hmac")).bind(m.get::<Option<Uuid>,_>("person_id")).bind(fact_id).bind(&family).bind(c.root).bind(p.get::<Uuid,_>("id")).bind(attempt).bind(manifest).execute(&mut *conn).await?;
                        let created = m.get::<Option<DateTime<Utc>>, _>("source_created_at");
                        let table = history_source::fact_table(&family)?;
                        let display_payload = serde_json::to_vec(&json!({
                            "binding": s::manifest_binding(key, c.org, &m)?,
                            "metadata": metadata,
                        }))
                        .map_err(|_| MigrationError::Crypto)?;
                        if display_payload.len() > 4096 {
                            return Err(MigrationError::StorageLimit);
                        }
                        let display = crypto::seal_history(
                            key,
                            c.org,
                            c.root,
                            manifest,
                            &format!(
                                "admitted-history-v1:{}:{}:{}:{}:display",
                                c.root,
                                p.get::<Uuid, _>("id"),
                                attempt,
                                manifest
                            ),
                            &display_payload,
                        )
                        .map_err(|_| MigrationError::Crypto)?;
                        sqlx::query("INSERT INTO migration_history_import_display(id,organization_id,plan_id,owner_run_id,nonce,ciphertext,admitted_root_id,admitted_plan_id,admitted_attempt_id) VALUES($1,$2,NULL,NULL,$3,$4,$5,$6,$7)").bind(manifest).bind(c.org.0).bind(display.nonce.as_slice()).bind(display.ciphertext).bind(c.root).bind(p.get::<Uuid,_>("id")).bind(attempt).execute(&mut *conn).await?;
                        let sql=format!("INSERT INTO {table}(id,organization_id,actor_kind,actor_user_id,on_behalf_of_user_id,origin,occurred_at,recorded_at,correlation_id,causation_id,corrects_id,person_id,plan_id,attempt_id,manifest_id,identity_id,source_created_at,source_time_basis,stable_position,admitted_root_id,admitted_plan_id,admitted_attempt_id,admitted_manifest_id) VALUES($1,$2,'user',$3,NULL,'migration',clock_timestamp(),clock_timestamp(),$4,$5,NULL,$6,NULL,NULL,NULL,$7,$8,$9,$10,$11,$12,$13,$14)");
                        sqlx::query(&sql)
                            .bind(fact_id)
                            .bind(c.org.0)
                            .bind(ctx.actor_user_id.0)
                            .bind(ctx.correlation_id.0)
                            .bind(c.root)
                            .bind(m.get::<Option<Uuid>, _>("person_id"))
                            .bind(identity_id)
                            .bind(created)
                            .bind(if created.is_some() {
                                "fub_record_created"
                            } else {
                                "unknown"
                            })
                            .bind(m.get::<i64, _>("position"))
                            .bind(c.root)
                            .bind(p.get::<Uuid, _>("id"))
                            .bind(attempt)
                            .bind(manifest)
                            .execute(&mut *conn)
                            .await?;
                    }
                }
            }
        }
        let result = Uuid::new_v4();
        sqlx::query("INSERT INTO migration_admitted_history_result(id,root_id,plan_id,attempt_id,manifest_id,organization_id,position,family,disposition,reason,fact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(result).bind(c.root).bind(p.get::<Uuid,_>("id")).bind(attempt).bind(manifest).bind(c.org.0).bind(m.get::<i64,_>("position")).bind(&family).bind(outcome).bind(reason).bind(fact).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_admitted_history_root SET result_counts=jsonb_set(COALESCE(result_counts,'{}'::jsonb),ARRAY[$3::text],to_jsonb(COALESCE((result_counts->>$3)::bigint,0)+1),true) WHERE id=$1 AND organization_id=$2").bind(c.root).bind(c.org.0).bind(outcome).execute(&mut *conn).await?;
        if outcome == "held"
            && (m.get::<Option<Vec<u8>>, _>("identity_hmac").is_none()
                || m.get::<Option<Uuid>, _>("first_manifest_id") == Some(manifest))
        {
            sqlx::query("UPDATE migration_admitted_history_root SET result_counts=jsonb_set(result_counts,'{unique_held}',to_jsonb(COALESCE((result_counts->>'unique_held')::bigint,0)+1),true) WHERE id=$1 AND organization_id=$2").bind(c.root).bind(c.org.0).execute(&mut *conn).await?;
        }
        sqlx::query("UPDATE migration_admitted_history_attempt SET applied_position=$3,revision=revision+1 WHERE id=$1 AND root_id=$2 AND organization_id=$4").bind(attempt).bind(c.root).bind(m.get::<i64,_>("position")).bind(c.org.0).execute(&mut *conn).await?;
    }
    Ok(())
}

// Shared with the scheduler hint; this SELECT never grants a work permit.
pub(super) const CANDIDATE_SQL: &str = "SELECT id,organization_id,executor_user_id FROM migration_admitted_history_root WHERE state IN ('queued','preparing','running') AND (state='preparing' OR current_attempt_id IS NOT NULL) AND (lease_expires_at IS NULL OR lease_expires_at<=clock_timestamp()) ORDER BY created_at,id LIMIT 1";

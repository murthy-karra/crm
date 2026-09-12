//! Bounded DB-only work. No source/provider reader is accepted by this module.
use super::{
    crypto, history_capture_source as capture_source, history_capture_store as capture_store,
    history_import_source as source, history_import_store as s, snapshot::SnapshotPolicy, store,
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;
#[derive(Default)]
pub struct WorkerSession {
    started: tokio::sync::OnceCell<DateTime<Utc>>,
    admitted: AtomicBool,
}
static SESSION: WorkerSession = WorkerSession {
    started: tokio::sync::OnceCell::const_new(),
    admitted: AtomicBool::new(false),
};
struct Claim {
    id: Uuid,
    org: OrganizationId,
    token: Uuid,
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    run_once_with_session(pool, key, policy, release, &SESSION).await
}
pub async fn run_once_with_session(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
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
    tokio::task::yield_now().await;
    if let Err(error) = unit(pool, key, policy, &claim).await {
        let reason = match error {
            MigrationError::Crypto => "retained_integrity_failed",
            MigrationError::StorageLimit => "interpretation_bound_exceeded",
            MigrationError::Forbidden => "executor_not_authorized",
            MigrationError::SourceNotEligible | MigrationError::Conflict => {
                "source_binding_changed"
            }
            other => return Err(other),
        };
        let mut tx = pool.begin().await?;
        store::lock_org(&mut tx, claim.org).await?;
        let r = s::row(&mut tx, claim.org, claim.id).await?;
        if r.get::<Option<Uuid>, _>("lease_token") == Some(claim.token) {
            s::pause(&mut tx, claim.org, claim.id, reason).await?;
        }
        tx.commit().await?;
    }
    Ok(true)
}
fn context(org: OrganizationId, r: &PgRow) -> CommandContext {
    CommandContext {
        organization_id: org,
        actor_user_id: UserId::new(r.get("executor_user_id")),
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}
async fn claim(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    session: &WorkerSession,
    boundary: DateTime<Utc>,
) -> Result<Option<Claim>, MigrationError> {
    let Some(candidate)=sqlx::query("SELECT id,organization_id FROM migration_history_import_run WHERE state IN ('preparing','queued') OR (state='running' AND lease_expires_at<=clock_timestamp()) ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await? else{return Ok(None);};
    let id = candidate.get("id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    // The shared workspace lock precedes membership and Organization row locks.
    crate::auth::workspace::shared(&mut tx, org).await?;
    let observed = s::row(&mut tx, org, id).await?;
    let ctx = context(org, &observed);
    // Do not lock the run before a potentially waiting Organization writer.
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    crate::auth::workspace::shared(&mut tx, org).await?;
    let authorized = store::require_admin(&mut tx, &ctx).await.is_ok();
    store::lock_org(&mut tx, org).await?;
    let r = s::row(&mut tx, org, id).await?;
    let active:bool=sqlx::query_scalar("SELECT state IN ('preparing','queued') OR (state='running' AND lease_expires_at<=clock_timestamp()) FROM migration_history_import_run WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *tx).await?;
    if !active {
        return Ok(None);
    }
    if !authorized || r.get::<Uuid, _>("executor_user_id") != ctx.actor_user_id.0 {
        s::pause(&mut tx, org, id, "executor_not_authorized").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let p = s::plan(&mut tx, org, r.get("plan_id")).await?;
    if let Err(e) = s::validate(&mut tx, key, org, &p).await {
        s::pause(
            &mut tx,
            org,
            id,
            if matches!(e, MigrationError::Crypto) {
                "retained_integrity_failed"
            } else {
                "source_binding_changed"
            },
        )
        .await?;
        tx.commit().await?;
        return Ok(None);
    }
    let fresh = !session.admitted.load(Ordering::Acquire)
        || r.get::<String, _>("state") == "running"
        || r.get::<Option<DateTime<Utc>>, _>("admitted_at")
            .is_none_or(|v| v < boundary);
    if fresh {
        let ready = if let Some(release) = release {
            release.require_history_timeline(&mut tx).await.is_ok()
        } else {
            false
        };
        if !ready {
            s::pause(&mut tx, org, id, "release_not_ready").await?;
            tx.commit().await?;
            return Ok(None);
        }
        session.admitted.store(true, Ordering::Release);
    }
    s::release_kind(&mut tx, org, id, "unit", 0).await?;
    if s::reserve(&mut tx, org, id, "unit", s::UNIT, policy)
        .await
        .is_err()
    {
        s::pause(&mut tx, org, id, "storage_limit").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_history_import_run SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',admitted_at=CASE WHEN $4 THEN clock_timestamp() ELSE admitted_at END,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(token).bind(fresh).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(Claim { id, org, token }))
}
async fn unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    c: &Claim,
) -> Result<(), MigrationError> {
    let observed=sqlx::query("SELECT executor_user_id FROM migration_history_import_run WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).fetch_one(pool).await?;
    let ctx = context(c.org, &observed);
    let mut tx = s::begin(pool, &ctx, false).await?;
    let r = s::row(&mut tx, c.org, c.id).await?;
    let valid:bool=sqlx::query_scalar("SELECT state='running' AND lease_token=$3 AND lease_expires_at>clock_timestamp() FROM migration_history_import_run WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(c.token).fetch_one(&mut *tx).await?;
    if !valid {
        return Ok(());
    }
    if r.get::<Uuid, _>("executor_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    let p = s::plan(&mut tx, c.org, r.get("plan_id")).await?;
    s::validate(&mut tx, key, c.org, &p).await?;
    // Existing admission may outlive report freshness; policy ceilings cannot.
    let ledger=sqlx::query("SELECT byte_limit,retained_bytes,reserved_bytes FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(c.org.0).fetch_one(&mut *tx).await?;
    if r.get::<i64, _>("retained_bytes") + r.get::<i64, _>("reserved_bytes")
        > r.get::<i64, _>("run_byte_limit")
            .min(policy.run_ceiling_bytes)
        || ledger.get::<i64, _>("retained_bytes") + ledger.get::<i64, _>("reserved_bytes")
            > ledger
                .get::<i64, _>("byte_limit")
                .min(policy.org_ceiling_bytes)
    {
        s::pause(&mut tx, c.org, c.id, "storage_limit").await?;
        tx.commit().await?;
        return Ok(());
    }
    let before = r.get::<i64, _>("retained_bytes");
    match r.get::<String, _>("phase").as_str() {
        "capture" => prepare_page(&mut tx, key, c, &p).await?,
        "classify" => classify(&mut tx, c, &p).await?,
        "apply" => {
            sqlx::query("SELECT set_config('crm.history_import_token',$1,true)")
                .bind(c.token.to_string())
                .execute(&mut *tx)
                .await?;
            apply(&mut tx, key, c, &ctx, &r, &p).await?;
        }
        _ => return Err(MigrationError::Conflict),
    }
    // The row/Org locks serialize cancellation; this final DB clock fence also
    // rejects an overlong bounded unit rather than accepting an expired lease.
    if !sqlx::query_scalar::<_,bool>("SELECT lease_token=$3 AND lease_expires_at>clock_timestamp() FROM migration_history_import_run WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(c.token).fetch_one(&mut *tx).await?{return Err(MigrationError::Conflict);}
    let updated = s::row(&mut tx, c.org, c.id).await?;
    let delta = updated.get::<i64, _>("retained_bytes") - before;
    s::release_kind(&mut tx, c.org, c.id, "unit", delta).await?;
    if updated.get::<String, _>("phase") == "finished" {
        s::release_kind(&mut tx, c.org, c.id, "control", 0).await?;
    }
    sqlx::query("UPDATE migration_history_import_run SET state=CASE WHEN phase='finished' THEN 'completed' WHEN phase='apply' AND confirmed_at IS NULL THEN 'ready' WHEN phase='apply' THEN 'queued' ELSE 'preparing' END,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=clock_timestamp(),completed_at=CASE WHEN phase='finished' THEN clock_timestamp() ELSE completed_at END WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn prepare_page(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let raw=sqlx::query("SELECT * FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2 AND classification='advancing' AND sequence>$3 AND sequence<=$4 ORDER BY sequence LIMIT 1").bind(p.get::<Uuid,_>("capture_id")).bind(c.org.0).bind(p.get::<i64,_>("last_capture_sequence")).bind(p.get::<i64,_>("capture_sequence")).fetch_optional(&mut *conn).await?;
    let Some(raw) = raw else {
        let unfinished:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_history_import_stream WHERE plan_id=$1 AND organization_id=$2 AND NOT finished)").bind(p.get::<Uuid,_>("id")).bind(c.org.0).fetch_one(&mut *conn).await?;
        if unfinished {
            return Err(MigrationError::Crypto);
        }
        sqlx::query("UPDATE migration_history_import_run SET phase='classify' WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(conn).await?;
        return Ok(());
    };
    let family = raw.get::<String, _>("family");
    let stream = capture_source::Stream::parse(&family).ok_or(MigrationError::Crypto)?;
    let progress=sqlx::query("SELECT * FROM migration_history_import_stream WHERE plan_id=$1 AND organization_id=$2 AND family=$3").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(&family).fetch_one(&mut *conn).await?;
    if progress.get::<bool, _>("finished")
        || raw.get::<i64, _>("checkpoint") != progress.get::<i64, _>("checkpoint")
        || raw.get::<bool, _>("truncated")
        || raw.get::<i32, _>("http_status") != 200
        || raw.get::<String, _>("representation") != stream.representation()
    {
        return Err(MigrationError::Crypto);
    }
    let bytes = crypto::open_history(
        key,
        c.org,
        p.get("capture_id"),
        raw.get("id"),
        "capture",
        raw.get("nonce"),
        raw.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() as i64 != raw.get::<i64, _>("raw_byte_len")
        || raw.get::<Vec<u8>, _>("content_hmac")
            != crypto::history_hmac(key, c.org, p.get("capture_id"), "capture", &bytes)
    {
        return Err(MigrationError::Crypto);
    }
    let cursor = if let Some(nonce) = progress.get::<Option<Vec<u8>>, _>("nonce") {
        let plaintext = crypto::open_history(
            key,
            c.org,
            p.get("id"),
            p.get("id"),
            &format!("timeline-import-stream-v1:{family}"),
            &nonce,
            &progress.get::<Vec<u8>, _>("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        serde_json::from_slice(&plaintext).map_err(|_| MigrationError::Crypto)?
    } else {
        capture_source::Cursor::default()
    };
    let parsed = capture_source::parse(
        &capture_source::Request { stream, cursor },
        &bytes,
        Some(&progress.get::<String, _>("reported_total")),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let start =
        usize::try_from(p.get::<i32, _>("page_ordinal")).map_err(|_| MigrationError::Crypto)?;
    if start > parsed.records.len() {
        return Err(MigrationError::Crypto);
    }
    let end = (start + 50).min(parsed.records.len());
    let observations=sqlx::query("SELECT * FROM migration_history_observation WHERE capture_id=$1 AND run_id=$2 AND organization_id=$3 AND ordinal>=$4 AND ordinal<$5 ORDER BY ordinal LIMIT 51").bind(raw.get::<Uuid,_>("id")).bind(p.get::<Uuid,_>("capture_id")).bind(c.org.0).bind(start as i32).bind(end as i32).fetch_all(&mut *conn).await?;
    if observations.len() != end - start {
        return Err(MigrationError::Crypto);
    }
    for (index, (record, observation)) in parsed.records[start..end]
        .iter()
        .zip(observations)
        .enumerate()
    {
        let ordinal = start + index;
        let position = p.get::<i64, _>("occurrences") + (index as i64) + 1;
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
                p.get("capture_id"),
                &format!("identity:{family}"),
                id.as_bytes(),
            )
            .to_vec()
        });
        let old_person = record.primary_person_id.as_ref().map(|id| {
            crypto::history_hmac(
                key,
                c.org,
                p.get("capture_id"),
                "person-reference",
                id.as_bytes(),
            )
            .to_vec()
        });
        if observation.get::<Option<Vec<u8>>, _>("identity_hmac") != old_identity
            || observation.get::<Option<Vec<u8>>, _>("primary_person_hmac") != old_person
            || observation.get::<Vec<u8>, _>("semantic_hmac")
                != crypto::history_hmac(
                    key,
                    c.org,
                    p.get("capture_id"),
                    &format!("semantic:{family}"),
                    &record.canonical,
                )
        {
            return Err(MigrationError::Crypto);
        }
        let stable = record.source_id.as_ref().map(|id| {
            crypto::snapshot_hmac(
                key,
                c.org,
                "timeline-import-identity-v1",
                &serde_json::to_vec(&(
                    p.get::<i64, _>("source_account_id"),
                    &family,
                    stream.representation(),
                    id,
                ))
                .expect("fixed serializable tuple"),
            )
        });
        let semantic = crypto::snapshot_hmac(
            key,
            c.org,
            &format!(
                "timeline-import-canonical-v1:{}:{family}:{}",
                p.get::<i64, _>("source_account_id"),
                stream.representation()
            ),
            &record.canonical,
        );
        let (link, person) =
            capture_store::parent_link(conn, c.org, p, record.primary_person_id.as_deref()).await?;
        // Capture keeps the mapped UUID after Person erasure. A single bounded
        // observation-link lookup also covers a non-primary group participant.
        let parent_erased = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM migration_history_person_link l LEFT JOIN person p ON p.id=l.person_id AND p.organization_id=l.organization_id WHERE l.observation_id=$1 AND l.organization_id=$2 AND l.person_id IS NOT NULL AND p.id IS NULL)").bind(observation.get::<Uuid,_>("id")).bind(c.org.0).fetch_one(&mut *conn).await? || (link == "parent_excluded" && person.is_some());
        let tombstoned = if let Some(stable) = stable.as_ref() {
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2 AND erased_at IS NOT NULL)").bind(c.org.0).bind(stable.as_slice()).fetch_one(&mut *conn).await?
        } else {
            false
        };
        let erased = parent_erased || tombstoned;
        let reason = if parent_erased {
            Some("parent_erased")
        } else if tombstoned {
            Some("identity_erased")
        } else if stable.is_none() {
            Some("invalid_identity")
        } else if record.relationship_uncertain {
            Some("ambiguous_relationship")
        } else {
            match link {
                "linked" => None,
                "parent_excluded" if person.is_some() => Some("parent_erased"),
                "parent_excluded" => Some("parent_excluded"),
                _ => Some("no_parent_identity"),
            }
        };
        let interpreted = if erased {
            source::Interpretation {
                created: None,
                metadata: serde_json::Value::Null,
            }
        } else {
            source::interpret(stream, &record.canonical, p.get("source_access_user_id"))?
        };
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO migration_history_import_manifest(id,organization_id,plan_id,owner_run_id,position,capture_id,observation_id,ordinal,family,representation,identity_hmac,semantic_hmac,person_id,source_created_at,disposition,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'pending',$15)").bind(id).bind(c.org.0).bind(p.get::<Uuid,_>("id")).bind(c.id).bind(position).bind(raw.get::<Uuid,_>("id")).bind(observation.get::<Uuid,_>("id")).bind(ordinal as i32).bind(&family).bind(stream.representation()).bind(stable.as_ref().map(|v|v.as_slice())).bind(semantic.as_slice()).bind(person).bind(interpreted.created).bind(reason).execute(&mut *conn).await?;
        if parent_erased {
            if let Some(stable) = stable.as_ref() {
                sqlx::query("INSERT INTO migration_history_import_identity(id,organization_id,owner_run_id,identity_hmac,semantic_hmac,person_id,fact_id,family,erased_at) VALUES($1,$2,$3,$4,$5,$6,NULL,$7,clock_timestamp()) ON CONFLICT(organization_id,identity_hmac) DO NOTHING").bind(Uuid::new_v4()).bind(c.org.0).bind(c.id).bind(stable.as_slice()).bind(semantic.as_slice()).bind(person).bind(&family).execute(&mut *conn).await?;
                sqlx::query("UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=$1 AND identity_hmac=$2 AND erased_at IS NULL").bind(c.org.0).bind(stable.as_slice()).execute(&mut *conn).await?;
            }
        }
        if !erased {
            let m = sqlx::query(
            "SELECT * FROM migration_history_import_manifest WHERE id=$1 AND organization_id=$2",
        )
        .bind(id)
        .bind(c.org.0)
        .fetch_one(&mut *conn)
        .await?;
            let display=serde_json::to_vec(&json!({"binding":s::manifest_binding(key,c.org,&m)?.to_vec(),"metadata":interpreted.metadata})).map_err(|_|MigrationError::Crypto)?;
            if display.len() > source::DISPLAY_BYTES {
                return Err(MigrationError::StorageLimit);
            }
            let sealed = crypto::seal_history(
                key,
                c.org,
                p.get("id"),
                id,
                "timeline-import-display-v1",
                &display,
            )
            .map_err(|_| MigrationError::Crypto)?;
            sqlx::query("INSERT INTO migration_history_import_display(id,organization_id,plan_id,owner_run_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(c.org.0).bind(p.get::<Uuid,_>("id")).bind(c.id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        }
        if let Some(stable) = stable {
            sqlx::query("INSERT INTO migration_history_import_candidate(plan_id,organization_id,owner_run_id,identity_hmac,semantic_hmac,first_manifest_id,person_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(plan_id,organization_id,identity_hmac) DO UPDATE SET occurrences=migration_history_import_candidate.occurrences+1,conflicting=migration_history_import_candidate.conflicting OR migration_history_import_candidate.semantic_hmac<>EXCLUDED.semantic_hmac OR migration_history_import_candidate.person_id IS DISTINCT FROM EXCLUDED.person_id").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(c.id).bind(stable.as_slice()).bind(semantic.as_slice()).bind(id).bind(person).execute(&mut *conn).await?;
        }
    }
    let complete = end == parsed.records.len();
    if complete {
        let sealed = parsed
            .next
            .as_ref()
            .map(|next| {
                crypto::seal_history(
                    key,
                    c.org,
                    p.get("id"),
                    p.get("id"),
                    &format!("timeline-import-stream-v1:{family}"),
                    &serde_json::to_vec(next).expect("fixed cursor"),
                )
            })
            .transpose()
            .map_err(|_| MigrationError::Crypto)?;
        sqlx::query("UPDATE migration_history_import_stream SET checkpoint=checkpoint+1,finished=$4,nonce=$5,ciphertext=$6 WHERE plan_id=$1 AND organization_id=$2 AND family=$3").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(family).bind(parsed.next.is_none()).bind(sealed.as_ref().map(|v|v.nonce.as_slice())).bind(sealed.as_ref().map(|v|v.ciphertext.as_slice())).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE migration_history_import_plan SET occurrences=occurrences+$3,last_capture_sequence=CASE WHEN $4 THEN $5 ELSE last_capture_sequence END,page_ordinal=$6 WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind((end-start) as i64).bind(complete).bind(raw.get::<i64,_>("sequence")).bind(if complete{0}else{end as i32}).execute(conn).await?;
    Ok(())
}
async fn classify(conn: &mut PgConnection, c: &Claim, p: &PgRow) -> Result<(), MigrationError> {
    let rows=sqlx::query("SELECT m.*,x.conflicting,x.first_manifest_id FROM migration_history_import_manifest m LEFT JOIN migration_history_import_candidate x ON x.plan_id=m.plan_id AND x.organization_id=m.organization_id AND x.identity_hmac=m.identity_hmac WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.position>$3 ORDER BY m.position LIMIT 50").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(p.get::<i64,_>("classified_position")).fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        sqlx::query("UPDATE migration_history_import_plan SET state='ready',ready_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes',added_byte_bound=occurrences*$3 WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(source::RECORD_RESERVATION).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_history_import_run SET phase='apply' WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(conn).await?;
        return Ok(());
    }
    let (mut eligible, mut repeats, mut held) = (0i64, 0i64, 0i64);
    let mut last = 0_i64;
    for m in rows {
        let reason = if m.get::<Option<bool>, _>("conflicting") == Some(true) {
            Some("conflicting_variants".to_string())
        } else {
            m.get::<Option<String>, _>("reason")
        };
        let disposition = if reason.is_some() {
            held += 1;
            "held"
        } else if m.get::<Option<Uuid>, _>("first_manifest_id") == Some(m.get("id")) {
            eligible += 1;
            "eligible"
        } else {
            repeats += 1;
            "equal_repeat"
        };
        sqlx::query("UPDATE migration_history_import_manifest SET disposition=$3,reason=$4 WHERE id=$1 AND organization_id=$2").bind(m.get::<Uuid,_>("id")).bind(c.org.0).bind(disposition).bind(reason).execute(&mut *conn).await?;
        last = m.get("position");
    }
    sqlx::query("UPDATE migration_history_import_plan SET classified_position=$3,eligible=eligible+$4,equal_repeats=equal_repeats+$5,held=held+$6 WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(last).bind(eligible).bind(repeats).bind(held).execute(conn).await?;
    Ok(())
}
async fn apply(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    ctx: &CommandContext,
    r: &PgRow,
    p: &PgRow,
) -> Result<(), MigrationError> {
    if r.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_none()
        || p.get::<String, _>("state") != "ready"
    {
        return Err(MigrationError::Conflict);
    }
    let rows=sqlx::query("SELECT * FROM migration_history_import_manifest WHERE plan_id=$1 AND organization_id=$2 AND position>$3 ORDER BY position LIMIT 50").bind(p.get::<Uuid,_>("id")).bind(c.org.0).bind(r.get::<i64,_>("applied_position")).fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        sqlx::query("UPDATE migration_history_import_run SET phase='finished' WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(conn).await?;
        return Ok(());
    }
    for m in rows {
        let manifest = m.get::<Uuid, _>("id");
        let mut reason = m.get::<Option<String>, _>("reason");
        let mut outcome = "held";
        let mut fact = None;
        if m.get::<String, _>("disposition") != "held" {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_history_import_display WHERE id=$1 AND organization_id=$2)").bind(manifest).bind(c.org.0).fetch_one(&mut *conn).await?;
            if !exists {
                reason = Some("identity_erased".into());
            } else {
                let metadata = s::display(conn, key, c.org, p.get("id"), manifest).await?;
                let (link, person) = capture_store::parent_link(
                    conn,
                    c.org,
                    p,
                    metadata["source_person_id"].as_str(),
                )
                .await?;
                if link != "linked" || person != m.get::<Option<Uuid>, _>("person_id") {
                    reason = Some("parent_erased".into());
                } else {
                    let identity = m
                        .get::<Option<Vec<u8>>, _>("identity_hmac")
                        .ok_or(MigrationError::Crypto)?;
                    let previous=sqlx::query("SELECT * FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2 FOR UPDATE").bind(c.org.0).bind(&identity).fetch_optional(&mut *conn).await?;
                    if let Some(old) = previous {
                        if old.get::<Option<DateTime<Utc>>, _>("erased_at").is_some() {
                            reason = Some("identity_erased".into());
                        } else if old.get::<Vec<u8>, _>("semantic_hmac")
                            != m.get::<Vec<u8>, _>("semantic_hmac")
                            || old.get::<Option<Uuid>, _>("person_id") != person
                        {
                            reason = Some("identity_conflict".into());
                        } else {
                            fact = old.get::<Option<Uuid>, _>("fact_id");
                            outcome = if m.get::<String, _>("disposition") == "equal_repeat" {
                                "equal_repeat"
                            } else {
                                "already_imported"
                            };
                        }
                    } else if m.get::<String, _>("disposition") == "equal_repeat" {
                        reason = Some("identity_erased".into());
                    } else {
                        let identity_id = Uuid::new_v4();
                        let fact_id = Uuid::new_v4();
                        let family = m.get::<String, _>("family");
                        sqlx::query("INSERT INTO migration_history_import_identity(id,organization_id,owner_run_id,identity_hmac,semantic_hmac,person_id,fact_id,family) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(identity_id).bind(c.org.0).bind(c.id).bind(identity).bind(m.get::<Vec<u8>,_>("semantic_hmac")).bind(person).bind(fact_id).bind(&family).execute(&mut *conn).await?;
                        let table = source::fact_table(&family)?;
                        let created = m.get::<Option<DateTime<Utc>>, _>("source_created_at");
                        sqlx::query(&format!("INSERT INTO {table}(id,organization_id,actor_kind,actor_user_id,on_behalf_of_user_id,origin,occurred_at,recorded_at,correlation_id,causation_id,corrects_id,person_id,plan_id,attempt_id,manifest_id,identity_id,source_created_at,source_time_basis,stable_position) VALUES($1,$2,'user',$3,NULL,'migration',clock_timestamp(),clock_timestamp(),$4,$5,NULL,$6,$7,$5,$8,$9,$10,$11,$12)")).bind(fact_id).bind(c.org.0).bind(ctx.actor_user_id.0).bind(ctx.correlation_id.0).bind(c.id).bind(person).bind(p.get::<Uuid,_>("id")).bind(manifest).bind(identity_id).bind(created).bind(if created.is_some(){"fub_record_created"}else{"unknown"}).bind(m.get::<i64,_>("position")).execute(&mut *conn).await?;
                        fact = Some(fact_id);
                        outcome = "imported";
                    }
                }
            }
        }
        sqlx::query("INSERT INTO migration_history_import_result(id,organization_id,plan_id,owner_run_id,manifest_id,position,family,disposition,reason,fact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(Uuid::new_v4()).bind(c.org.0).bind(p.get::<Uuid,_>("id")).bind(c.id).bind(manifest).bind(m.get::<i64,_>("position")).bind(m.get::<String,_>("family")).bind(outcome).bind(reason).bind(fact).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_history_import_run SET applied_position=$3,processed=processed+1,inserted=inserted+$4,already_imported=already_imported+$5,application_held=application_held+$6 WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(m.get::<i64,_>("position")).bind(i64::from(outcome=="imported")).bind(i64::from(outcome=="already_imported"||outcome=="equal_repeat")).bind(i64::from(outcome=="held")).execute(&mut *conn).await?;
    }
    Ok(())
}

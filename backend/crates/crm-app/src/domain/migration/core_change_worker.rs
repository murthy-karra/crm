//! Bounded retained-only work. No source reader, provider or credential argument.
use super::{
    core_change_source as source, core_change_store as s, crypto, snapshot::SnapshotPolicy,
    snapshot_source::Stream, store, MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

struct Claim {
    id: Uuid,
    org: OrganizationId,
    actor: UserId,
    token: Uuid,
    epoch: i64,
}
fn context(org: OrganizationId, actor: UserId) -> CommandContext {
    CommandContext {
        organization_id: org,
        actor_user_id: actor,
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}
fn valid(r: &PgRow, c: &Claim) -> bool {
    r.get::<String, _>("state") == "running"
        && r.get::<Option<Uuid>, _>("lease_token") == Some(c.token)
        && r.get::<i64, _>("lease_epoch") == c.epoch
        && r.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
            .is_some_and(|v| v > Utc::now())
}
#[tracing::instrument(skip_all, fields(operation = "core_change_worker"))]
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT id,organization_id,initiated_by_user_id,lease_epoch FROM migration_core_change_report WHERE state='queued' OR (state='running' AND lease_expires_at<=clock_timestamp()) ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    let mut c = Claim {
        id: candidate.get("id"),
        org: OrganizationId::new(candidate.get("organization_id")),
        actor: UserId::new(candidate.get("initiated_by_user_id")),
        token: Uuid::new_v4(),
        epoch: candidate.get("lease_epoch"),
    };
    let ctx = context(c.org, c.actor);
    let claim_result = claim(pool, key, policy, release, &ctx, &mut c).await;
    let claimed = match claim_result {
        Ok(v) => v,
        Err(error) => {
            pause_error(pool, &c, error, true).await?;
            return Ok(true);
        }
    };
    if !claimed {
        return Ok(false);
    }
    tokio::task::yield_now().await;
    match tokio::time::timeout(
        std::time::Duration::from_secs(45),
        unit(pool, key, policy, release, &ctx, &c),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => pause_error(pool, &c, error, false).await?,
        Err(_) => pause_reason(pool, &c, "work_unit_timed_out", false).await?,
    }
    Ok(true)
}
async fn claim(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    c: &mut Claim,
) -> Result<bool, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, c.org, c.id).await?;
    if r.get::<i64, _>("lease_epoch") != c.epoch {
        return Ok(false);
    }
    if r.get::<String, _>("state") != "queued"
        && !(r.get::<String, _>("state") == "running"
            && r.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
                .is_some_and(|v| v <= Utc::now()))
    {
        return Ok(false);
    }
    s::validate(&mut tx, key, c.org, &r).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_core_change(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    // Work transactions settle their own reservations atomically. Reclaim only
    // an expired lease's leftover token, never another process's current claim.
    let old=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_core_change_reservation WHERE report_id=$1 AND organization_id=$2 AND kind=1").bind(c.id).bind(c.org.0).fetch_optional(&mut *tx).await?;
    if let Some(token) = old {
        s::release(&mut tx, c.org, c.id, token, 0).await?;
    }
    let reserve = s::reserve(&mut tx, c.org, c.id, 1, Some(c.token), 128, policy).await?;
    let before = s::row(&mut tx, c.org, c.id)
        .await?
        .get::<i64, _>("retained_bytes");
    sqlx::query("UPDATE migration_core_change_report SET state='running',pause_reason=NULL,lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',lease_epoch=lease_epoch+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(c.token).execute(&mut *tx).await?;
    let after = s::row(&mut tx, c.org, c.id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, c.org, c.id, reserve, after - before).await?;
    tx.commit().await?;
    c.epoch += 1;
    Ok(true)
}
async fn pause_error(
    pool: &PgPool,
    c: &Claim,
    error: MigrationError,
    unclaimed: bool,
) -> Result<(), MigrationError> {
    let reason = match error {
        MigrationError::Crypto => "retained_integrity_failed",
        MigrationError::StorageLimit => "storage_budget_exhausted",
        MigrationError::Forbidden => "executor_not_authorized",
        MigrationError::SourceNotEligible
        | MigrationError::SourceAccountMismatch
        | MigrationError::Conflict => "source_binding_changed",
        MigrationError::ReleaseNotReady => "release_not_ready",
        MigrationError::Database(sqlx::Error::Database(ref e))
            if e.code().is_some_and(|v| v == "57014" || v == "25P03") =>
        {
            "work_unit_timed_out"
        }
        other => return Err(other),
    };
    pause_reason(pool, c, reason, unclaimed).await
}
async fn pause_reason(
    pool: &PgPool,
    c: &Claim,
    reason: &str,
    unclaimed: bool,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    store::lock_org(&mut tx, c.org).await?;
    let r = s::row(&mut tx, c.org, c.id).await?;
    // A failed fresh admission has no lease; an in-flight failure must still own
    // its token. Cancellation or a newer worker fences both pause and accounting.
    let state = r.get::<String, _>("state");
    let owns = r.get::<i64, _>("lease_epoch") == c.epoch
        && if unclaimed {
            state == "queued"
                || (state == "running"
                    && r.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
                        .is_some_and(|v| v <= Utc::now()))
        } else {
            state == "running" && r.get::<Option<Uuid>, _>("lease_token") == Some(c.token)
        };
    if owns {
        s::pause(&mut tx, c.org, c.id, reason).await?;
        let after = s::row(&mut tx, c.org, c.id)
            .await?
            .get::<i64, _>("retained_bytes");
        let delta = after - r.get::<i64, _>("retained_bytes");
        // Pause metadata consumes its own cancellation allowance; cancellation
        // remains possible when the ordinary allowance is exhausted.
        if delta > 0 {
            let changed=sqlx::query("UPDATE migration_core_change_reservation SET byte_count=byte_count-$3 WHERE report_id=$1 AND organization_id=$2 AND kind=0 AND byte_count>$3+4096").bind(c.id).bind(c.org.0).bind(delta).execute(&mut *tx).await?.rows_affected();
            if changed != 1 {
                return Err(MigrationError::StorageLimit);
            }
            sqlx::query("UPDATE migration_core_change_report SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(delta).execute(&mut *tx).await?;
            sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("newer_snapshot_id")).bind(c.org.0).bind(delta).execute(&mut *tx).await?;
            sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(c.org.0).bind(delta).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    tracing::info!(report_id=%c.id,organization_id=%c.org.0,reason,"core change report paused");
    Ok(())
}
async fn unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    c: &Claim,
) -> Result<(), MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, c.org, c.id).await?;
    if !valid(&r, c) {
        return Ok(());
    }
    let inputs = s::validate(&mut tx, key, c.org, &r).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_core_change(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let token = s::reserve(&mut tx, c.org, c.id, 1, Some(c.token), s::UNIT, policy).await?;
    let before = s::row(&mut tx, c.org, c.id)
        .await?
        .get::<i64, _>("retained_bytes");
    if r.get::<String, _>("phase") == "capture" {
        capture_unit(&mut tx, key, c, &r).await?;
    } else {
        compare_unit(&mut tx, key, c, &r, &inputs).await?;
    }
    // Locked authority and lease are checked again after bounded computation.
    let now = s::row(&mut tx, c.org, c.id).await?;
    if now.get::<String, _>("state") != "completed" && !valid(&now, c) {
        return Err(MigrationError::Conflict);
    }
    let after = now.get::<i64, _>("retained_bytes");
    s::release(&mut tx, c.org, c.id, token, after - before).await?;
    if now.get::<String, _>("state") == "completed" {
        let cancel=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_core_change_reservation WHERE report_id=$1 AND organization_id=$2 AND kind=0").bind(c.id).bind(c.org.0).fetch_optional(&mut *tx).await?;
        if let Some(cancel) = cancel {
            s::release(&mut tx, c.org, c.id, cancel, 0).await?;
        }
    } else {
        sqlx::query("UPDATE migration_core_change_report SET state='queued',lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND lease_token=$3").bind(c.id).bind(c.org.0).bind(c.token).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
async fn capture_unit(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    r: &PgRow,
) -> Result<(), MigrationError> {
    let side = r.get::<i16, _>("capture_side");
    if side > 1 {
        return Err(MigrationError::Conflict);
    }
    let snapshot: Uuid = r.get(if side == 0 {
        "baseline_snapshot_id"
    } else {
        "newer_snapshot_id"
    });
    let sequence: i64 = r.get(if side == 0 {
        "baseline_sequence"
    } else {
        "newer_sequence"
    });
    let capture=sqlx::query("SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 ORDER BY sequence LIMIT 1").bind(snapshot).bind(c.org.0).bind(r.get::<i64,_>("capture_checkpoint")).bind(sequence).fetch_optional(&mut *conn).await?;
    let Some(cap) = capture else {
        sqlx::query("UPDATE migration_core_change_report SET capture_side=capture_side+1,capture_checkpoint=0,phase=CASE WHEN capture_side=1 THEN 'compare' ELSE phase END WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(conn).await?;
        return Ok(());
    };
    let stream_name: String = cap.get("stream");
    let capture_id: Uuid = cap.get("id");
    let mut count = 0_i64;
    if stream_name != "identity" {
        let stream = Stream::parse(&stream_name).ok_or(MigrationError::SourceNotEligible)?;
        if cap.get::<i64, _>("raw_byte_len") > source::MAX_RAW as i64 {
            return Err(MigrationError::StorageLimit);
        }
        let raw = crypto::open_snapshot(
            key,
            c.org,
            snapshot,
            capture_id,
            "capture",
            cap.get("nonce"),
            cap.get("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        if raw.len() as i64 != cap.get::<i64, _>("raw_byte_len") {
            return Err(MigrationError::Crypto);
        }
        let qualified = cap.get::<bool, _>("accepted")
            && !cap.get::<bool, _>("truncated")
            && cap.get::<i32, _>("http_status") == 200
            && cap.get::<String, _>("classification") == "success"
            && cap.get::<String, _>("representation") == stream.representation();
        let mut parsed = match source::extract(stream, &raw, key, c.org) {
            Ok(v) => v,
            Err(_) => {
                let linked = if stream == Stream::NoteDetail {
                    sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_core_change_note_key WHERE report_id=$1 AND organization_id=$2 AND side=$3 AND request_hmac=$4").bind(c.id).bind(c.org.0).bind(side).bind(cap.get::<Vec<u8>,_>("request_fingerprint")).fetch_optional(&mut *conn).await?
                } else {
                    None
                };
                vec![source::unsupported(
                    linked,
                    if stream == Stream::NoteDetail && cap.get::<i32, _>("http_status") == 404 {
                        "note_content_inaccessible"
                    } else {
                        "unsupported_capture"
                    },
                )]
            }
        };
        let stored=sqlx::query("SELECT ordinal,family,source_id,representation,semantic_hmac FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101").bind(capture_id).bind(snapshot).bind(c.org.0).fetch_all(&mut *conn).await?;
        if stored.len() > 100 || ((qualified || !stored.is_empty()) && stored.len() != parsed.len())
        {
            return Err(MigrationError::Crypto);
        }
        let unsupported = parsed
            .iter()
            .any(|v| v.reasons.iter().any(|v| v == "unsupported_capture"));
        if !qualified || unsupported {
            let col = if side == 0 {
                "baseline_uncertain"
            } else {
                "newer_uncertain"
            };
            // A restricted detail is per-ID uncertainty; it does not change
            // unrelated list enumeration. Other rejected captures do.
            if !(stream == Stream::NoteDetail && cap.get::<i32, _>("http_status") == 404) {
                sqlx::query(&format!("UPDATE migration_core_change_report SET {col}=array(SELECT DISTINCT unnest({col}||ARRAY[$3]::text[])) WHERE id=$1 AND organization_id=$2")).bind(c.id).bind(c.org.0).bind(&stream_name).execute(&mut *conn).await?;
            }
        }
        for (ordinal, observation) in parsed.iter_mut().enumerate() {
            if let Some(rec) = stored.get(ordinal) {
                if rec.get::<i32, _>("ordinal") != ordinal as i32
                    || rec.get::<String, _>("family") != stream.family().as_str()
                    || rec.get::<Option<String>, _>("source_id") != observation.source_id
                    || rec.get::<String, _>("representation") != stream.representation()
                    || rec.get::<Vec<u8>, _>("semantic_hmac") != observation.semantic
                {
                    return Err(MigrationError::Crypto);
                }
            }
            if !qualified {
                observation.reasons.push("rejected_capture".into());
            }
            if cap.get::<String, _>("representation") != stream.representation() {
                observation.reasons.push("representation_mismatch".into());
            }
            if stream == Stream::NoteDetail && observation.source_id.is_none() {
                observation.source_id=sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_core_change_note_key WHERE report_id=$1 AND organization_id=$2 AND side=$3 AND request_hmac=$4").bind(c.id).bind(c.org.0).bind(side).bind(cap.get::<Vec<u8>,_>("request_fingerprint")).fetch_optional(&mut *conn).await?;
                if observation.source_id.is_some() {
                    observation.reasons.retain(|v| v != "invalid_source_id");
                    observation.reasons.push("note_content_inaccessible".into());
                }
            }
            stage(
                conn,
                key,
                c,
                side,
                stream,
                observation,
                source::Evidence {
                    snapshot_id: snapshot,
                    capture_id,
                    ordinal: ordinal as i32,
                    side: if side == 0 {
                        "baseline".into()
                    } else {
                        "newer".into()
                    },
                    stream: stream_name.clone(),
                },
            )
            .await?;
            count += 1;
        }
    }
    sqlx::query("UPDATE migration_core_change_report SET capture_checkpoint=$3,captures_processed=captures_processed+1,observations_processed=observations_processed+$4 WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(cap.get::<i64,_>("sequence")).bind(count).execute(conn).await?;
    Ok(())
}
async fn stage(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    side: i16,
    stream: Stream,
    observation: &source::Observation,
    evidence: source::Evidence,
) -> Result<(), MigrationError> {
    let source_key = observation
        .source_id
        .clone()
        .unwrap_or_else(|| format!("invalid:{}:{}", evidence.capture_id, evidence.ordinal));
    let existing=sqlx::query("SELECT id,nonce,ciphertext FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND family=$3 AND source_key=$4").bind(c.id).bind(c.org.0).bind(stream.family().as_str()).bind(&source_key).fetch_optional(&mut *conn).await?;
    let (id, mut group) = if let Some(r) = &existing {
        (
            r.get("id"),
            s::open::<source::Group>(
                key,
                c.org,
                c.id,
                r.get("id"),
                "group",
                r.get("nonce"),
                r.get("ciphertext"),
            )?,
        )
    } else {
        (Uuid::new_v4(), source::Group::default())
    };
    // Insert the owner before its variant FK, then persist the full aggregate
    // once. All writes and byte accounting belong to this one fenced unit.
    if existing.is_none() {
        let sealed = s::seal(key, c.org, c.id, id, "group", &group)?;
        sqlx::query("INSERT INTO migration_core_change_group(id,report_id,organization_id,family,source_key,source_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(c.id).bind(c.org.0).bind(stream.family().as_str()).bind(&source_key).bind(&observation.source_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    }
    let count:i64=sqlx::query_scalar("INSERT INTO migration_core_change_variant(group_id,report_id,organization_id,side,representation,semantic_hmac) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(group_id,side,representation,semantic_hmac) DO UPDATE SET occurrences=migration_core_change_variant.occurrences+1 RETURNING occurrences").bind(id).bind(c.id).bind(c.org.0).bind(side).bind(stream.representation()).bind(observation.semantic.as_slice()).fetch_one(&mut *conn).await?;
    group.add(
        side as usize,
        stream.representation(),
        observation,
        evidence,
        count > 1,
    );
    let sealed = s::seal(key, c.org, c.id, id, "group", &group)?;
    sqlx::query("UPDATE migration_core_change_group SET nonce=$4,ciphertext=$5 WHERE id=$1 AND report_id=$2 AND organization_id=$3").bind(id).bind(c.id).bind(c.org.0).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    if stream == Stream::Notes {
        if let Some(source_id) = &observation.source_id {
            sqlx::query("INSERT INTO migration_core_change_note_key(report_id,organization_id,side,request_hmac,source_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(c.id).bind(c.org.0).bind(side).bind(source::note_request(key,c.org,source_id).as_slice()).bind(source_id).execute(conn).await?;
        }
    }
    Ok(())
}
fn increment(v: &mut Value, amount: u64) -> Result<(), MigrationError> {
    let old = v
        .as_str()
        .ok_or(MigrationError::Crypto)?
        .parse::<u64>()
        .map_err(|_| MigrationError::Crypto)?;
    *v = Value::String(
        old.checked_add(amount)
            .ok_or(MigrationError::StorageLimit)?
            .to_string(),
    );
    Ok(())
}
async fn compare_unit(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    r: &PgRow,
    inputs: &Value,
) -> Result<(), MigrationError> {
    let groups=sqlx::query("SELECT * FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id>$3) ORDER BY id LIMIT 50").bind(c.id).bind(c.org.0).bind(r.get::<Option<Uuid>,_>("group_checkpoint")).fetch_all(&mut *conn).await?;
    let mut counts: Value = s::open(
        key,
        c.org,
        c.id,
        c.id,
        "summary",
        r.get::<Option<Vec<u8>>, _>("summary_nonce")
            .as_deref()
            .ok_or(MigrationError::Crypto)?,
        r.get::<Option<Vec<u8>>, _>("summary_ciphertext")
            .as_deref()
            .ok_or(MigrationError::Crypto)?,
    )?;
    let a: Vec<String> = r.get("baseline_uncertain");
    let b: Vec<String> = r.get("newer_uncertain");
    let mut last = None;
    for row in &groups {
        let id: Uuid = row.get("id");
        let family: String = row.get("family");
        let source_id: Option<String> = row.get("source_id");
        let group: source::Group = s::open(
            key,
            c.org,
            c.id,
            id,
            "group",
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        let output = source::compare(id, &family, source_id.as_deref(), &group, inputs, [&a, &b]);
        let disposition = output["disposition"]
            .as_str()
            .ok_or(MigrationError::Crypto)?;
        increment(&mut counts["families"][&family][disposition], 1)?;
        let total = group.observations(0) + group.observations(1);
        increment(&mut counts["source_ids"], u64::from(source_id.is_some()))?;
        increment(
            &mut counts["invalid_observations"],
            if source_id.is_none() { total } else { 0 },
        )?;
        increment(&mut counts["observations"], total)?;
        increment(&mut counts["equal_repeats"], group.repeats())?;
        increment(
            &mut counts["conflicting_groups"],
            u64::from(group.conflicted()),
        )?;
        let sealed = s::seal(key, c.org, c.id, id, "output", &output)?;
        sqlx::query("UPDATE migration_core_change_group SET disposition=$4,output_nonce=$5,output_ciphertext=$6 WHERE id=$1 AND report_id=$2 AND organization_id=$3 AND disposition IS NULL").bind(id).bind(c.id).bind(c.org.0).bind(disposition).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        last = Some(id);
    }
    let sealed = s::seal(key, c.org, c.id, c.id, "summary", &counts)?;
    sqlx::query("UPDATE migration_core_change_report SET summary_nonce=$3,summary_ciphertext=$4,group_checkpoint=COALESCE($5,group_checkpoint),groups_compared=groups_compared+$6 WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(last).bind(groups.len() as i64).execute(&mut *conn).await?;
    if groups.is_empty() {
        if counts["observations"].as_str()
            != Some(&r.get::<i64, _>("observations_processed").to_string())
        {
            return Err(MigrationError::Crypto);
        }
        let sealed=sqlx::query("UPDATE migration_core_change_report SET state='completed',phase='sealed',output_revision=$3,lease_token=NULL,lease_expires_at=NULL,completed_at=clock_timestamp(),updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state='running' AND lease_token=$4 AND lease_epoch=$5 AND lease_expires_at>clock_timestamp()").bind(c.id).bind(c.org.0).bind(Uuid::new_v4()).bind(c.token).bind(c.epoch).execute(conn).await?.rows_affected();
        if sealed != 1 {
            return Err(MigrationError::Conflict);
        }
        tracing::info!(report_id=%c.id,organization_id=%c.org.0,rows=r.get::<i64,_>("groups_compared"),"core change report sealed");
    }
    Ok(())
}

/// Deterministic crash/takeover handles for integration tests; absent from
/// production builds and never accepted from an HTTP/client request.
#[cfg(feature = "test-support")]
pub struct TestClaim(Claim);
#[cfg(feature = "test-support")]
pub async fn hold_for_test(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<TestClaim, MigrationError> {
    let r = sqlx::query(
        "SELECT lease_epoch FROM migration_core_change_report WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(ctx.organization_id.0)
    .fetch_one(pool)
    .await?;
    let mut c = Claim {
        id,
        org: ctx.organization_id,
        actor: ctx.actor_user_id,
        token: Uuid::new_v4(),
        epoch: r.get("lease_epoch"),
    };
    if !claim(
        pool,
        key,
        policy,
        Some(&ReleaseReadiness::for_tests()),
        ctx,
        &mut c,
    )
    .await?
    {
        return Err(MigrationError::Conflict);
    }
    Ok(TestClaim(c))
}
#[cfg(feature = "test-support")]
pub async fn settle_for_test(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &TestClaim,
) -> Result<(), MigrationError> {
    let c = &claim.0;
    unit(
        pool,
        key,
        policy,
        Some(&ReleaseReadiness::for_tests()),
        &context(c.org, c.actor),
        c,
    )
    .await
}
#[cfg(feature = "test-support")]
pub async fn pause_for_test(pool: &PgPool, claim: &TestClaim) -> Result<(), MigrationError> {
    pause_error(pool, &claim.0, MigrationError::Crypto, false).await
}

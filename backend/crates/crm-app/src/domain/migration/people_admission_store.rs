//! Admission-owned encryption, receipts and retained-byte accounting.
use super::{crypto, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
pub const ENGINE: &str = "fub-people-admission-v1";
pub const ITEM_LIMIT: i64 = 64 * 1024 * 1024;
pub const CAPTURE_LIMIT: i64 = 16 * 1024 * 1024;
pub async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    crate::auth::workspace::shared(&mut tx, ctx.organization_id).await?;
    // Discovery is nonlocking. Commands acquire current membership authority
    // after the parent/run; replay acquires it after finding a durable receipt.
    if sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active'").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).fetch_optional(&mut *tx).await?.is_none() {
        return Err(MigrationError::Forbidden);
    }
    Ok(tx)
}
pub fn digest<T: Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    admission: Option<Uuid>,
    value: &T,
) -> Result<[u8; 32], MigrationError> {
    let bytes = serde_json::to_vec(&(
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        action,
        admission,
        value,
    ))
    .map_err(|_| MigrationError::Crypto)?;
    Ok(crypto::request_digest(
        key,
        "people-admission-request",
        &bytes,
    ))
}
pub fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    admission: Uuid,
    row: Uuid,
    purpose: &str,
    value: &T,
) -> Result<crypto::Sealed, MigrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() as i64 > ITEM_LIMIT {
        return Err(MigrationError::StorageLimit);
    }
    crypto::seal_history(
        key,
        org,
        admission,
        row,
        &format!("people-admission-{purpose}"),
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)
}
pub fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    admission: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    if nonce.len() != 24 || ciphertext.len() as i64 > ITEM_LIMIT + 16 {
        return Err(MigrationError::Crypto);
    }
    let bytes = crypto::open_history(
        key,
        org,
        admission,
        row,
        &format!("people-admission-{purpose}"),
        nonce,
        ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() as i64 > ITEM_LIMIT {
        return Err(MigrationError::Crypto);
    }
    serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)
}
pub async fn replay(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    digest: &[u8; 32],
) -> Result<Option<serde_json::Value>, MigrationError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "people-admission-request:{}:{}:{}:{}",
            ctx.organization_id.0, ctx.actor_user_id.0, action, request
        ))
        .execute(&mut *conn)
        .await?;
    let row=sqlx::query("SELECT digest,nonce,ciphertext,admission_id FROM migration_people_admission_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(&mut *conn).await?;
    let Some(row) = row else { return Ok(None) };
    store::require_admin(conn, ctx).await?;
    if row.get::<Vec<u8>, _>("digest") != digest {
        return Err(MigrationError::Conflict);
    }
    Ok(Some(open(
        key,
        ctx.organization_id,
        row.get("admission_id"),
        request,
        "receipt",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?))
}
pub async fn receipt(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    admission: Uuid,
    digest: &[u8; 32],
    value: &serde_json::Value,
) -> Result<(), MigrationError> {
    let sealed = seal(
        key,
        ctx.organization_id,
        admission,
        request,
        "receipt",
        value,
    )?;
    let bytes = (sealed.nonce.len() + sealed.ciphertext.len() + digest.len() + action.len()) as i64;
    consume_control(
        conn,
        ctx.organization_id,
        admission,
        bytes,
        action == "cancel",
    )
    .await?;
    sqlx::query("INSERT INTO migration_people_admission_receipt(organization_id,actor_user_id,action,request_id,admission_id,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(admission).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}
#[cfg(feature = "test-support")]
pub async fn measured_bytes(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<i64, MigrationError> {
    sqlx::query_scalar(r#"SELECT (
        COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)) FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(source_key)+COALESCE(octet_length(source_id),0)+octet_length(projection_nonce)+octet_length(projection_ciphertext)+octet_length(provenance_nonce)+octet_length(provenance_ciphertext)) FROM migration_people_admission_item WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_people_admission_contact WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(action)+octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_people_admission_receipt WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM person_admission_provenance WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(source_id)+octet_length(disposition)) FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT sum(octet_length(source_id)+octet_length(family)) FROM migration_import_identity WHERE admission_id=$1 AND organization_id=$2),0)
      + COALESCE((SELECT octet_length(preparation_checkpoint_key) FROM migration_people_admission WHERE id=$1 AND organization_id=$2),0)
    )::bigint"#).bind(id).bind(org.0).fetch_one(conn).await.map_err(Into::into)
}
pub async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    purpose: &str,
    lease: Option<Uuid>,
    amount: i64,
    policy: &super::snapshot::SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    if !(1..=ITEM_LIMIT).contains(&amount) || !matches!(purpose, "work" | "cancel" | "prepare") {
        return Err(MigrationError::StorageLimit);
    }
    let row=sqlx::query("SELECT a.newer_snapshot_id,s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_people_admission a JOIN migration_snapshot s ON s.id=a.newer_snapshot_id AND s.organization_id=a.organization_id JOIN migration_snapshot_storage l ON l.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$2 FOR UPDATE OF a,s,l").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let available = row
        .get::<i64, _>("run_byte_limit")
        .min(policy.run_ceiling_bytes)
        .saturating_sub(row.get("retained_bytes"))
        .saturating_sub(row.get("reserved_bytes"));
    let org_available = row
        .get::<i64, _>("byte_limit")
        .min(policy.org_ceiling_bytes)
        .saturating_sub(row.get("org_retained"))
        .saturating_sub(row.get("org_reserved"));
    if amount > available || amount > org_available {
        return Err(MigrationError::StorageLimit);
    }
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_admission_reservation(token,admission_id,organization_id,purpose,lease_token,byte_count) VALUES($1,$2,$3,$4,$5,$6)").bind(token).bind(id).bind(org.0).bind(purpose).bind(lease).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_admission SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(amount).execute(&mut *conn).await?;
    let snapshot: Uuid = row.get("newer_snapshot_id");
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(token)
}
pub async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    if actual < 0 {
        return Err(MigrationError::Crypto);
    }
    let amount:i64=sqlx::query_scalar("DELETE FROM migration_people_admission_reservation WHERE token=$1 AND admission_id=$2 AND organization_id=$3 RETURNING byte_count").bind(token).bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    if actual > amount {
        return Err(MigrationError::StorageLimit);
    }
    let snapshot:Uuid=sqlx::query_scalar("SELECT newer_snapshot_id FROM migration_people_admission WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_admission SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(amount).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(amount).bind(actual).execute(conn).await?;
    Ok(())
}

pub const CONTROL_RESERVE: i64 = 64 * 1024;
pub const CANCEL_MINIMUM: i64 = 8192;

/// Bounded parent serialization shared by every admission control/worker unit.
pub async fn lock_run(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    crate::auth::workspace::shared(conn, org).await?;
    let parent: Uuid = sqlx::query_scalar("SELECT parent_import_id FROM migration_people_admission WHERE id=$1 AND organization_id=$2")
        .bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    lock_parent(conn, org, parent).await?;
    sqlx::query(
        "SELECT * FROM migration_people_admission WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
pub async fn lock_parent(
    conn: &mut PgConnection,
    org: OrganizationId,
    parent: Uuid,
) -> Result<(), MigrationError> {
    crate::auth::workspace::shared(conn, org).await?;
    sqlx::query("SELECT id FROM migration_import WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(parent)
        .bind(org.0)
        .fetch_optional(conn)
        .await?
        .ok_or(MigrationError::NotFound)?;
    Ok(())
}

// Unlike snapshot::row, this reads sealed metadata without taking a ledger lock.
async fn boundary(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    account: i64,
) -> Result<Value, MigrationError> {
    let r = sqlx::query("SELECT capture_sequence,profile_version,schema_version,started_at,completed_at FROM migration_snapshot WHERE id=$1 AND organization_id=$2 AND source_account_id=$3 AND profile_version='fub-core-v1' AND state IN ('completed','completed_with_gaps')")
        .bind(id).bind(org.0).bind(account).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let streams=sqlx::query("SELECT stream,state,reported_total,returned_items,content_gaps,accepted_captures FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 ORDER BY stream LIMIT 10")
        .bind(id).bind(org.0).fetch_all(conn).await?;
    if streams.len() > 9
        || streams.iter().any(|s| {
            s.get::<String, _>("stream").len() > 32
                || s.get::<String, _>("state").len() > 32
                || s.get::<Option<String>, _>("reported_total")
                    .is_some_and(|v| v.len() > 128)
        })
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(
        json!({"snapshot_id":id,"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"profile_version":r.get::<String,_>("profile_version"),"schema_version":r.get::<String,_>("schema_version"),"started_at":r.get::<Option<DateTime<Utc>>,_>("started_at").ok_or(MigrationError::SourceNotEligible)?,"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at").ok_or(MigrationError::SourceNotEligible)?,"streams":streams.iter().map(|s|json!({"stream":s.get::<String,_>("stream"),"state":s.get::<String,_>("state"),"reported_total":s.get::<Option<String>,_>("reported_total"),"returned_items":s.get::<i64,_>("returned_items").to_string(),"content_gaps":s.get::<i64,_>("content_gaps").to_string(),"accepted_captures":s.get::<i64,_>("accepted_captures").to_string()})).collect::<Vec<_>>()}),
    )
}

/// Revalidate immutable evidence identities. Caller has acquired parent/run locks;
/// this function acquires no membership or byte-ledger locks.
pub async fn validate_run(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &PgRow,
) -> Result<Value, MigrationError> {
    if run.get::<String, _>("engine_version") != ENGINE {
        return Err(MigrationError::ReleaseNotReady);
    }
    let parent =
        super::history_capture_store::parent(conn, org, run.get("parent_import_id")).await?;
    if parent.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(run.get("parent_plan_id"))
        || parent.get::<Uuid, _>("snapshot_id") != run.get::<Uuid, _>("original_snapshot_id")
        || parent.get::<i64, _>("capture_sequence") != run.get::<i64, _>("original_sequence")
        || parent.get::<i64, _>("source_account_id") != run.get::<i64, _>("source_account_id")
        || parent.get::<i64, _>("workspace_revision") != run.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let report = sqlx::query("SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2 AND state='completed'")
        .bind(run.get::<Uuid,_>("report_id")).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    for col in ["parent_import_id", "parent_plan_id", "newer_snapshot_id"] {
        if report.get::<Uuid, _>(col) != run.get::<Uuid, _>(col) {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    for col in ["source_account_id", "workspace_revision", "newer_sequence"] {
        if report.get::<i64, _>(col) != run.get::<i64, _>(col) {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    if report.get::<String, _>("engine_version") != super::core_change_source::ENGINE
        || report.get::<Uuid, _>("baseline_snapshot_id")
            != run.get::<Uuid, _>("original_snapshot_id")
        || report.get::<i64, _>("baseline_sequence") != run.get::<i64, _>("original_sequence")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let inputs = super::core_change_store::inputs(key, org, &report)?;
    if super::core_change_store::tuple(
        key,
        org,
        run.get("parent_import_id"),
        run.get("parent_plan_id"),
        &inputs,
    )?
    .as_slice()
        != report.get::<Vec<u8>, _>("tuple_hmac")
    {
        return Err(MigrationError::Crypto);
    }
    let original = boundary(
        conn,
        org,
        run.get("original_snapshot_id"),
        run.get("source_account_id"),
    )
    .await?;
    let newer = boundary(
        conn,
        org,
        run.get("newer_snapshot_id"),
        run.get("source_account_id"),
    )
    .await?;
    if original != inputs["baseline"]
        || newer != inputs["newer"]
        || newer["started_at"] != json!(run.get::<DateTime<Utc>, _>("newer_started_at"))
        || newer["completed_at"] != json!(run.get::<DateTime<Utc>, _>("newer_completed_at"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let mapping_digest: Vec<u8> = sqlx::query_scalar("SELECT confirmation_digest FROM migration_import_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 AND phase='ready'")
        .bind(run.get::<Uuid,_>("parent_plan_id")).bind(run.get::<Uuid,_>("parent_import_id")).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    if mapping_digest.len() != 32 {
        return Err(MigrationError::Crypto);
    }
    Ok(
        json!({"organization_id":org.0,"parent_import_id":run.get::<Uuid,_>("parent_import_id"),"parent_plan_id":run.get::<Uuid,_>("parent_plan_id"),"workspace_revision":run.get::<i64,_>("workspace_revision").to_string(),"source_account_id":run.get::<i64,_>("source_account_id").to_string(),"report_id":run.get::<Uuid,_>("report_id"),"report_output_revision":report.get::<Option<Uuid>,_>("output_revision").ok_or(MigrationError::SourceNotEligible)?,"report_tuple_hmac":report.get::<Vec<u8>,_>("tuple_hmac"),"original":original,"newer":newer,"original_mapping_digest":mapping_digest,"engine":ENGINE}),
    )
}

/// Debit only this run's held control capacity; cancellation remains possible
/// after quota exhaustion or an initiator's departure.
async fn consume_control(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    bytes: i64,
    terminal: bool,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT token,byte_count FROM migration_people_admission_reservation WHERE admission_id=$1 AND organization_id=$2 AND purpose='cancel' FOR UPDATE")
        .bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::StorageLimit)?;
    let amount: i64 = r.get("byte_count");
    if bytes < 0 || bytes > amount || (!terminal && amount - bytes < CANCEL_MINIMUM) {
        return Err(MigrationError::StorageLimit);
    }
    let token: Uuid = r.get("token");
    if terminal {
        return release(conn, org, id, token, bytes).await;
    }
    sqlx::query(
        "UPDATE migration_people_admission_reservation SET byte_count=byte_count-$2 WHERE token=$1",
    )
    .bind(token)
    .bind(bytes)
    .execute(&mut *conn)
    .await?;
    let snapshot:Uuid=sqlx::query_scalar("UPDATE migration_people_admission SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2 RETURNING newer_snapshot_id")
        .bind(id).bind(org.0).bind(bytes).fetch_one(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2")
        .bind(snapshot).bind(org.0).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$2 WHERE organization_id=$1")
        .bind(org.0).bind(bytes).execute(conn).await?;
    Ok(())
}
pub async fn release_control(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), MigrationError> {
    let token: Option<Uuid> = sqlx::query_scalar("SELECT token FROM migration_people_admission_reservation WHERE admission_id=$1 AND organization_id=$2 AND purpose='cancel'")
        .bind(id).bind(org.0).fetch_optional(&mut *conn).await?;
    if let Some(token) = token {
        release(conn, org, id, token, 0).await?;
    }
    Ok(())
}

/// Shrink a retained checkpoint; growth needs a worker reservation first.
pub async fn checkpoint(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    value: &str,
) -> Result<(), MigrationError> {
    let old: String=sqlx::query_scalar("SELECT preparation_checkpoint_key FROM migration_people_admission WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let delta = value.len() as i64 - old.len() as i64;
    if delta > 0 {
        return Err(MigrationError::StorageLimit);
    }
    let snapshot:Uuid=sqlx::query_scalar("UPDATE migration_people_admission SET preparation_checkpoint_key=$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2 RETURNING newer_snapshot_id")
        .bind(id).bind(org.0).bind(value).bind(delta).fetch_one(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2")
        .bind(snapshot).bind(org.0).bind(delta).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+$2 WHERE organization_id=$1")
        .bind(org.0).bind(delta).execute(conn).await?;
    Ok(())
}

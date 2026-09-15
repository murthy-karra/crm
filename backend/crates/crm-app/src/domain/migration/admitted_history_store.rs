//! Authority, authenticated bindings, receipts and owned bounded reservations.
use super::{
    admitted_history_source as source, crypto, snapshot::SnapshotPolicy, store, MigrationError,
};
use crate::{
    auth::workspace, config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub(crate) const CONTROL: i64 = 8192;
pub(crate) const UNIT: i64 = 50 * super::history_import_source::RECORD_RESERVATION + 16384;

pub(crate) async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
    exclusive: bool,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    if exclusive {
        workspace::exclusive(&mut tx, ctx.organization_id).await?;
    }
    workspace::shared(&mut tx, ctx.organization_id).await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    let held: bool = sqlx::query_scalar(
        "SELECT workspace_mode='migration_review' FROM organization WHERE id=$1",
    )
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    if !held {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    Ok(tx)
}
pub(crate) async fn root(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(id).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
pub(crate) async fn plan(
    conn: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_admitted_history_plan WHERE id=$1 AND root_id=$2 AND organization_id=$3").bind(id).bind(root).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
pub(crate) async fn validate(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    r: &PgRow,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let binding = p.get::<Value, _>("source_binding");
    if p.get::<Vec<u8>, _>("binding_hmac") != source::hash(key, org, &binding)? {
        return Err(MigrationError::Crypto);
    }
    let current = source::binding(
        conn,
        org,
        r.get("admission_id"),
        r.get("history_capture_id"),
    )
    .await?;
    if current != binding {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}
pub(crate) fn revision(r: &PgRow, value: &str) -> Result<(), MigrationError> {
    if super::snapshot::decimal(value)? != r.get::<i64, _>("revision") {
        Err(MigrationError::ImportConflict)
    } else {
        Ok(())
    }
}
pub(crate) fn digest<T: Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    id: Option<Uuid>,
    cmd: &T,
) -> Result<Vec<u8>, MigrationError> {
    Ok(crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        &format!(
            "admitted-history-request-v1:{}:{action}",
            ctx.actor_user_id.0
        ),
        &serde_json::to_vec(&(id, cmd)).map_err(|_| MigrationError::Crypto)?,
    )
    .to_vec())
}
pub(crate) async fn replay(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    digest: &[u8],
) -> Result<Option<Value>, MigrationError> {
    let r=sqlx::query("SELECT * FROM migration_admitted_history_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(conn).await?;
    let Some(r) = r else {
        return Ok(None);
    };
    if r.get::<Vec<u8>, _>("input_digest") != digest {
        return Err(MigrationError::ImportConflict);
    }
    let bytes = crypto::open_history(
        key,
        ctx.organization_id,
        r.get("root_id"),
        request,
        &format!(
            "admitted-history-request-v1:{}:{action}",
            ctx.actor_user_id.0
        ),
        r.get("nonce"),
        r.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4096 {
        return Err(MigrationError::Crypto);
    }
    Ok(Some(
        serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?,
    ))
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    action: &str,
    request: Uuid,
    digest: &[u8],
    value: Value,
) -> Result<Value, MigrationError> {
    let bytes = serde_json::to_vec(&value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4096 {
        return Err(MigrationError::StorageLimit);
    }
    let sealed = crypto::seal_history(
        key,
        ctx.organization_id,
        id,
        request,
        &format!(
            "admitted-history-request-v1:{}:{action}",
            ctx.actor_user_id.0
        ),
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_admitted_history_receipt(organization_id,actor_user_id,action,request_id,root_id,input_digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(id).bind(digest).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(value)
}
pub(crate) async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    purpose: &str,
    bytes: i64,
    policy: &SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    let r=sqlx::query("SELECT r.run_byte_limit,r.retained_bytes,r.reserved_bytes,s.byte_limit,s.retained_bytes AS org_retained,s.reserved_bytes AS org_reserved,r.latest_plan_id FROM migration_admitted_history_root r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2 FOR UPDATE OF s,r").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    if bytes <= 0
        || bytes > UNIT
        || bytes
            > r.get::<i64, _>("run_byte_limit")
                .min(policy.run_ceiling_bytes)
                .saturating_sub(r.get("retained_bytes"))
                .saturating_sub(r.get("reserved_bytes"))
        || bytes
            > r.get::<i64, _>("byte_limit")
                .min(policy.org_ceiling_bytes)
                .saturating_sub(r.get("org_retained"))
                .saturating_sub(r.get("org_reserved"))
    {
        return Err(MigrationError::StorageLimit);
    }
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_history_reservation(token,root_id,plan_id,organization_id,purpose,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+interval '60 seconds')").bind(token).bind(id).bind(r.get::<Uuid,_>("latest_plan_id")).bind(org.0).bind(purpose).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_admitted_history_root SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(bytes).execute(conn).await?;
    Ok(token)
}
pub(crate) async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    purpose: &str,
    actual: i64,
) -> Result<(), MigrationError> {
    let bytes:i64=sqlx::query_scalar("WITH gone AS (DELETE FROM migration_admitted_history_reservation WHERE root_id=$1 AND organization_id=$2 AND purpose=$3 RETURNING byte_count) SELECT COALESCE(sum(byte_count),0)::bigint FROM gone").bind(id).bind(org.0).bind(purpose).fetch_one(&mut *conn).await?;
    if actual > bytes {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_admitted_history_root SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(org.0).bind(bytes).execute(conn).await?;
    Ok(())
}
pub(crate) async fn pause(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    let before = root(conn, org, id).await?.get::<i64, _>("retained_bytes");
    sqlx::query("UPDATE migration_admitted_history_root SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,admitted_at=NULL,revision=revision+1 WHERE id=$1 AND organization_id=$2 AND state NOT IN ('completed','cancelled')").bind(id).bind(org.0).bind(reason).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_admitted_history_attempt SET state='paused',lease_token=NULL,lease_expires_at=NULL WHERE root_id=$1 AND organization_id=$2 AND state IN ('queued','running')").bind(id).bind(org.0).execute(&mut *conn).await?;
    let actual = (root(conn, org, id).await?.get::<i64, _>("retained_bytes") - before).max(0);
    let work:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_admitted_history_reservation WHERE root_id=$1 AND organization_id=$2 AND purpose='work'").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    if work >= actual {
        release(conn, org, id, "work", actual).await?;
    } else {
        // Failure reporting consumes part of reserved control capacity while
        // keeping enough for the bounded cancellation receipt at a full quota.
        let changed=sqlx::query("UPDATE migration_admitted_history_reservation SET byte_count=byte_count-$3 WHERE root_id=$1 AND organization_id=$2 AND purpose='cancel' AND byte_count-$3>=4200").bind(id).bind(org.0).bind(actual).execute(&mut *conn).await?;
        if changed.rows_affected() != 1 {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query("UPDATE migration_admitted_history_root SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(actual).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(org.0).bind(actual).execute(&mut *conn).await?;
        release(conn, org, id, "work", 0).await?;
    }
    Ok(())
}
pub(crate) fn manifest_binding(
    key: &RawPayloadKey,
    org: OrganizationId,
    m: &PgRow,
) -> Result<Vec<u8>, MigrationError> {
    let v = json!({"root":m.get::<Uuid,_>("root_id"),"plan":m.get::<Uuid,_>("plan_id"),"manifest":m.get::<Uuid,_>("id"),"position":m.get::<i64,_>("position"),"capture":m.get::<Uuid,_>("capture_id"),"observation":m.get::<Uuid,_>("observation_id"),"ordinal":m.get::<i32,_>("ordinal"),"family":m.get::<String,_>("family"),"representation":m.get::<String,_>("representation"),"identity":m.get::<Option<Vec<u8>>,_>("identity_hmac"),"semantic":m.get::<Vec<u8>,_>("semantic_hmac"),"person":m.get::<Option<Uuid>,_>("person_id"),"created":m.get::<Option<chrono::DateTime<chrono::Utc>>,_>("source_created_at")});
    Ok(crypto::snapshot_hmac(
        key,
        org,
        "admitted-history-manifest-v1",
        &serde_json::to_vec(&v).map_err(|_| MigrationError::Crypto)?,
    )
    .to_vec())
}

/// Decode only the display owned by the exact admitted fact provenance.
pub(crate) async fn display(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    root: Uuid,
    plan: Uuid,
    attempt: Uuid,
    manifest: Uuid,
) -> Result<Value, MigrationError> {
    let row = sqlx::query("SELECT m.*,d.nonce,d.ciphertext FROM migration_admitted_history_manifest m JOIN migration_history_import_display d ON d.id=m.id AND d.organization_id=m.organization_id AND d.admitted_root_id=m.root_id AND d.admitted_plan_id=m.plan_id WHERE m.id=$1 AND m.root_id=$2 AND m.plan_id=$3 AND m.organization_id=$4 AND d.admitted_attempt_id=$5")
        .bind(manifest).bind(root).bind(plan).bind(org.0).bind(attempt).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    let bytes = crypto::open_history(
        key,
        org,
        root,
        manifest,
        &format!("admitted-history-v1:{root}:{plan}:{attempt}:{manifest}:display"),
        row.get("nonce"),
        row.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > super::history_import_source::DISPLAY_BYTES {
        return Err(MigrationError::Crypto);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?;
    let binding: Vec<u8> =
        serde_json::from_value(value["binding"].clone()).map_err(|_| MigrationError::Crypto)?;
    if binding != manifest_binding(key, org, &row)? || !value["metadata"].is_object() {
        return Err(MigrationError::Crypto);
    }
    Ok(value["metadata"].clone())
}
pub(crate) fn open_metadata(
    key: &RawPayloadKey,
    org: OrganizationId,
    m: &PgRow,
) -> Result<Value, MigrationError> {
    let bytes = crypto::open_history(
        key,
        org,
        m.get("root_id"),
        m.get("id"),
        "admitted-history-manifest-v1",
        m.get("nonce"),
        m.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4096
        || v["binding"] != json!(manifest_binding(key, org, m)?)
        || !v["metadata"].is_object()
    {
        return Err(MigrationError::Crypto);
    }
    Ok(v["metadata"].clone())
}

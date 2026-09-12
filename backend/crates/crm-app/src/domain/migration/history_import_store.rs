//! Authority, owned logical bytes and authenticated metadata for retained import.
use super::{
    crypto, history_capture_store as capture, history_import_source as source,
    snapshot::{decimal, SnapshotPolicy},
    store, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
pub const CONTROL: i64 = 8192;
pub const UNIT: i64 = 50 * source::RECORD_RESERVATION + 16 * 1024;
pub async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
    exclusive: bool,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    if exclusive {
        crate::auth::workspace::exclusive(&mut tx, ctx.organization_id).await?;
    }
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}
pub async fn row(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_history_import_run WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
pub async fn plan(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_history_import_plan WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .fetch_optional(conn)
        .await?
        .ok_or(MigrationError::NotFound)
}
pub fn binding(
    key: &RawPayloadKey,
    org: OrganizationId,
    p: &PgRow,
) -> Result<[u8; 32], MigrationError> {
    let value = json!({"id":p.get::<Uuid,_>("id"),"parent_import_id":p.get::<Uuid,_>("parent_import_id"),"parent_plan_id":p.get::<Uuid,_>("parent_plan_id"),"snapshot_id":p.get::<Uuid,_>("snapshot_id"),"parent_capture_sequence":p.get::<i64,_>("parent_capture_sequence"),"workspace_revision":p.get::<i64,_>("workspace_revision"),"capture_id":p.get::<Uuid,_>("capture_id"),"capture_revision":p.get::<i64,_>("capture_revision"),"capture_sequence":p.get::<i64,_>("capture_sequence"),"source_account_id":p.get::<i64,_>("source_account_id"),"source_access_user_id":p.get::<i64,_>("source_access_user_id"),"profile_version":p.get::<String,_>("profile_version"),"parser_version":p.get::<String,_>("parser_version"),"schema_version":p.get::<String,_>("schema_version"),"interpretation_version":p.get::<String,_>("interpretation_version"),"reader_version":p.get::<String,_>("reader_version"),"coverage":p.get::<Value,_>("coverage")});
    Ok(crypto::snapshot_hmac(
        key,
        org,
        "timeline-import-plan-v1",
        &serde_json::to_vec(&value).map_err(|_| MigrationError::Crypto)?,
    ))
}
pub async fn validate(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    p: &PgRow,
) -> Result<(), MigrationError> {
    if p.get::<String, _>("interpretation_version") != source::INTERPRETATION
        || p.get::<String, _>("reader_version") != source::READER
        || p.get::<Vec<u8>, _>("binding_hmac") != binding(key, org, p)?
    {
        return Err(MigrationError::Crypto);
    }
    capture::verify_parent(conn, org, p).await?;
    let c = sqlx::query(
        "SELECT * FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2 FOR SHARE",
    )
    .bind(p.get::<Uuid, _>("capture_id"))
    .bind(org.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(MigrationError::SourceNotEligible)?;
    if !capture::current_profile(&c)
        || c.get::<String, _>("state") != "completed_with_gaps"
        || c.get::<Uuid, _>("parent_import_id") != p.get::<Uuid, _>("parent_import_id")
        || c.get::<Uuid, _>("parent_plan_id") != p.get::<Uuid, _>("parent_plan_id")
        || c.get::<Uuid, _>("snapshot_id") != p.get::<Uuid, _>("snapshot_id")
        || c.get::<i64, _>("revision") != p.get::<i64, _>("capture_revision")
        || c.get::<i64, _>("capture_sequence") != p.get::<i64, _>("capture_sequence")
        || c.get::<i64, _>("source_account_id") != p.get::<i64, _>("source_account_id")
        || c.get::<i64, _>("source_user_id") != p.get::<i64, _>("source_access_user_id")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let anchored=sqlx::query("SELECT plan_id,interpretation_version,reader_version FROM migration_history_import_anchor WHERE organization_id=$1 AND parent_import_id=$2").bind(org.0).bind(p.get::<Uuid,_>("parent_import_id")).fetch_optional(conn).await?;
    if anchored.is_some_and(|a| {
        a.get::<Uuid, _>("plan_id") != p.get::<Uuid, _>("id")
            || a.get::<String, _>("interpretation_version") != source::INTERPRETATION
            || a.get::<String, _>("reader_version") != source::READER
    }) {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}
pub fn revision(r: &PgRow, v: &str) -> Result<(), MigrationError> {
    if decimal(v)? != r.get::<i64, _>("revision") {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
pub fn policy_revision(policy: &SnapshotPolicy, v: &str) -> Result<(), MigrationError> {
    if policy.revision() != v {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
pub async fn admit(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    bytes: i64,
    policy: &SnapshotPolicy,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT r.run_byte_limit,r.retained_bytes,r.reserved_bytes,s.byte_limit,s.retained_bytes AS org_retained,s.reserved_bytes AS org_reserved FROM migration_history_import_run r JOIN migration_snapshot_storage s ON s.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2 FOR UPDATE OF r,s").bind(id).bind(org.0).fetch_one(conn).await?;
    if bytes < 0
        || r.get::<i64, _>("run_byte_limit")
            .min(policy.run_ceiling_bytes)
            .saturating_sub(r.get("retained_bytes"))
            .saturating_sub(r.get("reserved_bytes"))
            < bytes
        || r.get::<i64, _>("byte_limit")
            .min(policy.org_ceiling_bytes)
            .saturating_sub(r.get("org_retained"))
            .saturating_sub(r.get("org_reserved"))
            < bytes
    {
        Err(MigrationError::StorageLimit)
    } else {
        Ok(())
    }
}
pub async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    kind: &str,
    bytes: i64,
    policy: &SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    admit(conn, org, id, bytes, policy).await?;
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_history_import_reservation(token,organization_id,run_id,kind,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,CASE WHEN $4='unit' THEN clock_timestamp()+interval '60 seconds' ELSE NULL END)").bind(token).bind(org.0).bind(id).bind(kind).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_history_import_run SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(bytes).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(bytes).execute(conn).await?;
    Ok(token)
}
pub async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let amount:i64=sqlx::query_scalar("DELETE FROM migration_history_import_reservation WHERE token=$1 AND organization_id=$2 AND run_id=$3 RETURNING byte_count").bind(token).bind(org.0).bind(id).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    if actual > amount {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_history_import_run SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(())
}
pub async fn release_kind(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    kind: &str,
    actual: i64,
) -> Result<(), MigrationError> {
    let token:Option<Uuid>=sqlx::query_scalar("SELECT token FROM migration_history_import_reservation WHERE organization_id=$1 AND run_id=$2 AND kind=$3").bind(org.0).bind(id).bind(kind).fetch_optional(&mut *conn).await?;
    if let Some(token) = token {
        release(conn, org, id, token, actual).await?;
    } else if actual > 0 {
        return Err(MigrationError::StorageLimit);
    }
    Ok(())
}
pub fn digest<T: serde::Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    operation: &str,
    id: Option<Uuid>,
    input: &T,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        "timeline-import-command-v1",
        &serde_json::to_vec(&(ctx.actor_user_id.0, operation, id, input))
            .map_err(|_| MigrationError::InvalidInput)?,
    ))
}
pub async fn replay(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    request: Uuid,
    digest: &[u8],
) -> Result<Option<Value>, MigrationError> {
    let row = sqlx::query(
        "SELECT * FROM migration_history_import_receipt WHERE organization_id=$1 AND request_id=$2",
    )
    .bind(ctx.organization_id.0)
    .bind(request)
    .fetch_optional(conn)
    .await?;
    row.map(|r| {
        if r.get::<Uuid, _>("actor_user_id") != ctx.actor_user_id.0
            || r.get::<Vec<u8>, _>("digest") != digest
        {
            return Err(MigrationError::Conflict);
        }
        let bytes = crypto::open_history(
            key,
            ctx.organization_id,
            r.get("owner_run_id"),
            request,
            "timeline-import-receipt-v1",
            r.get("nonce"),
            r.get("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)
    })
    .transpose()
}
#[allow(clippy::too_many_arguments)]
pub async fn save(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    request: Uuid,
    operation: &str,
    digest: &[u8],
    policy: &SnapshotPolicy,
    control: bool,
) -> Result<Value, MigrationError> {
    let r = row(conn, ctx.organization_id, id).await?;
    let value = json!({"import_id":id,"plan_id":r.get::<Uuid,_>("plan_id"),"revision":r.get::<i64,_>("revision").to_string(),"state":r.get::<String,_>("state")});
    let bytes = serde_json::to_vec(&value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4096 {
        return Err(MigrationError::StorageLimit);
    }
    let sealed = crypto::seal_history(
        key,
        ctx.organization_id,
        id,
        request,
        "timeline-import-receipt-v1",
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let actual = (sealed.nonce.len() + sealed.ciphertext.len() + digest.len()) as i64;
    if !control {
        admit(conn, ctx.organization_id, id, actual, policy).await?;
    }
    sqlx::query("INSERT INTO migration_history_import_receipt(organization_id,request_id,owner_run_id,actor_user_id,operation,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(request).bind(id).bind(ctx.actor_user_id.0).bind(operation).bind(digest).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    if control {
        release_kind(conn, ctx.organization_id, id, "control", actual).await?;
    }
    Ok(value)
}
pub async fn pause(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    release_kind(conn, org, id, "unit", 0).await?;
    sqlx::query("UPDATE migration_history_import_run SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,admitted_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state NOT IN ('completed','cancelled')").bind(id).bind(org.0).bind(reason).execute(conn).await?;
    Ok(())
}
pub fn manifest_binding(
    key: &RawPayloadKey,
    org: OrganizationId,
    r: &PgRow,
) -> Result<[u8; 32], MigrationError> {
    let value = json!({"id":r.get::<Uuid,_>("id"),"plan_id":r.get::<Uuid,_>("plan_id"),"position":r.get::<i64,_>("position"),"capture_id":r.get::<Uuid,_>("capture_id"),"observation_id":r.get::<Uuid,_>("observation_id"),"ordinal":r.get::<i32,_>("ordinal"),"family":r.get::<String,_>("family"),"representation":r.get::<String,_>("representation"),"identity_hmac":r.get::<Option<Vec<u8>>,_>("identity_hmac"),"semantic_hmac":r.get::<Vec<u8>,_>("semantic_hmac"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"source_created_at":r.get::<Option<DateTime<Utc>>,_>("source_created_at")});
    Ok(crypto::snapshot_hmac(
        key,
        org,
        "timeline-import-manifest-v1",
        &serde_json::to_vec(&value).map_err(|_| MigrationError::Crypto)?,
    ))
}
pub async fn display(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    plan: Uuid,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT m.*,d.nonce,d.ciphertext FROM migration_history_import_manifest m JOIN migration_history_import_display d ON d.id=m.id AND d.plan_id=m.plan_id AND d.organization_id=m.organization_id WHERE m.id=$1 AND m.plan_id=$2 AND m.organization_id=$3").bind(id).bind(plan).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    let bytes = crypto::open_history(
        key,
        org,
        plan,
        id,
        "timeline-import-display-v1",
        r.get("nonce"),
        r.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > source::DISPLAY_BYTES {
        return Err(MigrationError::Crypto);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?;
    let bound: Vec<u8> =
        serde_json::from_value(value["binding"].clone()).map_err(|_| MigrationError::Crypto)?;
    if bound != manifest_binding(key, org, &r)? || !value["metadata"].is_object() {
        return Err(MigrationError::Crypto);
    }
    Ok(value["metadata"].clone())
}
pub fn terminal(state: &str) -> bool {
    matches!(state, "completed" | "cancelled")
}

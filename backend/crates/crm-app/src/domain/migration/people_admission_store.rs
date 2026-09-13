//! Admission-owned encryption, receipts and retained-byte accounting.
use super::{crypto, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use serde::{de::DeserializeOwned, Serialize};
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
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
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
    let row=sqlx::query("SELECT digest,nonce,ciphertext,admission_id FROM migration_people_admission_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(&mut *conn).await?;
    let Some(row) = row else { return Ok(None) };
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
    sqlx::query("INSERT INTO migration_people_admission_receipt(organization_id,actor_user_id,action,request_id,admission_id,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(admission).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}
pub async fn measured_bytes(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<i64, MigrationError> {
    sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)+prepared_bytes) FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(projection_nonce)+octet_length(projection_ciphertext)+octet_length(provenance_nonce)+octet_length(provenance_ciphertext)) FROM migration_people_admission_item WHERE admission_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_people_admission_contact WHERE admission_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_people_admission_receipt WHERE admission_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM person_admission_provenance p JOIN migration_people_admission_result r ON r.id=p.result_id WHERE r.admission_id=$1 AND p.organization_id=$2),0)::bigint").bind(id).bind(org.0).fetch_one(conn).await.map_err(Into::into)
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
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(amount).bind(actual).execute(conn).await?;
    Ok(())
}

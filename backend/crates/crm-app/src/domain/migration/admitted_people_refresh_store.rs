//! Refresh-owned encryption, request receipts and administrative transaction setup.
use super::{crypto, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub const ENGINE: &str = "fub-admitted-people-refresh-v1";
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
    refresh: Option<Uuid>,
    value: &T,
) -> Result<[u8; 32], MigrationError> {
    let bytes = serde_json::to_vec(&(
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        action,
        refresh,
        value,
    ))
    .map_err(|_| MigrationError::Crypto)?;
    Ok(crypto::request_digest(
        key,
        "admitted-people-refresh-request",
        &bytes,
    ))
}
pub fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    refresh: Uuid,
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
        refresh,
        row,
        &format!("admitted-people-refresh-{purpose}"),
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)
}
pub fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    refresh: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    let bytes = crypto::open_history(
        key,
        org,
        refresh,
        row,
        &format!("admitted-people-refresh-{purpose}"),
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
    let row=sqlx::query("SELECT digest,nonce,ciphertext,refresh_id FROM migration_admitted_people_refresh_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(conn).await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<Vec<u8>, _>("digest") != digest {
        return Err(MigrationError::Conflict);
    }
    Ok(Some(open(
        key,
        ctx.organization_id,
        row.get("refresh_id"),
        request,
        "receipt",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?))
}
// The receipt envelope is explicit so the persisted request binding is visible
// at this security boundary rather than hidden in a mutable aggregate.
#[allow(clippy::too_many_arguments)]
pub async fn receipt(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    refresh: Uuid,
    digest: &[u8; 32],
    value: &serde_json::Value,
) -> Result<(), MigrationError> {
    let sealed = seal(key, ctx.organization_id, refresh, request, "receipt", value)?;
    sqlx::query("INSERT INTO migration_admitted_people_refresh_receipt(organization_id,actor_user_id,action,request_id,refresh_id,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(refresh).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}

/// Return the precise retained encrypted byte footprint owned by one refresh.
/// The snapshot ledger is charged only when a refresh reservation settles, so
/// retries cannot charge the same durable row twice.
pub async fn measured_bytes(
    conn: &mut PgConnection,
    org: OrganizationId,
    refresh: Uuid,
) -> Result<i64, MigrationError> {
    let base: i64 = sqlx::query_scalar(
        "SELECT
          COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)) FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(source_key)+COALESCE(octet_length(source_id),0)+COALESCE(octet_length(source_semantic_hmac),0)+octet_length(proposed_nonce)+octet_length(proposed_ciphertext)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)+octet_length(current_nonce)+octet_length(current_ciphertext)+octet_length(instructions_nonce)+octet_length(instructions_ciphertext)) FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM migration_admitted_people_refresh_contact WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(source_id)+octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM person_admitted_refresh_provenance WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_people_refresh_receipt WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT sum(octet_length(source_id)+octet_length(projection_nonce)+octet_length(projection_ciphertext)) FROM migration_admitted_people_refresh_baseline WHERE refresh_id=$1 AND organization_id=$2),0)
        + COALESCE((SELECT octet_length(preparation_checkpoint_key) FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2),0)",
    )
    .bind(refresh)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await
    .map_err(MigrationError::from)?;
    Ok(base
        + super::people_mapping_repair::measured_bytes(
            conn,
            super::people_mapping_repair::Owner::Admitted,
            org,
            refresh,
        )
        .await?)
}

pub async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    refresh: Uuid,
    purpose: &str,
    lease: Option<Uuid>,
    amount: i64,
    policy: &super::snapshot::SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    if !(1..=ITEM_LIMIT).contains(&amount) || !matches!(purpose, "work" | "cancel" | "prepare") {
        return Err(MigrationError::StorageLimit);
    }
    let row = sqlx::query(
        "SELECT r.newer_snapshot_id,s.run_byte_limit,s.retained_bytes,s.reserved_bytes,
                l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved
           FROM migration_admitted_people_refresh r
           JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id
           JOIN migration_snapshot_storage l ON l.organization_id=r.organization_id
          WHERE r.id=$1 AND r.organization_id=$2 FOR UPDATE OF r,s,l",
    )
    .bind(refresh)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?;
    let available = row
        .get::<i64, _>("run_byte_limit")
        .min(policy.run_ceiling_bytes)
        .saturating_sub(row.get::<i64, _>("retained_bytes"))
        .saturating_sub(row.get::<i64, _>("reserved_bytes"));
    let org_available = row
        .get::<i64, _>("byte_limit")
        .min(policy.org_ceiling_bytes)
        .saturating_sub(row.get::<i64, _>("org_retained"))
        .saturating_sub(row.get::<i64, _>("org_reserved"));
    if amount > available || amount > org_available {
        return Err(MigrationError::StorageLimit);
    }
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_people_refresh_reservation(token,refresh_id,organization_id,purpose,lease_token,byte_count) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(token).bind(refresh).bind(org.0).bind(purpose).bind(lease).bind(amount)
        .execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_admitted_people_refresh SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2")
        .bind(refresh).bind(org.0).bind(amount).execute(&mut *conn).await?;
    let snapshot: Uuid = row.get("newer_snapshot_id");
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2")
        .bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1")
        .bind(org.0).bind(amount).execute(conn).await?;
    Ok(token)
}

pub async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    refresh: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    if actual < 0 {
        return Err(MigrationError::Crypto);
    }
    let amount: i64 = sqlx::query_scalar(
        "DELETE FROM migration_admitted_people_refresh_reservation
          WHERE token=$1 AND refresh_id=$2 AND organization_id=$3 RETURNING byte_count",
    )
    .bind(token)
    .bind(refresh)
    .bind(org.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(MigrationError::Conflict)?;
    if actual > amount {
        return Err(MigrationError::StorageLimit);
    }
    let snapshot: Uuid = sqlx::query_scalar(
        "SELECT newer_snapshot_id FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2",
    )
    .bind(refresh)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE migration_admitted_people_refresh SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2")
        .bind(refresh).bind(org.0).bind(amount).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2")
        .bind(snapshot).bind(org.0).bind(amount).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1")
        .bind(org.0).bind(amount).bind(actual).execute(conn).await?;
    Ok(())
}

/// Settle a precise physical-footprint delta while releasing the reservation
/// that fenced the mutation. A checkpoint replacement can shrink the owned
/// footprint, so its debit must be propagated to the run, snapshot and Org.
pub async fn release_delta(
    conn: &mut PgConnection,
    org: OrganizationId,
    refresh: Uuid,
    token: Uuid,
    delta: i64,
) -> Result<(), MigrationError> {
    release(conn, org, refresh, token, delta.max(0)).await?;
    if delta < 0 {
        debit_retained_owner(conn, org, refresh, delta.saturating_abs()).await?;
    }
    Ok(())
}

/// Debit a physical row whose ownership is leaving this refresh. The caller
/// holds the row and current refresh fence, so the run/snapshot/Org move once.
pub async fn debit_retained_owner(
    conn: &mut PgConnection,
    org: OrganizationId,
    prior_refresh: Uuid,
    bytes: i64,
) -> Result<(), MigrationError> {
    if bytes < 0 {
        return Err(MigrationError::Crypto);
    }
    let snapshot: Uuid = sqlx::query_scalar(
        "SELECT newer_snapshot_id FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(prior_refresh)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?;
    let changed = sqlx::query(
        "UPDATE migration_admitted_people_refresh SET retained_bytes=retained_bytes-$3
          WHERE id=$1 AND organization_id=$2 AND retained_bytes >= $3",
    )
    .bind(prior_refresh)
    .bind(org.0)
    .bind(bytes)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(MigrationError::Crypto);
    }
    let changed = sqlx::query(
        "UPDATE migration_snapshot SET retained_bytes=retained_bytes-$3
          WHERE id=$1 AND organization_id=$2 AND retained_bytes >= $3",
    )
    .bind(snapshot)
    .bind(org.0)
    .bind(bytes)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(MigrationError::Crypto);
    }
    let changed = sqlx::query(
        "UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes-$2
          WHERE organization_id=$1 AND retained_bytes >= $2",
    )
    .bind(org.0)
    .bind(bytes)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(MigrationError::Crypto);
    }
    Ok(())
}

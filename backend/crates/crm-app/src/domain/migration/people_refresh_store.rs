//! Refresh-owned encryption, request receipts and administrative transaction setup.
use super::{crypto, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub const ENGINE: &str = "fub-people-refresh-v1";
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
        "people-refresh-request",
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
        &format!("people-refresh-{purpose}"),
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
        &format!("people-refresh-{purpose}"),
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
    let row=sqlx::query("SELECT digest,nonce,ciphertext,refresh_id FROM migration_people_refresh_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4")
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
    sqlx::query("INSERT INTO migration_people_refresh_receipt(organization_id,actor_user_id,action,request_id,refresh_id,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(refresh).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}

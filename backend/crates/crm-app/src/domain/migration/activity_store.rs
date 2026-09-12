//! Activity-child-owned storage and encryption. No source/parent reservation is released here.
use super::{crypto, imports, snapshot::SnapshotPolicy, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub(crate) use imports::{begin, bytes, hex, sealed_bytes, CANCEL_RESERVATION, UNIT};
tokio::task_local! { static POLICY: SnapshotPolicy; }
pub(crate) async fn with_policy<F: std::future::Future>(p: &SnapshotPolicy, f: F) -> F::Output {
    POLICY.scope(p.clone(), f).await
}
pub(crate) fn current_policy() -> Result<SnapshotPolicy, MigrationError> {
    POLICY
        .try_with(Clone::clone)
        .map_err(|_| MigrationError::StorageLimit)
}
pub(crate) fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    value: &T,
) -> Result<crypto::Sealed, MigrationError> {
    crypto::seal_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("activity-v1:{plan}:{purpose}"),
        &bytes(value)?,
    )
    .map_err(|_| MigrationError::Crypto)
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
pub(crate) fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    let raw = crypto::open_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("activity-v1:{plan}:{purpose}"),
        nonce,
        ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)
}
pub(crate) fn source_key(
    key: &RawPayloadKey,
    org: OrganizationId,
    account: i64,
    kind: &str,
    raw: &[u8],
) -> Vec<u8> {
    crypto::snapshot_hmac(key, org, &format!("activity-key-v1:{account}:{kind}"), raw).to_vec()
}
pub(crate) async fn run(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_activity_import WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
pub(crate) async fn plan(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_activity_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE").bind(id).bind(child).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
pub(crate) async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    lease: Uuid,
    token: Uuid,
    amount: i64,
    purpose: &str,
) -> Result<bool, MigrationError> {
    if !(1..=UNIT).contains(&amount) || !matches!(purpose, "work" | "cancel") {
        return Err(MigrationError::InvalidInput);
    }
    let p = current_policy()?;
    let r=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(snapshot).bind(org.0).fetch_one(&mut *conn).await?;
    if amount
        > r.get::<i64, _>("run_byte_limit")
            .min(p.run_ceiling_bytes)
            .saturating_sub(r.get("retained_bytes"))
            .saturating_sub(r.get("reserved_bytes"))
        || amount
            > r.get::<i64, _>("byte_limit")
                .min(p.org_ceiling_bytes)
                .saturating_sub(r.get("org_retained"))
                .saturating_sub(r.get("org_reserved"))
    {
        return Ok(false);
    }
    sqlx::query("INSERT INTO migration_activity_reservation(token,import_id,snapshot_id,organization_id,plan_id,lease_token,purpose,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,now()+interval '60 seconds')").bind(token).bind(child).bind(snapshot).bind(org.0).bind(plan).bind(lease).bind(purpose).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_activity_import SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).bind(amount).execute(conn).await?;
    Ok(true)
}
pub(crate) async fn settle(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    token: Uuid,
    _actual: i64,
) -> Result<(), MigrationError> {
    let actual:i64=sqlx::query_scalar("SELECT measured_bytes-retained_bytes FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).fetch_one(&mut *conn).await?;
    let r=sqlx::query("DELETE FROM migration_activity_reservation WHERE token=$1 AND import_id=$2 AND organization_id=$3 RETURNING byte_count,snapshot_id").bind(token).bind(child).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::ImportConflict)?;
    let reserved: i64 = r.get("byte_count");
    if actual > reserved {
        return Err(MigrationError::StorageLimit);
    }
    let snapshot: Uuid = r.get("snapshot_id");
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(reserved).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(reserved).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_activity_import SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).bind(reserved).bind(actual).execute(conn).await?;
    Ok(())
}
pub(crate) async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    purpose: &str,
    actual: i64,
) -> Result<(), MigrationError> {
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_activity_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose=$3 ORDER BY token").bind(child).bind(org.0).bind(purpose).fetch_all(&mut *conn).await?;
    if actual > 0 && ids.len() != 1 {
        return Err(MigrationError::ImportConflict);
    }
    for id in ids {
        settle(conn, org, child, id, actual).await?;
    }
    Ok(())
}
/// Persist a bounded pause/retry control change using already admitted capacity.
/// Refunding a cleared pause restores that same child's cancellation capacity.
pub(crate) async fn control(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT i.snapshot_id,i.measured_bytes-i.retained_bytes AS delta,r.token,r.byte_count FROM migration_activity_import i JOIN migration_activity_reservation r ON r.import_id=i.id AND r.organization_id=i.organization_id AND r.purpose='cancel' WHERE i.id=$1 AND i.organization_id=$2 FOR UPDATE OF r").bind(child).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::StorageLimit)?;
    let delta: i64 = r.get("delta");
    let available: i64 = r.get("byte_count");
    if delta >= available {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_activity_reservation SET byte_count=byte_count-$3 WHERE token=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("token")).bind(org.0).bind(delta).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_activity_import SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).bind(delta).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).bind(delta).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(delta).execute(conn).await?;
    Ok(())
}
pub(crate) async fn charge(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    _amount: i64,
) -> Result<(), MigrationError> {
    let amount:i64=sqlx::query_scalar("SELECT measured_bytes-retained_bytes FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).fetch_one(&mut *conn).await?;
    if amount <= 0 {
        sqlx::query("UPDATE migration_activity_import SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(child).bind(org.0).bind(amount).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
        return Ok(());
    }
    let t = Uuid::new_v4();
    if !reserve(conn, org, child, snapshot, plan, t, t, amount, "work").await? {
        return Err(MigrationError::StorageLimit);
    }
    settle(conn, org, child, t, amount).await
}
pub(crate) async fn replay<T: Serialize>(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    input: &T,
) -> Result<Option<Value>, MigrationError> {
    let digest = crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        &format!("activity-request:{}:{action}", ctx.actor_user_id.0),
        &bytes(input)?,
    );
    let r=sqlx::query("SELECT * FROM migration_activity_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(conn).await?;
    let Some(r) = r else { return Ok(None) };
    if r.get::<Vec<u8>, _>("input_digest") != digest {
        return Err(MigrationError::ImportConflict);
    }
    let raw = crypto::open_receipt(
        key,
        ctx.organization_id,
        request,
        &format!("activity:{}:{action}", ctx.actor_user_id.0),
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    Ok(Some(
        serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)?,
    ))
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
pub(crate) async fn receipt<T: Serialize>(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    input: &T,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    response: &mut Value,
) -> Result<(), MigrationError> {
    let digest = crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        &format!("activity-request:{}:{action}", ctx.actor_user_id.0),
        &bytes(input)?,
    );
    let pending:i64=sqlx::query_scalar("SELECT measured_bytes-retained_bytes FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(child).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    let base = response["import"]["retained_bytes"]
        .as_str()
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or(MigrationError::Crypto)?;
    let policy_base = ["run_retained_bytes", "org_retained_bytes"]
        .map(|name| {
            response["import"]["policy"][name]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or(MigrationError::Crypto)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let cancellation_remaining = response["import"]["cancellation_reserved_bytes"]
        .as_str()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    if action == "cancel" {
        response["import"]["reserved_bytes"] = serde_json::json!("0");
        response["import"]["cancellation_reserved_bytes"] = serde_json::json!("0");
        for name in ["run_reserved_bytes", "org_reserved_bytes"] {
            let before = response["import"]["policy"][name]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or(MigrationError::Crypto)?;
            response["import"]["policy"][name] = serde_json::json!(before
                .checked_sub(cancellation_remaining)
                .ok_or(MigrationError::Crypto)?
                .to_string());
        }
    }
    let mut sealed = None;
    for _ in 0..20 {
        let s = crypto::seal_receipt(
            key,
            ctx.organization_id,
            request,
            &format!("activity:{}:{action}", ctx.actor_user_id.0),
            &bytes(response)?,
        )
        .map_err(|_| MigrationError::Crypto)?;
        let actual = sealed_bytes(&s) + 32 + action.len() as i64 + 256;
        let total = base
            .checked_add(actual + pending)
            .ok_or(MigrationError::StorageLimit)?
            .to_string();
        let run = policy_base[0]
            .checked_add(actual + pending)
            .ok_or(MigrationError::StorageLimit)?
            .to_string();
        let org = policy_base[1]
            .checked_add(actual + pending)
            .ok_or(MigrationError::StorageLimit)?
            .to_string();
        if response["import"]["retained_bytes"] != total
            || response["import"]["policy"]["run_retained_bytes"] != run
            || response["import"]["policy"]["org_retained_bytes"] != org
        {
            response["import"]["retained_bytes"] = serde_json::json!(total);
            response["import"]["policy"]["run_retained_bytes"] = serde_json::json!(run);
            response["import"]["policy"]["org_retained_bytes"] = serde_json::json!(org);
            continue;
        }
        sealed = Some(s);
        break;
    }
    let s = sealed.ok_or(MigrationError::StorageLimit)?;
    let actual = sealed_bytes(&s) + 32 + action.len() as i64 + 256;
    sqlx::query("INSERT INTO migration_activity_receipt(organization_id,actor_user_id,action,request_id,import_id,input_digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(child).bind(digest.as_slice()).bind(s.nonce.as_slice()).bind(s.ciphertext).execute(&mut *conn).await?;
    if action == "cancel" {
        release(conn, ctx.organization_id, child, "cancel", actual).await?;
    } else {
        charge(conn, ctx.organization_id, child, snapshot, plan, actual).await?;
    }
    Ok(())
}

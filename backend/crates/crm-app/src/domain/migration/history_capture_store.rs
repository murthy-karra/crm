//! Private history ownership, accounting and authority. No native mutations.
use super::snapshot::SnapshotPolicy;
use super::{crypto, reader, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
pub const REQUEST_RESERVATION: i64 = 16 * 1024 * 1024;
pub const CONTROL_RESERVATION: i64 = 8192;
pub const PROFILE: &str = "fub-history-v1";
pub const PARSER: &str = "1";
pub const SCHEMA: &str = "sha256:d20b212ecf0db3e6fbbf467b272501dd0796d9dc4af7e58fd50a075ae4a8841b";

pub fn current_profile(r: &sqlx::postgres::PgRow) -> bool {
    r.get::<String, _>("profile_version") == PROFILE
        && r.get::<String, _>("parser_version") == PARSER
        && r.get::<String, _>("schema_version") == SCHEMA
}

pub async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}
pub async fn row(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
pub async fn parent(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    if sqlx::query("SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .is_none()
    {
        return Err(MigrationError::NotFound);
    }
    sqlx::query("SELECT p.*,o.workspace_revision,s.started_at AS source_started_at,s.completed_at AS source_completed_at FROM migration_import p JOIN migration_workspace w ON w.organization_id=p.organization_id AND w.import_id=p.id AND w.plan_id=p.confirmed_plan_id JOIN organization o ON o.id=p.organization_id JOIN migration_snapshot s ON s.id=p.snapshot_id AND s.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 AND p.state='completed' AND o.workspace_mode='migration_review' AND s.profile_version='fub-core-v1' AND s.state IN ('completed','completed_with_gaps') AND s.capture_sequence=p.capture_sequence AND s.source_account_id=p.source_account_id").bind(id).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)
}
pub async fn verify_parent(
    conn: &mut PgConnection,
    org: OrganizationId,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let p = parent(conn, org, r.get("parent_import_id")).await?;
    if p.get::<Uuid, _>("snapshot_id") != r.get::<Uuid, _>("snapshot_id")
        || p.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(r.get("parent_plan_id"))
        || p.get::<i64, _>("capture_sequence") != r.get::<i64, _>("parent_capture_sequence")
        || p.get::<i64, _>("source_account_id") != r.get::<i64, _>("source_account_id")
        || p.get::<i64, _>("workspace_revision") != r.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}
pub async fn connection(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    revision: i32,
) -> Result<(sqlx::postgres::PgRow, reader::Identity), MigrationError> {
    let r = sqlx::query(
        "SELECT * FROM migration_connection WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if r.get::<i32, _>("revision") != revision || r.get::<String, _>("status") != "connected" {
        return Err(MigrationError::Conflict);
    }
    let bytes = crypto::open_identity(
        key,
        org,
        id,
        r.get("identity_revision"),
        r.get("identity_nonce"),
        r.get("identity_ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let identity = super::history_capture_source::parse_identity(&bytes)
        .map_err(|_| MigrationError::Crypto)?;
    if identity.user_id.is_none_or(|v| v <= 0)
        || identity.account_id != r.get::<i64, _>("source_account_id")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok((r, identity))
}
pub async fn source_authority(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    let (_, i) = connection(
        conn,
        key,
        ctx.organization_id,
        r.get("connection_id"),
        r.get("connection_revision"),
    )
    .await?;
    if i.account_id != r.get::<i64, _>("source_account_id")
        || i.user_id != Some(r.get("source_user_id"))
    {
        return Err(MigrationError::Conflict);
    }
    verify_parent(conn, ctx.organization_id, r).await
}
pub async fn exclusive(
    conn: &mut PgConnection,
    org: OrganizationId,
    except: Uuid,
) -> Result<(), MigrationError> {
    let found=sqlx::query("SELECT 1 FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry') UNION ALL SELECT 1 FROM migration_snapshot WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry') UNION ALL SELECT 1 FROM migration_history_capture_run WHERE organization_id=$1 AND id<>$2 AND state IN ('queued','running','waiting_retry') LIMIT 1").bind(org.0).bind(except).fetch_optional(conn).await?;
    if found.is_some() {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
pub async fn receipt(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    request: Uuid,
    digest: &[u8],
) -> Result<Option<Value>, MigrationError> {
    let r = sqlx::query(
        "SELECT * FROM migration_history_receipt WHERE organization_id=$1 AND request_id=$2",
    )
    .bind(ctx.organization_id.0)
    .bind(request)
    .fetch_optional(conn)
    .await?;
    r.map(|r| {
        if r.get::<Uuid, _>("actor_user_id") != ctx.actor_user_id.0
            || r.get::<Vec<u8>, _>("digest") != digest
        {
            return Err(MigrationError::Conflict);
        }
        let b = crypto::open_history(
            key,
            ctx.organization_id,
            r.get("run_id"),
            request,
            "receipt",
            r.get("nonce"),
            r.get("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        serde_json::from_slice(&b).map_err(|_| MigrationError::Crypto)
    })
    .transpose()
}
pub fn digest<T: serde::Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    op: &str,
    id: Option<Uuid>,
    cmd: &T,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::request_digest(
        key,
        "history-command",
        &serde_json::to_vec(&(ctx.organization_id.0, ctx.actor_user_id.0, op, id, cmd))
            .map_err(|_| MigrationError::InvalidInput)?,
    ))
}
pub async fn admit(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    amount: i64,
    policy: &SnapshotPolicy,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT h.run_byte_limit,h.retained_bytes,h.reserved_bytes,s.byte_limit,s.retained_bytes AS org_retained,s.reserved_bytes AS org_reserved FROM migration_history_capture_run h JOIN migration_snapshot_storage s ON s.organization_id=h.organization_id WHERE h.id=$1 AND h.organization_id=$2 FOR UPDATE OF h,s").bind(run).bind(org.0).fetch_one(&mut *conn).await?;
    if amount < 0
        || r.get::<i64, _>("run_byte_limit")
            .min(policy.run_ceiling_bytes)
            .saturating_sub(r.get::<i64, _>("retained_bytes"))
            .saturating_sub(r.get::<i64, _>("reserved_bytes"))
            < amount
        || r.get::<i64, _>("byte_limit")
            .min(policy.org_ceiling_bytes)
            .saturating_sub(r.get::<i64, _>("org_retained"))
            .saturating_sub(r.get::<i64, _>("org_reserved"))
            < amount
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(())
}
pub async fn charge(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    amount: i64,
) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_history_capture_run SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(())
}
pub async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    kind: &str,
    amount: i64,
    policy: &SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    admit(conn, org, run, amount, policy).await?;
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_history_reservation(token,run_id,organization_id,kind,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,CASE WHEN $4='source' THEN now()+interval '60 seconds' ELSE NULL END)").bind(token).bind(run).bind(org.0).bind(kind).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(token)
}
pub async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let held=sqlx::query_scalar::<_,i64>("DELETE FROM migration_history_reservation WHERE token=$1 AND run_id=$2 AND organization_id=$3 RETURNING byte_count").bind(token).bind(run).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    if actual > held {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(held).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(org.0).bind(held).execute(&mut *conn).await?;
    charge(conn, org, run, actual).await
}
pub async fn release_kind(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    kind: &str,
) -> Result<(), MigrationError> {
    let token=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_history_reservation WHERE run_id=$1 AND organization_id=$2 AND kind=$3").bind(run).bind(org.0).bind(kind).fetch_optional(&mut *conn).await?;
    if let Some(token) = token {
        release(conn, org, run, token, 0).await?
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub async fn save(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    request: Uuid,
    op: &str,
    digest: &[u8],
    policy: &SnapshotPolicy,
    cancel: bool,
) -> Result<Value, MigrationError> {
    let r = row(conn, ctx.organization_id, run).await?;
    let value = json!({"capture_id":run,"revision":r.get::<i64,_>("revision").to_string(),"state":r.get::<String,_>("state")});
    let bytes = serde_json::to_vec(&value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4096 || op.len() > 32 {
        return Err(MigrationError::InvalidInput);
    }
    let sealed = crypto::seal_history(key, ctx.organization_id, run, request, "receipt", &bytes)
        .map_err(|_| MigrationError::Crypto)?;
    let actual = (sealed.nonce.len() + sealed.ciphertext.len() + digest.len() + op.len()) as i64;
    if cancel {
        let token=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_history_reservation WHERE run_id=$1 AND organization_id=$2 AND kind='control'").bind(run).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
        release(conn, ctx.organization_id, run, token, actual).await?;
    } else {
        admit(conn, ctx.organization_id, run, actual, policy).await?;
        charge(conn, ctx.organization_id, run, actual).await?;
    }
    sqlx::query("INSERT INTO migration_history_receipt(organization_id,request_id,run_id,actor_user_id,operation,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(request).bind(run).bind(ctx.actor_user_id.0).bind(op).bind(digest).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(value)
}
pub async fn pause(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    release_kind(conn, org, run, "source").await?;
    sqlx::query("UPDATE migration_history_capture_run SET state='paused',pause_reason=$3,identity_required=true,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2 AND state NOT IN ('cancelled','completed_with_gaps')").bind(run).bind(org.0).bind(reason).execute(conn).await?;
    Ok(())
}
pub async fn invalidate_connection(
    conn: &mut PgConnection,
    org: OrganizationId,
    connection: Uuid,
) -> Result<(), MigrationError> {
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_history_capture_run WHERE organization_id=$1 AND connection_id=$2 AND state NOT IN ('cancelled','completed_with_gaps') FOR UPDATE").bind(org.0).bind(connection).fetch_all(&mut *conn).await?;
    for id in ids {
        pause(conn, org, id, "connection_changed").await?
    }
    Ok(())
}
pub async fn parent_link(
    conn: &mut PgConnection,
    org: OrganizationId,
    r: &sqlx::postgres::PgRow,
    source: Option<&str>,
) -> Result<(&'static str, Option<Uuid>), MigrationError> {
    let Some(source) = source else {
        return Ok(("invalid_person_reference", None));
    };
    let found=sqlx::query("SELECT i.target_id,p.id AS live_id FROM migration_import_identity i JOIN migration_import_result r ON r.import_id=i.import_id AND r.organization_id=i.organization_id AND r.source_id=i.source_id AND r.person_id=i.target_id AND r.disposition IN ('imported','already_imported') LEFT JOIN person p ON p.id=i.target_id AND p.organization_id=i.organization_id WHERE i.organization_id=$1 AND i.source_account_id=$2 AND i.family='people' AND i.source_id=$3 AND i.import_id=$4 AND i.plan_id=$5").bind(org.0).bind(r.get::<i64,_>("source_account_id")).bind(source).bind(r.get::<Uuid,_>("parent_import_id")).bind(r.get::<Uuid,_>("parent_plan_id")).fetch_optional(&mut *conn).await?;
    if let Some(found) = found {
        return Ok((
            if found.get::<Option<Uuid>, _>("live_id").is_some() {
                "linked"
            } else {
                "parent_excluded"
            },
            Some(found.get("target_id")),
        ));
    }
    let excluded=sqlx::query("SELECT 1 FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id=$3").bind(r.get::<Uuid,_>("parent_plan_id")).bind(org.0).bind(source).fetch_optional(conn).await?.is_some();
    Ok((
        if excluded {
            "parent_excluded"
        } else {
            "no_parent_identity"
        },
        None,
    ))
}
pub fn terminal(state: &str) -> bool {
    matches!(state, "cancelled" | "completed_with_gaps")
}
pub fn valid_lease(r: &sqlx::postgres::PgRow, token: Uuid) -> bool {
    r.get::<String, _>("state") == "running"
        && r.get::<Option<Uuid>, _>("lease_token") == Some(token)
        && r.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
            .is_some_and(|v| v > Utc::now())
}

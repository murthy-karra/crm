//! Report-owned authority, input bindings, encryption, receipts and byte admission.
use super::{
    core_change_source as source, crypto, history_capture_source, history_capture_store,
    snapshot::{self, SnapshotPolicy},
    store, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

pub const UNIT: i64 = 4 * 1024 * 1024;
pub const CONTROL: i64 = 8192;
pub async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL idle_in_transaction_session_timeout='15s'")
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
) -> Result<PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_core_change_report WHERE organization_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(org.0)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
pub fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    report: Uuid,
    row: Uuid,
    purpose: &str,
    v: &T,
) -> Result<crypto::Sealed, MigrationError> {
    let b = serde_json::to_vec(v).map_err(|_| MigrationError::Crypto)?;
    if b.len() > 32768 {
        return Err(MigrationError::StorageLimit);
    }
    crypto::seal_history(key, org, report, row, &format!("core-change-{purpose}"), &b)
        .map_err(|_| MigrationError::Crypto)
}
pub fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    report: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    let b = crypto::open_history(
        key,
        org,
        report,
        row,
        &format!("core-change-{purpose}"),
        nonce,
        ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    if b.len() > 32768 {
        return Err(MigrationError::Crypto);
    }
    serde_json::from_slice(&b).map_err(|_| MigrationError::Crypto)
}
pub fn inputs(
    key: &RawPayloadKey,
    org: OrganizationId,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    open(
        key,
        org,
        r.get("id"),
        r.get("id"),
        "inputs",
        r.get("inputs_nonce"),
        r.get("inputs_ciphertext"),
    )
}
pub fn tuple(
    key: &RawPayloadKey,
    org: OrganizationId,
    parent: Uuid,
    plan: Uuid,
    inputs: &Value,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::request_digest(
        key,
        "core-change-inputs",
        &serde_json::to_vec(&(org.0, parent, plan, source::ENGINE, inputs))
            .map_err(|_| MigrationError::Crypto)?,
    ))
}
pub async fn boundary(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r = snapshot::row(conn, org, id).await?;
    if r.get::<String, _>("profile_version") != snapshot::PROFILE
        || !matches!(
            r.get::<String, _>("state").as_str(),
            "completed" | "completed_with_gaps"
        )
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let streams=sqlx::query("SELECT stream,state,reported_total,returned_items,content_gaps,accepted_captures FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 ORDER BY stream LIMIT 10").bind(id).bind(org.0).fetch_all(conn).await?;
    if streams.len() > 9
        || streams.iter().any(|v| {
            v.get::<String, _>("stream").len() > 32
                || v.get::<String, _>("state").len() > 32
                || v.get::<Option<String>, _>("reported_total")
                    .is_some_and(|v| v.len() > 128)
        })
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(
        json!({"snapshot_id":id,"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"profile_version":r.get::<String,_>("profile_version"),"schema_version":r.get::<String,_>("schema_version"),"started_at":r.get::<Option<DateTime<Utc>>,_>("started_at").ok_or(MigrationError::SourceNotEligible)?,"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at").ok_or(MigrationError::SourceNotEligible)?,"streams":streams.iter().map(|s|json!({"stream":s.get::<String,_>("stream"),"state":s.get::<String,_>("state"),"reported_total":s.get::<Option<String>,_>("reported_total"),"returned_items":s.get::<i64,_>("returned_items").to_string(),"content_gaps":s.get::<i64,_>("content_gaps").to_string(),"accepted_captures":s.get::<i64,_>("accepted_captures").to_string()})).collect::<Vec<_>>()}),
    )
}
async fn identity(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    account: i64,
    sequence: i64,
) -> Result<Option<i64>, MigrationError> {
    let bounds=sqlx::query("SELECT count(*) AS captures,COALESCE(sum(raw_byte_len),0)::bigint AS raw_bytes,COALESCE(sum(octet_length(ciphertext)),0)::bigint AS encrypted_bytes FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='identity' AND sequence<=$3").bind(snapshot).bind(org.0).bind(sequence).fetch_one(&mut *conn).await?;
    if bounds.get::<i64, _>("captures") > 100
        || bounds.get::<i64, _>("raw_bytes") > source::MAX_RAW as i64
        || bounds.get::<i64, _>("encrypted_bytes") > source::MAX_RAW as i64 + 1600
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let captures=sqlx::query("SELECT id,nonce,ciphertext,accepted,truncated,http_status,classification,raw_byte_len FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='identity' AND sequence<=$3 ORDER BY sequence LIMIT 101").bind(snapshot).bind(org.0).bind(sequence).fetch_all(conn).await?;
    if captures.len() > 100 {
        return Err(MigrationError::SourceNotEligible);
    }
    let mut user = None;
    let mut unknown = captures.is_empty();
    for c in captures {
        if c.get::<i64, _>("raw_byte_len") > source::MAX_RAW as i64 {
            return Err(MigrationError::SourceNotEligible);
        }
        let raw = crypto::open_snapshot(
            key,
            org,
            snapshot,
            c.get("id"),
            "capture",
            c.get("nonce"),
            c.get("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        if raw.len() as i64 != c.get::<i64, _>("raw_byte_len") {
            return Err(MigrationError::Crypto);
        }
        if !c.get::<bool, _>("accepted")
            || c.get::<bool, _>("truncated")
            || c.get::<i32, _>("http_status") != 200
        {
            unknown = true;
            continue;
        }
        let i = history_capture_source::parse_identity(&raw).map_err(|_| MigrationError::Crypto)?;
        if i.account_id != account {
            return Err(MigrationError::SourceAccountMismatch);
        }
        match i.user_id {
            Some(id) if id > 0 => {
                if user.is_some_and(|v| v != id) {
                    return Err(MigrationError::SourceNotEligible);
                }
                user = Some(id);
            }
            _ => unknown = true,
        }
    }
    Ok(if unknown { None } else { user })
}
pub async fn freeze(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    parent: Uuid,
    newer: Uuid,
) -> Result<(PgRow, Value), MigrationError> {
    let p = history_capture_store::parent(conn, org, parent).await?;
    let base: Uuid = p.get("snapshot_id");
    if newer == base {
        return Err(MigrationError::SourceNotEligible);
    }
    let n = snapshot::row(conn, org, newer).await?;
    if n.get::<i64, _>("source_account_id") != p.get::<i64, _>("source_account_id") {
        return Err(MigrationError::SourceAccountMismatch);
    }
    let baseline = boundary(conn, org, base).await?;
    let new = boundary(conn, org, newer).await?;
    if baseline["schema_version"] != new["schema_version"] {
        return Err(MigrationError::SourceNotEligible);
    }
    let first:Option<DateTime<Utc>>=sqlx::query_scalar("SELECT min(captured_at) FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2").bind(newer).bind(org.0).fetch_one(&mut *conn).await?;
    if first.is_none_or(|v| {
        v <= p
            .get::<Option<DateTime<Utc>>, _>("source_completed_at")
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    }) {
        return Err(MigrationError::SourceNotEligible);
    }
    let account = p.get("source_account_id");
    let a = identity(conn, key, org, base, account, p.get("capture_sequence")).await?;
    let b = identity(conn, key, org, newer, account, n.get("capture_sequence")).await?;
    let scope = match (a, b) {
        (Some(a), Some(b)) if a == b => "consistent_identity",
        (Some(_), Some(_)) => "changed_identity",
        _ => "unknown_identity",
    };
    let mut warnings = vec![
        "effective_access_not_proven",
        "not_atomic_snapshots",
        "absence_is_not_deletion",
        "report_is_not_cutover_readiness",
    ];
    if scope != "consistent_identity" {
        warnings.push("source_scope_changed_or_unknown");
    }
    let inputs = json!({"baseline":baseline,"newer":new,"source_account_id":account.to_string(),"workspace_revision":p.get::<i64,_>("workspace_revision").to_string(),"source_scope":scope,"warnings":warnings});
    Ok((p, inputs))
}
pub async fn validate(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    if r.get::<String, _>("engine_version") != source::ENGINE {
        return Err(MigrationError::ReleaseNotReady);
    }
    let p = history_capture_store::parent(conn, org, r.get("parent_import_id")).await?;
    if p.get::<Uuid, _>("snapshot_id") != r.get::<Uuid, _>("baseline_snapshot_id")
        || p.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(r.get("parent_plan_id"))
        || p.get::<i64, _>("capture_sequence") != r.get::<i64, _>("baseline_sequence")
        || p.get::<i64, _>("workspace_revision") != r.get::<i64, _>("workspace_revision")
        || p.get::<i64, _>("source_account_id") != r.get::<i64, _>("source_account_id")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let inputs = inputs(key, org, r)?;
    if tuple(
        key,
        org,
        r.get("parent_import_id"),
        r.get("parent_plan_id"),
        &inputs,
    )?
    .as_slice()
        != r.get::<Vec<u8>, _>("tuple_hmac")
    {
        return Err(MigrationError::Crypto);
    }
    for (name, column) in [
        ("baseline", "baseline_snapshot_id"),
        ("newer", "newer_snapshot_id"),
    ] {
        if boundary(conn, org, r.get(column)).await? != inputs[name] {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    Ok(inputs)
}
pub async fn admit(
    conn: &mut PgConnection,
    org: OrganizationId,
    snapshot: Uuid,
    amount: i64,
    policy: &SnapshotPolicy,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(snapshot).bind(org.0).fetch_one(conn).await?;
    if amount < 0
        || amount
            > r.get::<i64, _>("run_byte_limit")
                .min(policy.run_ceiling_bytes)
                .saturating_sub(r.get("retained_bytes"))
                .saturating_sub(r.get("reserved_bytes"))
        || amount
            > r.get::<i64, _>("byte_limit")
                .min(policy.org_ceiling_bytes)
                .saturating_sub(r.get("org_retained"))
                .saturating_sub(r.get("org_reserved"))
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(())
}
pub async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    kind: i16,
    lease: Option<Uuid>,
    amount: i64,
    policy: &SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    let r = row(conn, org, id).await?;
    let snapshot: Uuid = r.get("newer_snapshot_id");
    admit(conn, org, snapshot, amount, policy).await?;
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_core_change_reservation(token,report_id,organization_id,kind,lease_token,byte_count) VALUES($1,$2,$3,$4,$5,$6)").bind(token).bind(id).bind(org.0).bind(kind).bind(lease).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_core_change_report SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(amount).execute(&mut *conn).await?;
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
    let amount: i64=sqlx::query_scalar("DELETE FROM migration_core_change_reservation WHERE token=$1 AND report_id=$2 AND organization_id=$3 RETURNING byte_count").bind(token).bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    if actual > amount {
        return Err(MigrationError::StorageLimit);
    }
    let snapshot: Uuid = row(conn, org, id).await?.get("newer_snapshot_id");
    sqlx::query("UPDATE migration_core_change_report SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(())
}
pub fn digest<T: Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    op: &str,
    id: Option<Uuid>,
    cmd: &T,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::request_digest(
        key,
        "core-change-command",
        &serde_json::to_vec(&(ctx.organization_id.0, ctx.actor_user_id.0, op, id, cmd))
            .map_err(|_| MigrationError::InvalidInput)?,
    ))
}
fn bound_digest(key: &RawPayloadKey, body: &[u8], tuple: &[u8]) -> [u8; 32] {
    let mut input = body.to_vec();
    input.extend(tuple);
    crypto::request_digest(key, "core-change-bound-receipt", &input)
}
pub async fn replay(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    request: Uuid,
    digest: &[u8],
) -> Result<Option<Value>, MigrationError> {
    let r = sqlx::query(
        "SELECT c.*,r.tuple_hmac FROM migration_core_change_receipt c JOIN migration_core_change_report r ON r.id=c.report_id AND r.organization_id=c.organization_id WHERE c.organization_id=$1 AND c.request_id=$2",
    )
    .bind(ctx.organization_id.0)
    .bind(request)
    .fetch_optional(conn)
    .await?;
    r.map(|r| {
        if r.get::<Uuid, _>("actor_user_id") != ctx.actor_user_id.0
            || r.get::<Vec<u8>, _>("digest")
                != bound_digest(key, digest, &r.get::<Vec<u8>, _>("tuple_hmac"))
        {
            return Err(MigrationError::Conflict);
        }
        open(
            key,
            ctx.organization_id,
            r.get("report_id"),
            request,
            "receipt",
            r.get("nonce"),
            r.get("ciphertext"),
        )
    })
    .transpose()
}
pub async fn save(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    request: Uuid,
    op: &str,
    digest: &[u8],
) -> Result<Value, MigrationError> {
    let r = row(conn, ctx.organization_id, id).await?;
    let value = json!({"report_id":id,"state":r.get::<String,_>("state"),"inputs":inputs(key,ctx.organization_id,&r)?});
    let sealed = seal(key, ctx.organization_id, id, request, "receipt", &value)?;
    sqlx::query("INSERT INTO migration_core_change_receipt(organization_id,request_id,report_id,actor_user_id,operation,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(request).bind(id).bind(ctx.actor_user_id.0).bind(op).bind(bound_digest(key,digest,&r.get::<Vec<u8>,_>("tuple_hmac")).as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(value)
}
pub fn empty_counts() -> Value {
    json!({"families":source::FAMILIES.iter().map(|family|((*family).to_owned(),Value::Object(source::DISPOSITIONS.iter().map(|d|((*d).to_owned(),json!("0"))).collect::<serde_json::Map<_,_>>()))).collect::<serde_json::Map<_,_>>(),"source_ids":"0","invalid_observations":"0","observations":"0","equal_repeats":"0","conflicting_groups":"0"})
}
pub async fn pause(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_core_change_report SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state NOT IN ('completed','cancelled')").bind(id).bind(org.0).bind(reason).execute(conn).await?;
    Ok(())
}

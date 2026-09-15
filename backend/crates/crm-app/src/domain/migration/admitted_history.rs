//! Slice 010d3 admitted-People history boundary.
//!
//! The source capture is immutable retained evidence.  This module owns only
//! the admitted root/plan/attempt lifecycle and bounded review projections;
//! source capture and the original 010d2 owner are never rewritten.
use super::{
    admitted_history_source as source, admitted_history_store as s, snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const ENGINE: &str = "fub-admitted-history-v1";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub request_id: Uuid,
    pub admission_id: Uuid,
    pub history_capture_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Confirm {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub expected_revision: String,
    pub acknowledge_held: bool,
    pub acknowledge_coverage: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub request_id: Uuid,
    pub expected_revision: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub run_byte_limit: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Remainder {
    pub request_id: Uuid,
    pub attempt_id: Uuid,
    pub expected_revision: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub admission_id: Option<Uuid>,
    pub family: Option<String>,
    pub disposition: Option<String>,
}
impl Page {
    pub(crate) fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(25);
        if !(1..=50).contains(&n) || self.cursor.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err(MigrationError::InvalidInput);
        }
        if self
            .family
            .as_deref()
            .is_some_and(|v| !matches!(v, "events" | "calls" | "text_messages"))
            || self.disposition.as_deref().is_some_and(|v| {
                !matches!(
                    v,
                    "eligible"
                        | "equal_repeat"
                        | "excluded"
                        | "held"
                        | "imported"
                        | "already_present"
                )
            })
        {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}

pub use super::admitted_history_queries::{get, issues, list, records, remainder_view, results};

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id,operation="admitted_history_prepare"))]
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: Prepare,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let digest = s::digest(key, ctx, "prepare", None, &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "prepare", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let binding = source::binding(
        &mut tx,
        ctx.organization_id,
        cmd.admission_id,
        cmd.history_capture_id,
    )
    .await?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=$1 AND admission_id=$2 AND state NOT IN ('completed','cancelled'))").bind(ctx.organization_id.0).bind(cmd.admission_id).fetch_one(&mut *tx).await?;
    if active {
        return Err(MigrationError::ImportConflict);
    }
    let id = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let uuid = |k: &str| -> Result<Uuid, MigrationError> {
        serde_json::from_value(binding[k].clone()).map_err(|_| MigrationError::Crypto)
    };
    let number = |k: &str| -> Result<i64, MigrationError> {
        binding[k]
            .as_str()
            .ok_or(MigrationError::Crypto)?
            .parse()
            .map_err(|_| MigrationError::Crypto)
    };
    sqlx::query("INSERT INTO migration_admitted_history_root(id,organization_id,parent_import_id,parent_plan_id,admission_id,admission_plan_id,history_capture_id,capture_revision,source_account_id,source_access_user_id,workspace_revision,executor_user_id,state,phase,run_byte_limit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'preparing','preparing',$13)")
        .bind(id).bind(ctx.organization_id.0).bind(uuid("parent_import_id")?).bind(uuid("parent_plan_id")?).bind(cmd.admission_id).bind(uuid("admission_plan_id")?).bind(cmd.history_capture_id).bind(number("capture_revision")?).bind(number("source_account_id")?).bind(number("source_access_user_id")?).bind(number("workspace_revision")?).bind(ctx.actor_user_id.0).bind(policy.initial_run()).execute(&mut *tx).await?;
    let progress:serde_json::Map<String,Value>=binding["coverage"]["streams"].as_array().ok_or(MigrationError::Crypto)?.iter().map(|v|(v["family"].as_str().unwrap_or_default().to_owned(),json!({"checkpoint":0,"finished":false,"reported_total":v["reported_total"],"nonce":null,"ciphertext":null}))).collect();
    sqlx::query("INSERT INTO migration_admitted_history_plan(id,root_id,organization_id,revision,state,binding_hmac,source_binding,coverage,stream_progress) VALUES($1,$2,$3,1,'building',$4,$5,$6,$7)")
        .bind(plan).bind(id).bind(ctx.organization_id.0).bind(source::hash(key,ctx.organization_id,&binding)?.as_slice()).bind(&binding).bind(&binding["coverage"]).bind(json!(progress)).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_history_root SET latest_plan_id=$2 WHERE id=$1 AND organization_id=$3").bind(id).bind(plan).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "cancel",
        s::CONTROL,
        policy,
    )
    .await?;
    s::reserve(&mut tx, ctx.organization_id, id, "work", 16384, policy).await?;
    let v = super::admitted_history_queries::receipt(&mut tx, ctx.organization_id, id).await?;
    let value = s::save(&mut tx, key, ctx, id, "prepare", cmd.request_id, &digest, v).await?;
    let actual = s::root(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, ctx.organization_id, id, "work", actual).await?;
    tx.commit().await?;
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id,root_id=%id,operation="admitted_history_confirm"))]
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: Confirm,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, true).await?;
    let digest = s::digest(key, ctx, "confirm", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "confirm", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    s::revision(&r, &cmd.expected_revision)?;
    let p = s::plan(&mut tx, ctx.organization_id, id, cmd.plan_id).await?;
    if r.get::<String, _>("state") != "ready"
        || r.get::<Option<Uuid>, _>("latest_plan_id") != Some(cmd.plan_id)
        || p.get::<String, _>("state") != "ready"
        || !cmd.acknowledge_held
        || !cmd.acknowledge_coverage
        || p.get::<i64, _>("eligible") <= 0
    {
        return Err(MigrationError::ImportConflict);
    }
    let expired:bool=sqlx::query_scalar("SELECT expires_at<=clock_timestamp() FROM migration_admitted_history_plan WHERE id=$1 AND organization_id=$2").bind(cmd.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if expired {
        return Err(MigrationError::ImportExpired);
    }
    s::validate(&mut tx, key, ctx.organization_id, &r, &p).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_admitted_history(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    s::reserve(&mut tx, ctx.organization_id, id, "work", s::UNIT, policy).await?;
    let attempt = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_history_attempt(id,root_id,plan_id,organization_id,state,executor_user_id) VALUES($1,$2,$3,$4,'queued',$5)").bind(attempt).bind(id).bind(cmd.plan_id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_history_root SET state='queued',phase='applying',confirmed_plan_id=$2,current_attempt_id=$3,confirmed_at=clock_timestamp(),executor_user_id=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$5").bind(id).bind(cmd.plan_id).bind(attempt).bind(ctx.actor_user_id.0).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let v = finish(
        &mut tx,
        key,
        ctx,
        id,
        "confirm",
        cmd.request_id,
        &digest,
        r.get("retained_bytes"),
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id,root_id=%id,operation="admitted_history_action"))]
pub async fn action(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: Action,
    cancel: bool,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, true).await?;
    let op = if cancel { "cancel" } else { "resume" };
    let digest = s::digest(key, ctx, op, Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, op, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    s::revision(&r, &cmd.expected_revision)?;
    let state = r.get::<String, _>("state");
    if (cancel && matches!(state.as_str(), "completed" | "cancelled"))
        || (!cancel && state != "paused")
    {
        return Err(MigrationError::ImportConflict);
    }
    s::release(&mut tx, ctx.organization_id, id, "work", 0).await?;
    if !cancel {
        let p = s::plan(&mut tx, ctx.organization_id, id, r.get("latest_plan_id")).await?;
        s::validate(&mut tx, key, ctx.organization_id, &r, &p).await?;
        release
            .ok_or(MigrationError::ReleaseNotReady)?
            .require_admitted_history(&mut tx)
            .await
            .map_err(|_| MigrationError::ReleaseNotReady)?;
        s::reserve(&mut tx, ctx.organization_id, id, "work", 16384, policy).await?;
    }
    let next = if cancel {
        "cancelled"
    } else if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_none() {
        "preparing"
    } else {
        "queued"
    };
    sqlx::query("UPDATE migration_admitted_history_attempt SET state=$3,executor_user_id=$4,lease_token=NULL,lease_expires_at=NULL,revision=revision+1 WHERE root_id=$1 AND organization_id=$2 AND id=$5").bind(id).bind(ctx.organization_id.0).bind(if cancel{"cancelled"}else{"queued"}).bind(ctx.actor_user_id.0).bind(r.get::<Option<Uuid>,_>("current_attempt_id")).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_history_root SET state=$3,executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,admitted_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(next).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let v = finish(
        &mut tx,
        key,
        ctx,
        id,
        op,
        cmd.request_id,
        &digest,
        r.get("retained_bytes"),
        cancel,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id,root_id=%id,operation="admitted_history_increase_budget"))]
pub async fn increase_budget(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: Budget,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let digest = s::digest(key, ctx, "budget", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "budget", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    s::revision(&r, &cmd.expected_revision)?;
    let amount = super::snapshot::decimal(&cmd.run_byte_limit)?;
    if amount <= r.get::<i64, _>("run_byte_limit")
        || amount > policy.run_ceiling_bytes
        || matches!(
            r.get::<String, _>("state").as_str(),
            "completed" | "cancelled" | "running"
        )
    {
        return Err(MigrationError::ImportConflict);
    }
    sqlx::query("UPDATE migration_admitted_history_root SET run_byte_limit=$3,budget_revision=budget_revision+1,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(amount).execute(&mut *tx).await?;
    s::reserve(&mut tx, ctx.organization_id, id, "work", 16384, policy).await?;
    let v = finish(
        &mut tx,
        key,
        ctx,
        id,
        "budget",
        cmd.request_id,
        &digest,
        r.get("retained_bytes"),
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id,root_id=%id,operation="admitted_history_create_remainder"))]
pub async fn create_remainder(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: Remainder,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, true).await?;
    let digest = s::digest(key, ctx, "remainder", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "remainder", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    s::revision(&r, &cmd.expected_revision)?;
    if r.get::<String, _>("state") != "cancelled"
        || r.get::<Option<Uuid>, _>("current_attempt_id") != Some(cmd.attempt_id)
    {
        return Err(MigrationError::ImportConflict);
    }
    let plan = r
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .ok_or(MigrationError::ImportConflict)?;
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    s::validate(&mut tx, key, ctx.organization_id, &r, &p).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_admitted_history(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_history_remainder WHERE predecessor_attempt_id=$1 AND organization_id=$2)").bind(cmd.attempt_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if exists {
        return Err(MigrationError::ImportConflict);
    }
    let counts = r.get::<Value, _>("result_counts");
    let settled: i64 = [
        "imported",
        "already_present",
        "held",
        "excluded",
        "equal_repeat",
    ]
    .iter()
    .map(|k| counts[*k].as_i64().unwrap_or(0))
    .sum();
    let pending = p
        .get::<i64, _>("occurrences")
        .checked_sub(settled)
        .filter(|v| *v >= 0)
        .ok_or(MigrationError::Crypto)?;
    if pending == 0 {
        return Err(MigrationError::ImportConflict);
    }
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "cancel",
        s::CONTROL,
        policy,
    )
    .await?;
    s::reserve(&mut tx, ctx.organization_id, id, "work", s::UNIT, policy).await?;
    let next = Uuid::new_v4();
    // Every committed unit settles a contiguous prefix and advances this
    // cursor atomically. Carry it through the remainder chain so the next
    // bounded unit never scans all earlier settled records again.
    sqlx::query("INSERT INTO migration_admitted_history_attempt(id,root_id,plan_id,organization_id,state,executor_user_id,applied_position) SELECT $1,$2,$3,$4,'queued',$5,applied_position FROM migration_admitted_history_attempt WHERE id=$6 AND root_id=$2 AND plan_id=$3 AND organization_id=$4")
        .bind(next).bind(id).bind(plan).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.attempt_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO migration_admitted_history_remainder(id,root_id,predecessor_attempt_id,attempt_id,plan_id,organization_id,never_settled_count) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(id).bind(cmd.attempt_id).bind(next).bind(plan).bind(ctx.organization_id.0).bind(pending).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_history_root SET state='queued',current_attempt_id=$2,executor_user_id=$3,lease_token=NULL,lease_expires_at=NULL,admitted_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$4").bind(id).bind(next).bind(ctx.actor_user_id.0).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let v = finish(
        &mut tx,
        key,
        ctx,
        id,
        "remainder",
        cmd.request_id,
        &digest,
        r.get("retained_bytes"),
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
async fn finish(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    op: &str,
    request: Uuid,
    digest: &[u8],
    before: i64,
    cancel: bool,
) -> Result<Value, MigrationError> {
    let v = super::admitted_history_queries::receipt(conn, ctx.organization_id, id).await?;
    let v = s::save(conn, key, ctx, id, op, request, digest, v).await?;
    let after = s::root(conn, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(
        conn,
        ctx.organization_id,
        id,
        if cancel { "cancel" } else { "work" },
        after - before,
    )
    .await?;
    Ok(v)
}

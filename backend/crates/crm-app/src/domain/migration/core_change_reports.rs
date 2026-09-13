//! D-075 typed retained-only core change reports.
pub use super::core_change_queries::{detail, list, row_detail, rows};
use super::{
    core_change_source as source, core_change_store as s, snapshot::SnapshotPolicy, MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareCoreChangeReport {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
    pub newer_snapshot_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRequest {
    pub request_id: Uuid,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportPage {
    pub parent_import_id: Option<Uuid>,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub family: Option<String>,
    pub disposition: Option<String>,
}
impl ReportPage {
    pub fn limit(&self, max: u16) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(max);
        if n == 0
            || n > max
            || self.cursor.as_ref().is_some_and(|v| v.len() > 4096)
            || self
                .family
                .as_ref()
                .is_some_and(|v| !source::FAMILIES.contains(&v.as_str()))
            || self
                .disposition
                .as_ref()
                .is_some_and(|v| !source::DISPOSITIONS.contains(&v.as_str()))
        {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,operation="core_change_prepare"))]
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: PrepareCoreChangeReport,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "prepare", None, &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        // Receipt replay remains a current workspace read, never an authority bypass.
        let id = Uuid::parse_str(v["report_id"].as_str().ok_or(MigrationError::Crypto)?)
            .map_err(|_| MigrationError::Crypto)?;
        let r = s::row(&mut tx, ctx.organization_id, id).await?;
        s::validate(&mut tx, key, ctx.organization_id, &r).await?;
        return Ok(v);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_core_change(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let (parent, inputs) = s::freeze(
        &mut tx,
        key,
        ctx.organization_id,
        cmd.parent_import_id,
        cmd.newer_snapshot_id,
    )
    .await?;
    let plan: Uuid = parent
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    let tuple = s::tuple(
        key,
        ctx.organization_id,
        cmd.parent_import_id,
        plan,
        &inputs,
    )?;
    let existing=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_core_change_report WHERE organization_id=$1 AND parent_import_id=$2 AND newer_snapshot_id=$3 AND tuple_hmac=$4 AND state<>'cancelled'").bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(cmd.newer_snapshot_id).bind(tuple.as_slice()).fetch_optional(&mut *tx).await?;
    let id = if let Some(id) = existing {
        id
    } else {
        if sqlx::query("SELECT 1 FROM migration_core_change_report WHERE organization_id=$1 AND state IN ('queued','running','paused')").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.is_some(){return Err(MigrationError::Conflict);}
        s::admit(
            &mut tx,
            ctx.organization_id,
            cmd.newer_snapshot_id,
            64 * 1024 + s::CONTROL,
            policy,
        )
        .await?;
        let id = Uuid::new_v4();
        let sealed = s::seal(key, ctx.organization_id, id, id, "inputs", &inputs)?;
        let summary = s::seal(
            key,
            ctx.organization_id,
            id,
            id,
            "summary",
            &s::empty_counts(),
        )?;
        sqlx::query("INSERT INTO migration_core_change_report(id,organization_id,parent_import_id,parent_plan_id,baseline_snapshot_id,newer_snapshot_id,baseline_sequence,newer_sequence,source_account_id,workspace_revision,initiated_by_user_id,engine_version,tuple_hmac,inputs_nonce,inputs_ciphertext,summary_nonce,summary_ciphertext,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,'queued')").bind(id).bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(plan).bind(parent.get::<Uuid,_>("snapshot_id")).bind(cmd.newer_snapshot_id).bind(parent.get::<i64,_>("capture_sequence")).bind(inputs["newer"]["capture_sequence"].as_str().ok_or(MigrationError::Crypto)?.parse::<i64>().map_err(|_|MigrationError::Crypto)?).bind(parent.get::<i64,_>("source_account_id")).bind(parent.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(source::ENGINE).bind(tuple.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(summary.nonce.as_slice()).bind(summary.ciphertext).execute(&mut *tx).await?;
        s::reserve(
            &mut tx,
            ctx.organization_id,
            id,
            0,
            None,
            s::CONTROL,
            policy,
        )
        .await?;
        id
    };
    let token = s::reserve(&mut tx, ctx.organization_id, id, 1, None, 32768, policy).await?;
    let before = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    let v = s::save(&mut tx, key, ctx, id, cmd.request_id, "prepare", &digest).await?;
    let after = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, ctx.organization_id, id, token, after - before).await?;
    tx.commit().await?;
    Ok(v)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,report_id=%id,operation="core_change_resume"))]
pub async fn resume(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ReportRequest,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    s::validate(&mut tx, key, ctx.organization_id, &r).await?;
    let digest = s::digest(key, ctx, "resume", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    if r.get::<String, _>("state") != "paused" {
        return Err(MigrationError::Conflict);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_core_change(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let token = s::reserve(&mut tx, ctx.organization_id, id, 1, None, s::UNIT, policy).await?;
    let before = r.get::<i64, _>("retained_bytes");
    sqlx::query("UPDATE migration_core_change_report SET state='queued',pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let v = s::save(&mut tx, key, ctx, id, cmd.request_id, "resume", &digest).await?;
    let after = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, ctx.organization_id, id, token, after - before).await?;
    tx.commit().await?;
    Ok(v)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,report_id=%id,operation="core_change_cancel"))]
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ReportRequest,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    // Another current admin may cancel when the original executor lost authority.
    let digest = s::digest(key, ctx, "cancel", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    if r.get::<String, _>("state") == "completed" {
        return Err(MigrationError::Conflict);
    }
    let pending=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_core_change_reservation WHERE report_id=$1 AND organization_id=$2 AND kind=1").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    if let Some(token) = pending {
        s::release(&mut tx, ctx.organization_id, id, token, 0).await?;
    }
    let token=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_core_change_reservation WHERE report_id=$1 AND organization_id=$2 AND kind=0").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    let token = if let Some(t) = token {
        t
    } else {
        s::reserve(&mut tx, ctx.organization_id, id, 1, None, 32768, policy).await?
    };
    let before = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    sqlx::query("UPDATE migration_core_change_report SET state='cancelled',pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,completed_at=clock_timestamp(),updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state<>'cancelled'").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let v = s::save(&mut tx, key, ctx, id, cmd.request_id, "cancel", &digest).await?;
    let after = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, ctx.organization_id, id, token, after - before).await?;
    tx.commit().await?;
    Ok(v)
}

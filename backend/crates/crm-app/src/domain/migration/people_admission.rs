//! Typed command boundary for D-078 People admissions.
pub use super::people_admission_queries::{
    admission_provenance, admission_provenance_contacts, admission_provenance_field, contacts,
    detail, field, item, items, list, results, Page,
};
use super::{people_admission_store as s, snapshot::SnapshotPolicy, MigrationError};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparePeopleAdmission {
    pub request_id: Uuid,
    pub report_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepreviewPeopleAdmission {
    pub request_id: Uuid,
    pub expected_plan_revision: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPeopleAdmission {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub plan_revision: i64,
    pub plan_digest: String,
    pub eligible_count: i64,
    pub acknowledged_coverage: bool,
    pub acknowledged_mappings: bool,
    pub acknowledged_distinct_contacts: bool,
    pub acknowledged_review_hold: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecyclePeopleAdmission {
    pub request_id: Uuid,
    pub expected_lifecycle_revision: i64,
}
async fn release(
    conn: &mut sqlx::PgConnection,
    r: Option<&ReleaseReadiness>,
) -> Result<(), MigrationError> {
    r.ok_or(MigrationError::ReleaseNotReady)?
        .require_people_admission(conn)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)
}
async fn resource(
    conn: &mut sqlx::PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_people_admission WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
fn hex(value: &str) -> Result<Vec<u8>, MigrationError> {
    if value.len() != 64 {
        return Err(MigrationError::InvalidInput);
    }
    (0..32)
        .map(|n| {
            u8::from_str_radix(&value[n * 2..n * 2 + 2], 16)
                .map_err(|_| MigrationError::InvalidInput)
        })
        .collect()
}
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: PreparePeopleAdmission,
    readiness: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "prepare", None, &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "prepare", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    release(&mut tx, readiness).await?;
    let report=sqlx::query("SELECT r.*,i.state parent_state,i.confirmed_plan_id,i.snapshot_id original_snapshot_id,i.source_account_id,o.workspace_mode,o.workspace_revision,ns.started_at newer_started_at,ns.completed_at newer_completed_at FROM migration_core_change_report r JOIN migration_import i ON i.id=r.parent_import_id AND i.organization_id=r.organization_id JOIN organization o ON o.id=r.organization_id JOIN migration_snapshot ns ON ns.id=r.newer_snapshot_id AND ns.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2 FOR UPDATE").bind(cmd.report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if report.get::<String, _>("state") != "completed"
        || report.get::<String, _>("parent_state") != "completed"
        || report.get::<String, _>("workspace_mode") != "migration_review"
        || report.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(report.get("parent_plan_id"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let id = Uuid::new_v4();
    let started = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_started_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    let completed = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_completed_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    sqlx::query("INSERT INTO migration_people_admission(id,organization_id,parent_import_id,parent_plan_id,report_id,source_account_id,original_snapshot_id,original_sequence,newer_snapshot_id,newer_sequence,newer_started_at,newer_completed_at,workspace_revision,initiated_by_user_id,engine_version,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'preparing')").bind(id).bind(ctx.organization_id.0).bind(report.get::<Uuid,_>("parent_import_id")).bind(report.get::<Uuid,_>("parent_plan_id")).bind(cmd.report_id).bind(report.get::<i64,_>("source_account_id")).bind(report.get::<Uuid,_>("original_snapshot_id")).bind(report.get::<i64,_>("original_sequence")).bind(report.get::<Uuid,_>("newer_snapshot_id")).bind(report.get::<i64,_>("newer_sequence")).bind(started).bind(completed).bind(report.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(s::ENGINE).execute(&mut *tx).await?;
    let cancel = s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "cancel",
        None,
        64 * 1024,
        policy,
    )
    .await?;
    let before = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    let value = json!({"admission_id":id,"state":"preparing"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "prepare",
        cmd.request_id,
        id,
        &digest,
        &value,
    )
    .await?;
    let after = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    s::release(
        &mut tx,
        ctx.organization_id,
        id,
        cancel,
        after.saturating_sub(before),
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn repreview(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: RepreviewPeopleAdmission,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "repreview", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "repreview", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let r = resource(&mut tx, ctx.organization_id.0, id).await?;
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0
        || r.get::<String, _>("state") != "ready"
        || r.get::<i64, _>("lifecycle_revision") != cmd.expected_plan_revision
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_people_admission_plan SET state='superseded' WHERE admission_id=$1 AND organization_id=$2 AND state='ready'").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission SET state='preparing',preparation_checkpoint_key='',preparation_phase='groups',lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let v = json!({"admission_id":id,"state":"preparing"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "repreview",
        cmd.request_id,
        id,
        &digest,
        &v,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmPeopleAdmission,
    readiness: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "confirm", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "confirm", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    release(&mut tx, readiness).await?;
    let r = resource(&mut tx, ctx.organization_id.0, id).await?;
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0
        || r.get::<String, _>("state") != "ready"
    {
        return Err(MigrationError::Forbidden);
    }
    let p=sqlx::query("SELECT * FROM migration_people_admission_plan WHERE id=$1 AND admission_id=$2 AND organization_id=$3 FOR UPDATE").bind(cmd.plan_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Conflict)?;
    if p.get::<i64, _>("revision") != cmd.plan_revision
        || p.get::<String, _>("state") != "ready"
        || p.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
            .is_none_or(|x| x <= chrono::Utc::now())
        || p.get::<Vec<u8>, _>("digest") != hex(&cmd.plan_digest)?
        || p.get::<i64, _>("eligible_count") != cmd.eligible_count
        || cmd.eligible_count == 0
        || !cmd.acknowledged_coverage
        || !cmd.acknowledged_mappings
        || !cmd.acknowledged_distinct_contacts
        || !cmd.acknowledged_review_hold
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_people_admission SET state='queued',confirmed_boundary=newer_sequence,confirmed_snapshot_id=newer_snapshot_id,confirmed_completed_at=newer_completed_at,confirmed_admission_plan_id=$3,lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.plan_id).execute(&mut *tx).await?;
    let v = json!({"admission_id":id,"state":"queued"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "confirm",
        cmd.request_id,
        id,
        &digest,
        &v,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}
pub async fn retry(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecyclePeopleAdmission,
    r: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, id, cmd, "retry", r).await
}
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecyclePeopleAdmission,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, id, cmd, "cancel", None).await
}
async fn lifecycle(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecyclePeopleAdmission,
    action: &str,
    readiness: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, action, Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, action, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    if action == "retry" {
        release(&mut tx, readiness).await?
    }
    let r = resource(&mut tx, ctx.organization_id.0, id).await?;
    if r.get::<i64, _>("lifecycle_revision") != cmd.expected_lifecycle_revision {
        return Err(MigrationError::Conflict);
    }
    if action == "retry"
        && (r.get::<String, _>("state") != "paused"
            || r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0)
    {
        return Err(MigrationError::Forbidden);
    }
    let state = if action == "cancel" {
        "cancelled"
    } else {
        "queued"
    };
    sqlx::query("UPDATE migration_people_admission SET state=$3,lifecycle_revision=lifecycle_revision+1,lease_token=NULL,lease_expires_at=NULL,cancelled_at=CASE WHEN $3='cancelled' THEN clock_timestamp() ELSE cancelled_at END WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(state).execute(&mut *tx).await?;
    let v = json!({"admission_id":id,"state":state});
    s::receipt(&mut tx, key, ctx, action, cmd.request_id, id, &digest, &v).await?;
    tx.commit().await?;
    Ok(v)
}

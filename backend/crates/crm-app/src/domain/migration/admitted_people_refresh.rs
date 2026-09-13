//! Typed command boundary for D-076 Admitted Admitted People refreshes.
pub use super::admitted_people_refresh_queries::{
    contacts, detail, field, item, items, list, results, Page,
};
use super::{admitted_people_refresh_store as s, snapshot::SnapshotPolicy, MigrationError};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareAdmittedPeopleRefresh {
    pub request_id: Uuid,
    pub admission_id: Uuid,
    pub report_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepreviewAdmittedPeopleRefresh {
    pub request_id: Uuid,
    pub expected_plan_revision: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmAdmittedPeopleRefresh {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub plan_revision: i64,
    pub plan_digest: String,
    pub acknowledged_coverage: bool,
    pub acknowledged_exclusions: bool,
    pub acknowledged_name_clears: i64,
    pub acknowledged_assignment_clears: i64,
    pub acknowledged_contact_removals: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleAdmittedPeopleRefresh {
    pub request_id: Uuid,
    pub expected_lifecycle_revision: i64,
}

async fn release(
    conn: &mut sqlx::PgConnection,
    release: Option<&ReleaseReadiness>,
) -> Result<(), MigrationError> {
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_admitted_people_refresh(conn)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)
}
async fn resource(
    conn: &mut sqlx::PgConnection,
    org: uuid::Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query(
        "SELECT * FROM migration_admitted_people_refresh WHERE organization_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(MigrationError::NotFound)
}
fn decode_digest(value: &str) -> Result<Vec<u8>, MigrationError> {
    if value.len() != 64 {
        return Err(MigrationError::InvalidInput);
    }
    (0..32)
        .map(|i| {
            u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
                .map_err(|_| MigrationError::InvalidInput)
        })
        .collect()
}

pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: PrepareAdmittedPeopleRefresh,
    release_evidence: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "prepare", None, &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "prepare", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    release(&mut tx, release_evidence).await?;
    let report=sqlx::query("SELECT r.*,a.id AS admission_id,a.state AS admission_state,a.parent_import_id AS admission_parent_import_id,a.parent_plan_id AS admission_parent_plan_id,a.source_account_id AS admission_account,a.workspace_revision AS admission_workspace_revision,a.confirmed_admission_plan_id,a.newer_completed_at AS admission_completed_at,i.state AS parent_state,i.confirmed_plan_id,i.snapshot_id AS parent_snapshot,i.source_account_id AS parent_account,o.workspace_mode,o.workspace_revision,ns.started_at AS newer_started_at,ns.completed_at AS newer_completed_at FROM migration_core_change_report r JOIN migration_people_admission a ON a.id=$1 AND a.organization_id=r.organization_id JOIN migration_import i ON i.id=r.parent_import_id AND i.organization_id=r.organization_id JOIN organization o ON o.id=r.organization_id JOIN migration_snapshot ns ON ns.id=r.newer_snapshot_id AND ns.organization_id=r.organization_id WHERE r.id=$2 AND r.organization_id=$3 FOR UPDATE")
  .bind(cmd.admission_id)
  .bind(cmd.report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if report.get::<String, _>("state") != "completed"
        || !matches!(report.get::<String, _>("admission_state").as_str(), "completed" | "cancelled")
        || report.get::<Option<Uuid>, _>("confirmed_admission_plan_id").is_none()
        || report.get::<String, _>("parent_state") != "completed"
        || report.get::<String, _>("workspace_mode") != "migration_review"
        || report.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(report.get("parent_plan_id"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let newer_started = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_started_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    let newer_completed = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_completed_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    if newer_started <= report.get::<chrono::DateTime<chrono::Utc>, _>("admission_completed_at") {
        return Err(MigrationError::SourceNotEligible);
    }
    let successful: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND disposition='settled'").bind(cmd.admission_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if successful == 0 || report.get::<Uuid,_>("parent_import_id") != report.get::<Uuid,_>("admission_parent_import_id") || report.get::<Uuid,_>("parent_plan_id") != report.get::<Uuid,_>("admission_parent_plan_id") || report.get::<i64,_>("source_account_id") != report.get::<i64,_>("admission_account") || report.get::<i64,_>("workspace_revision") != report.get::<i64,_>("admission_workspace_revision") { return Err(MigrationError::SourceNotEligible); }
    let prior=sqlx::query("SELECT confirmed_snapshot_id,confirmed_completed_at FROM migration_admitted_people_refresh WHERE organization_id=$1 AND admission_id=$2 AND confirmed_snapshot_id IS NOT NULL AND confirmed_completed_at IS NOT NULL ORDER BY confirmed_completed_at DESC,id DESC LIMIT 1 FOR SHARE")
        .bind(ctx.organization_id.0).bind(cmd.admission_id).fetch_optional(&mut *tx).await?;
    if prior.is_some_and(|prior| {
        prior.get::<Uuid, _>("confirmed_snapshot_id") != report.get::<Uuid, _>("newer_snapshot_id")
            && newer_started
                <= prior.get::<chrono::DateTime<chrono::Utc>, _>("confirmed_completed_at")
    }) {
        return Err(MigrationError::Conflict);
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_people_refresh(id,organization_id,admission_id,parent_import_id,parent_plan_id,report_id,source_account_id,newer_snapshot_id,newer_sequence,newer_started_at,newer_completed_at,workspace_revision,initiated_by_user_id,engine_version,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'preparing')")
  .bind(id).bind(ctx.organization_id.0).bind(cmd.admission_id).bind(report.get::<Uuid,_>("parent_import_id")).bind(report.get::<Uuid,_>("parent_plan_id")).bind(cmd.report_id).bind(report.get::<i64,_>("source_account_id")).bind(report.get::<Uuid,_>("newer_snapshot_id")).bind(report.get::<i64,_>("newer_sequence")).bind(newer_started).bind(newer_completed).bind(report.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(s::ENGINE).execute(&mut *tx).await?;
    // Preserve an independent cancellation allowance before any preparation
    // bytes are admitted, so cancellation remains possible at a full ledger.
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "cancel",
        None,
        64 * 1024,
        policy,
    )
    .await?;
    let receipt_before = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    let receipt_reservation = s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "prepare",
        None,
        8 * 1024,
        policy,
    )
    .await?;
    let value = json!({"refresh_id":id,"state":"preparing"});
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
    let receipt_after = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    s::release(
        &mut tx,
        ctx.organization_id,
        id,
        receipt_reservation,
        receipt_after.saturating_sub(receipt_before),
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
    cmd: RepreviewAdmittedPeopleRefresh,
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
    };
    let receipt_before = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    let receipt_reservation = s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "prepare",
        None,
        8 * 1024,
        &SnapshotPolicy::default(),
    )
    .await?;
    let superseded = sqlx::query("UPDATE migration_admitted_people_refresh_plan SET state='superseded' WHERE refresh_id=$1 AND organization_id=$2 AND state='ready'")
        .bind(id)
        .bind(ctx.organization_id.0)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if superseded != 1 {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_admitted_people_refresh SET state='preparing',preparation_phase='imported',preparation_checkpoint_key='',preparation_checkpoint_id=NULL,lifecycle_revision=lifecycle_revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = json!({"refresh_id":id,"state":"preparing"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "repreview",
        cmd.request_id,
        id,
        &digest,
        &value,
    )
    .await?;
    let receipt_after = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    s::release(
        &mut tx,
        ctx.organization_id,
        id,
        receipt_reservation,
        receipt_after.saturating_sub(receipt_before),
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmAdmittedPeopleRefresh,
    release_evidence: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "confirm", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "confirm", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    release(&mut tx, release_evidence).await?;
    let r = resource(&mut tx, ctx.organization_id.0, id).await?;
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0
        || r.get::<String, _>("state") != "ready"
    {
        return Err(MigrationError::Forbidden);
    }
    let p=sqlx::query("SELECT * FROM migration_admitted_people_refresh_plan WHERE id=$1 AND refresh_id=$2 AND organization_id=$3 FOR UPDATE").bind(cmd.plan_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Conflict)?;
    if p.get::<i64, _>("revision") != cmd.plan_revision
        || p.get::<String, _>("state") != "ready"
        || p.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
            .is_none_or(|expires| expires <= chrono::Utc::now())
        || p.get::<Vec<u8>, _>("digest") != decode_digest(&cmd.plan_digest)?
        || !cmd.acknowledged_coverage
        || !cmd.acknowledged_exclusions
        || p.get::<i64, _>("name_clear_count") != cmd.acknowledged_name_clears
        || p.get::<i64, _>("assignment_clear_count") != cmd.acknowledged_assignment_clears
        || p.get::<i64, _>("contact_removal_count") != cmd.acknowledged_contact_removals
    {
        return Err(MigrationError::Conflict);
    }
    if p.get::<i64, _>("eligible_count") == 0 {
        return Err(MigrationError::Conflict);
    }
    let receipt_before = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    let receipt_reservation = s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "prepare",
        None,
        8 * 1024,
        &SnapshotPolicy::default(),
    )
    .await?;
    sqlx::query("UPDATE migration_admitted_people_refresh SET state='queued',confirmed_boundary=newer_sequence,confirmed_snapshot_id=newer_snapshot_id,confirmed_started_at=newer_started_at,confirmed_completed_at=newer_completed_at,confirmed_refresh_plan_id=$3,lifecycle_revision=lifecycle_revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.plan_id).execute(&mut *tx).await?;
    let value = json!({"refresh_id":id,"state":"queued"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "confirm",
        cmd.request_id,
        id,
        &digest,
        &value,
    )
    .await?;
    let receipt_after = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
    s::release(
        &mut tx,
        ctx.organization_id,
        id,
        receipt_reservation,
        receipt_after.saturating_sub(receipt_before),
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn retry(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecycleAdmittedPeopleRefresh,
    release_evidence: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, id, cmd, "retry", release_evidence).await
}
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecycleAdmittedPeopleRefresh,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, id, cmd, "cancel", None).await
}
async fn lifecycle(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: LifecycleAdmittedPeopleRefresh,
    action: &str,
    release_evidence: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, action, Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, action, cmd.request_id, &digest).await? {
        return Ok(v);
    };
    if action == "retry" {
        release(&mut tx, release_evidence).await?
    };
    let r = resource(&mut tx, ctx.organization_id.0, id).await?;
    let cancellation_before = if action == "cancel" {
        Some(s::measured_bytes(&mut tx, ctx.organization_id, id).await?)
    } else {
        None
    };
    if r.get::<i64, _>("lifecycle_revision") != cmd.expected_lifecycle_revision {
        return Err(MigrationError::Conflict);
    };
    if action == "retry"
        && (r.get::<String, _>("state") != "paused"
            || r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0)
    {
        return Err(MigrationError::Forbidden);
    };
    let state = if action == "cancel" {
        "cancelled"
    } else {
        "queued"
    };
    sqlx::query("UPDATE migration_admitted_people_refresh SET state=$3,lifecycle_revision=lifecycle_revision+1,lease_token=NULL,lease_expires_at=NULL,cancelled_at=CASE WHEN $3='cancelled' THEN clock_timestamp() ELSE cancelled_at END,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(state).execute(&mut *tx).await?;
    let value = json!({"refresh_id":id,"state":state});
    s::receipt(
        &mut tx,
        key,
        ctx,
        action,
        cmd.request_id,
        id,
        &digest,
        &value,
    )
    .await?;
    if let Some(before) = cancellation_before {
        let token = sqlx::query_scalar::<_, Uuid>(
            "SELECT token FROM migration_admitted_people_refresh_reservation WHERE refresh_id=$1 AND organization_id=$2 AND purpose='cancel' FOR UPDATE",
        )
        .bind(id)
        .bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::Conflict)?;
        let after = s::measured_bytes(&mut tx, ctx.organization_id, id).await?;
        s::release(
            &mut tx,
            ctx.organization_id,
            id,
            token,
            after.saturating_sub(before),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(value)
}

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
    s::lock_run(conn, crate::ids::OrganizationId(org), id).await
}
fn hex(value: &str) -> Result<Vec<u8>, MigrationError> {
    if value.len() != 64 || !value.is_ascii() {
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
    let parent: Uuid = sqlx::query_scalar("SELECT parent_import_id FROM migration_core_change_report WHERE id=$1 AND organization_id=$2")
        .bind(cmd.report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    s::lock_parent(&mut tx, ctx.organization_id, parent).await?;
    let report=sqlx::query("SELECT r.*,i.state parent_state,i.confirmed_plan_id,i.snapshot_id original_snapshot_id,o.workspace_mode,os.capture_sequence original_sequence,ns.started_at newer_started_at,ns.completed_at newer_completed_at FROM migration_core_change_report r JOIN migration_import i ON i.id=r.parent_import_id AND i.organization_id=r.organization_id JOIN organization o ON o.id=r.organization_id JOIN migration_snapshot os ON os.id=i.snapshot_id AND os.organization_id=i.organization_id JOIN migration_snapshot ns ON ns.id=r.newer_snapshot_id AND ns.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2").bind(cmd.report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
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
    let run = resource(&mut tx, ctx.organization_id.0, id).await?;
    let inputs = s::validate_run(&mut tx, key, ctx.organization_id, &run).await?;
    check_boundary(&mut tx, key, ctx.organization_id, &run, &inputs).await?;
    super::store::require_admin(&mut tx, ctx).await?;
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "cancel",
        None,
        s::CONTROL_RESERVE,
        policy,
    )
    .await?;
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
    {
        return Err(MigrationError::Conflict);
    }
    s::validate_run(&mut tx, key, ctx.organization_id, &r).await?;
    let plan = sqlx::query("SELECT id,revision FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1 FOR UPDATE")
        .bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Conflict)?;
    if plan.get::<i64, _>("revision") != cmd.expected_plan_revision {
        return Err(MigrationError::Conflict);
    }
    super::store::require_admin(&mut tx, ctx).await?;
    sqlx::query("UPDATE migration_people_admission_plan SET state='superseded' WHERE id=$1 AND organization_id=$2 AND state='ready'")
        .bind(plan.get::<Uuid,_>("id")).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    s::checkpoint(&mut tx, ctx.organization_id, id, "").await?;
    sqlx::query("UPDATE migration_people_admission SET state='preparing',preparation_phase='original_people',lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
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
        || cmd.eligible_count <= 0
        || !cmd.acknowledged_coverage
        || !cmd.acknowledged_mappings
        || !cmd.acknowledged_distinct_contacts
        || !cmd.acknowledged_review_hold
    {
        return Err(MigrationError::Conflict);
    }
    let inputs = validate_plan(&mut tx, key, ctx.organization_id, &r, &p).await?;
    check_boundary(&mut tx, key, ctx.organization_id, &r, &inputs).await?;
    super::store::require_admin(&mut tx, ctx).await?;
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
    if action == "cancel"
        && matches!(
            r.get::<String, _>("state").as_str(),
            "completed" | "cancelled"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let state = if action == "cancel" {
        "cancelled"
    } else if r
        .get::<Option<Uuid>, _>("confirmed_admission_plan_id")
        .is_some()
    {
        "queued"
    } else {
        "preparing"
    };
    if action == "retry" {
        s::validate_run(&mut tx, key, ctx.organization_id, &r).await?;
        if let Some(plan) = r.get::<Option<Uuid>, _>("confirmed_admission_plan_id") {
            let p=sqlx::query("SELECT * FROM migration_people_admission_plan WHERE id=$1 AND admission_id=$2 AND organization_id=$3")
                .bind(plan).bind(id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
            validate_plan(&mut tx, key, ctx.organization_id, &r, &p).await?;
        }
    }
    super::store::require_admin(&mut tx, ctx).await?;
    sqlx::query("UPDATE migration_people_admission SET state=$3,lifecycle_revision=lifecycle_revision+1,lease_token=NULL,lease_expires_at=NULL,pause_reason=NULL,cancelled_at=CASE WHEN $3='cancelled' THEN clock_timestamp() ELSE cancelled_at END WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(state).execute(&mut *tx).await?;
    let v = json!({"admission_id":id,"state":state});
    s::receipt(&mut tx, key, ctx, action, cmd.request_id, id, &digest, &v).await?;
    tx.commit().await?;
    Ok(v)
}

async fn validate_plan(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: crate::ids::OrganizationId,
    run: &sqlx::postgres::PgRow,
    plan: &sqlx::postgres::PgRow,
) -> Result<Value, MigrationError> {
    let inputs = s::validate_run(conn, key, org, run).await?;
    let frozen: Value = s::open(
        key,
        org,
        run.get("id"),
        plan.get("id"),
        "inputs",
        plan.get("inputs_nonce"),
        plan.get("inputs_ciphertext"),
    )?;
    if inputs != frozen {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(inputs)
}

async fn check_boundary(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: crate::ids::OrganizationId,
    run: &sqlx::postgres::PgRow,
    inputs: &Value,
) -> Result<(), MigrationError> {
    let previous = sqlx::query("SELECT * FROM migration_people_admission WHERE organization_id=$1 AND parent_import_id=$2 AND id<>$3 AND confirmed_admission_plan_id IS NOT NULL ORDER BY confirmed_completed_at DESC,created_at DESC,id DESC LIMIT 1")
        .bind(org.0).bind(run.get::<Uuid,_>("parent_import_id")).bind(run.get::<Uuid,_>("id")).fetch_optional(&mut *conn).await?;
    let Some(previous) = previous else {
        return Ok(());
    };
    let completed = previous
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_completed_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    if run.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at") > completed {
        return Ok(());
    }
    let same = previous.get::<String, _>("state") == "cancelled"
        && previous.get::<Uuid, _>("report_id") == run.get::<Uuid, _>("report_id")
        && previous.get::<Uuid, _>("newer_snapshot_id") == run.get::<Uuid, _>("newer_snapshot_id")
        && previous.get::<i64, _>("newer_sequence") == run.get::<i64, _>("newer_sequence")
        && previous.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
            == run.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
        && previous.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at")
            == run.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at");
    if !same {
        return Err(MigrationError::SourceNotEligible);
    }
    let plan = sqlx::query("SELECT id,inputs_nonce,inputs_ciphertext FROM migration_people_admission_plan WHERE id=$1 AND admission_id=$2 AND organization_id=$3")
        .bind(previous.get::<Option<Uuid>,_>("confirmed_admission_plan_id")).bind(previous.get::<Uuid,_>("id")).bind(org.0).fetch_one(conn).await?;
    let frozen: Value = s::open(
        key,
        org,
        previous.get("id"),
        plan.get("id"),
        "inputs",
        plan.get("inputs_nonce"),
        plan.get("inputs_ciphertext"),
    )?;
    if frozen != *inputs {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_digest_is_rejected_without_slicing_unicode() {
        assert!(super::hex(&"é".repeat(32)).is_err());
        assert!(super::hex(&"g".repeat(64)).is_err());
        assert_eq!(super::hex(&"a0".repeat(32)).unwrap(), vec![160; 32]);
    }
}

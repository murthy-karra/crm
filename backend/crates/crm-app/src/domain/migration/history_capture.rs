//! D-070 current-admin commands for independently confirmed retained capture.
use super::{
    crypto, history_capture_queries as q, history_capture_store as s,
    snapshot::{decimal, SnapshotPolicy},
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use chrono::{DateTime, Utc};
pub use q::{detail, list, record_detail, records};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposeFubHistoryCapture {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
    pub connection_id: Uuid,
    pub expected_revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgements {
    pub api_visible_account_scope: bool,
    pub coverage_gaps: bool,
    pub retained_not_imported: bool,
    pub source_user_evidence_revision: String,
    pub source_user_difference: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmFubHistoryCapture {
    pub request_id: Uuid,
    pub expected_run_revision: String,
    pub acknowledgements: Acknowledgements,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRequest {
    pub request_id: Uuid,
    pub expected_run_revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncreaseFubHistoryCaptureBudget {
    pub request_id: Uuid,
    pub expected_run_revision: String,
    pub expected_run_budget_revision: String,
    pub expected_org_budget_revision: String,
    pub expected_policy_revision: String,
    pub run_byte_limit: String,
    pub org_byte_limit: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPage {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub parent_import_id: Option<Uuid>,
    pub family: Option<String>,
    pub disposition: Option<String>,
    pub record_id: Option<Uuid>,
}
impl HistoryPage {
    pub fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(50);
        if !(1..=50).contains(&n) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}
fn revision(r: &sqlx::postgres::PgRow, v: &str) -> Result<(), MigrationError> {
    if decimal(v)? != r.get::<i64, _>("revision") {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,operation="history_propose"))]
pub async fn propose(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: ProposeFubHistoryCapture,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "propose", None, &cmd)?;
    if let Some(value) = s::receipt(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(value);
    }
    let parent = s::parent(&mut tx, ctx.organization_id, cmd.parent_import_id).await?;
    let rev = i32::try_from(decimal(&cmd.expected_revision)?)
        .map_err(|_| MigrationError::InvalidInput)?;
    let (connection, identity) =
        s::connection(&mut tx, key, ctx.organization_id, cmd.connection_id, rev).await?;
    if identity.account_id != parent.get::<i64, _>("source_account_id") {
        return Err(MigrationError::SourceAccountMismatch);
    }
    let old=sqlx::query("SELECT id,nonce,ciphertext FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='identity' AND accepted AND sequence<=$3 ORDER BY sequence DESC LIMIT 1").bind(parent.get::<Uuid,_>("snapshot_id")).bind(ctx.organization_id.0).bind(parent.get::<i64,_>("capture_sequence")).fetch_optional(&mut *tx).await?;
    let older_user = if let Some(old) = old {
        let bytes = crypto::open_snapshot(
            key,
            ctx.organization_id,
            parent.get("snapshot_id"),
            old.get("id"),
            "capture",
            old.get("nonce"),
            old.get("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        super::history_capture_source::parse_identity(&bytes)
            .map_err(|_| MigrationError::Crypto)?
            .user_id
    } else {
        None
    };
    sqlx::query("INSERT INTO migration_snapshot_storage(organization_id,byte_limit,budget_policy_revision) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(ctx.organization_id.0).bind(policy.initial_org()).bind(policy.revision()).execute(&mut *tx).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_history_capture_run(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,parent_capture_sequence,workspace_revision,connection_id,connection_revision,source_account_id,source_user_id,parent_source_user_id,source_user_evidence_revision,profile_version,parser_version,schema_version,initiated_by_user_id,state,proposal_expires_at,original_run_byte_limit,run_byte_limit,budget_policy_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,'proposed',now()+interval '10 minutes',$18,$18,$19)").bind(id).bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(parent.get::<Option<Uuid>,_>("confirmed_plan_id")).bind(parent.get::<Uuid,_>("snapshot_id")).bind(parent.get::<i64,_>("capture_sequence")).bind(parent.get::<i64,_>("workspace_revision")).bind(cmd.connection_id).bind(rev).bind(identity.account_id).bind(identity.user_id).bind(older_user).bind(connection.get::<i32,_>("identity_revision")).bind(s::PROFILE).bind(s::PARSER).bind(s::SCHEMA).bind(ctx.actor_user_id.0).bind(policy.initial_run()).bind(policy.revision()).execute(&mut *tx).await?;
    let metadata =
        (s::PROFILE.len() + s::PARSER.len() + s::SCHEMA.len() + policy.revision().len()) as i64;
    s::admit(&mut tx, ctx.organization_id, id, metadata, policy).await?;
    s::charge(&mut tx, ctx.organization_id, id, metadata).await?;
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "control",
        s::CONTROL_RESERVATION,
        policy,
    )
    .await?;
    for family in ["events", "calls", "text_messages"] {
        sqlx::query(
            "INSERT INTO migration_history_stream(run_id,organization_id,family) VALUES($1,$2,$3)",
        )
        .bind(id)
        .bind(ctx.organization_id.0)
        .bind(family)
        .execute(&mut *tx)
        .await?;
    }
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "propose",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,run_id=%id,operation="history_confirm"))]
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmFubHistoryCapture,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "confirm", Some(id), &cmd)?;
    if let Some(v) = s::receipt(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    revision(&r, &cmd.expected_run_revision)?;
    s::source_authority(&mut tx, key, ctx, &r).await?;
    if r.get::<String, _>("state") != "proposed"
        || r.get::<DateTime<Utc>, _>("proposal_expires_at") <= Utc::now()
        || !s::current_profile(&r)
    {
        return Err(MigrationError::Conflict);
    }
    let a = &cmd.acknowledgements;
    let difference =
        r.get::<Option<i64>, _>("parent_source_user_id") != Some(r.get("source_user_id"));
    if !a.api_visible_account_scope
        || !a.coverage_gaps
        || !a.retained_not_imported
        || a.source_user_difference != difference
        || decimal(&a.source_user_evidence_revision)?
            != i64::from(r.get::<i32, _>("source_user_evidence_revision"))
    {
        return Err(MigrationError::InvalidInput);
    }
    release
        .ok_or(MigrationError::Conflict)?
        .require_history_capture(&mut tx)
        .await
        .map_err(|_| MigrationError::Conflict)?;
    s::exclusive(&mut tx, ctx.organization_id, id).await?;
    s::admit(
        &mut tx,
        ctx.organization_id,
        id,
        s::REQUEST_RESERVATION,
        policy,
    )
    .await?;
    sqlx::query("UPDATE migration_history_capture_run SET state='queued',confirmed_at=now(),identity_required=true,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "confirm",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,run_id=%id,operation="history_retry"))]
pub async fn retry(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: HistoryRequest,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "retry", Some(id), &cmd)?;
    if let Some(v) = s::receipt(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    revision(&r, &cmd.expected_run_revision)?;
    s::source_authority(&mut tx, key, ctx, &r).await?;
    if r.get::<String, _>("state") != "paused"
        || r.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_none()
        || !s::current_profile(&r)
    {
        return Err(MigrationError::Conflict);
    }
    if matches!(
        r.get::<Option<String>, _>("pause_reason").as_deref(),
        Some(
            "source_identity_mismatch"
                | "connection_changed"
                | "parent_changed"
                | "profile_incompatible"
        )
    ) {
        return Err(MigrationError::Conflict);
    }
    release
        .ok_or(MigrationError::Conflict)?
        .require_history_capture(&mut tx)
        .await
        .map_err(|_| MigrationError::Conflict)?;
    s::exclusive(&mut tx, ctx.organization_id, id).await?;
    s::admit(
        &mut tx,
        ctx.organization_id,
        id,
        s::REQUEST_RESERVATION,
        policy,
    )
    .await?;
    sqlx::query("UPDATE migration_history_capture_run SET state='queued',identity_required=true,identity_attempts=0,pause_reason=NULL,next_attempt_at=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_history_stream SET cycle_attempts=0 WHERE run_id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "retry",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,run_id=%id,operation="history_cancel"))]
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: HistoryRequest,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "cancel", Some(id), &cmd)?;
    if let Some(v) = s::receipt(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    revision(&r, &cmd.expected_run_revision)?;
    if s::terminal(&r.get::<String, _>("state")) {
        return Err(MigrationError::Conflict);
    }
    s::release_kind(&mut tx, ctx.organization_id, id, "source").await?;
    sqlx::query("UPDATE migration_history_capture_run SET state='cancelled',pause_reason=NULL,completed_at=now(),lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "cancel",
        &digest,
        policy,
        true,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,run_id=%id,operation="history_budget"))]
pub async fn increase_budget(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: IncreaseFubHistoryCaptureBudget,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "budget", Some(id), &cmd)?;
    if let Some(v) = s::receipt(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    revision(&r, &cmd.expected_run_revision)?;
    if s::terminal(&r.get::<String, _>("state")) {
        return Err(MigrationError::Conflict);
    }
    let org=sqlx::query("SELECT byte_limit,budget_revision,budget_policy_revision FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    let run = decimal(&cmd.run_byte_limit)?;
    let org_limit = decimal(&cmd.org_byte_limit)?;
    if cmd.expected_policy_revision != policy.revision()
        || decimal(&cmd.expected_run_budget_revision)? != r.get::<i64, _>("budget_revision")
        || decimal(&cmd.expected_org_budget_revision)? != org.get::<i64, _>("budget_revision")
    {
        return Err(MigrationError::Conflict);
    }
    if run < r.get::<i64, _>("run_byte_limit")
        || org_limit < org.get::<i64, _>("byte_limit")
        || run > policy.run_ceiling_bytes
        || org_limit > policy.org_ceiling_bytes
        || run > org_limit
    {
        return Err(MigrationError::InvalidInput);
    }
    let delta =
        policy.revision().len() as i64 - r.get::<String, _>("budget_policy_revision").len() as i64;
    sqlx::query("UPDATE migration_history_capture_run SET run_byte_limit=$3,budget_revision=budget_revision+1,budget_policy_revision=$4,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(run).bind(policy.revision()).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET byte_limit=$2,budget_revision=budget_revision+1,budget_policy_revision=$3 WHERE organization_id=$1").bind(ctx.organization_id.0).bind(org_limit).bind(policy.revision()).execute(&mut *tx).await?;
    if delta > 0 {
        s::admit(&mut tx, ctx.organization_id, id, delta, policy).await?
    }
    s::charge(&mut tx, ctx.organization_id, id, delta).await?;
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "budget",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}

#[cfg(feature = "test-support")]
pub use super::history_capture_queries::records_sql as plans_records_sql;

#[cfg(feature = "test-support")]
pub use super::history_capture_queries::record_detail_sql as plans_record_detail_sql;

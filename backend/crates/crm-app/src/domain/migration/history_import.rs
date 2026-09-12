//! D-072 typed retained-only history import commands and bounded query surface.
pub use super::history_import_queries::{detail, list, records, results};
use super::{
    history_capture_store as capture, history_import_source as source, history_import_store as s,
    snapshot::{decimal, SnapshotPolicy},
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
};
use chrono::{DateTime, Utc};
pub use s::display;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareFubHistoryImport {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
    pub capture_id: Uuid,
    pub expected_capture_revision: String,
    pub expected_workspace_revision: String,
    pub expected_policy_revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgements {
    pub external_facts: bool,
    pub date_uncertainty: bool,
    pub coverage_and_holds: bool,
    pub review_only: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmFubHistoryImport {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub expected_revision: String,
    pub expected_plan_revision: String,
    pub expected_workspace_revision: String,
    pub expected_policy_revision: String,
    pub acknowledgements: Acknowledgements,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeFubHistoryImport {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub expected_policy_revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelFubHistoryImport {
    pub request_id: Uuid,
    pub expected_revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncreaseFubHistoryImportBudget {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub expected_run_budget_revision: String,
    pub expected_org_budget_revision: String,
    pub expected_policy_revision: String,
    pub run_byte_limit: String,
    pub org_byte_limit: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportPage {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub parent_import_id: Option<Uuid>,
    pub family: Option<String>,
    pub disposition: Option<String>,
}
impl ImportPage {
    pub fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(25);
        if !(1..=50).contains(&n) || self.cursor.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,operation="history_import_prepare"))]
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: PrepareFubHistoryImport,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let digest = s::digest(key, ctx, "prepare", None, &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    s::policy_revision(policy, &cmd.expected_policy_revision)?;
    let parent = capture::parent(&mut tx, ctx.organization_id, cmd.parent_import_id).await?;
    let captured = capture::row(&mut tx, ctx.organization_id, cmd.capture_id).await?;
    if !capture::current_profile(&captured)
        || captured.get::<String, _>("state") != "completed_with_gaps"
        || captured.get::<Uuid, _>("parent_import_id") != cmd.parent_import_id
        || captured.get::<i64, _>("revision") != decimal(&cmd.expected_capture_revision)?
        || parent.get::<i64, _>("workspace_revision") != decimal(&cmd.expected_workspace_revision)?
    {
        return Err(MigrationError::Conflict);
    }
    capture::verify_parent(&mut tx, ctx.organization_id, &captured).await?;
    if sqlx::query("SELECT 1 FROM migration_history_import_run WHERE organization_id=$1 AND parent_import_id=$2 AND state NOT IN ('completed','cancelled')").bind(ctx.organization_id.0).bind(cmd.parent_import_id).fetch_optional(&mut *tx).await?.is_some(){return Err(MigrationError::Conflict);}
    let anchor=sqlx::query("SELECT * FROM migration_history_import_anchor WHERE organization_id=$1 AND parent_import_id=$2").bind(ctx.organization_id.0).bind(cmd.parent_import_id).fetch_optional(&mut *tx).await?;
    let id = Uuid::new_v4();
    let plan_id = anchor
        .as_ref()
        .map_or_else(Uuid::new_v4, |a| a.get("plan_id"));
    if anchor.is_some() {
        let p = s::plan(&mut tx, ctx.organization_id, plan_id).await?;
        s::validate(&mut tx, key, ctx.organization_id, &p).await?;
        if p.get::<Uuid, _>("capture_id") != cmd.capture_id {
            return Err(MigrationError::Conflict);
        }
    }
    sqlx::query("INSERT INTO migration_history_import_run(id,organization_id,plan_id,parent_import_id,executor_user_id,state,phase,run_byte_limit,budget_policy_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(cmd.parent_import_id).bind(ctx.actor_user_id.0).bind(if anchor.is_some(){"ready"}else{"preparing"}).bind(if anchor.is_some(){"apply"}else{"capture"}).bind(policy.initial_run()).bind(policy.revision()).execute(&mut *tx).await?;
    s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "control",
        s::CONTROL,
        policy,
    )
    .await?;
    let token = s::reserve(&mut tx, ctx.organization_id, id, "unit", 16384, policy).await?;
    let before = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    if anchor.is_none() {
        let streams=sqlx::query("SELECT * FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 ORDER BY family").bind(cmd.capture_id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
        if streams.len() != 3
            || streams
                .iter()
                .any(|r| r.get::<String, _>("state") != "enumerated")
        {
            return Err(MigrationError::Conflict);
        }
        let stream_values:Vec<Value>=streams.iter().map(|r|{let mut value=json!({"family":r.get::<String,_>("family"),"state":r.get::<String,_>("state"),"reported_total":r.get::<Option<String>,_>("reported_total"),"api_inaccessible_count":null,"count_basis":"advancing_pages","content_scope":"exact_returned_json_only","enumeration_is_complete_account_history":false});for column in ["checkpoint","occurrences","valid_occurrences","invalid_occurrences","unique_ids","equal_repeats","conflicting_variants","linked","parent_excluded","no_parent_identity","invalid_person_reference","conflicting_reference","attempts"]{value[column]=json!(r.get::<i64,_>(column).to_string());}value}).collect();
        let coverage = json!({"streams":stream_values,"warnings":["api_restricted_records_unknown","detail_content_not_fetched","not_atomic_snapshot","external_facts_only"],"api_inaccessible_count":null,"enumeration_is_complete_account_history":false});
        if serde_json::to_vec(&coverage)
            .map_err(|_| MigrationError::Crypto)?
            .len()
            > 4096
        {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query("INSERT INTO migration_history_import_plan(id,organization_id,owner_run_id,parent_import_id,parent_plan_id,snapshot_id,parent_capture_sequence,workspace_revision,capture_id,capture_revision,capture_sequence,source_account_id,source_access_user_id,profile_version,parser_version,schema_version,interpretation_version,reader_version,binding_hmac,state,coverage) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,'building',$20)").bind(plan_id).bind(ctx.organization_id.0).bind(id).bind(cmd.parent_import_id).bind(captured.get::<Uuid,_>("parent_plan_id")).bind(captured.get::<Uuid,_>("snapshot_id")).bind(captured.get::<i64,_>("parent_capture_sequence")).bind(captured.get::<i64,_>("workspace_revision")).bind(cmd.capture_id).bind(captured.get::<i64,_>("revision")).bind(captured.get::<i64,_>("capture_sequence")).bind(captured.get::<i64,_>("source_account_id")).bind(captured.get::<i64,_>("source_user_id")).bind(captured.get::<String,_>("profile_version")).bind(captured.get::<String,_>("parser_version")).bind(captured.get::<String,_>("schema_version")).bind(source::INTERPRETATION).bind(source::READER).bind([0u8;32].as_slice()).bind(coverage).execute(&mut *tx).await?;
        let p = s::plan(&mut tx, ctx.organization_id, plan_id).await?;
        let hash = s::binding(key, ctx.organization_id, &p)?;
        sqlx::query("UPDATE migration_history_import_plan SET binding_hmac=$3 WHERE id=$1 AND organization_id=$2").bind(plan_id).bind(ctx.organization_id.0).bind(hash.as_slice()).execute(&mut *tx).await?;
        for stream in streams {
            sqlx::query("INSERT INTO migration_history_import_stream(plan_id,organization_id,owner_run_id,family,reported_total) VALUES($1,$2,$3,$4,$5)").bind(plan_id).bind(ctx.organization_id.0).bind(id).bind(stream.get::<String,_>("family")).bind(stream.get::<Option<String>,_>("reported_total").ok_or(MigrationError::Crypto)?).execute(&mut *tx).await?;
        }
    }
    let after = s::row(&mut tx, ctx.organization_id, id)
        .await?
        .get::<i64, _>("retained_bytes");
    s::release(&mut tx, ctx.organization_id, id, token, after - before).await?;
    let value = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "prepare",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,import_id=%id,operation="history_import_confirm"))]
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmFubHistoryImport,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, true).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "confirm", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    s::revision(&r, &cmd.expected_revision)?;
    s::policy_revision(policy, &cmd.expected_policy_revision)?;
    let p = s::plan(&mut tx, ctx.organization_id, r.get("plan_id")).await?;
    s::validate(&mut tx, key, ctx.organization_id, &p).await?;
    let anchored=sqlx::query("SELECT 1 FROM migration_history_import_anchor WHERE organization_id=$1 AND parent_import_id=$2").bind(ctx.organization_id.0).bind(r.get::<Uuid,_>("parent_import_id")).fetch_optional(&mut *tx).await?.is_some();
    let a = &cmd.acknowledgements;
    if r.get::<String, _>("state") != "ready"
        || cmd.plan_id != p.get::<Uuid, _>("id")
        || decimal(&cmd.expected_plan_revision)? != p.get::<i64, _>("revision")
        || decimal(&cmd.expected_workspace_revision)? != p.get::<i64, _>("workspace_revision")
        || p.get::<String, _>("state") != "ready"
        || p.get::<i64, _>("eligible") == 0
        || (!anchored
            && p.get::<Option<DateTime<Utc>>, _>("expires_at")
                .is_none_or(|v| v <= Utc::now()))
        || !a.external_facts
        || !a.date_uncertainty
        || !a.coverage_and_holds
        || !a.review_only
    {
        return Err(MigrationError::Conflict);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_history_timeline(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    s::admit(&mut tx, ctx.organization_id, id, s::UNIT, policy).await?;
    sqlx::query("INSERT INTO migration_history_import_anchor(organization_id,parent_import_id,plan_id,interpretation_version,reader_version) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(ctx.organization_id.0).bind(r.get::<Uuid,_>("parent_import_id")).bind(cmd.plan_id).bind(source::INTERPRETATION).bind(source::READER).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_history_import_run SET state='queued',phase='apply',executor_user_id=$3,confirmed_at=clock_timestamp(),admitted_at=NULL,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let v = s::save(
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
    Ok(v)
}
pub async fn resume(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ResumeFubHistoryImport,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "resume", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    s::revision(&r, &cmd.expected_revision)?;
    s::policy_revision(policy, &cmd.expected_policy_revision)?;
    if r.get::<String, _>("state") != "paused" {
        return Err(MigrationError::Conflict);
    }
    let p = s::plan(&mut tx, ctx.organization_id, r.get("plan_id")).await?;
    s::validate(&mut tx, key, ctx.organization_id, &p).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_history_timeline(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    s::admit(&mut tx, ctx.organization_id, id, s::UNIT, policy).await?;
    sqlx::query("UPDATE migration_history_import_run SET state=CASE WHEN confirmed_at IS NOT NULL THEN 'queued' WHEN phase='apply' THEN 'ready' ELSE 'preparing' END,executor_user_id=$3,pause_reason=NULL,admitted_at=NULL,budget_policy_revision=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(policy.revision()).execute(&mut *tx).await?;
    let v = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "resume",
        &digest,
        policy,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: CancelFubHistoryImport,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "cancel", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    s::revision(&r, &cmd.expected_revision)?;
    let terminal = s::terminal(&r.get::<String, _>("state"));
    if !terminal {
        s::release_kind(&mut tx, ctx.organization_id, id, "unit", 0).await?;
        sqlx::query("UPDATE migration_history_import_run SET state='cancelled',lease_token=NULL,lease_expires_at=NULL,completed_at=clock_timestamp(),revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    }
    let v = s::save(
        &mut tx,
        key,
        ctx,
        id,
        cmd.request_id,
        "cancel",
        &digest,
        policy,
        !terminal,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}
pub async fn increase_budget(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: IncreaseFubHistoryImportBudget,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    let digest = s::digest(key, ctx, "budget", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, cmd.request_id, &digest).await? {
        return Ok(v);
    }
    s::revision(&r, &cmd.expected_revision)?;
    s::policy_revision(policy, &cmd.expected_policy_revision)?;
    let ledger =
        sqlx::query("SELECT * FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE")
            .bind(ctx.organization_id.0)
            .fetch_one(&mut *tx)
            .await?;
    let run_limit = decimal(&cmd.run_byte_limit)?;
    let org_limit = decimal(&cmd.org_byte_limit)?;
    if s::terminal(&r.get::<String, _>("state"))
        || decimal(&cmd.expected_run_budget_revision)? != r.get::<i64, _>("budget_revision")
        || decimal(&cmd.expected_org_budget_revision)? != ledger.get::<i64, _>("budget_revision")
        || run_limit < r.get::<i64, _>("run_byte_limit")
        || org_limit < ledger.get::<i64, _>("byte_limit")
        || run_limit > policy.run_ceiling_bytes
        || org_limit > policy.org_ceiling_bytes
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_history_import_run SET run_byte_limit=$3,budget_revision=budget_revision+1,budget_policy_revision=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(run_limit).bind(policy.revision()).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET byte_limit=$2,budget_revision=budget_revision+1,budget_policy_revision=$3 WHERE organization_id=$1").bind(ctx.organization_id.0).bind(org_limit).bind(policy.revision()).execute(&mut *tx).await?;
    let v = s::save(
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
    Ok(v)
}

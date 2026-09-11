//! D-063 typed snapshot commands. All entry points recheck current tenant authority.
use super::{crypto, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
pub const PROFILE: &str = "fub-core-v1";
pub const ENGINE: &str = "1";
pub const SOURCE_RESERVATION: i64 = 64 * 1024 * 1024;
pub const PREVIEW_RESERVATION: i64 = 2 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct SnapshotPolicy {
    pub run_ceiling_bytes: i64,
    pub org_ceiling_bytes: i64,
}
impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            run_ceiling_bytes: 2 * 1024 * 1024 * 1024,
            org_ceiling_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}
impl SnapshotPolicy {
    pub fn revision(&self) -> String {
        format!("v1-{}-{}", self.run_ceiling_bytes, self.org_ceiling_bytes)
    }
    pub fn initial_run(&self) -> i64 {
        self.run_ceiling_bytes
            .min(Self::default().run_ceiling_bytes)
    }
    pub fn initial_org(&self) -> i64 {
        self.org_ceiling_bytes
            .min(Self::default().org_ceiling_bytes)
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposeCoreSnapshot {
    pub request_id: Uuid,
    pub connection_id: Uuid,
    pub expected_revision: i32,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRequest {
    pub request_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IncreaseCoreSnapshotBudget {
    pub request_id: Uuid,
    pub expected_run_budget_revision: String,
    pub expected_org_budget_revision: String,
    pub expected_policy_revision: String,
    pub run_byte_limit: String,
    pub org_byte_limit: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub family: Option<String>,
    pub disposition: Option<String>,
    pub record_id: Option<Uuid>,
}
impl PageQuery {
    pub fn limit(&self, max: u16) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(max);
        if n == 0 || n > max {
            Err(MigrationError::InvalidInput)
        } else {
            Ok(i64::from(n))
        }
    }
}
pub(crate) fn decimal(v: &str) -> Result<i64, MigrationError> {
    if v.is_empty() || v.len() > 19 || !v.bytes().all(|v| v.is_ascii_digit()) {
        return Err(MigrationError::InvalidInput);
    }
    v.parse::<i64>().map_err(|_| MigrationError::InvalidInput)
}
pub(crate) async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}
pub(crate) async fn row(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_snapshot WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(id)
        .bind(org.0)
        .fetch_optional(conn)
        .await?
        .ok_or(MigrationError::NotFound)
}
pub(crate) async fn exclusive(
    conn: &mut PgConnection,
    org: OrganizationId,
    except: Uuid,
) -> Result<(), MigrationError> {
    let found=sqlx::query("SELECT 1 FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry') UNION ALL SELECT 1 FROM migration_snapshot WHERE organization_id=$1 AND id<>$2 AND state IN ('queued','running','waiting_retry') LIMIT 1").bind(org.0).bind(except).fetch_optional(conn).await?;
    if found.is_some() {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
async fn source_authority(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if r.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    let found=sqlx::query("SELECT 1 FROM migration_connection WHERE id=$1 AND organization_id=$2 AND revision=$3 AND status='connected' FOR UPDATE").bind(r.get::<Uuid,_>("connection_id")).bind(ctx.organization_id.0).bind(r.get::<i32,_>("connection_revision")).fetch_optional(conn).await?;
    if found.is_none() {
        Err(MigrationError::Conflict)
    } else {
        Ok(())
    }
}
pub(crate) async fn save(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    op: &str,
    request: Uuid,
    digest: &[u8],
    value: &Value,
) -> Result<(), MigrationError> {
    store::insert_receipt(key, conn, ctx.organization_id, op, request, digest, value).await
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="propose"))]
pub async fn propose(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: ProposeCoreSnapshot,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let digest = crypto::request_digest(
        key,
        "propose_snapshot",
        &serde_json::to_vec(&(
            ctx.actor_user_id.0,
            cmd.connection_id,
            cmd.expected_revision,
        ))
        .map_err(|_| MigrationError::InvalidInput)?,
    );
    if let Some(v) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "propose_snapshot",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    let conn=sqlx::query("SELECT source_account_id,revision,status FROM migration_connection WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(cmd.connection_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if conn.get::<i32, _>("revision") != cmd.expected_revision
        || conn.get::<String, _>("status") != "connected"
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("INSERT INTO migration_snapshot_storage(organization_id,byte_limit,budget_policy_revision) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(ctx.organization_id.0).bind(policy.initial_org()).bind(policy.revision()).execute(&mut *tx).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_snapshot(id,organization_id,connection_id,connection_revision,source_account_id,profile_version,schema_version,initiated_by_user_id,state,proposal_expires_at,original_run_byte_limit,run_byte_limit,budget_policy_revision) VALUES($1,$2,$3,$4,$5,$6,'sha256:e136daf321fae96c7feb895fe662e0e925c501e059b8fd0faff6074cccff5c79',$7,'proposed',now()+interval '10 minutes',$8,$8,$9)").bind(id).bind(ctx.organization_id.0).bind(cmd.connection_id).bind(cmd.expected_revision).bind(conn.get::<i64,_>("source_account_id")).bind(PROFILE).bind(ctx.actor_user_id.0).bind(policy.initial_run()).bind(policy.revision()).execute(&mut *tx).await?;
    let snapshot = view(&mut tx, policy, ctx, id).await?;
    let result = json!({"proposal":{"families":["users","stages","custom_fields","people","notes","tasks"],"profile_version":PROFILE,"expires_at":snapshot["proposal_expires_at"],"run_byte_limit":snapshot["run_byte_limit"],"org_byte_limit":snapshot["org_byte_limit"],"source_scope":"Credential-visible core collections; offered unclaimed People only. Not a transactional or complete account export."},"snapshot":snapshot});
    save(
        &mut tx,
        key,
        ctx,
        "propose_snapshot",
        cmd.request_id,
        &digest,
        &result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
pub enum SourceAction {
    Confirm,
    Retry,
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="source_action"))]
pub async fn source_action(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: SnapshotRequest,
    action: SourceAction,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = row(&mut tx, ctx.organization_id, id).await?;
    source_authority(&mut tx, ctx, &r).await?;
    let op = match action {
        SourceAction::Confirm => "confirm_snapshot",
        SourceAction::Retry => "retry_snapshot",
    };
    let digest = crypto::request_digest(
        key,
        op,
        &serde_json::to_vec(&(id, ctx.actor_user_id.0))
            .map_err(|_| MigrationError::InvalidInput)?,
    );
    if let Some(v) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        op,
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    let state = r.get::<String, _>("state");
    match action {
        SourceAction::Confirm => {
            if state != "proposed" || r.get::<DateTime<Utc>, _>("proposal_expires_at") <= Utc::now()
            {
                return Err(MigrationError::Conflict);
            }
            for (stream, family) in [
                ("users", "users"),
                ("stages", "stages"),
                ("custom_fields", "custom_fields"),
                ("people", "people"),
                ("notes", "notes"),
                ("note_detail", "notes"),
                ("tasks_open", "tasks"),
                ("tasks_completed", "tasks"),
            ] {
                sqlx::query("INSERT INTO migration_snapshot_stream(snapshot_id,organization_id,stream,family) VALUES($1,$2,$3,$4)").bind(id).bind(ctx.organization_id.0).bind(stream).bind(family).execute(&mut *tx).await?;
            }
        }
        SourceAction::Retry => {
            if state != "paused" {
                return Err(MigrationError::Conflict);
            }
            sqlx::query("UPDATE migration_snapshot_stream SET cycle_attempts=0,error_code=NULL WHERE snapshot_id=$1 AND organization_id=$2 AND state<>'completed'").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
        }
    }
    exclusive(&mut tx, ctx.organization_id, id).await?;
    sqlx::query("UPDATE migration_snapshot SET state='queued',confirmed_at=COALESCE(confirmed_at,now()),identity_required=true,pause_reason=NULL,next_attempt_at=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let result = json!({"snapshot":view(&mut tx,policy,ctx,id).await?});
    save(&mut tx, key, ctx, op, cmd.request_id, &digest, &result).await?;
    tx.commit().await?;
    Ok(result)
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="cancel"))]
pub async fn cancel(
    pool: &PgPool,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    row(&mut tx, ctx.organization_id, id).await?;
    sqlx::query("UPDATE migration_snapshot SET state='cancelled',pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,completed_at=now() WHERE id=$1 AND organization_id=$2 AND state IN ('proposed','queued','running','waiting_retry','paused')").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    release_source(&mut tx, ctx.organization_id, id).await?;
    let result = json!({"snapshot":view(&mut tx,policy,ctx,id).await?});
    tx.commit().await?;
    Ok(result)
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="increase_budget"))]
pub async fn increase_budget(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: IncreaseCoreSnapshotBudget,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = row(&mut tx, ctx.organization_id, id).await?;
    let digest = crypto::request_digest(
        key,
        "increase_snapshot_budget",
        &serde_json::to_vec(&(id, ctx.actor_user_id.0, &cmd))
            .map_err(|_| MigrationError::InvalidInput)?,
    );
    if let Some(v) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "increase_snapshot_budget",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    let ledger =
        sqlx::query("SELECT * FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE")
            .bind(ctx.organization_id.0)
            .fetch_one(&mut *tx)
            .await?;
    let run = decimal(&cmd.run_byte_limit)?;
    let org = decimal(&cmd.org_byte_limit)?;
    let oldrun = r.get::<i64, _>("run_byte_limit");
    let oldorg = ledger.get::<i64, _>("byte_limit");
    if r.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_none()
        || decimal(&cmd.expected_run_budget_revision)? != r.get::<i64, _>("budget_revision")
        || decimal(&cmd.expected_org_budget_revision)? != ledger.get::<i64, _>("budget_revision")
        || cmd.expected_policy_revision != policy.revision()
        || run < oldrun
        || org < oldorg
        || (run == oldrun && org == oldorg)
        || run > policy.run_ceiling_bytes
        || org > policy.org_ceiling_bytes
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_snapshot SET run_byte_limit=$3,budget_revision=budget_revision+1,budget_policy_revision=$4 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(run).bind(policy.revision()).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET byte_limit=$2,budget_revision=budget_revision+1,budget_policy_revision=$3 WHERE organization_id=$1").bind(ctx.organization_id.0).bind(org).bind(policy.revision()).execute(&mut *tx).await?;
    let result = json!({"snapshot":view(&mut tx,policy,ctx,id).await?,"budget":{"approved_by_user_id":ctx.actor_user_id.0,"old_run_byte_limit":oldrun.to_string(),"old_org_byte_limit":oldorg.to_string(),"run_byte_limit":run.to_string(),"org_byte_limit":org.to_string(),"old_run_budget_revision":r.get::<i64,_>("budget_revision").to_string(),"run_budget_revision":(r.get::<i64,_>("budget_revision")+1).to_string(),"old_org_budget_revision":ledger.get::<i64,_>("budget_revision").to_string(),"org_budget_revision":(ledger.get::<i64,_>("budget_revision")+1).to_string(),"old_run_policy_revision":r.get::<String,_>("budget_policy_revision"),"old_org_policy_revision":ledger.get::<String,_>("budget_policy_revision"),"policy_revision":policy.revision()}});
    save(
        &mut tx,
        key,
        ctx,
        "increase_snapshot_budget",
        cmd.request_id,
        &digest,
        &result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) async fn view(
    conn: &mut PgConnection,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT s.*,l.byte_limit AS org_byte_limit,l.budget_revision AS org_budget_revision,l.budget_policy_revision AS org_budget_policy_revision,l.retained_bytes AS org_retained_bytes,l.reserved_bytes AS org_reserved_bytes,c.status AS connection_status,c.revision AS current_revision FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id JOIN migration_connection c ON c.id=s.connection_id AND c.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    let mut state: String = r.get("state");
    if state == "proposed" && r.get::<DateTime<Utc>, _>("proposal_expires_at") <= Utc::now() {
        state = "expired".into()
    }
    let source = r.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0
        && r.get::<i32, _>("connection_revision") == r.get::<i32, _>("current_revision")
        && r.get::<String, _>("connection_status") == "connected";
    let mut actions = Vec::new();
    if source && state == "proposed" {
        actions.push("confirm")
    }
    if source && state == "paused" {
        actions.push("retry")
    }
    if matches!(
        state.as_str(),
        "proposed" | "queued" | "running" | "waiting_retry" | "paused"
    ) {
        actions.push("cancel")
    }
    if !matches!(state.as_str(), "proposed" | "expired") {
        actions.push("increase_budget")
    }
    if matches!(
        state.as_str(),
        "completed" | "completed_with_gaps" | "paused" | "cancelled"
    ) && r.get::<i64, _>("accepted_captures") > 0
    {
        actions.push("generate_preview")
    }
    let previews=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_snapshot_preview WHERE snapshot_id=$1 AND organization_id=$2 ORDER BY created_at DESC,id DESC LIMIT 20").bind(id).bind(ctx.organization_id.0).fetch_all(&mut *conn).await?;
    Ok(
        json!({"id":id,"connection_id":r.get::<Uuid,_>("connection_id"),"connection_revision":r.get::<i32,_>("connection_revision"),"source_account_id":r.get::<i64,_>("source_account_id").to_string(),"profile_version":r.get::<String,_>("profile_version"),"state":state,"pause_reason":r.get::<Option<String>,_>("pause_reason"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"started_at":r.get::<Option<DateTime<Utc>>,_>("started_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"proposal_expires_at":r.get::<DateTime<Utc>,_>("proposal_expires_at"),"raw_bytes":r.get::<i64,_>("raw_bytes").to_string(),"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),"accepted_captures":r.get::<i64,_>("accepted_captures").to_string(),"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"original_run_byte_limit":r.get::<i64,_>("original_run_byte_limit").to_string(),"run_byte_limit":r.get::<i64,_>("run_byte_limit").to_string(),"org_byte_limit":r.get::<i64,_>("org_byte_limit").to_string(),"org_retained_bytes":r.get::<i64,_>("org_retained_bytes").to_string(),"org_reserved_bytes":r.get::<i64,_>("org_reserved_bytes").to_string(),"run_budget_revision":r.get::<i64,_>("budget_revision").to_string(),"org_budget_revision":r.get::<i64,_>("org_budget_revision").to_string(),"run_budget_policy_revision":r.get::<String,_>("budget_policy_revision"),"org_budget_policy_revision":r.get::<String,_>("org_budget_policy_revision"),"policy_revision":policy.revision(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"required_reservation_bytes":SOURCE_RESERVATION.to_string(),"actions":actions,"preview_ids":previews}),
    )
}
pub(crate) async fn streams(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Vec<Value>, MigrationError> {
    let rows=sqlx::query("SELECT s.*,(SELECT count(DISTINCT r.source_id) FROM migration_snapshot_record r WHERE r.snapshot_id=s.snapshot_id AND r.organization_id=s.organization_id AND r.family=s.family) AS distinct_ids,(SELECT min(c.captured_at) FROM migration_snapshot_capture c WHERE c.snapshot_id=s.snapshot_id AND c.organization_id=s.organization_id AND c.stream=s.stream) AS first_observed_at FROM migration_snapshot_stream s WHERE snapshot_id=$1 AND organization_id=$2 ORDER BY stream").bind(id).bind(org.0).fetch_all(conn).await?;
    Ok(rows.into_iter().map(|r|json!({"stream":r.get::<String,_>("stream"),"family":r.get::<String,_>("family"),"state":r.get::<String,_>("state"),"reported_total":r.get::<Option<String>,_>("reported_total"),"returned_items":r.get::<i64,_>("returned_items").to_string(),"distinct_ids":r.get::<i64,_>("distinct_ids").to_string(),"accepted_captures":r.get::<i64,_>("accepted_captures").to_string(),"content_gaps":r.get::<i64,_>("content_gaps").to_string(),"attempts":r.get::<i64,_>("attempts").to_string(),"error_code":r.get::<Option<String>,_>("error_code"),"observed_at":r.get::<Option<DateTime<Utc>>,_>("observed_at"),"first_observed_at":r.get::<Option<DateTime<Utc>>,_>("first_observed_at")})).collect())
}
pub(crate) fn coverage(streams: &[Value]) -> Value {
    let mut result = Vec::new();
    for family in [
        "users",
        "stages",
        "custom_fields",
        "people",
        "notes",
        "tasks",
    ] {
        let members: Vec<_> = streams.iter().filter(|s| s["family"] == family).collect();
        let complete = !members.is_empty() && members.iter().all(|s| s["state"] == "completed");
        let gaps = members.iter().any(|s| s["content_gaps"] != "0");
        result.push(json!({"family":family,"state":if complete {if gaps{"completed_with_gaps"}else{"complete_for_selected_query"}}else{"partial"},"reason":"Credential-visible query observed over time; live qualification pending.","next_action":"Review preserved records and source access before later import decisions."}));
    }
    for family in [
        "inquiries_history",
        "calls",
        "texts",
        "emails",
        "recordings",
        "tags",
        "addresses",
        "relationships",
        "appointments",
        "deals",
        "automation_settings",
        "external_files",
    ] {
        result.push(json!({"family":family,"state":"not_captured","reason":"Standalone collection and file bytes are outside this core profile; any embedded fields remain in raw evidence.","next_action":"Capture in a later snapshot step before cutover fidelity can be assessed."}));
    }
    Value::Array(result)
}
pub async fn detail(
    pool: &PgPool,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    let snapshot = view(&mut tx, policy, ctx, id).await?;
    let streams = streams(&mut tx, ctx.organization_id, id).await?;
    let result = json!({"snapshot":snapshot,"coverage":coverage(&streams),"streams":streams});
    tx.commit().await?;
    Ok(result)
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    q: PageQuery,
) -> Result<Value, MigrationError> {
    let n = q.limit(20)?;
    if q.family.is_some() || q.disposition.is_some() || q.record_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    let scope = "snapshots";
    let cursor = decode_cursor(
        key,
        ctx.organization_id,
        Uuid::nil(),
        scope,
        q.cursor.as_deref(),
    )?;
    let before: Option<(DateTime<Utc>, Uuid)> = cursor
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| MigrationError::InvalidInput)?;
    let rows=sqlx::query("SELECT id,created_at FROM migration_snapshot WHERE organization_id=$1 AND ($2::timestamptz IS NULL OR (created_at,id)<($2,$3)) ORDER BY created_at DESC,id DESC LIMIT $4").bind(ctx.organization_id.0).bind(before.map(|v|v.0)).bind(before.map(|v|v.1)).bind(n+1).fetch_all(&mut *tx).await?;
    let next = if rows.len() > n as usize {
        let r = &rows[n as usize - 1];
        Some(encode_cursor(
            key,
            ctx.organization_id,
            Uuid::nil(),
            scope,
            &json!([
                r.get::<DateTime<Utc>, _>("created_at"),
                r.get::<Uuid, _>("id")
            ]),
        )?)
    } else {
        None
    };
    let mut snapshots = Vec::new();
    for r in rows.iter().take(n as usize) {
        snapshots.push(view(&mut tx, policy, ctx, r.get("id")).await?)
    }
    let active=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_snapshot WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry','paused') ORDER BY created_at DESC,id DESC LIMIT 1").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    let completed=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_snapshot WHERE organization_id=$1 AND state IN ('completed','completed_with_gaps') ORDER BY created_at DESC,id DESC LIMIT 1").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(
        json!({"snapshots":snapshots,"next_cursor":next,"active_snapshot_id":active,"latest_completed_snapshot_id":completed}),
    )
}
pub(crate) fn encode_cursor(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    scope: &str,
    value: &Value,
) -> Result<String, MigrationError> {
    use base64::Engine;
    let sealed = crypto::seal_snapshot(
        key,
        org,
        run,
        Uuid::nil(),
        &format!("cursor:{scope}"),
        &serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
pub(crate) fn decode_cursor(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    scope: &str,
    cursor: Option<&str>,
) -> Result<Option<Value>, MigrationError> {
    use base64::Engine;
    let Some(c) = cursor else { return Ok(None) };
    if c.len() > 2048 {
        return Err(MigrationError::InvalidInput);
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c)
        .map_err(|_| MigrationError::InvalidInput)?;
    if bytes.len() < 40 {
        return Err(MigrationError::InvalidInput);
    }
    let raw = crypto::open_snapshot(
        key,
        org,
        run,
        Uuid::nil(),
        &format!("cursor:{scope}"),
        &bytes[..24],
        &bytes[24..],
    )
    .map_err(|_| MigrationError::InvalidInput)?;
    Ok(Some(
        serde_json::from_slice(&raw).map_err(|_| MigrationError::InvalidInput)?,
    ))
}
/// Must be called under the Organization and snapshot locks. Reclaim is permitted
/// only after the matching source/preview writer has already been fenced.
pub(crate) async fn release(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let reservation=sqlx::query_scalar::<_,i64>("DELETE FROM migration_snapshot_reservation WHERE token=$1 AND snapshot_id=$2 AND organization_id=$3 RETURNING byte_count").bind(token).bind(run).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    if actual > reservation {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(reservation).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(reservation).bind(actual).execute(conn).await?;
    Ok(())
}
pub(crate) async fn release_source(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
) -> Result<(), MigrationError> {
    let tokens=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_snapshot_reservation WHERE snapshot_id=$1 AND organization_id=$2 AND preview_id IS NULL").bind(run).bind(org.0).fetch_all(&mut *conn).await?;
    for token in tokens {
        release(conn, org, run, token, 0).await?
    }
    Ok(())
}
pub(crate) async fn reserve(
    conn: &mut PgConnection,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    run: Uuid,
    preview: Option<Uuid>,
    token: Uuid,
    amount: i64,
) -> Result<bool, MigrationError> {
    let r=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(run).bind(org.0).fetch_one(&mut *conn).await?;
    let available = r
        .get::<i64, _>("run_byte_limit")
        .min(policy.run_ceiling_bytes)
        .saturating_sub(r.get::<i64, _>("retained_bytes"))
        .saturating_sub(r.get::<i64, _>("reserved_bytes"));
    let org_available = r
        .get::<i64, _>("byte_limit")
        .min(policy.org_ceiling_bytes)
        .saturating_sub(r.get::<i64, _>("org_retained"))
        .saturating_sub(r.get::<i64, _>("org_reserved"));
    if amount <= 0 || amount > available || amount > org_available {
        return Ok(false);
    }
    sqlx::query("INSERT INTO migration_snapshot_reservation(token,snapshot_id,organization_id,preview_id,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '60 seconds')").bind(token).bind(run).bind(org.0).bind(preview).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
    Ok(true)
}

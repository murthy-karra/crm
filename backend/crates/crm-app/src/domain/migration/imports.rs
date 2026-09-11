//! Frozen, database-only People import commands and shared persistence.
use super::{crypto, snapshot, store, MigrationError};
use crate::{
    auth::workspace, config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

tokio::task_local! { static POLICY: snapshot::SnapshotPolicy; }
pub(crate) async fn with_policy<F: std::future::Future>(
    policy: &snapshot::SnapshotPolicy,
    future: F,
) -> F::Output {
    POLICY.scope(policy.clone(), future).await
}
pub const ENGINE: &str = "fub-people-import-v1";
pub const UNIT: i64 = 64 * 1024 * 1024;
pub const CANCEL_RESERVATION: i64 = 64 * 1024;
pub const DISPLAY_LIMIT: usize = 512 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPeopleImport {
    pub request_id: Uuid,
    pub snapshot_id: Uuid,
    pub preview_id: Uuid,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageChoice {
    Existing { stage_id: Uuid },
    Create,
    Hold,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssigneeChoice {
    Member { user_id: Uuid },
    Unassigned,
    Hold,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyChoice {}
impl<'de> Deserialize<'de> for StageChoice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Existing { stage_id: Uuid },
            Create(EmptyChoice),
            Hold(EmptyChoice),
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Existing { stage_id } => Self::Existing { stage_id },
            Wire::Create(_) => Self::Create,
            Wire::Hold(_) => Self::Hold,
        })
    }
}
impl<'de> Deserialize<'de> for AssigneeChoice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Member { user_id: Uuid },
            Unassigned(EmptyChoice),
            Hold(EmptyChoice),
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Member { user_id } => Self::Member { user_id },
            Wire::Unassigned(_) => Self::Unassigned,
            Wire::Hold(_) => Self::Hold,
        })
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StagePatch {
    pub source_key: String,
    pub choice: StageChoice,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssigneePatch {
    pub source_key: String,
    pub choice: AssigneeChoice,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplanPeopleImport {
    pub request_id: Uuid,
    pub expected_plan_revision: String,
    #[serde(default)]
    pub stage_mappings: Vec<StagePatch>,
    #[serde(default)]
    pub assignee_mappings: Vec<AssigneePatch>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgments {
    pub held_count: String,
    pub review_only: bool,
    pub remaining_data: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPeopleImport {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub confirmation_digest: String,
    pub acknowledgments: Acknowledgments,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub request_id: Uuid,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportPage {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub kind: Option<String>,
    pub disposition: Option<String>,
}
impl ImportPage {
    pub fn limit(&self) -> Result<i64, MigrationError> {
        let v = self.limit.unwrap_or(50);
        if !(1..=50).contains(&v) {
            Err(MigrationError::InvalidInput)
        } else {
            Ok(v.into())
        }
    }
}

pub(crate) fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, MigrationError> {
    serde_json::to_vec(v).map_err(|_| MigrationError::Crypto)
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
pub(crate) fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    v: &T,
) -> Result<crypto::Sealed, MigrationError> {
    crypto::seal_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("import-v1:{plan}:{purpose}"),
        &bytes(v)?,
    )
    .map_err(|_| MigrationError::Crypto)
}
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, resource, crypto and request scopes must remain independent"
)]
pub(crate) fn open<T: serde::de::DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    let v = crypto::open_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("import-v1:{plan}:{purpose}"),
        nonce,
        ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    serde_json::from_slice(&v).map_err(|_| MigrationError::Crypto)
}
pub(crate) fn sealed_bytes(v: &crypto::Sealed) -> i64 {
    (v.nonce.len() + v.ciphertext.len()) as i64
}
pub(crate) async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
    exclusive: bool,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = tokio::time::timeout(workspace::WAIT, pool.begin())
        .await
        .map_err(|_| MigrationError::Database(sqlx::Error::PoolTimedOut))??;
    workspace::bounded_lock_wait(&mut tx).await?;
    if exclusive {
        workspace::exclusive(&mut tx, ctx.organization_id).await?;
    } else {
        workspace::shared(&mut tx, ctx.organization_id).await?;
    }
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}
pub(crate) async fn run(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_import WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(id)
        .bind(org.0)
        .fetch_optional(conn)
        .await?
        .ok_or(MigrationError::NotFound)
}
pub(crate) async fn plan(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_import_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE").bind(id).bind(run).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, resource, crypto and request scopes must remain independent"
)]
pub(crate) async fn reserve(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    lease: Uuid,
    token: Uuid,
    amount: i64,
) -> Result<bool, MigrationError> {
    if !(1..=UNIT).contains(&amount) {
        return Err(MigrationError::InvalidInput);
    }
    let policy = POLICY
        .try_with(Clone::clone)
        .map_err(|_| MigrationError::StorageLimit)?;
    let r=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(snapshot).bind(org.0).fetch_one(&mut *conn).await?;
    if amount
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
        return Ok(false);
    }
    sqlx::query("INSERT INTO migration_import_reservation(token,import_id,snapshot_id,organization_id,plan_id,lease_token,byte_count,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,now()+interval '60 seconds')").bind(token).bind(run).bind(snapshot).bind(org.0).bind(plan).bind(lease).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_import SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(amount).execute(conn).await?;
    Ok(true)
}
pub(crate) async fn settle(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let r=sqlx::query("DELETE FROM migration_import_reservation WHERE token=$1 AND import_id=$2 AND organization_id=$3 RETURNING byte_count,snapshot_id").bind(token).bind(run).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Conflict)?;
    let reserved: i64 = r.get("byte_count");
    if actual > reserved {
        return Err(MigrationError::Conflict);
    }
    let snapshot: Uuid = r.get("snapshot_id");
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(reserved).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(reserved).bind(actual).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_import SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(reserved).bind(actual).execute(conn).await?;
    Ok(())
}
pub(crate) async fn release_all(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
) -> Result<(), MigrationError> {
    let tokens=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_import_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='work'").bind(run).bind(org.0).fetch_all(&mut *conn).await?;
    for token in tokens {
        settle(conn, org, run, token, 0).await?;
    }
    Ok(())
}
pub(crate) async fn charge(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    amount: i64,
) -> Result<(), MigrationError> {
    if amount <= 0 {
        sqlx::query("UPDATE migration_import SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(amount).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(conn).await?;
        return Ok(());
    }
    let token = Uuid::new_v4();
    if !reserve(conn, org, run, snapshot, plan, token, token, amount.max(1)).await? {
        return Err(MigrationError::StorageLimit);
    }
    settle(conn, org, run, token, amount).await
}
async fn replay<T: Serialize>(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    input: &T,
) -> Result<Option<Value>, MigrationError> {
    let digest = crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        &format!("import-request:{}:{action}", ctx.actor_user_id.0),
        &bytes(input)?,
    );
    let r=sqlx::query("SELECT * FROM migration_import_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).fetch_optional(&mut *conn).await?;
    let Some(r) = r else { return Ok(None) };
    if r.get::<Vec<u8>, _>("input_digest") != digest {
        return Err(MigrationError::ImportConflict);
    }
    let purpose = format!("import:{}:{action}", ctx.actor_user_id.0);
    let raw = crypto::open_receipt(
        key,
        ctx.organization_id,
        request,
        &purpose,
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    Ok(Some(
        serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)?,
    ))
}
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, resource, crypto and request scopes must remain independent"
)]
async fn receipt<T: Serialize>(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    input: &T,
    run: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    response: &mut Value,
) -> Result<(), MigrationError> {
    let digest = crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        &format!("import-request:{}:{action}", ctx.actor_user_id.0),
        &bytes(input)?,
    );
    let retained = response
        .get("import")
        .map(|value| {
            value["retained_bytes"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or(MigrationError::Crypto)
        })
        .transpose()?;
    if action == "cancel" {
        response["import"]["reserved_bytes"] = json!("0");
        response["import"]["cancellation_reserved_bytes"] = json!("0");
    }
    // The persisted receipt includes its own final retained-byte counter. Only
    // the decimal counter width can affect its size; bounded fixed-point sizing
    // produces the exact encrypted response before settling any ledger.
    let mut final_sealed = None;
    for _ in 0..20 {
        let candidate = crypto::seal_receipt(
            key,
            ctx.organization_id,
            request,
            &format!("import:{}:{action}", ctx.actor_user_id.0),
            &bytes(response)?,
        )
        .map_err(|_| MigrationError::Crypto)?;
        if let Some(retained) = retained {
            let total = retained
                .checked_add(sealed_bytes(&candidate) + 32)
                .ok_or(MigrationError::StorageLimit)?
                .to_string();
            if response["import"]["retained_bytes"] != total {
                response["import"]["retained_bytes"] = json!(total);
                continue;
            }
        }
        final_sealed = Some(candidate);
        break;
    }
    let sealed = final_sealed.ok_or(MigrationError::StorageLimit)?;
    let actual = sealed_bytes(&sealed) + 32;
    if action == "cancel" {
        release_cancel(conn, ctx.organization_id, run, actual).await?;
    } else {
        charge(conn, ctx.organization_id, run, snapshot, plan, actual).await?;
    }
    sqlx::query("INSERT INTO migration_import_receipt(organization_id,actor_user_id,action,request_id,input_digest,import_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request).bind(digest.as_slice()).bind(run).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}
pub(crate) async fn eligible(
    conn: &mut PgConnection,
    org: OrganizationId,
    snapshot: Uuid,
    preview: Uuid,
) -> Result<(i64, i64), MigrationError> {
    if !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_snapshot s JOIN migration_snapshot_preview p ON p.snapshot_id=s.id AND p.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 AND p.id=$3)").bind(snapshot).bind(org.0).bind(preview).fetch_one(&mut *conn).await?{return Err(MigrationError::NotFound)}
    let r=sqlx::query("SELECT s.source_account_id,s.capture_sequence FROM migration_snapshot s JOIN migration_snapshot_preview p ON p.snapshot_id=s.id AND p.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 AND p.id=$3 AND s.profile_version='fub-core-v1' AND s.state IN ('completed','completed_with_gaps') AND p.state='completed' AND p.capture_sequence=s.capture_sequence AND (SELECT count(*) FROM migration_snapshot_stream t WHERE t.snapshot_id=s.id AND t.organization_id=s.organization_id AND t.stream IN ('people','users','stages') AND t.state='completed')=3").bind(snapshot).bind(org.0).bind(preview).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    Ok((r.get("source_account_id"), r.get("capture_sequence")))
}
#[tracing::instrument(name="migration.people_import.propose",skip_all,fields(organization_id=%ctx.organization_id,actor_id=%ctx.actor_user_id))]
async fn propose_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: PlanPeopleImport,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    if let Some(v) = replay(&mut tx, key, ctx, "plan", cmd.request_id, &cmd).await? {
        return Ok(v);
    }
    let (account, boundary) = eligible(
        &mut tx,
        ctx.organization_id,
        cmd.snapshot_id,
        cmd.preview_id,
    )
    .await?;
    let id = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let patch = seal(
        key,
        ctx.organization_id,
        cmd.snapshot_id,
        plan,
        plan,
        "patch",
        &json!({"stage_mappings":[],"assignee_mappings":[]}),
    )?;
    sqlx::query("INSERT INTO migration_import(id,organization_id,snapshot_id,preview_id,source_account_id,capture_sequence,executor_user_id,state) VALUES($1,$2,$3,$4,$5,$6,$7,'proposed')").bind(id).bind(ctx.organization_id.0).bind(cmd.snapshot_id).bind(cmd.preview_id).bind(account).bind(boundary).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO migration_import_plan(id,import_id,snapshot_id,organization_id,revision,state,patch_nonce,patch_ciphertext) VALUES($1,$2,$3,$4,1,'building',$5,$6)").bind(plan).bind(id).bind(cmd.snapshot_id).bind(ctx.organization_id.0).bind(patch.nonce.as_slice()).bind(&patch.ciphertext).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_import SET latest_plan_id=$3 WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(ctx.organization_id.0)
        .bind(plan)
        .execute(&mut *tx)
        .await?;
    charge(
        &mut tx,
        ctx.organization_id,
        id,
        cmd.snapshot_id,
        plan,
        sealed_bytes(&patch),
    )
    .await?;
    let cancel_token = Uuid::new_v4();
    if !reserve(
        &mut tx,
        ctx.organization_id,
        id,
        cmd.snapshot_id,
        plan,
        cancel_token,
        cancel_token,
        CANCEL_RESERVATION,
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_import_reservation SET purpose='cancel',expires_at='infinity' WHERE token=$1 AND organization_id=$2").bind(cancel_token).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_import SET cancel_reservation_token=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cancel_token).execute(&mut *tx).await?;
    let mut result = json!({"import_id":id,"plan_id":plan,"state":"building"});
    receipt(
        &mut tx,
        key,
        ctx,
        "plan",
        cmd.request_id,
        &cmd,
        id,
        cmd.snapshot_id,
        plan,
        &mut result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
#[tracing::instrument(name="migration.people_import.replan",skip_all,fields(organization_id=%ctx.organization_id,actor_id=%ctx.actor_user_id))]
async fn replan_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ReplanPeopleImport,
) -> Result<Value, MigrationError> {
    if cmd
        .expected_plan_revision
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .is_none_or(|v| v.to_string() != cmd.expected_plan_revision)
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut seen = BTreeSet::new();
    if cmd.stage_mappings.len() + cmd.assignee_mappings.len() > 50 {
        return Err(MigrationError::InvalidImportChoice);
    }
    for (kind, k) in cmd
        .stage_mappings
        .iter()
        .map(|v| ("stage", &v.source_key))
        .chain(
            cmd.assignee_mappings
                .iter()
                .map(|v| ("assignee", &v.source_key)),
        )
    {
        if k.is_empty() || k.len() > 160 || !seen.insert((kind, k)) {
            return Err(MigrationError::InvalidImportChoice);
        }
    }
    let mut tx = begin(pool, ctx, false).await?;
    let input = json!({"id":id,"command":&cmd});
    if let Some(v) = replay(&mut tx, key, ctx, "replan", cmd.request_id, &input).await? {
        return Ok(v);
    }
    let r = run(&mut tx, ctx.organization_id, id).await?;
    if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some()
        || r.get::<String, _>("state") == "cancelled"
    {
        return Err(MigrationError::ImportConflict);
    }
    let parent: Uuid = r.get("latest_plan_id");
    let old = plan(&mut tx, ctx.organization_id, id, parent).await?;
    if old.get::<String, _>("state") == "building" {
        return Err(MigrationError::ImportBusy);
    }
    if old.get::<i64, _>("revision").to_string() != cmd.expected_plan_revision {
        return Err(MigrationError::ImportConflict);
    }
    for (kind, k) in &seen {
        if !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4)").bind(parent).bind(ctx.organization_id.0).bind(kind).bind(k).fetch_one(&mut *tx).await?{return Err(MigrationError::InvalidImportChoice);}
    }
    release_all(&mut tx, ctx.organization_id, id).await?;
    let plan = Uuid::new_v4();
    let snapshot: Uuid = r.get("snapshot_id");
    let patch = seal(
        key,
        ctx.organization_id,
        snapshot,
        plan,
        plan,
        "patch",
        &json!({"stage_mappings":cmd.stage_mappings,"assignee_mappings":cmd.assignee_mappings}),
    )?;
    sqlx::query(
        "UPDATE migration_import_plan SET state='superseded' WHERE id=$1 AND organization_id=$2",
    )
    .bind(parent)
    .bind(ctx.organization_id.0)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO migration_import_plan(id,import_id,snapshot_id,organization_id,revision,parent_plan_id,state,patch_nonce,patch_ciphertext) VALUES($1,$2,$3,$4,$5,$6,'building',$7,$8)").bind(plan).bind(id).bind(snapshot).bind(ctx.organization_id.0).bind(old.get::<i64,_>("revision")+1).bind(parent).bind(patch.nonce.as_slice()).bind(&patch.ciphertext).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_import SET latest_plan_id=$3,state='proposed',phase='preparation',executor_user_id=$4,lease_token=NULL,lease_expires_at=NULL,pause_reason=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    charge(
        &mut tx,
        ctx.organization_id,
        id,
        snapshot,
        plan,
        sealed_bytes(&patch),
    )
    .await?;
    let mut response = json!({"import_id":id,"plan_id":plan,"state":"building"});
    receipt(
        &mut tx,
        key,
        ctx,
        "replan",
        cmd.request_id,
        &input,
        id,
        snapshot,
        plan,
        &mut response,
    )
    .await?;
    tx.commit().await?;
    Ok(response)
}

/// Explicit, reviewed inventory. Configuration and terminal IDs-only Operator
/// audit are intentionally absent; no destructive cleanup establishes emptiness.
pub const EMPTY_TABLES: &[&str] = &[
    "person",
    "contact_method",
    "inquiry",
    "inquiry_received",
    "routing_decision",
    "assignment_changed",
    "stage_changed",
    "contact_attempted",
    "person_tag",
    "person_custom_field_value",
    "note",
    "task",
    "call",
    "call_completed",
    "raw_payload",
    "intake_extraction",
    "correspondence_raw",
    "correspondence_captured",
    "capture_message",
];
pub async fn empty(conn: &mut PgConnection, org: OrganizationId) -> Result<bool, MigrationError> {
    for table in EMPTY_TABLES {
        let q = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE organization_id=$1)");
        if sqlx::query_scalar::<_, bool>(&q)
            .bind(org.0)
            .fetch_one(&mut *conn)
            .await?
        {
            return Ok(false);
        }
    }
    let busy=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM operator_task_proposal t JOIN operator_proposal p ON p.id=t.proposal_id WHERE p.organization_id=$1) OR EXISTS(SELECT 1 FROM operator_proposal WHERE organization_id=$1 AND (status='claimed' OR (status='proposed' AND expires_at>now()))) OR EXISTS(SELECT 1 FROM workspace_operation_admission WHERE organization_id=$1 AND deadline>clock_timestamp())").bind(org.0).fetch_one(conn).await?;
    Ok(!busy)
}
#[tracing::instrument(name="migration.people_import.confirm",skip_all,fields(organization_id=%ctx.organization_id,actor_id=%ctx.actor_user_id))]
async fn confirm_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmPeopleImport,
    release: &workspace::ReleaseReadiness,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, true).await?;
    let input = json!({"id":id,"command":&cmd});
    if let Some(v) = replay(&mut tx, key, ctx, "confirm", cmd.request_id, &input).await? {
        return Ok(v);
    }
    release
        .require_current(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    let p = plan(&mut tx, ctx.organization_id, id, cmd.plan_id).await?;
    if p.get::<Option<DateTime<Utc>>, _>("expires_at")
        .is_some_and(|v| v <= Utc::now())
    {
        return Err(MigrationError::ImportExpired);
    }
    if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some()
        || r.get::<Uuid, _>("latest_plan_id") != cmd.plan_id
        || r.get::<String, _>("state") != "proposed"
        || p.get::<String, _>("state") != "ready"
        || p.get::<i64, _>("revision").to_string() != cmd.plan_revision
        || p.get::<Option<DateTime<Utc>>, _>("expires_at")
            .is_none_or(|v| v <= Utc::now())
        || hex(&p.get::<Vec<u8>, _>("confirmation_digest")) != cmd.confirmation_digest
    {
        return Err(MigrationError::ImportConflict);
    }
    if p.get::<i64, _>("eligible_people") == 0
        || p.get::<i64, _>("held_people").to_string() != cmd.acknowledgments.held_count
        || !cmd.acknowledgments.review_only
        || !cmd.acknowledgments.remaining_data
    {
        return Err(MigrationError::InvalidImportChoice);
    }
    let snapshot: Uuid = r.get("snapshot_id");
    eligible(&mut tx, ctx.organization_id, snapshot, r.get("preview_id")).await?;
    let binding = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=$1)",
    )
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    if binding
        || workspace::mode(&mut tx, ctx.organization_id).await?.0 != "operational"
        || !empty(&mut tx, ctx.organization_id).await?
    {
        return Err(MigrationError::WorkspaceNotEmpty);
    }
    // Destination decisions are rechecked under the same exclusive workspace lock.
    // Each scan page contains at most 50 descriptors; payloads load one at a time.
    let mut cursor = Uuid::nil();
    loop {
        let rows=sqlx::query("SELECT id,kind,disposition,target_id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND disposition IN ('existing','create','member') AND dependent_count>0 ORDER BY id LIMIT 50").bind(cmd.plan_id).bind(ctx.organization_id.0).bind(cursor).fetch_all(&mut *tx).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            cursor = row.get("id");
            let m=sqlx::query("SELECT nonce,ciphertext FROM migration_import_mapping WHERE id=$1 AND organization_id=$2").bind(cursor).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
            let mapping: super::import_worker::Mapping = open(
                key,
                ctx.organization_id,
                snapshot,
                cmd.plan_id,
                cursor,
                "mapping",
                &m.get::<Vec<u8>, _>("nonce"),
                &m.get::<Vec<u8>, _>("ciphertext"),
            )?;
            let target: Uuid = row.get("target_id");
            let disposition: String = row.get("disposition");
            let valid=match disposition.as_str(){
 "member"=>sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.user_id=$2 AND m.status='active' AND u.email=$3)").bind(ctx.organization_id.0).bind(target).bind(mapping.target.as_ref().and_then(|v|v["email"].as_str())).fetch_one(&mut *tx).await?,
 "existing"=>sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM stage WHERE organization_id=$1 AND id=$2 AND name=$3)").bind(ctx.organization_id.0).bind(target).bind(mapping.target.as_ref().and_then(|v|v["name"].as_str())).fetch_one(&mut *tx).await?,
 "create"=>!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM stage WHERE organization_id=$1 AND name=$2)").bind(ctx.organization_id.0).bind(mapping.target.as_ref().and_then(|v|v["name"].as_str())).fetch_one(&mut *tx).await?,_=>false};
            if !valid {
                return Err(MigrationError::InvalidImportChoice);
            }
        }
    }
    let available: i64 = sqlx::query_scalar(
        "SELECT 32767-COALESCE(max(position)::bigint,-1) FROM stage WHERE organization_id=$1",
    )
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    if p.get::<i64, _>("stages_to_create") > available {
        return Err(MigrationError::InvalidImportChoice);
    }
    sqlx::query("INSERT INTO migration_workspace(organization_id,import_id,plan_id,entered_by_user_id,gate_version) VALUES($1,$2,$3,$4,'crm-workspace-v1')").bind(ctx.organization_id.0).bind(id).bind(cmd.plan_id).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_import SET state='queued',phase='stages',confirmed_plan_id=$3,executor_user_id=$4,confirmed_at=now(),lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.plan_id).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let mode = workspace::mode(&mut tx, ctx.organization_id).await?;
    let mut response = json!({"import":detail_in(&mut tx,ctx.organization_id,id).await?,"receipt":{"request_id":cmd.request_id,"plan_id":cmd.plan_id,"plan_revision":cmd.plan_revision,"confirmed_by_user_id":ctx.actor_user_id.0,"held_count":cmd.acknowledgments.held_count},"workspace_mode":mode.0,"workspace_revision":mode.1.to_string()});
    receipt(
        &mut tx,
        key,
        ctx,
        "confirm",
        cmd.request_id,
        &input,
        id,
        snapshot,
        cmd.plan_id,
        &mut response,
    )
    .await?;
    tx.commit().await?;
    Ok(response)
}
/// Resolve an already committed confirmation before the HTTP layer consults
/// current fleet readiness. Current tenant/admin authority and the exact
/// actor/action/input digest still apply to every receipt read.
pub async fn replay_confirmation(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: &ConfirmPeopleImport,
) -> Result<Option<Value>, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    let input = json!({"id":id,"command":cmd});
    let result = replay(&mut tx, key, ctx, "confirm", cmd.request_id, &input).await?;
    if result.is_none() {
        run(&mut tx, ctx.organization_id, id).await?;
    }
    Ok(result)
}
#[tracing::instrument(name="migration.people_import.action",skip_all,fields(organization_id=%ctx.organization_id,actor_id=%ctx.actor_user_id))]
async fn action_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ImportRequest,
    retry: bool,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    let action = if retry { "retry" } else { "cancel" };
    let input = json!({"id":id,"request_id":cmd.request_id});
    if let Some(v) = replay(&mut tx, key, ctx, action, cmd.request_id, &input).await? {
        return Ok(v);
    }
    let r = run(&mut tx, ctx.organization_id, id).await?;
    let state: String = r.get("state");
    let plan: Uuid = r.get("latest_plan_id");
    let snapshot: Uuid = r.get("snapshot_id");
    if retry {
        if state != "paused" {
            return Err(MigrationError::ImportConflict);
        };
        eligible(&mut tx, ctx.organization_id, snapshot, r.get("preview_id")).await?;
        sqlx::query("UPDATE migration_import SET state=CASE WHEN confirmed_plan_id IS NULL THEN 'proposed' ELSE 'queued' END,pause_reason=NULL,executor_user_id=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_import_plan SET state='building',pause_reason=NULL WHERE id=$1 AND organization_id=$2 AND state='paused'").bind(plan).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    } else {
        if matches!(state.as_str(), "completed" | "cancelled" | "expired") {
            return Err(MigrationError::ImportConflict);
        }
        sqlx::query("UPDATE migration_import SET state='cancelled',lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    }
    release_all(&mut tx, ctx.organization_id, id).await?;
    let mut result = json!({"import":detail_in(&mut tx,ctx.organization_id,id).await?});
    receipt(
        &mut tx,
        key,
        ctx,
        action,
        cmd.request_id,
        &input,
        id,
        snapshot,
        plan,
        &mut result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
async fn detail_in(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r = run(conn, org, id).await?;
    let p = plan(conn, org, id, r.get("latest_plan_id")).await?;
    let mut counts = serde_json::Map::new();
    for name in [
        "source_people",
        "eligible_people",
        "held_people",
        "invalid_ids",
        "contacts",
        "overlap_people",
        "stages_to_create",
        "assigned_people",
        "unassigned_people",
    ] {
        counts.insert(name.into(), json!(p.get::<i64, _>(name).to_string()));
    }
    for name in ["imported_people", "imported_contacts"] {
        counts.insert(name.into(), json!(r.get::<i64, _>(name).to_string()));
    }
    let finished = r.get::<i64, _>("settled_people");
    counts.insert(
        "pending_people".into(),
        json!((p.get::<i64, _>("source_people") - finished)
            .max(0)
            .to_string()),
    );
    let issues=sqlx::query("SELECT code,record_count FROM migration_import_issue WHERE plan_id=$1 AND organization_id=$2 ORDER BY code").bind(p.get::<Uuid,_>("id")).bind(org.0).fetch_all(&mut *conn).await?;
    counts.insert(
        "reasons".into(),
        Value::Object(
            issues
                .into_iter()
                .map(|v| {
                    (
                        v.get::<String, _>("code"),
                        json!(v.get::<i64, _>("record_count").to_string()),
                    )
                })
                .collect(),
        ),
    );
    let policy = POLICY
        .try_with(Clone::clone)
        .map_err(|_| MigrationError::StorageLimit)?;
    let state: String = r.get("state");
    let ready = p.get::<String, _>("state") == "ready";
    let expired = p
        .get::<Option<DateTime<Utc>>, _>("expires_at")
        .is_some_and(|v| v <= Utc::now());
    let confirmed: Option<Uuid> = r.get("confirmed_plan_id");
    let mode = workspace::mode(conn, org).await?;
    let source=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,(SELECT captured_at FROM migration_snapshot_capture c WHERE c.snapshot_id=s.id AND c.organization_id=s.organization_id ORDER BY sequence LIMIT 1) AS first_observed_at,(SELECT captured_at FROM migration_snapshot_capture c WHERE c.snapshot_id=s.id AND c.organization_id=s.organization_id ORDER BY sequence DESC LIMIT 1) AS last_observed_at,l.byte_limit FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *conn).await?;
    let required = if ready {
        p.get::<i64, _>("max_added_byte_bound")
    } else {
        UNIT
    };
    Ok(json!({
      "id":id,"snapshot_id":r.get::<Uuid,_>("snapshot_id"),"preview_id":r.get::<Uuid,_>("preview_id"),
      "source_account_id":r.get::<i64,_>("source_account_id").to_string(),"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),
      "state":state,"phase":r.get::<String,_>("phase"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),
      "latest_plan_id":r.get::<Uuid,_>("latest_plan_id"),"confirmed_plan_id":confirmed,"executor_user_id":r.get::<Uuid,_>("executor_user_id"),
      "created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"counts":counts,
      "retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),
      "cancellation_reserved_bytes":if r.get::<Option<Uuid>,_>("cancel_reservation_token").is_some(){CANCEL_RESERVATION.to_string()}else{"0".into()},
      "actions":{"confirm":state=="proposed"&&ready&&!expired&&p.get::<i64,_>("eligible_people")>0&&mode.0=="operational",
        "replan":confirmed.is_none()&&state!="cancelled"&&p.get::<String,_>("state")!="building","retry":state=="paused",
        "cancel":matches!(state.as_str(),"proposed"|"queued"|"running"|"paused")},
      "plan":{"id":p.get::<Uuid,_>("id"),"revision":p.get::<i64,_>("revision").to_string(),"state":p.get::<String,_>("state"),
        "phase":p.get::<String,_>("phase"),"confirmation_digest":p.get::<Option<Vec<u8>>,_>("confirmation_digest").map(|v|hex(&v)),
        "expires_at":p.get::<Option<DateTime<Utc>>,_>("expires_at"),"expired":expired,"counts":counts,
        "required_reservation_bytes":required.to_string(),"created_at":p.get::<DateTime<Utc>,_>("created_at"),"completed_at":p.get::<Option<DateTime<Utc>>,_>("completed_at")},
      "workspace":{"mode":mode.0,"revision":mode.1.to_string(),"activation_available":false},
      "coverage":{"imported_families":["people"],"remaining_families":["custom_fields","notes","tasks","emails","calls","inquiries","deals","relationships","addresses","tags","appointments","automation_settings"],"source_remains_retained":true,"cutover_complete":false},
      "source_window":{"first_observed_at":source.get::<Option<DateTime<Utc>>,_>("first_observed_at"),"last_observed_at":source.get::<Option<DateTime<Utc>>,_>("last_observed_at")},
      "policy":{"run_byte_limit":source.get::<i64,_>("run_byte_limit").to_string(),"org_byte_limit":source.get::<i64,_>("byte_limit").to_string(),"unit_ceiling_bytes":UNIT.to_string(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"policy_revision":policy.revision()},
      "engine_version":ENGINE
    }))
}
async fn detail_inner(
    pool: &PgPool,
    _key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    detail_in(&mut tx, ctx.organization_id, id).await
}
async fn list_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.kind.is_some() || q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx, false).await?;
    let cursor = snapshot::decode_cursor(
        key,
        ctx.organization_id,
        Uuid::nil(),
        "import-list",
        q.cursor.as_deref(),
    )?;
    let (time, id) = match cursor {
        Some(v) => (
            serde_json::from_value::<DateTime<Utc>>(v["created_at"].clone())
                .map_err(|_| MigrationError::InvalidInput)?,
            serde_json::from_value::<Uuid>(v["id"].clone())
                .map_err(|_| MigrationError::InvalidInput)?,
        ),
        None => (DateTime::<Utc>::MAX_UTC, Uuid::max()),
    };
    let rows=sqlx::query("SELECT id,created_at FROM migration_import WHERE organization_id=$1 AND (created_at,id)<($2,$3) ORDER BY created_at DESC,id DESC LIMIT $4").bind(ctx.organization_id.0).bind(time).bind(id).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut items = vec![];
    for r in rows.iter().take(limit as usize) {
        items.push(detail_in(&mut tx, ctx.organization_id, r.get("id")).await?)
    }
    let next = if rows.len() > items.len() {
        let r = &rows[items.len() - 1];
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            Uuid::nil(),
            "import-list",
            &json!({"id":r.get::<Uuid,_>("id"),"created_at":r.get::<DateTime<Utc>,_>("created_at")}),
        )?)
    } else {
        None
    };
    Ok(json!({"imports":items,"next_cursor":next}))
}
fn page_cursor(
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    scope: &str,
    q: &ImportPage,
) -> Result<String, MigrationError> {
    Ok(
        snapshot::decode_cursor(key, org, id, scope, q.cursor.as_deref())?
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or(MigrationError::InvalidInput)
            })
            .transpose()?
            .unwrap_or_default(),
    )
}
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan_id: Uuid,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.kind.is_some()
        || q.disposition
            .as_deref()
            .is_some_and(|v| !matches!(v, "eligible" | "held"))
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx, false).await?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    plan(&mut tx, ctx.organization_id, id, plan_id).await?;
    let snapshot = r.get("snapshot_id");
    let scope = format!(
        "import-records:{plan_id}:{}",
        q.disposition.as_deref().unwrap_or("all")
    );
    let after = page_cursor(key, ctx.organization_id, id, &scope, &q)?;
    let rows=sqlx::query("SELECT id,source_id FROM migration_import_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id>$3 AND ($4::text IS NULL OR disposition=$4) ORDER BY source_id LIMIT $5").bind(plan_id).bind(ctx.organization_id.0).bind(after).bind(&q.disposition).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut values = vec![];
    let mut last = None;
    let mut used = 128usize;
    for row in rows.iter().take(limit as usize) {
        let m=sqlx::query("SELECT * FROM migration_import_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(row.get::<Uuid,_>("id")).bind(plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let source=sqlx::query("SELECT * FROM migration_import_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(m.get::<Uuid,_>("source_row_id")).bind(plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let item: super::import_source::ExtractedRecord = open(
            key,
            ctx.organization_id,
            snapshot,
            plan_id,
            source.get("id"),
            "source",
            &source.get::<Vec<u8>, _>("nonce"),
            &source.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let manifest: super::import_worker::Manifest = open(
            key,
            ctx.organization_id,
            snapshot,
            plan_id,
            m.get("id"),
            "manifest",
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let value = json!({"id":m.get::<Uuid,_>("id"),"source_id":m.get::<String,_>("source_id"),"disposition":m.get::<String,_>("disposition"),"held_reasons":manifest.reasons,"transformations":manifest.transformations,"proposed":super::import_display::summary(&item),"overlap_count":m.get::<i64,_>("overlap_count").to_string(),"added_byte_bound":m.get::<i64,_>("added_byte_bound").to_string()});
        let size = bytes(&value)?.len();
        if used + size > DISPLAY_LIMIT {
            break;
        }
        used += size;
        last = Some(m.get::<String, _>("source_id"));
        values.push(value);
    }
    let next = if rows.len() > values.len() {
        last.map(|v| snapshot::encode_cursor(key, ctx.organization_id, id, &scope, &json!(v)))
            .transpose()?
    } else {
        None
    };
    Ok(json!({"records":values,"next_cursor":next}))
}
pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan_id: Uuid,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    let kind = q
        .kind
        .as_deref()
        .filter(|v| matches!(*v, "stage" | "assignee"))
        .ok_or(MigrationError::InvalidInput)?;
    if q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx, false).await?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    plan(&mut tx, ctx.organization_id, id, plan_id).await?;
    let scope = format!("import-mappings:{plan_id}:{kind}");
    let after = page_cursor(key, ctx.organization_id, id, &scope, &q)?;
    let rows=sqlx::query("SELECT id,source_key FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key>$4 ORDER BY source_key LIMIT $5").bind(plan_id).bind(ctx.organization_id.0).bind(kind).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut values = vec![];
    let mut used = 128;
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        let m=sqlx::query("SELECT * FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(row.get::<Uuid,_>("id")).bind(plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let body: super::import_worker::Mapping = open(
            key,
            ctx.organization_id,
            r.get("snapshot_id"),
            plan_id,
            m.get("id"),
            "mapping",
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let value = json!({"id":m.get::<Uuid,_>("id"),"source_key":m.get::<String,_>("source_key"),"qualified":m.get::<bool,_>("qualified"),"disposition":m.get::<String,_>("disposition"),"source":body.source.as_ref().map(super::import_display::summary),"choice":body.choice,"suggestions":body.suggestions,"target":body.target,"reasons":body.reasons,"dependent_count":m.get::<i64,_>("dependent_count").to_string()});
        let size = bytes(&value)?.len();
        if used + size > DISPLAY_LIMIT {
            break;
        }
        used += size;
        last = Some(m.get::<String, _>("source_key"));
        values.push(value);
    }
    let next = if rows.len() > values.len() {
        last.map(|v| snapshot::encode_cursor(key, ctx.organization_id, id, &scope, &json!(v)))
            .transpose()?
    } else {
        None
    };
    Ok(json!({"mappings":values,"next_cursor":next}))
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.kind.is_some()
        || q.disposition
            .as_deref()
            .is_some_and(|v| !matches!(v, "imported" | "already_imported" | "held" | "pending"))
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx, false).await?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    let plan_id: Uuid = r
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .unwrap_or(r.get("latest_plan_id"));
    let scope = format!(
        "import-results:{plan_id}:{}",
        q.disposition.as_deref().unwrap_or("all")
    );
    let after = page_cursor(key, ctx.organization_id, id, &scope, &q)?;
    // Keep each indexed result lookup correlated to the ordered manifest page.
    // Constant OFFSET 0 is an optimization boundary, not offset pagination;
    // it preserves all matching rows and the filter remains outside the lookup.
    let rows=sqlx::query("SELECT m.id,m.source_id,m.disposition AS planned,r.disposition,r.person_id,r.contact_count,r.committed_at FROM migration_import_manifest m LEFT JOIN LATERAL (SELECT x.disposition,x.person_id,x.contact_count,x.committed_at FROM migration_import_result x WHERE x.manifest_id=m.id AND x.organization_id=m.organization_id OFFSET 0) r ON TRUE WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.source_id>$3 AND ($4::text IS NULL OR COALESCE(r.disposition,'pending')=$4) ORDER BY m.source_id LIMIT $5").bind(plan_id).bind(ctx.organization_id.0).bind(after).bind(&q.disposition).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut values = vec![];
    for row in rows.iter().take(limit as usize) {
        let m=sqlx::query("SELECT nonce,ciphertext FROM migration_import_manifest WHERE id=$1 AND organization_id=$2").bind(row.get::<Uuid,_>("id")).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let body: super::import_worker::Manifest = open(
            key,
            ctx.organization_id,
            r.get("snapshot_id"),
            plan_id,
            row.get("id"),
            "manifest",
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )?;
        values.push(json!({"id":row.get::<Uuid,_>("id"),"source_id":row.get::<String,_>("source_id"),"disposition":row.get::<Option<String>,_>("disposition").unwrap_or_else(||"pending".into()),"planned_disposition":row.get::<String,_>("planned"),"person_id":row.get::<Option<Uuid>,_>("person_id"),"contact_count":row.get::<Option<i64>,_>("contact_count").map(|v|v.to_string()),"committed_at":row.get::<Option<DateTime<Utc>>,_>("committed_at"),"held_reasons":body.reasons}));
    }
    let next = if rows.len() > values.len() {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            id,
            &scope,
            &json!(rows[values.len() - 1].get::<String, _>("source_id")),
        )?)
    } else {
        None
    };
    Ok(json!({"results":values,"next_cursor":next}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}
fn segment_response(
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    scope: &str,
    text: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let offset = snapshot::decode_cursor(key, org, id, scope, q.cursor.as_deref())?
        .map(|v| {
            v.as_str()
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or(MigrationError::InvalidInput)
        })
        .transpose()?
        .unwrap_or(0);
    let s = super::import_display::segment(text, offset, q.limit.unwrap_or(16384))
        .map_err(|_| MigrationError::InvalidInput)?;
    let next = s
        .next_offset
        .as_ref()
        .map(|v| snapshot::encode_cursor(key, org, id, scope, &json!(v)))
        .transpose()?;
    Ok(
        json!({"text":s.text,"offset":s.offset,"full_utf8_bytes":s.full_utf8_bytes,"next_cursor":next}),
    )
}
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, resource, crypto and request scopes must remain independent"
)]
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan_id: Uuid,
    record: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    plan(&mut tx, ctx.organization_id, id, plan_id).await?;
    let row=sqlx::query("SELECT s.* FROM migration_import_manifest m JOIN migration_import_source s ON s.id=m.source_row_id AND s.organization_id=m.organization_id WHERE m.id=$1 AND m.plan_id=$2 AND m.import_id=$3 AND m.organization_id=$4").bind(record).bind(plan_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let item: super::import_source::ExtractedRecord = open(
        key,
        ctx.organization_id,
        r.get("snapshot_id"),
        plan_id,
        row.get("id"),
        "source",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let text = super::import_display::field_text(&item, field).ok_or(MigrationError::NotFound)?;
    segment_response(
        key,
        ctx.organization_id,
        id,
        &format!("import-field:{plan_id}:{record}:{field}"),
        &text,
        q,
    )
}
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, resource, crypto and request scopes must remain independent"
)]
pub async fn mapping_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan_id: Uuid,
    mapping: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    let r = run(&mut tx, ctx.organization_id, id).await?;
    plan(&mut tx, ctx.organization_id, id, plan_id).await?;
    let row = sqlx::query("SELECT nonce,ciphertext FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4")
        .bind(mapping).bind(plan_id).bind(id).bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let body: super::import_worker::Mapping = open(
        key,
        ctx.organization_id,
        r.get("snapshot_id"),
        plan_id,
        mapping,
        "mapping",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let source = body.source.as_ref().ok_or(MigrationError::NotFound)?;
    let text = super::import_display::field_text(source, field).ok_or(MigrationError::NotFound)?;
    segment_response(
        key,
        ctx.organization_id,
        id,
        &format!("import-mapping-field:{plan_id}:{mapping}:{field}"),
        &text,
        q,
    )
}
pub async fn provenance(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    field: Option<(&str, FieldQuery)>,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx, false).await?;
    let row=sqlx::query("SELECT r.*,i.snapshot_id,s.record_id,s.capture_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id JOIN migration_import_manifest m ON m.id=r.manifest_id AND m.organization_id=r.organization_id JOIN migration_import_source s ON s.id=m.source_row_id AND s.organization_id=m.organization_id JOIN person p ON p.id=r.person_id AND p.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.person_id=$2 AND r.disposition='imported'").bind(ctx.organization_id.0).bind(person).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let item: super::import_source::ExtractedRecord = open(
        key,
        ctx.organization_id,
        row.get("snapshot_id"),
        row.get("plan_id"),
        row.get("id"),
        "provenance",
        &row.get::<Vec<u8>, _>("provenance_nonce"),
        &row.get::<Vec<u8>, _>("provenance_ciphertext"),
    )?;
    if let Some((name, q)) = field {
        let text =
            super::import_display::field_text(&item, name).ok_or(MigrationError::NotFound)?;
        return segment_response(
            key,
            ctx.organization_id,
            person,
            &format!("import-provenance:{}:{name}", row.get::<Uuid, _>("id")),
            &text,
            q,
        );
    }
    Ok(
        json!({"person_id":person,"import_id":row.get::<Uuid,_>("import_id"),"plan_id":row.get::<Uuid,_>("plan_id"),"snapshot_id":row.get::<Uuid,_>("snapshot_id"),"source_record_id":row.get::<Uuid,_>("record_id"),"capture_id":row.get::<Uuid,_>("capture_id"),"source_id":row.get::<String,_>("source_id"),"committed_at":row.get::<DateTime<Utc>,_>("committed_at"),"source":super::import_display::summary(&item),"operational_use_available":false}),
    )
}

pub(crate) async fn release_cancel(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let token = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT cancel_reservation_token FROM migration_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?;
    if let Some(token) = token {
        settle(conn, org, id, token, actual).await?;
        sqlx::query("UPDATE migration_import SET cancel_reservation_token=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
    } else if actual != 0 {
        return Err(MigrationError::ImportConflict);
    }
    Ok(())
}

pub async fn propose(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: PlanPeopleImport,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, propose_inner(pool, key, ctx, cmd)).await
}
pub async fn replan(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ReplanPeopleImport,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, replan_inner(pool, key, ctx, id, cmd)).await
}
#[allow(
    clippy::too_many_arguments,
    reason = "Command and server policy are independent trusted inputs"
)]
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmPeopleImport,
    release: &workspace::ReleaseReadiness,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, confirm_inner(pool, key, ctx, id, cmd, release)).await
}
pub async fn action(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ImportRequest,
    retry: bool,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, action_inner(pool, key, ctx, id, cmd, retry)).await
}
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, detail_inner(pool, key, ctx, id)).await
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: ImportPage,
    policy: &snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    with_policy(policy, list_inner(pool, key, ctx, q)).await
}

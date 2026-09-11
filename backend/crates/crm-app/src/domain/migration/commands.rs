//! Typed commands and queries; all boundaries recheck trusted active admin context.
use super::{
    crypto, is_probed_check,
    reader::{self, FubReader, ReaderError},
    store, AssessmentView, ConnectionView, MigrationError, CHECKS, FUB_PROFILE_V1,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use sqlx::{PgPool, Row};
use uuid::Uuid;
// One in-process source/command lane matches the single modular workload.
// This prevents duplicate validation before receipt commit without holding a DB
// connection across source I/O. DB organization locks also serialize mutations.
static COMMAND_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub struct ConnectFub {
    pub request_id: Uuid,
    pub api_key: String,
}
pub struct ReplaceFubCredential {
    pub request_id: Uuid,
    pub connection_id: Uuid,
    pub expected_revision: i32,
    pub api_key: String,
}
pub struct StartFubAssessment {
    pub request_id: Uuid,
    pub connection_id: Uuid,
    pub expected_revision: i32,
}
pub struct DisconnectFub {
    pub connection_id: Uuid,
}
pub struct RetryFubAssessment {
    pub assessment_id: Uuid,
}
pub struct CancelFubAssessment {
    pub assessment_id: Uuid,
}
#[derive(serde::Serialize)]
pub struct FubSummary {
    pub connection: Option<ConnectionView>,
    pub active_assessment: Option<AssessmentView>,
    pub latest_assessment: Option<AssessmentView>,
    pub latest_report: Option<AssessmentView>,
}
pub struct Idempotent<T> {
    pub value: T,
    pub replayed: bool,
}
async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}
fn valid_key(key: &str) -> bool {
    !key.trim().is_empty() && key.len() <= 12 * 1024 && !key.contains(['\0', '\r', '\n', ':'])
}
fn source_error(error: ReaderError) -> MigrationError {
    match error.split().0 {
        ReaderError::InvalidCredential => MigrationError::InvalidCredential,
        ReaderError::AccessDenied => MigrationError::Forbidden,
        _ => MigrationError::ReaderUnavailable,
    }
}
fn replay<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<Idempotent<T>, MigrationError> {
    Ok(Idempotent {
        value: serde_json::from_value(value).map_err(|_| MigrationError::Conflict)?,
        replayed: true,
    })
}
async fn save<T: serde::Serialize>(
    key: &RawPayloadKey,
    tx: &mut sqlx::PgConnection,
    ctx: &CommandContext,
    operation: &str,
    id: Uuid,
    digest: &[u8],
    value: &T,
) -> Result<(), MigrationError> {
    store::insert_receipt(
        key,
        tx,
        ctx.organization_id,
        operation,
        id,
        digest,
        &serde_json::to_value(value).map_err(|_| MigrationError::Conflict)?,
    )
    .await
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,actor_id=%ctx.actor_user_id.0,request_id=%cmd.request_id,operation="connect_fub"))]
pub async fn connect_fub(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    ctx: &CommandContext,
    cmd: ConnectFub,
) -> Result<Idempotent<ConnectionView>, MigrationError> {
    let _gate = COMMAND_GATE.lock().await;
    let digest = crypto::request_digest(key, "create_connection", cmd.api_key.as_bytes());
    let mut tx = begin(pool, ctx).await?;
    if let Some(value) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "create_connection",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return replay(value);
    }
    if !valid_key(&cmd.api_key) {
        return Err(MigrationError::InvalidCredential);
    }
    if store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .is_some()
    {
        return Err(MigrationError::Conflict);
    }
    tx.commit().await?;
    let _permit = reader::source_read_permit().await;
    if !store::active_admin(pool, ctx.organization_id, ctx.actor_user_id).await? {
        return Err(MigrationError::Forbidden);
    }
    let (identity, raw) = reader.identity(&cmd.api_key).await.map_err(source_error)?;
    if identity.account_id <= 0 || identity.user_id.unwrap_or_default() <= 0 {
        return Err(MigrationError::InvalidCredential);
    }
    let id = Uuid::new_v4();
    let identity_seal = crypto::seal_identity(key, ctx.organization_id, id, 1, &raw)
        .map_err(|_| MigrationError::Crypto)?;
    let credential =
        crypto::seal_credential(key, ctx.organization_id, id, 1, cmd.api_key.as_bytes())
            .map_err(|_| MigrationError::Crypto)?;
    let mut tx = begin(pool, ctx).await?;
    if let Some(value) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "create_connection",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return replay(value);
    }
    if store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .is_some()
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("INSERT INTO migration_connection(id,organization_id,source_account_id,identity_nonce,identity_ciphertext,credential_nonce,credential_ciphertext,status,initiated_by_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,'connected',$8)").bind(id).bind(ctx.organization_id.0).bind(identity.account_id).bind(identity_seal.nonce.as_slice()).bind(identity_seal.ciphertext).bind(credential.nonce.as_slice()).bind(credential.ciphertext).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let value = store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .ok_or(MigrationError::NotFound)?;
    save(
        key,
        &mut tx,
        ctx,
        "create_connection",
        cmd.request_id,
        &digest,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(Idempotent {
        value,
        replayed: false,
    })
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,actor_id=%ctx.actor_user_id.0,request_id=%cmd.request_id,operation="replace_fub_credential"))]
pub async fn replace_fub_credential(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    ctx: &CommandContext,
    cmd: ReplaceFubCredential,
) -> Result<Idempotent<ConnectionView>, MigrationError> {
    let _gate = COMMAND_GATE.lock().await;
    let input = serde_json::to_vec(&(cmd.connection_id, cmd.expected_revision, &cmd.api_key))
        .map_err(|_| MigrationError::InvalidInput)?;
    let digest = crypto::request_digest(key, "replace_credential", &input);
    let mut tx = begin(pool, ctx).await?;
    if let Some(value) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "replace_credential",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return replay(value);
    }
    let current = store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .filter(|v| v.id == cmd.connection_id)
        .ok_or(MigrationError::NotFound)?;
    if current.revision != cmd.expected_revision {
        return Err(MigrationError::Conflict);
    }
    if !valid_key(&cmd.api_key) {
        return Err(MigrationError::InvalidCredential);
    }
    let revision = cmd
        .expected_revision
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    tx.commit().await?;
    let _permit = reader::source_read_permit().await;
    if !store::active_admin(pool, ctx.organization_id, ctx.actor_user_id).await? {
        return Err(MigrationError::Forbidden);
    }
    let (identity, raw) = reader.identity(&cmd.api_key).await.map_err(source_error)?;
    if identity.account_id != current.source_account_id {
        return Err(MigrationError::SourceAccountMismatch);
    }
    if identity.user_id.unwrap_or_default() <= 0 {
        return Err(MigrationError::InvalidCredential);
    }
    let credential = crypto::seal_credential(
        key,
        ctx.organization_id,
        cmd.connection_id,
        revision,
        cmd.api_key.as_bytes(),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let identity =
        crypto::seal_identity(key, ctx.organization_id, cmd.connection_id, revision, &raw)
            .map_err(|_| MigrationError::Crypto)?;
    let mut tx = begin(pool, ctx).await?;
    if let Some(value) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "replace_credential",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return replay(value);
    }
    let changed=sqlx::query("UPDATE migration_connection SET credential_nonce=$4,credential_ciphertext=$5,identity_nonce=$6,identity_ciphertext=$7,revision=$8,identity_revision=$8,status='connected',disconnected_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2 AND revision=$3").bind(cmd.connection_id).bind(ctx.organization_id.0).bind(cmd.expected_revision).bind(credential.nonce.as_slice()).bind(credential.ciphertext).bind(identity.nonce.as_slice()).bind(identity.ciphertext).bind(revision).execute(&mut *tx).await?;
    if changed.rows_affected() != 1 {
        return Err(MigrationError::Conflict);
    }
    store::cancel_connection_jobs(&mut tx, ctx.organization_id, cmd.connection_id).await?;
    let value = store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .ok_or(MigrationError::NotFound)?;
    save(
        key,
        &mut tx,
        ctx,
        "replace_credential",
        cmd.request_id,
        &digest,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(Idempotent {
        value,
        replayed: false,
    })
}
pub async fn disconnect_fub(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DisconnectFub,
) -> Result<(), MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    if sqlx::query(
        "SELECT id FROM migration_connection WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(cmd.connection_id)
    .bind(ctx.organization_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .is_none()
    {
        return Err(MigrationError::NotFound);
    }
    sqlx::query("UPDATE migration_connection SET status='disconnected',credential_nonce=NULL,credential_ciphertext=NULL,revision=revision+1,disconnected_at=now(),updated_at=now() WHERE id=$1 AND organization_id=$2 AND status='connected'").bind(cmd.connection_id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    store::cancel_connection_jobs(&mut tx, ctx.organization_id, cmd.connection_id).await?;
    tx.commit().await?;
    Ok(())
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,actor_id=%ctx.actor_user_id.0,request_id=%cmd.request_id,operation="start_fub_assessment"))]
pub async fn start_fub_assessment(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: StartFubAssessment,
) -> Result<Idempotent<AssessmentView>, MigrationError> {
    let input = serde_json::to_vec(&(cmd.connection_id, cmd.expected_revision))
        .map_err(|_| MigrationError::InvalidInput)?;
    let digest = crypto::request_digest(key, "start_assessment", &input);
    let mut tx = begin(pool, ctx).await?;
    if let Some(value) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "start_assessment",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return replay(value);
    }
    let r = sqlx::query(
        "SELECT * FROM migration_connection WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(cmd.connection_id)
    .bind(ctx.organization_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if r.get::<i32, _>("revision") != cmd.expected_revision
        || r.get::<String, _>("status") != "connected"
    {
        return Err(MigrationError::Conflict);
    }
    super::snapshot::exclusive(&mut tx, ctx.organization_id, Uuid::nil()).await?;
    if sqlx::query("SELECT 1 FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry')").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.is_some(){return Err(MigrationError::Conflict)}
    let id = Uuid::new_v4();
    let raw = crypto::open_identity(
        key,
        ctx.organization_id,
        cmd.connection_id,
        r.get("identity_revision"),
        r.get("identity_nonce"),
        r.get("identity_ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let identity = crypto::seal_identity(key, ctx.organization_id, id, 1, &raw)
        .map_err(|_| MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_assessment(id,organization_id,connection_id,connection_revision,profile_version,initiated_by_user_id,state,source_account_id,identity_nonce,identity_ciphertext) VALUES($1,$2,$3,$4,$5,$6,'queued',$7,$8,$9)").bind(id).bind(ctx.organization_id.0).bind(cmd.connection_id).bind(cmd.expected_revision).bind(FUB_PROFILE_V1).bind(ctx.actor_user_id.0).bind(r.get::<i64,_>("source_account_id")).bind(identity.nonce.as_slice()).bind(identity.ciphertext).execute(&mut *tx).await?;
    for check in CHECKS {
        sqlx::query("INSERT INTO migration_assessment_check(assessment_id,organization_id,check_key,state,coverage) VALUES($1,$2,$3,$4,'not_checked')").bind(id).bind(ctx.organization_id.0).bind(check).bind(if is_probed_check(check){"pending"}else{"completed"}).execute(&mut *tx).await?;
    }
    let value = store::assessment(&mut tx, key, ctx.organization_id, id).await?;
    save(
        key,
        &mut tx,
        ctx,
        "start_assessment",
        cmd.request_id,
        &digest,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(Idempotent {
        value,
        replayed: false,
    })
}
pub async fn retry_fub_assessment(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: RetryFubAssessment,
) -> Result<AssessmentView, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let a = store::assessment(&mut tx, key, ctx.organization_id, cmd.assessment_id).await?;
    let c = store::connection(&mut tx, key, ctx.organization_id)
        .await?
        .ok_or(MigrationError::NotFound)?;
    if c.id != a.connection_id || c.revision != a.connection_revision || c.status != "connected" {
        return Err(MigrationError::Conflict);
    }
    if a.state == "paused" {
        super::snapshot::exclusive(&mut tx, ctx.organization_id, Uuid::nil()).await?;
        if sqlx::query("SELECT 1 FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry')").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.is_some(){return Err(MigrationError::Conflict)}
        sqlx::query("UPDATE migration_assessment SET state='queued',pause_reason=NULL,next_attempt_at=NULL,initiated_by_user_id=$3,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(a.id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_assessment_check SET state='pending',next_attempt_at=NULL,cycle_attempts=0 WHERE assessment_id=$1 AND organization_id=$2 AND state<>'completed'").bind(a.id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    }
    let value = store::assessment(&mut tx, key, ctx.organization_id, a.id).await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn cancel_fub_assessment(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: CancelFubAssessment,
) -> Result<AssessmentView, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let a = store::assessment(&mut tx, key, ctx.organization_id, cmd.assessment_id).await?;
    // Lock assessment before its checks, matching worker fence.
    sqlx::query(
        "SELECT id FROM migration_assessment WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(a.id)
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE migration_assessment SET state='cancelled',pause_reason=NULL,completed_at=now(),lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2 AND state IN ('queued','running','waiting_retry','paused')").bind(a.id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_assessment_check c SET state='cancelled' FROM migration_assessment a WHERE a.id=c.assessment_id AND a.organization_id=c.organization_id AND a.id=$1 AND a.organization_id=$2 AND a.state='cancelled' AND c.state<>'completed'").bind(a.id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = store::assessment(&mut tx, key, ctx.organization_id, a.id).await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn get_fub_assessment(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<AssessmentView, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    let value = store::assessment(&mut tx, key, ctx.organization_id, id).await?;
    tx.commit().await?;
    Ok(value)
}
pub async fn fub_summary(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
) -> Result<FubSummary, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    let connection = store::connection(&mut tx, key, ctx.organization_id).await?;
    let ids=sqlx::query("SELECT id,state FROM ((SELECT id,state,created_at FROM migration_assessment WHERE organization_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1) UNION (SELECT id,state,created_at FROM migration_assessment WHERE organization_id=$1 AND state IN ('queued','running','waiting_retry','paused') ORDER BY created_at DESC,id DESC LIMIT 1) UNION (SELECT id,state,created_at FROM migration_assessment WHERE organization_id=$1 AND state='completed' ORDER BY created_at DESC,id DESC LIMIT 1)) selected ORDER BY created_at DESC,id DESC").bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    let mut active_assessment = None;
    let mut latest_report = None;
    let mut latest_assessment = None;
    for r in ids {
        let state: String = r.get("state");
        let active = matches!(
            state.as_str(),
            "queued" | "running" | "waiting_retry" | "paused"
        );
        if latest_assessment.is_none()
            || (active && active_assessment.is_none())
            || (state == "completed" && latest_report.is_none())
        {
            let a = store::assessment(&mut tx, key, ctx.organization_id, r.get("id")).await?;
            if latest_assessment.is_none() {
                latest_assessment = Some(a.clone())
            }
            if active && active_assessment.is_none() {
                active_assessment = Some(a.clone())
            }
            if state == "completed" && latest_report.is_none() {
                latest_report = Some(a)
            }
        }
        if latest_assessment.is_some() && active_assessment.is_some() && latest_report.is_some() {
            break;
        }
    }
    tx.commit().await?;
    Ok(FubSummary {
        connection,
        active_assessment,
        latest_assessment,
        latest_report,
    })
}

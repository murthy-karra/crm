//! One atomic confirmed manifest unit. Source adapters authenticate before the
//! final transaction; native, head, catalog and membership checks hold its locks.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    model::{Counts, ENGINE},
    native_baseline::ResultData,
    preparation, sealing,
};
use crate::{
    auth::workspace::{self, ReleaseReadiness},
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
    ids::OrganizationId,
};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Finished,
    Capacity,
}
pub(super) struct ResultUnit {
    pub disposition: &'static str,
    pub reason: Option<super::model::Hold>,
    pub person: Option<Uuid>,
    pub target: Option<Uuid>,
    pub revision: Option<i64>,
    pub counts: Counts,
    pub data: ResultData,
    pub native_bytes: i64,
}
impl ResultUnit {
    pub fn held(unit: &PgRow, reason: super::model::Hold) -> Self {
        Self {
            disposition: "held",
            reason: Some(reason),
            person: unit.get("person_id"),
            target: unit.get("target_id"),
            revision: None,
            counts: Counts {
                units: 1,
                held: 1,
                ..Counts::default()
            },
            data: ResultData { after_state: None },
            native_bytes: 0,
        }
    }
}
pub(super) async fn begin(
    pool: &PgPool,
    claim: &Claim,
) -> Result<(Transaction<'static, Postgres>, PgRow, PgRow), MigrationError> {
    let (tx, b, p) = preparation::read_begin(pool, claim).await?;
    if p.get::<String, _>("state") != "running"
        || p.get::<String, _>("phase") != "apply"
        || p.get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_none()
    {
        return Err(MigrationError::Conflict);
    }
    Ok((tx, b, p))
}
pub(super) fn scope(claim: &Claim, p: &PgRow) -> Result<Scope, MigrationError> {
    Ok(Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: serde_json::from_value(serde_json::json!(p.get::<String, _>("family")))
            .map_err(|_| MigrationError::Crypto)?,
        revision: p.get("revision"),
    })
}
/// Initially dispatched for metadata; additional family executors join this
/// same queue once their exclusive owner/read adapters are installed.
pub async fn claim_next(pool: &PgPool) -> Result<Option<Claim>, MigrationError> {
    let runnable="p.family='metadata' AND p.state IN ('queued','running') AND p.phase='apply' AND p.confirmed_at IS NOT NULL AND NOT p.cancel_requested AND b.confirmed_at IS NOT NULL AND b.state IN ('queued','running','paused') AND (p.lease_token IS NULL OR p.lease_expires_at<=clock_timestamp())";
    let sql=format!("SELECT p.id,p.bundle_id,p.organization_id FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE {runnable} AND m.role='admin' AND m.status='active' ORDER BY p.lease_epoch,p.created_at,p.id LIMIT 1");
    let Some(candidate) = sqlx::query(&sql).fetch_optional(pool).await? else {
        return Ok(None);
    };
    let org = OrganizationId::new(candidate.get("organization_id"));
    let bundle: Uuid = candidate.get("bundle_id");
    let plan: Uuid = candidate.get("id");
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true)")
        .bind(ENGINE)
        .execute(&mut *tx)
        .await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, org).await?;
    let active=sqlx::query("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m").bind(bundle).bind(org.0).fetch_optional(&mut *tx).await?.is_some();
    if !active {
        return Ok(None);
    }
    sqlx::query("SELECT id FROM organization WHERE id=$1 FOR UPDATE")
        .bind(org.0)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(org.0).fetch_one(&mut *tx).await?;
    let payer:Uuid=sqlx::query_scalar("SELECT payer_plan_id FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(bundle).bind(org.0).fetch_one(&mut *tx).await?;
    let sql=format!("SELECT p.lease_epoch FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 AND {runnable} FOR UPDATE OF p");
    let Some(p) = sqlx::query(&sql)
        .bind(plan)
        .bind(org.0)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(None);
    };
    let epoch = p
        .get::<i64, _>("lease_epoch")
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='running',lease_epoch=$3,lease_token=$4,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(epoch).bind(token).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='running',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(bundle).bind(org.0).execute(&mut *tx).await?;
    sealing::settle_control(&mut tx, org.0, bundle, plan).await?;
    if payer != plan {
        sealing::settle_control(&mut tx, org.0, bundle, payer).await?;
    }
    tx.commit().await?;
    Ok(Some(Claim {
        organization: org,
        bundle,
        plan,
        token,
        epoch,
    }))
}

#[tracing::instrument(name="family_refresh_execution",skip_all,fields(organization_id=%claim.organization.0,bundle_id=%claim.bundle,plan_id=%claim.plan,lease_epoch=claim.epoch))]
pub async fn apply_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: &ReleaseReadiness,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, _, p) = begin(pool, claim).await?;
    release
        .require_family_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let next = p
        .get::<i64, _>("apply_position")
        .checked_add(1)
        .ok_or(MigrationError::Crypto)?;
    let unit=sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND position=$3").bind(claim.plan).bind(claim.organization.0).bind(next).fetch_optional(&mut *tx).await?;
    let Some(unit) = unit else {
        let totals: Counts = if p.get::<serde_json::Value, _>("results") == serde_json::json!({}) {
            Counts::default()
        } else {
            serde_json::from_value(p.get("results")).map_err(|_| MigrationError::Crypto)?
        };
        if !totals.reconciles()
            || totals.units
                != u64::try_from(p.get::<i64, _>("position")).map_err(|_| MigrationError::Crypto)?
        {
            return Err(MigrationError::Crypto);
        }
        if p.get::<i64, _>("apply_position") != p.get::<i64, _>("position") {
            return Err(MigrationError::Crypto);
        }
        sqlx::query("UPDATE migration_family_refresh_plan SET state='completed',phase='finished',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_family_refresh_bundle SET state=CASE WHEN EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state IN ('queued','running')) THEN 'running' WHEN EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state='paused') THEN 'paused' ELSE 'completed' END,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(claim.bundle).bind(claim.organization.0).execute(&mut *tx).await?;
        sealing::settle_control(&mut tx, claim.organization.0, claim.bundle, claim.plan).await?;
        let payer:Uuid=sqlx::query_scalar("SELECT payer_plan_id FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2").bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
        if payer != claim.plan {
            sealing::settle_control(&mut tx, claim.organization.0, claim.bundle, payer).await?;
        }
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    let scope = scope(claim, &p)?;
    let _: serde_json::Value = scope.open(
        key,
        unit.get("id"),
        Purpose::Manifest,
        unit.get("nonce"),
        unit.get("ciphertext"),
    )?;
    drop(tx);
    super::metadata_execution::apply(pool, key, policy, release, claim, scope, unit).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish(
    mut tx: Transaction<'static, Postgres>,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    p: &PgRow,
    unit: &PgRow,
    result_id: Uuid,
    reservation: Uuid,
    result: ResultUnit,
) -> Result<Progress, MigrationError> {
    let value = scope.seal(key, result_id, Purpose::Result, &result.data)?;
    let reason = result
        .reason
        .map(|r| serde_json::to_value(r).map_err(|_| MigrationError::Crypto))
        .transpose()?
        .and_then(|v| v.as_str().map(str::to_owned))
        .or_else(|| unit.get("reason"));
    sqlx::query("INSERT INTO migration_family_refresh_result(id,bundle_id,plan_id,organization_id,manifest_id,disposition,person_id,target_id,native_revision,reason,nonce,ciphertext,native_bytes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(result_id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(unit.get::<Uuid,_>("id")).bind(result.disposition).bind(result.person).bind(result.target).bind(result.revision).bind(reason).bind(value.nonce.as_slice()).bind(value.ciphertext).bind(result.native_bytes).execute(&mut *tx).await?;
    if result.data.after_state.is_some() {
        let changed=sqlx::query("INSERT INTO migration_family_refresh_head(organization_id,source_account_id,kind,source_key_hmac,person_id,target_id,result_id,version,storage_plan_id) SELECT $1,b.source_account_id,$3,$4,$5,$6,$7,1,$8 FROM migration_family_refresh_bundle b WHERE b.id=$2 AND b.organization_id=$1 ON CONFLICT(organization_id,source_account_id,kind,source_key_hmac) DO UPDATE SET result_id=EXCLUDED.result_id,version=migration_family_refresh_head.version+1 WHERE migration_family_refresh_head.result_id IS NOT DISTINCT FROM $9::uuid").bind(claim.organization.0).bind(claim.bundle).bind(unit.get::<String,_>("kind")).bind(unit.get::<Vec<u8>,_>("source_key_hmac")).bind(result.person).bind(result.target).bind(result_id).bind(claim.plan).bind(unit.get::<Option<Uuid>,_>("expected_head_id")).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(MigrationError::Conflict);
        }
    }
    let prior: Counts = if p.get::<serde_json::Value, _>("results") == serde_json::json!({}) {
        Counts::default()
    } else {
        serde_json::from_value(p.get("results")).map_err(|_| MigrationError::Crypto)?
    };
    let counts = prior
        .checked_add(&result.counts)
        .filter(Counts::reconciles)
        .ok_or(MigrationError::Crypto)?;
    sqlx::query("UPDATE migration_family_refresh_plan SET apply_position=$3,results=$4 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(unit.get::<i64,_>("position")).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Progress::Advanced)
}

async fn pause(pool: &PgPool, claim: &Claim, reason: &str) -> Result<(), MigrationError> {
    let (mut tx, b, _) = begin(pool, claim).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2")
        .bind(claim.plan).bind(claim.organization.0).bind(reason).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET state=CASE WHEN EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state IN ('queued','running')) THEN 'running' ELSE 'paused' END,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
        .bind(claim.bundle).bind(claim.organization.0).execute(&mut *tx).await?;
    sealing::settle_control(&mut tx, claim.organization.0, claim.bundle, claim.plan).await?;
    let payer: Uuid = b.get("payer_plan_id");
    if payer != claim.plan {
        sealing::settle_control(&mut tx, claim.organization.0, claim.bundle, payer).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// One scheduler turn; every successful unit releases its lease so Cancel and
/// other families get a bounded opportunity to run. Durable failures need Resume.
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<super::preparation_worker::Progress, MigrationError> {
    use super::preparation_worker::{self, Progress as Sweep};
    if super::revocation::run_once(pool).await? {
        return Ok(Sweep::Paused);
    }
    let Some(claim) = claim_next(pool).await? else {
        return Ok(Sweep::Idle);
    };
    let result = match release {
        Some(release) => apply_once(pool, key, policy, release, &claim).await,
        None => Err(MigrationError::ReleaseNotReady),
    };
    let reason = match &result {
        Ok(Progress::Capacity) | Err(MigrationError::StorageLimit) => Some("storage_limit"),
        Err(MigrationError::Crypto) => Some("retained_integrity_failed"),
        Err(MigrationError::SourceNotEligible) => Some("source_binding_changed"),
        Err(MigrationError::ReleaseNotReady) => Some("release_not_ready"),
        _ => None,
    };
    if let Some(reason) = reason {
        pause(pool, &claim, reason).await?;
        return Ok(Sweep::Paused);
    }
    preparation_worker::release(pool, &claim).await?;
    result.map(|_| Sweep::Advanced)
}

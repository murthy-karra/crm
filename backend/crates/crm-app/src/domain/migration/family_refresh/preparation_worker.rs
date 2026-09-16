//! One bounded preparation step for the application's existing scheduler. No
//! polling loop is created here; mappings/classification/execution have their
//! own later dispatch branches.
use super::{
    cohort::{self, Claim},
    core_source, history_index,
    model::ENGINE,
    preparation,
};
use crate::{
    auth::workspace,
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
    ids::OrganizationId,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Idle,
    Advanced,
    Paused,
}

// Core index ownership stays with the fixed payer. Other core plans wait for
// that one index; history can advance once the shared cohort is frozen.
const RUNNABLE: &str = "b.state='preparing' AND b.confirmed_at IS NULL AND p.state='preparing' AND p.confirmed_at IS NULL AND EXISTS(SELECT 1 FROM migration_workspace w WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id) AND (p.lease_token IS NULL OR p.lease_expires_at<=clock_timestamp()) AND ((p.family IN ('metadata','activity') AND p.phase='mappings' AND NOT p.mappings_complete) OR (p.family='activity' AND p.phase IN ('mappings','classify') AND p.mappings_complete AND NOT p.source_walk_complete) OR (p.family='history' AND p.phase IN ('mappings','classify') AND (NOT p.source_walk_complete OR NOT p.owned_walk_complete)) OR (p.phase='capture' AND (p.id=b.payer_plan_id OR p.family='history')) OR (p.phase='cohort' AND (p.id=b.payer_plan_id OR (owner.phase<>'cohort' AND (p.family='history' OR owner.phase NOT IN ('cohort','capture'))))))";

/// Server-owned 60-second lease; an active owner is never displaced. Epochs
/// balance bounded steps among eligible families without trusting client input.
pub async fn claim_next(pool: &PgPool) -> Result<Option<Claim>, MigrationError> {
    let sql=format!("SELECT p.id,p.bundle_id,p.organization_id FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id JOIN migration_family_refresh_plan owner ON owner.id=b.payer_plan_id AND owner.organization_id=b.organization_id JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE {RUNNABLE} AND m.role='admin' AND m.status='active' ORDER BY p.lease_epoch,p.created_at,p.id LIMIT 1");
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
    let active=sqlx::query("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m")
        .bind(bundle).bind(org.0).fetch_optional(&mut *tx).await?.is_some();
    if !active {
        return Ok(None);
    }
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(org.0).fetch_one(&mut *tx).await?;
    sqlx::query("SELECT id FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(bundle).bind(org.0).fetch_one(&mut *tx).await?;
    let sql=format!("SELECT p.lease_epoch FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id JOIN migration_family_refresh_plan owner ON owner.id=b.payer_plan_id AND owner.organization_id=b.organization_id WHERE p.id=$1 AND p.organization_id=$2 AND {RUNNABLE} FOR UPDATE OF p");
    let Some(row) = sqlx::query(&sql)
        .bind(plan)
        .bind(org.0)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(None);
    };
    let epoch = row
        .get::<i64, _>("lease_epoch")
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=$4,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2")
        .bind(plan).bind(org.0).bind(token).bind(epoch).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(Claim {
        organization: org,
        bundle,
        plan,
        token,
        epoch,
    }))
}

/// Release only this exact claim. A late completion cannot clear a takeover's
/// lease. This changes fixed-width lease fields and creates no evidence charge.
pub async fn release(pool: &PgPool, claim: &Claim) -> Result<bool, MigrationError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true)")
        .bind(ENGINE)
        .execute(&mut *tx)
        .await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, claim.organization).await?;
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND bundle_id=$2 AND organization_id=$3 AND lease_token=$4 AND lease_epoch=$5")
        .bind(claim.plan).bind(claim.bundle).bind(claim.organization.0).bind(claim.token).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    Ok(changed == 1)
}

async fn pause(pool: &PgPool, claim: &Claim, reason: &str) -> Result<(), MigrationError> {
    let (mut tx, _, _) = preparation::begin(pool, claim).await?;
    let control:Uuid=sqlx::query_scalar("SELECT token FROM migration_family_refresh_reservation WHERE plan_id=$1 AND organization_id=$2 AND purpose='control'").bind(claim.plan).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let changed = sqlx::query("UPDATE migration_family_refresh_plan SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND lease_token=$4 AND lease_epoch=$5 AND lease_expires_at>clock_timestamp()")
        .bind(claim.plan).bind(claim.organization.0).bind(reason).bind(claim.token).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(control)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
) -> Result<Progress, MigrationError> {
    let Some(claim) = claim_next(pool).await? else {
        return Ok(Progress::Idle);
    };
    let result = step(pool, key, policy, &claim).await;
    let result = match result {
        Ok(false) => {
            pause(pool, &claim, "storage_limit").await?;
            Ok(Progress::Paused)
        }
        Ok(true) => Ok(Progress::Advanced),
        Err(MigrationError::Crypto) => {
            pause(pool, &claim, "retained_integrity_failed").await?;
            Ok(Progress::Paused)
        }
        Err(MigrationError::SourceNotEligible) => {
            pause(pool, &claim, "source_binding_changed").await?;
            Ok(Progress::Paused)
        }
        Err(MigrationError::ReleaseNotReady) => {
            pause(pool, &claim, "release_not_ready").await?;
            Ok(Progress::Paused)
        }
        Err(e) => Err(e),
    };
    release(pool, &claim).await?;
    result
}

#[tracing::instrument(name="family_refresh_preparation",skip_all,fields(organization_id=%claim.organization.0,bundle_id=%claim.bundle,plan_id=%claim.plan,lease_epoch=claim.epoch))]
async fn step(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<bool, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    let phase = p.get::<String, _>("phase");
    let family = p.get::<String, _>("family");
    let payer = b.get::<Uuid, _>("payer_plan_id") == claim.plan;
    // Authenticate both frozen envelopes before any phase work. The index
    // runners independently validate retained captures/profiles and raw bytes.
    let owner=sqlx::query("SELECT id,family,revision,phase FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2").bind(b.get::<Uuid,_>("payer_plan_id")).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let owner_family = serde_json::from_value(serde_json::json!(owner.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    let scope = super::evidence::Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: owner.get("id"),
        family: owner_family,
        revision: owner.get("revision"),
    };
    let frozen: serde_json::Value = scope.open(
        key,
        claim.bundle,
        super::evidence::Purpose::Binding,
        b.get("source_nonce"),
        b.get("source_ciphertext"),
    )?;
    let scope = super::evidence::Scope {
        plan: claim.plan,
        family: serde_json::from_value(serde_json::json!(family))
            .map_err(|_| MigrationError::Crypto)?,
        revision: p.get("revision"),
        ..scope
    };
    let own: serde_json::Value = scope.open(
        key,
        claim.plan,
        super::evidence::Purpose::Binding,
        p.get("nonce"),
        p.get("ciphertext"),
    )?;
    super::plan_commands::validate_runtime(&frozen)?;
    if frozen != own
        || frozen["version"] != ENGINE
        || frozen["parent_import_id"] != serde_json::json!(b.get::<Uuid, _>("parent_import_id"))
        || frozen["parent_plan_id"] != serde_json::json!(b.get::<Uuid, _>("parent_plan_id"))
        || frozen["account"]
            .as_str()
            .and_then(|v| v.parse::<i64>().ok())
            != Some(b.get::<i64, _>("source_account_id"))
    {
        return Err(MigrationError::Crypto);
    }
    if phase == "cohort" && !payer {
        if owner.get::<String, _>("phase") == "cohort"
            || (family != "history" && owner.get::<String, _>("phase") == "capture")
        {
            return Err(MigrationError::Conflict);
        }
        let Some(reservation) = preparation::reserve(&mut tx, claim, policy, 4096).await? else {
            return Ok(false);
        };
        sqlx::query(
            "UPDATE migration_family_refresh_plan SET phase=$3 WHERE id=$1 AND organization_id=$2",
        )
        .bind(claim.plan)
        .bind(claim.organization.0)
        .bind(if family == "history" {
            "capture"
        } else {
            "mappings"
        })
        .execute(&mut *tx)
        .await?;
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(reservation)
            .bind(claim.epoch)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(true);
    }
    let sources_complete = p.get::<bool, _>("source_walk_complete");
    drop(tx);
    if phase == "cohort" {
        return Ok(!matches!(
            cohort::freeze_page(pool, claim, policy, 50).await?,
            cohort::Progress::Capacity
        ));
    }
    if family == "history" && matches!(phase.as_str(), "mappings" | "classify") {
        let progress = if sources_complete {
            super::history_missing::run_once(pool, key, policy, claim).await?
        } else {
            super::history_walk::run_once(pool, key, policy, claim).await?
        };
        return Ok(progress != super::history_walk::Progress::Capacity);
    }
    if family == "activity"
        && p.get::<bool, _>("mappings_complete")
        && matches!(phase.as_str(), "mappings" | "classify")
    {
        return Ok(
            super::activity_walk::run_once(pool, key, policy, claim).await?
                != super::activity_walk::Progress::Capacity,
        );
    }
    if phase == "mappings" && family != "history" {
        return Ok(
            super::mapping_inventory::run_once(pool, key, policy, claim).await?
                != super::mapping_inventory::Progress::Capacity,
        );
    }
    if phase != "capture" {
        return Err(MigrationError::Conflict);
    }
    let result = if family == "history" {
        history_index::index_page(pool, key, claim, policy).await?
    } else {
        core_source::index_page(pool, key, claim, policy).await?
    };
    Ok(result != core_source::Progress::Capacity)
}

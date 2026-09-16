//! Walk frozen first owners after source reconciliation. Absence records a held
//! proposal without deleting native history or advancing a last-applied head.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    history_baseline::{self, Baseline, Discovery},
    history_walk::Progress,
    model::{Counts, Family, Hold, Kind},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct MissingProposal {
    pub kind: Kind,
    pub identity: Uuid,
    pub identity_hmac: Vec<u8>,
    pub source_not_observed: bool,
    pub reason: Hold,
    pub baseline: Option<Baseline>,
}

async fn checkpoint(
    conn: &mut PgConnection,
    claim: &Claim,
    before: Option<Uuid>,
    next: Option<Uuid>,
) -> Result<(), MigrationError> {
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET owned_after=COALESCE($4,owned_after),owned_walk_complete=($4::uuid IS NULL) WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$5 AND lease_expires_at>clock_timestamp() AND owned_after IS NOT DISTINCT FROM $6 AND NOT owned_walk_complete AND source_walk_complete AND state='preparing' AND phase='classify'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(next).bind(claim.epoch).bind(before).execute(conn).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history"
        || p.get::<String, _>("phase") != "classify"
        || !p.get::<bool, _>("source_walk_complete")
    {
        return Err(MigrationError::Conflict);
    }
    if p.get::<bool, _>("owned_walk_complete") {
        return Ok(Progress::Finished);
    }
    let before: Option<Uuid> = p.get("owned_after");
    let next = sqlx::query("SELECT * FROM crm_family_refresh_next_owned_history($1,$2,$3)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(before)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(next) = next else {
        // Completion only changes fixed-width state; no evidence or counts grow.
        checkpoint(&mut tx, claim, before, None).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    let identity: Uuid = next.get("identity_id");
    let hash: Vec<u8> = next.get("identity_hmac");
    let kind_name: String = next.get("kind");
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4)")
        .bind(claim.plan).bind(claim.organization.0).bind(&kind_name).bind(&hash).fetch_one(&mut *tx).await?;
    if exists {
        checkpoint(&mut tx, claim, before, Some(identity)).await?;
        tx.commit().await?;
        return Ok(Progress::Advanced);
    }
    let cohort: Uuid = next.get("cohort_id");
    let person: Uuid = next.get("person_id");
    let kind: Kind =
        serde_json::from_value(serde_json::json!(kind_name)).map_err(|_| MigrationError::Crypto)?;
    let identity_hash: [u8; 32] = hash
        .as_slice()
        .try_into()
        .map_err(|_| MigrationError::Crypto)?;
    drop(tx);
    let (baseline, reason) =
        match history_baseline::discover(pool, key, claim, cohort, kind, &identity_hash).await? {
            Discovery::Proven(b) => (Some(b), Hold::SourceNotObserved),
            Discovery::Held(h) => (None, h),
        };
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<Option<Uuid>, _>("owned_after") != before || p.get::<bool, _>("owned_walk_complete")
    {
        return Err(MigrationError::Conflict);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: p.get("revision"),
    };
    let id = Uuid::new_v4();
    let proposal = MissingProposal {
        kind,
        identity,
        identity_hmac: hash.clone(),
        source_not_observed: true,
        reason,
        baseline,
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &proposal)?;
    let bound =
        i64::try_from(sealed.ciphertext.len()).map_err(|_| MigrationError::StorageLimit)? + 16384;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Progress::Capacity);
    };
    let counts = Counts {
        units: 1,
        held: 1,
        ..Counts::default()
    };
    let old = p.get::<serde_json::Value, _>("counts");
    let totals: Counts = if old == serde_json::json!({}) {
        Counts::default()
    } else {
        serde_json::from_value(old).map_err(|_| MigrationError::Crypto)?
    };
    let totals = totals
        .checked_add(&counts)
        .filter(Counts::reconciles)
        .ok_or(MigrationError::Crypto)?;
    let position = p
        .get::<i64, _>("position")
        .checked_add(1)
        .ok_or(MigrationError::Crypto)?;
    let reason = serde_json::to_value(reason).map_err(|_| MigrationError::Crypto)?;
    let reason = reason.as_str().ok_or(MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,position,kind,source_key_hmac,person_id,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'held',$10,$11,$12,$13,$14)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(cohort).bind(position).bind(kind_name).bind(hash).bind(person).bind(reason)
        .bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET position=$3,counts=$4 WHERE id=$1 AND organization_id=$2")
        .bind(claim.plan).bind(claim.organization.0).bind(position).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    checkpoint(&mut tx, claim, before, Some(identity)).await?;
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,reason,"Family refresh missing history held");
    Ok(Progress::Advanced)
}

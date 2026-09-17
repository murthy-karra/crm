//! Persist one atomic Person metadata proposal or conservative hold.
use super::{
    cohort::Claim,
    core_resolution::{self, Resolution},
    evidence::{Purpose, Scope},
    metadata_baseline,
    metadata_delta::{self, Ownership, Snapshot},
    metadata_discovery::{self, Discovery},
    metadata_mapping::{self, Conversion},
    model::{Counts, Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{metadata_store, snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Decision {
    Ready {
        baseline: Snapshot,
        ownership: Ownership,
        baseline_result: Uuid,
        source: Box<metadata_mapping::Evidence>,
        native: Box<metadata_delta::Proposal>,
    },
    Held {
        reason: Hold,
        source: Option<Box<metadata_mapping::Evidence>>,
    },
}
#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub source_id: String,
    pub source_row: Option<Uuid>,
    pub decision: Decision,
}
pub enum Prepared {
    Unit(Uuid),
    Capacity,
}

pub async fn prepare_unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    cohort: Uuid,
) -> Result<Prepared, MigrationError> {
    prepare(pool, key, policy, claim, cohort, None).await
}
pub(super) async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    cohort: Uuid,
    checkpoint: Option<super::metadata_walk::Checkpoint>,
) -> Result<Prepared, MigrationError> {
    let (source_id, person, account) = {
        let (mut tx, b, p) = preparation::begin(pool, claim).await?;
        if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("catalog_walk_complete")
        {
            return Err(MigrationError::ImportBusy);
        }
        let c=sqlx::query("SELECT source_person_id,person_id FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
        if let Some(id)=sqlx::query_scalar("SELECT id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind='metadata' AND cohort_id=$3").bind(claim.plan).bind(claim.organization.0).bind(cohort).fetch_optional(&mut *tx).await? {super::metadata_walk::advance(&mut tx,claim,checkpoint).await?;tx.commit().await?;return Ok(Prepared::Unit(id));}
        (
            c.get::<String, _>("source_person_id"),
            c.get::<Uuid, _>("person_id"),
            b.get::<i64, _>("source_account_id"),
        )
    };
    let selected =
        core_resolution::resolve(pool, key, claim, core_resolution::Kind::Person, &source_id)
            .await?;
    let (source_row, mut held) = match selected {
        Resolution::Ready(r) => (Some(r.row), None),
        Resolution::Held(h) => (None, Some(h)),
    };
    let mut baseline = None;
    let mut converted = None;
    if held.is_none() {
        match metadata_discovery::discover(pool, key, claim, cohort).await? {
            Discovery::Held(h) => held = Some(h),
            Discovery::Proven(b) => {
                match metadata_mapping::convert(pool, key, claim, &source_id, &b.ownership.fields)
                    .await?
                {
                    Conversion::Held(h) => held = Some(h),
                    Conversion::Ready(c) => converted = Some(c),
                }
                baseline = Some(b);
            }
        }
    }
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if let Some(id)=sqlx::query_scalar("SELECT id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind='metadata' AND cohort_id=$3").bind(claim.plan).bind(claim.organization.0).bind(cohort).fetch_optional(&mut *tx).await? {super::metadata_walk::advance(&mut tx,claim,checkpoint).await?;tx.commit().await?;return Ok(Prepared::Unit(id));}
    let mut native = None;
    if let (Some(b), Some(c)) = (&baseline, &converted) {
        if c.evidence.source_row != source_row.ok_or(MigrationError::Crypto)? {
            return Err(MigrationError::Crypto);
        }
        let mut current = metadata_baseline::observe(&mut tx, claim.organization, person).await?;
        let heads:Vec<Uuid>=sqlx::query_scalar("SELECT result_id FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind='metadata' AND target_id=$3 LIMIT 2").bind(claim.organization.0).bind(account).bind(person).fetch_all(&mut *tx).await?;
        current.head = heads.first().copied();
        if heads.len() > 1 {
            held = Some(Hold::IdentityMismatch);
        } else {
            match metadata_delta::propose(
                Some(&b.baseline),
                Some(&current),
                &b.ownership,
                &c.evidence.source,
                &c.catalog,
                true,
            ) {
                Ok(n) => native = Some(n),
                Err(h) => held = Some(h),
            }
        }
    }
    let (decision, counts, disposition, target, head, revision, result) = if let Some(reason) = held
    {
        (
            Decision::Held {
                reason,
                source: converted.map(|c| Box::new(c.evidence)),
            },
            Counts {
                units: 1,
                held: 1,
                ..Default::default()
            },
            "held",
            None,
            None,
            None,
            None,
        )
    } else {
        let b = baseline.ok_or(MigrationError::Crypto)?;
        let n = native.ok_or(MigrationError::Crypto)?;
        let counts = n.counts.clone();
        let disposition = if counts.updates == 1 {
            "update"
        } else if counts.already_current == 1 {
            "already_current"
        } else {
            return Err(MigrationError::Crypto);
        };
        let head = n.expected_head;
        let revision = n.expected_revision;
        (
            Decision::Ready {
                baseline: b.baseline,
                ownership: b.ownership,
                baseline_result: b.result,
                source: Box::new(converted.ok_or(MigrationError::Crypto)?.evidence),
                native: Box::new(n),
            },
            counts,
            disposition,
            Some(person),
            head,
            Some(revision),
            Some(b.result),
        )
    };
    let id = Uuid::new_v4();
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: p.get("revision"),
    };
    let sealed = scope.seal(
        key,
        id,
        Purpose::Manifest,
        &Evidence {
            source_id: source_id.clone(),
            source_row,
            decision,
        },
    )?;
    let bound = i64::try_from(sealed.ciphertext.len())
        .map_err(|_| MigrationError::StorageLimit)?
        .checked_add(16384)
        .ok_or(MigrationError::StorageLimit)?;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Prepared::Capacity);
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
    let reason = held
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| MigrationError::Crypto)?;
    let hash = metadata_store::source_key(
        key,
        claim.organization,
        account,
        "person",
        source_id.as_bytes(),
    );
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,baseline_result_id,expected_head_id,expected_revision,disposition,reason,counts,nonce,ciphertext,added_byte_bound,source_id) VALUES($1,$2,$3,$4,$5,$6,$7,'metadata',$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(cohort).bind(source_row).bind(position).bind(hash).bind(person).bind(target).bind(result).bind(head).bind(revision).bind(disposition).bind(reason.as_ref().and_then(|v|v.as_str())).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).bind(source_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET position=$3,counts=$4,phase='classify' WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(position).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    super::metadata_walk::advance(&mut tx, claim, checkpoint).await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,disposition,"Family refresh Person metadata proposal prepared");
    Ok(Prepared::Unit(id))
}

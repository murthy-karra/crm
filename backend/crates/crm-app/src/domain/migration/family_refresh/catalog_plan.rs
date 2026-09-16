//! Atomic retained catalog outcomes. These are prerequisites for Person units,
//! not native write authority or sufficient evidence to seal a whole family.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    metadata_destination::{self, Inspection},
    model::{Counts, Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Decision {
    Ready {
        evidence: Box<metadata_destination::Evidence>,
    },
    Held {
        reason: Hold,
        mapping: Box<Mapping>,
    },
}
#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub mapping_id: Uuid,
    pub source_row: Uuid,
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
    mapping_id: Uuid,
) -> Result<Prepared, MigrationError> {
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("mappings_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: p.get("revision"),
    };
    let row=sqlx::query("SELECT m.*,parent.source_key_hmac AS parent_key FROM migration_family_refresh_mapping m LEFT JOIN migration_family_refresh_mapping parent ON parent.id=m.parent_id AND parent.plan_id=m.plan_id AND parent.organization_id=m.organization_id WHERE m.id=$1 AND m.plan_id=$2 AND m.bundle_id=$3 AND m.organization_id=$4")
        .bind(mapping_id).bind(claim.plan).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let mapping: Mapping = scope.open(
        key,
        mapping_id,
        Purpose::Mapping,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    mapping.verify(&row)?;
    if !matches!(mapping.kind.as_str(), "tag" | "field" | "option") {
        return Err(MigrationError::InvalidInput);
    }
    let source = mapping.source.as_ref().ok_or(MigrationError::Crypto)?.row;
    if let Some(existing)=sqlx::query("SELECT id,mapping_id,source_row_id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind='catalog' AND source_key_hmac=$3")
        .bind(claim.plan).bind(claim.organization.0).bind(&mapping.source_key).fetch_optional(&mut *tx).await? {
        if existing.get::<Option<Uuid>,_>("mapping_id")!=Some(mapping_id) || existing.get::<Option<Uuid>,_>("source_row_id")!=Some(source){return Err(MigrationError::Crypto);}
        return Ok(Prepared::Unit(existing.get("id")));
    }
    let dependency = if let Some(parent) = row.get::<Option<Uuid>, _>("parent_id") {
        let parent:Option<String>=sqlx::query_scalar("SELECT disposition FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND mapping_id=$3 AND kind='catalog'")
            .bind(claim.plan).bind(claim.organization.0).bind(parent).fetch_optional(&mut *tx).await?;
        Some(parent.ok_or(MigrationError::ImportBusy)?)
    } else {
        None
    };
    drop(tx);
    let inspected = if dependency
        .as_deref()
        .is_some_and(|d| !matches!(d, "insert" | "already_current"))
    {
        Inspection::Held(Hold::MappingRequired)
    } else {
        metadata_destination::inspect(pool, key, claim, mapping_id).await?
    };
    let (decision, counts, disposition, target, reason) = match inspected {
        Inspection::Ready(evidence) => {
            if evidence.mapping_id != mapping_id
                || evidence.mapping.source_key != mapping.source_key
            {
                return Err(MigrationError::Crypto);
            }
            let insert = matches!(evidence.mapping.choice, Choice::CreateMatching { .. });
            let target = evidence.target;
            (
                Decision::Ready { evidence },
                Counts {
                    units: 1,
                    inserts: u64::from(insert),
                    already_current: u64::from(!insert),
                    ..Counts::default()
                },
                if insert { "insert" } else { "already_current" },
                Some(target),
                None,
            )
        }
        Inspection::Held(reason) => (
            Decision::Held {
                reason,
                mapping: Box::new(mapping.clone()),
            },
            Counts {
                units: 1,
                held: 1,
                ..Counts::default()
            },
            "held",
            None,
            Some(reason),
        ),
    };
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    let id = Uuid::new_v4();
    let sealed = scope.seal(
        key,
        id,
        Purpose::Manifest,
        &Evidence {
            mapping_id,
            source_row: source,
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
    let reason = reason
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,mapping_id,source_row_id,position,kind,source_key_hmac,target_id,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,'catalog',$8,$9,$10,$11,$12,$13,$14,$15)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(mapping_id).bind(source).bind(position).bind(&mapping.source_key).bind(target).bind(disposition).bind(reason.as_ref().and_then(|v|v.as_str())).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET position=$4,counts=$5,phase='classify' WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND state='preparing'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(position).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,disposition,"Family refresh catalog outcome prepared");
    Ok(Prepared::Unit(id))
}

//! A qualified retained identity can have an unproven/ineligible baseline. Freeze
//! that visible hold without allocating a native identity, fact, version or head.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    history_plan::Prepared,
    history_resolution::Selected,
    history_source::HistoryEvidence,
    model::{Counts, Family, Hold, Kind},
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
pub struct HeldProposal {
    pub kind: Kind,
    pub source_id: String,
    pub source: HistoryEvidence,
    pub reason: Hold,
}
pub(super) struct Input {
    pub cohort: Uuid,
    pub kind: Kind,
    pub source_id: String,
    pub selected: Selected,
    pub reason: Hold,
}
pub(super) async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    input: Input,
) -> Result<Prepared, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
        || b.get::<Option<Uuid>, _>("history_capture_id") != Some(input.selected.capture)
    {
        return Err(MigrationError::Conflict);
    }
    let c=sqlx::query("SELECT person_id,source_person_id FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3")
        .bind(input.cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if c.get::<String, _>("source_person_id") != input.selected.evidence.source_person {
        return Ok(Prepared::Held(Hold::IdentityMismatch));
    }
    let kind = match input.kind {
        Kind::Event => "event",
        Kind::Call => "call",
        Kind::Text => "text",
        _ => return Err(MigrationError::InvalidInput),
    };
    if let Some(existing)=sqlx::query("SELECT id,cohort_id,source_row_id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4")
        .bind(claim.plan).bind(claim.organization.0).bind(kind).bind(input.selected.evidence.identity_hmac.as_slice()).fetch_optional(&mut *tx).await? {
        if existing.get::<Option<Uuid>,_>("cohort_id")!=Some(input.cohort) || existing.get::<Option<Uuid>,_>("source_row_id")!=Some(input.selected.row) { return Err(MigrationError::Conflict); }
        return Ok(Prepared::Unit(existing.get("id")));
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: p.get("revision"),
    };
    let id = Uuid::new_v4();
    let proposal = HeldProposal {
        kind: input.kind,
        source_id: input.source_id,
        source: input.selected.evidence,
        reason: input.reason,
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &proposal)?;
    let bound =
        i64::try_from(sealed.ciphertext.len()).map_err(|_| MigrationError::StorageLimit)? + 16384;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Prepared::Capacity);
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
    let reason = serde_json::to_value(input.reason).map_err(|_| MigrationError::Crypto)?;
    let reason = reason.as_str().ok_or(MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'held',$11,$12,$13,$14,$15)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(input.cohort).bind(input.selected.row)
        .bind(position).bind(kind).bind(proposal.source.identity_hmac.as_slice()).bind(c.get::<Uuid,_>("person_id")).bind(reason)
        .bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
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
    tracing::info!(organization_id=%claim.organization,bundle_id=%claim.bundle,plan_id=%claim.plan,manifest_id=%id,reason,"Family refresh history hold prepared");
    Ok(Prepared::Unit(id))
}

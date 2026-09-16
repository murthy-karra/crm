//! Freeze one authenticated history proposal. Native facts, versions and heads
//! remain untouched until a separately confirmed execution unit revalidates it.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    history_baseline::{self, Baseline},
    history_hold,
    history_resolution::{self, Resolution},
    history_source::HistoryEvidence,
    model::{Counts, Family, HistoryChange, Hold, Kind},
    new_identity, preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Proposal {
    pub kind: Kind,
    pub source_id: String,
    pub source: HistoryEvidence,
    pub baseline: Option<Baseline>,
    pub change: HistoryChange,
    pub identity: Uuid,
    pub target: Uuid,
    pub version_id: Option<Uuid>,
}
pub enum Prepared {
    Unit(Uuid),
    Held(Hold),
    Capacity,
}

/// This entry point accepts only a resolved in-cohort source identity. Source
/// diagnostic/excluded occurrences are settled by the source-walk dispatcher.
/// Qualified sources with baseline holds become immutable held manifests. A
/// returned hold is not stored when source/cohort proof itself is absent.
pub async fn prepare_unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    cohort: Uuid,
    kind: Kind,
    source_id: &str,
) -> Result<Prepared, MigrationError> {
    let kind_name = match kind {
        Kind::Event => "event",
        Kind::Call => "call",
        Kind::Text => "text",
        _ => return Err(MigrationError::InvalidInput),
    };
    let selected = match history_resolution::resolve(pool, key, claim, kind, source_id).await? {
        Resolution::Ready(r) => r,
        Resolution::Held(h) => return Ok(Prepared::Held(h)),
    };
    // Replay the immutable proposal before observing mutable baseline state.
    // A later local edit must not allocate a replacement target on a retry.
    {
        let (mut tx, _, p) = preparation::begin(pool, claim).await?;
        if p.get::<String, _>("family") != "history"
            || !matches!(
                p.get::<String, _>("phase").as_str(),
                "mappings" | "classify"
            )
        {
            return Err(MigrationError::Conflict);
        }
        let existing = sqlx::query("SELECT m.id,m.cohort_id,m.source_row_id,c.source_person_id FROM migration_family_refresh_manifest m JOIN migration_family_refresh_cohort c ON c.id=m.cohort_id AND c.bundle_id=m.bundle_id AND c.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind=$3 AND m.source_key_hmac=$4")
            .bind(claim.plan).bind(claim.organization.0).bind(kind_name).bind(selected.evidence.identity_hmac.as_slice()).fetch_optional(&mut *tx).await?;
        if let Some(m) = existing {
            if m.get::<Option<Uuid>, _>("cohort_id") != Some(cohort)
                || m.get::<Option<Uuid>, _>("source_row_id") != Some(selected.row)
                || m.get::<String, _>("source_person_id") != selected.evidence.source_person
            {
                return Err(MigrationError::Conflict);
            }
            return Ok(Prepared::Unit(m.get("id")));
        }
    }
    let baseline = match history_baseline::discover(
        pool,
        key,
        claim,
        cohort,
        kind,
        &selected.evidence.identity_hmac,
    )
    .await?
    {
        history_baseline::Discovery::Proven(b) => Ok(Some(b)),
        history_baseline::Discovery::Held(Hold::BaselineUnproven) => {
            match new_identity::discover(pool, key, claim, cohort, kind, source_id).await? {
                new_identity::Discovery::New(_) => Ok(None),
                new_identity::Discovery::Held(h) => Err(h),
            }
        }
        history_baseline::Discovery::Held(h) => Err(h),
    };
    let baseline = match baseline {
        Ok(baseline) => baseline,
        Err(reason) => {
            return history_hold::prepare(
                pool,
                key,
                policy,
                claim,
                history_hold::Input {
                    cohort,
                    kind,
                    source_id: source_id.to_owned(),
                    selected: *selected,
                    reason,
                },
            )
            .await
        }
    };
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let c = sqlx::query("SELECT person_id,source_person_id FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3")
        .bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let person: Uuid = c.get("person_id");
    if c.get::<String, _>("source_person_id") != selected.evidence.source_person {
        return Ok(Prepared::Held(Hold::IdentityMismatch));
    }
    let existing = sqlx::query("SELECT id,cohort_id,source_row_id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4")
        .bind(claim.plan).bind(claim.organization.0).bind(kind_name).bind(selected.evidence.identity_hmac.as_slice()).fetch_optional(&mut *tx).await?;
    if let Some(existing) = existing {
        if existing.get::<Option<Uuid>, _>("cohort_id") != Some(cohort)
            || existing.get::<Option<Uuid>, _>("source_row_id") != Some(selected.row)
        {
            return Err(MigrationError::Conflict);
        }
        return Ok(Prepared::Unit(existing.get("id")));
    }
    let change = super::model::compare_history(
        baseline.as_ref().map(|b| (b.semantic.as_slice(), b.person)),
        &selected.evidence.semantic_hmac,
        person,
        true,
        false,
        false,
    );
    let (disposition, counts) = match change {
        HistoryChange::New => (
            "insert",
            Counts {
                units: 1,
                inserts: 1,
                ..Counts::default()
            },
        ),
        HistoryChange::AlreadyCurrent => (
            "already_current",
            Counts {
                units: 1,
                already_current: 1,
                ..Counts::default()
            },
        ),
        HistoryChange::Correction => (
            "correction",
            Counts {
                units: 1,
                history_corrections: 1,
                ..Counts::default()
            },
        ),
        HistoryChange::Held(h) => return Ok(Prepared::Held(h)),
    };
    let identity = baseline.as_ref().map_or_else(Uuid::new_v4, |b| b.identity);
    let target = baseline
        .as_ref()
        .map_or_else(Uuid::new_v4, |b| b.original_fact);
    let expected_head = baseline.as_ref().and_then(|b| b.result);
    let version = baseline.as_ref().map(|b| b.version);
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: p.get("revision"),
    };
    let id = Uuid::new_v4();
    let proposal = Proposal {
        kind,
        source_id: source_id.to_owned(),
        source: selected.evidence,
        baseline,
        change,
        identity,
        target,
        version_id: (change == HistoryChange::Correction).then(Uuid::new_v4),
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &proposal)?;
    // Metadata-only evidence is bounded; retain room for counters and references.
    let bound =
        i64::try_from(sealed.ciphertext.len()).map_err(|_| MigrationError::StorageLimit)? + 16384;
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
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,baseline_result_id,expected_head_id,expected_revision,disposition,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$12,$13,$14,$15,$16,$17,$18)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(cohort).bind(selected.row).bind(position).bind(kind_name)
        .bind(proposal.source.identity_hmac.as_slice()).bind(person).bind(target).bind(expected_head).bind(version).bind(disposition)
        .bind(serde_json::to_value(counts).map_err(|_| MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound)
        .execute(&mut *tx).await?;
    let changed = sqlx::query("UPDATE migration_family_refresh_plan SET position=$4,counts=$5,phase='classify' WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND state='preparing'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(position).bind(serde_json::to_value(totals).map_err(|_| MigrationError::Crypto)?).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if changed != 1 || b.get::<Option<Uuid>, _>("history_capture_id") != Some(selected.capture) {
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
    tracing::info!(organization_id=%claim.organization,bundle_id=%claim.bundle,plan_id=%claim.plan,manifest_id=%id,"Family refresh history proposal prepared");
    Ok(Prepared::Unit(id))
}

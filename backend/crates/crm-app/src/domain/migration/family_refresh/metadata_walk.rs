//! One retained Person occurrence per transaction outcome. Equal occurrences
//! share a unit; source conflicts are resolved before frozen-cohort filtering.
use super::{
    cohort::Claim,
    core_resolution::{self, Resolution},
    core_source::Derived,
    evidence::{Purpose, Scope},
    metadata_plan::{self, Prepared},
    model::{Counts, Family, Hold, Kind},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{crypto, metadata_store, snapshot::SnapshotPolicy, MigrationError},
};
use serde::Serialize;
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) struct Position {
    before: Option<Uuid>,
    next: Uuid,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Finished,
    Capacity,
}
async fn advance_source(
    conn: &mut PgConnection,
    claim: &Claim,
    position: Option<Position>,
) -> Result<(), MigrationError> {
    let Some(position) = position else {
        return Ok(());
    };
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET checkpoint_id=$4 WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$5 AND lease_expires_at>clock_timestamp() AND checkpoint_id IS NOT DISTINCT FROM $6 AND NOT source_walk_complete AND state='preparing'").bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(position.next).bind(claim.epoch).bind(position.before).execute(conn).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}
pub(super) enum Checkpoint {
    Source(Position),
    Owned { before: Option<Uuid>, next: Uuid },
}
pub(super) async fn advance(
    conn: &mut PgConnection,
    claim: &Claim,
    checkpoint: Option<Checkpoint>,
) -> Result<(), MigrationError> {
    match checkpoint {
        None => Ok(()),
        Some(Checkpoint::Source(position)) => advance_source(conn, claim, Some(position)).await,
        Some(Checkpoint::Owned { before, next }) => {
            let changed=sqlx::query("UPDATE migration_family_refresh_plan SET owned_after=$3 WHERE id=$1 AND organization_id=$2 AND owned_after IS NOT DISTINCT FROM $4 AND lease_token=$5 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp()").bind(claim.plan).bind(claim.organization.0).bind(next).bind(before).bind(claim.token).bind(claim.epoch).execute(conn).await?.rows_affected();
            if changed != 1 {
                return Err(MigrationError::Conflict);
            }
            Ok(())
        }
    }
}

pub async fn missing_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("source_walk_complete") {
        return Err(MigrationError::ImportBusy);
    }
    if p.get::<bool, _>("owned_walk_complete") {
        return Ok(Progress::Finished);
    }
    let before = p.get::<Option<Uuid>, _>("owned_after");
    let next:Option<Uuid>=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id>$3) ORDER BY id LIMIT 1").bind(claim.bundle).bind(claim.organization.0).bind(before).fetch_optional(&mut *tx).await?;
    let Some(next) = next else {
        sqlx::query("UPDATE migration_family_refresh_plan SET owned_walk_complete=true WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    drop(tx);
    Ok(
        match metadata_plan::prepare(
            pool,
            key,
            policy,
            claim,
            next,
            Some(Checkpoint::Owned { before, next }),
        )
        .await?
        {
            Prepared::Unit(_) => Progress::Advanced,
            Prepared::Capacity => Progress::Capacity,
        },
    )
}

#[derive(Serialize)]
struct Diagnostic {
    kind: Kind,
    source_id: Option<String>,
    source_row: Uuid,
    reason: String,
    excluded: bool,
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata"
        || !p.get::<bool, _>("catalog_walk_complete")
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    if p.get::<bool, _>("source_walk_complete") {
        return Ok(Progress::Finished);
    }
    let before: Option<Uuid> = p.get("checkpoint_id");
    let row=sqlx::query("SELECT s.*,owner.revision AS source_revision,owner.phase AS source_phase,owner.family AS source_family FROM migration_family_refresh_source s JOIN migration_family_refresh_plan owner ON owner.id=s.plan_id AND owner.bundle_id=s.bundle_id AND owner.organization_id=s.organization_id WHERE s.bundle_id=$1 AND s.organization_id=$2 AND s.kind='person' AND ($3::uuid IS NULL OR s.id>$3) ORDER BY s.id LIMIT 1").bind(claim.bundle).bind(claim.organization.0).bind(before).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        let Some(reservation) = preparation::reserve(&mut tx, claim, policy, 8192).await? else {
            return Ok(Progress::Capacity);
        };
        sqlx::query("UPDATE migration_family_refresh_plan SET source_walk_complete=true,phase='classify' WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        settle(&mut tx, claim, reservation).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    let position = Position {
        before,
        next: row.get("id"),
    };
    let family: Family =
        serde_json::from_value(serde_json::json!(row.get::<String, _>("source_family")))
            .map_err(|_| MigrationError::Crypto)?;
    if family == Family::History
        || !matches!(
            row.get::<String, _>("source_phase").as_str(),
            "mappings" | "classify" | "apply" | "finished"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: row.get("plan_id"),
        family,
        revision: row.get("source_revision"),
    };
    let data: Derived = scope.open(
        key,
        position.next,
        Purpose::Source,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let kind = Kind::Metadata;
    let source_kind = core_resolution::Kind::Person;
    let source_id: Option<String> = row.get("source_id");
    match &data {
        Derived::Metadata(record)
            if record.source_id == source_id
                && source_id == row.get::<Option<String>, _>("source_person_id") => {}
        Derived::Oversized { .. }
            if row.get::<Option<String>, _>("reason").as_deref() == Some("unit_too_large")
                && !row.get::<bool, _>("qualified") => {}
        _ => return Err(MigrationError::Crypto),
    }
    let hash = if let Some(source) = source_id.as_ref() {
        metadata_store::source_key(
            key,
            claim.organization,
            b.get("source_account_id"),
            "person",
            source.as_bytes(),
        )
    } else {
        crypto::snapshot_hmac(
            key,
            claim.organization,
            "family-refresh-metadata-diagnostic-v1",
            position.next.as_bytes(),
        )
        .to_vec()
    };
    drop(tx);
    let mut excluded = false;
    let held = if let Some(source) = source_id.as_deref() {
        // Resolve the entire identity before cohort filtering. Even a conflicting
        // in-cohort Person receives one atomic held Person unit, not an exclusion.
        let resolution = core_resolution::resolve(pool, key, claim, source_kind, source).await?;
        let (mut tx, _, _) = preparation::begin(pool, claim).await?;
        let cohort:Option<Uuid>=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND organization_id=$2 AND source_person_id=$3").bind(claim.bundle).bind(claim.organization.0).bind(source).fetch_optional(&mut *tx).await?;
        drop(tx);
        if let Some(cohort) = cohort {
            return Ok(
                match metadata_plan::prepare(
                    pool,
                    key,
                    policy,
                    claim,
                    cohort,
                    Some(Checkpoint::Source(position)),
                )
                .await?
                {
                    Prepared::Unit(_) => Progress::Advanced,
                    Prepared::Capacity => Progress::Capacity,
                },
            );
        }
        match resolution {
            Resolution::Ready(_) => {
                excluded = true;
                Hold::IdentityMismatch
            }
            Resolution::Held(h) => h,
        }
    } else if matches!(data, Derived::Oversized { .. }) {
        Hold::UnitTooLarge
    } else {
        Hold::UnsupportedSource
    };
    let reason = if excluded {
        "outside_frozen_cohort".into()
    } else {
        serde_json::to_value(held)
            .map_err(|_| MigrationError::Crypto)?
            .as_str()
            .ok_or(MigrationError::Crypto)?
            .to_owned()
    };
    persist(
        pool,
        key,
        policy,
        claim,
        position,
        &hash,
        Diagnostic {
            kind,
            source_id,
            source_row: position.next,
            reason,
            excluded,
        },
    )
    .await
}
async fn settle(conn: &mut PgConnection, claim: &Claim, token: Uuid) -> Result<(), MigrationError> {
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(token)
        .bind(claim.epoch)
        .execute(conn)
        .await?;
    Ok(())
}
async fn persist(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    position: Position,
    hash: &[u8],
    diagnostic: Diagnostic,
) -> Result<Progress, MigrationError> {
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    let kind = "metadata";
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4)").bind(claim.plan).bind(claim.organization.0).bind(kind).bind(hash).fetch_one(&mut *tx).await?;
    if exists {
        advance(&mut tx, claim, Some(Checkpoint::Source(position))).await?;
        tx.commit().await?;
        return Ok(Progress::Advanced);
    }
    let id = Uuid::new_v4();
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: p.get("revision"),
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &diagnostic)?;
    let bound =
        i64::try_from(sealed.ciphertext.len()).map_err(|_| MigrationError::StorageLimit)? + 16384;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Progress::Capacity);
    };
    let counts = Counts {
        units: 1,
        held: u64::from(!diagnostic.excluded),
        excluded: u64::from(diagnostic.excluded),
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
    let ordinal = p
        .get::<i64, _>("position")
        .checked_add(1)
        .ok_or(MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,source_row_id,position,kind,source_key_hmac,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(position.next).bind(ordinal).bind(kind).bind(hash).bind(if diagnostic.excluded{"excluded"}else{"held"}).bind(diagnostic.reason).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET position=$3,counts=$4,phase='classify' WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(ordinal).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    settle(&mut tx, claim, reservation).await?;
    advance(&mut tx, claim, Some(Checkpoint::Source(position))).await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,excluded=diagnostic.excluded,"Family refresh metadata diagnostic prepared");
    Ok(Progress::Advanced)
}

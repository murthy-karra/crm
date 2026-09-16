//! One exact mapping/outcome checkpoint per existing scheduler turn. Complete
//! catalog traversal is a preparation stage, never a ready/confirmation seal.
use super::{
    catalog_plan::{self, Prepared},
    cohort::Claim,
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Finished,
    Capacity,
}
pub(super) struct Position {
    pub id: Uuid,
    pub after: Option<Uuid>,
}
pub(super) async fn advance(
    conn: &mut PgConnection,
    claim: &Claim,
    position: Option<Position>,
) -> Result<(), MigrationError> {
    let Some(position) = position else {
        return Ok(());
    };
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET catalog_after=$3 WHERE id=$1 AND organization_id=$2 AND catalog_after IS NOT DISTINCT FROM $4 AND lease_token=$5 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND state='preparing'")
        .bind(claim.plan).bind(claim.organization.0).bind(position.id).bind(position.after).bind(claim.token).bind(claim.epoch).execute(conn).await?.rows_affected();
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
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("mappings_complete") {
        return Err(MigrationError::ImportBusy);
    }
    if p.get::<bool, _>("catalog_walk_complete") {
        return Ok(Progress::Finished);
    }
    let incomplete:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind IN ('tag','field','option') AND (source_sequence IS NULL OR source_ordinal IS NULL OR source_row_id IS NULL OR source_element IS NULL))")
        .bind(claim.plan).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    if incomplete {
        return Err(MigrationError::SourceNotEligible);
    }
    let after = p.get::<Option<Uuid>, _>("catalog_after");
    let next: Option<Uuid> =
        sqlx::query_scalar("SELECT crm_family_refresh_next_catalog_mapping($1,$2,$3,$4)")
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(after)
            .fetch_one(&mut *tx)
            .await?;
    let Some(id) = next else {
        sqlx::query("UPDATE migration_family_refresh_plan SET catalog_walk_complete=true WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    drop(tx);
    Ok(
        match catalog_plan::prepare(pool, key, policy, claim, id, Some(Position { id, after }))
            .await?
        {
            Prepared::Unit(_) => Progress::Advanced,
            Prepared::Capacity => Progress::Capacity,
        },
    )
}

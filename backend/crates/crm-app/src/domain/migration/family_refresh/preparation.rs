//! Common transaction admission for the two bounded preparation phases.
use super::{cohort::Claim, model::ENGINE};
use crate::{
    auth::workspace,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
pub(super) async fn begin(
    pool: &PgPool,
    claim: &Claim,
) -> Result<(Transaction<'static, Postgres>, PgRow, PgRow), MigrationError> {
    admit(pool, claim, false).await
}
/// Read-only proof adapters are reusable while a confirmed unit owns a live
/// execution lease. Preparation mutations retain their stricter admission.
pub(super) async fn read_begin(
    pool: &PgPool,
    claim: &Claim,
) -> Result<(Transaction<'static, Postgres>, PgRow, PgRow), MigrationError> {
    admit(pool, claim, true).await
}
async fn admit(
    pool: &PgPool,
    claim: &Claim,
    allow_execution: bool,
) -> Result<(Transaction<'static, Postgres>, PgRow, PgRow), MigrationError> {
    if claim.epoch <= 0 {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true),set_config('crm.family_refresh_lease',$2,true)")
        .bind(ENGINE).bind(claim.token.to_string()).execute(&mut *tx).await?;
    workspace::shared(&mut tx, claim.organization).await?;
    // Membership precedes retention/bundle/plan locks. Executor ownership is
    // immutable while preparing; it is rechecked below with the locked bundle.
    let executor=sqlx::query_scalar::<_,Uuid>("SELECT m.user_id FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m")
        .bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Forbidden)?;
    if allow_execution {
        sqlx::query("SELECT id FROM organization WHERE id=$1 FOR UPDATE")
            .bind(claim.organization.0)
            .fetch_one(&mut *tx)
            .await?;
    }
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let b=sqlx::query("SELECT * FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let p=sqlx::query("SELECT *,lease_expires_at>clock_timestamp() AS live_lease FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3 FOR UPDATE")
        .bind(claim.plan).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let preparing = b.get::<String, _>("state") == "preparing"
        && b.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_none()
        && p.get::<String, _>("state") == "preparing"
        && p.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_none();
    let executing = allow_execution
        && matches!(
            b.get::<String, _>("state").as_str(),
            "queued" | "running" | "paused"
        )
        && b.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_some()
        && p.get::<String, _>("state") == "running"
        && p.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_some()
        && !p.get::<bool, _>("cancel_requested");
    if b.get::<Uuid, _>("executor_user_id") != executor
        || !(preparing || executing)
        || p.get::<Option<Uuid>, _>("lease_token") != Some(claim.token)
        || p.get::<i64, _>("lease_epoch") != claim.epoch
        || p.get::<Option<bool>, _>("live_lease") != Some(true)
    {
        return Err(MigrationError::Conflict);
    }
    let workspace_matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=$1 AND import_id=$2 AND plan_id=$3)")
        .bind(claim.organization.0).bind(b.get::<Uuid,_>("parent_import_id")).bind(b.get::<Uuid,_>("parent_plan_id")).fetch_one(&mut *tx).await?;
    if !workspace_matches {
        return Err(MigrationError::Conflict);
    }
    Ok((tx, b, p))
}
pub(super) async fn reserve(
    conn: &mut PgConnection,
    claim: &Claim,
    policy: &SnapshotPolicy,
    amount: i64,
) -> Result<Option<Uuid>, MigrationError> {
    // A crashed older claim can leave only a settled reservation; reclaim it
    // through the existing epoch-fenced function before reserving this page.
    if let Some(old)=sqlx::query("SELECT token,lease_epoch FROM migration_family_refresh_reservation WHERE plan_id=$1 AND organization_id=$2 AND purpose='unit'")
        .bind(claim.plan).bind(claim.organization.0).fetch_optional(&mut *conn).await? {
        if old.get::<i64,_>("lease_epoch")>=claim.epoch {return Err(MigrationError::Conflict);}
        sqlx::query("SELECT crm_family_refresh_reclaim($1,$2,$3,$4,$5)").bind(claim.organization.0).bind(claim.bundle).bind(claim.plan).bind(old.get::<Uuid,_>("token")).bind(claim.epoch).execute(&mut *conn).await?;
    }
    let reservation = Uuid::new_v4();
    let reserved: bool =
        sqlx::query_scalar("SELECT crm_family_refresh_reserve($1,$2,$3,$4,$5,$6,'unit',$7,$8)")
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(reservation)
            .bind(claim.epoch)
            .bind(amount)
            .bind(policy.run_ceiling_bytes)
            .bind(policy.org_ceiling_bytes)
            .fetch_one(&mut *conn)
            .await?;
    if !reserved {
        return Ok(None);
    }
    Ok(Some(reservation))
}

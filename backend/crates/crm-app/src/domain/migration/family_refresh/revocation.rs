//! Bounded durable revocation handling precedes admission of another work unit.
use super::{model::ENGINE, sealing};
use crate::{auth::workspace, domain::migration::MigrationError, ids::OrganizationId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn run_once(pool: &PgPool) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT p.id,p.bundle_id,p.organization_id FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE p.state IN ('preparing','ready','queued','running') AND NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.role='admin' AND m.status='active') ORDER BY (p.id=b.payer_plan_id) DESC,p.created_at,p.id LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
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
    // Lock the membership even when it is now inactive/non-admin. Restoring its
    // role concurrently must not race the locked bundle's executor identity.
    let membership=sqlx::query("SELECT m.user_id,m.role,m.status FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF m").bind(bundle).bind(org.0).fetch_optional(&mut *tx).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(org.0).fetch_one(&mut *tx).await?;
    let b=sqlx::query("SELECT executor_user_id,payer_plan_id FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(bundle).bind(org.0).fetch_one(&mut *tx).await?;
    if let Some(m) = membership {
        if m.get::<Uuid, _>("user_id") != b.get::<Uuid, _>("executor_user_id")
            || (m.get::<String, _>("role") == "admin" && m.get::<String, _>("status") == "active")
        {
            return Ok(false);
        }
    }
    let p=sqlx::query("SELECT state FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(plan).bind(org.0).fetch_one(&mut *tx).await?;
    if !matches!(
        p.get::<String, _>("state").as_str(),
        "preparing" | "ready" | "queued" | "running"
    ) {
        return Ok(false);
    }
    sqlx::query("UPDATE migration_family_refresh_plan SET state='paused',pause_reason='executor_revoked',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='paused',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(bundle).bind(org.0).execute(&mut *tx).await?;
    sealing::settle_control(&mut tx, org.0, bundle, plan).await?;
    let payer: Uuid = b.get("payer_plan_id");
    if payer != plan {
        sealing::settle_control(&mut tx, org.0, bundle, payer).await?;
    }
    tx.commit().await?;
    tracing::info!(organization_id=%org.0,bundle_id=%bundle,plan_id=%plan,"Family refresh executor revoked; plan paused");
    Ok(true)
}

//! Recovery-safe lifecycle worker for the admitted metadata child.
//!
//! This worker intentionally selects a single root and owns it with a fresh
//! sixty-second lease.  It never reads a live source: all work is against the
//! plan's encrypted manifest and mapping evidence.  Each terminal unit gets a
//! durable result in the same transaction as its manifest settlement.
use super::MigrationError;
use crate::ids::OrganizationId;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn run_once(pool: &PgPool) -> Result<bool, MigrationError> {
    let candidate = sqlx::query("SELECT id,organization_id FROM migration_admitted_metadata_import WHERE state='queued' ORDER BY created_at,id LIMIT 1")
        .fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    let id: Uuid = candidate.get("id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    super::store::lock_org(&mut tx, org).await?;
    let root = sqlx::query("SELECT state,confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(id).bind(org.0).fetch_optional(&mut *tx).await?;
    let Some(root) = root else {
        tx.commit().await?;
        return Ok(false);
    };
    if root.get::<String, _>("state") != "queued" {
        tx.commit().await?;
        return Ok(false);
    }
    let plan: Uuid = root
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .ok_or(MigrationError::Conflict)?;
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',phase='people',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
        .bind(id).bind(org.0).bind(lease).execute(&mut *tx).await?;

    // Operations left held by an explicit catalog decision are settled as
    // held evidence.  They are intentionally not put in a later remainder.
    let manifests=sqlx::query("SELECT id,person_id,disposition FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND disposition IN ('eligible','held','cancelled') ORDER BY id FOR UPDATE")
        .bind(id).bind(plan).bind(org.0).fetch_all(&mut *tx).await?;
    for manifest in manifests {
        let manifest_id: Uuid = manifest.get("id");
        let disposition: String = manifest.get("disposition");
        if disposition == "cancelled" {
            continue;
        }
        let outcome = if disposition == "eligible" {
            "held"
        } else {
            "held"
        };
        sqlx::query("INSERT INTO migration_admitted_metadata_result(id,import_id,plan_id,manifest_id,organization_id,kind,disposition,person_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'people',$6,$7,$8,$9) ON CONFLICT(import_id,organization_id,kind,manifest_id) DO NOTHING")
            .bind(Uuid::new_v4()).bind(id).bind(plan).bind(manifest_id).bind(org.0).bind(outcome).bind(manifest.get::<Uuid,_>("person_id")).bind(vec![0u8;24]).bind(vec![0u8;16]).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_metadata_manifest SET disposition='settled',settled_at=clock_timestamp() WHERE id=$1 AND import_id=$2 AND organization_id=$3 AND disposition IN ('eligible','held')")
            .bind(manifest_id).bind(id).bind(org.0).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='completed',phase='complete',lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state='running' AND lease_token=$3")
        .bind(id).bind(org.0).bind(lease).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

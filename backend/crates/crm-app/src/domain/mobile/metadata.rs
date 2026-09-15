//! Shared admission for the metadata lock graph.
//!
//! Link/value writers take the shared side before locking a Person. Catalog
//! writers and import workers take the exclusive side before their catalog or
//! Person locks. Keeping this one small primitive independent of `MobileError`
//! lets the established command errors retain their normal SQLx conversion.
use sqlx::PgConnection;

use crate::ids::OrganizationId;

pub(crate) async fn acquire_shared(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock_shared(hashtextextended('crm-mobile-metadata-catalog:' || $1::text, 0))",
    )
    .bind(organization_id.0)
    .execute(conn)
    .await?;
    Ok(())
}

pub(crate) async fn acquire_exclusive(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-metadata-catalog:' || $1::text, 0))",
    )
    .bind(organization_id.0)
    .execute(conn)
    .await?;
    Ok(())
}

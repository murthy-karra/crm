//! Authenticate a refresh-owned first metadata display under its original plan.
use super::{
    evidence::{Purpose, Scope},
    model::Family,
};
use crate::{config::RawPayloadKey, domain::migration::MigrationError, ids::OrganizationId};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use uuid::Uuid;
pub(crate) async fn initial(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    bundle: Uuid,
    plan: Uuid,
    manifest: Uuid,
) -> Result<Value, MigrationError> {
    let row=sqlx::query("SELECT d.nonce,d.ciphertext,p.revision FROM migration_history_import_display d JOIN migration_family_refresh_plan p ON p.id=d.refresh_plan_id AND p.bundle_id=d.refresh_bundle_id AND p.organization_id=d.organization_id JOIN migration_family_refresh_manifest m ON m.id=d.refresh_manifest_id AND m.plan_id=p.id AND m.bundle_id=p.bundle_id AND m.organization_id=p.organization_id JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=p.id AND r.bundle_id=p.bundle_id AND r.organization_id=p.organization_id JOIN migration_history_import_identity i ON i.refresh_manifest_id=m.id AND i.refresh_plan_id=p.id AND i.refresh_bundle_id=p.bundle_id AND i.organization_id=p.organization_id AND i.person_id=r.person_id AND i.fact_id=r.target_id WHERE d.id=$1 AND d.organization_id=$2 AND d.refresh_plan_id=$3 AND d.refresh_bundle_id=$4 AND p.family='history' AND p.confirmed_at IS NOT NULL AND m.disposition='insert' AND r.disposition='applied' AND i.erased_at IS NULL")
        .bind(manifest).bind(org.0).bind(plan).bind(bundle).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    Scope {
        organization: org,
        bundle,
        plan,
        family: Family::History,
        revision: row.try_get("revision")?,
    }
    .open(
        key,
        manifest,
        Purpose::HistoryDisplay,
        &row.try_get::<Vec<u8>, _>("nonce")?,
        &row.try_get::<Vec<u8>, _>("ciphertext")?,
    )
}

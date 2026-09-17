//! Recover choices by exact source key from the immediate immutable predecessor.
use super::{
    evidence::{Purpose, Scope},
    mapping_inventory::Mapping,
    mapping_selection::Patch,
};
use crate::{config::RawPayloadKey, domain::migration::MigrationError};
use sqlx::{PgConnection, Row};

pub(super) async fn apply(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    data: &mut Mapping,
) -> Result<(), MigrationError> {
    let previous=sqlx::query("SELECT m.*,parent.source_key_hmac AS parent_key,previous.revision AS previous_revision FROM migration_family_refresh_plan current JOIN migration_family_refresh_plan previous ON previous.id=current.predecessor_plan_id AND previous.bundle_id=current.bundle_id AND previous.organization_id=current.organization_id AND previous.family=current.family JOIN migration_family_refresh_mapping m ON m.plan_id=previous.id AND m.bundle_id=previous.bundle_id AND m.organization_id=previous.organization_id LEFT JOIN migration_family_refresh_mapping parent ON parent.id=m.parent_id AND parent.plan_id=m.plan_id AND parent.organization_id=m.organization_id WHERE current.id=$1 AND current.bundle_id=$2 AND current.organization_id=$3 AND m.kind=$4 AND m.source_key_hmac=$5")
        .bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).bind(&data.kind).bind(&data.source_key).fetch_optional(&mut *conn).await?;
    let Some(previous) = previous else {
        return Ok(());
    };
    let previous_scope = Scope {
        plan: previous.get("plan_id"),
        revision: previous.get("previous_revision"),
        ..scope
    };
    let old: Mapping = previous_scope.open(
        key,
        previous.get::<uuid::Uuid, _>("id"),
        Purpose::Mapping,
        previous.get("nonce"),
        previous.get("ciphertext"),
    )?;
    old.verify(&previous)?;
    if old.kind != data.kind
        || old.source_key != data.source_key
        || old.parent_key != data.parent_key
        || old.qualified != data.qualified
        || old.creation_allowed != data.creation_allowed
    {
        return Err(MigrationError::Crypto);
    }
    data.choice = old.choice;
    data.destination = old.destination;
    if let Some(row)=sqlx::query("SELECT * FROM migration_family_refresh_mapping_patch WHERE plan_id=$1 AND bundle_id=$2 AND organization_id=$3 AND kind=$4 AND source_key_hmac=$5")
        .bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).bind(&data.kind).bind(&data.source_key).fetch_optional(conn).await? {
        let patch:Patch=scope.open(key,row.get("id"),Purpose::MappingPatch,row.get("nonce"),row.get("ciphertext"))?;
        let (disposition,target)=patch.choice.columns();
        if patch.kind!=data.kind || patch.source_key!=data.source_key || patch.source_mapping!=previous.get::<uuid::Uuid,_>("id") || patch.source_mapping!=row.get::<uuid::Uuid,_>("source_mapping_id") || patch.source_plan!=previous_scope.plan || patch.source_plan!=row.get::<uuid::Uuid,_>("source_plan_id") || disposition!=row.get::<String,_>("disposition") || target!=row.get("target_id") {
            return Err(MigrationError::Crypto);
        }
        data.choice=patch.choice;
        data.destination=patch.destination;
    }
    Ok(())
}

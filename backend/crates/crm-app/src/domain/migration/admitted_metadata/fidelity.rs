//! Bounded held-unit summaries preserve complete counts without expanding an
//! oversized operation set into an execution or reader response.
use super::*;
use sqlx::PgConnection;

pub(crate) async fn groups(
    c: &mut PgConnection,
    org: OrganizationId,
    plan: Uuid,
    manifest: Uuid,
) -> Result<Value, MigrationError> {
    let rows=sqlx::query("SELECT kind,disposition,count(*) AS n FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3 GROUP BY kind,disposition ORDER BY kind,disposition").bind(manifest).bind(plan).bind(org.0).fetch_all(c).await?;
    Ok(json!(rows.iter().map(|r|json!({"kind":r.get::<String,_>("kind"),"disposition":r.get::<String,_>("disposition"),"count":r.get::<i64,_>("n")})).collect::<Vec<_>>()))
}
pub(super) fn counts(
    groups: &Value,
    result: bool,
) -> Result<super::super::metadata_model::Counts, MigrationError> {
    let mut c = super::super::metadata_model::Counts::default();
    for group in groups.as_array().ok_or(MigrationError::Crypto)? {
        let kind = group["kind"].as_str().ok_or(MigrationError::Crypto)?;
        let disposition = group["disposition"]
            .as_str()
            .ok_or(MigrationError::Crypto)?;
        let n = group["count"].as_i64().ok_or(MigrationError::Crypto)?;
        let family = c.family(kind);
        family.planned += n;
        if !result {
            family.pending += n;
        }
        family.add(disposition, n);
        if disposition == "held" {
            c.held_count += n;
        }
    }
    Ok(c)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_definition(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    parent: Option<Uuid>,
    f: &mut FrozenMapping,
) -> Result<(), MigrationError> {
    if f.definition.is_none() {
        if let Some(parent) = parent {
            let r=sqlx::query("SELECT nonce,ciphertext FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3 AND kind='field'").bind(parent).bind(plan).bind(org.0).fetch_one(c).await?;
            let original: FrozenMapping = super::super::admitted_metadata_worker::open(
                key,
                org,
                snapshot,
                plan,
                parent,
                "mapping",
                &r.get::<Vec<u8>, _>("nonce"),
                &r.get::<Vec<u8>, _>("ciphertext"),
            )?;
            f.definition = original.definition;
        }
    }
    Ok(())
}
pub(crate) async fn mapping(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    r: &sqlx::postgres::PgRow,
) -> Result<FrozenMapping, MigrationError> {
    let mut f = super::super::admitted_metadata_worker::open(
        key,
        org,
        snapshot,
        plan,
        r.get("id"),
        "mapping",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )?;
    resolve_definition(
        c,
        key,
        org,
        snapshot,
        plan,
        r.get("parent_mapping_id"),
        &mut f,
    )
    .await?;
    Ok(f)
}

//! Complete retained field qualification before mapping or Person conversion.
//! One bounded definition is decrypted; an indexed equality probe detects names
//! claimed by another source ID across every occurrence. No native catalog permission
//! is produced here.
use super::{
    cohort::Claim,
    core_resolution::{self, Resolution, Resolved},
    core_source::Derived,
    model::Hold,
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{metadata_source::Entity, metadata_store, MigrationError},
};
use sqlx::{PgPool, Row};

pub enum Qualification {
    Ready(Box<Resolved>),
    Held(Hold),
}

pub async fn qualify_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    source_id: &str,
) -> Result<Qualification, MigrationError> {
    let selected =
        match core_resolution::resolve(pool, key, claim, core_resolution::Kind::Field, source_id)
            .await?
        {
            Resolution::Ready(r) => r,
            Resolution::Held(h) => return Ok(Qualification::Held(h)),
        };
    let Derived::Metadata(record) = &selected.record else {
        return Err(MigrationError::Crypto);
    };
    let Entity::Field(field) = &record.entity else {
        return Err(MigrationError::Crypto);
    };
    if !record.reasons.is_empty() || !field.reasons.is_empty() {
        return Ok(Qualification::Held(Hold::UnsupportedSource));
    }
    let Some(name) = field.name.as_ref() else {
        return Ok(Qualification::Held(Hold::UnsupportedSource));
    };
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("mappings_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let account = b.get("source_account_id");
    let name_key = metadata_store::source_key(
        key,
        claim.organization,
        account,
        "field-name",
        name.as_bytes(),
    );
    // Every occurrence participates, including conflicting definitions of one
    // source ID. A deduplicated mapping index could hide a colliding second name.
    let incomplete:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND kind='field' AND NOT catalog_names_indexed)")
        .bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    if incomplete {
        return Ok(Qualification::Held(Hold::SourceUnavailable));
    }
    let stored:Option<Vec<u8>>=sqlx::query_scalar("SELECT field_name_hmac FROM migration_family_refresh_source WHERE id=$1 AND bundle_id=$2 AND organization_id=$3 AND kind='field' AND catalog_names_indexed")
        .bind(selected.row).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    if stored.as_ref() != Some(&name_key) {
        return Err(MigrationError::Crypto);
    }
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT DISTINCT source_id FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND kind='field' AND field_name_hmac=$3 LIMIT 2) names")
        .bind(claim.bundle).bind(claim.organization.0).bind(name_key).fetch_one(&mut *tx).await?;
    if count != 1 {
        return Ok(Qualification::Held(Hold::SourceConflict));
    }
    if field.field_type.as_deref() == Some("choice") {
        let labels: Vec<&str> = field
            .choices
            .iter()
            .filter_map(|c| c.label.as_deref())
            .collect();
        let collision:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM unnest($1::text[]) labels(label) GROUP BY lower(label) HAVING count(*)>1)")
            .bind(labels).fetch_one(&mut *tx).await?;
        if collision {
            return Ok(Qualification::Held(Hold::SourceConflict));
        }
    }
    Ok(Qualification::Ready(selected))
}

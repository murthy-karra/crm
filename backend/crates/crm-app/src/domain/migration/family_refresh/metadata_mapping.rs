//! Convert retained Person values through this plan's immutable catalog outcomes.
//! Missing/null values retain their distinct gap semantics; no source null is a
//! clear under the current capture profile. Prospective catalog IDs stay frozen.
use super::{
    catalog_plan,
    cohort::Claim,
    core_resolution::{self, Resolution},
    core_source::Derived,
    evidence::{Purpose, Scope},
    metadata_delta::{Catalog, Field, FieldIntent, Source, Tags},
    metadata_destination,
    model::{Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        custom_field::CustomFieldValue,
        migration::{
            metadata_source::{self, Entity, NativeValue},
            metadata_store, MigrationError,
        },
    },
    ids::CustomFieldOptionId,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub source_row: Uuid,
    pub semantic: Vec<u8>,
    pub source: Source,
    pub catalog_units: BTreeSet<Uuid>,
    pub transformations: BTreeSet<String>,
}
pub struct Converted {
    pub evidence: Evidence,
    pub catalog: Catalog,
}
pub enum Conversion {
    Ready(Box<Converted>),
    Held(Hold),
}

async fn catalog_unit(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    kind: &str,
    hash: &[u8],
) -> Result<Result<(Uuid, Box<metadata_destination::Evidence>), Hold>, MigrationError> {
    let row=sqlx::query("SELECT u.* FROM migration_family_refresh_mapping m JOIN migration_family_refresh_manifest u ON u.mapping_id=m.id AND u.plan_id=m.plan_id AND u.organization_id=m.organization_id AND u.kind='catalog' WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind=$3 AND m.source_key_hmac=$4")
        .bind(scope.plan).bind(scope.organization.0).bind(kind).bind(hash).fetch_optional(conn).await?;
    let Some(row) = row else {
        return Ok(Err(Hold::MappingRequired));
    };
    let evidence: catalog_plan::Evidence = scope.open(
        key,
        row.get("id"),
        Purpose::Manifest,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    if Some(evidence.mapping_id) != row.get::<Option<Uuid>, _>("mapping_id")
        || Some(evidence.source_row) != row.get::<Option<Uuid>, _>("source_row_id")
    {
        return Err(MigrationError::Crypto);
    }
    match evidence.decision {
        catalog_plan::Decision::Held { reason, .. } => Ok(Err(reason)),
        catalog_plan::Decision::Ready { evidence } => {
            if evidence.mapping_id != row.get::<Uuid, _>("mapping_id")
                || evidence.mapping.kind != kind
                || evidence.mapping.source_key != hash
                || Some(evidence.target) != row.get::<Option<Uuid>, _>("target_id")
            {
                return Err(MigrationError::Crypto);
            }
            Ok(Ok((row.get("id"), evidence)))
        }
    }
}

pub async fn convert(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    source_id: &str,
    owned_fields: &BTreeSet<Uuid>,
) -> Result<Conversion, MigrationError> {
    let selected =
        match core_resolution::resolve(pool, key, claim, core_resolution::Kind::Person, source_id)
            .await?
        {
            Resolution::Ready(r) => r,
            Resolution::Held(h) => return Ok(Conversion::Held(h)),
        };
    let Derived::Metadata(record) = &selected.record else {
        return Err(MigrationError::Crypto);
    };
    let Entity::Person(person) = &record.entity else {
        return Err(MigrationError::Crypto);
    };
    if !record.reasons.is_empty() {
        return Ok(Conversion::Held(Hold::UnsupportedSource));
    }
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("catalog_walk_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: p.get("revision"),
    };
    let account = b.get("source_account_id");
    let hash = |kind: &str, raw: &[u8]| {
        metadata_store::source_key(key, claim.organization, account, kind, raw)
    };
    let mut units = BTreeSet::new();
    let mut transformations = record
        .transformations
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut catalog = Catalog {
        live_tags: BTreeSet::new(),
        live_fields: BTreeMap::new(),
    };
    let native_tags: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM tag WHERE organization_id=$1 ORDER BY id LIMIT 201")
            .bind(claim.organization.0)
            .fetch_all(&mut *tx)
            .await?;
    if native_tags.len() > 200 {
        return Ok(Conversion::Held(Hold::TargetUnavailable));
    }
    catalog.live_tags.extend(native_tags);
    let tags = match person.tags_state.as_str() {
        "not_supplied" => Tags::Missing,
        "source_null" => Tags::Null,
        "empty" | "present" => {
            let mut values = Vec::new();
            let mut groups = BTreeMap::new();
            for tag in &person.tags {
                if !tag.reasons.is_empty() {
                    return Ok(Conversion::Held(Hold::UnsupportedSource));
                }
                let (Some(raw), Some(label)) = (&tag.raw, &tag.label) else {
                    return Err(MigrationError::Crypto);
                };
                let target = if let Some(target) = groups.get(label) {
                    *target
                } else {
                    let folded: String = sqlx::query_scalar("SELECT lower($1)")
                        .bind(label)
                        .fetch_one(&mut *tx)
                        .await?;
                    let (unit, evidence) = match catalog_unit(
                        &mut tx,
                        key,
                        scope,
                        "tag",
                        &hash("tag-group", folded.as_bytes()),
                    )
                    .await?
                    {
                        Ok(v) => v,
                        Err(h) => return Ok(Conversion::Held(h)),
                    };
                    units.insert(unit);
                    catalog.live_tags.insert(evidence.target);
                    groups.insert(label.clone(), evidence.target);
                    evidence.target
                };
                values.push((
                    hash("tag-alias", raw.as_bytes())
                        .try_into()
                        .map_err(|_| MigrationError::Crypto)?,
                    target,
                ));
            }
            Tags::Complete(values)
        }
        _ => return Ok(Conversion::Held(Hold::UnsupportedSource)),
    };
    // One indexed batch discovers definitions for supplied properties, including
    // held/unselected definitions. Do not decrypt every source field per Person.
    let names: Vec<Vec<u8>> = record
        .provenance
        .iter()
        .filter(|(_, raw)| raw.as_str() != "null")
        .map(|(name, _)| hash("field-name", name.as_bytes()))
        .collect();
    let supplied:Vec<String>=sqlx::query_scalar("SELECT DISTINCT source_id FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND kind='field' AND field_name_hmac=ANY($3) ORDER BY source_id LIMIT 51")
        .bind(claim.bundle).bind(claim.organization.0).bind(names).fetch_all(&mut *tx).await?;
    if supplied.len() > 50 {
        return Ok(Conversion::Held(Hold::UnitTooLarge));
    }
    let mut fields: BTreeSet<Vec<u8>> = supplied
        .iter()
        .map(|id| hash("field", id.as_bytes()))
        .collect();
    let selected_fields:Vec<Vec<u8>>=sqlx::query_scalar("SELECT m.source_key_hmac FROM migration_family_refresh_mapping m JOIN migration_family_refresh_manifest u ON u.mapping_id=m.id AND u.plan_id=m.plan_id AND u.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind='field' AND u.disposition IN ('insert','already_current') ORDER BY m.id LIMIT 51")
        .bind(claim.plan).bind(claim.organization.0).fetch_all(&mut *tx).await?;
    if selected_fields.len() > 50 {
        return Ok(Conversion::Held(Hold::UnitTooLarge));
    }
    fields.extend(selected_fields);
    let mut intents = Vec::new();
    for field_key in fields {
        let (unit, evidence) = match catalog_unit(&mut tx, key, scope, "field", &field_key).await? {
            Ok(v) => v,
            Err(h) => return Ok(Conversion::Held(h)),
        };
        let definition = evidence
            .frozen
            .definition
            .as_ref()
            .ok_or(MigrationError::Crypto)?;
        let value = metadata_source::extract_value(record, definition);
        transformations.extend(value.transformations);
        let intent = match value.disposition.as_str() {
            "not_supplied" => FieldIntent::Missing,
            "source_null" => FieldIntent::UnknownNull,
            "eligible" => FieldIntent::Set(match value.value.ok_or(MigrationError::Crypto)? {
                NativeValue::Text(v) => CustomFieldValue::Text(v),
                NativeValue::Number(v) => {
                    // Extraction already proves exact NUMERIC(19,4) range and
                    // scale. Compare in PostgreSQL's storage representation so
                    // 1.25 and retained 1.2500 do not create a false update.
                    let native: String =
                        sqlx::query_scalar("SELECT ($1::text::numeric(19,4))::text")
                            .bind(&v)
                            .fetch_one(&mut *tx)
                            .await?;
                    CustomFieldValue::Number(native)
                }
                NativeValue::Date(v) => CustomFieldValue::Date(
                    chrono::NaiveDate::parse_from_str(&v, "%Y-%m-%d")
                        .map_err(|_| MigrationError::Crypto)?,
                ),
                NativeValue::Choice(raw) => {
                    let encoded =
                        serde_json::to_vec(&(evidence.frozen.source_id.as_str(), raw.as_str()))
                            .map_err(|_| MigrationError::Crypto)?;
                    let (option_unit, option) = match catalog_unit(
                        &mut tx,
                        key,
                        scope,
                        "option",
                        &hash("option", &encoded),
                    )
                    .await?
                    {
                        Ok(v) => v,
                        Err(h) => return Ok(Conversion::Held(h)),
                    };
                    if option.field != Some(evidence.target) {
                        return Err(MigrationError::Crypto);
                    }
                    units.insert(option_unit);
                    catalog
                        .live_fields
                        .entry(evidence.target)
                        .or_insert_with(|| Field {
                            kind: crate::domain::custom_field::FieldType::Choice,
                            live_options: BTreeSet::new(),
                        })
                        .live_options
                        .insert(option.target);
                    CustomFieldValue::Choice(CustomFieldOptionId::new(option.target))
                }
            }),
            _ => return Ok(Conversion::Held(Hold::UnsupportedSource)),
        };
        let kind = serde_json::from_value(serde_json::json!(definition
            .field_type
            .as_ref()
            .ok_or(MigrationError::Crypto)?))
        .map_err(|_| MigrationError::Crypto)?;
        catalog
            .live_fields
            .entry(evidence.target)
            .or_insert_with(|| Field {
                kind,
                live_options: BTreeSet::new(),
            });
        units.insert(unit);
        intents.push((evidence.target, intent));
    }
    let observed: BTreeSet<_> = intents.iter().map(|(id, _)| *id).collect();
    intents.extend(
        owned_fields
            .difference(&observed)
            .map(|id| (*id, FieldIntent::Missing)),
    );
    Ok(Conversion::Ready(Box::new(Converted {
        evidence: Evidence {
            source_row: selected.row,
            semantic: selected.semantic,
            source: Source {
                tags,
                fields: intents,
            },
            catalog_units: units,
            transformations,
        },
        catalog,
    })))
}

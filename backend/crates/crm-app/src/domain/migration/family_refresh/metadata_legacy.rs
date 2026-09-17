//! Legacy metadata B is reconstructed from successful immutable operations.
use super::metadata_baseline::{self, AfterState, Binding};
use crate::domain::migration::{metadata_model::Operation, MigrationError};
use serde_json::Value;
use sqlx::{postgres::PgRow, PgConnection, Row};

pub(super) async fn original(
    conn: &mut PgConnection,
    row: &PgRow,
    operations: Value,
    binding: &Binding,
) -> Result<Option<AfterState>, MigrationError> {
    if !revision_proven(conn, row, binding).await? {
        return Ok(None);
    }
    let Ok(mut operations) = serde_json::from_value::<Vec<Operation>>(operations) else {
        return Ok(None);
    };
    normalize_numbers(conn, &mut operations).await?;
    metadata_baseline::reconstruct(binding, &operations)
}

pub(super) async fn revision_proven(
    conn: &mut PgConnection,
    row: &PgRow,
    binding: &Binding,
) -> Result<bool, MigrationError> {
    // Each fact is written atomically with a new, empty Person. The source
    // import's scoped cohort/result has already been authenticated by discovery.
    // Recovery has its own immutable creation fact, not a mutable created_at.
    Ok(sqlx::query_scalar("WITH floor AS (SELECT crm_family_refresh_revision_installed('metadata') AS at) SELECT EXISTS(SELECT 1 FROM floor WHERE at IS NOT NULL AND $3::timestamptz>at AND EXISTS(SELECT 1 FROM person_imported WHERE organization_id=$1 AND person_id=$2 AND occurred_at>at UNION ALL SELECT 1 FROM person_admitted WHERE organization_id=$1 AND person_id=$2 AND occurred_at>at UNION ALL SELECT 1 FROM person_recovered WHERE organization_id=$1 AND person_id=$2 AND occurred_at>at))")
        .bind(binding.organization.0).bind(binding.person).bind(row.get::<chrono::DateTime<chrono::Utc>,_>("committed_at"))
        .fetch_one(conn).await?)
}

pub(super) async fn admitted(
    conn: &mut PgConnection,
    key: &crate::config::RawPayloadKey,
    row: &PgRow,
    outcomes: Value,
    binding: &Binding,
) -> Result<Option<AfterState>, MigrationError> {
    use crate::domain::migration::{
        admitted_metadata::{FrozenMapping, FrozenOperation},
        admitted_metadata_worker::open,
        metadata_source::NativeValue,
    };
    use uuid::Uuid;
    if !revision_proven(conn, row, binding).await? {
        return Ok(None);
    }
    let Some(outcomes) = outcomes.as_array() else {
        return Ok(None);
    };
    let plan: Uuid = row.get("plan_id");
    let snapshot: Uuid = row.get("snapshot_id");
    let frozen = sqlx::query("SELECT * FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4 ORDER BY id")
        .bind(binding.manifest).bind(binding.import).bind(plan).bind(binding.organization.0).fetch_all(&mut *conn).await?;
    if frozen.len() != outcomes.len() {
        return Ok(None);
    }
    let mut operations = Vec::with_capacity(frozen.len());
    for op in frozen {
        let id: Uuid = op.get("id");
        let matching: Vec<_> = outcomes
            .iter()
            .filter(|r| r["operation_id"] == serde_json::json!(id))
            .collect();
        if matching.len() != 1 {
            return Ok(None);
        }
        let result = matching[0];
        let kind: String = op.get("kind");
        let mapping: Option<Uuid> = op.get("mapping_id");
        if result["kind"] != kind
            || result["mapping_id"] != serde_json::json!(mapping)
            || result["planned_disposition"] != op.get::<String, _>("disposition")
        {
            return Ok(None);
        }
        let Some(outcome) = result["outcome"].as_str() else {
            return Ok(None);
        };
        let mut data: FrozenOperation = open(
            key,
            binding.organization,
            snapshot,
            plan,
            id,
            "operation",
            &op.get::<Vec<u8>, _>("nonce"),
            &op.get::<Vec<u8>, _>("ciphertext"),
        )?;
        if matches!(outcome, "applied" | "already_present") {
            let Some(mapping) = mapping else {
                return Ok(None);
            };
            let map=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3")
                .bind(mapping).bind(plan).bind(binding.organization.0).fetch_optional(&mut *conn).await?;
            let Some(map) = map else { return Ok(None) };
            let _: FrozenMapping = open(
                key,
                binding.organization,
                snapshot,
                plan,
                mapping,
                "mapping",
                &map.get::<Vec<u8>, _>("nonce"),
                &map.get::<Vec<u8>, _>("ciphertext"),
            )?;
            if map.get::<Option<Uuid>, _>("target_id") != op.get::<Option<Uuid>, _>("target_id")
                || map.get::<Vec<u8>, _>("source_key") != op.get::<Vec<u8>, _>("source_key")
            {
                return Ok(None);
            };
            if let Some(NativeValue::Choice(raw)) = &data.value {
                let choices=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE parent_mapping_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY id")
                    .bind(mapping).bind(plan).bind(binding.organization.0).fetch_all(&mut *conn).await?;
                let mut targets = Vec::new();
                for choice in choices {
                    let value: FrozenMapping = open(
                        key,
                        binding.organization,
                        snapshot,
                        plan,
                        choice.get("id"),
                        "mapping",
                        &choice.get::<Vec<u8>, _>("nonce"),
                        &choice.get::<Vec<u8>, _>("ciphertext"),
                    )?;
                    if value.raw_choice.as_ref() == Some(raw) {
                        let Some(target) = choice.get::<Option<Uuid>, _>("target_id") else {
                            return Ok(None);
                        };
                        targets.push(target);
                    }
                }
                if targets.len() != 1 {
                    return Ok(None);
                };
                data.value = Some(NativeValue::Choice(targets[0].to_string()));
            }
        }
        operations.push(Operation {
            id,
            kind,
            source_key: op.get("source_key"),
            source_field: data.source_field,
            mapping_id: mapping,
            target_id: op.get("target_id"),
            disposition: outcome.into(),
            value: data.value,
            reasons: Vec::new(),
            transformations: Vec::new(),
        });
    }
    normalize_numbers(conn, &mut operations).await?;
    metadata_baseline::reconstruct(binding, &operations)
}

/// Native storage uses NUMERIC(19,4); reproduce that immutable INSERT cast,
/// including trailing scale, without consulting the current stored value.
async fn normalize_numbers(
    conn: &mut PgConnection,
    operations: &mut [Operation],
) -> Result<(), MigrationError> {
    use crate::domain::migration::metadata_source::NativeValue;
    for op in operations {
        if let Some(NativeValue::Number(value)) = &mut op.value {
            if matches!(op.disposition.as_str(), "applied" | "already_present") {
                *value = sqlx::query_scalar("SELECT $1::text::numeric(19,4)::text")
                    .bind(&*value)
                    .fetch_one(&mut *conn)
                    .await?;
            }
        }
    }
    Ok(())
}

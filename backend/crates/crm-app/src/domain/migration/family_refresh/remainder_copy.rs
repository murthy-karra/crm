//! A bounded exact copy of frozen preparation evidence. Only typed reference
//! fields are remapped; native IDs, choices, values and baseline heads are fixed.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    model::{Counts, Family},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

async fn mapped(
    conn: &mut PgConnection,
    claim: &Claim,
    table: &str,
    id: Uuid,
) -> Result<Uuid, MigrationError> {
    // Shared source/cohort evidence can come from another earlier attempt in
    // the same frozen lineage when only some families were confirmed. Resolve
    // these by their complete immutable identity, never by a mutable value.
    let sql = match table {
        "source" => "SELECT c.id FROM migration_family_refresh_source c JOIN migration_family_refresh_source original ON original.id=$3 AND original.organization_id=c.organization_id WHERE c.bundle_id=$1 AND c.organization_id=$2 AND c.capture_id=original.capture_id AND c.ordinal=original.ordinal AND c.kind=original.kind AND to_jsonb(c)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext'] = to_jsonb(original)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext'] LIMIT 2".to_owned(),
        "cohort" => "SELECT c.id FROM migration_family_refresh_cohort c JOIN migration_family_refresh_cohort original ON original.id=$3 AND original.organization_id=c.organization_id WHERE c.bundle_id=$1 AND c.organization_id=$2 AND c.person_id=original.person_id AND to_jsonb(c)-ARRAY['id','bundle_id','remainder_source_id'] = to_jsonb(original)-ARRAY['id','bundle_id','remainder_source_id'] LIMIT 2".to_owned(),
        // All callers pass a compile-time table suffix, never a retained value.
        _ => format!("SELECT id FROM migration_family_refresh_{table} WHERE bundle_id=$1 AND organization_id=$2 AND remainder_source_id=$3 LIMIT 2"),
    };
    let rows: Vec<Uuid> = sqlx::query_scalar(&sql)
        .bind(claim.bundle)
        .bind(claim.organization.0)
        .bind(id)
        .fetch_all(conn)
        .await?;
    if rows.len() != 1 {
        return Err(MigrationError::Crypto);
    }
    Ok(rows[0])
}
async fn mapping(
    conn: &mut PgConnection,
    claim: &Claim,
    m: &mut super::mapping_inventory::Mapping,
) -> Result<(), MigrationError> {
    if let Some(reference) = &mut m.source {
        reference.row = mapped(conn, claim, "source", reference.row).await?;
    }
    Ok(())
}
async fn remap(
    conn: &mut PgConnection,
    claim: &Claim,
    kind: &str,
    value: Value,
) -> Result<Value, MigrationError> {
    match kind {
        "mapping" => {
            let mut m: super::mapping_inventory::Mapping =
                serde_json::from_value(value).map_err(|_| MigrationError::Crypto)?;
            mapping(conn, claim, &mut m).await?;
            serde_json::to_value(m).map_err(|_| MigrationError::Crypto)
        }
        "metadata" => {
            let mut e: super::metadata_plan::Evidence =
                serde_json::from_value(value).map_err(|_| MigrationError::Crypto)?;
            if let Some(id) = e.source_row {
                e.source_row = Some(mapped(conn, claim, "source", id).await?);
            }
            let super::metadata_plan::Decision::Ready { source, .. } = &mut e.decision else {
                return Err(MigrationError::Crypto);
            };
            source.source_row = mapped(conn, claim, "source", source.source_row).await?;
            let mut units = std::collections::BTreeSet::new();
            for id in &source.catalog_units {
                units.insert(mapped(conn, claim, "manifest", *id).await?);
            }
            source.catalog_units = units;
            serde_json::to_value(e).map_err(|_| MigrationError::Crypto)
        }
        "note" | "task" => {
            let mut e: super::activity_plan::Evidence =
                serde_json::from_value(value).map_err(|_| MigrationError::Crypto)?;
            e.source_row = mapped(conn, claim, "source", e.source_row).await?;
            let super::activity_plan::Decision::Ready { source, .. } = &mut e.decision else {
                return Err(MigrationError::Crypto);
            };
            source.source_row = mapped(conn, claim, "source", source.source_row).await?;
            for m in &mut source.mappings {
                mapping(conn, claim, m).await?;
            }
            serde_json::to_value(e).map_err(|_| MigrationError::Crypto)
        }
        "catalog" => {
            let mut e: super::catalog_plan::Evidence =
                serde_json::from_value(value).map_err(|_| MigrationError::Crypto)?;
            e.mapping_id = mapped(conn, claim, "mapping", e.mapping_id).await?;
            e.source_row = mapped(conn, claim, "source", e.source_row).await?;
            let super::catalog_plan::Decision::Ready { evidence } = &mut e.decision else {
                return Err(MigrationError::Crypto);
            };
            evidence.mapping_id = mapped(conn, claim, "mapping", evidence.mapping_id).await?;
            mapping(conn, claim, &mut evidence.mapping).await?;
            if let Some(parent) = &mut evidence.parent {
                mapping(conn, claim, parent).await?;
            }
            serde_json::to_value(e).map_err(|_| MigrationError::Crypto)
        }
        _ => Ok(value),
    }
}
fn counts(v: Value) -> Result<Counts, MigrationError> {
    if v == json!({}) {
        Ok(Counts::default())
    } else {
        serde_json::from_value(v).map_err(|_| MigrationError::Crypto)
    }
}

pub(super) async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<bool, MigrationError> {
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<bool, _>("remainder_complete")
        || p.get::<Option<Uuid>, _>("remainder_source_plan_id")
            .is_none()
    {
        return Err(MigrationError::Conflict);
    }
    let stage = p.get::<i16, _>("remainder_stage");
    let next:Option<Uuid>=sqlx::query_scalar("SELECT crm_family_refresh_remainder_next(p) FROM migration_family_refresh_plan p WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let Some(old_id) = next else {
        let Some(reservation) = preparation::reserve(&mut tx, claim, policy, 8192).await? else {
            return Ok(false);
        };
        sqlx::query("UPDATE migration_family_refresh_plan SET remainder_stage=remainder_stage+1,remainder_after=NULL,remainder_complete=(remainder_stage=7),source_walk_complete=(remainder_stage=7),owned_walk_complete=(remainder_stage=7),mappings_complete=(remainder_stage=7),catalog_walk_complete=(remainder_stage=7),apply_position=CASE WHEN remainder_stage=7 THEN inherited_position ELSE apply_position END,phase=CASE WHEN remainder_stage=7 THEN 'classify' ELSE phase END WHERE id=$1 AND organization_id=$2")
            .bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        settle(&mut tx, claim, reservation).await?;
        tx.commit().await?;
        return Ok(true);
    };
    let table = match stage {
        0 => "cohort",
        1 => "core_page",
        2 => "history_page",
        3 => "source",
        4 | 5 => "mapping",
        6 | 7 => "manifest",
        _ => return Err(MigrationError::Crypto),
    };
    let sql=format!("SELECT to_jsonb(r) AS body FROM migration_family_refresh_{table} r WHERE id=$1 AND organization_id=$2");
    let mut body: Value = sqlx::query_scalar(&sql)
        .bind(old_id)
        .bind(claim.organization.0)
        .fetch_one(&mut *tx)
        .await?;
    let id = Uuid::new_v4();
    let old_body = body.clone();
    body["id"] = json!(id);
    body["bundle_id"] = json!(claim.bundle);
    body["remainder_source_id"] = json!(old_id);
    if table != "cohort" {
        let old_plan: Uuid =
            serde_json::from_value(body["plan_id"].clone()).map_err(|_| MigrationError::Crypto)?;
        let old=sqlx::query("SELECT bundle_id,family,revision FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2").bind(old_plan).bind(claim.organization.0).fetch_one(&mut *tx).await?;
        let old_scope = Scope {
            organization: claim.organization,
            bundle: old.get("bundle_id"),
            plan: old_plan,
            family: serde_json::from_value(json!(old.get::<String, _>("family")))
                .map_err(|_| MigrationError::Crypto)?,
            revision: old.get("revision"),
        };
        let purpose = match table {
            "core_page" | "history_page" => Purpose::Binding,
            "source" => Purpose::Source,
            "mapping" => Purpose::Mapping,
            _ => Purpose::Manifest,
        };
        // Decode bytea in PostgreSQL rather than accepting a JSON encoding guess.
        let sql=format!("SELECT nonce,ciphertext FROM migration_family_refresh_{table} WHERE id=$1 AND organization_id=$2");
        let encrypted = sqlx::query(&sql)
            .bind(old_id)
            .bind(claim.organization.0)
            .fetch_one(&mut *tx)
            .await?;
        let value: Value = old_scope.open(
            key,
            old_id,
            purpose,
            encrypted.get("nonce"),
            encrypted.get("ciphertext"),
        )?;
        let kind = if table == "manifest" {
            body["kind"].as_str().ok_or(MigrationError::Crypto)?
        } else {
            table
        };
        let value = remap(&mut tx, claim, kind, value).await?;
        let scope = Scope {
            organization: claim.organization,
            bundle: claim.bundle,
            plan: claim.plan,
            family: serde_json::from_value::<Family>(json!(p.get::<String, _>("family")))
                .map_err(|_| MigrationError::Crypto)?,
            revision: p.get("revision"),
        };
        let encrypted = scope.seal(key, id, purpose, &value)?;
        let hex = |bytes: &[u8]| -> String {
            let mut out = String::from("\\x");
            for b in bytes {
                use std::fmt::Write;
                write!(out, "{b:02x}").expect("string write");
            }
            out
        };
        body["nonce"] = json!(hex(&encrypted.nonce));
        body["ciphertext"] = json!(hex(&encrypted.ciphertext));
        body["plan_id"] = json!(claim.plan);
    }
    for (field, target) in [
        ("cohort_id", "cohort"),
        ("source_row_id", "source"),
        ("mapping_id", "mapping"),
        ("parent_id", "mapping"),
        ("core_page_id", "core_page"),
        ("history_page_id", "history_page"),
    ] {
        if let Some(raw) = body.get(field).filter(|v| !v.is_null()) {
            let old: Uuid =
                serde_json::from_value(raw.clone()).map_err(|_| MigrationError::Crypto)?;
            body[field] = json!(mapped(&mut tx, claim, target, old).await?);
        }
    }
    let mut totals = counts(p.get("counts"))?;
    let mut position = p.get::<i64, _>("position");
    let mut inherited = p.get::<i64, _>("inherited_position");
    if table == "manifest" {
        position = position.checked_add(1).ok_or(MigrationError::Crypto)?;
        body["position"] = json!(position);
        if stage == 6 {
            let result: Uuid = if let Some(raw) =
                old_body.get("inherited_result_id").filter(|v| !v.is_null())
            {
                serde_json::from_value(raw.clone()).map_err(|_| MigrationError::Crypto)?
            } else {
                sqlx::query_scalar("SELECT id FROM migration_family_refresh_result WHERE manifest_id=$1 AND organization_id=$2 AND disposition IN ('applied','already_current','held','excluded')").bind(old_id).bind(claim.organization.0).fetch_one(&mut *tx).await?
            };
            body["inherited_result_id"] = json!(result);
            body["counts"] =
                serde_json::to_value(Counts::default()).map_err(|_| MigrationError::Crypto)?;
            inherited += 1;
        }
        totals = totals
            .checked_add(&counts(body["counts"].clone())?)
            .ok_or(MigrationError::Crypto)?;
    }
    let bytes: i64 = sqlx::query_scalar("SELECT crm_family_refresh_retained_size($1)")
        .bind(&body)
        .fetch_one(&mut *tx)
        .await?;
    let Some(reservation) = preparation::reserve(
        &mut tx,
        claim,
        policy,
        bytes
            .checked_add(16384)
            .ok_or(MigrationError::StorageLimit)?,
    )
    .await?
    else {
        return Ok(false);
    };
    let sql=format!("INSERT INTO migration_family_refresh_{table} SELECT (jsonb_populate_record(NULL::migration_family_refresh_{table},$1)).*");
    sqlx::query(&sql).bind(body).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET remainder_after=$3,position=$4,counts=$5,inherited_position=$6 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(old_id).bind(position).bind(if table=="manifest" {serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?} else {p.get::<Value,_>("counts")}).bind(inherited).execute(&mut *tx).await?;
    settle(&mut tx, claim, reservation).await?;
    tx.commit().await?;
    Ok(true)
}
async fn settle(conn: &mut PgConnection, claim: &Claim, token: Uuid) -> Result<(), MigrationError> {
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(token)
        .bind(claim.epoch)
        .execute(conn)
        .await?;
    Ok(())
}

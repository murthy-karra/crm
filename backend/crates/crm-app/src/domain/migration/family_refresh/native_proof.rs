//! Immutable exact-row recipes. PostgreSQL canonicalizes typed timestamps and
//! numeric values before hashing; execution uses closed table-specific writes.
use super::{
    activity_plan, catalog_plan,
    cohort::Claim,
    evidence::{Purpose, Scope},
    metadata_delta::Change,
    metadata_plan,
    model::Family,
};
use crate::{
    config::RawPayloadKey,
    domain::{custom_field::CustomFieldValue, migration::MigrationError},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, Row};
use uuid::Uuid;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Tag,
    Field,
    Option,
    PersonTag,
    PersonValue,
    Note,
    Task,
}
impl Table {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Field => "custom_field",
            Self::Option => "custom_field_option",
            Self::PersonTag => "person_tag",
            Self::PersonValue => "person_custom_field_value",
            Self::Note => "note",
            Self::Task => "task",
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Recipe {
    pub table: Table,
    pub target: Uuid,
    pub expected_revision: Option<i64>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}
impl Recipe {
    pub fn operation(&self) -> &'static str {
        match (&self.before, &self.after) {
            (None, Some(_)) => "INSERT",
            (Some(_), Some(_)) => "UPDATE",
            (Some(_), None) => "DELETE",
            _ => unreachable!("recipe has a before or after"),
        }
    }
}
async fn canonical(
    conn: &mut PgConnection,
    table: Table,
    value: Value,
) -> Result<Value, MigrationError> {
    let projection = if matches!(table, Table::PersonValue) {
        "to_jsonb(r)||jsonb_build_object('number_value',r.number_value::text)"
    } else {
        "to_jsonb(r)"
    };
    let sql = format!(
        "SELECT {projection} FROM jsonb_populate_record(NULL::{},$1) r",
        table.name()
    );
    Ok(sqlx::query_scalar(&sql).bind(value).fetch_one(conn).await?)
}
async fn recipe(
    conn: &mut PgConnection,
    table: Table,
    target: Uuid,
    revision: Option<i64>,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<Recipe, MigrationError> {
    let before = match before {
        Some(v) => Some(canonical(conn, table, v).await?),
        None => None,
    };
    let after = match after {
        Some(v) => Some(canonical(conn, table, v).await?),
        None => None,
    };
    Ok(Recipe {
        table,
        target,
        expected_revision: revision,
        before,
        after,
    })
}

pub(super) async fn draft(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    bundle: &PgRow,
    plan: &PgRow,
    unit: &PgRow,
) -> Result<Vec<Recipe>, MigrationError> {
    if !matches!(
        unit.get::<String, _>("disposition").as_str(),
        "insert" | "update"
    ) {
        return Ok(vec![]);
    }
    let family: Family = serde_json::from_value(json!(plan.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family,
        revision: plan.get("revision"),
    };
    let id: Uuid = unit.get("id");
    let actor: Uuid = bundle.get("executor_user_id");
    let timestamp: chrono::DateTime<chrono::Utc> = plan.get("created_at");
    let mut recipes = Vec::new();
    match unit.get::<String, _>("kind").as_str() {
        "note" | "task" => {
            let evidence: activity_plan::Evidence = scope.open(
                key,
                id,
                Purpose::Manifest,
                unit.get("nonce"),
                unit.get("ciphertext"),
            )?;
            let activity_plan::Decision::Ready {
                baseline, native, ..
            } = evidence.decision
            else {
                return Err(MigrationError::Crypto);
            };
            let table = if unit.get::<String, _>("kind") == "note" {
                Table::Note
            } else {
                Table::Task
            };
            recipes.push(
                recipe(
                    conn,
                    table,
                    native.target,
                    baseline.as_ref().map(|b| b.revision),
                    baseline.map(|b| b.native),
                    Some(native.after),
                )
                .await?,
            );
        }
        "metadata" => {
            let evidence: metadata_plan::Evidence = scope.open(
                key,
                id,
                Purpose::Manifest,
                unit.get("nonce"),
                unit.get("ciphertext"),
            )?;
            let metadata_plan::Decision::Ready { native, .. } = evidence.decision else {
                return Err(MigrationError::Crypto);
            };
            let revision: Option<i64> = sqlx::query_scalar(
                "SELECT metadata_revision FROM person WHERE id=$1 AND organization_id=$2 FOR SHARE",
            )
            .bind(native.person)
            .bind(claim.organization.0)
            .fetch_optional(&mut *conn)
            .await?;
            // A stale proposal will become a held execution result. Never borrow
            // a newly changed native row to fabricate an approved before-state.
            if revision != Some(native.expected_revision) {
                return Ok(vec![]);
            }
            for (offset, change) in native.changes.iter().enumerate() {
                let revision = native
                    .expected_revision
                    .checked_add(i64::try_from(offset).map_err(|_| MigrationError::Crypto)?)
                    .ok_or(MigrationError::Crypto)?;
                let (table, target, delete) = match change {
                    Change::AddTag(id) => (Table::PersonTag, *id, false),
                    Change::RemoveTag(id) => (Table::PersonTag, *id, true),
                    Change::SetField(id, _) => (Table::PersonValue, *id, false),
                    Change::ClearField(id) => (Table::PersonValue, *id, true),
                };
                let column = match table {
                    Table::PersonTag => "tag_id",
                    Table::PersonValue => "field_id",
                    _ => unreachable!(),
                };
                let projection = if matches!(table, Table::PersonValue) {
                    "to_jsonb(x)||jsonb_build_object('number_value',x.number_value::text)"
                } else {
                    "to_jsonb(x)"
                };
                let sql=format!("SELECT {projection} FROM {} x WHERE organization_id=$1 AND person_id=$2 AND {column}=$3",table.name());
                let before: Option<Value> = sqlx::query_scalar(&sql)
                    .bind(claim.organization.0)
                    .bind(native.person)
                    .bind(target)
                    .fetch_optional(&mut *conn)
                    .await?;
                let after = if delete {
                    None
                } else {
                    Some(match change {
                        Change::AddTag(_) => {
                            json!({"organization_id":claim.organization.0,"person_id":native.person,"tag_id":target,"added_by_user_id":actor,"created_at":timestamp})
                        }
                        Change::SetField(_, value) => {
                            let mut row=before.clone().unwrap_or_else(||json!({"organization_id":claim.organization.0,"person_id":native.person,"field_id":target,"field_type":value.field_type().as_str(),"created_at":timestamp}));
                            for name in ["text_value", "number_value", "date_value", "option_id"] {
                                row[name] = Value::Null;
                            }
                            match value {
                                CustomFieldValue::Text(v) => row["text_value"] = json!(v),
                                CustomFieldValue::Number(v) => row["number_value"] = json!(v),
                                CustomFieldValue::Date(v) => row["date_value"] = json!(v),
                                CustomFieldValue::Choice(v) => row["option_id"] = json!(v),
                            };
                            row["updated_by_user_id"] = json!(actor);
                            row["updated_at"] = json!(timestamp);
                            row["origin"] = json!("migration");
                            row["correlation_id"] = json!(claim.bundle);
                            row
                        }
                        _ => return Err(MigrationError::Crypto),
                    })
                };
                if delete && before.is_none() {
                    return Err(MigrationError::Conflict);
                }
                recipes.push(recipe(conn, table, target, Some(revision), before, after).await?);
            }
        }
        "catalog" => {
            let evidence: catalog_plan::Evidence = scope.open(
                key,
                id,
                Purpose::Manifest,
                unit.get("nonce"),
                unit.get("ciphertext"),
            )?;
            let catalog_plan::Decision::Ready { evidence } = evidence.decision else {
                return Err(MigrationError::Crypto);
            };
            let target = evidence.target;
            let label = evidence
                .mapping
                .label
                .as_ref()
                .ok_or(MigrationError::Crypto)?;
            let (table, value) = match evidence.mapping.kind.as_str() {
                "tag" => (
                    Table::Tag,
                    json!({"id":target,"organization_id":claim.organization.0,"name":label,"created_by_user_id":actor,"created_at":timestamp,"updated_at":timestamp}),
                ),
                "field" => {
                    let max:i32=sqlx::query_scalar("SELECT COALESCE(max(position),0) FROM custom_field WHERE organization_id=$1").bind(claim.organization.0).fetch_one(&mut *conn).await?;
                    let position = max
                        .checked_add(
                            i32::try_from(unit.get::<i64, _>("position"))
                                .map_err(|_| MigrationError::Crypto)?,
                        )
                        .ok_or(MigrationError::Crypto)?;
                    (
                        Table::Field,
                        json!({"id":target,"organization_id":claim.organization.0,"label":label,"field_type":evidence.frozen.field_type,"position":position,"archived_at":null,"created_by_user_id":actor,"source":"fub","external_key":evidence.frozen.machine_name,"created_at":timestamp,"updated_at":timestamp}),
                    )
                }
                "option" => {
                    let parent = evidence.field.ok_or(MigrationError::Crypto)?;
                    let max:i32=sqlx::query_scalar("SELECT COALESCE(max(position),0) FROM custom_field_option WHERE organization_id=$1 AND field_id=$2").bind(claim.organization.0).bind(parent).fetch_one(&mut *conn).await?;
                    let ordinal = evidence
                        .mapping
                        .source
                        .as_ref()
                        .ok_or(MigrationError::Crypto)?
                        .element;
                    let position = max
                        .checked_add(i32::try_from(ordinal).map_err(|_| MigrationError::Crypto)?)
                        .ok_or(MigrationError::Crypto)?;
                    (
                        Table::Option,
                        json!({"id":target,"organization_id":claim.organization.0,"field_id":parent,"label":label,"position":position,"archived_at":null,"created_at":timestamp,"updated_at":timestamp}),
                    )
                }
                _ => return Err(MigrationError::Crypto),
            };
            recipes.push(recipe(conn, table, target, None, None, Some(value)).await?);
        }
        // History's append-only typed fact/version proof has its own guard.
        "event" | "call" | "text" => {}
        _ => return Err(MigrationError::Crypto),
    }
    Ok(recipes)
}

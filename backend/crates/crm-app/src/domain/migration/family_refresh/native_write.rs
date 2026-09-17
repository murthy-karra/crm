//! Closed native SQL driven only by authenticated, sealed row recipes.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    native_proof::{Recipe, Table},
};
use crate::{config::RawPayloadKey, domain::migration::MigrationError};
use sqlx::{PgConnection, Row};
use uuid::Uuid;
pub(super) struct Proven {
    pub id: Uuid,
    pub recipe: Recipe,
}
pub(super) async fn load(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    unit: Uuid,
) -> Result<Vec<Proven>, MigrationError> {
    let rows=sqlx::query("SELECT * FROM migration_family_refresh_write_proof WHERE manifest_id=$1 AND plan_id=$2 AND bundle_id=$3 AND organization_id=$4 ORDER BY expected_revision NULLS FIRST,id LIMIT 501").bind(unit).bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).fetch_all(&mut *conn).await?;
    if rows.len() > 500 {
        return Err(MigrationError::Crypto);
    }
    let mut output = Vec::new();
    for r in rows {
        let recipe: Recipe = scope.open(
            key,
            r.get("id"),
            Purpose::NativeProof,
            &r.get::<Option<Vec<u8>>, _>("nonce")
                .ok_or(MigrationError::Crypto)?,
            &r.get::<Option<Vec<u8>>, _>("ciphertext")
                .ok_or(MigrationError::Crypto)?,
        )?;
        if recipe.before.is_none() && recipe.after.is_none() {
            return Err(MigrationError::Crypto);
        }
        if r.get::<String, _>("table_name") != recipe.table.name()
            || r.get::<String, _>("operation") != recipe.operation()
            || r.get::<Uuid, _>("target_id") != recipe.target
            || r.get::<Option<i64>, _>("expected_revision") != recipe.expected_revision
        {
            return Err(MigrationError::Crypto);
        }
        let sql=format!("SELECT (CASE WHEN $1::jsonb IS NULL THEN NULL ELSE crm_family_refresh_native_digest(to_jsonb(jsonb_populate_record(NULL::{table},$1))) END IS NOT DISTINCT FROM $2::bytea) AND (CASE WHEN $3::jsonb IS NULL THEN NULL ELSE crm_family_refresh_native_digest(to_jsonb(jsonb_populate_record(NULL::{table},$3))) END IS NOT DISTINCT FROM $4::bytea)",table=recipe.table.name());
        let valid: bool = sqlx::query_scalar(&sql)
            .bind(&recipe.before)
            .bind(r.get::<Option<Vec<u8>>, _>("before_hash"))
            .bind(&recipe.after)
            .bind(r.get::<Option<Vec<u8>>, _>("after_hash"))
            .fetch_one(&mut *conn)
            .await?;
        if !valid {
            return Err(MigrationError::Crypto);
        }
        output.push(Proven {
            id: r.get("id"),
            recipe,
        });
    }
    Ok(output)
}
fn key_column(table: Table) -> &'static str {
    match table {
        Table::PersonTag => "tag_id",
        Table::PersonValue => "field_id",
        _ => "id",
    }
}
fn person(recipe: &Recipe) -> Result<Option<Uuid>, MigrationError> {
    if matches!(recipe.table, Table::PersonTag | Table::PersonValue) {
        recipe
            .after
            .as_ref()
            .or(recipe.before.as_ref())
            .and_then(|v| v["person_id"].as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
            .map(Some)
            .ok_or(MigrationError::Crypto)
    } else {
        Ok(None)
    }
}
/// The caller holds Person/catalog locks before checking the entire unit. No
/// write occurs until every recipe's before-state has been verified.
pub(super) async fn current_matches(
    conn: &mut PgConnection,
    claim: &Claim,
    proof: &Proven,
) -> Result<bool, MigrationError> {
    let recipe = &proof.recipe;
    let predicate = if person(recipe)?.is_some() {
        " AND person_id=$3"
    } else {
        ""
    };
    // Person/tag links have no UPDATE grant. The caller already holds the
    // Person row used by metadata revision serialization; DELETE is guarded
    // against the exact sealed before-state when the recipe is applied.
    let lock = if matches!(recipe.table, Table::PersonTag) {
        ""
    } else {
        " FOR UPDATE"
    };
    let sql=format!("SELECT crm_family_refresh_native_digest(to_jsonb(n)) FROM {} n WHERE organization_id=$1 AND {}=$2{predicate}{lock}",recipe.table.name(),key_column(recipe.table));
    let query = sqlx::query_scalar::<_, Vec<u8>>(&sql)
        .bind(claim.organization.0)
        .bind(recipe.target);
    let current = if let Some(person) = person(recipe)? {
        query.bind(person).fetch_optional(&mut *conn).await?
    } else {
        query.fetch_optional(&mut *conn).await?
    };
    let sql=format!("SELECT CASE WHEN $1::jsonb IS NULL THEN NULL ELSE crm_family_refresh_native_digest(to_jsonb(jsonb_populate_record(NULL::{},$1))) END",recipe.table.name());
    let expected: Option<Vec<u8>> = sqlx::query_scalar(&sql)
        .bind(&recipe.before)
        .fetch_one(conn)
        .await?;
    Ok(current == expected)
}
pub(super) async fn apply(
    conn: &mut PgConnection,
    claim: &Claim,
    proof: &Proven,
) -> Result<(), MigrationError> {
    let recipe = &proof.recipe;
    let table = recipe.table.name();
    sqlx::query("SELECT set_config('crm.family_refresh_proof',$1,true)")
        .bind(proof.id.to_string())
        .execute(&mut *conn)
        .await?;
    let count = match (&recipe.before, &recipe.after) {
        (None, Some(after)) => {
            let sql =
                format!("INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},$1)).*");
            sqlx::query(&sql)
                .bind(after)
                .execute(&mut *conn)
                .await?
                .rows_affected()
        }
        (Some(_), Some(after)) => {
            let columns=match recipe.table {
                Table::Note=>"body,author_user_id,updated_at,correlation_id",
                Table::Task=>"title,kind,due_at,assignee_user_id,created_by_user_id,completed_at,completed_by_user_id,updated_at,correlation_id",
                Table::PersonValue=>"text_value,number_value,date_value,option_id,updated_by_user_id,updated_at,origin,correlation_id",
                _=>return Err(MigrationError::Crypto),
            };
            let select = columns
                .split(',')
                .map(|c| format!("r.{c}"))
                .collect::<Vec<_>>()
                .join(",");
            let predicate = if person(recipe)?.is_some() {
                " AND n.person_id=$4"
            } else {
                ""
            };
            let sql=format!("UPDATE {table} n SET ({columns})=({select}) FROM jsonb_populate_record(NULL::{table},$1) r WHERE n.organization_id=$2 AND n.{}=$3{predicate}",key_column(recipe.table));
            let query = sqlx::query(&sql)
                .bind(after)
                .bind(claim.organization.0)
                .bind(recipe.target);
            if let Some(person) = person(recipe)? {
                query
                    .bind(person)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected()
            } else {
                query.execute(&mut *conn).await?.rows_affected()
            }
        }
        (Some(_), None) if matches!(recipe.table, Table::PersonTag | Table::PersonValue) => {
            let sql = format!(
                "DELETE FROM {table} WHERE organization_id=$1 AND {}=$2 AND person_id=$3",
                key_column(recipe.table)
            );
            sqlx::query(&sql)
                .bind(claim.organization.0)
                .bind(recipe.target)
                .bind(person(recipe)?)
                .execute(&mut *conn)
                .await?
                .rows_affected()
        }
        _ => return Err(MigrationError::Crypto),
    };
    sqlx::query("SELECT set_config('crm.family_refresh_proof','',true)")
        .execute(conn)
        .await?;
    if count != 1 {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}

pub(super) async fn growth(conn: &mut PgConnection, proof: &Proven) -> Result<i64, MigrationError> {
    let sql=format!("SELECT GREATEST(CASE WHEN $1::jsonb IS NULL THEN 0 ELSE octet_length(to_jsonb(jsonb_populate_record(NULL::{table},$1))::text) END - CASE WHEN $2::jsonb IS NULL THEN 0 ELSE octet_length(to_jsonb(jsonb_populate_record(NULL::{table},$2))::text) END,0)::bigint",table=proof.recipe.table.name());
    Ok(sqlx::query_scalar(&sql)
        .bind(&proof.recipe.after)
        .bind(&proof.recipe.before)
        .fetch_one(conn)
        .await?)
}

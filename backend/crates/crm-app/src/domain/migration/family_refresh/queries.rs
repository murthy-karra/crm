//! Small workspace-scoped summaries for the common refresh workflow. These
//! projections never fetch encrypted source, manifest, mapping or result bodies.
use super::model::{Counts, Family, ENGINE, PAGE_MAX, RESPONSE_BYTES};
use crate::{
    auth::workspace,
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{snapshot, store, MigrationError},
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePage {
    pub parent_import_id: Uuid,
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}
#[derive(Serialize)]
pub struct BundleSummary {
    pub id: Uuid,
    pub parent_import_id: Uuid,
    pub revision: String,
    pub state: String,
    pub core_report_id: Option<Uuid>,
    pub history_capture_id: Option<Uuid>,
    pub predecessor_id: Option<Uuid>,
    pub digest: Option<String>,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}
#[derive(Serialize)]
pub struct Bundles {
    pub items: Vec<BundleSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize)]
pub struct FamilySummary {
    pub plan_id: Uuid,
    pub family: Family,
    pub revision: String,
    pub state: String,
    pub phase: String,
    pub pause_reason: Option<String>,
    pub digest: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub counts: Counts,
}
#[derive(Serialize)]
pub struct Families {
    pub bundle_id: Uuid,
    pub bundle_revision: String,
    /// Exactly the latest selected plan per family, never the revision history.
    pub items: Vec<FamilySummary>,
}
#[derive(Serialize)]
pub struct Detail {
    pub bundle: BundleSummary,
    pub families: Vec<FamilySummary>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    upper: DateTime<Utc>,
    after: Option<(DateTime<Utc>, Uuid)>,
}
const BUNDLE_COLUMNS: &str="b.id,b.parent_import_id,b.revision,b.state,b.core_report_id,b.history_capture_id,b.predecessor_id,b.digest,b.created_at,b.confirmed_at";
pub(super) async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true)")
        .bind(ENGINE)
        .execute(&mut *tx)
        .await?;
    store::require_admin(&mut tx, ctx).await?;
    Ok(tx)
}
pub(super) fn bounded<T: Serialize>(value: &T, limit: usize) -> Result<(), MigrationError> {
    if serde_json::to_vec(value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > limit
    {
        return Err(MigrationError::Crypto);
    }
    Ok(())
}
fn bundle(row: &PgRow) -> Result<BundleSummary, MigrationError> {
    let summary = BundleSummary {
        id: row.get("id"),
        parent_import_id: row.get("parent_import_id"),
        revision: row.get::<i64, _>("revision").to_string(),
        state: row.get("state"),
        core_report_id: row.get("core_report_id"),
        history_capture_id: row.get("history_capture_id"),
        predecessor_id: row.get("predecessor_id"),
        digest: row
            .get::<Option<Vec<u8>>, _>("digest")
            .map(|bytes| crate::domain::migration::imports::hex(&bytes)),
        created_at: row.get("created_at"),
        confirmed_at: row.get("confirmed_at"),
    };
    bounded(&summary, 4096)?;
    Ok(summary)
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,parent_import_id=%q.parent_import_id))]
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: BundlePage,
) -> Result<Bundles, MigrationError> {
    let limit = q.limit.unwrap_or(25);
    if limit == 0 || limit > PAGE_MAX {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    let workspace=sqlx::query("SELECT w.plan_id,o.workspace_revision FROM migration_workspace w JOIN organization o ON o.id=w.organization_id WHERE w.organization_id=$1 AND w.import_id=$2")
        .bind(ctx.organization_id.0).bind(q.parent_import_id).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let scope=serde_json::to_string(&serde_json::json!({"engine":ENGINE,"endpoint":"bundles","actor":ctx.actor_user_id.0,"organization":ctx.organization_id.0,"parent":q.parent_import_id,"workspace":workspace.get::<Uuid,_>("plan_id"),"workspace_revision":workspace.get::<i64,_>("workspace_revision"),"limit":limit,"order":"created_id_desc"})).map_err(|_|MigrationError::Crypto)?;
    let position: Position = match snapshot::decode_cursor(
        key,
        ctx.organization_id,
        q.parent_import_id,
        &scope,
        q.cursor.as_deref(),
    )? {
        Some(v) => serde_json::from_value(v).map_err(|_| MigrationError::InvalidInput)?,
        None => Position {
            upper: sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(&mut *tx)
                .await?,
            after: None,
        },
    };
    if position
        .after
        .is_some_and(|(created, _)| created > position.upper)
    {
        return Err(MigrationError::InvalidInput);
    }
    let sql=format!("SELECT {BUNDLE_COLUMNS} FROM migration_family_refresh_bundle b WHERE b.organization_id=$1 AND b.parent_import_id=$2 AND b.parent_plan_id=$3 AND b.created_at<=$4 AND ($5::timestamptz IS NULL OR (b.created_at,b.id)<($5,$6)) ORDER BY b.created_at DESC,b.id DESC LIMIT $7");
    let rows = sqlx::query(&sql)
        .bind(ctx.organization_id.0)
        .bind(q.parent_import_id)
        .bind(workspace.get::<Uuid, _>("plan_id"))
        .bind(position.upper)
        .bind(position.after.map(|x| x.0))
        .bind(position.after.map(|x| x.1))
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await?;
    let items = rows
        .iter()
        .take(usize::from(limit))
        .map(bundle)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if rows.len() > items.len() {
        let last = items.last().ok_or(MigrationError::Crypto)?;
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            q.parent_import_id,
            &scope,
            &serde_json::to_value(Position {
                upper: position.upper,
                after: Some((last.created_at, last.id)),
            })
            .map_err(|_| MigrationError::Crypto)?,
        )?)
    } else {
        None
    };
    let result = Bundles { items, next_cursor };
    bounded(&result, RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(result)
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%id))]
pub async fn detail(
    pool: &PgPool,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Detail, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let sql=format!("SELECT {BUNDLE_COLUMNS} FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF b");
    let row = sqlx::query(&sql)
        .bind(id)
        .bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::NotFound)?;
    let bundle = bundle(&row)?;
    // The partial unique index permits at most one non-superseded plan per
    // selected family. Lock in UUID order after the bundle, as preparation does.
    let rows=sqlx::query("SELECT id,family,revision,state,phase,pause_reason,digest,expires_at,CASE WHEN octet_length(counts::text)<=4096 THEN counts END AS counts FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state<>'superseded' ORDER BY id LIMIT 4 FOR SHARE")
        .bind(id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    if rows.is_empty() || rows.len() > 3 {
        return Err(MigrationError::Crypto);
    }
    let mut families = Vec::new();
    for row in rows {
        let raw: serde_json::Value = row
            .get::<Option<_>, _>("counts")
            .ok_or(MigrationError::Crypto)?;
        let counts: Counts = if raw == serde_json::json!({}) {
            Counts::default()
        } else {
            serde_json::from_value(raw).map_err(|_| MigrationError::Crypto)?
        };
        if !counts.reconciles() {
            return Err(MigrationError::Crypto);
        }
        let summary = FamilySummary {
            plan_id: row.get("id"),
            family: serde_json::from_value(serde_json::json!(row.get::<String, _>("family")))
                .map_err(|_| MigrationError::Crypto)?,
            revision: row.get::<i64, _>("revision").to_string(),
            state: row.get("state"),
            phase: row.get("phase"),
            pause_reason: row.get("pause_reason"),
            digest: row
                .get::<Option<Vec<u8>>, _>("digest")
                .map(|bytes| crate::domain::migration::imports::hex(&bytes)),
            expires_at: row.get("expires_at"),
            counts,
        };
        bounded(&summary, 4096)?;
        families.push(summary);
    }
    families.sort_by_key(|f| f.family);
    let result = Detail { bundle, families };
    bounded(&result, RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(result)
}
pub async fn families(
    pool: &PgPool,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Families, MigrationError> {
    let detail = detail(pool, ctx, id).await?;
    Ok(Families {
        bundle_id: detail.bundle.id,
        bundle_revision: detail.bundle.revision,
        items: detail.families,
    })
}

//! Bounded manifest summaries. Full encrypted proof/field review is a separate
//! reader; this projection never loads a large manifest ciphertext.
use super::model::{Counts, Family, ENGINE, PAGE_MAX, RESPONSE_BYTES};
use crate::{
    auth::workspace,
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{snapshot, store, MigrationError},
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Insert,
    Update,
    AlreadyCurrent,
    Correction,
    Held,
    Excluded,
}
impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::AlreadyCurrent => "already_current",
            Self::Correction => "correction",
            Self::Held => "held",
            Self::Excluded => "excluded",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPage {
    pub family: Family,
    pub plan_id: Uuid,
    pub cohort_id: Option<Uuid>,
    pub outcome: Option<Outcome>,
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Catalog,
    Metadata,
    Note,
    Task,
    Event,
    Call,
    Text,
}
#[derive(Serialize)]
pub struct ItemSummary {
    pub id: Uuid,
    pub position: String,
    pub kind: ItemKind,
    pub cohort_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub target_id: Option<Uuid>,
    pub outcome: Outcome,
    pub reason: Option<String>,
    pub counts: Counts,
}
#[derive(Serialize)]
pub struct Items {
    pub bundle_id: Uuid,
    pub bundle_revision: String,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub family: Family,
    pub items: Vec<ItemSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    after: i64,
    upper: i64,
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%bundle,plan_id=%q.plan_id))]
pub async fn items(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    q: ItemPage,
) -> Result<Items, MigrationError> {
    let limit = q.limit.unwrap_or(25);
    if limit == 0 || limit > PAGE_MAX {
        return Err(MigrationError::InvalidInput);
    }
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
    // A shared plan lock keeps revision/position stable for this bounded read.
    // Native evidence remains behind its separately authorized field reader.
    let row=sqlx::query("SELECT b.revision AS bundle_revision,b.parent_import_id,b.parent_plan_id,o.workspace_revision FROM migration_family_refresh_bundle b JOIN organization o ON o.id=b.organization_id JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF b")
        .bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let plan=sqlx::query("SELECT revision AS plan_revision,position FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND id=$3 AND family=$4 FOR SHARE")
        .bind(bundle).bind(ctx.organization_id.0).bind(q.plan_id).bind(q.family.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(cohort) = q.cohort_id {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3)")
            .bind(cohort).bind(bundle).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        if !exists {
            return Err(MigrationError::NotFound);
        }
    }
    // Every cursor authority dimension is authenticated as AEAD associated data.
    // Contents are positions only, with a fixed upper bound across growing plans.
    let scope=serde_json::to_string(&serde_json::json!({
        "engine":ENGINE,"actor":ctx.actor_user_id.0,"organization":ctx.organization_id.0,
        "parent":row.get::<Uuid,_>("parent_import_id"),"workspace":row.get::<Uuid,_>("parent_plan_id"),
        "workspace_revision":row.get::<i64,_>("workspace_revision"),
        "bundle":bundle,"bundle_revision":row.get::<i64,_>("bundle_revision"),
        "plan":q.plan_id,"revision":plan.get::<i64,_>("plan_revision"),"family":q.family,
        "cohort":q.cohort_id,"outcome":q.outcome,"endpoint":"items","limit":limit,"order":"position_asc"
    })).map_err(|_|MigrationError::Crypto)?;
    let position: Position = match snapshot::decode_cursor(
        key,
        ctx.organization_id,
        bundle,
        &scope,
        q.cursor.as_deref(),
    )? {
        Some(v) => serde_json::from_value(v).map_err(|_| MigrationError::InvalidInput)?,
        None => Position {
            after: 0,
            upper: plan.get("position"),
        },
    };
    if position.after < 0
        || position.upper < position.after
        || position.upper > plan.get::<i64, _>("position")
    {
        return Err(MigrationError::InvalidInput);
    }
    let raw=sqlx::query("SELECT id,position,kind,cohort_id,person_id,target_id,disposition,reason,CASE WHEN octet_length(counts::text)<=4096 THEN counts END AS counts FROM migration_family_refresh_manifest WHERE bundle_id=$1 AND plan_id=$2 AND organization_id=$3 AND position>$4 AND position<=$5 AND ($6::uuid IS NULL OR cohort_id=$6) AND ($7::text IS NULL OR disposition=$7) ORDER BY position LIMIT $8")
        .bind(bundle).bind(q.plan_id).bind(ctx.organization_id.0).bind(position.after).bind(position.upper)
        .bind(q.cohort_id).bind(q.outcome.map(Outcome::as_str)).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
    let mut result = Items {
        bundle_id: bundle,
        bundle_revision: row.get::<i64, _>("bundle_revision").to_string(),
        plan_id: q.plan_id,
        plan_revision: plan.get::<i64, _>("plan_revision").to_string(),
        family: q.family,
        items: Vec::new(),
        next_cursor: None,
    };
    let mut after = position.after;
    for r in raw.iter().take(usize::from(limit)) {
        let counts: Counts = serde_json::from_value(
            r.get::<Option<serde_json::Value>, _>("counts")
                .ok_or(MigrationError::Crypto)?,
        )
        .map_err(|_| MigrationError::Crypto)?;
        if !counts.reconciles() {
            return Err(MigrationError::Crypto);
        }
        let kind: ItemKind = serde_json::from_value(serde_json::json!(r.get::<String, _>("kind")))
            .map_err(|_| MigrationError::Crypto)?;
        let item = ItemSummary {
            id: r.get("id"),
            position: r.get::<i64, _>("position").to_string(),
            kind,
            cohort_id: r.get("cohort_id"),
            person_id: r.get("person_id"),
            target_id: r.get("target_id"),
            outcome: serde_json::from_value(serde_json::json!(r.get::<String, _>("disposition")))
                .map_err(|_| MigrationError::Crypto)?,
            reason: r.get("reason"),
            counts,
        };
        if serde_json::to_vec(&item)
            .map_err(|_| MigrationError::Crypto)?
            .len()
            > 4096
        {
            return Err(MigrationError::Crypto);
        }
        after = r.get("position");
        result.items.push(item);
    }
    if raw.len() > result.items.len() {
        result.next_cursor = Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            bundle,
            &scope,
            &serde_json::to_value(Position {
                after,
                upper: position.upper,
            })
            .map_err(|_| MigrationError::Crypto)?,
        )?);
    }
    if serde_json::to_vec(&result)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > RESPONSE_BYTES
    {
        return Err(MigrationError::Crypto);
    }
    tx.commit().await?;
    Ok(result)
}

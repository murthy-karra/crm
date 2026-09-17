//! Bounded committed-result summaries; encrypted result bodies are never loaded.
use super::{
    item_queries::ItemKind,
    model::{Family, ENGINE, PAGE_MAX, RESPONSE_BYTES},
    queries,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{snapshot, MigrationError},
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    AlreadyCurrent,
    Held,
    Excluded,
}
impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::AlreadyCurrent => "already_current",
            Self::Held => "held",
            Self::Excluded => "excluded",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultPage {
    pub family: Family,
    pub plan_id: Uuid,
    pub cohort_id: Option<Uuid>,
    pub outcome: Option<Outcome>,
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}
#[derive(Serialize)]
pub struct ResultSummary {
    pub id: Uuid,
    pub item_id: Uuid,
    pub position: String,
    pub kind: ItemKind,
    pub cohort_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub target_id: Option<Uuid>,
    pub outcome: Outcome,
    pub reason: Option<String>,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Serialize)]
pub struct Results {
    pub bundle_id: Uuid,
    pub bundle_revision: String,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub family: Family,
    pub items: Vec<ResultSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    after: i64,
    upper: i64,
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%bundle,plan_id=%q.plan_id))]
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    q: ResultPage,
) -> Result<Results, MigrationError> {
    let limit = q.limit.unwrap_or(25);
    if limit == 0 || limit > PAGE_MAX {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = queries::begin(pool, ctx).await?;
    let b=sqlx::query("SELECT b.revision,b.parent_import_id,b.parent_plan_id,o.workspace_revision FROM migration_family_refresh_bundle b JOIN organization o ON o.id=b.organization_id JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF b").bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let p=sqlx::query("SELECT revision,apply_position FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3 AND family=$4 FOR SHARE").bind(q.plan_id).bind(bundle).bind(ctx.organization_id.0).bind(q.family.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(cohort) = q.cohort_id {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3)").bind(cohort).bind(bundle).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        if !valid {
            return Err(MigrationError::NotFound);
        }
    }
    let scope=serde_json::to_string(&serde_json::json!({"engine":ENGINE,"actor":ctx.actor_user_id.0,"organization":ctx.organization_id.0,"parent":b.get::<Uuid,_>("parent_import_id"),"workspace":b.get::<Uuid,_>("parent_plan_id"),"workspace_revision":b.get::<i64,_>("workspace_revision"),"bundle":bundle,"bundle_revision":b.get::<i64,_>("revision"),"plan":q.plan_id,"plan_revision":p.get::<i64,_>("revision"),"family":q.family,"cohort":q.cohort_id,"outcome":q.outcome,"limit":limit,"endpoint":"results","order":"position_asc"})).map_err(|_|MigrationError::Crypto)?;
    let position: Position = match snapshot::decode_cursor(
        key,
        ctx.organization_id,
        bundle,
        &scope,
        q.cursor.as_deref(),
    )? {
        Some(value) => serde_json::from_value(value).map_err(|_| MigrationError::InvalidInput)?,
        None => Position {
            after: 0,
            upper: p.get("apply_position"),
        },
    };
    if position.after < 0
        || position.upper < position.after
        || position.upper > p.get::<i64, _>("apply_position")
    {
        return Err(MigrationError::InvalidInput);
    }
    let rows=sqlx::query("SELECT r.id,r.manifest_id,m.position,m.kind,m.cohort_id,r.person_id,r.target_id,r.disposition,r.reason,r.committed_at FROM migration_family_refresh_manifest m JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.bundle_id=m.bundle_id AND r.organization_id=m.organization_id WHERE m.bundle_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND m.position>$4 AND m.position<=$5 AND ($6::uuid IS NULL OR m.cohort_id=$6) AND ($7::text IS NULL OR r.disposition=$7) ORDER BY m.position LIMIT $8").bind(bundle).bind(q.plan_id).bind(ctx.organization_id.0).bind(position.after).bind(position.upper).bind(q.cohort_id).bind(q.outcome.map(Outcome::as_str)).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
    let mut response = Results {
        bundle_id: bundle,
        bundle_revision: b.get::<i64, _>("revision").to_string(),
        plan_id: q.plan_id,
        plan_revision: p.get::<i64, _>("revision").to_string(),
        family: q.family,
        items: Vec::new(),
        next_cursor: None,
    };
    let mut after = position.after;
    for row in rows.iter().take(usize::from(limit)) {
        let item = ResultSummary {
            id: row.get("id"),
            item_id: row.get("manifest_id"),
            position: row.get::<i64, _>("position").to_string(),
            kind: serde_json::from_value(serde_json::json!(row.get::<String, _>("kind")))
                .map_err(|_| MigrationError::Crypto)?,
            cohort_id: row.get("cohort_id"),
            person_id: row.get("person_id"),
            target_id: row.get("target_id"),
            outcome: serde_json::from_value(serde_json::json!(row.get::<String, _>("disposition")))
                .map_err(|_| MigrationError::Crypto)?,
            reason: row.get("reason"),
            committed_at: row.get("committed_at"),
        };
        queries::bounded(&item, 4096)?;
        after = row.get("position");
        response.items.push(item);
    }
    if rows.len() > response.items.len() {
        response.next_cursor = Some(snapshot::encode_cursor(
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
    queries::bounded(&response, RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(response)
}

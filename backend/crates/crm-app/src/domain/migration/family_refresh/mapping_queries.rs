//! Bounded mapping summaries; full source values remain behind exact retained
//! references. Inventory progress is cursor-bound so partial discovery cannot
//! silently change the set being paged.
use super::{
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    model::{Family, ENGINE, PAGE_MAX, RESPONSE_BYTES},
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
pub enum Kind {
    Tag,
    Field,
    Option,
    NoteAuthor,
    TaskCreator,
    TaskAssignee,
    TaskKind,
    Timezone,
}
impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Field => "field",
            Self::Option => "option",
            Self::NoteAuthor => "note_author",
            Self::TaskCreator => "task_creator",
            Self::TaskAssignee => "task_assignee",
            Self::TaskKind => "task_kind",
            Self::Timezone => "timezone",
        }
    }
    fn family(self) -> Family {
        match self {
            Self::Tag | Self::Field | Self::Option => Family::Metadata,
            _ => Family::Activity,
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Hold,
    Existing,
    CreateMatching,
    Unassigned,
    Kind,
    Timezone,
}
impl Disposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Existing => "existing",
            Self::CreateMatching => "create_matching",
            Self::Unassigned => "unassigned",
            Self::Kind => "kind",
            Self::Timezone => "timezone",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPage {
    pub family: Family,
    pub plan_id: Uuid,
    pub kind: Option<Kind>,
    pub disposition: Option<Disposition>,
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}
#[derive(Serialize)]
pub struct Summary {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub kind: Kind,
    pub label: Option<String>,
    pub value_qualified: bool,
    pub creation_allowed: bool,
    pub choice: Choice,
}
#[derive(Serialize)]
pub struct Mappings {
    pub bundle_id: Uuid,
    pub bundle_revision: String,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub family: Family,
    pub inventory_complete: bool,
    pub items: Vec<Summary>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    after: Option<Uuid>,
    upper: Option<Uuid>,
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%bundle,plan_id=%q.plan_id))]
pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    q: MappingPage,
) -> Result<Mappings, MigrationError> {
    let limit = q.limit.unwrap_or(25);
    if limit == 0 || limit > PAGE_MAX || q.kind.is_some_and(|kind| kind.family() != q.family) {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = super::queries::begin(pool, ctx).await?;
    let b=sqlx::query("SELECT b.revision,b.parent_import_id,b.parent_plan_id,o.workspace_revision FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id JOIN organization o ON o.id=b.organization_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF b")
        .bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let p=sqlx::query("SELECT revision,mapping_after,mapping_current,mapping_offset,mappings_complete FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3 AND family=$4 FOR SHARE")
        .bind(q.plan_id).bind(bundle).bind(ctx.organization_id.0).bind(q.family.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let scope=serde_json::to_string(&serde_json::json!({"engine":ENGINE,"endpoint":"mappings","actor":ctx.actor_user_id.0,"organization":ctx.organization_id.0,"parent":b.get::<Uuid,_>("parent_import_id"),"workspace":b.get::<Uuid,_>("parent_plan_id"),"workspace_revision":b.get::<i64,_>("workspace_revision"),"bundle":bundle,"bundle_revision":b.get::<i64,_>("revision"),"plan":q.plan_id,"plan_revision":p.get::<i64,_>("revision"),"family":q.family,"kind":q.kind,"disposition":q.disposition,"limit":limit,"order":"id_asc","inventory":[p.get::<Option<Uuid>,_>("mapping_after"),p.get::<Option<Uuid>,_>("mapping_current"),p.get::<i32,_>("mapping_offset"),p.get::<bool,_>("mappings_complete")]})).map_err(|_|MigrationError::Crypto)?;
    let position:Position=match snapshot::decode_cursor(key,ctx.organization_id,bundle,&scope,q.cursor.as_deref())? {
        Some(value)=>serde_json::from_value(value).map_err(|_|MigrationError::InvalidInput)?,
        None=>Position{after:None,upper:sqlx::query_scalar("SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND ($3::text IS NULL OR kind=$3) AND ($4::text IS NULL OR disposition=$4) ORDER BY id DESC LIMIT 1")
            .bind(q.plan_id).bind(ctx.organization_id.0).bind(q.kind.map(Kind::as_str)).bind(q.disposition.map(Disposition::as_str)).fetch_optional(&mut *tx).await?},
    };
    if position
        .after
        .is_some_and(|after| position.upper.is_none_or(|upper| after > upper))
    {
        return Err(MigrationError::InvalidInput);
    }
    let rows=sqlx::query("SELECT m.*,parent.source_key_hmac AS parent_key FROM migration_family_refresh_mapping m LEFT JOIN migration_family_refresh_mapping parent ON parent.id=m.parent_id AND parent.plan_id=m.plan_id AND parent.organization_id=m.organization_id WHERE m.bundle_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND ($4::uuid IS NULL OR m.id>$4) AND m.id<=$5 AND ($6::text IS NULL OR m.kind=$6) AND ($7::text IS NULL OR m.disposition=$7) ORDER BY m.id LIMIT $8")
        .bind(bundle).bind(q.plan_id).bind(ctx.organization_id.0).bind(position.after).bind(position.upper).bind(q.kind.map(Kind::as_str)).bind(q.disposition.map(Disposition::as_str)).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
    let envelope = Scope {
        organization: ctx.organization_id,
        bundle,
        plan: q.plan_id,
        family: q.family,
        revision: p.get("revision"),
    };
    let mut items = Vec::new();
    for row in rows.iter().take(usize::from(limit)) {
        let data: Mapping = envelope.open(
            key,
            row.get("id"),
            Purpose::Mapping,
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        data.verify(row)?;
        let item = Summary {
            id: row.get("id"),
            parent_id: row.get("parent_id"),
            kind: serde_json::from_value(serde_json::json!(data.kind))
                .map_err(|_| MigrationError::Crypto)?,
            label: data.label,
            value_qualified: data.qualified,
            creation_allowed: data.creation_allowed,
            choice: data.choice,
        };
        super::queries::bounded(&item, 4096)?;
        items.push(item);
    }
    let next_cursor = if rows.len() > items.len() {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            bundle,
            &scope,
            &serde_json::to_value(Position {
                after: Some(items.last().ok_or(MigrationError::Crypto)?.id),
                upper: position.upper,
            })
            .map_err(|_| MigrationError::Crypto)?,
        )?)
    } else {
        None
    };
    let result = Mappings {
        bundle_id: bundle,
        bundle_revision: b.get::<i64, _>("revision").to_string(),
        plan_id: q.plan_id,
        plan_revision: p.get::<i64, _>("revision").to_string(),
        family: q.family,
        inventory_complete: q.family == Family::History || p.get("mappings_complete"),
        items,
        next_cursor,
    };
    super::queries::bounded(&result, RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(result)
}

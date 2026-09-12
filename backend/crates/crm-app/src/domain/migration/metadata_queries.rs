//! Admin-only, bounded reads of frozen metadata plans and committed results.
//! Source content has no Debug/logging path. List queries fetch descriptors;
//! each selected encrypted payload is opened individually before summarization.
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

use super::{
    metadata::{self, Choice, FieldQuery, MetadataPage},
    metadata_model::{self as m, Counts, Manifest, Mapping, ResultData},
    metadata_source::{Entity, Record},
    metadata_store as s,
    snapshot::{self, SnapshotPolicy},
    MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};

const PAGE_BYTES: usize = 512 * 1024;
const ITEM_BYTES: usize = 128 * 1024;
// Covers the envelope and a bounded authenticated cursor, including JSON quotes.
const PAGE_OVERHEAD: usize = 4096;
const KINDS: &[&str] = &["tag", "field", "option"];
const RESULT_KINDS: &[&str] = &["tag", "field", "option", "people"];
const RESULT_DISPOSITIONS: &[&str] = &[
    "applied",
    "created",
    "already_present",
    "held",
    "not_supplied",
    "source_null",
];

#[derive(Clone, Copy)]
struct PlanScope {
    org: OrganizationId,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    revision: i64,
}

async fn scope(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    plan: Uuid,
) -> Result<PlanScope, MigrationError> {
    let run = s::run(conn, org, child).await?;
    let row = s::plan(conn, org, child, plan).await?;
    Ok(PlanScope {
        org,
        child,
        snapshot: run.get("snapshot_id"),
        plan,
        revision: row.get("revision"),
    })
}

fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    scope: PlanScope,
    row: &PgRow,
    purpose: &str,
) -> Result<T, MigrationError> {
    s::open(
        key,
        scope.org,
        scope.snapshot,
        scope.plan,
        row.get("id"),
        purpose,
        row.get("nonce"),
        row.get("ciphertext"),
    )
}

fn validate_page(
    q: &MetadataPage,
    kinds: &[&str],
    dispositions: &[&str],
    parent: bool,
    field: bool,
) -> Result<i64, MigrationError> {
    let limit = q.limit()?;
    if q.kind.as_deref().is_some_and(|v| !kinds.contains(&v))
        || q.disposition
            .as_deref()
            .is_some_and(|v| !dispositions.contains(&v))
        || (!parent && q.parent_import_id.is_some())
        || (q.field_id.is_some() && (!field || q.kind.as_deref() != Some("option")))
    {
        return Err(MigrationError::InvalidInput);
    }
    Ok(limit)
}

fn page_scope(
    endpoint: &str,
    plan: Option<PlanScope>,
    entity: Option<Uuid>,
    q: &MetadataPage,
) -> Result<String, MigrationError> {
    let context = json!({"plan":plan.map(|s|s.plan),"revision":plan.map(|s|s.revision.to_string()),
        "entity":entity,"kind":q.kind,"disposition":q.disposition,"parent":q.parent_import_id,
        "field":q.field_id,"limit":q.limit()?.to_string()});
    Ok(format!(
        "metadata-{endpoint}:{}",
        serde_json::to_string(&context).map_err(|_| MigrationError::Crypto)?
    ))
}

fn cursor_value<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    owner: Uuid,
    purpose: &str,
    cursor: Option<&str>,
) -> Result<Option<T>, MigrationError> {
    snapshot::decode_cursor(key, org, owner, purpose, cursor)?
        .map(|value| serde_json::from_value(value).map_err(|_| MigrationError::InvalidInput))
        .transpose()
}

fn after(
    key: &RawPayloadKey,
    org: OrganizationId,
    owner: Uuid,
    purpose: &str,
    q: &MetadataPage,
) -> Result<Uuid, MigrationError> {
    Ok(cursor_value(key, org, owner, purpose, q.cursor.as_deref())?.unwrap_or(Uuid::nil()))
}

struct Page {
    items: Vec<Value>,
    used: usize,
}

impl Page {
    fn new() -> Self {
        Self {
            items: vec![],
            used: PAGE_OVERHEAD,
        }
    }

    fn push(&mut self, value: Value) -> Result<bool, MigrationError> {
        let size = s::bytes(&value)?.len();
        // Summary helpers bound ordinary items. A corrupt retained structure must
        // fail rather than emit an empty page with a nonadvancing cursor.
        if size > ITEM_BYTES {
            return Err(MigrationError::Crypto);
        }
        if self.used + size + 1 > PAGE_BYTES {
            return Ok(false);
        }
        self.used += size + 1;
        self.items.push(value);
        Ok(true)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Page output preserves independent cryptographic and pagination scopes"
    )]
    fn finish<T: Serialize>(
        self,
        key: &RawPayloadKey,
        org: OrganizationId,
        owner: Uuid,
        purpose: &str,
        rows: usize,
        last: Option<T>,
        envelope: &str,
    ) -> Result<Value, MigrationError> {
        let next = if rows > self.items.len() {
            let last = last.ok_or(MigrationError::Crypto)?;
            Some(snapshot::encode_cursor(
                key,
                org,
                owner,
                purpose,
                &serde_json::to_value(last).map_err(|_| MigrationError::Crypto)?,
            )?)
        } else {
            None
        };
        let value = json!({envelope:self.items,"next_cursor":next});
        if s::bytes(&value)?.len() > PAGE_BYTES {
            return Err(MigrationError::Crypto);
        }
        Ok(value)
    }
}

pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: MetadataPage,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy, async {
        let mut tx = s::begin(pool, ctx, false).await?;
        let limit = validate_page(&q, &[], &[], true, false)?;
        if let Some(parent) = q.parent_import_id {
            if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2)")
                .bind(parent).bind(ctx.organization_id.0).fetch_one(&mut *tx).await? { return Err(MigrationError::NotFound); }
        }
        let purpose = page_scope("list", None, None, &q)?;
        let after = after(key, ctx.organization_id, Uuid::nil(), &purpose, &q)?;
        let rows = sqlx::query_scalar::<_, Uuid>("SELECT id FROM migration_metadata_import WHERE organization_id=$1 AND id>$2 AND ($3::uuid IS NULL OR parent_import_id=$3) ORDER BY id LIMIT $4")
            .bind(ctx.organization_id.0).bind(after).bind(q.parent_import_id).bind(limit+1).fetch_all(&mut *tx).await?;
        let mut page = Page::new(); let mut last = None;
        for id in rows.iter().take(limit as usize) {
            let run = s::run(&mut tx, ctx.organization_id, *id).await?;
            let item = metadata::view(&mut tx, key, ctx.organization_id, &run).await?;
            if !page.push(item)? { break; } last = Some(*id);
        }
        page.finish(key, ctx.organization_id, Uuid::nil(), &purpose, rows.len(), last, "imports")
    }).await
}

async fn mapping_row(
    conn: &mut PgConnection,
    scope: PlanScope,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4")
        .bind(id).bind(scope.plan).bind(scope.child).bind(scope.org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}

async fn mapping_view(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: PlanScope,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    let data: Mapping = open(key, scope, row, "mapping")?;
    let kind: String = row.get("kind");
    let source_id = if kind == "field" {
        sqlx::query_scalar::<_, String>("SELECT source_id FROM migration_metadata_source WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4")
            .bind(row.get::<Option<Uuid>,_>("source_row_id")).bind(scope.plan).bind(scope.child).bind(scope.org.0).fetch_optional(&mut *conn).await?
    } else {
        None
    };
    let parent: Option<Uuid> = row.get("parent_mapping_id");
    let mut field_id = None;
    if kind == "option" {
        if let Some(parent) = parent {
            let parent = mapping_row(conn, scope, parent).await?;
            let body: Mapping = open(key, scope, &parent, "mapping")?;
            if parent.get::<String, _>("disposition") == "map_existing" {
                if let Choice::MapExisting { target_id } = body.choice {
                    if body
                        .target
                        .as_ref()
                        .is_some_and(|t| t.id == target_id && t.kind == "field")
                    {
                        field_id = Some(target_id);
                    }
                }
            }
        }
    }
    let aliases: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_metadata_alias WHERE mapping_id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4")
        .bind(row.get::<Uuid,_>("id")).bind(scope.plan).bind(scope.child).bind(scope.org.0).fetch_one(conn).await?;
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"kind":kind,"parent_mapping_id":parent,"source_id":source_id,
        "field_id":field_id,"disposition":row.get::<String,_>("disposition"),"qualified":row.get::<bool,_>("qualified"),"create_matching_available":data.create_matching_available(row.get("qualified")),
        "choice":data.choice,"target_id":row.get::<Option<Uuid>,_>("target_id"),"target":data.target.as_ref().map(|t|t.wire()),
        "reasons":data.reasons,"suggestions":data.suggestions.iter().map(|t|t.wire()).collect::<Vec<_>>(),
        "dependent_count":row.get::<i64,_>("dependent_count").to_string(),"source":m::summary("source",&data.source),
        "added_byte_bound":row.get::<i64,_>("added_byte_bound").to_string(),"alias_count":aliases.to_string()}),
    )
}

pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, KINDS, &[], false, false)?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    let purpose = page_scope("mappings", Some(scope), None, &q)?;
    let after = after(key, ctx.organization_id, id, &purpose, &q)?;
    let rows = sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_metadata_mapping WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND id>$4 AND ($5::text IS NULL OR kind=$5) ORDER BY id LIMIT $6")
        .bind(plan).bind(id).bind(ctx.organization_id.0).bind(after).bind(&q.kind).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for item in rows.iter().take(limit as usize) {
        let row = mapping_row(&mut tx, scope, *item).await?;
        let value = mapping_view(&mut tx, key, scope, &row).await?;
        if !page.push(value)? {
            break;
        }
        last = Some(*item);
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

pub async fn targets(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, KINDS, &[], false, true)?;
    let kind = q.kind.as_deref().ok_or(MigrationError::InvalidInput)?;
    let run = s::run(&mut tx, ctx.organization_id, id).await?;
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let scope = PlanScope {
        org: ctx.organization_id,
        child: id,
        snapshot: run.get("snapshot_id"),
        plan,
        revision: p.get("revision"),
    };
    let destination = metadata::decode_destination(key, &run, &p)?;
    if q.field_id
        .is_some_and(|field| !destination.fields.iter().any(|v| v.id == field))
    {
        return Err(MigrationError::NotFound);
    }
    let purpose = page_scope("targets", Some(scope), None, &q)?;
    let after = after(key, ctx.organization_id, id, &purpose, &q)?;
    // The frozen destination is bounded to 200 tags, 50 fields and 2,500 options.
    let mut candidates = destination
        .all()
        .filter(|t| {
            t.kind == kind
                && t.id > after
                && q.field_id.is_none_or(|field| t.field_id == Some(field))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|t| t.id);
    let mut page = Page::new();
    let mut last = None;
    for target in candidates.iter().take(limit as usize) {
        if !page.push(target.wire())? {
            break;
        }
        last = Some(target.id);
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        candidates.len(),
        last,
        "items",
    )
}

async fn manifest_row(
    conn: &mut PgConnection,
    scope: PlanScope,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_metadata_manifest WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4")
        .bind(id).bind(scope.plan).bind(scope.child).bind(scope.org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}

async fn source_record(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: PlanScope,
    id: Uuid,
) -> Result<Record, MigrationError> {
    let row = sqlx::query("SELECT * FROM migration_metadata_source WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND snapshot_id=$4 AND organization_id=$5")
        .bind(id).bind(scope.plan).bind(scope.child).bind(scope.snapshot).bind(scope.org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    open(key, scope, &row, "source")
}

fn record_counts(row: &PgRow, manifest: &Manifest) -> Counts {
    let mut counts = Counts::default();
    counts.people.source = 1;
    if row.get::<String, _>("disposition") == "eligible" {
        counts.people.eligible = 1;
    } else {
        counts.people.excluded = 1;
    }
    for op in &manifest.operations {
        counts.planned(&op.kind, &op.disposition);
    }
    counts
}

async fn record_view(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: PlanScope,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    let source = source_record(conn, key, scope, row.get("source_row_id")).await?;
    let manifest: Manifest = open(key, scope, row, "manifest")?;
    let operations =
        serde_json::to_value(&manifest.operations).map_err(|_| MigrationError::Crypto)?;
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"source_id":row.get::<String,_>("source_id"),"person_id":row.get::<Option<Uuid>,_>("person_id"),
        "disposition":row.get::<String,_>("disposition"),"reasons":manifest.reasons,"counts":record_counts(row,&manifest).wire(),
        "added_byte_bound":row.get::<i64,_>("added_byte_bound").to_string(),"source":m::summary("source",&source.provenance),
        "operations":m::summary("operations",&m::operation_fields(&operations))}),
    )
}

pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, &[], &["eligible", "held"], false, false)?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    let purpose = page_scope("records", Some(scope), None, &q)?;
    let after = after(key, ctx.organization_id, id, &purpose, &q)?;
    let rows = sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND id>$4 AND ($5::text IS NULL OR disposition=$5) ORDER BY id LIMIT $6")
        .bind(plan).bind(id).bind(ctx.organization_id.0).bind(after).bind(&q.disposition).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for item in rows.iter().take(limit as usize) {
        let row = manifest_row(&mut tx, scope, *item).await?;
        let value = record_view(&mut tx, key, scope, &row).await?;
        if !page.push(value)? {
            break;
        }
        last = Some(*item);
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

async fn result_row(
    conn: &mut PgConnection,
    org: OrganizationId,
    child: Uuid,
    result: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_metadata_result WHERE id=$1 AND import_id=$2 AND organization_id=$3")
        .bind(result).bind(child).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}

fn result_view(
    key: &RawPayloadKey,
    scope: PlanScope,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    let result: ResultData = open(key, scope, row, "result")?;
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"kind":row.get::<String,_>("kind"),"source_id":row.get::<Option<String>,_>("source_id"),
        "person_id":row.get::<Option<Uuid>,_>("person_id"),"mapping_id":row.get::<Option<Uuid>,_>("mapping_id"),
        "record_id":row.get::<Option<Uuid>,_>("manifest_id"),"disposition":row.get::<String,_>("disposition"),
        "committed_at":row.get::<DateTime<Utc>,_>("committed_at"),"counts":m::decimal(row.get("counts")),"reasons":result.reasons,
        "source":m::summary("source",&result.source),"operations":m::summary("operations",&m::operation_fields(&result.operations))}),
    )
}

pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, RESULT_KINDS, RESULT_DISPOSITIONS, false, false)?;
    let run = s::run(&mut tx, ctx.organization_id, id).await?;
    let plan = run
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .unwrap_or(run.get("latest_plan_id"));
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let scope = PlanScope {
        org: ctx.organization_id,
        child: id,
        snapshot: run.get("snapshot_id"),
        plan,
        revision: p.get("revision"),
    };
    let purpose = page_scope("results", Some(scope), None, &q)?;
    let after = after(key, ctx.organization_id, id, &purpose, &q)?;
    let rows = sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_metadata_result WHERE import_id=$1 AND organization_id=$2 AND plan_id=$3 AND id>$4 AND ($5::text IS NULL OR kind=$5) AND ($6::text IS NULL OR disposition=$6) ORDER BY id LIMIT $7")
        .bind(id).bind(ctx.organization_id.0).bind(plan).bind(after).bind(&q.kind).bind(&q.disposition).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for item in rows.iter().take(limit as usize) {
        let row = result_row(&mut tx, ctx.organization_id, id, *item).await?;
        if row.get::<Uuid, _>("plan_id") != plan {
            return Err(MigrationError::Crypto);
        }
        if !page.push(result_view(key, scope, &row)?)? {
            break;
        }
        last = Some(*item);
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

pub async fn issues(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, &[], &[], false, false)?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    let purpose = page_scope("issues", Some(scope), None, &q)?;
    let after: String = cursor_value(key, ctx.organization_id, id, &purpose, q.cursor.as_deref())?
        .unwrap_or_default();
    let rows = sqlx::query("SELECT code,record_count FROM migration_metadata_issue WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND code>$4 ORDER BY code LIMIT $5")
        .bind(plan).bind(id).bind(ctx.organization_id.0).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        let code: String = row.get("code");
        if !page.push(json!({"code":code,"count":row.get::<i64,_>("record_count").to_string()}))? {
            break;
        }
        last = Some(code);
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

pub async fn aliases(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    mapping: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate_page(&q, &[], &[], false, false)?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    let row = mapping_row(&mut tx, scope, mapping).await?;
    if row.get::<String, _>("kind") != "tag" {
        return Err(MigrationError::NotFound);
    }
    let purpose = page_scope("aliases", Some(scope), Some(mapping), &q)?;
    let after = after(key, ctx.organization_id, id, &purpose, &q)?;
    let rows = sqlx::query("SELECT id,source_row_id,element_ordinal FROM migration_metadata_alias WHERE mapping_id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4 AND id>$5 ORDER BY id LIMIT $6")
        .bind(mapping).bind(plan).bind(id).bind(ctx.organization_id.0).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        let source_row: Uuid = row.get("source_row_id");
        let source = source_record(&mut tx, key, scope, source_row).await?;
        let ordinal: i32 = row.get("element_ordinal");
        let Entity::Person(person) = source.entity else {
            return Err(MigrationError::Crypto);
        };
        let raw = person
            .tags
            .iter()
            .find(|tag| i64::from(tag.ordinal) == i64::from(ordinal))
            .and_then(|tag| tag.raw.as_deref())
            .ok_or(MigrationError::Crypto)?;
        let evidence = BTreeMap::from([
            (
                "tag".into(),
                serde_json::to_string(raw).map_err(|_| MigrationError::Crypto)?,
            ),
            ("ordinal".into(), ordinal.to_string()),
        ]);
        let record_id = sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_metadata_manifest WHERE plan_id=$1 AND import_id=$2 AND organization_id=$3 AND source_id=$4 AND source_row_id=$5")
            .bind(plan).bind(id).bind(ctx.organization_id.0).bind(&source.source_id).bind(source_row).fetch_optional(&mut *tx).await?;
        if !page.push(json!({"source_id":source.source_id,"record_id":record_id,"ordinal":ordinal.to_string(),"source":m::summary("source",&evidence)}))? { break; }
        last = Some(row.get::<Uuid, _>("id"));
    }
    page.finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

fn valid_field(field: &str) -> bool {
    if matches!(field, "all" | "source.all" | "operations.all") {
        return true;
    }
    let Some((section, digest)) = field.split_once('.') else {
        return false;
    };
    matches!(section, "source" | "operations")
        && digest.len() == 64
        && digest
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Field cursors bind every independent evidence scope"
)]
fn segment_response(
    key: &RawPayloadKey,
    scope: PlanScope,
    endpoint: &str,
    entity: Uuid,
    person: Option<Uuid>,
    field: &str,
    text: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let limit = q.limit.unwrap_or(16384) as usize;
    if !(4..=65536).contains(&limit) {
        return Err(MigrationError::InvalidInput);
    }
    let purpose = format!(
        "metadata-{endpoint}:{}:{}:{entity}:{}:{field}:{limit}",
        scope.plan,
        scope.revision,
        person.map(|p| p.to_string()).unwrap_or_default()
    );
    let offset: Option<String> =
        cursor_value(key, scope.org, scope.child, &purpose, q.cursor.as_deref())?;
    let offset = offset
        .map(|v| v.parse::<usize>().map_err(|_| MigrationError::InvalidInput))
        .transpose()?
        .unwrap_or(0);
    let (segment, end) = m::segment(text, offset, limit)?;
    let complete = end == text.len();
    let next = if complete {
        None
    } else {
        Some(snapshot::encode_cursor(
            key,
            scope.org,
            scope.child,
            &purpose,
            &json!(end.to_string()),
        )?)
    };
    let value = json!({"text":segment,"full_utf8_bytes":text.len().to_string(),"offset_bytes":offset.to_string(),"next_cursor":next,"complete":complete});
    if s::bytes(&value)?.len() > PAGE_BYTES {
        return Err(MigrationError::Crypto);
    }
    Ok(value)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, plan, record and cryptographic scopes"
)]
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    record: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    if !valid_field(field) {
        return Err(MigrationError::NotFound);
    }
    let row = manifest_row(&mut tx, scope, record).await?;
    let source = source_record(&mut tx, key, scope, row.get("source_row_id")).await?;
    let manifest: Manifest = open(key, scope, &row, "manifest")?;
    let operations =
        serde_json::to_value(manifest.operations).map_err(|_| MigrationError::Crypto)?;
    let text =
        m::field_text(&source.provenance, &operations, field).ok_or(MigrationError::NotFound)?;
    segment_response(key, scope, "record-field", record, None, field, &text, q)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Explicit tenant, plan, mapping and cryptographic scopes"
)]
pub async fn mapping_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    mapping: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let scope = scope(&mut tx, ctx.organization_id, id, plan).await?;
    if !valid_field(field) {
        return Err(MigrationError::NotFound);
    }
    let row = mapping_row(&mut tx, scope, mapping).await?;
    let data: Mapping = open(key, scope, &row, "mapping")?;
    let operations = json!({"choice":data.choice,"target":data.target.map(|t|t.wire()),"reasons":data.reasons,"transformations":data.transformations});
    let text = m::field_text(&data.source, &operations, field).ok_or(MigrationError::NotFound)?;
    segment_response(key, scope, "mapping-field", mapping, None, field, &text, q)
}

pub async fn result_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    result: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let row = result_row(&mut tx, ctx.organization_id, id, result).await?;
    let scope = scope(&mut tx, ctx.organization_id, id, row.get("plan_id")).await?;
    if !valid_field(field) {
        return Err(MigrationError::NotFound);
    }
    let data: ResultData = open(key, scope, &row, "result")?;
    let text =
        m::field_text(&data.source, &data.operations, field).ok_or(MigrationError::NotFound)?;
    segment_response(key, scope, "result-field", result, None, field, &text, q)
}

async fn live_person(
    conn: &mut PgConnection,
    org: OrganizationId,
    person: Uuid,
) -> Result<(), MigrationError> {
    if !sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM person WHERE id=$1 AND organization_id=$2)",
    )
    .bind(person)
    .bind(org.0)
    .fetch_one(conn)
    .await?
    {
        return Err(MigrationError::NotFound);
    }
    Ok(())
}

pub async fn provenance(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    live_person(&mut tx, ctx.organization_id, person).await?;
    let limit = validate_page(&q, &[], &[], false, false)?;
    let purpose = page_scope("provenance", None, Some(person), &q)?;
    let after = after(key, ctx.organization_id, person, &purpose, &q)?;
    let rows = sqlx::query("SELECT id,import_id,plan_id FROM migration_metadata_result WHERE organization_id=$1 AND person_id=$2 AND kind='people' AND id>$3 ORDER BY id LIMIT $4")
        .bind(ctx.organization_id.0).bind(person).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for item in rows.iter().take(limit as usize) {
        let scope = scope(
            &mut tx,
            ctx.organization_id,
            item.get("import_id"),
            item.get("plan_id"),
        )
        .await?;
        let row = result_row(&mut tx, ctx.organization_id, scope.child, item.get("id")).await?;
        if row.get::<Option<Uuid>, _>("person_id") != Some(person) {
            return Err(MigrationError::NotFound);
        }
        if !page.push(result_view(key, scope, &row)?)? {
            break;
        }
        last = Some(item.get::<Uuid, _>("id"));
    }
    page.finish(
        key,
        ctx.organization_id,
        person,
        &purpose,
        rows.len(),
        last,
        "items",
    )
}

pub async fn provenance_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    result: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    live_person(&mut tx, ctx.organization_id, person).await?;
    let row = sqlx::query("SELECT * FROM migration_metadata_result WHERE id=$1 AND organization_id=$2 AND person_id=$3 AND kind='people'")
        .bind(result).bind(ctx.organization_id.0).bind(person).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let scope = scope(
        &mut tx,
        ctx.organization_id,
        row.get("import_id"),
        row.get("plan_id"),
    )
    .await?;
    if !valid_field(field) {
        return Err(MigrationError::NotFound);
    }
    let data: ResultData = open(key, scope, &row, "result")?;
    let text =
        m::field_text(&data.source, &data.operations, field).ok_or(MigrationError::NotFound)?;
    segment_response(
        key,
        scope,
        "provenance-field",
        result,
        Some(person),
        field,
        &text,
        q,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> PlanScope {
        PlanScope {
            org: OrganizationId::new(Uuid::new_v4()),
            child: Uuid::new_v4(),
            snapshot: Uuid::new_v4(),
            plan: Uuid::new_v4(),
            revision: 1,
        }
    }

    #[test]
    fn page_cursor_binds_endpoint_plan_filter_entity_limit_and_tenant() {
        let key = RawPayloadKey::new([19; 32]);
        let scope = scope();
        let entity = Uuid::new_v4();
        let q = MetadataPage {
            kind: Some("field".into()),
            limit: Some(1),
            ..Default::default()
        };
        let purpose = page_scope("mappings", Some(scope), Some(entity), &q).unwrap();
        let value = Uuid::new_v4();
        let cursor =
            snapshot::encode_cursor(&key, scope.org, scope.child, &purpose, &json!(value)).unwrap();
        assert_eq!(
            cursor_value::<Uuid>(&key, scope.org, scope.child, &purpose, Some(&cursor)).unwrap(),
            Some(value)
        );
        for changed in [
            page_scope("records", Some(scope), Some(entity), &q).unwrap(),
            page_scope(
                "mappings",
                Some(PlanScope {
                    revision: 2,
                    ..scope
                }),
                Some(entity),
                &q,
            )
            .unwrap(),
            page_scope("mappings", Some(scope), Some(Uuid::new_v4()), &q).unwrap(),
            page_scope(
                "mappings",
                Some(scope),
                Some(entity),
                &MetadataPage {
                    limit: Some(2),
                    kind: Some("field".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
            page_scope(
                "mappings",
                Some(scope),
                Some(entity),
                &MetadataPage {
                    limit: Some(1),
                    kind: Some("tag".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        ] {
            assert!(
                cursor_value::<Uuid>(&key, scope.org, scope.child, &changed, Some(&cursor))
                    .is_err()
            );
        }
        assert!(cursor_value::<Uuid>(
            &key,
            OrganizationId::new(Uuid::new_v4()),
            scope.child,
            &purpose,
            Some(&cursor)
        )
        .is_err());
        assert!(
            cursor_value::<Uuid>(&key, scope.org, Uuid::new_v4(), &purpose, Some(&cursor)).is_err()
        );
    }

    #[test]
    fn page_shrinks_before_its_byte_ceiling_and_advances_from_last_visible_item() {
        let key = RawPayloadKey::new([20; 32]);
        let scope = scope();
        let mut page = Page::new();
        for n in 0..10 {
            if !page
                .push(json!({"n":n,"text":"x".repeat(120*1024)}))
                .unwrap()
            {
                break;
            }
        }
        assert_eq!(page.items.len(), 4);
        let response = page
            .finish(
                &key,
                scope.org,
                scope.child,
                "metadata-test",
                10,
                Some("visible-four"),
                "items",
            )
            .unwrap();
        assert!(s::bytes(&response).unwrap().len() <= PAGE_BYTES);
        assert_eq!(
            cursor_value::<String>(
                &key,
                scope.org,
                scope.child,
                "metadata-test",
                response["next_cursor"].as_str()
            )
            .unwrap()
            .as_deref(),
            Some("visible-four")
        );
        assert!(Page::new().push(json!("x".repeat(ITEM_BYTES))).is_err());
        assert!(Page::new()
            .finish::<Uuid>(
                &key,
                scope.org,
                scope.child,
                "metadata-test",
                1,
                None,
                "items"
            )
            .is_err());
    }

    #[test]
    fn field_segments_use_exact_offsets_and_do_not_reuse_other_evidence_cursors() {
        let key = RawPayloadKey::new([21; 32]);
        let scope = scope();
        let entity = Uuid::new_v4();
        let person = Uuid::new_v4();
        let first = segment_response(
            &key,
            scope,
            "record-field",
            entity,
            None,
            "source.all",
            "🙂🙂end",
            FieldQuery {
                cursor: None,
                limit: Some(4),
            },
        )
        .unwrap();
        assert_eq!(first["text"], "🙂");
        assert_eq!(first["offset_bytes"], "0");
        assert_eq!(first["complete"], false);
        let cursor = first["next_cursor"].as_str().unwrap().to_owned();
        let second = segment_response(
            &key,
            scope,
            "record-field",
            entity,
            None,
            "source.all",
            "🙂🙂end",
            FieldQuery {
                cursor: Some(cursor.clone()),
                limit: Some(4),
            },
        )
        .unwrap();
        assert_eq!(second["text"], "🙂");
        assert_eq!(second["offset_bytes"], "4");
        for (endpoint, entity, person, field, limit) in [
            ("result-field", entity, None, "source.all", 4),
            ("record-field", Uuid::new_v4(), None, "source.all", 4),
            ("record-field", entity, Some(person), "source.all", 4),
            ("record-field", entity, None, "operations.all", 4),
            ("record-field", entity, None, "source.all", 5),
        ] {
            assert!(segment_response(
                &key,
                scope,
                endpoint,
                entity,
                person,
                field,
                "🙂🙂end",
                FieldQuery {
                    cursor: Some(cursor.clone()),
                    limit: Some(limit)
                }
            )
            .is_err());
        }
    }

    #[test]
    fn query_and_field_names_remain_closed_and_bounded() {
        assert!(valid_field("all"));
        assert!(valid_field("operations.all"));
        assert!(valid_field(&format!("source.{}", "a".repeat(64))));
        for bad in ["name", "source.", "source.all:other", "unknown.all"] {
            assert!(!valid_field(bad));
        }
        for limit in [0, 51] {
            assert!(validate_page(
                &MetadataPage {
                    limit: Some(limit),
                    ..Default::default()
                },
                KINDS,
                &[],
                false,
                false
            )
            .is_err());
        }
        assert!(validate_page(
            &MetadataPage {
                field_id: Some(Uuid::new_v4()),
                kind: Some("tag".into()),
                ..Default::default()
            },
            KINDS,
            &[],
            false,
            true
        )
        .is_err());
        assert!(validate_page(
            &MetadataPage {
                parent_import_id: Some(Uuid::new_v4()),
                ..Default::default()
            },
            KINDS,
            &[],
            false,
            false
        )
        .is_err());
        assert!(validate_page(
            &MetadataPage {
                kind: Some("people".into()),
                ..Default::default()
            },
            KINDS,
            &[],
            false,
            false
        )
        .is_err());
    }
}

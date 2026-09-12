//! Bounded current-admin activity views. Bodies remain untrusted, no-store data.
use super::{
    activity::{self, ActivityPage, FieldQuery},
    activity_model::{self as m, Manifest, Mapping, ResultData},
    activity_store as s, metadata_model as display,
    snapshot::{self, SnapshotPolicy},
    MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
const PAGE_BYTES: usize = 512 * 1024;
const ITEM_BYTES: usize = 128 * 1024;
fn purpose(
    endpoint: &str,
    plan: Uuid,
    revision: i64,
    q: &ActivityPage,
) -> Result<String, MigrationError> {
    Ok(format!("activity-{endpoint}:{}",serde_json::to_string(&json!({"plan":plan,"revision":revision.to_string(),"kind":q.kind,"disposition":q.disposition,"issue":q.issue,"parent":q.parent_import_id,"limit":q.limit()?})).map_err(|_|MigrationError::Crypto)?))
}
fn cursor<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    purpose: &str,
    value: Option<&str>,
) -> Result<Option<T>, MigrationError> {
    snapshot::decode_cursor(key, org, id, purpose, value)?
        .map(|v| serde_json::from_value(v).map_err(|_| MigrationError::InvalidInput))
        .transpose()
}
fn validate(q: &ActivityPage, endpoint: &str) -> Result<i64, MigrationError> {
    let limit = q.limit()?;
    let kinds = if endpoint == "mappings" {
        &["note_author", "task_creator", "task_assignee", "task_kind"][..]
    } else {
        &["note", "task"][..]
    };
    if q.kind.as_deref().is_some_and(|v| !kinds.contains(&v))
        || q.disposition
            .as_deref()
            .is_some_and(|v| !matches!(v, "eligible" | "already_present" | "held" | "applied"))
        || q.issue.as_deref().is_some_and(|v| {
            v.len() > 100 || !v.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
        })
        || ((endpoint == "list" || endpoint == "targets")
            && (q.kind.is_some() || q.disposition.is_some() || q.issue.is_some()))
        || ((endpoint == "mappings") && (q.disposition.is_some() || q.issue.is_some()))
        || endpoint != "list" && q.parent_import_id.is_some()
    {
        return Err(MigrationError::InvalidInput);
    }
    Ok(limit)
}
fn push(items: &mut Vec<Value>, used: &mut usize, value: Value) -> Result<bool, MigrationError> {
    let n = s::bytes(&value)?.len();
    if n > ITEM_BYTES {
        return Err(MigrationError::Crypto);
    }
    if *used + n + 1 > PAGE_BYTES {
        return Ok(false);
    }
    *used += n + 1;
    items.push(value);
    Ok(true)
}
#[allow(clippy::too_many_arguments)]
fn finish(
    key: &RawPayloadKey,
    org: OrganizationId,
    owner: Uuid,
    purpose: &str,
    items: Vec<Value>,
    rows: usize,
    last: Option<Uuid>,
    envelope: &str,
) -> Result<Value, MigrationError> {
    let next = if rows > items.len() {
        Some(snapshot::encode_cursor(
            key,
            org,
            owner,
            purpose,
            &json!(last.ok_or(MigrationError::Crypto)?),
        )?)
    } else {
        None
    };
    let value = json!({envelope:items,"next_cursor":next});
    if s::bytes(&value)?.len() > PAGE_BYTES {
        return Err(MigrationError::Crypto);
    }
    Ok(value)
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: ActivityPage,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{let mut tx=s::begin(pool,ctx,false).await?;let limit=validate(&q,"list")?;if let Some(parent)=q.parent_import_id{if !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2)").bind(parent).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?{return Err(MigrationError::NotFound)}}let purpose=purpose("list",Uuid::nil(),0,&q)?;let after=cursor(key,ctx.organization_id,Uuid::nil(),&purpose,q.cursor.as_deref())?.unwrap_or(Uuid::nil());let rows=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_activity_import WHERE organization_id=$1 AND id>$2 AND ($3::uuid IS NULL OR parent_import_id=$3) ORDER BY id LIMIT $4").bind(ctx.organization_id.0).bind(after).bind(q.parent_import_id).bind(limit+1).fetch_all(&mut *tx).await?;let mut items=vec![];let mut used=4096;let mut last=None;for id in rows.iter().take(limit as usize){let r=s::run(&mut tx,ctx.organization_id,*id).await?;activity::validate_binding(&mut tx,&r).await?;let v=activity::view(&mut tx,key,ctx.organization_id,&r).await?;if !push(&mut items,&mut used,v)?{break}last=Some(*id)}finish(key,ctx.organization_id,Uuid::nil(),&purpose,items,rows.len(),last,"imports")}).await
}
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ActivityPage,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, q, "records").await
}
pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ActivityPage,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, q, "mappings").await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ActivityPage,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, q, "results").await
}
async fn page(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ActivityPage,
    endpoint: &str,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate(&q, endpoint)?;
    let r = s::run(&mut tx, ctx.organization_id, id).await?;
    activity::validate_binding(&mut tx, &r).await?;
    let plan = q.plan_id.unwrap_or(r.get("latest_plan_id"));
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let purpose = purpose(endpoint, plan, p.get("revision"), &q)?;
    let after =
        cursor(key, ctx.organization_id, id, &purpose, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    // Start selective issue filters from the immutable issue projection. A
    // correlated optional EXISTS can scan every result for an absent code.
    let sql = match (endpoint, q.issue.is_some()) {
        ("records", true) => "SELECT a.id FROM migration_activity_manifest_issue x JOIN migration_activity_manifest a ON a.id=x.manifest_id AND a.plan_id=x.plan_id AND a.import_id=x.import_id AND a.organization_id=x.organization_id WHERE x.plan_id=$1 AND x.organization_id=$2 AND x.manifest_id>$3 AND ($4::text IS NULL OR a.kind=$4) AND ($5::text IS NULL OR a.disposition=$5) AND x.code=$6 ORDER BY x.manifest_id LIMIT $7",
        ("results", true) => "SELECT a.id FROM migration_activity_result_issue x JOIN migration_activity_result a ON a.id=x.result_id AND a.plan_id=x.plan_id AND a.import_id=x.import_id AND a.organization_id=x.organization_id WHERE x.plan_id=$1 AND x.organization_id=$2 AND x.result_id>$3 AND ($4::text IS NULL OR a.kind=$4) AND ($5::text IS NULL OR a.disposition=$5) AND x.code=$6 ORDER BY x.result_id LIMIT $7",
        ("records", false) => "SELECT a.id FROM migration_activity_manifest a WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.id>$3 AND ($4::text IS NULL OR a.kind=$4) AND ($5::text IS NULL OR a.disposition=$5) AND $6::text IS NULL ORDER BY a.id LIMIT $7",
        ("results", false) => "SELECT a.id FROM migration_activity_result a WHERE a.plan_id=$1 AND a.organization_id=$2 AND a.id>$3 AND ($4::text IS NULL OR a.kind=$4) AND ($5::text IS NULL OR a.disposition=$5) AND $6::text IS NULL ORDER BY a.id LIMIT $7",
        _ => "SELECT id FROM migration_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND ($4::text IS NULL OR kind=$4) AND $5::text IS NULL AND $6::text IS NULL ORDER BY id LIMIT $7",
    };
    let rows = sqlx::query_scalar::<_, Uuid>(sql)
        .bind(plan)
        .bind(ctx.organization_id.0)
        .bind(after)
        .bind(&q.kind)
        .bind(&q.disposition)
        .bind(&q.issue)
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await?;
    let mut items = vec![];
    let mut used = 4096;
    let mut last = None;
    for rowid in rows.iter().take(limit as usize) {
        let sql=match endpoint{"records"=>"SELECT * FROM migration_activity_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3","results"=>"SELECT * FROM migration_activity_result WHERE id=$1 AND plan_id=$2 AND organization_id=$3",_=>"SELECT * FROM migration_activity_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3"};
        let row = sqlx::query(sql)
            .bind(rowid)
            .bind(plan)
            .bind(ctx.organization_id.0)
            .fetch_one(&mut *tx)
            .await?;
        let field_url=format!("/api/migrations/fub/activity-imports/{id}/{endpoint}/{rowid}/fields/all?plan_id={plan}");
        let value = match endpoint {
            "records" => {
                let d: Manifest = s::open(
                    key,
                    ctx.organization_id,
                    r.get("snapshot_id"),
                    plan,
                    *rowid,
                    "manifest",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                json!({"id":rowid,"kind":row.get::<String,_>("kind"),"source_id":row.get::<Option<String>,_>("source_id"),"person_id":row.get::<Option<Uuid>,_>("person_id"),"target_id":row.get::<Option<Uuid>,_>("target_id"),"disposition":row.get::<String,_>("disposition"),"source_summary":display::summary("source",&d.record.provenance),"preview":m::preview_wire(&d),"observations_url":format!("/api/migrations/fub/activity-imports/{id}/records/{rowid}/observations?plan_id={plan}"),"field_url":field_url})
            }
            "results" => {
                let d: ResultData = s::open(
                    key,
                    ctx.organization_id,
                    r.get("snapshot_id"),
                    plan,
                    *rowid,
                    "result",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                json!({"id":rowid,"kind":row.get::<String,_>("kind"),"source_id":row.get::<Option<String>,_>("source_id"),"person_id":row.get::<Option<Uuid>,_>("person_id"),"target_id":row.get::<Option<Uuid>,_>("target_id"),"disposition":row.get::<String,_>("disposition"),"committed_at":row.get::<chrono::DateTime<chrono::Utc>,_>("committed_at"),"reasons":d.reasons,"source_summary":display::summary("source",&d.source),"field_url":field_url})
            }
            _ => {
                let d: Mapping = s::open(
                    key,
                    ctx.organization_id,
                    r.get("snapshot_id"),
                    plan,
                    *rowid,
                    "mapping",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                json!({"id":rowid,"role":d.role,"source_value":d.source_value.chars().take(1024).collect::<String>(),"source_value_abbreviated":d.source_value.chars().count()>1024,"source_value_full_utf8_bytes":d.source_value.len().to_string(),"choice":d.choice,"suggestions":d.suggestions,"suggested_kind":d.suggested_kind,"dependent_count":row.get::<i64,_>("dependent_count").to_string(),"source_summary":display::summary("source",&d.source),"field_url":field_url})
            }
        };
        if !push(&mut items, &mut used, value)? {
            break;
        }
        last = Some(*rowid)
    }
    finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        items,
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
    q: ActivityPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate(&q, "targets")?;
    let r = s::run(&mut tx, ctx.organization_id, id).await?;
    activity::validate_binding(&mut tx, &r).await?;
    let plan = q.plan_id.unwrap_or(r.get("latest_plan_id"));
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let d = activity::decode_destination(key, &r, &p)?;
    let purpose = purpose("targets", plan, p.get("revision"), &q)?;
    let after =
        cursor(key, ctx.organization_id, id, &purpose, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let rows = d
        .members
        .into_iter()
        .filter(|m| m.id > after)
        .take(limit as usize + 1)
        .collect::<Vec<_>>();
    let mut items = vec![];
    let mut used = 4096;
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        if !push(&mut items, &mut used, json!(row))? {
            break;
        }
        last = Some(row.id)
    }
    finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        items,
        rows.len(),
        last,
        "items",
    )
}
#[allow(clippy::too_many_arguments)]
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    endpoint: &str,
    rowid: Uuid,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::run(&mut tx, ctx.organization_id, id).await?;
    activity::validate_binding(&mut tx, &r).await?;
    let plan = q.plan_id.unwrap_or(r.get("latest_plan_id"));
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let sql=match endpoint{"records"=>"SELECT * FROM migration_activity_manifest WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4","mappings"=>"SELECT * FROM migration_activity_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4","results"=>"SELECT * FROM migration_activity_result WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4","observations"=>"SELECT * FROM migration_activity_source WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4",_=>return Err(MigrationError::NotFound)};
    let row = sqlx::query(sql)
        .bind(rowid)
        .bind(plan)
        .bind(id)
        .bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::NotFound)?;
    let (src, op) = match endpoint {
        "observations" => {
            let data: super::activity_source::Record = s::open(
                key,
                ctx.organization_id,
                r.get("snapshot_id"),
                plan,
                rowid,
                "source",
                row.get("nonce"),
                row.get("ciphertext"),
            )?;
            (
                data.provenance,
                json!({"capture_id":row.get::<Uuid,_>("capture_id"),"stream":row.get::<String,_>("stream"),"representation":row.get::<String,_>("representation")}),
            )
        }
        "records" => {
            let d: Manifest = s::open(
                key,
                ctx.organization_id,
                r.get("snapshot_id"),
                plan,
                rowid,
                "manifest",
                row.get("nonce"),
                row.get("ciphertext"),
            )?;
            (d.record.provenance.clone(), m::preview_wire(&d))
        }
        "mappings" => {
            let d: Mapping = s::open(
                key,
                ctx.organization_id,
                r.get("snapshot_id"),
                plan,
                rowid,
                "mapping",
                row.get("nonce"),
                row.get("ciphertext"),
            )?;
            (
                d.source,
                json!({"choice":d.choice,"source_value":d.source_value}),
            )
        }
        _ => {
            let d: ResultData = s::open(
                key,
                ctx.organization_id,
                r.get("snapshot_id"),
                plan,
                rowid,
                "result",
                row.get("nonce"),
                row.get("ciphertext"),
            )?;
            (
                d.source,
                json!({"native":d.native,"reasons":d.reasons,"transformations":d.transformations,"source_only":d.source_only}),
            )
        }
    };
    let text = if endpoint == "observations" && field == "raw" {
        let capture: Uuid = row.get("capture_id");
        let capture_row=sqlx::query("SELECT nonce,ciphertext FROM migration_snapshot_capture WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(capture).bind(r.get::<Uuid,_>("snapshot_id")).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        String::from_utf8(
            super::crypto::open_snapshot(
                key,
                ctx.organization_id,
                r.get("snapshot_id"),
                capture,
                "capture",
                capture_row.get("nonce"),
                capture_row.get("ciphertext"),
            )
            .map_err(|_| MigrationError::Crypto)?,
        )
        .map_err(|_| MigrationError::Crypto)?
    } else {
        display::field_text(&src, &op, field).ok_or(MigrationError::NotFound)?
    };
    let limit = q.limit.unwrap_or(16384) as usize;
    if !(4..=65536).contains(&limit) {
        return Err(MigrationError::InvalidInput);
    }
    let purpose = format!(
        "activity-field:{endpoint}:{plan}:{}:{rowid}:{field}:{limit}",
        p.get::<i64, _>("revision")
    );
    let offset = cursor::<String>(key, ctx.organization_id, id, &purpose, q.cursor.as_deref())?
        .map(|v| v.parse::<usize>().map_err(|_| MigrationError::InvalidInput))
        .transpose()?
        .unwrap_or(0);
    let (text_segment, end) = display::segment(&text, offset, limit)?;
    let next = if end < text.len() {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            id,
            &purpose,
            &json!(end.to_string()),
        )?)
    } else {
        None
    };
    Ok(
        json!({"text":text_segment,"offset":offset.to_string(),"next_offset":end.to_string(),"total_utf8_bytes":text.len().to_string(),"next_cursor":next}),
    )
}

pub async fn observations(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    record: Uuid,
    q: ActivityPage,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let limit = validate(&q, "targets")?;
    let r = s::run(&mut tx, ctx.organization_id, id).await?;
    activity::validate_binding(&mut tx, &r).await?;
    let plan = q.plan_id.unwrap_or(r.get("latest_plan_id"));
    let p = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    let manifest=sqlx::query("SELECT source_id,kind,source_row_id FROM migration_activity_manifest WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4").bind(record).bind(plan).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let sid: Option<String> = manifest.get("source_id");
    let family = if manifest.get::<String, _>("kind") == "note" {
        "notes"
    } else {
        "tasks"
    };
    let fingerprint = super::crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        "request",
        &s::bytes(&json!({"stream":"note_detail","offset":0,"next":null,"source_id":sid}))?,
    );
    let purpose = format!(
        "{}:{record}",
        purpose("observations", plan, p.get("revision"), &q)?
    );
    let after =
        cursor(key, ctx.organization_id, id, &purpose, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT id,capture_id,record_id,stream,representation,negative,source_id FROM migration_activity_source WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND ((family=$4 AND source_id=$5) OR id=$6 OR ($4='notes' AND negative AND semantic_hmac=$7)) ORDER BY id LIMIT $8").bind(plan).bind(ctx.organization_id.0).bind(after).bind(family).bind(sid).bind(manifest.get::<Uuid,_>("source_row_id")).bind(fingerprint.as_slice()).bind(limit+1).fetch_all(&mut *tx).await?;
    let items=rows.iter().take(limit as usize).map(|row|{let rowid:Uuid=row.get("id");json!({"id":rowid,"capture_id":row.get::<Uuid,_>("capture_id"),"record_id":row.get::<Option<Uuid>,_>("record_id"),"stream":row.get::<String,_>("stream"),"representation":row.get::<String,_>("representation"),"negative":row.get::<bool,_>("negative"),"source_id":row.get::<Option<String>,_>("source_id"),"field_url":format!("/api/migrations/fub/activity-imports/{id}/observations/{rowid}/fields/all?plan_id={plan}"),"raw_url":format!("/api/migrations/fub/activity-imports/{id}/observations/{rowid}/fields/raw?plan_id={plan}")})}).collect::<Vec<_>>();
    let last = rows
        .get(items.len().saturating_sub(1))
        .map(|r| r.get::<Uuid, _>("id"));
    finish(
        key,
        ctx.organization_id,
        id,
        &purpose,
        items,
        rows.len(),
        last,
        "items",
    )
}

//! Bounded admin review reads for the D-078 admission origin.
use super::{
    core_change_store, history_capture_store, people_admission_store as s, store, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
const SUMMARY_BYTES: usize = 128 * 1024;
const PAGE_BYTES: usize = 256 * 1024;
const FIELD_BYTES: usize = 16 * 1024;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub parent_import_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub disposition: Option<String>,
}
fn limit(p: &Page, max: u16) -> Result<i64, MigrationError> {
    let n = p.limit.unwrap_or(max);
    if n == 0
        || n > max
        || p.cursor.as_ref().is_some_and(|v| v.len() > 4096)
        || p.disposition.as_deref().is_some_and(|v| {
            !matches!(
                v,
                "eligible"
                    | "already_imported"
                    | "already_admitted"
                    | "excluded_original"
                    | "held_baseline_gap"
                    | "held_evidence_gap"
                    | "held_mapping_gap"
                    | "held_identity"
                    | "held_target"
                    | "settled"
                    | "cancelled"
            )
        })
    {
        Err(MigrationError::InvalidInput)
    } else {
        Ok(n.into())
    }
}
async fn begin<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<Transaction<'a, Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    store::require_admin(&mut tx, ctx).await?;
    Ok(tx)
}
async fn resource(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    let row = sqlx::query(
        "SELECT * FROM migration_people_admission WHERE id=$1 AND organization_id=$2 FOR SHARE",
    )
    .bind(id)
    .bind(ctx.organization_id.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if row.get::<String, _>("engine_version") != s::ENGINE {
        return Err(MigrationError::ReleaseNotReady);
    }
    let parent =
        history_capture_store::parent(conn, ctx.organization_id, row.get("parent_import_id"))
            .await?;
    if parent.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(row.get("parent_plan_id"))
        || parent.get::<i64, _>("source_account_id") != row.get::<i64, _>("source_account_id")
        || parent.get::<i64, _>("workspace_revision") != row.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let report = sqlx::query(
        "SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2",
    )
    .bind(row.get::<Uuid, _>("report_id"))
    .bind(ctx.organization_id.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if report.get::<String, _>("state") != "completed"
        || report.get::<Option<Uuid>, _>("output_revision").is_none()
        || report.get::<Uuid, _>("parent_import_id") != row.get::<Uuid, _>("parent_import_id")
        || report.get::<Uuid, _>("parent_plan_id") != row.get::<Uuid, _>("parent_plan_id")
        || report.get::<Uuid, _>("newer_snapshot_id") != row.get::<Uuid, _>("newer_snapshot_id")
        || report.get::<i64, _>("newer_sequence") != row.get::<i64, _>("newer_sequence")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    core_change_store::validate(conn, key, ctx.organization_id, &report).await?;
    Ok(row)
}
async fn plan(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    id: Uuid,
    want: Option<Uuid>,
) -> Result<PgRow, MigrationError> {
    let r=sqlx::query("SELECT * FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id=$3) ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).bind(want).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    if r.get::<String, _>("state") == "building"
        || r.get::<Option<DateTime<Utc>>, _>("sealed_at").is_none()
        || r.get::<Option<Vec<u8>>, _>("digest")
            .is_none_or(|v| v.len() != 32)
    {
        return Err(MigrationError::Conflict);
    }
    Ok(r)
}
async fn finish(
    tx: Transaction<'_, Postgres>,
    value: Value,
    max: usize,
) -> Result<Value, MigrationError> {
    if serde_json::to_vec(&value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > max
    {
        return Err(MigrationError::StorageLimit);
    }
    tx.commit().await?;
    Ok(value)
}
fn overview(r: &PgRow) -> Value {
    json!({"id":r.get::<Uuid,_>("id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"report_id":r.get::<Uuid,_>("report_id"),"state":r.get::<String,_>("state"),"lifecycle_revision":r.get::<i64,_>("lifecycle_revision").to_string(),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":r.get::<i64,_>("newer_sequence").to_string(),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"progress":{"settled_items":r.get::<i64,_>("settled_items").to_string()},"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string()})
}
fn projection(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    admission: Uuid,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    s::open(
        key,
        ctx.organization_id,
        admission,
        row.get("id"),
        "projection",
        &row.get::<Vec<u8>, _>("projection_nonce"),
        &row.get::<Vec<u8>, _>("projection_ciphertext"),
    )
}
fn summary(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    admission: Uuid,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"source_id":row.get::<Option<String>,_>("source_id"),"prospective_person_id":row.get::<Uuid,_>("prospective_person_id"),"disposition":row.get::<String,_>("disposition"),"settled_at":row.get::<Option<DateTime<Utc>>,_>("settled_at"),"plan_id":row.get::<Uuid,_>("plan_id"),"projection":projection(key,ctx,admission,row)?}),
    )
}
pub async fn list(
    pool: &PgPool,
    _key: &RawPayloadKey,
    ctx: &CommandContext,
    p: Page,
) -> Result<Value, MigrationError> {
    let parent = p.parent_import_id.ok_or(MigrationError::InvalidInput)?;
    let n = limit(&p, 20)?;
    if p.plan_id.is_some() || p.disposition.is_some() || p.cursor.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    history_capture_store::parent(&mut tx, ctx.organization_id, parent).await?;
    let rows=sqlx::query("SELECT * FROM migration_people_admission WHERE organization_id=$1 AND parent_import_id=$2 ORDER BY created_at DESC,id DESC LIMIT $3").bind(ctx.organization_id.0).bind(parent).bind(n).fetch_all(&mut *tx).await?;
    finish(tx,json!({"admissions":rows.iter().map(overview).collect::<Vec<_>>(),"next_cursor":Value::Null}),SUMMARY_BYTES).await
}
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = resource(&mut tx, key, ctx, id).await?;
    let mut v = overview(&r);
    if let Some(p)=sqlx::query("SELECT * FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?{if p.get::<Option<DateTime<Utc>>,_>("sealed_at").is_some(){let d:Vec<u8>=p.get("digest");v["plan"]=json!({"id":p.get::<Uuid,_>("id"),"revision":p.get::<i64,_>("revision").to_string(),"digest":d.iter().map(|b|format!("{b:02x}")).collect::<String>(),"expires_at":p.get::<Option<DateTime<Utc>>,_>("expires_at"),"counts":{"total":p.get::<i64,_>("total_count").to_string(),"eligible":p.get::<i64,_>("eligible_count").to_string(),"already_imported":p.get::<i64,_>("already_imported_count").to_string(),"already_admitted":p.get::<i64,_>("already_admitted_count").to_string(),"excluded_original":p.get::<i64,_>("excluded_original_count").to_string(),"held":p.get::<i64,_>("held_count").to_string(),"intended_contacts":p.get::<i64,_>("intended_contact_count").to_string()}})}}
    let state = r.get::<String, _>("state");
    let initiator = r.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0;
    v["actions"] = json!({"confirm":initiator&&state=="ready"&&v["plan"]["counts"]["eligible"]!="0","repreview":initiator&&state=="ready","retry":initiator&&state=="paused","cancel":!matches!(state.as_str(),"completed"|"cancelled")});
    finish(tx, v, SUMMARY_BYTES).await
}
pub async fn items(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.parent_import_id.is_some() || p.cursor.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let plan = plan(&mut tx, ctx, id, p.plan_id).await?;
    let rows=sqlx::query("SELECT * FROM migration_people_admission_item WHERE admission_id=$1 AND organization_id=$2 AND plan_id=$3 AND ($4::text IS NULL OR disposition=$4) ORDER BY id LIMIT $5").bind(id).bind(ctx.organization_id.0).bind(plan.get::<Uuid,_>("id")).bind(p.disposition).bind(n).fetch_all(&mut *tx).await?;
    let rows = rows
        .iter()
        .map(|r| summary(key, ctx, id, r))
        .collect::<Result<Vec<_>, _>>()?;
    finish(tx,json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"items":rows,"next_cursor":Value::Null}),PAGE_BYTES).await
}
async fn item_row(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
) -> Result<(PgRow, PgRow), MigrationError> {
    let r=sqlx::query("SELECT * FROM migration_people_admission_item WHERE id=$1 AND admission_id=$2 AND organization_id=$3").bind(item).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    if r.get::<i64, _>("item_byte_bound") > s::ITEM_LIMIT {
        return Err(MigrationError::StorageLimit);
    }
    let p = plan(conn, ctx, id, Some(r.get("plan_id"))).await?;
    Ok((r, p))
}
pub async fn item(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (r, p) = item_row(&mut tx, ctx, id, item).await?;
    let mut v = summary(key, ctx, id, &r)?;
    v["plan_revision"] = json!(p.get::<i64, _>("revision").to_string());
    finish(tx, v, SUMMARY_BYTES).await
}
pub async fn contacts(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.parent_import_id.is_some()
        || p.plan_id.is_some()
        || p.disposition.is_some()
        || p.cursor.is_some()
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (_, plan) = item_row(&mut tx, ctx, id, item).await?;
    let rows=sqlx::query("SELECT * FROM migration_people_admission_contact WHERE admission_id=$1 AND organization_id=$2 AND item_id=$3 ORDER BY kind,import_order,id LIMIT $4").bind(id).bind(ctx.organization_id.0).bind(item).bind(n).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    for r in rows {
        let value: Value = s::open(
            key,
            ctx.organization_id,
            id,
            r.get("id"),
            "contact",
            &r.get::<Vec<u8>, _>("value_nonce"),
            &r.get::<Vec<u8>, _>("value_ciphertext"),
        )?;
        values.push(json!({"id":r.get::<Uuid,_>("id"),"kind":r.get::<String,_>("kind"),"import_order":r.get::<i32,_>("import_order").to_string(),"primary":r.get::<bool,_>("primary_contact"),"value":value}))
    }
    finish(tx,json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"contacts":values,"next_cursor":Value::Null}),PAGE_BYTES).await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.parent_import_id.is_some()
        || p.plan_id.is_some()
        || p.disposition.is_some()
        || p.cursor.is_some()
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let rows=sqlx::query("SELECT id,item_id,person_id,source_id,disposition,committed_at FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 ORDER BY committed_at,id LIMIT $3").bind(id).bind(ctx.organization_id.0).bind(n).fetch_all(&mut *tx).await?;
    let values=rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"item_id":r.get::<Uuid,_>("item_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"source_id":r.get::<String,_>("source_id"),"disposition":r.get::<String,_>("disposition"),"committed_at":r.get::<DateTime<Utc>,_>("committed_at")})).collect::<Vec<_>>();
    finish(
        tx,
        json!({"results":values,"next_cursor":Value::Null}),
        PAGE_BYTES,
    )
    .await
}
fn prefix(t: &str, n: usize) -> &str {
    let mut n = t.len().min(n);
    while !t.is_char_boundary(n) {
        n -= 1
    }
    &t[..n]
}
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    (id, item, side, field): (Uuid, Uuid, String, String),
    p: Page,
) -> Result<Value, MigrationError> {
    if side != "proposed"
        || !matches!(field.as_str(), "first_name" | "last_name")
        || p.parent_import_id.is_some()
        || p.plan_id.is_some()
        || p.disposition.is_some()
        || p.cursor.is_some()
    {
        return Err(MigrationError::InvalidInput);
    }
    let n = usize::from(p.limit.unwrap_or(FIELD_BYTES as u16));
    if n == 0 || n > FIELD_BYTES {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (r, plan) = item_row(&mut tx, ctx, id, item).await?;
    let p = projection(key, ctx, id, &r)?;
    let text = p[&field].as_str().ok_or(MigrationError::NotFound)?;
    finish(tx,json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"side":side,"field":field,"offset":"0","total_bytes":text.len().to_string(),"fragment":prefix(text,n),"next_cursor":Value::Null}),SUMMARY_BYTES).await
}
async fn provenance_row(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    person: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT p.*,a.parent_import_id,a.parent_plan_id,a.newer_snapshot_id,a.original_snapshot_id FROM person_admission_provenance p JOIN migration_people_admission a ON a.id=p.admission_id AND a.organization_id=p.organization_id WHERE p.organization_id=$1 AND p.person_id=$2").bind(ctx.organization_id.0).bind(person).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
pub async fn admission_provenance(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = provenance_row(&mut tx, ctx, person).await?;
    let v: Value = s::open(
        key,
        ctx.organization_id,
        r.get("admission_id"),
        r.get("item_id"),
        "provenance",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )?;
    finish(tx,json!({"person_id":person,"admission_id":r.get::<Uuid,_>("admission_id"),"item_id":r.get::<Uuid,_>("item_id"),"result_id":r.get::<Uuid,_>("result_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"original_snapshot_id":r.get::<Uuid,_>("original_snapshot_id"),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"coverage":"core_only_notes_tasks_metadata_history_deferred","provenance":v}),SUMMARY_BYTES).await
}
pub async fn admission_provenance_contacts(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.cursor.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    let p = provenance_row(&mut tx, ctx, person).await?;
    let rows=sqlx::query("SELECT * FROM migration_people_admission_contact WHERE admission_id=$1 AND item_id=$2 AND organization_id=$3 ORDER BY kind,import_order,id LIMIT $4").bind(p.get::<Uuid,_>("admission_id")).bind(p.get::<Uuid,_>("item_id")).bind(ctx.organization_id.0).bind(n).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    for r in rows {
        let v: Value = s::open(
            key,
            ctx.organization_id,
            p.get("admission_id"),
            r.get("id"),
            "contact",
            &r.get::<Vec<u8>, _>("value_nonce"),
            &r.get::<Vec<u8>, _>("value_ciphertext"),
        )?;
        values.push(json!({"kind":r.get::<String,_>("kind"),"import_order":r.get::<i32,_>("import_order").to_string(),"primary":r.get::<bool,_>("primary_contact"),"value":v}))
    }
    finish(
        tx,
        json!({"items":values,"next_cursor":Value::Null}),
        PAGE_BYTES,
    )
    .await
}
pub async fn admission_provenance_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    field: String,
) -> Result<Value, MigrationError> {
    if !matches!(field.as_str(), "source_id" | "coverage") {
        return Err(MigrationError::InvalidInput);
    }
    let v = admission_provenance(pool, key, ctx, person).await?;
    let t = if field == "source_id" {
        v["provenance"]["source_id"]
            .as_str()
            .ok_or(MigrationError::NotFound)?
    } else {
        "core_only_notes_tasks_metadata_history_deferred"
    };
    Ok(
        json!({"field":field,"offset":"0","total_bytes":t.len().to_string(),"fragment":t,"next_cursor":Value::Null}),
    )
}

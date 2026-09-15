//! Bounded admin review reads for the D-078 admission origin.
use super::{history_capture_store, people_admission_store as s, store, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
const CURSOR_LIMIT: usize = 4096;
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub(super) actor: Uuid,
    pub(super) owner: Uuid,
    pub(super) endpoint: String,
    pub(super) plan: Option<Uuid>,
    pub(super) revision: Option<i64>,
    pub(super) digest: Option<Vec<u8>>,
    pub(super) filter: Option<String>,
    pub(super) limit: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Position {
    pub(super) id: Uuid,
    pub(super) time: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    pub(super) binding: Binding,
    pub(super) last: Position,
    pub(super) upper: Option<Position>,
    pub(super) offset: usize,
}
pub(super) fn binding(
    ctx: &CommandContext,
    owner: Uuid,
    endpoint: &str,
    plan: Option<&PgRow>,
    filter: Option<String>,
    limit: i64,
) -> Binding {
    Binding {
        actor: ctx.actor_user_id.0,
        owner,
        endpoint: endpoint.into(),
        plan: plan.map(|v| v.get("id")),
        revision: plan.map(|v| v.get("revision")),
        digest: plan.map(|v| v.get("digest")),
        filter,
        limit,
    }
}
pub(super) fn decode(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    expected: &Binding,
    token: Option<&str>,
) -> Result<Option<Cursor>, MigrationError> {
    token
        .map(|token| {
            if token.len() > CURSOR_LIMIT {
                return Err(MigrationError::InvalidInput);
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(token)
                .map_err(|_| MigrationError::InvalidInput)?;
            if bytes.len() < 40 {
                return Err(MigrationError::InvalidInput);
            }
            let cursor: Cursor = s::open(
                key,
                ctx.organization_id,
                expected.owner,
                Uuid::nil(),
                "cursor",
                &bytes[..24],
                &bytes[24..],
            )
            .map_err(|_| MigrationError::InvalidInput)?;
            if cursor.binding != *expected {
                return Err(MigrationError::InvalidInput);
            }
            Ok(cursor)
        })
        .transpose()
}
pub(super) fn encode(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    binding: Binding,
    last: Position,
    upper: Option<Position>,
    offset: usize,
) -> Result<String, MigrationError> {
    let sealed = s::seal(
        key,
        ctx.organization_id,
        binding.owner,
        Uuid::nil(),
        "cursor",
        &Cursor {
            binding,
            last,
            upper,
            offset,
        },
    )?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    if token.len() > CURSOR_LIMIT {
        return Err(MigrationError::Crypto);
    }
    Ok(token)
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
    let _ = key;
    let parent =
        history_capture_store::parent(conn, ctx.organization_id, row.get("parent_import_id"))
            .await?;
    if row.get::<String, _>("engine_version")
        != if super::people_recovery::is_recovery(&row) {
            super::people_recovery::ENGINE
        } else {
            s::ENGINE
        }
        || parent.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(row.get("parent_plan_id"))
        || parent.get::<i64, _>("workspace_revision") != row.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }

    Ok(row)
}
async fn plan(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    id: Uuid,
    want: Option<Uuid>,
) -> Result<PgRow, MigrationError> {
    let r=sqlx::query("SELECT id,admission_id,organization_id,revision,state,digest,total_count,eligible_count,already_imported_count,already_admitted_count,excluded_original_count,held_count,intended_contact_count,prepared_bytes,recovery_candidate_count,recovery_unassigned_count,sealed_at,expires_at FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id=$3) ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).bind(want).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
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
    json!({"id":r.get::<Uuid,_>("id"),"mode":r.get::<String,_>("mode"),"confirmed_admission_plan_id":r.get::<Option<Uuid>,_>("confirmed_admission_plan_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"report_id":r.get::<Uuid,_>("report_id"),"state":r.get::<String,_>("state"),"lifecycle_revision":r.get::<i64,_>("lifecycle_revision").to_string(),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":r.get::<i64,_>("newer_sequence").to_string(),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"progress":{"settled_items":r.get::<i64,_>("settled_items").to_string()},"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string()})
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
        json!({"id":row.get::<Uuid,_>("id"),"source_id":row.get::<Option<String>,_>("source_id"),"prospective_person_id":row.get::<Uuid,_>("prospective_person_id"),"disposition":row.try_get::<String,_>("display_disposition").unwrap_or_else(|_|row.get("disposition")),"settled_at":row.get::<Option<DateTime<Utc>>,_>("settled_at"),"plan_id":row.get::<Uuid,_>("plan_id"),"projection":projection(key,ctx,admission,row)?}),
    )
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    p: Page,
) -> Result<Value, MigrationError> {
    let parent = p.parent_import_id.ok_or(MigrationError::InvalidInput)?;
    let n = limit(&p, 20)?;
    if p.plan_id.is_some() || p.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    history_capture_store::parent(&mut tx, ctx.organization_id, parent).await?;
    let bind = binding(ctx, parent, "list", None, None, n);
    let cursor = decode(key, ctx, &bind, p.cursor.as_deref())?;
    let rows=sqlx::query("SELECT * FROM migration_people_admission WHERE organization_id=$1 AND parent_import_id=$2 AND ($3::timestamptz IS NULL OR (created_at,id)<($3,$4)) ORDER BY created_at DESC,id DESC LIMIT $5")
        .bind(ctx.organization_id.0).bind(parent).bind(cursor.as_ref().and_then(|c|c.last.time)).bind(cursor.as_ref().map(|c|c.last.id)).bind(n+1).fetch_all(&mut *tx).await?;
    let count = rows.len().min(n as usize);
    let more = rows.len() > count;
    let next = if more {
        let r = &rows[count - 1];
        Some(encode(
            key,
            ctx,
            bind,
            Position {
                id: r.get("id"),
                time: Some(r.get("created_at")),
            },
            None,
            0,
        )?)
    } else {
        None
    };
    finish(
        tx,
        json!({"items":rows[..count].iter().map(overview).collect::<Vec<_>>(),"next_cursor":next}),
        SUMMARY_BYTES,
    )
    .await
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
    let original = sqlx::query(
        "SELECT started_at,completed_at FROM migration_snapshot WHERE id=$1 AND organization_id=$2",
    )
    .bind(r.get::<Uuid, _>("original_snapshot_id"))
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    v["source_boundary"] = json!({
        "original":{"snapshot_id":r.get::<Uuid,_>("original_snapshot_id"),"sequence":r.get::<i64,_>("original_sequence").to_string(),"started_at":original.get::<Option<DateTime<Utc>>,_>("started_at"),"completed_at":original.get::<Option<DateTime<Utc>>,_>("completed_at")},
        "newer":{"snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"sequence":r.get::<i64,_>("newer_sequence").to_string(),"started_at":r.get::<DateTime<Utc>,_>("newer_started_at"),"completed_at":r.get::<DateTime<Utc>,_>("newer_completed_at")}
    });
    v["recovery"] = if super::people_recovery::is_recovery(&r) {
        super::people_recovery::frozen(&r)
    } else {
        Value::Null
    };
    v["coverage"] = json!({"covered_families":["Person core values","contacts","initial stage","initial assignment","admission provenance"],"deferred_families":["notes","tasks","tags","custom fields","historical events/calls/texts","later refresh"],"review_hold":true});
    if let Some(p)=sqlx::query("SELECT id,admission_id,organization_id,revision,state,digest,total_count,eligible_count,already_imported_count,already_admitted_count,excluded_original_count,held_count,intended_contact_count,prepared_bytes,recovery_candidate_count,recovery_unassigned_count,sealed_at,expires_at FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?{if p.get::<Option<DateTime<Utc>>,_>("sealed_at").is_some(){let d:Vec<u8>=p.get("digest");v["plan"]=json!({"id":p.get::<Uuid,_>("id"),"revision":p.get::<i64,_>("revision").to_string(),"digest":d.iter().map(|b|format!("{b:02x}")).collect::<String>(),"expires_at":p.get::<Option<DateTime<Utc>>,_>("expires_at"),"counts":{"total":p.get::<i64,_>("total_count").to_string(),"eligible":p.get::<i64,_>("eligible_count").to_string(),"already_imported":p.get::<i64,_>("already_imported_count").to_string(),"already_admitted":p.get::<i64,_>("already_admitted_count").to_string(),"excluded_original":p.get::<i64,_>("excluded_original_count").to_string(),"held":p.get::<i64,_>("held_count").to_string(),"intended_contacts":p.get::<i64,_>("intended_contact_count").to_string(),"recovery_candidates":p.get::<i64,_>("recovery_candidate_count").to_string(),"recovery_unassigned":p.get::<i64,_>("recovery_unassigned_count").to_string()}})}}
    if super::people_recovery::is_recovery(&r) {
        v["coverage"]["follow_on"] =
            super::people_recovery::coverage::coverage(&mut tx, ctx, &r).await?;
        v["coverage"]["deferred_families"] = json!([]);
    }
    let state = r.get::<String, _>("state");
    let initiator = r.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0;
    v["actions"] = json!({"confirm":initiator&&state=="ready"&&v["plan"]["counts"]["eligible"]!="0","repreview":initiator&&state=="ready"&&!super::people_recovery::is_recovery(&r),"retry":initiator&&state=="paused"&&r.get::<Option<String>,_>("pause_reason").as_deref()!=Some("awaiting_mapping_choices"),"cancel":!matches!(state.as_str(),"completed"|"cancelled")});
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
    if p.parent_import_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    let run = resource(&mut tx, key, ctx, id).await?;
    let plan = plan(&mut tx, ctx, id, p.plan_id).await?;
    let plan_id = plan.get::<Uuid, _>("id");
    let bind = binding(ctx, id, "items", Some(&plan), p.disposition.clone(), n);
    let cursor = decode(key, ctx, &bind, p.cursor.as_deref())?;
    let after = cursor.as_ref().map(|c| c.last.id);
    // The run is locked FOR SHARE for this read. Resolve virtual cancellation
    // before SQL so a CASE/join cannot turn a sparse page into a full-plan scan.
    // Cancelled display combines two disjoint, independently bounded ranges.
    let mut ids: Vec<Uuid> = if let Some(disposition) = p.disposition.as_deref() {
        let cancelled = run.get::<String, _>("state") == "cancelled";
        let mut filters = vec![(disposition, None)];
        if cancelled && disposition == "cancelled" {
            filters.push(("eligible", Some(true)));
        } else if cancelled && disposition == "eligible" {
            // A cancelled run has no eligible future work. Settled items have
            // their own immutable result disposition; remaining eligible items
            // are displayed as cancelled without rewriting the sealed plan.
            filters.clear();
        }
        let mut ids = Vec::new();
        for (stored_disposition, unsettled) in filters {
            let page = sqlx::query_scalar::<_, Uuid>("SELECT i.id FROM migration_people_admission_item i WHERE i.admission_id=$1 AND i.organization_id=$2 AND i.plan_id=$3 AND i.disposition=$4 AND ($5::uuid IS NULL OR i.id>$5) AND ($7::boolean IS NULL OR (i.settled_at IS NULL)=$7) ORDER BY i.id LIMIT $6")
                .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(stored_disposition).bind(after).bind(n+1).bind(unsettled).fetch_all(&mut *tx).await?;
            ids.extend(page);
        }
        ids
    } else {
        sqlx::query_scalar::<_, Uuid>("SELECT i.id FROM migration_people_admission_item i WHERE i.admission_id=$1 AND i.organization_id=$2 AND i.plan_id=$3 AND ($4::uuid IS NULL OR i.id>$4) ORDER BY i.id LIMIT $5")
            .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(after).bind(n+1).fetch_all(&mut *tx).await?
    };
    ids.sort_unstable();
    ids.dedup();
    ids.truncate((n + 1) as usize);
    let mut values = Vec::new();
    let mut bytes = 0;
    let mut last = None;
    for descriptor in ids.iter().take(n as usize) {
        let (row, _) = item_row(&mut tx, ctx, id, *descriptor).await?;
        let value = summary(key, ctx, id, &row)?;
        let len = encoded_len(&value)?;
        if !values.is_empty() && bytes + len > PAGE_BYTES - 8192 {
            break;
        }
        if len > PAGE_BYTES - 8192 {
            return Err(MigrationError::StorageLimit);
        }
        bytes += len;
        last = Some(Position {
            id: row.get("id"),
            time: None,
        });
        values.push(value);
    }
    let next = if values.len() < ids.len() {
        Some(encode(
            key,
            ctx,
            bind,
            last.ok_or(MigrationError::Crypto)?,
            None,
            0,
        )?)
    } else {
        None
    };
    finish(tx,json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"items":values,"next_cursor":next}),PAGE_BYTES).await
}
async fn item_row(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
) -> Result<(PgRow, PgRow), MigrationError> {
    let descriptor=sqlx::query("SELECT item_byte_bound,octet_length(projection_nonce) nonce_len,octet_length(projection_ciphertext) cipher_len FROM migration_people_admission_item WHERE id=$1 AND admission_id=$2 AND organization_id=$3")
        .bind(item).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    if descriptor.get::<i64, _>("item_byte_bound") > s::ITEM_LIMIT
        || descriptor.get::<i32, _>("nonce_len") != 24
        || descriptor.get::<i32, _>("cipher_len") > 32768
    {
        return Err(MigrationError::StorageLimit);
    }
    let row=sqlx::query("SELECT i.id,i.plan_id,i.source_id,i.prospective_person_id,i.disposition,i.settled_at,i.projection_nonce,i.projection_ciphertext,CASE WHEN a.state='cancelled' AND i.disposition='eligible' AND i.settled_at IS NULL THEN 'cancelled' ELSE i.disposition END display_disposition FROM migration_people_admission_item i JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id WHERE i.id=$1 AND i.admission_id=$2 AND i.organization_id=$3")
        .bind(item).bind(id).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    let plan = plan(conn, ctx, id, Some(row.get("plan_id"))).await?;
    Ok((row, plan))
}
fn encoded_len(v: &Value) -> Result<usize, MigrationError> {
    Ok(serde_json::to_vec(v)
        .map_err(|_| MigrationError::Crypto)?
        .len())
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
    v["fields"] = field_summaries(&item_fields(&mut tx, key, ctx, id, &r).await?)?;
    v["held_reasons"] = if v["disposition"]
        .as_str()
        .is_some_and(|d| d.starts_with("held_"))
    {
        json!([v["disposition"].clone()])
    } else {
        json!([])
    };
    finish(tx, v, SUMMARY_BYTES).await
}
async fn item_fields(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    let item: Uuid = row.get("id");
    let descriptor=sqlx::query("SELECT octet_length(provenance_nonce) nonce_len,octet_length(provenance_ciphertext) cipher_len FROM migration_people_admission_item WHERE id=$1 AND admission_id=$2 AND organization_id=$3")
        .bind(item).bind(id).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    if descriptor.get::<i32, _>("nonce_len") != 24
        || i64::from(descriptor.get::<i32, _>("cipher_len")) > s::ITEM_LIMIT + 16
    {
        return Err(MigrationError::StorageLimit);
    }
    let stored=sqlx::query("SELECT provenance_nonce,provenance_ciphertext FROM migration_people_admission_item WHERE id=$1 AND admission_id=$2 AND organization_id=$3")
        .bind(item).bind(id).bind(ctx.organization_id.0).fetch_one(conn).await?;
    let value: Value = s::open(
        key,
        ctx.organization_id,
        id,
        item,
        "provenance",
        stored.get("provenance_nonce"),
        stored.get("provenance_ciphertext"),
    )?;
    let mut fields = value["fields"].as_object().cloned().unwrap_or_default();
    let proposed = projection(key, ctx, id, row)?;
    for name in ["first_name", "last_name"] {
        if let Some(value) = proposed[name].as_str() {
            fields.insert(name.into(), json!(value));
        }
    }
    Ok(Value::Object(fields))
}
pub async fn contacts(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (_, plan) = item_row(&mut tx, ctx, id, item).await?;
    let page = contact_page(&mut tx, key, ctx, id, item, &plan, p, "contacts").await?;
    finish(tx, page, PAGE_BYTES).await
}
#[allow(clippy::too_many_arguments)] // Explicit scoped identity and cryptographic receipt/page inputs.
async fn contact_page(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
    plan: &PgRow,
    p: Page,
    endpoint: &str,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.parent_import_id.is_some() || p.plan_id.is_some() || p.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let bind = binding(ctx, id, &format!("{endpoint}/{item}"), Some(plan), None, n);
    let cursor = decode(key, ctx, &bind, p.cursor.as_deref())?;
    // A UUID keyset preserves all original ordering metadata in each row.
    let rows=sqlx::query("SELECT id,kind,import_order,primary_contact,octet_length(value_nonce) nonce_len,octet_length(value_ciphertext) cipher_len FROM migration_people_admission_contact WHERE admission_id=$1 AND organization_id=$2 AND item_id=$3 AND ($4::uuid IS NULL OR (kind,import_order,id)>(SELECT kind,import_order,id FROM migration_people_admission_contact WHERE id=$4 AND item_id=$3 AND organization_id=$2)) ORDER BY kind,import_order,id LIMIT $5")
        .bind(id).bind(ctx.organization_id.0).bind(item).bind(cursor.as_ref().map(|c|c.last.id)).bind(n+1).fetch_all(&mut *conn).await?;
    let mut values = Vec::new();
    let mut bytes = 0;
    let mut last = None;
    for r in rows.iter().take(n as usize) {
        if r.get::<i32, _>("nonce_len") != 24 || r.get::<i32, _>("cipher_len") > 65536 {
            return Err(MigrationError::StorageLimit);
        }
        let c=sqlx::query("SELECT value_nonce,value_ciphertext FROM migration_people_admission_contact WHERE id=$1 AND item_id=$2 AND organization_id=$3")
            .bind(r.get::<Uuid,_>("id")).bind(item).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
        let value: Value = s::open(
            key,
            ctx.organization_id,
            id,
            r.get("id"),
            "contact",
            c.get("value_nonce"),
            c.get("value_ciphertext"),
        )?;
        let value = json!({"id":r.get::<Uuid,_>("id"),"kind":r.get::<String,_>("kind"),"import_order":r.get::<i32,_>("import_order").to_string(),"primary":r.get::<bool,_>("primary_contact"),"value":value});
        let len = encoded_len(&value)?;
        if !values.is_empty() && bytes + len > PAGE_BYTES - 8192 {
            break;
        }
        if len > PAGE_BYTES - 8192 {
            return Err(MigrationError::StorageLimit);
        }
        bytes += len;
        last = Some(Position {
            id: r.get("id"),
            time: None,
        });
        values.push(value);
    }
    let next = if values.len() < rows.len() {
        Some(encode(
            key,
            ctx,
            bind,
            last.ok_or(MigrationError::Crypto)?,
            None,
            0,
        )?)
    } else {
        None
    };
    Ok(
        json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"contacts":values,"next_cursor":next}),
    )
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    if p.parent_import_id.is_some() || p.plan_id.is_some() || p.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let bind = binding(ctx, id, "results", None, None, n);
    let cursor = decode(key, ctx, &bind, p.cursor.as_deref())?;
    let upper = if let Some(c) = &cursor {
        c.upper.clone()
    } else {
        sqlx::query("SELECT id,committed_at FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 ORDER BY committed_at DESC,id DESC LIMIT 1")
        .bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.map(|r|Position{id:r.get("id"),time:Some(r.get("committed_at"))})
    };
    let rows=sqlx::query("SELECT id,item_id,person_id,source_id,disposition,committed_at FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND ($3::timestamptz IS NULL OR (committed_at,id)>($3,$4)) AND ($5::timestamptz IS NOT NULL AND (committed_at,id)<=($5,$6)) ORDER BY committed_at,id LIMIT $7")
        .bind(id).bind(ctx.organization_id.0).bind(cursor.as_ref().and_then(|c|c.last.time)).bind(cursor.as_ref().map(|c|c.last.id)).bind(upper.as_ref().and_then(|p|p.time)).bind(upper.as_ref().map(|p|p.id)).bind(n+1).fetch_all(&mut *tx).await?;
    let count = rows.len().min(n as usize);
    let values=rows[..count].iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"item_id":r.get::<Uuid,_>("item_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"source_id":r.get::<String,_>("source_id"),"disposition":r.get::<String,_>("disposition"),"committed_at":r.get::<DateTime<Utc>,_>("committed_at")})).collect::<Vec<_>>();
    let next = if count < rows.len() {
        let r = &rows[count - 1];
        Some(encode(
            key,
            ctx,
            bind,
            Position {
                id: r.get("id"),
                time: Some(r.get("committed_at")),
            },
            upper,
            0,
        )?)
    } else {
        None
    };
    finish(tx, json!({"results":values,"next_cursor":next}), PAGE_BYTES).await
}
pub(super) fn prefix(t: &str, n: usize) -> &str {
    let mut n = t.len().min(n);
    while !t.is_char_boundary(n) {
        n -= 1
    }
    &t[..n]
}
pub(super) fn field_limit(p: &Page) -> Result<usize, MigrationError> {
    let n = usize::from(p.limit.unwrap_or(FIELD_BYTES as u16));
    if n == 0
        || n > FIELD_BYTES
        || p.parent_import_id.is_some()
        || p.plan_id.is_some()
        || p.disposition.is_some()
        || p.cursor.as_ref().is_some_and(|c| c.len() > CURSOR_LIMIT)
    {
        return Err(MigrationError::InvalidInput);
    }
    Ok(n)
}
pub(super) fn fragment(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bind: Binding,
    cursor: Option<&str>,
    text: &str,
    n: usize,
) -> Result<Value, MigrationError> {
    let cursor = decode(key, ctx, &bind, cursor)?;
    let offset = cursor.as_ref().map(|c| c.offset).unwrap_or(0);
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(MigrationError::InvalidInput);
    }
    let part = prefix(&text[offset..], n);
    if part.is_empty() && offset < text.len() {
        return Err(MigrationError::InvalidInput);
    }
    let end = offset + part.len();
    let next = if end < text.len() {
        Some(encode(
            key,
            ctx,
            bind,
            Position {
                id: Uuid::nil(),
                time: None,
            },
            None,
            end,
        )?)
    } else {
        None
    };
    Ok(
        json!({"offset":offset.to_string(),"total_bytes":text.len().to_string(),"fragment":part,"next_cursor":next}),
    )
}
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    (id, item, side, field): (Uuid, Uuid, String, String),
    p: Page,
) -> Result<Value, MigrationError> {
    let n = field_limit(&p)?;
    if side != "proposed" || field.is_empty() || field.len() > 2048 {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (r, plan) = item_row(&mut tx, ctx, id, item).await?;
    let value = item_fields(&mut tx, key, ctx, id, &r).await?;
    let text = value[&field].as_str().ok_or(MigrationError::NotFound)?;
    let bind = binding(
        ctx,
        id,
        &format!("field/{item}/{side}/{field}"),
        Some(&plan),
        None,
        n as i64,
    );
    let mut v = fragment(key, ctx, bind, p.cursor.as_deref(), text, n)?;
    v["plan_id"] = json!(plan.get::<Uuid, _>("id"));
    v["plan_revision"] = json!(plan.get::<i64, _>("revision").to_string());
    v["side"] = json!(side);
    v["field"] = json!(field);
    finish(tx, v, SUMMARY_BYTES).await
}
async fn provenance_row(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    person: Uuid,
) -> Result<PgRow, MigrationError> {
    let descriptor=sqlx::query("SELECT p.id,octet_length(p.nonce) nonce_len,octet_length(p.ciphertext) cipher_len FROM person_admission_provenance p JOIN person n ON n.id=p.person_id AND n.organization_id=p.organization_id WHERE p.organization_id=$1 AND p.person_id=$2")
        .bind(ctx.organization_id.0).bind(person).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    if descriptor.get::<i32, _>("nonce_len") != 24
        || i64::from(descriptor.get::<i32, _>("cipher_len")) > s::ITEM_LIMIT + 16
    {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("SELECT p.*,a.parent_import_id,a.parent_plan_id,a.newer_snapshot_id,a.original_snapshot_id FROM person_admission_provenance p JOIN migration_people_admission a ON a.id=p.admission_id AND a.organization_id=p.organization_id WHERE p.organization_id=$1 AND p.person_id=$2")
        .bind(ctx.organization_id.0).bind(person).fetch_one(conn).await.map_err(Into::into)
}
fn provenance(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    s::open(
        key,
        ctx.organization_id,
        r.get("admission_id"),
        r.get("item_id"),
        "provenance",
        r.get("nonce"),
        r.get("ciphertext"),
    )
}
pub(super) fn field_summaries(raw: &Value) -> Result<Value, MigrationError> {
    let Some(fields) = raw.as_object() else {
        return Ok(json!({}));
    };
    // Retained capture's property count and key lengths are bounded by its raw
    // page ceiling; each listing must still fit a review response.
    let mut result = serde_json::Map::new();
    let mut bytes = 2;
    for (name, value) in fields {
        if name.len() > 2048 {
            return Err(MigrationError::StorageLimit);
        }
        let text = value.as_str().ok_or(MigrationError::Crypto)?;
        let entry = json!({"prefix":prefix(text,256),"total_bytes":text.len().to_string(),"truncated":text.len()>256});
        bytes += encoded_len(&json!({name:entry.clone()}))?;
        result.insert(name.clone(), entry);
        if bytes > SUMMARY_BYTES - 16384 {
            return Err(MigrationError::StorageLimit);
        }
    }
    Ok(Value::Object(result))
}
pub async fn admission_provenance(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = provenance_row(&mut tx, ctx, person).await?;
    resource(&mut tx, key, ctx, r.get("admission_id")).await?;
    let mut v = provenance(key, ctx, &r)?;
    if let Some(raw) = v.get("fields").cloned() {
        v["fields"] = field_summaries(&raw)?;
    }
    if let Some(raw) = v.get("source_fields").cloned() {
        v["source_fields"] = field_summaries(&raw)?;
    }
    finish(tx,json!({"person_id":person,"admission_id":r.get::<Uuid,_>("admission_id"),"item_id":r.get::<Uuid,_>("item_id"),"result_id":r.get::<Uuid,_>("result_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"original_snapshot_id":r.get::<Uuid,_>("original_snapshot_id"),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"coverage":"core_only_notes_tasks_metadata_history_deferred","provenance":v}),SUMMARY_BYTES).await
}
pub async fn admission_provenance_contacts(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let r = provenance_row(&mut tx, ctx, person).await?;
    let id = r.get("admission_id");
    let item = r.get("item_id");
    resource(&mut tx, key, ctx, id).await?;
    let (_, plan) = item_row(&mut tx, ctx, id, item).await?;
    let mut v = contact_page(
        &mut tx,
        key,
        ctx,
        id,
        item,
        &plan,
        p,
        &format!("provenance-contacts/{person}"),
    )
    .await?;
    v["items"] = v["contacts"].take();
    v.as_object_mut()
        .ok_or(MigrationError::Crypto)?
        .remove("contacts");
    finish(tx, v, PAGE_BYTES).await
}
pub async fn admission_provenance_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    person: Uuid,
    field: String,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = field_limit(&p)?;
    if field.is_empty() || field.len() > 2048 {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    let r = provenance_row(&mut tx, ctx, person).await?;
    let id = r.get("admission_id");
    resource(&mut tx, key, ctx, id).await?;
    let (_, plan) = item_row(&mut tx, ctx, id, r.get("item_id")).await?;
    let value = provenance(key, ctx, &r)?;
    let text = value["fields"][&field]
        .as_str()
        .or_else(|| value["source_fields"][&field].as_str())
        .or_else(|| {
            if matches!(field.as_str(), "source_id" | "coverage") {
                value[&field].as_str()
            } else {
                None
            }
        })
        .ok_or(MigrationError::NotFound)?;
    let bind = binding(
        ctx,
        id,
        &format!("provenance-field/{person}/{field}"),
        Some(&plan),
        None,
        n as i64,
    );
    let mut v = fragment(key, ctx, bind, p.cursor.as_deref(), text, n)?;
    v["field"] = json!(field);
    finish(tx, v, SUMMARY_BYTES).await
}

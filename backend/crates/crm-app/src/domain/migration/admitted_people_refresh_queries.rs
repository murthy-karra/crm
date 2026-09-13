//! Bounded, sealed-plan reads. Authority and workspace barriers survive response assembly.
use super::{
    admitted_people_refresh_store as s, core_change_store, history_capture_store, store,
    MigrationError,
};
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
    pub admission_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub disposition: Option<String>,
}
fn limit(page: &Page, maximum: u16) -> Result<i64, MigrationError> {
    let value = page.limit.unwrap_or(maximum);
    if value == 0
        || value > maximum
        || page.cursor.as_ref().is_some_and(|v| v.len() > CURSOR_LIMIT)
        || page.disposition.as_deref().is_some_and(|v| {
            !matches!(
                v,
                "eligible"
                    | "already_current"
                    | "held_local_change"
                    | "held_evidence_gap"
                    | "held_mapping_gap"
                    | "held_target_missing"
                    | "held_original_hold"
                    | "excluded_source_only"
                    | "not_seen_again"
                    | "settled"
                    | "settled_noop"
                    | "held_stale"
                    | "cancelled"
            )
        })
    {
        return Err(MigrationError::InvalidInput);
    }
    Ok(i64::from(value))
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    actor: Uuid,
    owner: Uuid,
    endpoint: String,
    plan: Option<Uuid>,
    revision: Option<i64>,
    digest: Option<Vec<u8>>,
    filter: Option<String>,
    limit: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    id: Uuid,
    time: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    binding: Binding,
    last: Position,
    upper: Option<Position>,
    offset: usize,
}
fn binding(
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
fn decode(
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
fn encode(
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
    sqlx::query("SET LOCAL idle_in_transaction_session_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    store::require_admin(&mut tx, ctx).await?;
    sqlx::query("SELECT id FROM organization WHERE id=$1 FOR SHARE")
        .bind(ctx.organization_id.0)
        .fetch_one(&mut *tx)
        .await?;
    Ok(tx)
}
async fn resource(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    let row = sqlx::query(
        "SELECT * FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2 FOR SHARE",
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
async fn selected_plan(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    refresh: Uuid,
    requested: Option<Uuid>,
) -> Result<PgRow, MigrationError> {
    let row=sqlx::query("SELECT * FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id=$3) ORDER BY revision DESC LIMIT 1").bind(refresh).bind(ctx.organization_id.0).bind(requested).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    if row.get::<String, _>("state") == "building"
        || row.get::<Option<DateTime<Utc>>, _>("sealed_at").is_none()
    {
        return Err(MigrationError::Conflict);
    }
    if row
        .get::<Option<Vec<u8>>, _>("digest")
        .is_none_or(|v| v.len() != 32)
    {
        return Err(MigrationError::Crypto);
    }
    Ok(row)
}
async fn finish(
    tx: Transaction<'_, Postgres>,
    value: Value,
    maximum: usize,
) -> Result<Value, MigrationError> {
    if serde_json::to_vec(&value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > maximum
    {
        return Err(MigrationError::StorageLimit);
    }
    tx.commit().await?;
    Ok(value)
}
fn push_bounded(
    output: &mut Vec<Value>,
    value: Value,
    used: &mut usize,
    maximum: usize,
) -> Result<bool, MigrationError> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        + 1;
    if used.saturating_add(bytes) > maximum {
        if output.is_empty() {
            return Err(MigrationError::StorageLimit);
        }
        return Ok(false);
    }
    *used += bytes;
    output.push(value);
    Ok(true)
}
fn position(row: &PgRow, column: Option<&str>) -> Position {
    Position {
        id: row.get("id"),
        time: column.map(|column| row.get(column)),
    }
}
fn instructions(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    refresh: Uuid,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    let ciphertext: Vec<u8> = row.get("instructions_ciphertext");
    if ciphertext.len() > 1024 {
        return Err(MigrationError::Crypto);
    }
    let values: Vec<String> = s::open(
        key,
        ctx.organization_id,
        refresh,
        row.get("id"),
        "instructions",
        &row.get::<Vec<u8>, _>("instructions_nonce"),
        &ciphertext,
    )?;
    if values.len() > 6
        || values.iter().any(|v| {
            !matches!(
                v.as_str(),
                "firstName" | "lastName" | "emails" | "phones" | "stage" | "assignment"
            )
        })
    {
        return Err(MigrationError::Crypto);
    }
    Ok(json!(values))
}
fn item_summary(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    refresh: Uuid,
    row: &PgRow,
) -> Result<Value, MigrationError> {
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"source_id":row.get::<Option<String>,_>("source_id"),"person_id":row.get::<Option<Uuid>,_>("person_id"),"disposition":row.get::<String,_>("display_disposition"),"settled_at":row.get::<Option<DateTime<Utc>>,_>("settled_at"),"clear_counts":{"names":row.get::<i64,_>("name_clear_count").to_string(),"assignments":row.get::<i64,_>("assignment_clear_count").to_string(),"contacts":row.get::<i64,_>("contact_removal_count").to_string()},"no_instruction":instructions(key,ctx,refresh,row)?}),
    )
}
fn overview(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<Uuid,_>("id"),"admission_id":r.get::<Uuid,_>("admission_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"report_id":r.get::<Uuid,_>("report_id"),"state":r.get::<String,_>("state"),"lifecycle_revision":r.get::<i64,_>("lifecycle_revision").to_string(),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":r.get::<i64,_>("newer_sequence").to_string(),"created_at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updated_at":r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),"completed_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"progress":{"settled_items":r.get::<i64,_>("settled_items").to_string()},"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string()})
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0))]
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    page: Page,
) -> Result<Value, MigrationError> {
    let parent = page.admission_id.ok_or(MigrationError::InvalidInput)?;
    let count = limit(&page, 20)?;
    if page.disposition.is_some() || page.plan_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    // The list owner is the admission cohort, not its original import. Read
    // immutable terminal metadata first, then acquire the ordinary parent gate.
    let admission = sqlx::query("SELECT parent_import_id,parent_plan_id,source_account_id,workspace_revision FROM migration_people_admission WHERE id=$1 AND organization_id=$2 AND state IN ('completed','cancelled')")
        .bind(parent).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?
        .ok_or(MigrationError::NotFound)?;
    let original = history_capture_store::parent(
        &mut tx,
        ctx.organization_id,
        admission.get("parent_import_id"),
    )
    .await?;
    if original.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(admission.get("parent_plan_id"))
        || original.get::<i64, _>("source_account_id")
            != admission.get::<i64, _>("source_account_id")
        || original.get::<i64, _>("workspace_revision")
            != admission.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let tag = binding(ctx, parent, "list", None, None, count);
    let cursor = decode(key, ctx, &tag, page.cursor.as_deref())?;
    let upper = if let Some(cursor) = &cursor {
        cursor.upper.clone().ok_or(MigrationError::InvalidInput)?
    } else {
        Position {
            id: Uuid::max(),
            time: Some(
                sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(&mut *tx)
                    .await?,
            ),
        }
    };
    let rows=sqlx::query("SELECT * FROM migration_admitted_people_refresh WHERE organization_id=$1 AND admission_id=$2 AND (created_at,id)<=($3,$4) AND ($5::timestamptz IS NULL OR (created_at,id)<($5,$6)) ORDER BY created_at DESC,id DESC LIMIT $7")
        .bind(ctx.organization_id.0).bind(parent).bind(upper.time).bind(upper.id).bind(cursor.as_ref().and_then(|v|v.last.time)).bind(cursor.as_ref().map(|v|v.last.id)).bind(count+1).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    let mut used = 8192;
    for row in rows.iter().take(count as usize) {
        if row.get::<String, _>("engine_version") != s::ENGINE {
            return Err(MigrationError::ReleaseNotReady);
        }
        if !push_bounded(&mut values, overview(row), &mut used, SUMMARY_BYTES)? {
            break;
        }
    }
    let next = if rows.len() > values.len() {
        Some(encode(
            key,
            ctx,
            tag,
            position(&rows[values.len() - 1], Some("created_at")),
            Some(upper),
            0,
        )?)
    } else {
        None
    };
    finish(
        tx,
        json!({"refreshes":values,"next_cursor":next}),
        SUMMARY_BYTES,
    )
    .await
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0,refresh_id=%id))]
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = begin(pool, ctx).await?;
    let row = resource(&mut tx, key, ctx, id).await?;
    let mut value = overview(&row);
    let plan=sqlx::query("SELECT * FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let mut live_plan = false;
    let mut eligible = false;
    if let Some(plan) = plan.filter(|v| v.get::<Option<DateTime<Utc>>, _>("sealed_at").is_some()) {
        let digest: Vec<u8> = plan
            .get::<Option<Vec<u8>>, _>("digest")
            .ok_or(MigrationError::Crypto)?;
        if digest.len() != 32 {
            return Err(MigrationError::Crypto);
        }
        live_plan = plan
            .get::<Option<DateTime<Utc>>, _>("expires_at")
            .is_some_and(|v| v > now);
        eligible = plan.get::<i64, _>("eligible_count") > 0;
        value["plan"] = json!({"id":plan.get::<Uuid,_>("id"),"revision":plan.get::<i64,_>("revision").to_string(),"digest":digest.iter().map(|v|format!("{v:02x}")).collect::<String>(),"expires_at":plan.get::<Option<DateTime<Utc>>,_>("expires_at"),"counts":{"eligible":plan.get::<i64,_>("eligible_count").to_string(),"already_current":plan.get::<i64,_>("already_current_count").to_string(),"held":plan.get::<i64,_>("held_count").to_string(),"excluded":plan.get::<i64,_>("excluded_count").to_string(),"name_clears":plan.get::<i64,_>("name_clear_count").to_string(),"assignment_clears":plan.get::<i64,_>("assignment_clear_count").to_string(),"contact_removals":plan.get::<i64,_>("contact_removal_count").to_string(),"no_instruction":plan.get::<i64,_>("no_instruction_count").to_string()}});
    }
    let state = row.get::<String, _>("state");
    let initiator = row.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0;
    value["actions"] = json!({"confirm":initiator&&state=="ready"&&live_plan&&eligible,"repreview":initiator&&state=="ready","retry":initiator&&state=="paused","cancel":!matches!(state.as_str(),"completed"|"cancelled")});
    finish(tx, value, SUMMARY_BYTES).await
}
pub async fn items(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    page: Page,
) -> Result<Value, MigrationError> {
    let count = limit(&page, 50)?;
    if page.admission_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    let run = resource(&mut tx, key, ctx, id).await?;
    let cancelled = run.get::<String, _>("state") == "cancelled";
    let plan = selected_plan(&mut tx, ctx, id, page.plan_id).await?;
    let plan_id: Uuid = plan.get("id");
    let tag = binding(
        ctx,
        id,
        "items",
        Some(&plan),
        page.disposition.clone(),
        count,
    );
    let cursor = decode(key, ctx, &tag, page.cursor.as_deref())?;
    let after = cursor.as_ref().map_or(Uuid::nil(), |v| v.last.id);
    // Resolve each display-outcome range independently before combining bounded
    // IDs. A CASE over every plan row would defeat sparse disposition paging.
    let mut ids: Vec<Uuid> = if let Some(disposition) = page.disposition.as_deref() {
        let mut ids = Vec::new();
        if cancelled && disposition == "cancelled" {
            ids.extend(sqlx::query_scalar::<_, Uuid>("SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND settled_at IS NULL AND id>$4 ORDER BY id LIMIT $5")
                .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(after).bind(count+1).fetch_all(&mut *tx).await?);
        } else if !cancelled {
            ids.extend(sqlx::query_scalar::<_, Uuid>("SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND disposition=$4 AND settled_at IS NULL AND id>$5 ORDER BY id LIMIT $6")
                .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(disposition).bind(after).bind(count+1).fetch_all(&mut *tx).await?);
        }
        if run.get::<Option<Uuid>, _>("confirmed_refresh_plan_id") == Some(plan_id) {
            ids.extend(sqlx::query_scalar::<_, Uuid>("SELECT item_id FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2 AND disposition=$3 AND item_id>$4 ORDER BY item_id LIMIT $5")
                .bind(id).bind(ctx.organization_id.0).bind(disposition).bind(after).bind(count+1).fetch_all(&mut *tx).await?);
        }
        ids
    } else {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND id>$4 ORDER BY id LIMIT $5")
            .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(after).bind(count+1).fetch_all(&mut *tx).await?
    };
    ids.sort_unstable();
    ids.dedup();
    ids.truncate((count + 1) as usize);
    let rows = sqlx::query("SELECT i.id,i.source_id,i.person_id,i.settled_at,COALESCE(r.disposition,CASE WHEN $4::boolean AND i.settled_at IS NULL THEN 'cancelled' ELSE i.disposition END) AS display_disposition,i.name_clear_count,i.assignment_clear_count,i.contact_removal_count,i.instructions_nonce,i.instructions_ciphertext FROM migration_admitted_people_refresh_item i LEFT JOIN migration_admitted_people_refresh_result r ON r.id=i.settled_result_id AND r.refresh_id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.refresh_id=$1 AND i.organization_id=$2 AND i.plan_id=$3 AND i.id=ANY($5) ORDER BY i.id")
        .bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(cancelled).bind(&ids).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    let mut used = 8192;
    for row in rows.iter().take(count as usize) {
        if !push_bounded(
            &mut values,
            item_summary(key, ctx, id, row)?,
            &mut used,
            PAGE_BYTES,
        )? {
            break;
        }
    }
    let next = if rows.len() > values.len() {
        Some(encode(
            key,
            ctx,
            tag,
            position(&rows[values.len() - 1], None),
            None,
            0,
        )?)
    } else {
        None
    };
    finish(tx,json!({"plan_id":plan_id,"plan_revision":plan.get::<i64,_>("revision").to_string(),"items":values,"next_cursor":next}),PAGE_BYTES).await
}

async fn item_row(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    refresh: Uuid,
    item: Uuid,
) -> Result<(PgRow, PgRow), MigrationError> {
    let metadata=sqlx::query("SELECT plan_id,octet_length(baseline_ciphertext)+octet_length(current_ciphertext)+octet_length(proposed_ciphertext)+octet_length(instructions_ciphertext) AS bytes FROM migration_admitted_people_refresh_item WHERE id=$1 AND refresh_id=$2 AND organization_id=$3")
        .bind(item).bind(refresh).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    if i64::from(metadata.get::<i32, _>("bytes")) > s::ITEM_LIMIT {
        return Err(MigrationError::StorageLimit);
    }
    let plan = selected_plan(conn, ctx, refresh, Some(metadata.get("plan_id"))).await?;
    let row=sqlx::query("SELECT i.*,COALESCE(r.disposition,CASE WHEN f.state='cancelled' AND i.settled_at IS NULL THEN 'cancelled' ELSE i.disposition END) AS display_disposition FROM migration_admitted_people_refresh_item i JOIN migration_admitted_people_refresh f ON f.id=i.refresh_id AND f.organization_id=i.organization_id LEFT JOIN migration_admitted_people_refresh_result r ON r.id=i.settled_result_id AND r.refresh_id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=$1 AND i.refresh_id=$2 AND i.organization_id=$3").bind(item).bind(refresh).bind(ctx.organization_id.0).fetch_one(conn).await?;
    Ok((row, plan))
}
fn projection(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    refresh: Uuid,
    row: &PgRow,
    side: &str,
) -> Result<Value, MigrationError> {
    if !matches!(side, "baseline" | "current" | "proposed") {
        return Err(MigrationError::InvalidInput);
    }
    s::open(
        key,
        ctx.organization_id,
        refresh,
        row.get("id"),
        side,
        &row.get::<Vec<u8>, _>(format!("{side}_nonce").as_str()),
        &row.get::<Vec<u8>, _>(format!("{side}_ciphertext").as_str()),
    )
}
fn prefix(value: &str, bytes: usize) -> &str {
    let mut end = value.len().min(bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}
fn scalar_projection(value: Value) -> Value {
    let mut output = json!({});
    let mut truncated = Vec::new();
    for field in ["first_name", "last_name", "stage_id", "assigned_user_id"] {
        if let Some(text) = value[field].as_str() {
            let part = prefix(text, 1024);
            output[field] = json!(part);
            if part.len() < text.len() {
                truncated.push(field);
            }
        } else if value.get(field).is_some() {
            output[field] = Value::Null;
        }
    }
    let contacts = value["contacts"].as_array();
    let count = |kind: &str| {
        contacts
            .into_iter()
            .flatten()
            .filter(|v| v["kind"] == kind)
            .count()
            .to_string()
    };
    output["contact_counts"] = json!({"email":count("email"),"phone":count("phone")});
    output["truncated_fields"] = json!(truncated);
    output
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
    let (row, plan) = item_row(&mut tx, ctx, id, item).await?;
    let mut value = item_summary(key, ctx, id, &row)?;
    value["plan_id"] = json!(plan.get::<Uuid, _>("id"));
    value["plan_revision"] = json!(plan.get::<i64, _>("revision").to_string());
    for side in ["baseline", "current", "proposed"] {
        value[side] = scalar_projection(projection(key, ctx, id, &row, side)?);
    }
    // Labels are bounded current-catalog hints, never frozen source evidence or
    // command inputs. Exact IDs remain in each immutable projection.
    let ids = |field: &str| -> Vec<Uuid> {
        ["baseline", "current", "proposed"]
            .iter()
            .filter_map(|side| value[*side][field].as_str().and_then(|v| Uuid::parse_str(v).ok()))
            .collect()
    };
    let stages = sqlx::query("SELECT id,left(name,256) AS label FROM stage WHERE organization_id=$1 AND id=ANY($2)")
        .bind(ctx.organization_id.0).bind(ids("stage_id")).fetch_all(&mut *tx).await?;
    let assignees = sqlx::query("SELECT u.id,left(u.display_name,256) AS label FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND u.id=ANY($2)")
        .bind(ctx.organization_id.0).bind(ids("assigned_user_id")).fetch_all(&mut *tx).await?;
    for side in ["baseline", "current", "proposed"] {
        for (field, label, rows) in [("stage_id", "current_stage_label", &stages), ("assigned_user_id", "current_assignee_label", &assignees)] {
            let id = value[side][field].as_str().and_then(|v| Uuid::parse_str(v).ok());
            value[side][label] = json!(rows.iter().find(|row| Some(row.get::<Uuid,_>("id")) == id).map(|row| row.get::<String,_>("label")));
        }
    }
    finish(tx, value, SUMMARY_BYTES).await
}
pub async fn contacts(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
    page: Page,
) -> Result<Value, MigrationError> {
    let count = limit(&page, 50)?;
    if page.admission_id.is_some() || page.plan_id.is_some() || page.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let plan_id:Uuid=sqlx::query_scalar("SELECT plan_id FROM migration_admitted_people_refresh_item WHERE id=$1 AND refresh_id=$2 AND organization_id=$3").bind(item).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let plan = selected_plan(&mut tx, ctx, id, Some(plan_id)).await?;
    let tag = binding(ctx, item, "contacts", Some(&plan), None, count);
    let cursor = decode(key, ctx, &tag, page.cursor.as_deref())?;
    let rows=sqlx::query("SELECT c.*,CASE c.side WHEN 'current' THEN 'current' WHEN 'baseline' THEN CASE WHEN EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact p WHERE p.organization_id=c.organization_id AND p.refresh_id=c.refresh_id AND p.item_id=c.item_id AND p.side='proposed' AND p.contact_id=c.contact_id) THEN 'retain' ELSE 'remove' END WHEN 'proposed' THEN CASE WHEN EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact b WHERE b.organization_id=c.organization_id AND b.refresh_id=c.refresh_id AND b.item_id=c.item_id AND b.side='baseline' AND b.contact_id=c.contact_id) THEN 'retain' ELSE 'add' END END AS change FROM migration_admitted_people_refresh_contact c WHERE c.refresh_id=$1 AND c.organization_id=$2 AND c.item_id=$3 AND c.id>$4 ORDER BY c.id LIMIT $5")
        .bind(id).bind(ctx.organization_id.0).bind(item).bind(cursor.as_ref().map_or(Uuid::nil(),|v|v.last.id)).bind(count+1).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    let mut used = 8192;
    for row in rows.iter().take(count as usize) {
        let encrypted: Vec<u8> = row.get("value_ciphertext");
        if encrypted.len() > 32768 {
            return Err(MigrationError::Crypto);
        }
        let contact: Value = s::open(
            key,
            ctx.organization_id,
            id,
            row.get("id"),
            "contact",
            &row.get::<Vec<u8>, _>("value_nonce"),
            &encrypted,
        )?;
        let value = json!({"id":row.get::<Uuid,_>("id"),"side":row.get::<String,_>("side"),"change":row.get::<String,_>("change"),"contact_id":row.get::<Option<Uuid>,_>("contact_id"),"kind":row.get::<String,_>("kind"),"import_order":row.get::<i32,_>("import_order"),"value":{"id":contact["id"],"kind":contact["kind"],"value":contact["value"],"normalized_value":contact["normalized_value"],"import_order":contact["import_order"]}});
        if !push_bounded(&mut values, value, &mut used, PAGE_BYTES)? {
            break;
        }
    }
    let next = if rows.len() > values.len() {
        Some(encode(
            key,
            ctx,
            tag,
            position(&rows[values.len() - 1], None),
            None,
            0,
        )?)
    } else {
        None
    };
    finish(tx,json!({"plan_id":plan_id,"plan_revision":plan.get::<i64,_>("revision").to_string(),"contacts":values,"next_cursor":next}),PAGE_BYTES).await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    page: Page,
) -> Result<Value, MigrationError> {
    let count = limit(&page, 50)?;
    if page.admission_id.is_some() || page.plan_id.is_some() || page.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let tag = binding(ctx, id, "results", None, None, count);
    let cursor = decode(key, ctx, &tag, page.cursor.as_deref())?;
    let upper = if let Some(cursor) = &cursor {
        cursor.upper.clone().ok_or(MigrationError::InvalidInput)?
    } else {
        let last=sqlx::query("SELECT id,committed_at FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2 ORDER BY committed_at DESC,id DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
        let Some(last) = last else {
            return finish(
                tx,
                json!({"results":[],"next_cursor":Value::Null}),
                PAGE_BYTES,
            )
            .await;
        };
        position(&last, Some("committed_at"))
    };
    if upper.time.is_none() {
        return Err(MigrationError::InvalidInput);
    }
    let rows=sqlx::query("SELECT id,item_id,person_id,source_id,disposition,committed_at FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2 AND (committed_at,id)<=($3,$4) AND ($5::timestamptz IS NULL OR (committed_at,id)>($5,$6)) ORDER BY committed_at,id LIMIT $7")
        .bind(id).bind(ctx.organization_id.0).bind(upper.time).bind(upper.id).bind(cursor.as_ref().and_then(|v|v.last.time)).bind(cursor.as_ref().map(|v|v.last.id)).bind(count+1).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    let mut used = 8192;
    for row in rows.iter().take(count as usize) {
        let value = json!({"id":row.get::<Uuid,_>("id"),"item_id":row.get::<Uuid,_>("item_id"),"person_id":row.get::<Option<Uuid>,_>("person_id"),"source_id":row.get::<Option<String>,_>("source_id"),"disposition":row.get::<String,_>("disposition"),"committed_at":row.get::<DateTime<Utc>,_>("committed_at")});
        if !push_bounded(&mut values, value, &mut used, PAGE_BYTES)? {
            break;
        }
    }
    let next = if rows.len() > values.len() {
        Some(encode(
            key,
            ctx,
            tag,
            position(&rows[values.len() - 1], Some("committed_at")),
            Some(upper),
            0,
        )?)
    } else {
        None
    };
    finish(tx, json!({"results":values,"next_cursor":next}), PAGE_BYTES).await
}
/// Only scalar name fields are fragmentable; caller input never selects arbitrary source paths.
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    (id, item, side, field): (Uuid, Uuid, String, String),
    page: Page,
) -> Result<Value, MigrationError> {
    if !matches!(side.as_str(), "baseline" | "current" | "proposed")
        || !matches!(field.as_str(), "first_name" | "last_name")
        || page.admission_id.is_some()
        || page.plan_id.is_some()
        || page.disposition.is_some()
    {
        return Err(MigrationError::InvalidInput);
    }
    let bytes = usize::from(page.limit.unwrap_or(FIELD_BYTES as u16));
    if !(4..=FIELD_BYTES).contains(&bytes) {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = begin(pool, ctx).await?;
    resource(&mut tx, key, ctx, id).await?;
    let (row, plan) = item_row(&mut tx, ctx, id, item).await?;
    let tag = binding(
        ctx,
        item,
        "field",
        Some(&plan),
        Some(format!("{side}:{field}")),
        bytes as i64,
    );
    let cursor = decode(key, ctx, &tag, page.cursor.as_deref())?;
    let offset = cursor.as_ref().map_or(0, |v| v.offset);
    let projection = projection(key, ctx, id, &row, &side)?;
    let text = projection[&field]
        .as_str()
        .ok_or(MigrationError::NotFound)?;
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(MigrationError::InvalidInput);
    }
    let fragment = prefix(&text[offset..], bytes);
    let end = offset + fragment.len();
    let next = if end < text.len() {
        Some(encode(
            key,
            ctx,
            tag,
            Position {
                id: item,
                time: None,
            },
            None,
            end,
        )?)
    } else {
        None
    };
    let value = json!({"plan_id":plan.get::<Uuid,_>("id"),"plan_revision":plan.get::<i64,_>("revision").to_string(),"side":side,"field":field,"offset":offset.to_string(),"total_bytes":text.len().to_string(),"fragment":fragment,"next_cursor":next});
    finish(tx, value, SUMMARY_BYTES).await
}

//! Sealed-only keyset pages and bounded current-admin report discovery.
use super::{
    core_change_reports::ReportPage, core_change_source as source, core_change_store as s,
    MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListCursor {
    parent: Uuid,
    upper_time: DateTime<Utc>,
    upper_id: Uuid,
    last_time: DateTime<Utc>,
    last_id: Uuid,
    limit: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowCursor {
    report: Uuid,
    engine: String,
    inputs: Vec<u8>,
    revision: Uuid,
    family: Option<String>,
    disposition: Option<String>,
    last: Uuid,
    limit: i64,
}
fn encode<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    owner: Uuid,
    purpose: &str,
    value: &T,
) -> Result<String, MigrationError> {
    let sealed = s::seal(key, org, owner, Uuid::nil(), purpose, value)?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn decode<T: serde::de::DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    owner: Uuid,
    purpose: &str,
    token: Option<&str>,
) -> Result<Option<T>, MigrationError> {
    token
        .map(|token| {
            if token.len() > 4096 {
                return Err(MigrationError::InvalidInput);
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(token)
                .map_err(|_| MigrationError::InvalidInput)?;
            if bytes.len() < 40 {
                return Err(MigrationError::InvalidInput);
            }
            s::open(
                key,
                org,
                owner,
                Uuid::nil(),
                purpose,
                &bytes[..24],
                &bytes[24..],
            )
            .map_err(|_| MigrationError::InvalidInput)
        })
        .transpose()
}
fn summary(key: &RawPayloadKey, ctx: &CommandContext, r: &PgRow) -> Result<Value, MigrationError> {
    let id = r.get("id");
    let state: String = r.get("state");
    let completed = state == "completed";
    let counts = if completed {
        s::open(
            key,
            ctx.organization_id,
            id,
            id,
            "summary",
            r.get::<Option<Vec<u8>>, _>("summary_nonce")
                .as_deref()
                .ok_or(MigrationError::Crypto)?,
            r.get::<Option<Vec<u8>>, _>("summary_ciphertext")
                .as_deref()
                .ok_or(MigrationError::Crypto)?,
        )?
    } else {
        Value::Null
    };
    Ok(
        json!({"id":id,"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"engine_version":r.get::<String,_>("engine_version"),"state":state,"phase":r.get::<String,_>("phase"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"output_revision":r.get::<Option<Uuid>,_>("output_revision"),"inputs":s::inputs(key,ctx.organization_id,r)?,"counts":counts,"progress":{"captures_processed":r.get::<i64,_>("captures_processed").to_string(),"observations_processed":r.get::<i64,_>("observations_processed").to_string(),"groups_compared":r.get::<i64,_>("groups_compared").to_string()},"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),"actions":{"resume":state=="paused"&&r.get::<Uuid,_>("initiated_by_user_id")==ctx.actor_user_id.0,"cancel":!matches!(state.as_str(),"completed"|"cancelled")}}),
    )
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,report_id=%id))]
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    s::validate(&mut tx, key, ctx.organization_id, &r).await?;
    let v = summary(key, ctx, &r)?;
    tx.commit().await?;
    Ok(v)
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: ReportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit(20)?;
    if q.family.is_some() || q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let parent = q.parent_import_id.ok_or(MigrationError::InvalidInput)?;
    let mut tx = s::begin(pool, ctx).await?;
    super::history_capture_store::parent(&mut tx, ctx.organization_id, parent).await?;
    let cursor: Option<ListCursor> = decode(
        key,
        ctx.organization_id,
        parent,
        "list-cursor",
        q.cursor.as_deref(),
    )?;
    if cursor
        .as_ref()
        .is_some_and(|v| v.parent != parent || v.limit != limit)
    {
        return Err(MigrationError::InvalidInput);
    }
    let upper = if let Some(v) = &cursor {
        (v.upper_time, v.upper_id)
    } else {
        (
            sqlx::query_scalar::<_, DateTime<Utc>>("SELECT clock_timestamp()")
                .fetch_one(&mut *tx)
                .await?,
            Uuid::max(),
        )
    };
    let raw=sqlx::query("SELECT * FROM migration_core_change_report WHERE organization_id=$1 AND parent_import_id=$2 AND (created_at,id)<=($3,$4) AND ($5::timestamptz IS NULL OR (created_at,id)<($5,$6)) ORDER BY created_at DESC,id DESC LIMIT $7").bind(ctx.organization_id.0).bind(parent).bind(upper.0).bind(upper.1).bind(cursor.as_ref().map(|v|v.last_time)).bind(cursor.as_ref().map(|v|v.last_id)).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut reports = Vec::new();
    let mut size = 4096;
    let mut last = None;
    for r in &raw {
        if reports.len() >= limit as usize {
            break;
        }
        s::validate(&mut tx, key, ctx.organization_id, r).await?;
        let v = summary(key, ctx, r)?;
        let bytes = serde_json::to_vec(&v)
            .map_err(|_| MigrationError::Crypto)?
            .len();
        if size + bytes > 128 * 1024 {
            break;
        }
        size += bytes;
        reports.push(v);
        last = Some((
            r.get::<DateTime<Utc>, _>("created_at"),
            r.get::<Uuid, _>("id"),
        ));
    }
    let next = if raw.len() > reports.len() {
        let (time, id) = last.ok_or(MigrationError::StorageLimit)?;
        Some(encode(
            key,
            ctx.organization_id,
            parent,
            "list-cursor",
            &ListCursor {
                parent,
                upper_time: upper.0,
                upper_id: upper.1,
                last_time: time,
                last_id: id,
                limit,
            },
        )?)
    } else {
        None
    };
    tx.commit().await?;
    Ok(json!({"reports":reports,"next_cursor":next}))
}
fn published(r: &PgRow) -> Result<Uuid, MigrationError> {
    if r.get::<String, _>("state") != "completed"
        || r.get::<String, _>("phase") != "sealed"
        || r.get::<String, _>("engine_version") != source::ENGINE
    {
        return Err(MigrationError::Conflict);
    }
    r.get::<Option<Uuid>, _>("output_revision")
        .ok_or(MigrationError::Conflict)
}
fn output(
    key: &RawPayloadKey,
    org: OrganizationId,
    report: Uuid,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    s::open(
        key,
        org,
        report,
        r.get("id"),
        "output",
        r.get::<Option<Vec<u8>>, _>("output_nonce")
            .as_deref()
            .ok_or(MigrationError::Crypto)?,
        r.get::<Option<Vec<u8>>, _>("output_ciphertext")
            .as_deref()
            .ok_or(MigrationError::Crypto)?,
    )
}
pub async fn rows(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ReportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit(50)?;
    if q.parent_import_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    s::validate(&mut tx, key, ctx.organization_id, &r).await?;
    let revision = published(&r)?;
    let cursor: Option<RowCursor> = decode(
        key,
        ctx.organization_id,
        id,
        "row-cursor",
        q.cursor.as_deref(),
    )?;
    let binding: Vec<u8> = r.get("tuple_hmac");
    if cursor.as_ref().is_some_and(|v| {
        v.report != id
            || v.engine != source::ENGINE
            || v.inputs != binding
            || v.revision != revision
            || v.family != q.family
            || v.disposition != q.disposition
            || v.limit != limit
    }) {
        return Err(MigrationError::InvalidInput);
    }
    let raw=sqlx::query("SELECT id,output_nonce,output_ciphertext FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND ($3::text IS NULL OR family=$3) AND ($4::text IS NULL OR disposition=$4) AND ($5::uuid IS NULL OR id>$5) ORDER BY id LIMIT $6").bind(id).bind(ctx.organization_id.0).bind(&q.family).bind(&q.disposition).bind(cursor.as_ref().map(|v|v.last)).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    let mut size = 4096;
    let mut last = None;
    for row in &raw {
        if values.len() >= limit as usize {
            break;
        }
        let value = output(key, ctx.organization_id, id, row)?;
        let bytes = serde_json::to_vec(&value)
            .map_err(|_| MigrationError::Crypto)?
            .len();
        if size + bytes > 256 * 1024 {
            break;
        }
        size += bytes;
        values.push(value);
        last = Some(row.get("id"));
    }
    let next = if raw.len() > values.len() {
        Some(encode(
            key,
            ctx.organization_id,
            id,
            "row-cursor",
            &RowCursor {
                report: id,
                engine: source::ENGINE.into(),
                inputs: binding,
                revision,
                family: q.family,
                disposition: q.disposition,
                last: last.ok_or(MigrationError::StorageLimit)?,
                limit,
            },
        )?)
    } else {
        None
    };
    tx.commit().await?;
    Ok(json!({"rows":values,"next_cursor":next,"output_revision":revision}))
}
pub async fn row_detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    row_id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, id).await?;
    s::validate(&mut tx, key, ctx.organization_id, &r).await?;
    let revision = published(&r)?;
    let row=sqlx::query("SELECT id,output_nonce,output_ciphertext FROM migration_core_change_group WHERE id=$1 AND report_id=$2 AND organization_id=$3").bind(row_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let v = output(key, ctx.organization_id, id, &row)?;
    tx.commit().await?;
    Ok(json!({"row":v,"output_revision":revision}))
}

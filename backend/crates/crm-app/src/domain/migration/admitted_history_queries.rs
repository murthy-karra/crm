//! Bounded metadata-only readers with authenticated, scope-bound cursors.
use super::{
    admitted_history::{Page, ENGINE},
    admitted_history_store as s, crypto, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

pub(crate) async fn receipt(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT r.id,r.state,r.phase,r.revision,r.latest_plan_id,r.confirmed_plan_id,r.current_attempt_id,r.pause_reason FROM migration_admitted_history_root r WHERE r.id=$1 AND r.organization_id=$2").bind(id).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    Ok(
        json!({"import":{"id":id,"state":r.get::<String,_>("state"),"phase":r.get::<String,_>("phase"),"revision":r.get::<i64,_>("revision").to_string(),"latest_plan_id":r.get::<Option<Uuid>,_>("latest_plan_id"),"confirmed_plan_id":r.get::<Option<Uuid>,_>("confirmed_plan_id"),"current_attempt_id":r.get::<Option<Uuid>,_>("current_attempt_id"),"pause_reason":r.get::<Option<String>,_>("pause_reason")}}),
    )
}
async fn detail(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r = s::root(conn, org, id).await?;
    let p = s::plan(conn, org, id, r.get("latest_plan_id")).await?;
    let mut v = receipt(conn, org, id).await?["import"].clone();
    let state = r.get::<String, _>("state");
    v["engine_version"] = json!(ENGINE);
    v["admission_id"] = json!(r.get::<Uuid, _>("admission_id"));
    v["history_capture_id"] = json!(r.get::<Uuid, _>("history_capture_id"));
    v["source_binding"] = p.get::<Value, _>("source_binding");
    v["coverage"] = p.get::<Value, _>("coverage");
    v["latest_plan"] = json!({"id":p.get::<Uuid,_>("id"),"state":p.get::<String,_>("state"),"expires_at":p.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),"counts":p.get::<Value,_>("counts")});
    v["results"] = r.get::<Value, _>("result_counts");
    if let Some(counts) = v["results"].as_object_mut() {
        for value in counts.values_mut() {
            if let Some(n) = value.as_i64() {
                *value = json!(n.to_string());
            }
        }
    }
    for key in [
        "revision",
        "workspace_revision",
        "run_byte_limit",
        "budget_revision",
        "retained_bytes",
        "reserved_bytes",
    ] {
        v[key] = json!(r.get::<i64, _>(key).to_string());
    }
    let expired: bool=sqlx::query_scalar("SELECT COALESCE(expires_at<=clock_timestamp(),true) FROM migration_admitted_history_plan WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(org.0).fetch_one(&mut *conn).await?;
    v["actions"] = json!({"confirm":state=="ready"&&!expired&&p.get::<i64,_>("eligible")>0,"resume":state=="paused","cancel":!matches!(state.as_str(),"completed"|"cancelled"),"remainder":state=="cancelled"&&r.get::<Option<Uuid>,_>("current_attempt_id").is_some()});
    Ok(v)
}
pub async fn get(pool: &PgPool, ctx: &CommandContext, id: Uuid) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let v = detail(&mut tx, ctx.organization_id, id).await?;
    tx.commit().await?;
    Ok(v)
}

fn cursor(
    key: &RawPayloadKey,
    org: OrganizationId,
    scope: &Value,
    last: &Value,
) -> Result<String, MigrationError> {
    let sealed = crypto::seal_history(
        key,
        org,
        Uuid::nil(),
        Uuid::nil(),
        "admitted-history-cursor-v1",
        &serde_json::to_vec(&json!({"scope":scope,"last":last}))
            .map_err(|_| MigrationError::Crypto)?,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn after(
    key: &RawPayloadKey,
    org: OrganizationId,
    scope: &Value,
    raw: Option<&str>,
) -> Result<Option<Value>, MigrationError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| MigrationError::InvalidInput)?;
    if bytes.len() < 40 || bytes.len() > 4096 {
        return Err(MigrationError::InvalidInput);
    }
    let plain = crypto::open_history(
        key,
        org,
        Uuid::nil(),
        Uuid::nil(),
        "admitted-history-cursor-v1",
        &bytes[..24],
        &bytes[24..],
    )
    .map_err(|_| MigrationError::InvalidInput)?;
    let v: Value = serde_json::from_slice(&plain).map_err(|_| MigrationError::InvalidInput)?;
    if v["scope"] != *scope {
        return Err(MigrationError::InvalidInput);
    }
    Ok(Some(v["last"].clone()))
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    q: Page,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.family.is_some() || q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx, false).await?;
    let scope = json!({"endpoint":"list","org":ctx.organization_id,"admission":q.admission_id,"limit":limit});
    let last = after(key, ctx.organization_id, &scope, q.cursor.as_deref())?;
    let at = last
        .as_ref()
        .map(|v| serde_json::from_value::<chrono::DateTime<chrono::Utc>>(v["at"].clone()))
        .transpose()
        .map_err(|_| MigrationError::InvalidInput)?;
    let last_id = last
        .as_ref()
        .map(|v| serde_json::from_value::<Uuid>(v["id"].clone()))
        .transpose()
        .map_err(|_| MigrationError::InvalidInput)?;
    let rows=sqlx::query("SELECT id,created_at FROM migration_admitted_history_root WHERE organization_id=$1 AND ($2::uuid IS NULL OR admission_id=$2) AND ($3::timestamptz IS NULL OR (created_at,id)<($3,$4)) ORDER BY created_at DESC,id DESC LIMIT $5").bind(ctx.organization_id.0).bind(q.admission_id).bind(at).bind(last_id).bind(limit+1).fetch_all(&mut *tx).await?;
    let next = if rows.len() > limit as usize {
        let r = &rows[limit as usize - 1];
        Some(cursor(
            key,
            ctx.organization_id,
            &scope,
            &json!({"at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"id":r.get::<Uuid,_>("id")}),
        )?)
    } else {
        None
    };
    let mut items = Vec::new();
    for r in rows.iter().take(limit as usize) {
        items.push(detail(&mut tx, ctx.organization_id, r.get("id")).await?);
    }
    tx.commit().await?;
    Ok(json!({"imports":items,"next_cursor":next}))
}
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: Page,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, plan, q, "manifests").await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: Page,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, plan, q, "results").await
}
pub async fn issues(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: Page,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, plan, q, "issues").await
}
async fn page(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    plan: Uuid,
    q: Page,
    kind: &str,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.admission_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    let owned_plan = s::plan(&mut tx, ctx.organization_id, id, plan).await?;
    if owned_plan.get::<String, _>("state") == "building" {
        return Err(MigrationError::ImportConflict);
    }
    let scope = json!({"endpoint":kind,"org":ctx.organization_id,"root":id,"plan":plan,"attempt":r.get::<Option<Uuid>,_>("current_attempt_id"),"revision":r.get::<i64,_>("revision"),"family":q.family,"disposition":q.disposition,"limit":limit});
    let last = after(key, ctx.organization_id, &scope, q.cursor.as_deref())?;
    let mut items = Vec::new();
    let mut keys = Vec::new();
    if kind == "issues" {
        if q.family.is_some() || q.disposition.is_some() {
            return Err(MigrationError::InvalidInput);
        }
        if last.as_ref().is_some_and(|v| !v.is_string()) {
            return Err(MigrationError::InvalidInput);
        }
        let rows=sqlx::query("SELECT code,record_count FROM migration_admitted_history_issue WHERE root_id=$1 AND plan_id=$2 AND organization_id=$3 AND ($4::text IS NULL OR code>$4) ORDER BY code LIMIT $5").bind(id).bind(plan).bind(ctx.organization_id.0).bind(last.as_ref().and_then(Value::as_str)).bind(limit+1).fetch_all(&mut *tx).await?;
        for row in rows {
            let code = row.get::<String, _>("code");
            keys.push(json!(code));
            items.push(
                json!({"code":code,"record_count":row.get::<i64,_>("record_count").to_string()}),
            );
        }
    } else {
        let table = if kind == "results" {
            "migration_admitted_history_result"
        } else {
            "migration_admitted_history_manifest"
        };
        let position = last
            .as_ref()
            .map(|v| v.as_i64().ok_or(MigrationError::InvalidInput))
            .transpose()?
            .unwrap_or(0);
        let rows=sqlx::query(&format!("SELECT * FROM {table} WHERE root_id=$1 AND plan_id=$2 AND organization_id=$3 AND position>$4 AND ($5::text IS NULL OR family=$5) AND ($6::text IS NULL OR disposition=$6) ORDER BY position LIMIT $7")).bind(id).bind(plan).bind(ctx.organization_id.0).bind(position).bind(q.family).bind(q.disposition).bind(limit+1).fetch_all(&mut *tx).await?;
        for row in rows {
            let pos = row.get::<i64, _>("position");
            keys.push(json!(pos));
            let mut item = json!({"id":row.get::<Uuid,_>("id"),"position":pos.to_string(),"family":row.get::<String,_>("family"),"disposition":row.get::<String,_>("disposition"),"reason":row.get::<Option<String>,_>("reason")});
            if kind == "manifests" {
                item["person_id"] = json!(row.get::<Option<Uuid>, _>("person_id"));
                item["source_created_at"] =
                    json!(row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("source_created_at"));
                item["metadata"] = if row.get::<Option<Vec<u8>>, _>("nonce").is_some() {
                    s::open_metadata(key, ctx.organization_id, &row)?
                } else {
                    Value::Null
                };
                item["suppressed"] = json!(item["metadata"].is_null());
            } else {
                item["attempt_id"] = json!(row.get::<Uuid, _>("attempt_id"));
                item["manifest_id"] = json!(row.get::<Uuid, _>("manifest_id"));
                item["fact_id"] = json!(row.get::<Option<Uuid>, _>("fact_id"));
            }
            items.push(item);
        }
    }
    let next = if items.len() > limit as usize {
        items.truncate(limit as usize);
        Some(cursor(
            key,
            ctx.organization_id,
            &scope,
            &keys[limit as usize - 1],
        )?)
    } else {
        None
    };
    tx.commit().await?;
    Ok(json!({kind:items,"next_cursor":next}))
}
pub async fn remainder_view(
    pool: &PgPool,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let r = s::root(&mut tx, ctx.organization_id, id).await?;
    let row=sqlx::query("SELECT predecessor_attempt_id,attempt_id,never_settled_count FROM migration_admitted_history_remainder WHERE root_id=$1 AND organization_id=$2 AND attempt_id=$3").bind(id).bind(ctx.organization_id.0).bind(r.get::<Option<Uuid>,_>("current_attempt_id")).fetch_optional(&mut *tx).await?;
    let v=row.map(|r|json!({"predecessor_attempt_id":r.get::<Uuid,_>("predecessor_attempt_id"),"attempt_id":r.get::<Uuid,_>("attempt_id"),"never_settled_count":r.get::<i64,_>("never_settled_count").to_string()}));
    tx.commit().await?;
    Ok(json!({"remainder":v}))
}

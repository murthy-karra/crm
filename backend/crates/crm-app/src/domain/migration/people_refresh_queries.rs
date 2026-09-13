//! Bounded, current-admin reads for immutable People refresh plans and results.
use super::{people_refresh_store as s, MigrationError};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
        || p.cursor.as_ref().is_some_and(|v| v.len() > 512)
        || p.disposition.as_ref().is_some_and(|v| {
            !matches!(
                v.as_str(),
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
    Ok(i64::from(n))
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    last: Uuid,
    scope: Uuid,
    kind: String,
}
fn cursor(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    owner: Uuid,
    kind: &str,
    v: Option<&str>,
) -> Result<Uuid, MigrationError> {
    let Some(v) = v else { return Ok(Uuid::nil()) };
    let b = URL_SAFE_NO_PAD
        .decode(v)
        .map_err(|_| MigrationError::InvalidInput)?;
    if b.len() < 24 {
        return Err(MigrationError::InvalidInput);
    };
    let c: Cursor = s::open(
        key,
        ctx.organization_id,
        owner,
        Uuid::nil(),
        "cursor",
        &b[..24],
        &b[24..],
    )?;
    if c.scope != ctx.organization_id.0 || c.kind != kind {
        return Err(MigrationError::InvalidInput);
    };
    Ok(c.last)
}
fn next(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    owner: Uuid,
    kind: &str,
    last: Uuid,
) -> Result<String, MigrationError> {
    let sealed = s::seal(
        key,
        ctx.organization_id,
        owner,
        Uuid::nil(),
        "cursor",
        &Cursor {
            last,
            scope: ctx.organization_id.0,
            kind: kind.into(),
        },
    )?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn overview(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<Uuid,_>("id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"report_id":r.get::<Uuid,_>("report_id"),"state":r.get::<String,_>("state"),"lifecycle_revision":r.get::<i64,_>("lifecycle_revision").to_string(),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":r.get::<i64,_>("newer_sequence").to_string(),"created_at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updated_at":r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),"completed_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"progress":{"settled_items":r.get::<i64,_>("settled_items").to_string()},"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string()})
}
fn scalar_projection(mut projection: Value) -> Value {
    let contacts = projection
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = |kind: &str| {
        contacts
            .iter()
            .filter(|contact| contact["kind"] == kind)
            .count()
            .to_string()
    };
    if let Some(object) = projection.as_object_mut() {
        object.remove("contacts");
        object.insert(
            "contact_counts".into(),
            json!({"email":count("email"),"phone":count("phone")}),
        );
    }
    projection
}
fn clear_counts(baseline: &Value, proposed: &Value) -> Value {
    let names = ["first_name", "last_name"]
        .into_iter()
        .filter(|field| !baseline[*field].is_null() && proposed[*field].is_null())
        .count();
    let assignments = usize::from(
        !baseline["assigned_user_id"].is_null() && proposed["assigned_user_id"].is_null(),
    );
    let contacts = baseline["contacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|old| {
            !proposed["contacts"]
                .as_array()
                .is_some_and(|next| next.iter().any(|candidate| candidate["id"] == old["id"]))
        })
        .count();
    json!({"names":names.to_string(),"assignments":assignments.to_string(),"contacts":contacts.to_string()})
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    p: Page,
) -> Result<Value, MigrationError> {
    let parent = p.parent_import_id.ok_or(MigrationError::InvalidInput)?;
    let n = limit(&p, 20)?;
    let last = cursor(key, ctx, parent, "list", p.cursor.as_deref())?;
    let rows=sqlx::query("SELECT * FROM migration_people_refresh WHERE organization_id=$1 AND parent_import_id=$2 AND id>$3 ORDER BY id LIMIT $4").bind(ctx.organization_id.0).bind(parent).bind(last).bind(n+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > n;
    let rows = rows.into_iter().take(n as usize).collect::<Vec<_>>();
    let next_cursor = if more {
        Some(next(
            key,
            ctx,
            parent,
            "list",
            rows.last().ok_or(MigrationError::Crypto)?.get("id"),
        )?)
    } else {
        None
    };
    Ok(json!({"refreshes":rows.iter().map(overview).collect::<Vec<_>>(),"next_cursor":next_cursor}))
}
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r =
        sqlx::query("SELECT * FROM migration_people_refresh WHERE organization_id=$1 AND id=$2")
            .bind(ctx.organization_id.0)
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or(MigrationError::NotFound)?;
    let mut value = overview(&r);
    let plan=sqlx::query("SELECT * FROM migration_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 AND state IN ('ready','superseded','expired') ORDER BY revision DESC LIMIT 1").bind(id).bind(ctx.organization_id.0).fetch_optional(pool).await?;
    if let Some(p) = plan {
        value["plan"] = json!({"id":p.get::<Uuid,_>("id"),"revision":p.get::<i64,_>("revision").to_string(),"digest":p.get::<Vec<u8>,_>("digest").iter().map(|v|format!("{v:02x}")).collect::<String>(),"expires_at":p.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),"counts":{"eligible":p.get::<i64,_>("eligible_count").to_string(),"already_current":p.get::<i64,_>("already_current_count").to_string(),"held":p.get::<i64,_>("held_count").to_string(),"excluded":p.get::<i64,_>("excluded_count").to_string(),"name_clears":p.get::<i64,_>("name_clear_count").to_string(),"assignment_clears":p.get::<i64,_>("assignment_clear_count").to_string(),"contact_removals":p.get::<i64,_>("contact_removal_count").to_string(),"no_instruction":p.get::<i64,_>("no_instruction_count").to_string()}});
    };
    let state = r.get::<String, _>("state");
    let initiator = r.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0;
    let live_plan = value["plan"]["expires_at"]
        .as_str()
        .and_then(|value| value.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .is_some_and(|expires| expires > chrono::Utc::now());
    value["actions"] = json!({"confirm":initiator&&state=="ready"&&live_plan,"repreview":initiator&&state=="ready","retry":initiator&&state=="paused","cancel":state!="completed"&&state!="cancelled"});
    Ok(value)
}
pub async fn items(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    p: Page,
) -> Result<Value, MigrationError> {
    let n = limit(&p, 50)?;
    let plan = if let Some(plan) = p.plan_id {
        sqlx::query("SELECT id,revision FROM migration_people_refresh_plan WHERE id=$1 AND refresh_id=$2 AND organization_id=$3")
            .bind(plan).bind(id).bind(ctx.organization_id.0).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)?
    } else {
        sqlx::query("SELECT id,revision FROM migration_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1")
            .bind(id).bind(ctx.organization_id.0).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)?
    };
    let plan_id: Uuid = plan.get("id");
    let plan_revision: i64 = plan.get("revision");
    let last = cursor(key, ctx, id, "items", p.cursor.as_deref())?;
    let rows=sqlx::query("SELECT i.id,i.source_id,i.person_id,i.disposition,i.settled_at,i.plan_id,p.revision,i.instructions_nonce,i.instructions_ciphertext,i.baseline_nonce,i.baseline_ciphertext,i.proposed_nonce,i.proposed_ciphertext FROM migration_people_refresh_item i JOIN migration_people_refresh_plan p ON p.id=i.plan_id AND p.refresh_id=i.refresh_id AND p.organization_id=i.organization_id WHERE i.refresh_id=$1 AND i.organization_id=$2 AND i.plan_id=$3 AND i.id>$4 AND ($5::text IS NULL OR i.disposition=$5) ORDER BY i.id LIMIT $6").bind(id).bind(ctx.organization_id.0).bind(plan_id).bind(last).bind(&p.disposition).bind(n+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > n;
    let rows = rows.into_iter().take(n as usize).collect::<Vec<_>>();
    let c = if more {
        Some(next(
            key,
            ctx,
            id,
            "items",
            rows.last().ok_or(MigrationError::Crypto)?.get("id"),
        )?)
    } else {
        None
    };
    Ok(
        json!({"plan_id":plan_id,"plan_revision":plan_revision.to_string(),"items":rows.iter().map(|r|{let item=r.get::<Uuid,_>("id");let baseline=s::open::<Value>(key,ctx.organization_id,id,item,"baseline",&r.get::<Vec<u8>,_>("baseline_nonce"),&r.get::<Vec<u8>,_>("baseline_ciphertext"))?;let proposed=s::open::<Value>(key,ctx.organization_id,id,item,"proposed",&r.get::<Vec<u8>,_>("proposed_nonce"),&r.get::<Vec<u8>,_>("proposed_ciphertext"))?;Ok::<_,MigrationError>(json!({"id":item,"plan_id":r.get::<Uuid,_>("plan_id"),"plan_revision":r.get::<i64,_>("revision").to_string(),"source_id":r.get::<String,_>("source_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"disposition":r.get::<String,_>("disposition"),"settled_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("settled_at"),"clear_counts":clear_counts(&baseline,&proposed),"no_instruction":s::open::<Value>(key,ctx.organization_id,id,item,"instructions",&r.get::<Vec<u8>,_>("instructions_nonce"),&r.get::<Vec<u8>,_>("instructions_ciphertext"))?}))}).collect::<Result<Vec<_>,_>>()?,"next_cursor":c}),
    )
}
pub async fn item(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    item: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT i.*,p.revision FROM migration_people_refresh_item i JOIN migration_people_refresh_plan p ON p.id=i.plan_id AND p.refresh_id=i.refresh_id AND p.organization_id=i.organization_id WHERE i.refresh_id=$1 AND i.organization_id=$2 AND i.id=$3").bind(id).bind(ctx.organization_id.0).bind(item).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)?;
    let decode = |purpose: &str, n: &str, c: &str| {
        s::open::<Value>(
            key,
            ctx.organization_id,
            id,
            item,
            purpose,
            &r.get::<Vec<u8>, _>(n),
            &r.get::<Vec<u8>, _>(c),
        )
    };
    Ok(
        json!({"id":item,"plan_id":r.get::<Uuid,_>("plan_id"),"plan_revision":r.get::<i64,_>("revision").to_string(),"source_id":r.get::<String,_>("source_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"disposition":r.get::<String,_>("disposition"),"baseline":scalar_projection(decode("baseline","baseline_nonce","baseline_ciphertext")?),"current":scalar_projection(decode("current","current_nonce","current_ciphertext")?),"proposed":scalar_projection(decode("proposed","proposed_nonce","proposed_ciphertext")?),"no_instruction":decode("instructions","instructions_nonce","instructions_ciphertext")?}),
    )
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
    let plan = sqlx::query("SELECT i.plan_id,p.revision FROM migration_people_refresh_item i JOIN migration_people_refresh_plan p ON p.id=i.plan_id AND p.refresh_id=i.refresh_id AND p.organization_id=i.organization_id WHERE i.id=$1 AND i.refresh_id=$2 AND i.organization_id=$3")
        .bind(item).bind(id).bind(ctx.organization_id.0).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)?;
    let plan_id: Uuid = plan.get("plan_id");
    let plan_revision: i64 = plan.get("revision");
    let last = cursor(key, ctx, item, "contacts", p.cursor.as_deref())?;
    let rows=sqlx::query("SELECT c.*,CASE c.side WHEN 'current' THEN 'current' WHEN 'baseline' THEN CASE WHEN EXISTS(SELECT 1 FROM migration_people_refresh_contact p WHERE p.item_id=c.item_id AND p.side='proposed' AND p.contact_id IS NOT DISTINCT FROM c.contact_id) THEN 'retain' ELSE 'remove' END WHEN 'proposed' THEN CASE WHEN EXISTS(SELECT 1 FROM migration_people_refresh_contact b WHERE b.item_id=c.item_id AND b.side='baseline' AND b.contact_id IS NOT DISTINCT FROM c.contact_id) THEN 'retain' ELSE 'add' END END AS change FROM migration_people_refresh_contact c WHERE c.refresh_id=$1 AND c.organization_id=$2 AND c.item_id=$3 AND c.id>$4 ORDER BY c.id LIMIT $5").bind(id).bind(ctx.organization_id.0).bind(item).bind(last).bind(n+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > n;
    let rows = rows.into_iter().take(n as usize).collect::<Vec<_>>();
    let c = if more {
        Some(next(
            key,
            ctx,
            item,
            "contacts",
            rows.last().ok_or(MigrationError::Crypto)?.get("id"),
        )?)
    } else {
        None
    };
    Ok(
        json!({"plan_id":plan_id,"plan_revision":plan_revision.to_string(),"contacts":rows.iter().map(|r|Ok::<_,MigrationError>(json!({"id":r.get::<Uuid,_>("id"),"side":r.get::<String,_>("side"),"change":r.get::<String,_>("change"),"contact_id":r.get::<Option<Uuid>,_>("contact_id"),"kind":r.get::<String,_>("kind"),"import_order":r.get::<i32,_>("import_order"),"value":s::open::<Value>(key,ctx.organization_id,id,r.get("id"),"contact",&r.get::<Vec<u8>,_>("value_nonce"),&r.get::<Vec<u8>,_>("value_ciphertext"))?}))).collect::<Result<Vec<_>,_>>()?,"next_cursor":c}),
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
    let last = cursor(key, ctx, id, "results", p.cursor.as_deref())?;
    let rows=sqlx::query("SELECT id,item_id,person_id,source_id,disposition,committed_at FROM migration_people_refresh_result WHERE refresh_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT $4").bind(id).bind(ctx.organization_id.0).bind(last).bind(n+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > n;
    let rows = rows.into_iter().take(n as usize).collect::<Vec<_>>();
    let c = if more {
        Some(next(
            key,
            ctx,
            id,
            "results",
            rows.last().ok_or(MigrationError::Crypto)?.get("id"),
        )?)
    } else {
        None
    };
    Ok(
        json!({"results":rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"item_id":r.get::<Uuid,_>("item_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"source_id":r.get::<String,_>("source_id"),"disposition":r.get::<String,_>("disposition"),"committed_at":r.get::<chrono::DateTime<chrono::Utc>,_>("committed_at")})).collect::<Vec<_>>(),"next_cursor":c}),
    )
}

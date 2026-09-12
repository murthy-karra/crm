//! Bounded retained-import review; source credentials and source readers are absent.
use super::{
    crypto, history_import::ImportPage, history_import_source as source, history_import_store as s,
    snapshot::SnapshotPolicy, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

const RESPONSE_BYTES: usize = 512 * 1024;
const ROW_BYTES: usize = 4096;
const DETAIL_BYTES: usize = 8192;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListCursor {
    parent: Option<Uuid>,
    limit: i64,
    upper: DateTime<Utc>,
    created: DateTime<Utc>,
    id: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordCursor {
    plan: Uuid,
    revision: i64,
    plan_revision: i64,
    limit: i64,
    family: Option<String>,
    disposition: Option<String>,
    position: i64,
}
fn bounded(value: Value, maximum: usize) -> Result<Value, MigrationError> {
    if serde_json::to_vec(&value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > maximum
    {
        Err(MigrationError::StorageLimit)
    } else {
        Ok(value)
    }
}
fn encode<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    endpoint: &str,
    value: &T,
) -> Result<String, MigrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    let sealed = crypto::seal_history(key, org, run, Uuid::nil(), endpoint, &bytes)
        .map_err(|_| MigrationError::Crypto)?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    if token.len() > 4096 {
        return Err(MigrationError::StorageLimit);
    }
    Ok(token)
}
fn decode<T: serde::de::DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    endpoint: &str,
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
            let plaintext = crypto::open_history(
                key,
                org,
                run,
                Uuid::nil(),
                endpoint,
                &bytes[..24],
                &bytes[24..],
            )
            .map_err(|_| MigrationError::InvalidInput)?;
            serde_json::from_slice(&plaintext).map_err(|_| MigrationError::InvalidInput)
        })
        .transpose()
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,import_id=%id))]
pub async fn detail(
    pool: &PgPool,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    let value = view(&mut tx, policy, ctx, id).await?;
    tx.commit().await?;
    Ok(value)
}

async fn view(
    conn: &mut PgConnection,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let run=sqlx::query("SELECT r.*,l.byte_limit AS org_byte_limit,l.budget_revision AS org_budget_revision,EXISTS(SELECT 1 FROM migration_history_import_anchor a WHERE a.organization_id=r.organization_id AND a.parent_import_id=r.parent_import_id AND a.plan_id=r.plan_id) AS anchored FROM migration_history_import_run r JOIN migration_snapshot_storage l ON l.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2")
        .bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    let plan = s::plan(conn, ctx.organization_id, run.get("plan_id")).await?;
    let state: String = run.get("state");
    let preview_complete = plan.get::<String, _>("state") == "ready";
    let anchored: bool = run.get("anchored");
    let expires: Option<DateTime<Utc>> = plan.get("expires_at");
    let qualified = plan.get::<String, _>("interpretation_version") == source::INTERPRETATION
        && plan.get::<String, _>("reader_version") == source::READER;
    let mut value = json!({"id":id,"plan_id":run.get::<Uuid,_>("plan_id"),
        "parent_import_id":run.get::<Uuid,_>("parent_import_id"),"capture_id":plan.get::<Uuid,_>("capture_id"),
        "state":state,"phase":run.get::<String,_>("phase"),
        "interpretation_version":plan.get::<String,_>("interpretation_version"),
        "reader_version":plan.get::<String,_>("reader_version"),"executor_user_id":run.get::<Uuid,_>("executor_user_id"),
        "created_at":run.get::<DateTime<Utc>,_>("created_at"),"updated_at":run.get::<DateTime<Utc>,_>("updated_at"),
        "confirmed_at":run.get::<Option<DateTime<Utc>>,_>("confirmed_at"),
        "completed_at":run.get::<Option<DateTime<Utc>>,_>("completed_at"),"plan_expires_at":expires,
        "pause_reason":run.get::<Option<String>,_>("pause_reason"),"preview_complete":preview_complete,
        "coverage":plan.get::<Value,_>("coverage"),"counts":{},"release_ready":false,
        "policy_revision":policy.revision(),"run_byte_ceiling":policy.run_ceiling_bytes.to_string(),
        "org_byte_ceiling":policy.org_ceiling_bytes.to_string(),"plan_revision":plan.get::<i64,_>("revision").to_string(),
        "run_budget_revision":run.get::<i64,_>("budget_revision").to_string(),
        "actions":{"confirm":qualified&&state=="ready"&&preview_complete&&plan.get::<i64,_>("eligible")>0&&(anchored||expires.is_some_and(|v|v>Utc::now())),
            "resume":qualified&&state=="paused","cancel":!s::terminal(&state),
            "increase_budget":!s::terminal(&state),"prepare_same_plan":anchored&&state=="cancelled"}});
    for column in [
        "revision",
        "retained_bytes",
        "reserved_bytes",
        "run_byte_limit",
        "org_byte_limit",
        "org_budget_revision",
    ] {
        value[column] = json!(run.get::<i64, _>(column).to_string());
    }
    for column in [
        "workspace_revision",
        "capture_revision",
        "capture_sequence",
        "parent_capture_sequence",
        "added_byte_bound",
    ] {
        value[column] = json!(plan.get::<i64, _>(column).to_string());
    }
    for column in ["occurrences", "eligible", "equal_repeats", "held"] {
        value["counts"][column] = json!(plan.get::<i64, _>(column).to_string());
    }
    for column in [
        "processed",
        "inserted",
        "already_imported",
        "application_held",
    ] {
        value["counts"][column] = json!(run.get::<i64, _>(column).to_string());
    }
    bounded(value, DETAIL_BYTES)
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0))]
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.family.is_some() || q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx, false).await?;
    if let Some(parent) = q.parent_import_id {
        if sqlx::query("SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2")
            .bind(parent)
            .bind(ctx.organization_id.0)
            .fetch_optional(&mut *tx)
            .await?
            .is_none()
        {
            return Err(MigrationError::NotFound);
        }
    }
    let cursor: Option<ListCursor> = decode(
        key,
        ctx.organization_id,
        Uuid::nil(),
        "timeline-import-list-v1",
        q.cursor.as_deref(),
    )?;
    if cursor
        .as_ref()
        .is_some_and(|c| c.parent != q.parent_import_id || c.limit != limit || c.created > c.upper)
    {
        return Err(MigrationError::InvalidInput);
    }
    let upper = match &cursor {
        Some(c) => c.upper,
        None => {
            sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(&mut *tx)
                .await?
        }
    };
    let rows=sqlx::query("SELECT id,created_at FROM migration_history_import_run WHERE organization_id=$1 AND ($2::uuid IS NULL OR parent_import_id=$2) AND created_at<=$3 AND ($4::timestamptz IS NULL OR (created_at,id)<($4,$5)) ORDER BY created_at DESC,id DESC LIMIT $6")
        .bind(ctx.organization_id.0).bind(q.parent_import_id).bind(upper).bind(cursor.as_ref().map(|c|c.created))
        .bind(cursor.as_ref().map(|c|c.id)).bind(limit+1).fetch_all(&mut *tx).await?;
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    for row in rows.iter().take(limit as usize) {
        items.push(view(&mut tx, policy, ctx, row.get("id")).await?);
    }
    let next = if more {
        let last = &rows[limit as usize - 1];
        Some(encode(
            key,
            ctx.organization_id,
            Uuid::nil(),
            "timeline-import-list-v1",
            &ListCursor {
                parent: q.parent_import_id,
                limit,
                upper,
                created: last.get("created_at"),
                id: last.get("id"),
            },
        )?)
    } else {
        None
    };
    let value = bounded(json!({"imports":items,"next_cursor":next}), RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(value)
}

pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, q, false).await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ImportPage,
) -> Result<Value, MigrationError> {
    page(pool, key, ctx, id, q, true).await
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,import_id=%id,results))]
async fn page(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: ImportPage,
    results: bool,
) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    if q.parent_import_id.is_some()
        || q.family
            .as_ref()
            .is_some_and(|f| !matches!(f.as_str(), "events" | "calls" | "text_messages"))
        || q.disposition.as_ref().is_some_and(|d| {
            if results {
                !matches!(
                    d.as_str(),
                    "imported" | "already_imported" | "equal_repeat" | "held"
                )
            } else {
                !matches!(d.as_str(), "eligible" | "equal_repeat" | "held")
            }
        })
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx, false).await?;
    let run = s::row(&mut tx, ctx.organization_id, id).await?;
    let plan_id: Uuid = run.get("plan_id");
    let plan = s::plan(&mut tx, ctx.organization_id, plan_id).await?;
    // Unclassified candidates are not a complete preview and may change identity
    // disposition. The immutable manifest becomes public only when ready.
    if !results && plan.get::<String, _>("state") != "ready" {
        return Err(MigrationError::Conflict);
    }
    let revision: i64 = run.get("revision");
    let plan_revision: i64 = plan.get("revision");
    let endpoint = if results {
        "timeline-import-results-v1"
    } else {
        "timeline-import-records-v1"
    };
    let cursor: Option<RecordCursor> =
        decode(key, ctx.organization_id, id, endpoint, q.cursor.as_deref())?;
    if let Some(c) = &cursor {
        if c.plan != plan_id
            || c.limit != limit
            || c.family != q.family
            || c.disposition != q.disposition
            || c.position <= 0
        {
            return Err(MigrationError::InvalidInput);
        }
        if c.revision != revision || c.plan_revision != plan_revision {
            return Err(MigrationError::Conflict);
        }
    }
    let after = cursor.as_ref().map_or(0, |c| c.position);
    let rows=sqlx::query(if results {
        "SELECT m.*,r.id AS result_id,r.fact_id,r.disposition AS result_disposition,r.reason AS result_reason FROM migration_history_import_result r JOIN migration_history_import_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.organization_id=r.organization_id WHERE r.owner_run_id=$1 AND r.organization_id=$2 AND r.position>$3 AND ($4::text IS NULL OR r.family=$4) AND ($5::text IS NULL OR r.disposition=$5) ORDER BY r.position LIMIT $6"
    }else{
        "SELECT m.* FROM migration_history_import_manifest m WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.position>$3 AND ($4::text IS NULL OR m.family=$4) AND ($5::text IS NULL OR m.disposition=$5) ORDER BY m.position LIMIT $6"
    }).bind(if results {id}else{plan_id}).bind(ctx.organization_id.0).bind(after).bind(q.family.as_deref())
        .bind(q.disposition.as_deref()).bind(limit+1).fetch_all(&mut *tx).await?;
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    for row in rows.iter().take(limit as usize) {
        items.push(item(&mut tx, key, ctx, plan_id, row, results).await?);
    }
    let next = if more {
        Some(encode(
            key,
            ctx.organization_id,
            id,
            endpoint,
            &RecordCursor {
                plan: plan_id,
                revision,
                plan_revision,
                limit,
                family: q.family,
                disposition: q.disposition,
                position: rows[limit as usize - 1].get("position"),
            },
        )?)
    } else {
        None
    };
    let mut value = json!({"next_cursor":next,"plan_id":plan_id,"revision":revision.to_string()});
    value[if results { "results" } else { "records" }] = Value::Array(items);
    let value = bounded(value, RESPONSE_BYTES)?;
    tx.commit().await?;
    Ok(value)
}

async fn item(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    plan: Uuid,
    row: &PgRow,
    results: bool,
) -> Result<Value, MigrationError> {
    let id: Uuid = row.get("id");
    let person: Option<Uuid> = row.get("person_id");
    let visible:bool=sqlx::query_scalar("SELECT ($1::uuid IS NULL OR EXISTS(SELECT 1 FROM person WHERE id=$1 AND organization_id=$2)) AND NOT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE organization_id=$2 AND identity_hmac=$3 AND erased_at IS NOT NULL) AND EXISTS(SELECT 1 FROM migration_history_import_display WHERE id=$4 AND plan_id=$5 AND organization_id=$2)")
        .bind(person).bind(ctx.organization_id.0).bind(row.get::<Option<Vec<u8>>,_>("identity_hmac"))
        .bind(id).bind(plan).fetch_one(&mut *conn).await?;
    let metadata = if visible {
        s::display(conn, key, ctx.organization_id, plan, id).await?
    } else {
        Value::Null
    };
    let mut value = json!({"id":id,"position":row.get::<i64,_>("position").to_string(),
        "family":row.get::<String,_>("family"),"disposition":row.get::<String,_>(if results{"result_disposition"}else{"disposition"}),
        "reason":row.get::<Option<String>,_>(if results{"result_reason"}else{"reason"}),
        "person_id":person,"metadata":metadata,"capture_id":row.get::<Uuid,_>("capture_id"),
        "ordinal":row.get::<i32,_>("ordinal"),"observation_id":row.get::<Uuid,_>("observation_id")});
    if results {
        value["result_id"] = json!(row.get::<Uuid, _>("result_id"));
        value["fact_id"] = json!(row.get::<Option<Uuid>, _>("fact_id"));
    }
    bounded(value, ROW_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_cannot_cross_org_run_endpoint_or_accept_malformed_bytes() {
        let key = RawPayloadKey::new([0x11; 32]);
        let org = OrganizationId::new(Uuid::new_v4());
        let run = Uuid::new_v4();
        let value = RecordCursor {
            plan: Uuid::new_v4(),
            revision: 1,
            plan_revision: 1,
            limit: 25,
            family: Some("events".into()),
            disposition: None,
            position: 42,
        };
        let token = encode(&key, org, run, "timeline-import-records-v1", &value).unwrap();
        let roundtrip: RecordCursor =
            decode(&key, org, run, "timeline-import-records-v1", Some(&token))
                .unwrap()
                .unwrap();
        assert_eq!(roundtrip.position, 42);
        for (scope, id, endpoint) in [
            (
                OrganizationId::new(Uuid::new_v4()),
                run,
                "timeline-import-records-v1",
            ),
            (org, Uuid::new_v4(), "timeline-import-records-v1"),
            (org, run, "timeline-import-results-v1"),
        ] {
            assert!(decode::<RecordCursor>(&key, scope, id, endpoint, Some(&token)).is_err());
        }
        for token in ["!", "AAA", "", &"x".repeat(4097)] {
            assert!(decode::<RecordCursor>(
                &key,
                org,
                run,
                "timeline-import-records-v1",
                Some(token)
            )
            .is_err());
        }
    }
}

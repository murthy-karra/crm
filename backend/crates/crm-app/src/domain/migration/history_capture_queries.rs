//! Bounded metadata-only retained reads. No source or native history access.
use super::{
    crypto, history_capture::HistoryPage, history_capture_store as s, snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
const MAX_RESPONSE: usize = 512 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordCursor {
    sequence: i64,
    last_sequence: i64,
    last_ordinal: i32,
    last_id: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunCursor {
    upper: DateTime<Utc>,
    created: DateTime<Utc>,
    id: Uuid,
}
fn encode<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    scope: &str,
    value: &T,
) -> Result<String, MigrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    let sealed = crypto::seal_history(
        key,
        org,
        run,
        Uuid::nil(),
        &format!("cursor:{scope}"),
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mut b = sealed.nonce.to_vec();
    b.extend(sealed.ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(b))
}
fn decode<T: serde::de::DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    scope: &str,
    cursor: Option<&str>,
) -> Result<Option<T>, MigrationError> {
    cursor
        .map(|v| {
            if v.len() > 4096 {
                return Err(MigrationError::InvalidInput);
            }
            let b = URL_SAFE_NO_PAD
                .decode(v)
                .map_err(|_| MigrationError::InvalidInput)?;
            if b.len() < 40 {
                return Err(MigrationError::InvalidInput);
            }
            let p = crypto::open_history(
                key,
                org,
                run,
                Uuid::nil(),
                &format!("cursor:{scope}"),
                &b[..24],
                &b[24..],
            )
            .map_err(|_| MigrationError::InvalidInput)?;
            serde_json::from_slice(&p).map_err(|_| MigrationError::InvalidInput)
        })
        .transpose()
}
fn bound(v: Value) -> Result<Value, MigrationError> {
    if serde_json::to_vec(&v)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > MAX_RESPONSE
    {
        Err(MigrationError::StorageLimit)
    } else {
        Ok(v)
    }
}
pub async fn detail(
    pool: &PgPool,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let value = view(&mut tx, policy, ctx, id).await?;
    tx.commit().await?;
    bound(value)
}
async fn view(
    conn: &mut PgConnection,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT h.*,l.byte_limit AS org_byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved,l.budget_revision AS org_budget_revision,c.status AS connection_status,c.revision AS current_connection_revision,s.started_at AS parent_source_started_at,s.completed_at AS parent_source_completed_at FROM migration_history_capture_run h JOIN migration_snapshot_storage l ON l.organization_id=h.organization_id JOIN migration_connection c ON c.id=h.connection_id AND c.organization_id=h.organization_id JOIN migration_snapshot s ON s.id=h.snapshot_id AND s.organization_id=h.organization_id WHERE h.id=$1 AND h.organization_id=$2").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    let state = r.get::<String, _>("state");
    let current = r.get::<Uuid, _>("initiated_by_user_id") == ctx.actor_user_id.0
        && s::current_profile(&r)
        && r.get::<String, _>("connection_status") == "connected"
        && r.get::<i32, _>("current_connection_revision") == r.get::<i32, _>("connection_revision");
    let reason = r.get::<Option<String>, _>("pause_reason");
    let retry = current
        && state == "paused"
        && r.get::<Option<DateTime<Utc>>, _>("confirmed_at").is_some()
        && !matches!(
            reason.as_deref(),
            Some(
                "source_identity_mismatch"
                    | "connection_changed"
                    | "parent_changed"
                    | "profile_incompatible"
            )
        );
    let mut v = json!({"id":id,"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"snapshot_id":r.get::<Uuid,_>("snapshot_id"),"connection_id":r.get::<Uuid,_>("connection_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"initiated_by_user_id":r.get::<Uuid,_>("initiated_by_user_id"),"state":state,"pause_reason":reason,"profile_version":r.get::<String,_>("profile_version"),"parser_version":r.get::<String,_>("parser_version"),"schema_version":r.get::<String,_>("schema_version"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"proposal_expires_at":r.get::<DateTime<Utc>,_>("proposal_expires_at"),"started_at":r.get::<Option<DateTime<Utc>>,_>("started_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"confirmed_at":r.get::<Option<DateTime<Utc>>,_>("confirmed_at"),"parent_source_started_at":r.get::<Option<DateTime<Utc>>,_>("parent_source_started_at"),"parent_source_completed_at":r.get::<Option<DateTime<Utc>>,_>("parent_source_completed_at"),"parent_source_user_id":r.get::<Option<i64>,_>("parent_source_user_id").map(|v|v.to_string()),"source_user_difference":r.get::<Option<i64>,_>("parent_source_user_id")!=Some(r.get("source_user_id")),"connection_revision":r.get::<i32,_>("connection_revision").to_string(),"source_user_evidence_revision":r.get::<i32,_>("source_user_evidence_revision").to_string(),"policy_revision":policy.revision(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"required_reservation_bytes":s::REQUEST_RESERVATION.to_string(),"release_ready":false,"actions":{"confirm":current&&state=="proposed"&&r.get::<DateTime<Utc>,_>("proposal_expires_at")>Utc::now(),"retry":retry,"cancel":!s::terminal(&state),"increase_budget":!s::terminal(&state)},"coverage_reasons":["api_restricted_records_unknown","detail_content_not_fetched","not_atomic_snapshot","retained_not_imported"]});
    for (wire, column) in [
        ("revision", "revision"),
        ("parent_capture_sequence", "parent_capture_sequence"),
        ("source_account_id", "source_account_id"),
        ("source_user_id", "source_user_id"),
        ("capture_sequence", "capture_sequence"),
        ("raw_bytes", "raw_bytes"),
        ("retained_bytes", "retained_bytes"),
        ("reserved_bytes", "reserved_bytes"),
        ("run_byte_limit", "run_byte_limit"),
        ("org_byte_limit", "org_byte_limit"),
        ("org_retained_bytes", "org_retained"),
        ("org_reserved_bytes", "org_reserved"),
        ("run_budget_revision", "budget_revision"),
        ("org_budget_revision", "org_budget_revision"),
    ] {
        v[wire] = json!(r.get::<i64, _>(column).to_string())
    }
    v["streams"] = streams(conn, ctx.organization_id, id).await?;
    Ok(v)
}
async fn streams(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let rows=sqlx::query("SELECT * FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 ORDER BY CASE family WHEN 'events' THEN 0 WHEN 'calls' THEN 1 ELSE 2 END").bind(id).bind(org.0).fetch_all(conn).await?;
    Ok(Value::Array(rows.into_iter().map(|r|{let mut v=json!({"family":r.get::<String,_>("family"),"state":r.get::<String,_>("state"),"reported_total":r.get::<Option<String>,_>("reported_total"),"api_inaccessible_count":null,"count_basis":"advancing_pages","content_scope":"exact_returned_json_only","enumeration_is_complete_account_history":false});for column in ["checkpoint","occurrences","valid_occurrences","invalid_occurrences","unique_ids","equal_repeats","conflicting_variants","linked","parent_excluded","no_parent_identity","invalid_person_reference","conflicting_reference","attempts"]{v[column]=json!(r.get::<i64,_>(column).to_string())}v}).collect()))
}
pub async fn list(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    page: HistoryPage,
) -> Result<Value, MigrationError> {
    if page.family.is_some() || page.disposition.is_some() || page.record_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let limit = page.limit()?;
    let scope = format!("runs:{:?}:{limit}", page.parent_import_id);
    let c: Option<RunCursor> = decode(
        key,
        ctx.organization_id,
        Uuid::nil(),
        &scope,
        page.cursor.as_deref(),
    )?;
    let upper = c.as_ref().map_or_else(Utc::now, |v| v.upper);
    let last = c.as_ref().map(|v| v.created).unwrap_or(upper);
    let id = c.as_ref().map(|v| v.id).unwrap_or(Uuid::max());
    let mut tx = s::begin(pool, ctx).await?;
    if let Some(parent) = page.parent_import_id {
        let exists =
            sqlx::query("SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2")
                .bind(parent)
                .bind(ctx.organization_id.0)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
        if !exists {
            return Err(MigrationError::NotFound);
        }
    }
    let rows=sqlx::query("SELECT id,created_at FROM migration_history_capture_run WHERE organization_id=$1 AND ($2::uuid IS NULL OR parent_import_id=$2) AND created_at<=$3 AND (created_at,id)<($4,$5) ORDER BY created_at DESC,id DESC LIMIT $6").bind(ctx.organization_id.0).bind(page.parent_import_id).bind(upper).bind(last).bind(id).bind(limit+1).fetch_all(&mut *tx).await?;
    let mut values = Vec::new();
    for r in rows.iter().take(limit as usize) {
        values.push(view(&mut tx, policy, ctx, r.get("id")).await?)
    }
    let next = if rows.len() > limit as usize {
        let r = &rows[limit as usize - 1];
        Some(encode(
            key,
            ctx.organization_id,
            Uuid::nil(),
            &scope,
            &RunCursor {
                upper,
                created: r.get("created_at"),
                id: r.get("id"),
            },
        )?)
    } else {
        None
    };
    tx.commit().await?;
    bound(json!({"captures":values,"next_cursor":next}))
}
const EFFECTIVE_LINK:&str="CASE WHEN o.identity_hmac IS NOT NULL AND EXISTS(SELECT 1 FROM migration_history_observation x WHERE x.run_id=o.run_id AND x.organization_id=o.organization_id AND x.family=o.family AND x.identity_hmac=o.identity_hmac AND x.capture_sequence<=$3 AND x.primary_person_hmac IS DISTINCT FROM o.primary_person_hmac) THEN 'conflicting_reference' ELSE o.disposition END";
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    page: HistoryPage,
) -> Result<Value, MigrationError> {
    if page.parent_import_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let limit = page.limit()?;
    if page
        .family
        .as_ref()
        .is_some_and(|v| super::history_capture_source::Stream::parse(v).is_none())
        || page.disposition.as_ref().is_some_and(|v| {
            !matches!(
                v.as_str(),
                "linked"
                    | "parent_excluded"
                    | "no_parent_identity"
                    | "invalid_person_reference"
                    | "conflicting_reference"
            )
        })
    {
        return Err(MigrationError::InvalidInput);
    }
    let scope = format!(
        "records:{:?}:{:?}:{:?}:{limit}",
        page.family, page.disposition, page.record_id
    );
    let cursor: Option<RecordCursor> = decode(
        key,
        ctx.organization_id,
        run,
        &scope,
        page.cursor.as_deref(),
    )?;
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, run).await?;
    let sequence = cursor
        .as_ref()
        .map_or(r.get("capture_sequence"), |v| v.sequence);
    if sequence < 0 || sequence > r.get::<i64, _>("capture_sequence") {
        return Err(MigrationError::InvalidInput);
    }
    let mut identity = None;
    let mut exact = None;
    let mut filter_family = None;
    if let Some(id) = page.record_id {
        let record=sqlx::query("SELECT identity_hmac,family,capture_sequence FROM migration_history_observation WHERE id=$1 AND run_id=$2 AND organization_id=$3").bind(id).bind(run).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
        if record.get::<i64, _>("capture_sequence") > sequence {
            return Err(MigrationError::InvalidInput);
        }
        identity = record.get::<Option<Vec<u8>>, _>("identity_hmac");
        filter_family = Some(record.get::<String, _>("family"));
        if identity.is_none() {
            exact = Some(id)
        }
    }
    let sql = records_sql();
    let rows = sqlx::query(&sql)
        .bind(run)
        .bind(ctx.organization_id.0)
        .bind(sequence)
        .bind(page.family)
        .bind(page.disposition)
        .bind(identity)
        .bind(filter_family)
        .bind(exact)
        .bind(cursor.as_ref().map_or(0, |v| v.last_sequence))
        .bind(cursor.as_ref().map_or(-1, |v| v.last_ordinal))
        .bind(cursor.as_ref().map_or(Uuid::nil(), |v| v.last_id))
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await?;
    let mut values = Vec::new();
    for row in rows.iter().take(limit as usize) {
        values.push(render(&mut tx, key, ctx.organization_id, run, &r, row, sequence).await?)
    }
    let next = if rows.len() > limit as usize {
        let last = &rows[limit as usize - 1];
        Some(encode(
            key,
            ctx.organization_id,
            run,
            &scope,
            &RecordCursor {
                sequence,
                last_sequence: last.get("capture_sequence"),
                last_ordinal: last.get("ordinal"),
                last_id: last.get("id"),
            },
        )?)
    } else {
        None
    };
    let counts = streams(&mut tx, ctx.organization_id, run).await?;
    tx.commit().await?;
    bound(
        json!({"records":values,"next_cursor":next,"capture_sequence":sequence.to_string(),"counter_basis":"current_run","counts":counts}),
    )
}
pub async fn record_detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let r = s::row(&mut tx, ctx.organization_id, run).await?;
    let sequence = r.get::<i64, _>("capture_sequence");
    let sql = record_detail_sql();
    let row = sqlx::query(&sql)
        .bind(run)
        .bind(ctx.organization_id.0)
        .bind(sequence)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::NotFound)?;
    let value = render(&mut tx, key, ctx.organization_id, run, &r, &row, sequence).await?;
    tx.commit().await?;
    Ok(value)
}
async fn render(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    r: &sqlx::postgres::PgRow,
    row: &sqlx::postgres::PgRow,
    sequence: i64,
) -> Result<Value, MigrationError> {
    let id = row.get::<Uuid, _>("id");
    let bytes = crypto::open_history(
        key,
        org,
        run,
        id,
        "projection",
        row.get("nonce"),
        row.get("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?;
    if !value.is_object() {
        return Err(MigrationError::Crypto);
    }
    let identity = row.get::<Option<Vec<u8>>, _>("identity_hmac");
    let counts = if let Some(identity) = identity {
        let c=sqlx::query("SELECT count(*) AS observations,count(DISTINCT semantic_hmac) AS variants FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND identity_hmac=$4 AND capture_sequence<=$5").bind(run).bind(org.0).bind(row.get::<String,_>("family")).bind(identity).bind(sequence).fetch_one(&mut *conn).await?;
        (c.get::<i64, _>("observations"), c.get::<i64, _>("variants"))
    } else {
        (1, 1)
    };
    let person = row.get::<Option<Uuid>, _>("person_id");
    let available = if let Some(person) = person {
        sqlx::query("SELECT 1 FROM person WHERE id=$1 AND organization_id=$2")
            .bind(person)
            .bind(org.0)
            .fetch_optional(&mut *conn)
            .await?
            .is_some()
    } else {
        false
    };
    value["id"] = json!(id);
    value["capture_id"] = json!(row.get::<Uuid, _>("capture_id"));
    value["capture_sequence"] = json!(row.get::<i64, _>("capture_sequence").to_string());
    value["series_capture_sequence"] = json!(sequence.to_string());
    value["ordinal"] = json!(row.get::<i32, _>("ordinal").to_string());
    value["family"] = json!(row.get::<String, _>("family"));
    value["disposition"] = json!(row.get::<String, _>("effective_link"));
    value["person_id"] = json!(if available { person } else { None });
    value["reference_available"] = json!(available);
    value["variant_count"] = json!(counts.1.to_string());
    value["observation_count"] = json!(counts.0.to_string());
    value["representation"] = json!(row.get::<String, _>("representation"));
    value["captured_at"] = json!(row.get::<DateTime<Utc>, _>("captured_at"));
    value["integrity_status"] = json!("verified_projection");
    for column in ["profile_version", "schema_version", "parser_version"] {
        value[column] = json!(r.get::<String, _>(column))
    }
    if serde_json::to_vec(&value)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > 8192
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(value)
}

pub fn records_sql() -> String {
    format!("SELECT o.*,c.captured_at,c.representation,{EFFECTIVE_LINK} AS effective_link FROM migration_history_observation o JOIN migration_history_capture c ON c.id=o.capture_id AND c.run_id=o.run_id AND c.organization_id=o.organization_id WHERE o.run_id=$1 AND o.organization_id=$2 AND o.capture_sequence<=$3 AND ($4::text IS NULL OR o.family=$4) AND ($5::text IS NULL OR ({EFFECTIVE_LINK})=$5) AND ($6::bytea IS NULL OR (o.identity_hmac=$6 AND o.family=$7)) AND ($8::uuid IS NULL OR o.id=$8) AND (o.capture_sequence,o.ordinal,o.id)>($9,$10,$11) ORDER BY o.capture_sequence,o.ordinal,o.id LIMIT $12")
}

pub fn record_detail_sql() -> String {
    format!("SELECT o.*,c.captured_at,c.representation,{EFFECTIVE_LINK} AS effective_link FROM migration_history_observation o JOIN migration_history_capture c ON c.id=o.capture_id AND c.run_id=o.run_id AND c.organization_id=o.organization_id WHERE o.run_id=$1 AND o.organization_id=$2 AND o.id=$4")
}

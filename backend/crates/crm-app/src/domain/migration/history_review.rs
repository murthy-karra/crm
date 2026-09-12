//! Bounded, metadata-only review of native and independently imported facts.
//! All families, maintained counts and cursor revisions share one MVCC snapshot.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::{activity_review, crypto, history_import};
use crate::{
    auth::{workspace, AuthContext},
    config::RawPayloadKey,
    domain::admin::Role,
    ids::{OrganizationId, PersonId},
};

const PAGE_BYTES: usize = 512 * 1024;
const SUMMARY_BYTES: usize = 4096;
const DETAIL_BYTES: usize = 16384;
const CURSOR_BYTES: usize = 4096;
const PURPOSE: &str = "history-review-v1";

#[derive(Debug)]
pub enum ReviewError {
    Forbidden,
    NotFound,
    Malformed,
    RefreshRequired,
    Unavailable,
    Database(sqlx::Error),
}
impl From<sqlx::Error> for ReviewError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
impl PageQuery {
    fn limit(&self) -> Result<usize, ReviewError> {
        let limit = self.limit.unwrap_or(25);
        if !(1..=50).contains(&limit)
            || self.cursor.as_ref().is_some_and(|s| s.len() > CURSOR_BYTES)
        {
            return Err(ReviewError::Malformed);
        }
        Ok(limit)
    }
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    #[default]
    All,
    Native,
    Events,
    Calls,
    TextMessages,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Dated {
    #[default]
    Known,
    Unknown,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    pub family: Option<Family>,
    pub dated: Option<Dated>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Serialize)]
pub struct Page {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub read_revision: String,
}
struct Scope {
    org: OrganizationId,
    person: PersonId,
    parent: Uuid,
    snapshot: Uuid,
    workspace_revision: i64,
    revision: i64,
    counts: Value,
}

pub const STATE_SQL: &str = "SELECT revision,counts FROM migration_history_review_state WHERE organization_id=$1 AND person_id=$2";
async fn begin<'a>(
    pool: &'a PgPool,
    auth: &AuthContext,
    person: PersonId,
) -> Result<(Transaction<'a, Postgres>, Scope), ReviewError> {
    if auth.role != Role::Admin {
        return Err(ReviewError::Forbidden);
    }
    let org = auth.active_organization_id;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, org).await?;
    let member=sqlx::query("SELECT role,status FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR SHARE").bind(org.0).bind(auth.actor_user_id.0).fetch_optional(&mut *tx).await?;
    if !member.is_some_and(|r| {
        r.get::<String, _>("role") == "admin" && r.get::<String, _>("status") == "active"
    }) {
        return Err(ReviewError::Forbidden);
    }
    let organization = sqlx::query(
        "SELECT workspace_mode,workspace_revision FROM organization WHERE id=$1 FOR SHARE",
    )
    .bind(org.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ReviewError::NotFound)?;
    if organization.get::<String, _>("workspace_mode") != "migration_review" {
        return Err(ReviewError::NotFound);
    }
    // The guard and its compiled capability run on this actual connection;
    // nested savepoints cannot leak a permit to another pooled connection.
    workspace::with_reader(auth, async {
        let guard = workspace::read(&mut tx, org).await?;
        guard.commit().await
    })
    .await?;
    let parent = sqlx::query(activity_review::REVIEW_BINDING_SQL)
        .bind(org.0)
        .bind(person.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReviewError::NotFound)?;
    let state = sqlx::query(STATE_SQL)
        .bind(org.0)
        .bind(person.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReviewError::Unavailable)?;
    let scope = Scope {
        org,
        person,
        parent: parent.get("import_id"),
        snapshot: parent.get("snapshot_id"),
        workspace_revision: organization.get("workspace_revision"),
        revision: state.get("revision"),
        counts: state.get("counts"),
    };
    Ok((tx, scope))
}
fn bounded(value: &Value, max: usize) -> Result<(), ReviewError> {
    if serde_json::to_vec(value)
        .map_err(|_| ReviewError::Unavailable)?
        .len()
        > max
    {
        Err(ReviewError::Unavailable)
    } else {
        Ok(())
    }
}
fn count(scope: &Scope, key: &str) -> Result<i64, ReviewError> {
    scope
        .counts
        .get(key)
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
        .ok_or(ReviewError::Unavailable)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    time: Option<DateTime<Utc>>,
    position: Option<i64>,
    recorded: DateTime<Utc>,
    rank: i16,
    id: Uuid,
    kind: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    endpoint: String,
    family: Family,
    dated: Dated,
    limit: usize,
    workspace_revision: i64,
    revision: i64,
    last: Key,
}
#[allow(clippy::too_many_arguments)]
fn encode(
    key: &RawPayloadKey,
    scope: &Scope,
    endpoint: &str,
    family: Family,
    dated: Dated,
    limit: usize,
    last: Key,
) -> Result<String, ReviewError> {
    let plain = serde_json::to_vec(&Cursor {
        endpoint: endpoint.into(),
        family,
        dated,
        limit,
        workspace_revision: scope.workspace_revision,
        revision: scope.revision,
        last,
    })
    .map_err(|_| ReviewError::Unavailable)?;
    let sealed = crypto::seal_snapshot(
        key,
        scope.org,
        scope.snapshot,
        scope.person.0,
        &format!("{PURPOSE}:{}", scope.parent),
        &plain,
    )
    .map_err(|_| ReviewError::Unavailable)?;
    let mut bytes = sealed.nonce.to_vec();
    bytes.extend(sealed.ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn decode(
    key: &RawPayloadKey,
    scope: &Scope,
    query: &PageQuery,
    endpoint: &str,
    family: Family,
    dated: Dated,
) -> Result<Option<Key>, ReviewError> {
    let limit = query.limit()?;
    let Some(token) = &query.cursor else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| ReviewError::Malformed)?;
    if bytes.len() <= 24 {
        return Err(ReviewError::Malformed);
    }
    let plain = crypto::open_snapshot(
        key,
        scope.org,
        scope.snapshot,
        scope.person.0,
        &format!("{PURPOSE}:{}", scope.parent),
        &bytes[..24],
        &bytes[24..],
    )
    .map_err(|_| ReviewError::Malformed)?;
    let c: Cursor = serde_json::from_slice(&plain).map_err(|_| ReviewError::Malformed)?;
    if c.endpoint != endpoint || c.family != family || c.dated != dated || c.limit != limit {
        return Err(ReviewError::Malformed);
    }
    if c.revision != scope.revision || c.workspace_revision != scope.workspace_revision {
        return Err(ReviewError::RefreshRequired);
    }
    if (dated == Dated::Known && (c.last.time.is_none() || c.last.position.is_some()))
        || (dated == Dated::Unknown && (c.last.position.is_none() || c.last.time.is_some()))
    {
        return Err(ReviewError::Malformed);
    }
    Ok(Some(c.last))
}

#[derive(Clone, Copy)]
struct Kind {
    name: &'static str,
    table: &'static str,
    rank: i16,
    family: Family,
}
const KINDS: [Kind; 11] = [
    Kind {
        name: "person_imported",
        table: "person_imported",
        rank: 0,
        family: Family::Native,
    },
    Kind {
        name: "inquiry_received",
        table: "inquiry_received",
        rank: 0,
        family: Family::Native,
    },
    Kind {
        name: "routing_decision",
        table: "routing_decision",
        rank: 1,
        family: Family::Native,
    },
    Kind {
        name: "assignment_changed",
        table: "assignment_changed",
        rank: 2,
        family: Family::Native,
    },
    Kind {
        name: "stage_changed",
        table: "stage_changed",
        rank: 3,
        family: Family::Native,
    },
    Kind {
        name: "contact_attempted",
        table: "contact_attempted",
        rank: 4,
        family: Family::Native,
    },
    Kind {
        name: "call_completed",
        table: "call_completed",
        rank: 5,
        family: Family::Native,
    },
    Kind {
        name: "correspondence",
        table: "correspondence_captured",
        rank: 6,
        family: Family::Native,
    },
    Kind {
        name: "fub_event_record_imported",
        table: "fub_event_record_imported",
        rank: 9,
        family: Family::Events,
    },
    Kind {
        name: "fub_call_record_imported",
        table: "fub_call_record_imported",
        rank: 10,
        family: Family::Calls,
    },
    Kind {
        name: "fub_text_record_imported",
        table: "fub_text_record_imported",
        rank: 11,
        family: Family::TextMessages,
    },
];
const ACTOR:&str="CASE WHEN a.id IS NULL THEN NULL ELSE jsonb_build_object('id',a.id,'display_name',a.display_name) END";
fn reference(alias: &str, label: &str) -> String {
    format!("CASE WHEN {alias}.id IS NULL THEN NULL ELSE jsonb_build_object('id',{alias}.id,'{label}',{alias}.{label}) END")
}
fn native_parts(kind: Kind) -> (String, String) {
    let from_user = reference("fu", "display_name");
    let to_user = reference("tu", "display_name");
    let from_stage = reference("fs", "name");
    let to_stage = reference("ts", "name");
    match kind.name {
        "person_imported"=>("jsonb_build_object('import_id',f.import_id,'plan_id',f.plan_id,'source_record_id',f.source_record_id,'capture_id',f.capture_id,'on_behalf_of_user_id',f.on_behalf_of_user_id)".into(),String::new()),
        "inquiry_received"=>("jsonb_build_object('inquiry_id',f.inquiry_id,'source',f.source,'person_created',f.person_created,'matched_by',f.matched_by)".into(),String::new()),
        "routing_decision"=>(format!("jsonb_build_object('inquiry_id',f.inquiry_id,'strategy',f.strategy,'assignee',{to_user})")," LEFT JOIN app_user tu ON tu.id=f.assignee_user_id".into()),
        "assignment_changed"=>(format!("jsonb_build_object('from',{from_user},'to',{to_user},'reason',f.reason)")," LEFT JOIN app_user fu ON fu.id=f.from_user_id LEFT JOIN app_user tu ON tu.id=f.to_user_id".into()),
        "stage_changed"=>(format!("jsonb_build_object('from_stage',{from_stage},'to_stage',{to_stage},'reason',f.reason)")," LEFT JOIN stage fs ON fs.id=f.from_stage_id AND fs.organization_id=f.organization_id JOIN stage ts ON ts.id=f.to_stage_id AND ts.organization_id=f.organization_id".into()),
        "contact_attempted"=>("jsonb_build_object('channel',f.channel,'outcome',f.outcome,'call_id',cl.id,'corrects_id',f.corrects_id,'superseded',correction.id IS NOT NULL)".into()," LEFT JOIN call cl ON cl.id=f.causation_id AND cl.organization_id=f.organization_id LEFT JOIN LATERAL (SELECT c.id FROM contact_attempted c WHERE c.corrects_id=f.id AND c.organization_id=f.organization_id AND c.corrects_id IS NOT NULL LIMIT 1) correction ON true".into()),
        "call_completed"=>("jsonb_build_object('call_id',f.call_id,'outcome',f.outcome,'talk_seconds',f.talk_seconds,'answered_at',f.answered_at)".into(),String::new()),
        "correspondence"=>(format!("jsonb_build_object('direction',f.direction,'agent',{ACTOR},'captured_at',f.recorded_at,'via',f.via,'backdated',f.backdated)"),String::new()),
        _=>unreachable!(),
    }
}
/// SQL text is assembled only from the closed server-owned kind inventory.
/// The final kind discriminator preserves existing ranks, including the two
/// rank-zero kinds, even when different fact tables deliberately share a UUID.
fn candidate_sql(kind: Kind, dated: Dated, detail: bool, after: Option<&Key>) -> String {
    let external = kind.family != Family::Native;
    let display = if external {
        "f.source_created_at"
    } else if kind.name == "contact_attempted" {
        "CASE WHEN f.corrects_id IS NOT NULL THEN f.recorded_at ELSE f.occurred_at END"
    } else {
        "f.occurred_at"
    };
    let actor_column = if kind.name == "correspondence" {
        "on_behalf_of_user_id"
    } else {
        "actor_user_id"
    };
    let actor = if matches!(kind.name, "person_imported" | "correspondence") {
        "NULL::jsonb"
    } else {
        ACTOR
    };
    let (metadata, joins) = if external {
        ("NULL::jsonb".into()," JOIN migration_history_import_identity hi ON hi.id=f.identity_id AND hi.organization_id=f.organization_id AND hi.erased_at IS NULL JOIN migration_history_import_display hd ON hd.id=f.manifest_id AND hd.organization_id=f.organization_id AND hd.plan_id=f.plan_id".into())
    } else {
        native_parts(kind)
    };
    let extra = if external {
        "f.stable_position,f.plan_id,f.attempt_id,f.manifest_id,f.identity_id,f.source_time_basis"
    } else {
        "NULL::bigint AS stable_position,NULL::uuid AS plan_id,NULL::uuid AS attempt_id,NULL::uuid AS manifest_id,NULL::uuid AS identity_id,NULL::text AS source_time_basis"
    };
    let mut sql=format!("SELECT f.id,f.occurred_at,f.recorded_at,f.origin,f.correlation_id,{display} AS display_at,{actor} AS actor,CASE WHEN octet_length(m.value::text)<=16384 THEN m.value ELSE NULL END AS metadata,octet_length(m.value::text)>16384 AS metadata_overflow,{extra} FROM {} f LEFT JOIN app_user a ON a.id=f.{actor_column}{joins} CROSS JOIN LATERAL (SELECT {metadata} AS value) m WHERE f.organization_id=$1 AND f.person_id=$2",kind.table);
    if detail {
        sql.push_str(" AND f.id=$3 LIMIT 1");
        return sql;
    }
    if external {
        sql.push_str(if dated == Dated::Known {
            " AND f.source_created_at IS NOT NULL"
        } else {
            " AND f.source_created_at IS NULL"
        });
    }
    let position = if dated == Dated::Known {
        display
    } else {
        "f.stable_position"
    };
    // Preserve a fixed bind shape while allowing a direct index range for the
    // current family. Rank is constant within a table, so inserting it between
    // indexed columns in a row comparison needlessly weakens deep-page seeks.
    sql = sql.replace(" CROSS JOIN LATERAL", " CROSS JOIN (SELECT $3::timestamptz AS at,$4::timestamptz AS recorded,$5::smallint AS rank,$6::uuid AS id,$7::text AS kind,$9::bigint AS position) boundary CROSS JOIN LATERAL");
    let cursor_position = if dated == Dated::Known {
        "boundary.at"
    } else {
        "boundary.position"
    };
    if let Some(after) = after {
        match kind.rank.cmp(&after.rank) {
            std::cmp::Ordering::Less => sql.push_str(&format!(
                " AND ({position},f.recorded_at)<=({cursor_position},boundary.recorded)"
            )),
            std::cmp::Ordering::Greater => sql.push_str(&format!(
                " AND ({position},f.recorded_at)<({cursor_position},boundary.recorded)"
            )),
            std::cmp::Ordering::Equal => {
                let comparison = if kind.name < after.kind.as_str() {
                    "<="
                } else {
                    "<"
                };
                sql.push_str(&format!(" AND ({position},f.recorded_at,f.id){comparison}({cursor_position},boundary.recorded,boundary.id)"));
            }
        }
    }
    sql.push_str(&format!(
        " ORDER BY {position} DESC,f.recorded_at DESC,f.id DESC LIMIT $8"
    ));
    sql
}
/// The performance harness explains the actual closed reader statement.
#[cfg(feature = "test-support")]
pub fn candidate_sql_for_test(kind: &str, dated: Dated) -> Option<String> {
    KINDS
        .iter()
        .find(|k| k.name == kind)
        .map(|k| candidate_sql(*k, dated, false, None))
}
#[cfg(feature = "test-support")]
pub fn candidate_after_sql_for_test(kind: &str, dated: Dated, after_kind: &str) -> Option<String> {
    let kind = *KINDS.iter().find(|k| k.name == kind)?;
    let after_kind = *KINDS.iter().find(|k| k.name == after_kind)?;
    let key = Key {
        time: None,
        position: None,
        recorded: Utc::now(),
        rank: after_kind.rank,
        id: Uuid::nil(),
        kind: after_kind.name.into(),
    };
    Some(candidate_sql(kind, dated, false, Some(&key)))
}
fn row_key(row: &PgRow, kind: Kind, dated: Dated) -> Result<Key, ReviewError> {
    Ok(Key {
        time: if dated == Dated::Known {
            row.try_get("display_at")?
        } else {
            None
        },
        position: if dated == Dated::Unknown {
            row.try_get("stable_position")?
        } else {
            None
        },
        recorded: row.try_get("recorded_at")?,
        rank: kind.rank,
        id: row.try_get("id")?,
        kind: kind.name.into(),
    })
}
fn compare(a: &Key, b: &Key) -> std::cmp::Ordering {
    (a.time, a.position, a.recorded, a.rank, a.id, &a.kind)
        .cmp(&(b.time, b.position, b.recorded, b.rank, b.id, &b.kind))
}
async fn value(
    tx: &mut Transaction<'_, Postgres>,
    key: &RawPayloadKey,
    scope: &Scope,
    row: &PgRow,
    kind: Kind,
    detail: bool,
) -> Result<Value, ReviewError> {
    if row
        .try_get::<Option<bool>, _>("metadata_overflow")?
        .unwrap_or(false)
    {
        return Err(ReviewError::Unavailable);
    }
    let external = kind.family != Family::Native;
    let metadata = if external {
        history_import::display(
            tx,
            key,
            scope.org,
            row.try_get("plan_id")?,
            row.try_get("manifest_id")?,
        )
        .await
        .map_err(|_| ReviewError::Unavailable)?
    } else {
        row.try_get("metadata")?
    };
    if kind.name == "correspondence" && metadata["agent"].is_null() {
        return Err(ReviewError::Unavailable);
    }
    let id: Uuid = row.try_get("id")?;
    let mut result = json!({"kind":kind.name,"id":id,"display_at":row.try_get::<Option<DateTime<Utc>>,_>("display_at")?,"occurred_at":row.try_get::<DateTime<Utc>,_>("occurred_at")?,"recorded_at":row.try_get::<DateTime<Utc>,_>("recorded_at")?,"actor":row.try_get::<Option<Value>,_>("actor")?,"origin":row.try_get::<String,_>("origin")?,"metadata":metadata,"detail_url":format!("/api/people/{}/migration-review/timeline/{}/{id}",scope.person.0,kind.name)});
    if detail {
        result["read_revision"] = json!(scope.revision.to_string());
        result["correlation_id"] = json!(row.try_get::<Uuid, _>("correlation_id")?);
        result["provenance"] = if external {
            json!({"plan_id":row.try_get::<Uuid,_>("plan_id")?,"attempt_id":row.try_get::<Uuid,_>("attempt_id")?,"manifest_id":row.try_get::<Uuid,_>("manifest_id")?,"identity_id":row.try_get::<Uuid,_>("identity_id")?,"source_time_basis":row.try_get::<String,_>("source_time_basis")?,"stable_position":row.try_get::<i64,_>("stable_position")?.to_string()})
        } else {
            Value::Null
        };
    }
    bounded(&result, if detail { DETAIL_BYTES } else { SUMMARY_BYTES })?;
    Ok(result)
}
#[cfg(feature = "test-support")]
#[derive(Default)]
pub struct SnapshotTestGate {
    pub reached: tokio::sync::Notify,
    pub proceed: tokio::sync::Notify,
}
#[cfg(feature = "test-support")]
pub async fn timeline_with_test_gate(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    query: &TimelineQuery,
    gate: &SnapshotTestGate,
) -> Result<Page, ReviewError> {
    timeline_inner(pool, key, auth, person, query, Some(gate)).await
}
pub async fn timeline(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    query: &TimelineQuery,
) -> Result<Page, ReviewError> {
    timeline_inner(
        pool,
        key,
        auth,
        person,
        query,
        #[cfg(feature = "test-support")]
        None,
    )
    .await
}
async fn timeline_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    query: &TimelineQuery,
    #[cfg(feature = "test-support")] gate: Option<&SnapshotTestGate>,
) -> Result<Page, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    #[cfg(feature = "test-support")]
    let mut gate = gate;
    let page = PageQuery {
        limit: query.limit,
        cursor: query.cursor.clone(),
    };
    let limit = page.limit()?;
    let family = query.family.unwrap_or_default();
    let dated = query.dated.unwrap_or_default();
    let after = decode(key, &scope, &page, "timeline", family, dated)?;
    let mut candidates = Vec::new();
    for kind in KINDS.into_iter().filter(|k| {
        (family == Family::All || family == k.family)
            && (dated == Dated::Known || k.family != Family::Native)
    }) {
        let rows = sqlx::query(&candidate_sql(kind, dated, false, after.as_ref()))
            .bind(scope.org.0)
            .bind(person.0)
            .bind(after.as_ref().and_then(|k| k.time))
            .bind(after.as_ref().map(|k| k.recorded))
            .bind(after.as_ref().map(|k| k.rank))
            .bind(after.as_ref().map(|k| k.id))
            .bind(after.as_ref().map(|k| k.kind.as_str()))
            .bind((limit + 1) as i64)
            .bind(after.as_ref().and_then(|k| k.position))
            .fetch_all(&mut *tx)
            .await?;
        for row in rows {
            candidates.push((row_key(&row, kind, dated)?, kind, row));
        }
        #[cfg(feature = "test-support")]
        if let Some(g) = gate.take() {
            g.reached.notify_one();
            g.proceed.notified().await;
        }
    }
    candidates.sort_by(|a, b| compare(&b.0, &a.0));
    let more = candidates.len() > limit;
    candidates.truncate(limit);
    let last = candidates.last().map(|r| r.0.clone());
    let mut items = Vec::with_capacity(candidates.len());
    for (_, kind, row) in candidates {
        items.push(value(&mut tx, key, &scope, &row, kind, false).await?);
    }
    let next_cursor = if more {
        Some(encode(
            key,
            &scope,
            "timeline",
            family,
            dated,
            limit,
            last.ok_or(ReviewError::Unavailable)?,
        )?)
    } else {
        None
    };
    let result = Page {
        items,
        next_cursor,
        read_revision: scope.revision.to_string(),
    };
    bounded(
        &serde_json::to_value(&result).map_err(|_| ReviewError::Unavailable)?,
        PAGE_BYTES,
    )?;
    Ok(result)
}
pub async fn entry(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    kind: &str,
    id: Uuid,
) -> Result<Value, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let kind = KINDS
        .iter()
        .find(|k| k.name == kind)
        .copied()
        .ok_or(ReviewError::Malformed)?;
    let row = sqlx::query(&candidate_sql(kind, Dated::Known, true, None))
        .bind(scope.org.0)
        .bind(person.0)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReviewError::NotFound)?;
    value(&mut tx, key, &scope, &row, kind, true).await
}

pub const INQUIRIES_SQL: &str = r#"SELECT id,received_at,CASE WHEN octet_length(source)<=4096 THEN source ELSE NULL END AS source,CASE WHEN octet_length(source_external_id)<=4096 THEN source_external_id ELSE NULL END AS source_external_id,octet_length(source)>4096 OR COALESCE(octet_length(source_external_id)>4096,false) AS overflow FROM inquiry WHERE organization_id=$1 AND person_id=$2 AND ($3::timestamptz IS NULL OR (received_at,id)<($3,$4::uuid)) ORDER BY received_at DESC,id DESC LIMIT $5"#;
pub async fn inquiries(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    query: &PageQuery,
) -> Result<Page, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let limit = query.limit()?;
    let cursor = decode(key, &scope, query, "inquiries", Family::All, Dated::Known)?;
    let mut rows = sqlx::query(INQUIRIES_SQL)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(cursor.as_ref().and_then(|c| c.time))
        .bind(cursor.as_ref().map(|c| c.id))
        .bind((limit + 1) as i64)
        .fetch_all(&mut *tx)
        .await?;
    let more = rows.len() > limit;
    rows.truncate(limit);
    let mut items = Vec::new();
    let mut last = None;
    for row in rows {
        if row.try_get::<bool, _>("overflow")? {
            return Err(ReviewError::Unavailable);
        }
        let time: DateTime<Utc> = row.try_get("received_at")?;
        let id: Uuid = row.try_get("id")?;
        let item = json!({"id":id,"source":row.try_get::<String,_>("source")?,"source_external_id":row.try_get::<Option<String>,_>("source_external_id")?,"received_at":time});
        bounded(&item, SUMMARY_BYTES)?;
        items.push(item);
        last = Some(Key {
            time: Some(time),
            position: None,
            recorded: time,
            rank: 0,
            id,
            kind: "inquiry".into(),
        });
    }
    let next_cursor = if more {
        Some(encode(
            key,
            &scope,
            "inquiries",
            Family::All,
            Dated::Known,
            limit,
            last.ok_or(ReviewError::Unavailable)?,
        )?)
    } else {
        None
    };
    Ok(Page {
        items,
        next_cursor,
        read_revision: scope.revision.to_string(),
    })
}

pub const CORE_PERSON_SQL: &str = r#"SELECT CASE WHEN octet_length(v::text)<=524288 THEN v ELSE NULL END AS value FROM (
 SELECT jsonb_build_object('id',p.id,'first_name',p.first_name,'last_name',p.last_name,
 'display_name',COALESCE(NULLIF(concat_ws(' ',NULLIF(p.first_name,''),NULLIF(p.last_name,'')),''),em.value,ph.value,''),
 'stage',jsonb_build_object('id',s.id,'name',s.name),'assigned_user',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('id',u.id,'display_name',u.display_name) END,
 'primary_email',em.value,'primary_phone',ph.value,'inquiry_count',$3::text,'last_inquiry_at',li.received_at,'created_at',p.created_at) AS v
 FROM person p JOIN stage s ON s.id=p.stage_id AND s.organization_id=p.organization_id LEFT JOIN app_user u ON u.id=p.assigned_user_id
 LEFT JOIN LATERAL (SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='email' ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 1) em ON true
 LEFT JOIN LATERAL (SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='phone' ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 1) ph ON true
 LEFT JOIN LATERAL (SELECT received_at FROM inquiry WHERE organization_id=p.organization_id AND person_id=p.id ORDER BY received_at DESC,id DESC LIMIT 1) li ON true
 WHERE p.organization_id=$1 AND p.id=$2) p"#;
const CORE_CONTACTS:&str="SELECT jsonb_build_object('id',id,'kind',kind,'value',value) AS v FROM contact_method WHERE organization_id=$1 AND person_id=$2 ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 51 OFFSET $3";
const CORE_TAGS:&str="SELECT jsonb_build_object('id',t.id,'name',t.name) AS v FROM person_tag p JOIN tag t ON t.id=p.tag_id AND t.organization_id=p.organization_id WHERE p.organization_id=$1 AND p.person_id=$2 ORDER BY lower(t.name),t.id LIMIT 51 OFFSET $3";
const CORE_FIELDS: &str = r#"SELECT jsonb_build_object('field_id',v.field_id,'label',cf.label,'field_type',cf.field_type,
'value',CASE cf.field_type WHEN 'text' THEN jsonb_build_object('text',v.text_value) WHEN 'number' THEN jsonb_build_object('number',trim_scale(v.number_value)::text) WHEN 'date' THEN jsonb_build_object('date',v.date_value) WHEN 'choice' THEN jsonb_build_object('option_id',v.option_id) END,
'option_label',o.label,'updated_at',v.updated_at) AS v
FROM person_custom_field_value v JOIN custom_field cf ON cf.id=v.field_id AND cf.organization_id=v.organization_id LEFT JOIN custom_field_option o ON o.id=v.option_id AND o.organization_id=v.organization_id
WHERE v.organization_id=$1 AND v.person_id=$2 AND cf.archived_at IS NULL ORDER BY cf.position,cf.id LIMIT 51 OFFSET $3"#;
#[cfg(feature = "test-support")]
pub fn core_collection_sql_for_test(kind: &str) -> Option<String> {
    let sql = match kind {
        "contacts" => CORE_CONTACTS,
        "tags" => CORE_TAGS,
        "custom_fields" => CORE_FIELDS,
        _ => return None,
    };
    Some(format!("SELECT CASE WHEN octet_length(v::text)<=524288 THEN v ELSE NULL END AS value FROM ({sql}) bounded"))
}
#[cfg(feature = "test-support")]
pub fn entry_sql_for_test(kind: &str) -> Option<String> {
    KINDS
        .iter()
        .find(|k| k.name == kind)
        .map(|k| candidate_sql(*k, Dated::Known, true, None))
}
async fn core_collection(
    tx: &mut Transaction<'_, Postgres>,
    scope: &Scope,
    sql: &str,
    bytes: &mut usize,
) -> Result<Vec<Value>, ReviewError> {
    let sql=format!("SELECT CASE WHEN octet_length(v::text)<=524288 THEN v ELSE NULL END AS value FROM ({sql}) bounded");
    let mut values = Vec::new();
    loop {
        let rows = sqlx::query(&sql)
            .bind(scope.org.0)
            .bind(scope.person.0)
            .bind(values.len() as i64)
            .fetch_all(&mut **tx)
            .await?;
        let done = rows.len() < 51;
        for row in rows {
            let v: Option<Value> = row.try_get("value")?;
            let v = v.ok_or(ReviewError::Unavailable)?;
            *bytes += serde_json::to_vec(&v)
                .map_err(|_| ReviewError::Unavailable)?
                .len()
                + 1;
            if *bytes > PAGE_BYTES {
                return Err(ReviewError::Unavailable);
            }
            values.push(v);
        }
        if done {
            return Ok(values);
        }
    }
}
pub async fn core(
    pool: &PgPool,
    auth: &AuthContext,
    person: PersonId,
) -> Result<Value, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let inquiry_count = count(&scope, "inquiries")?.to_string();
    let person_value: Option<Value> = sqlx::query_scalar(CORE_PERSON_SQL)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(&inquiry_count)
        .fetch_one(&mut *tx)
        .await?;
    let person_value = person_value.ok_or(ReviewError::Unavailable)?;
    let mut bytes = serde_json::to_vec(&person_value)
        .map_err(|_| ReviewError::Unavailable)?
        .len();
    let contact_methods = core_collection(&mut tx, &scope, CORE_CONTACTS, &mut bytes).await?;
    let tags = core_collection(&mut tx, &scope, CORE_TAGS, &mut bytes).await?;
    let custom_fields = core_collection(&mut tx, &scope, CORE_FIELDS, &mut bytes).await?;
    let activity_revision: i64 = sqlx::query_scalar(activity_review::REVIEW_REVISION_SQL)
        .bind(scope.org.0)
        .fetch_one(&mut *tx)
        .await?;
    let mut counts = serde_json::Map::new();
    let mut known = 0i64;
    let mut unknown = 0i64;
    for kind in KINDS {
        let n = if kind.family == Family::Native {
            let n = count(&scope, kind.name)?;
            known = known.checked_add(n).ok_or(ReviewError::Unavailable)?;
            n
        } else {
            let k = count(&scope, &format!("{}_known", kind.name))?;
            let u = count(&scope, &format!("{}_unknown", kind.name))?;
            known = known.checked_add(k).ok_or(ReviewError::Unavailable)?;
            unknown = unknown.checked_add(u).ok_or(ReviewError::Unavailable)?;
            k.checked_add(u).ok_or(ReviewError::Unavailable)?
        };
        counts.insert(kind.name.into(), json!(n.to_string()));
    }
    let result = json!({"person":person_value,"contact_methods":contact_methods,"tags":tags,"custom_fields":custom_fields,
        "activity":{"notes_count":count(&scope,"notes")?.to_string(),"open_tasks_count":count(&scope,"open_tasks")?.to_string(),"completed_tasks_count":count(&scope,"completed_tasks")?.to_string(),"activity_revision":activity_revision.to_string(),"notes_url":format!("/api/people/{}/migration-review/notes",person.0),"tasks_url":format!("/api/people/{}/migration-review/tasks",person.0)},
        "inquiries":{"count":inquiry_count,"url":format!("/api/people/{}/migration-review/inquiries",person.0)},
        "history":{"read_revision":scope.revision.to_string(),"known_count":known.to_string(),"unknown_count":unknown.to_string(),"counts":counts,"timeline_url":format!("/api/people/{}/migration-review/timeline",person.0)}});
    bounded(&result, PAGE_BYTES)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        Scope {
            org: OrganizationId::new(Uuid::new_v4()),
            person: PersonId::new(Uuid::new_v4()),
            parent: Uuid::new_v4(),
            snapshot: Uuid::new_v4(),
            workspace_revision: 2,
            revision: 9,
            counts: json!({}),
        }
    }
    fn last() -> Key {
        Key {
            time: Some(Utc::now()),
            position: None,
            recorded: Utc::now(),
            rank: 4,
            id: Uuid::new_v4(),
            kind: "contact_attempted".into(),
        }
    }
    #[test]
    fn cursor_binds_scope_endpoint_filters_limit_and_revisions() {
        let key = RawPayloadKey::new([8; 32]);
        let mut scope = scope();
        let mut q = PageQuery {
            limit: Some(25),
            cursor: Some(
                encode(
                    &key,
                    &scope,
                    "timeline",
                    Family::All,
                    Dated::Known,
                    25,
                    last(),
                )
                .unwrap(),
            ),
        };
        assert!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known)
                .unwrap()
                .is_some()
        );
        for (endpoint, family, dated) in [
            ("inquiries", Family::All, Dated::Known),
            ("timeline", Family::Native, Dated::Known),
            ("timeline", Family::All, Dated::Unknown),
        ] {
            assert!(matches!(
                decode(&key, &scope, &q, endpoint, family, dated),
                Err(ReviewError::Malformed)
            ));
        }
        q.limit = Some(50);
        assert!(matches!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known),
            Err(ReviewError::Malformed)
        ));
        q.limit = Some(25);
        scope.revision += 1;
        assert!(matches!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known),
            Err(ReviewError::RefreshRequired)
        ));
        scope.revision -= 1;
        scope.workspace_revision += 1;
        assert!(matches!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known),
            Err(ReviewError::RefreshRequired)
        ));
        scope.workspace_revision -= 1;
        let original = scope.person;
        scope.person = PersonId::new(Uuid::new_v4());
        assert!(matches!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known),
            Err(ReviewError::Malformed)
        ));
        scope.person = original;
        scope.org = OrganizationId::new(Uuid::new_v4());
        assert!(matches!(
            decode(&key, &scope, &q, "timeline", Family::All, Dated::Known),
            Err(ReviewError::Malformed)
        ));
    }
    #[test]
    fn unknown_order_and_duplicate_native_rank_are_total() {
        let mut a = last();
        let mut b = a.clone();
        a.kind = "person_imported".into();
        b.kind = "inquiry_received".into();
        assert_ne!(compare(&a, &b), std::cmp::Ordering::Equal);
        a.time = None;
        b.time = None;
        a.position = Some(1);
        b.position = Some(2);
        a.recorded = b.recorded + chrono::Duration::days(10);
        assert!(compare(&a, &b).is_lt());
        let scope = scope();
        let key = RawPayloadKey::new([9; 32]);
        let q = PageQuery {
            limit: None,
            cursor: Some(
                encode(&key, &scope, "timeline", Family::All, Dated::Unknown, 25, a).unwrap(),
            ),
        };
        assert!(decode(&key, &scope, &q, "timeline", Family::All, Dated::Unknown).is_ok());
    }
    #[test]
    fn bounds_and_closed_filters_fail_without_echoing_content() {
        assert_eq!(PageQuery::default().limit().unwrap(), 25);
        for limit in [0, 51, usize::MAX] {
            assert!(PageQuery {
                limit: Some(limit),
                cursor: None
            }
            .limit()
            .is_err());
        }
        assert!(serde_json::from_value::<TimelineQuery>(json!({"family":"arbitrary"})).is_err());
        assert!(serde_json::from_value::<TimelineQuery>(json!({"dated":"all"})).is_err());
        assert!(serde_json::from_value::<PageQuery>(json!({"body":true})).is_err());
        assert!(bounded(&json!("x".repeat(SUMMARY_BYTES)), SUMMARY_BYTES).is_err());
    }
    #[test]
    fn family_boundaries_reduce_to_indexed_range_without_losing_rank_ties() {
        let mut boundary = last();
        boundary.rank = 0;
        boundary.kind = "person_imported".into();
        let earlier = candidate_sql(KINDS[1], Dated::Known, false, Some(&boundary));
        assert!(
            earlier.contains("f.recorded_at,f.id)<=(boundary.at,boundary.recorded,boundary.id)")
        );
        let same = candidate_sql(KINDS[0], Dated::Known, false, Some(&boundary));
        assert!(same.contains("f.recorded_at,f.id)<(boundary.at,boundary.recorded,boundary.id)"));
        let later = candidate_sql(KINDS[2], Dated::Known, false, Some(&boundary));
        assert!(later.contains("f.recorded_at)<(boundary.at,boundary.recorded)"));
        boundary.rank = 11;
        let lower = candidate_sql(KINDS[9], Dated::Unknown, false, Some(&boundary));
        assert!(lower
            .contains("f.stable_position,f.recorded_at)<=(boundary.position,boundary.recorded)"));
    }
    #[test]
    fn query_inventory_preserves_native_correction_time_and_fixed_candidate_limits() {
        assert_eq!(KINDS.len(), 11);
        for kind in KINDS {
            let sql = candidate_sql(kind, Dated::Known, false, None);
            assert!(sql.contains("LIMIT $8"));
            assert!(sql.contains("f.organization_id=$1 AND f.person_id=$2"));
            assert!(!sql.contains("body"));
        }
        assert!(candidate_sql(KINDS[5], Dated::Known, false, None).contains(
            "CASE WHEN f.corrects_id IS NOT NULL THEN f.recorded_at ELSE f.occurred_at END"
        ));
        assert!(!INQUIRIES_SQL.contains("message"));
    }
}

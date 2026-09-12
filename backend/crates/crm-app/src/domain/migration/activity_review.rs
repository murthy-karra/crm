//! D-068 bounded native activity reads. Every page holds current membership and
//! Organization guards through serialization; activity workers share that lock
//! order, so the revision, rows and counts describe one coherent page series.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::crypto;
use crate::{
    auth::{workspace, AuthContext},
    config::RawPayloadKey,
    domain::{admin::Role, custom_field, inquiry, person, tag, task::TaskKind},
    ids::{OrganizationId, PersonId, UserId},
};

pub const PAGE_BYTES: usize = 512 * 1024;
const PAGE_OVERHEAD: usize = 4096;
const CURSOR_MAX: usize = 4096;
const CURSOR_PURPOSE: &str = "activity-native-review-cursor-v1";

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
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
impl PageQuery {
    fn limit(&self) -> Result<usize, ReviewError> {
        let n = self.limit.unwrap_or(25);
        if !(1..=50).contains(&n) || self.cursor.as_ref().is_some_and(|c| c.len() > CURSOR_MAX) {
            return Err(ReviewError::Malformed);
        }
        Ok(n)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Open,
    Completed,
}
impl TaskState {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Open => "tasks_open",
            Self::Completed => "tasks_completed",
        }
    }
}

#[derive(Serialize)]
pub struct NativePage {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub activity_revision: String,
}

struct Scope {
    org: OrganizationId,
    person: PersonId,
    parent: Uuid,
    snapshot: Uuid,
    workspace_revision: i64,
    activity_revision: i64,
}

/// Membership then Organization locks follow the worker's established order.
/// Read guards are held until the bounded payload has been assembled.
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
    workspace::shared(&mut tx, org).await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    let member = sqlx::query("SELECT role,status FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR SHARE")
        .bind(org.0).bind(auth.actor_user_id.0).fetch_optional(&mut *tx).await?;
    if !member.is_some_and(|m| {
        m.get::<String, _>("role") == "admin" && m.get::<String, _>("status") == "active"
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
    let parent = sqlx::query(REVIEW_BINDING_SQL)
        .bind(org.0)
        .bind(person.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReviewError::NotFound)?;
    let activity_revision: i64 = sqlx::query_scalar(REVIEW_REVISION_SQL)
        .bind(org.0)
        .fetch_one(&mut *tx)
        .await?;
    let scope = Scope {
        org,
        person,
        parent: parent.get("import_id"),
        snapshot: parent.get("snapshot_id"),
        workspace_revision: organization.get("workspace_revision"),
        activity_revision,
    };
    Ok((tx, scope))
}

pub const REVIEW_BINDING_SQL: &str = r#"SELECT w.import_id,i.snapshot_id
FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id
JOIN migration_import_identity d ON d.organization_id=i.organization_id AND d.import_id=i.id
 AND d.plan_id=w.plan_id AND d.source_account_id=i.source_account_id AND d.family='people'
JOIN migration_import_result r ON r.organization_id=d.organization_id AND r.import_id=d.import_id
 AND r.plan_id=d.plan_id AND r.manifest_id=d.manifest_id AND r.source_id=d.source_id AND r.person_id=d.target_id
JOIN person p ON p.organization_id=d.organization_id AND p.id=d.target_id
WHERE w.organization_id=$1 AND p.id=$2 AND i.confirmed_plan_id=w.plan_id
 AND r.disposition IN ('imported','already_imported')"#;
pub const REVIEW_REVISION_SQL: &str = "SELECT COALESCE(sum(activity_revision),0)::bigint FROM migration_activity_import WHERE organization_id=$1";
pub const REVIEW_COUNTS_SQL: &str = r#"SELECT
 (SELECT count(*) FROM note WHERE organization_id=$1 AND person_id=$2 AND deleted_at IS NULL) AS notes,
 (SELECT count(*) FROM task WHERE organization_id=$1 AND person_id=$2 AND deleted_at IS NULL AND completed_at IS NULL) AS open_tasks,
 (SELECT count(*) FROM task WHERE organization_id=$1 AND person_id=$2 AND deleted_at IS NULL AND completed_at IS NOT NULL) AS completed_tasks"#;

pub async fn detail(
    pool: &PgPool,
    auth: &AuthContext,
    person_id: PersonId,
) -> Result<Value, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person_id).await?;
    workspace::history_complete_read(&mut tx, scope.org).await?;
    workspace::with_reader(auth, async {
        let p = person::queries::summary_by_id(&mut tx, scope.org, person_id).await?.ok_or(ReviewError::NotFound)?;
        let contacts = person::queries::contact_methods_for_person(&mut tx, scope.org, person_id).await?;
        let inquiries = inquiry::queries::list_for_person(&mut tx, scope.org, person_id).await?;
        let history = person::queries::core_history_for_migration_review(&mut tx, scope.org, person_id).await?;
        let tags = tag::list_for_person(&mut tx, scope.org, person_id).await.map_err(|_| ReviewError::Unavailable)?;
        let fields = custom_field::values_for_person(&mut tx, scope.org, person_id).await.map_err(|_| ReviewError::Unavailable)?;
        let counts = sqlx::query(REVIEW_COUNTS_SQL).bind(scope.org.0).bind(person_id.0).fetch_one(&mut *tx).await?;
        let result = json!({"person":p,"contact_methods":contacts,"inquiries":inquiries,"core_history":history,
            "tags":tags,"custom_fields":fields,"activity":{
                "notes_count":counts.get::<i64,_>("notes").to_string(),
                "open_tasks_count":counts.get::<i64,_>("open_tasks").to_string(),
                "completed_tasks_count":counts.get::<i64,_>("completed_tasks").to_string(),
                "activity_revision":scope.activity_revision.to_string(),
                "notes_url":format!("/api/people/{person_id}/migration-review/notes"),
                "tasks_url":format!("/api/people/{person_id}/migration-review/tasks")}});
        bounded(&result)?;
        Ok(result)
    }).await
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    endpoint: String,
    workspace_revision: i64,
    activity_revision: i64,
    time: Option<DateTime<Utc>>,
    id: Uuid,
}
fn encode(
    key: &RawPayloadKey,
    scope: &Scope,
    endpoint: &str,
    time: Option<DateTime<Utc>>,
    id: Uuid,
) -> Result<String, ReviewError> {
    let cursor = Cursor {
        endpoint: endpoint.into(),
        workspace_revision: scope.workspace_revision,
        activity_revision: scope.activity_revision,
        time,
        id,
    };
    let bytes = serde_json::to_vec(&cursor).map_err(|_| ReviewError::Unavailable)?;
    let sealed = crypto::seal_snapshot(
        key,
        scope.org,
        scope.snapshot,
        scope.person.0,
        &format!("{CURSOR_PURPOSE}:{}", scope.parent),
        &bytes,
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
) -> Result<Option<Cursor>, ReviewError> {
    let Some(raw) = &query.cursor else {
        return Ok(None);
    };
    if raw.len() > CURSOR_MAX {
        return Err(ReviewError::Malformed);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| ReviewError::Malformed)?;
    if bytes.len() <= 24 {
        return Err(ReviewError::Malformed);
    }
    let plaintext = crypto::open_snapshot(
        key,
        scope.org,
        scope.snapshot,
        scope.person.0,
        &format!("{CURSOR_PURPOSE}:{}", scope.parent),
        &bytes[..24],
        &bytes[24..],
    )
    .map_err(|_| ReviewError::Malformed)?;
    let cursor: Cursor = serde_json::from_slice(&plaintext).map_err(|_| ReviewError::Malformed)?;
    if cursor.endpoint != endpoint {
        return Err(ReviewError::Malformed);
    }
    if cursor.activity_revision != scope.activity_revision
        || cursor.workspace_revision != scope.workspace_revision
    {
        return Err(ReviewError::RefreshRequired);
    }
    if endpoint != "tasks_open" && cursor.time.is_none() {
        return Err(ReviewError::Malformed);
    }
    Ok(Some(cursor))
}

pub const NOTES_PAGE_SQL: &str = r#"SELECT n.id,n.created_at,n.updated_at,left(n.body,512) AS excerpt,
 char_length(n.body)>512 AS has_more,n.author_user_id,u.display_name AS author_name,
 i.import_id AS activity_import_id,i.source_account_id,i.source_id,r.id AS result_id
FROM note n LEFT JOIN app_user u ON u.id=n.author_user_id
LEFT JOIN migration_activity_identity i ON i.organization_id=n.organization_id AND i.kind='note' AND i.target_id=n.id
 AND n.source='fub' AND n.source_external_id='v1:'||i.source_account_id::text||':'||i.source_id
LEFT JOIN migration_activity_result r ON r.organization_id=i.organization_id AND r.import_id=i.import_id
 AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=n.person_id AND r.target_id=n.id
WHERE n.organization_id=$1 AND n.person_id=$2 AND n.deleted_at IS NULL
 AND ($3::timestamptz IS NULL OR (n.created_at,n.id)>($3,$4))
ORDER BY n.created_at,n.id LIMIT $5"#;
pub const NOTE_DETAIL_SQL: &str = r#"SELECT n.id,n.created_at,n.updated_at,n.body,n.author_user_id,u.display_name AS author_name,
 i.import_id AS activity_import_id,i.source_account_id,i.source_id,r.id AS result_id
FROM note n LEFT JOIN app_user u ON u.id=n.author_user_id
LEFT JOIN migration_activity_identity i ON i.organization_id=n.organization_id AND i.kind='note' AND i.target_id=n.id
 AND n.source='fub' AND n.source_external_id='v1:'||i.source_account_id::text||':'||i.source_id
LEFT JOIN migration_activity_result r ON r.organization_id=i.organization_id AND r.import_id=i.import_id
 AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=n.person_id AND r.target_id=n.id
WHERE n.organization_id=$1 AND n.person_id=$2 AND n.id=$3 AND n.deleted_at IS NULL"#;

fn actor(row: &PgRow, id_column: &str, name_column: &str) -> Result<Value, ReviewError> {
    let id: Option<Uuid> = row.try_get(id_column)?;
    let name: Option<String> = row.try_get(name_column)?;
    match (id, name) {
        (Some(id), Some(display_name)) => {
            Ok(json!({"id":UserId::new(id),"display_name":display_name}))
        }
        (None, None) => Ok(Value::Null),
        _ => Err(ReviewError::Unavailable),
    }
}
fn provenance(row: &PgRow) -> Result<Value, ReviewError> {
    let child: Option<Uuid> = row.try_get("activity_import_id")?;
    let Some(child) = child else {
        return Ok(Value::Null);
    };
    let result: Uuid = row
        .try_get::<Option<Uuid>, _>("result_id")?
        .ok_or(ReviewError::Unavailable)?;
    Ok(json!({"activity_import_id":child,"result_id":result,
        "source_account_id":row.try_get::<i64,_>("source_account_id")?.to_string(),
        "source_id":row.try_get::<String,_>("source_id")?,
        "source_url":format!("/api/migrations/fub/activity-imports/{child}/results/{result}/fields/all")}))
}
fn note_value(row: &PgRow, full: bool) -> Result<Value, ReviewError> {
    let mut v = json!({"id":row.try_get::<Uuid,_>("id")?,"created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,
        "updated_at":row.try_get::<DateTime<Utc>,_>("updated_at")?,"author":actor(row,"author_user_id","author_name")?,
        "can_manage":false,"provenance":provenance(row)?});
    if full {
        v["body"] = json!(row.try_get::<String, _>("body")?);
    } else {
        v["excerpt"] = json!(row.try_get::<String, _>("excerpt")?);
        v["has_more"] = json!(row.try_get::<bool, _>("has_more")?);
    }
    Ok(v)
}

fn bounded(value: &impl Serialize) -> Result<usize, ReviewError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| ReviewError::Unavailable)?
        .len();
    if bytes > PAGE_BYTES {
        return Err(ReviewError::Unavailable);
    }
    Ok(bytes)
}

pub async fn notes(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    query: &PageQuery,
) -> Result<NativePage, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let limit = query.limit()?;
    let cursor = decode(key, &scope, query, "notes")?;
    let rows = sqlx::query(NOTES_PAGE_SQL)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(cursor.as_ref().and_then(|c| c.time))
        .bind(cursor.as_ref().map(|c| c.id))
        .bind((limit + 1) as i64)
        .fetch_all(&mut *tx)
        .await?;
    page(
        key,
        &scope,
        "notes",
        rows,
        limit,
        |r| note_value(r, false),
        "created_at",
    )
}
pub async fn note(
    pool: &PgPool,
    auth: &AuthContext,
    person: PersonId,
    note: Uuid,
) -> Result<Value, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let row = sqlx::query(NOTE_DETAIL_SQL)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(note)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReviewError::NotFound)?;
    let result = note_value(&row, true)?;
    bounded(&result)?;
    Ok(result)
}

pub const TASKS_OPEN_PAGE_SQL: &str = r#"SELECT t.id,t.title,t.kind,t.due_at,t.completed_at,t.created_at,t.updated_at,
 t.assignee_user_id,a.display_name AS assignee_name,t.created_by_user_id,c.display_name AS creator_name,
 t.completed_by_user_id,b.display_name AS completer_name,
 i.import_id AS activity_import_id,i.source_account_id,i.source_id,r.id AS result_id
FROM task t LEFT JOIN app_user a ON a.id=t.assignee_user_id LEFT JOIN app_user c ON c.id=t.created_by_user_id
 LEFT JOIN app_user b ON b.id=t.completed_by_user_id
LEFT JOIN migration_activity_identity i ON i.organization_id=t.organization_id AND i.kind='task' AND i.target_id=t.id
 AND t.source='fub' AND t.source_external_id='v1:'||i.source_account_id::text||':'||i.source_id
LEFT JOIN migration_activity_result r ON r.organization_id=i.organization_id AND r.import_id=i.import_id
 AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=t.person_id AND r.target_id=t.id
WHERE t.organization_id=$1 AND t.person_id=$2 AND t.deleted_at IS NULL AND t.completed_at IS NULL
 AND ($4::uuid IS NULL OR ($3::timestamptz IS NULL AND t.due_at IS NULL AND t.id>$4)
  OR ($3::timestamptz IS NOT NULL AND (t.due_at IS NULL OR (t.due_at,t.id)>($3,$4))))
ORDER BY t.due_at NULLS LAST,t.id LIMIT $5"#;
pub const TASKS_COMPLETED_PAGE_SQL: &str = r#"SELECT t.id,t.title,t.kind,t.due_at,t.completed_at,t.created_at,t.updated_at,
 t.assignee_user_id,a.display_name AS assignee_name,t.created_by_user_id,c.display_name AS creator_name,
 t.completed_by_user_id,b.display_name AS completer_name,
 i.import_id AS activity_import_id,i.source_account_id,i.source_id,r.id AS result_id
FROM task t LEFT JOIN app_user a ON a.id=t.assignee_user_id LEFT JOIN app_user c ON c.id=t.created_by_user_id
 LEFT JOIN app_user b ON b.id=t.completed_by_user_id
LEFT JOIN migration_activity_identity i ON i.organization_id=t.organization_id AND i.kind='task' AND i.target_id=t.id
 AND t.source='fub' AND t.source_external_id='v1:'||i.source_account_id::text||':'||i.source_id
LEFT JOIN migration_activity_result r ON r.organization_id=i.organization_id AND r.import_id=i.import_id
 AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=t.person_id AND r.target_id=t.id
WHERE t.organization_id=$1 AND t.person_id=$2 AND t.deleted_at IS NULL AND t.completed_at IS NOT NULL
 AND ($3::timestamptz IS NULL OR (t.completed_at,t.id)>($3,$4))
ORDER BY t.completed_at,t.id LIMIT $5"#;

fn task_value(row: &PgRow) -> Result<Value, ReviewError> {
    let kind: String = row.try_get("kind")?;
    let kind = TaskKind::from_db_str(&kind).ok_or(ReviewError::Unavailable)?;
    Ok(
        json!({"id":row.try_get::<Uuid,_>("id")?,"title":row.try_get::<String,_>("title")?,"kind":kind,
        "due_at":row.try_get::<Option<DateTime<Utc>>,_>("due_at")?,
        "completed_at":row.try_get::<Option<DateTime<Utc>>,_>("completed_at")?,
        "created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,"updated_at":row.try_get::<DateTime<Utc>,_>("updated_at")?,
        "assignee":actor(row,"assignee_user_id","assignee_name")?,"created_by":actor(row,"created_by_user_id","creator_name")?,
        "completed_by":actor(row,"completed_by_user_id","completer_name")?,"can_manage":false,"provenance":provenance(row)?}),
    )
}
pub async fn tasks(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    state: TaskState,
    query: &PageQuery,
) -> Result<NativePage, ReviewError> {
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let limit = query.limit()?;
    let cursor = decode(key, &scope, query, state.endpoint())?;
    let sql = match state {
        TaskState::Open => TASKS_OPEN_PAGE_SQL,
        TaskState::Completed => TASKS_COMPLETED_PAGE_SQL,
    };
    let rows = sqlx::query(sql)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(cursor.as_ref().and_then(|c| c.time))
        .bind(cursor.as_ref().map(|c| c.id))
        .bind((limit + 1) as i64)
        .fetch_all(&mut *tx)
        .await?;
    page(
        key,
        &scope,
        state.endpoint(),
        rows,
        limit,
        task_value,
        match state {
            TaskState::Open => "due_at",
            TaskState::Completed => "completed_at",
        },
    )
}

fn page(
    key: &RawPayloadKey,
    scope: &Scope,
    endpoint: &str,
    rows: Vec<PgRow>,
    limit: usize,
    convert: impl Fn(&PgRow) -> Result<Value, ReviewError>,
    time_column: &str,
) -> Result<NativePage, ReviewError> {
    let mut items = Vec::new();
    let mut bytes = PAGE_OVERHEAD;
    let mut last = None;
    let mut more = false;
    for row in rows {
        if items.len() == limit {
            more = true;
            break;
        }
        let item = convert(&row)?;
        let size = bounded(&item)? + 1;
        if bytes + size > PAGE_BYTES {
            if items.is_empty() {
                return Err(ReviewError::Unavailable);
            }
            more = true;
            break;
        }
        bytes += size;
        last = Some((
            row.try_get::<Option<DateTime<Utc>>, _>(time_column)?,
            row.try_get::<Uuid, _>("id")?,
        ));
        items.push(item);
    }
    let next_cursor = if more {
        let (time, id) = last.ok_or(ReviewError::Unavailable)?;
        Some(encode(key, scope, endpoint, time, id)?)
    } else {
        None
    };
    let result = NativePage {
        items,
        next_cursor,
        activity_revision: scope.activity_revision.to_string(),
    };
    bounded(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cursor_binds_tenant_person_parent_endpoint_and_revision() {
        let key = RawPayloadKey::new([6; 32]);
        let mut scope = Scope {
            org: OrganizationId::new(Uuid::new_v4()),
            person: PersonId::new(Uuid::new_v4()),
            parent: Uuid::new_v4(),
            snapshot: Uuid::new_v4(),
            workspace_revision: 2,
            activity_revision: 7,
        };
        let token = encode(&key, &scope, "notes", Some(Utc::now()), Uuid::new_v4()).unwrap();
        let query = PageQuery {
            limit: Some(50),
            cursor: Some(token),
        };
        assert!(decode(&key, &scope, &query, "notes").unwrap().is_some());
        assert!(matches!(
            decode(&key, &scope, &query, "tasks_completed"),
            Err(ReviewError::Malformed)
        ));
        scope.activity_revision += 1;
        assert!(matches!(
            decode(&key, &scope, &query, "notes"),
            Err(ReviewError::RefreshRequired)
        ));
        scope.activity_revision -= 1;
        scope.person = PersonId::new(Uuid::new_v4());
        assert!(matches!(
            decode(&key, &scope, &query, "notes"),
            Err(ReviewError::Malformed)
        ));
        scope.org = OrganizationId::new(Uuid::new_v4());
        assert!(matches!(
            decode(&key, &scope, &query, "notes"),
            Err(ReviewError::Malformed)
        ));
    }
    #[test]
    fn page_bounds_and_undated_cursor_are_explicit() {
        assert!(PageQuery {
            limit: Some(0),
            cursor: None
        }
        .limit()
        .is_err());
        assert!(PageQuery {
            limit: Some(51),
            cursor: None
        }
        .limit()
        .is_err());
        assert_eq!(PageQuery::default().limit().unwrap(), 25);
        assert!(bounded(&"x".repeat(PAGE_BYTES)).is_err());
        let key = RawPayloadKey::new([7; 32]);
        let scope = Scope {
            org: OrganizationId::new(Uuid::new_v4()),
            person: PersonId::new(Uuid::new_v4()),
            parent: Uuid::new_v4(),
            snapshot: Uuid::new_v4(),
            workspace_revision: 2,
            activity_revision: 0,
        };
        let query = PageQuery {
            limit: None,
            cursor: Some(encode(&key, &scope, "tasks_open", None, Uuid::new_v4()).unwrap()),
        };
        assert!(decode(&key, &scope, &query, "tasks_open")
            .unwrap()
            .unwrap()
            .time
            .is_none());
    }
}

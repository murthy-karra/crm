//! Approved Mobile 001 adapter: durable receipts, bounded projections, no
//! independent business mutation path. Request/content types deliberately do
//! not implement Debug, so logs cannot accidentally format customer input.
mod generations;
mod operations;
pub use generations::{
    cleanup_once, component, create_generation, manifest, seal, stages, ReconciliationRequest,
};
pub use operations::{execute, lookup_receipt, Operation, Receipt};

use crate::{auth::AuthContext, domain::admin::Role};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub const PROTOCOL: &str = "mobile-v1";
pub const MAX_PEOPLE: usize = 25_000;
pub const PAGE_BYTES: usize = 512 * 1024;
const CURRENT_RECORD_BYTES: usize = 128 * 1024;

#[derive(Debug)]
pub enum MobileError {
    Code(u16, &'static str),
    Database(sqlx::Error),
    ProjectionChanged {
        changed: usize,
        added: usize,
        removed: usize,
        today_changed: bool,
    },
}
impl MobileError {
    pub fn changes(&self) -> Option<Value> {
        match self {
            Self::ProjectionChanged {
                changed,
                added,
                removed,
                today_changed,
            } => Some(
                json!({"changed":changed,"added":added,"removed":removed,"today_changed":today_changed}),
            ),
            _ => None,
        }
    }
    pub fn code(&self) -> (u16, &'static str) {
        match self {
            Self::Code(status, code) => (*status, code),
            Self::ProjectionChanged { .. } => (409, "generation_changed"),
            Self::Database(e) if crate::auth::workspace::is_review_error(e) => {
                (403, "workspace_in_migration_review")
            }
            Self::Database(e) if crate::auth::workspace::is_forbidden_error(e) => {
                (403, "forbidden")
            }
            Self::Database(_) => (503, "unavailable"),
        }
    }
}
impl From<sqlx::Error> for MobileError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
impl From<crate::domain::note::NoteError> for MobileError {
    fn from(e: crate::domain::note::NoteError) -> Self {
        use crate::domain::note::NoteError as E;
        match e {
            E::Database(e) => e.into(),
            E::NotFound => missing(),
            E::Forbidden => code(403, "forbidden"),
            E::MalformedRequest => code(422, "invalid_input"),
            E::Corrupt => code(503, "unavailable"),
        }
    }
}
impl From<crate::domain::task::TaskError> for MobileError {
    fn from(e: crate::domain::task::TaskError) -> Self {
        use crate::domain::task::TaskError as E;
        match e {
            E::Database(e) => e.into(),
            E::NotFound => missing(),
            E::Forbidden => code(403, "forbidden"),
            E::MalformedRequest => code(422, "invalid_input"),
            E::InvalidAssignee => code(422, "invalid_assignee"),
            E::Corrupt => code(503, "unavailable"),
        }
    }
}
fn code(status: u16, message: &'static str) -> MobileError {
    MobileError::Code(status, message)
}
fn missing() -> MobileError {
    code(404, "not_found")
}
fn invalid() -> MobileError {
    code(400, "malformed_request")
}
fn serialize<T: Serialize>(value: &T) -> Result<Value, MobileError> {
    serde_json::to_value(value).map_err(|_| code(503, "unavailable"))
}
fn bounded_current(value: Value) -> Result<Value, MobileError> {
    if serde_json::to_vec(&value)
        .map_err(|_| code(503, "unavailable"))?
        .len()
        > CURRENT_RECORD_BYTES
    {
        return Err(code(503, "unavailable"));
    }
    Ok(value)
}

#[derive(Clone)]
pub struct ReceiptKeys(Vec<(String, [u8; 32])>);
impl std::fmt::Debug for ReceiptKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReceiptKeys([REDACTED])")
    }
}
impl ReceiptKeys {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        let mut keys = Vec::new();
        for part in raw.split(',') {
            let (id, value) = part.split_once(':').ok_or("invalid mobile key ring")?;
            if id.is_empty()
                || id.len() > 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || keys.iter().any(|(k, _)| k == id)
            {
                return Err("invalid mobile key ring");
            }
            let bytes = STANDARD
                .decode(value)
                .map_err(|_| "invalid mobile key ring")?;
            let key =
                <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "invalid mobile key ring")?;
            keys.push((id.to_owned(), key));
        }
        if keys.is_empty() || keys.len() > 8 {
            return Err("invalid mobile key ring");
        }
        Ok(Self(keys))
    }
    /// Synthetic fixtures only. Production configuration never calls this.
    pub fn for_tests() -> Self {
        Self(vec![("synthetic-v1".into(), [71; 32])])
    }
    fn active(&self) -> &str {
        &self.0[0].0
    }
    fn digest(&self, key_id: &str, domain: &[u8], bytes: &[u8]) -> Result<Vec<u8>, MobileError> {
        let key = &self
            .0
            .iter()
            .find(|(id, _)| id == key_id)
            .ok_or(code(503, "mobile_unavailable"))?
            .1;
        let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| code(503, "unavailable"))?;
        mac.update(domain);
        mac.update(bytes);
        Ok(mac.finalize().into_bytes().to_vec())
    }
    fn matches(
        &self,
        key_id: &str,
        domain: &[u8],
        bytes: &[u8],
        expected: &[u8],
    ) -> Result<bool, MobileError> {
        let key = &self
            .0
            .iter()
            .find(|(id, _)| id == key_id)
            .ok_or(code(503, "mobile_unavailable"))?
            .1;
        let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| code(503, "unavailable"))?;
        mac.update(domain);
        mac.update(bytes);
        Ok(mac.verify_slice(expected).is_ok())
    }
    fn cursor(&self, value: &Cursor) -> Result<String, MobileError> {
        let raw = serde_json::to_vec(value).map_err(|_| invalid())?;
        let signature = self.digest(self.active(), b"crm-mobile-cursor-v1\0", &raw)?;
        Ok(format!(
            "{}.{}.{}",
            self.active(),
            URL_SAFE_NO_PAD.encode(raw),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }
    fn decode_cursor(&self, raw: &str) -> Result<Cursor, MobileError> {
        if raw.len() > 2048 {
            return Err(invalid());
        }
        let parts: Vec<_> = raw.split('.').collect();
        if parts.len() != 3 {
            return Err(invalid());
        }
        let body = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| invalid())?;
        let signature = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|_| invalid())?;
        if !self.matches(parts[0], b"crm-mobile-cursor-v1\0", &body, &signature)? {
            return Err(invalid());
        }
        serde_json::from_slice(&body).map_err(|_| invalid())
    }
}
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Cursor {
    context: Uuid,
    generation: Uuid,
    person: Option<Uuid>,
    section: String,
    revision: Option<i64>,
    #[serde(default)]
    after_position: Option<i16>,
    after: Uuid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapRequest {
    pub protocol: String,
    pub installation_id: Uuid,
}

async fn begin<'a>(
    pool: &'a PgPool,
    auth: &AuthContext,
    repeatable: bool,
) -> Result<Transaction<'a, Postgres>, MobileError> {
    let mut tx = pool.begin().await?;
    if repeatable {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("SET LOCAL lock_timeout='2000ms'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='5000ms'")
        .execute(&mut *tx)
        .await?;
    crate::auth::workspace::ordinary(&mut tx, auth.active_organization_id).await?;
    Ok(tx)
}
async fn authority(
    conn: &mut PgConnection,
    auth: &AuthContext,
    lock: bool,
) -> Result<(String, i64), MobileError> {
    let sql = if lock {
        "SELECT role FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' FOR SHARE"
    } else {
        "SELECT role FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active'"
    };
    let role: Option<String> = sqlx::query_scalar(sql)
        .bind(auth.active_organization_id.0)
        .bind(auth.actor_user_id.0)
        .fetch_optional(&mut *conn)
        .await?;
    let role = role.ok_or(code(403, "forbidden"))?;
    if Role::from_db_str(&role).is_none() {
        return Err(code(503, "unavailable"));
    }
    let (_, revision) = crate::auth::workspace::mode(conn, auth.active_organization_id).await?;
    Ok((role, revision))
}
async fn context(
    conn: &mut PgConnection,
    auth: &AuthContext,
    id: Uuid,
    exclusive: bool,
) -> Result<Uuid, MobileError> {
    let sql = if exclusive {
        "SELECT installation_id FROM mobile_context WHERE id=$1 AND organization_id=$2 AND actor_user_id=$3 AND protocol='mobile-v1' AND offline_access_expires_at>statement_timestamp() FOR UPDATE"
    } else {
        "SELECT installation_id FROM mobile_context WHERE id=$1 AND organization_id=$2 AND actor_user_id=$3 AND protocol='mobile-v1' AND offline_access_expires_at>statement_timestamp()"
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .bind(auth.active_organization_id.0)
        .bind(auth.actor_user_id.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(code(401, "unauthenticated"))
}
async fn admission(conn: &mut PgConnection, auth: &AuthContext) -> Result<(), MobileError> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-admission:'||$1::text,0))",
    )
    .bind(auth.active_organization_id.0)
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO mobile_admission(organization_id) VALUES($1) ON CONFLICT(organization_id) DO UPDATE SET revision=mobile_admission.revision+1").bind(auth.active_organization_id.0).execute(conn).await?;
    Ok(())
}
fn protocol(value: &str) -> Result<(), MobileError> {
    if value == PROTOCOL {
        Ok(())
    } else {
        Err(code(409, "protocol_unsupported"))
    }
}

#[tracing::instrument(name="mobile.bootstrap",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id))]
pub async fn bootstrap(
    pool: &PgPool,
    auth: &AuthContext,
    request: BootstrapRequest,
) -> Result<Value, MobileError> {
    protocol(&request.protocol)?;
    let mut tx = begin(pool, auth, false).await?;
    admission(&mut tx, auth).await?;
    let (_, revision) = authority(&mut tx, auth, true).await?;
    let existing:Option<Uuid>=sqlx::query_scalar("SELECT id FROM mobile_context WHERE organization_id=$1 AND actor_user_id=$2 AND installation_id=$3").bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(request.installation_id).fetch_optional(&mut *tx).await?;
    if existing.is_none() {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM mobile_context WHERE organization_id=$1 AND actor_user_id=$2",
        )
        .bind(auth.active_organization_id.0)
        .bind(auth.actor_user_id.0)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 10 {
            return Err(code(429, "mobile_capacity"));
        }
    }
    let id = existing.unwrap_or_else(Uuid::new_v4);
    let row=sqlx::query("INSERT INTO mobile_context(id,organization_id,actor_user_id,installation_id,protocol,authorized_at,offline_access_expires_at) VALUES($1,$2,$3,$4,'mobile-v1',statement_timestamp(),statement_timestamp()+interval '7 days') ON CONFLICT(id) DO UPDATE SET authorized_at=statement_timestamp(),offline_access_expires_at=statement_timestamp()+interval '7 days' RETURNING authorized_at,offline_access_expires_at")
        .bind(id).bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(request.installation_id).fetch_one(&mut *tx).await?;
    let now: DateTime<Utc> = row.get("authorized_at");
    let expiry: DateTime<Utc> = row.get("offline_access_expires_at");
    tx.commit().await?;
    Ok(
        json!({"protocol":PROTOCOL,"context_id":id,"installation_id":request.installation_id,"actor_user_id":auth.actor_user_id,"organization_id":auth.active_organization_id,"workspace_revision":revision.to_string(),"authorized_at":now,"offline_access_expires_at":expiry,"server_time":now,"capabilities":["add_note","create_task","complete_task","reconciliation","edit_note","update_task","note_revisions","log_contact_attempt","change_person_stage","stage_revisions","stage_catalog","update_person_details","details_revisions"],"bounds":{"selected_people":MAX_PEOPLE,"manifest_page":250,"component_rows":100,"component_bytes":PAGE_BYTES,"operation_bytes":131072,"concurrent_uploads":1,"concurrent_downloads":2,"generation_seconds":1800,"stage_catalog_page":100}}),
    )
}

/// Bounded live edit baseline. This is intentionally separate from a sealed
/// reconciliation component: it cannot advance any cursor or qualify an
/// offline bundle, but it rechecks the current context, workspace, membership
/// and organization-scoped Person visibility before returning content.
pub async fn current_note(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    person_id: Uuid,
    note_id: Uuid,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    context(&mut tx, auth, context_id, false).await?;
    let (role, _) = authority(&mut tx, auth, false).await?;
    let row = sqlx::query(
        "SELECT p.mobile_revision, jsonb_build_object(\
          'id',n.id,'person_id',n.person_id,'body',n.body,\
          'author',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('id',u.id,'display_name',u.display_name) END,\
          'created_at',n.created_at,'updated_at',n.updated_at,'edited',n.updated_at>n.created_at,\
          'can_manage',COALESCE(($4='admin' OR n.author_user_id=$5),false),'revision',n.revision::text) AS note \
         FROM note n JOIN person p ON p.id=n.person_id AND p.organization_id=n.organization_id \
         LEFT JOIN app_user u ON u.id=n.author_user_id \
         WHERE n.organization_id=$1 AND n.person_id=$2 AND n.id=$3 AND n.deleted_at IS NULL",
    )
    .bind(auth.active_organization_id.0)
    .bind(person_id)
    .bind(note_id)
    .bind(&role)
    .bind(auth.actor_user_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(missing)?;
    let person_revision: i64 = row.get("mobile_revision");
    let note: Value = row.get("note");
    tx.commit().await?;
    bounded_current(
        json!({"context_id":context_id,"person_id":person_id,"person_revision":person_revision.to_string(),"note":note}),
    )
}

/// See [`current_note`]. Task rows already have their independent revision;
/// returning this single record never promotes or seals a Person component.
pub async fn current_task(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    person_id: Uuid,
    task_id: Uuid,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    context(&mut tx, auth, context_id, false).await?;
    let (role, _) = authority(&mut tx, auth, false).await?;
    let row = sqlx::query(
        "SELECT p.mobile_revision, jsonb_build_object(\
          'id',t.id,'person_id',t.person_id,'title',t.title,'kind',t.kind,'due_at',t.due_at,\
          'assignee',CASE WHEN au.id IS NULL THEN NULL ELSE jsonb_build_object('id',au.id,'display_name',au.display_name) END,\
          'created_by',CASE WHEN cu.id IS NULL THEN NULL ELSE jsonb_build_object('id',cu.id,'display_name',cu.display_name) END,\
          'completed_at',t.completed_at,\
          'completed_by',CASE WHEN ku.id IS NULL THEN NULL ELSE jsonb_build_object('id',ku.id,'display_name',ku.display_name) END,\
          'created_at',t.created_at,'updated_at',t.updated_at,\
          'can_manage',COALESCE(($4='admin' OR t.assignee_user_id=$5 OR t.created_by_user_id=$5),false),\
          'revision',t.revision::text) AS task \
         FROM task t JOIN person p ON p.id=t.person_id AND p.organization_id=t.organization_id \
         LEFT JOIN app_user au ON au.id=t.assignee_user_id \
         LEFT JOIN app_user cu ON cu.id=t.created_by_user_id \
         LEFT JOIN app_user ku ON ku.id=t.completed_by_user_id \
         WHERE t.organization_id=$1 AND t.person_id=$2 AND t.id=$3 AND t.deleted_at IS NULL",
    )
    .bind(auth.active_organization_id.0)
    .bind(person_id)
    .bind(task_id)
    .bind(&role)
    .bind(auth.actor_user_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(missing)?;
    let person_revision: i64 = row.get("mobile_revision");
    let task: Value = row.get("task");
    tx.commit().await?;
    bounded_current(
        json!({"context_id":context_id,"person_id":person_id,"person_revision":person_revision.to_string(),"task":task}),
    )
}

/// Bounded live stage baseline for Mobile004 conflict review.  It is never a
/// reconciliation component and cannot qualify an incomplete downloaded bundle.
pub async fn current_stage(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    person_id: Uuid,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    context(&mut tx, auth, context_id, false).await?;
    authority(&mut tx, auth, false).await?;
    let row = sqlx::query(
        "SELECT p.mobile_revision,p.stage_revision,s.id AS stage_id,s.name AS stage_name \
         FROM person p JOIN stage s ON s.id=p.stage_id AND s.organization_id=p.organization_id \
         WHERE p.organization_id=$1 AND p.id=$2",
    )
    .bind(auth.active_organization_id.0)
    .bind(person_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(missing)?;
    let person_revision: i64 = row.get("mobile_revision");
    let stage_revision: i64 = row.get("stage_revision");
    let stage_id: Uuid = row.get("stage_id");
    let stage_name: String = row.get("stage_name");
    tx.commit().await?;
    bounded_current(json!({
        "context_id":context_id,
        "person_id":person_id,
        "person_revision":person_revision.to_string(),
        "stage_revision":stage_revision.to_string(),
        "stage":{"id":stage_id,"name":stage_name},
    }))
}

/// Bounded current profile traversal for conflict review. This is deliberately
/// separate from sealed reconciliation data and verifies the pinned details
/// revision on every page.
pub async fn current_details(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    person_id: Uuid,
    cursor: Option<&str>,
    keys: &ReceiptKeys,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    context(&mut tx, auth, context_id, false).await?;
    authority(&mut tx, auth, false).await?;
    let row = sqlx::query("SELECT mobile_revision,details_revision,first_name,last_name FROM person WHERE organization_id=$1 AND id=$2")
        .bind(auth.active_organization_id.0).bind(person_id).fetch_optional(&mut *tx).await?.ok_or_else(missing)?;
    let current: i64 = row.get("details_revision");
    let after_id = if let Some(cursor) = cursor {
        let cursor = keys.decode_cursor(cursor)?;
        if cursor.context != context_id
            || cursor.person != Some(person_id)
            || cursor.section != "details"
            || cursor.revision != Some(current)
        {
            return Err(code(409, "revision_conflict"));
        }
        cursor.after
    } else {
        Uuid::nil()
    };
    let previous = if after_id == Uuid::nil() {
        None
    } else {
        Some(sqlx::query("SELECT import_order,created_at,id FROM contact_method WHERE organization_id=$1 AND person_id=$2 AND id=$3")
            .bind(auth.active_organization_id.0).bind(person_id).bind(after_id).fetch_optional(&mut *tx).await?
            .map(|row| (row.get::<Option<i32>,_>("import_order"), row.get::<DateTime<Utc>,_>("created_at"), row.get::<Uuid,_>("id")))
            .ok_or_else(|| code(409, "revision_conflict"))?)
    };
    let rows = match previous {
        None => sqlx::query("SELECT id,kind,value,import_order,created_at FROM contact_method WHERE organization_id=$1 AND person_id=$2 ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 101").bind(auth.active_organization_id.0).bind(person_id).fetch_all(&mut *tx).await?,
        Some((Some(order), created_at, id)) => sqlx::query("SELECT id,kind,value,import_order,created_at FROM contact_method WHERE organization_id=$1 AND person_id=$2 AND (import_order IS NULL OR (import_order IS NOT NULL AND (import_order,created_at,id)>($3,$4,$5))) ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 101").bind(auth.active_organization_id.0).bind(person_id).bind(order).bind(created_at).bind(id).fetch_all(&mut *tx).await?,
        Some((None, created_at, id)) => sqlx::query("SELECT id,kind,value,import_order,created_at FROM contact_method WHERE organization_id=$1 AND person_id=$2 AND import_order IS NULL AND (created_at,id)>($3,$4) ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 101").bind(auth.active_organization_id.0).bind(person_id).bind(created_at).bind(id).fetch_all(&mut *tx).await?,
    };
    let mut items = Vec::new();
    let mut last = Uuid::nil();
    // Reserve framing/cursor space before accumulating exact row bytes. This
    // keeps a collection with many individually valid imported contacts
    // pageable instead of rejecting the entire current-profile traversal.
    let mut bytes = serde_json::to_vec(&json!({
        "context_id":context_id,"person_id":person_id,
        "person_revision":row.get::<i64,_>("mobile_revision").to_string(),
        "details_revision":current.to_string(),
        "first_name":row.get::<Option<String>,_>("first_name"),
        "last_name":row.get::<Option<String>,_>("last_name"),"items":[]
    }))
    .map_err(|_| invalid())?
    .len()
        + 4096;
    for row in rows.iter() {
        let value = json!({"id":row.get::<Uuid,_>("id"),"kind":row.get::<String,_>("kind"),"value":row.get::<String,_>("value"),"import_order":row.get::<Option<i32>,_>("import_order"),"created_at":row.get::<DateTime<Utc>,_>("created_at")});
        let size = serde_json::to_vec(&value).map_err(|_| invalid())?.len() + 1;
        if items.len() == 100 || bytes + size > PAGE_BYTES {
            break;
        }
        if size > PAGE_BYTES {
            return Err(code(422, "over_limit"));
        }
        bytes += size;
        last = row.get("id");
        items.push(value);
    }
    if !rows.is_empty() && items.is_empty() {
        return Err(code(422, "over_limit"));
    }
    let complete = items.len() == rows.len();
    let next_cursor = if complete {
        None
    } else {
        Some(keys.cursor(&Cursor {
            context: context_id,
            generation: Uuid::nil(),
            person: Some(person_id),
            section: "details".into(),
            revision: Some(current),
            after_position: None,
            after: last,
        })?)
    };
    let fresh: i64 = sqlx::query_scalar(
        "SELECT details_revision FROM person WHERE organization_id=$1 AND id=$2",
    )
    .bind(auth.active_organization_id.0)
    .bind(person_id)
    .fetch_one(&mut *tx)
    .await?;
    if fresh != current {
        return Err(code(409, "revision_conflict"));
    }
    let output = json!({"context_id":context_id,"person_id":person_id,"person_revision":row.get::<i64,_>("mobile_revision").to_string(),"details_revision":current.to_string(),"first_name":row.get::<Option<String>,_>("first_name"),"last_name":row.get::<Option<String>,_>("last_name"),"items":items,"next_cursor":next_cursor,"complete":complete});
    if serde_json::to_vec(&output).map_err(|_| invalid())?.len() > PAGE_BYTES {
        return Err(code(422, "over_limit"));
    }
    tx.commit().await?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_rotation_verifies_old_digests_and_cursors_without_rebinding() {
        let original = ReceiptKeys::for_tests();
        let digest = original
            .digest(
                "synthetic-v1",
                b"crm-mobile-operation-v1\0",
                b"normalized synthetic request",
            )
            .unwrap();
        let cursor = original
            .cursor(&Cursor {
                context: Uuid::new_v4(),
                generation: Uuid::new_v4(),
                person: None,
                section: "manifest".into(),
                revision: None,
                after_position: None,
                after: Uuid::new_v4(),
            })
            .unwrap();
        let rotated = ReceiptKeys::parse(&format!(
            "new:{},synthetic-v1:{}",
            STANDARD.encode([19; 32]),
            STANDARD.encode([71; 32])
        ))
        .unwrap();
        assert_eq!(rotated.active(), "new");
        assert!(rotated
            .matches(
                "synthetic-v1",
                b"crm-mobile-operation-v1\0",
                b"normalized synthetic request",
                &digest
            )
            .unwrap());
        assert!(!rotated
            .matches(
                "synthetic-v1",
                b"crm-mobile-operation-v1\0",
                b"changed request",
                &digest
            )
            .unwrap());
        assert!(!rotated
            .matches(
                "synthetic-v1",
                b"different-domain",
                b"normalized synthetic request",
                &digest
            )
            .unwrap());
        assert!(rotated.decode_cursor(&cursor).is_ok());
        let missing = ReceiptKeys::parse(&format!("new:{}", STANDARD.encode([19; 32]))).unwrap();
        assert_eq!(
            missing
                .matches(
                    "synthetic-v1",
                    b"crm-mobile-operation-v1\0",
                    b"normalized synthetic request",
                    &digest
                )
                .unwrap_err()
                .code(),
            (503, "mobile_unavailable")
        );
        assert!(ReceiptKeys::parse("").is_err());
        assert!(ReceiptKeys::parse(&format!(
            "same:{},same:{}",
            STANDARD.encode([1; 32]),
            STANDARD.encode([2; 32])
        ))
        .is_err());
        assert_eq!(format!("{rotated:?}"), "ReceiptKeys([REDACTED])");
    }
}

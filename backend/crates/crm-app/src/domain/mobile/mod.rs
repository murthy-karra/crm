//! Approved Mobile 001 adapter: durable receipts, bounded projections, no
//! independent business mutation path. Request/content types deliberately do
//! not implement Debug, so logs cannot accidentally format customer input.
mod generations;
mod operations;
pub use generations::{
    cleanup_once, component, create_generation, manifest, seal, ReconciliationRequest,
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
        json!({"protocol":PROTOCOL,"context_id":id,"installation_id":request.installation_id,"actor_user_id":auth.actor_user_id,"organization_id":auth.active_organization_id,"workspace_revision":revision.to_string(),"authorized_at":now,"offline_access_expires_at":expiry,"server_time":now,"capabilities":["add_note","create_task","complete_task","reconciliation"],"bounds":{"selected_people":MAX_PEOPLE,"manifest_page":250,"component_rows":100,"component_bytes":PAGE_BYTES,"operation_bytes":131072,"concurrent_uploads":1,"concurrent_downloads":2,"generation_seconds":1800}}),
    )
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

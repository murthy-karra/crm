//! D-065 workspace readiness. Guards are acquired before domain locks, and
//! held only for database work. No authority is inferred from Origin or role.
use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{Connection, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::ids::{OrganizationId, UserId};

pub const GATE_VERSION: &str = "crm-workspace-v1";
pub const WAIT: Duration = Duration::from_secs(2);

tokio::task_local! {
    // Set only by a server-owned AuthContext. SQL revalidates active membership.
    static READER: (OrganizationId, UserId);
}

pub async fn with_reader<F: Future>(auth: &AuthContext, future: F) -> F::Output {
    READER
        .scope((auth.active_organization_id, auth.actor_user_id), future)
        .await
}

pub fn is_review_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010C")
}
pub fn is_forbidden_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010A")
}

pub fn is_activity_review_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010F")
}

/// Call inside the existing shared workspace transaction, before any complete
/// activity fetch. First activity confirmation takes the exclusive barrier.
pub async fn activity_complete_read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_activity_complete_read($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn shared(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_workspace_shared($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn ordinary(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_workspace_operational($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn begin(
    pool: &PgPool,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = tokio::time::timeout(WAIT, pool.begin())
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)??;
    ordinary(&mut tx, org).await?;
    Ok(tx)
}

/// Protects direct-domain queries too. Without explicit server read authority,
/// only operational workspaces are readable. Nested queries use SQLx savepoints.
pub async fn read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    match READER.try_with(|a| *a).ok().filter(|a| a.0 == org) {
        Some((_, actor)) => read_check(&mut tx, org, actor, false).await?,
        None => ordinary(&mut tx, org).await?,
    }
    Ok(tx)
}
pub async fn read_check(
    conn: &mut PgConnection,
    org: OrganizationId,
    actor: UserId,
    operational_only: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_workspace_read($1,$2,$3)")
        .bind(org.0)
        .bind(actor.0)
        .bind(operational_only)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn operational_read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    ordinary(&mut tx, org).await?;
    Ok(tx)
}
pub async fn exclusive(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    bounded_lock_wait(conn).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||$1::text,0))")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
/// Transaction-local bound for subsequent membership, Organization and domain
/// row locks. Shared advisory guards alone do not bound those row waits.
pub(crate) async fn bounded_lock_wait(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('lock_timeout','2000ms',true)")
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn admit_operator(
    pool: &PgPool,
    auth: &AuthContext,
    id: Uuid,
    deadline: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let mut tx = begin(pool, auth.active_organization_id).await?;
    read_check(
        &mut tx,
        auth.active_organization_id,
        auth.actor_user_id,
        true,
    )
    .await?;
    sqlx::query("INSERT INTO workspace_operation_admission(id,organization_id,actor_user_id,kind,deadline) VALUES($1,$2,$3,'operator',$4)")
        .bind(id).bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(deadline).execute(&mut *tx).await?;
    tx.commit().await
}
pub async fn release_operator(
    pool: &PgPool,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM workspace_operation_admission WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .execute(pool)
        .await?;
    Ok(())
}

/// Only terminal settlement calls this after applying the closed transition
/// function. Database triggers independently verify call/person/terminal shape.
pub(crate) async fn terminal_call(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    shared(conn, org).await?;
    sqlx::query("SELECT set_config('crm.terminal_call',$1,true)")
        .bind(id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn mode(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<(String, i64), sqlx::Error> {
    let r = sqlx::query("SELECT workspace_mode,workspace_revision FROM organization WHERE id=$1")
        .bind(org.0)
        .fetch_one(conn)
        .await?;
    Ok((r.get("workspace_mode"), r.get("workspace_revision")))
}

/// Server/operator-owned readiness, never deserialized from a tenant request.
#[derive(Clone)]
pub struct ReleaseReadiness {
    database: String,
    checked_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    synthetic: bool,
    metadata: bool,
    activity: bool,
}
impl ReleaseReadiness {
    pub async fn load_report(pool: &PgPool, path: &std::path::Path) -> Result<Self, sqlx::Error> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|_| sqlx::Error::Protocol("release evidence unavailable".into()))?;
        if bytes.len() > 1024 * 1024 {
            return Err(sqlx::Error::Protocol("release evidence invalid".into()));
        }
        let report: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| sqlx::Error::Protocol("release evidence invalid".into()))?;
        let hash = artifact_fingerprint().await?;
        let matching = report["candidates"].as_array().is_some_and(|a| {
            a.iter()
                .any(|v| v["sha256"] == hash && v["gate_version"] == GATE_VERSION)
        });
        if report["confirmation_ready"] != true || !matching {
            return Err(sqlx::Error::Protocol("release not ready".into()));
        }
        let database = report["database_name"]
            .as_str()
            .ok_or_else(|| sqlx::Error::Protocol("release database unavailable".into()))?
            .to_owned();
        let checked_at = serde_json::from_value(report["checked_at"].clone())
            .map_err(|_| sqlx::Error::Protocol("release timestamp invalid".into()))?;
        let expires_at = serde_json::from_value(report["evidence_expires_at"].clone())
            .map_err(|_| sqlx::Error::Protocol("release deadline invalid".into()))?;
        let ready = Self {
            database,
            checked_at,
            expires_at,
            synthetic: false,
            activity: report["activity_confirmation_ready"] == true
                && report["candidates"].as_array().is_some_and(|items| {
                    items.iter().any(|v| {
                        v["sha256"] == hash
                            && v["gate_version"] == GATE_VERSION
                            && v["capabilities"]
                                .as_array()
                                .is_some_and(|c| c.iter().any(|v| v == "fub-activity-import-v1"))
                    })
                }),
            metadata: report["metadata_confirmation_ready"] == true
                && report["candidates"].as_array().is_some_and(|items| {
                    items.iter().any(|v| {
                        v["sha256"] == hash
                            && v["gate_version"] == GATE_VERSION
                            && v["capabilities"]
                                .as_array()
                                .is_some_and(|c| c.iter().any(|v| v == "fub-metadata-import-v1"))
                    })
                }),
        };
        ready.require_current(&mut *pool.acquire().await?).await?;
        Ok(ready)
    }
    pub async fn require_current(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        startup_compatible(conn).await?;
        if self.synthetic {
            return Ok(());
        }
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(conn)
            .await?;
        if database != self.database
            || self.expires_at <= Utc::now()
            || self.checked_at > Utc::now()
            || Utc::now() - self.checked_at > chrono::Duration::minutes(5)
        {
            return Err(sqlx::Error::Protocol("release evidence expired".into()));
        }
        Ok(())
    }
    pub fn metadata_ready(&self) -> bool {
        self.metadata
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_metadata(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.metadata_ready() {
            return Err(sqlx::Error::Protocol("metadata release not ready".into()));
        }
        Ok(())
    }
    #[cfg(feature = "test-support")]
    pub fn for_tests() -> Self {
        Self {
            database: "synthetic-only".into(),
            checked_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            synthetic: true,
            metadata: true,
            activity: true,
        }
    }

    pub fn activity_ready(&self) -> bool {
        self.activity
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_activity(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.activity_ready() {
            return Err(sqlx::Error::Protocol("activity release not ready".into()));
        }
        Ok(())
    }
}
/// Every new API/worker/CLI invokes this before accepting work. Old binaries
/// cannot acquire this capability; deployment additionally retires them using
/// the operator release preflight's complete workload inventory.
pub async fn startup_compatible(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('migration_workspace') IS NOT NULL AND to_regclass('migration_activity_import') IS NOT NULL")
        .fetch_one(&mut *conn)
        .await?;
    if !exists {
        return Err(sqlx::Error::Protocol(
            "workspace schema upgrade required".into(),
        ));
    }
    let unsupported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM migration_workspace WHERE gate_version<>$1)",
    )
    .bind(GATE_VERSION)
    .fetch_one(conn)
    .await?;
    if unsupported {
        return Err(sqlx::Error::Protocol(
            "workspace artifact incompatible".into(),
        ));
    }
    Ok(())
}

/// Called before listening, so a later filesystem replacement cannot relabel
/// the executable already running in this process.
pub async fn artifact_fingerprint() -> Result<String, sqlx::Error> {
    static ARTIFACT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(hash) = ARTIFACT.get() {
        return Ok(hash.clone());
    }
    let hash = tokio::task::spawn_blocking(|| {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let path = std::env::current_exe()
            .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
        let mut file = std::fs::File::open(path)
            .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok::<_, sqlx::Error>(
            hash.finalize()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
        )
    })
    .await
    .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))??;
    let _ = ARTIFACT.set(hash.clone());
    Ok(hash)
}

//! Internal persistence. Public callers use commands with trusted context.
use super::{crypto, reader, AssessmentCheckView, AssessmentView, ConnectionView, MigrationError};
use crate::{
    config::RawPayloadKey,
    domain::envelope::CommandContext,
    ids::{OrganizationId, UserId},
};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

pub async fn require_admin(
    conn: &mut PgConnection,
    ctx: &CommandContext,
) -> Result<(), MigrationError> {
    // Lock authority until transaction commit; a concurrent revocation waits.
    if sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).fetch_optional(conn).await?.is_some(){Ok(())}else{Err(MigrationError::Forbidden)}
}
pub async fn active_admin(
    pool: &PgPool,
    org: OrganizationId,
    user: UserId,
) -> Result<bool, MigrationError> {
    Ok(sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active'").bind(org.0).bind(user.0).fetch_optional(pool).await?.is_some())
}
pub async fn lock_org(conn: &mut PgConnection, org: OrganizationId) -> Result<(), MigrationError> {
    sqlx::query("SELECT id FROM organization WHERE id=$1 FOR UPDATE")
        .bind(org.0)
        .fetch_one(conn)
        .await?;
    Ok(())
}
pub async fn receipt(
    key: &RawPayloadKey,
    conn: &mut PgConnection,
    org: OrganizationId,
    operation: &str,
    request_id: Uuid,
    digest: &[u8],
) -> Result<Option<serde_json::Value>, MigrationError> {
    let row=sqlx::query("SELECT digest,response FROM migration_request_receipt WHERE organization_id=$1 AND operation=$2 AND request_id=$3").bind(org.0).bind(operation).bind(request_id).fetch_optional(conn).await?;
    match row {
        Some(r) => {
            if r.get::<Vec<u8>, _>("digest") != digest {
                return Err(MigrationError::Conflict);
            }
            let envelope: serde_json::Value = r.get("response");
            let nonce: Vec<u8> = serde_json::from_value(envelope["nonce"].clone())
                .map_err(|_| MigrationError::Crypto)?;
            let ciphertext: Vec<u8> = serde_json::from_value(envelope["ciphertext"].clone())
                .map_err(|_| MigrationError::Crypto)?;
            let bytes = crypto::open_receipt(key, org, request_id, operation, &nonce, &ciphertext)
                .map_err(|_| MigrationError::Crypto)?;
            Ok(Some(
                serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)?,
            ))
        }
        None => Ok(None),
    }
}
pub async fn insert_receipt(
    key: &RawPayloadKey,
    conn: &mut PgConnection,
    org: OrganizationId,
    operation: &str,
    request_id: Uuid,
    digest: &[u8],
    response: &serde_json::Value,
) -> Result<(), MigrationError> {
    let sealed = crypto::seal_receipt(
        key,
        org,
        request_id,
        operation,
        &serde_json::to_vec(response).map_err(|_| MigrationError::Crypto)?,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let response =
        serde_json::json!({"nonce":sealed.nonce.to_vec(),"ciphertext":sealed.ciphertext});
    sqlx::query("INSERT INTO migration_request_receipt(organization_id,operation,request_id,digest,response) VALUES($1,$2,$3,$4,$5)").bind(org.0).bind(operation).bind(request_id).bind(digest).bind(response).execute(conn).await?;
    Ok(())
}
pub fn display(
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    revision: i32,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Option<String>, MigrationError> {
    let bytes = crypto::open_identity(key, org, id, revision, nonce, ciphertext)
        .map_err(|_| MigrationError::Crypto)?;
    let identity = reader::parse_identity(&bytes).map_err(|_| MigrationError::Crypto)?;
    Ok(identity.account_domain.or(identity.display_name))
}
pub async fn connection(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
) -> Result<Option<ConnectionView>, MigrationError> {
    let row = sqlx::query("SELECT * FROM migration_connection WHERE organization_id=$1")
        .bind(org.0)
        .fetch_optional(conn)
        .await?;
    row.map(|r| {
        let id = r.get("id");
        Ok(ConnectionView {
            id,
            source_account_id: r.get("source_account_id"),
            source_display_name: display(
                key,
                org,
                id,
                r.get("identity_revision"),
                r.get("identity_nonce"),
                r.get("identity_ciphertext"),
            )?,
            source_access_scope: "unknown".into(),
            status: r.get("status"),
            revision: r.get("revision"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
    })
    .transpose()
}
fn readiness(check: &str) -> (&'static str, &'static str) {
    match check {
        "people_excluding_trash"
        | "people_including_trash"
        | "notes"
        | "tasks"
        | "tags"
        | "inquiries_history"
        | "users"
        | "stages" => (
            "model_available",
            "destination_model_available_mapping_pending",
        ),
        "identity" | "custom_fields" | "calls" | "emails" => {
            ("review_required", "destination_semantics_require_review")
        }
        _ => (
            "destination_missing",
            "destination_capability_not_available",
        ),
    }
}
pub async fn assessment(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
) -> Result<AssessmentView, MigrationError> {
    let r = sqlx::query("SELECT * FROM migration_assessment WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(MigrationError::NotFound)?;
    let rows=sqlx::query("SELECT check_key,state,reported_total::text AS reported_total,retrieved_count,coverage,error_code,attempts,observed_at FROM migration_assessment_check WHERE assessment_id=$1 AND organization_id=$2 ORDER BY CASE check_key WHEN 'identity' THEN 0 ELSE 1 END,check_key").bind(id).bind(org.0).fetch_all(&mut *conn).await?;
    let checks = rows
        .into_iter()
        .map(|c| {
            let check_key: String = c.get("check_key");
            let coverage: String = c.get("coverage");
            let error_code: Option<String> = c.get("error_code");
            let total: Option<String> = c.get("reported_total");
            let (ready, reason) = readiness(&check_key);
            let mut reasons = vec![reason.to_string()];
            if let Some(error) = &error_code {
                reasons.push(error.clone());
            } else if coverage == "not_checked" {
                reasons.push("not_checked_by_profile".into());
            } else if total.is_none() && check_key != "identity" {
                reasons.push("source_total_unknown_or_inconsistent".into());
            } else if coverage == "partial" {
                reasons.push("bounded_first_page_only".into());
            }
            let action = if error_code.is_some() {
                "Review the check failure and retry after resolving access or source availability."
            } else if coverage == "not_checked" {
                "Assess this family in the later inventory slice."
            } else {
                "Review full inventory and mappings in the next migration slice."
            };
            AssessmentCheckView {
                check_key,
                state: c.get("state"),
                reported_total: total,
                retrieved_count: c.get::<i32, _>("retrieved_count").to_string(),
                coverage,
                error_code,
                attempts: c.get("attempts"),
                observed_at: c.get("observed_at"),
                destination_readiness: ready.into(),
                reason_codes: reasons,
                next_action: action.into(),
            }
        })
        .collect();
    Ok(AssessmentView {
        destination_organization_id: org.0,
        source_account_id: r.get("source_account_id"),
        source_display_name: display(
            key,
            org,
            id,
            1,
            r.get("identity_nonce"),
            r.get("identity_ciphertext"),
        )?,
        source_access_scope: "unknown".into(),
        id,
        connection_id: r.get("connection_id"),
        connection_revision: r.get("connection_revision"),
        profile_version: r.get("profile_version"),
        state: r.get("state"),
        pause_reason: r.get("pause_reason"),
        created_at: r.get("created_at"),
        started_at: r.get("started_at"),
        completed_at: r.get("completed_at"),
        checks,
    })
}
pub async fn cancel_connection_jobs(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), MigrationError> {
    sqlx::query("SELECT id FROM migration_assessment WHERE organization_id=$1 AND connection_id=$2 AND state IN ('queued','running','waiting_retry','paused') FOR UPDATE").bind(org.0).bind(id).fetch_all(&mut *conn).await?;
    sqlx::query("UPDATE migration_assessment_check c SET state='cancelled' FROM migration_assessment a WHERE a.id=c.assessment_id AND a.organization_id=c.organization_id AND a.organization_id=$1 AND a.connection_id=$2 AND a.state IN ('queued','running','waiting_retry','paused') AND c.state<>'completed'").bind(org.0).bind(id).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_assessment SET state='cancelled',pause_reason=NULL,completed_at=now(),lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,updated_at=now() WHERE organization_id=$1 AND connection_id=$2 AND state IN ('queued','running','waiting_retry','paused')").bind(org.0).bind(id).execute(conn).await?;
    Ok(())
}

pub struct Claim {
    pub id: Uuid,
    pub org: OrganizationId,
    pub connection_id: Uuid,
    pub revision: i32,
    pub actor: UserId,
    pub token: Uuid,
    pub check: String,
    pub cycle_attempts: i32,
}
pub async fn claim_one(pool: &PgPool) -> Result<Option<Claim>, MigrationError> {
    let mut tx = pool.begin().await?;
    let row=sqlx::query("SELECT a.id,a.organization_id,a.connection_id,a.connection_revision,a.initiated_by_user_id FROM migration_assessment a WHERE ((a.state IN ('queued','waiting_retry') AND (a.next_attempt_at IS NULL OR a.next_attempt_at<=now())) OR (a.state='running' AND a.lease_expires_at<=now())) ORDER BY a.created_at FOR UPDATE SKIP LOCKED LIMIT 1").fetch_optional(&mut *tx).await?;
    let Some(r) = row else { return Ok(None) };
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let c=sqlx::query("SELECT check_key,cycle_attempts FROM migration_assessment_check WHERE assessment_id=$1 AND organization_id=$2 AND state IN ('pending','waiting_retry','running') AND (next_attempt_at IS NULL OR next_attempt_at<=now()) ORDER BY CASE check_key WHEN 'identity' THEN 0 ELSE 1 END,check_key LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *tx).await?;
    let Some(c) = c else { return Ok(None) };
    let cycle: i32 = c.get("cycle_attempts");
    if cycle >= 3 {
        sqlx::query("UPDATE migration_assessment SET state='paused',pause_reason='retry_exhausted',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_assessment_check SET state='paused',error_code='retry_exhausted' WHERE assessment_id=$1 AND organization_id=$2 AND check_key=$3").bind(id).bind(org.0).bind(c.get::<String,_>("check_key")).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = Uuid::new_v4();
    let check: String = c.get("check_key");
    sqlx::query("UPDATE migration_assessment SET state='running',pause_reason=NULL,started_at=COALESCE(started_at,now()),lease_token=$3,lease_expires_at=now()+interval '60 seconds',updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(token).execute(&mut *tx).await?;
    // Count at claim, so crashes and failed commits still consume an attempt.
    sqlx::query("UPDATE migration_assessment_check SET state='running',attempts=attempts+1,cycle_attempts=cycle_attempts+1 WHERE assessment_id=$1 AND organization_id=$2 AND check_key=$3").bind(id).bind(org.0).bind(&check).execute(&mut *tx).await?;
    let claim = Claim {
        id,
        org,
        connection_id: r.get("connection_id"),
        revision: r.get("connection_revision"),
        actor: UserId::new(r.get("initiated_by_user_id")),
        token,
        check,
        cycle_attempts: cycle + 1,
    };
    tx.commit().await?;
    Ok(Some(claim))
}
/// Locks in the same order as credential changes, before evidence insertion.
async fn fence(conn: &mut PgConnection, c: &Claim) -> Result<bool, MigrationError> {
    if sqlx::query("SELECT 1 FROM migration_connection WHERE id=$1 AND organization_id=$2 AND revision=$3 AND status='connected' FOR UPDATE").bind(c.connection_id).bind(c.org.0).bind(c.revision).fetch_optional(&mut *conn).await?.is_none(){return Ok(false)}
    Ok(sqlx::query("SELECT 1 FROM migration_assessment a JOIN migration_assessment_check c ON c.assessment_id=a.id AND c.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$2 AND a.state='running' AND a.lease_token=$3 AND a.lease_expires_at>now() AND a.connection_revision=$4 AND c.check_key=$5 AND c.state='running' FOR UPDATE OF a,c").bind(c.id).bind(c.org.0).bind(c.token).bind(c.revision).bind(&c.check).fetch_optional(conn).await?.is_some())
}
async fn authorized(conn: &mut PgConnection, c: &Claim) -> Result<bool, MigrationError> {
    Ok(sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE").bind(c.org.0).bind(c.actor.0).fetch_optional(conn).await?.is_some())
}
async fn pause_tx(conn: &mut PgConnection, c: &Claim, reason: &str) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_assessment SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(reason).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_assessment_check SET state='paused',error_code=$4 WHERE assessment_id=$1 AND organization_id=$2 AND check_key=$3").bind(c.id).bind(c.org.0).bind(&c.check).bind(reason).execute(conn).await?;
    Ok(())
}
pub async fn preflight(
    pool: &PgPool,
    c: &Claim,
) -> Result<Option<(i64, Vec<u8>, Vec<u8>)>, MigrationError> {
    let mut tx = pool.begin().await?;
    if !fence(&mut tx, c).await? {
        return Ok(None);
    }
    if !authorized(&mut tx, c).await? {
        pause_tx(&mut tx, c, "initiator_not_authorized").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let r=sqlx::query("SELECT source_account_id,credential_nonce,credential_ciphertext FROM migration_connection WHERE id=$1 AND organization_id=$2").bind(c.connection_id).bind(c.org.0).fetch_one(&mut *tx).await?;
    let result = (
        r.get("source_account_id"),
        r.get("credential_nonce"),
        r.get("credential_ciphertext"),
    );
    tx.commit().await?;
    Ok(Some(result))
}
pub struct CheckResult {
    pub state: &'static str,
    pub coverage: &'static str,
    pub total: Option<String>,
    pub retrieved: i32,
    pub reason: Option<&'static str>,
    pub next: Option<chrono::DateTime<chrono::Utc>>,
    pub capture: Option<reader::Capture>,
}
pub async fn settle(
    pool: &PgPool,
    key: &RawPayloadKey,
    c: &Claim,
    result: CheckResult,
) -> Result<bool, MigrationError> {
    let mut tx = pool.begin().await?;
    if !fence(&mut tx, c).await? {
        return Ok(false);
    }
    if !authorized(&mut tx, c).await? {
        pause_tx(&mut tx, c, "initiator_not_authorized").await?;
        tx.commit().await?;
        return Ok(false);
    }
    let evidence = if let Some(capture) = &result.capture {
        let id = Uuid::new_v4();
        let sealed = crypto::seal_evidence(key, c.org, id, &capture.body)
            .map_err(|_| MigrationError::Crypto)?;
        let hmac = crypto::content_hmac(key, &capture.body);
        sqlx::query("INSERT INTO migration_assessment_evidence(id,organization_id,assessment_id,check_key,http_status,byte_len,nonce,ciphertext,content_hmac,source_version,classification,truncated) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(c.org.0).bind(c.id).bind(&c.check).bind(i32::from(capture.status)).bind(capture.body.len() as i32).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(hmac.as_slice()).bind(&capture.source_version).bind(result.reason.unwrap_or("success")).bind(capture.truncated).execute(&mut *tx).await?;
        Some(id)
    } else {
        None
    };
    sqlx::query("UPDATE migration_assessment_check SET state=$4,coverage=$5,reported_total=$6::text::numeric,retrieved_count=$7,error_code=$8,evidence_id=COALESCE($9,evidence_id),observed_at=now(),next_attempt_at=$10 WHERE assessment_id=$1 AND organization_id=$2 AND check_key=$3").bind(c.id).bind(c.org.0).bind(&c.check).bind(result.state).bind(result.coverage).bind(result.total).bind(result.retrieved).bind(result.reason).bind(evidence).bind(result.next).execute(&mut *tx).await?;
    match result.state {
        "paused" => pause_tx(&mut tx, c, result.reason.unwrap_or("source_unavailable")).await?,
        "waiting_retry" => {
            sqlx::query("UPDATE migration_assessment SET state='waiting_retry',next_attempt_at=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(result.next).execute(&mut *tx).await?;
        }
        _ => {
            sqlx::query("UPDATE migration_assessment a SET state=CASE WHEN EXISTS(SELECT 1 FROM migration_assessment_check c WHERE c.assessment_id=a.id AND c.organization_id=a.organization_id AND c.state<>'completed') THEN 'queued' ELSE 'completed' END,completed_at=CASE WHEN NOT EXISTS(SELECT 1 FROM migration_assessment_check c WHERE c.assessment_id=a.id AND c.organization_id=a.organization_id AND c.state<>'completed') THEN now() ELSE NULL END,next_attempt_at=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

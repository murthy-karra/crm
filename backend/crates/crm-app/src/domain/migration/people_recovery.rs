//! D-090 positive-hold recovery. Uses the admission worker and identity namespace.
use super::{
    people_admission as admission, people_admission_store as s, snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
    ids::OrganizationId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

mod discovery;
mod mappings;
pub(crate) use discovery::{catalog, discover};
pub(crate) use mappings::resolve;
pub use mappings::{
    edit_mapping, mapping_field, mapping_page, seal_choices, EditRecoveryMapping,
    RecoveryMappingPage, SealRecoveryChoices,
};

pub const ENGINE: &str = "fub-people-recovery-v1";
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryAnchor {
    Original {
        import_id: Uuid,
        plan_id: Uuid,
    },
    Admission {
        admission_id: Uuid,
        plan_id: Uuid,
        remainder: bool,
    },
}
impl RecoveryAnchor {
    pub(crate) fn original_plan(&self) -> Option<Uuid> {
        match self {
            Self::Original { plan_id, .. } => Some(*plan_id),
            _ => None,
        }
    }
    pub(crate) fn admission(&self) -> Option<Uuid> {
        match self {
            Self::Admission { admission_id, .. } => Some(*admission_id),
            _ => None,
        }
    }
    pub(crate) fn admission_plan(&self) -> Option<Uuid> {
        match self {
            Self::Admission { plan_id, .. } => Some(*plan_id),
            _ => None,
        }
    }
    pub(crate) fn remainder(&self) -> bool {
        matches!(
            self,
            Self::Admission {
                remainder: true,
                ..
            }
        )
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRecovery {
    pub request_id: Uuid,
    pub report_id: Uuid,
    pub anchor: RecoveryAnchor,
    pub expected_anchor_revision: i64,
}
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    cmd: PrepareRecovery,
    release: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    admission::prepare_internal(
        pool,
        key,
        policy,
        ctx,
        admission::PreparePeopleAdmission {
            request_id: cmd.request_id,
            report_id: cmd.report_id,
        },
        Some(cmd),
        release,
    )
    .await
}
pub(crate) fn is_recovery(run: &PgRow) -> bool {
    run.get::<String, _>("mode") == "mapping_recovery"
}

/// Parent is already exclusively locked. Retirement and root creation share one
/// transaction, so an old preview confirmation cannot race a recovery start.
pub(crate) async fn validate_anchor(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    report: &PgRow,
    cmd: &PrepareRecovery,
) -> Result<(), MigrationError> {
    if cmd.expected_anchor_revision < 1 {
        return Err(MigrationError::InvalidInput);
    }
    let org = ctx.organization_id.0;
    match cmd.anchor {
        RecoveryAnchor::Original { import_id, plan_id } => {
            if import_id != report.get::<Uuid, _>("parent_import_id")
                || plan_id != report.get::<Uuid, _>("parent_plan_id")
            {
                return Err(MigrationError::NotFound);
            }
            let revision:Option<i64>=sqlx::query_scalar("SELECT p.revision FROM migration_import_plan p JOIN migration_import i ON i.id=p.import_id AND i.organization_id=p.organization_id WHERE p.id=$1 AND p.import_id=$2 AND p.organization_id=$3 AND i.state='completed' AND i.confirmed_plan_id=p.id AND p.phase='ready'").bind(plan_id).bind(import_id).bind(org).fetch_optional(&mut *conn).await?;
            if revision != Some(cmd.expected_anchor_revision) {
                return Err(MigrationError::Conflict);
            }
        }
        RecoveryAnchor::Admission {
            admission_id,
            plan_id,
            remainder,
        } => {
            let a=sqlx::query("SELECT a.*,p.state anchor_plan_state FROM migration_people_admission a JOIN migration_people_admission_plan p ON p.admission_id=a.id AND p.organization_id=a.organization_id WHERE a.id=$1 AND p.id=$2 AND a.organization_id=$3 FOR UPDATE OF a,p").bind(admission_id).bind(plan_id).bind(org).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
            if a.get::<Uuid, _>("parent_import_id") != report.get::<Uuid, _>("parent_import_id")
                || a.get::<Uuid, _>("parent_plan_id") != report.get::<Uuid, _>("parent_plan_id")
                || a.get::<i64, _>("source_account_id") != report.get::<i64, _>("source_account_id")
            {
                return Err(MigrationError::NotFound);
            }
            if a.get::<i64, _>("lifecycle_revision") != cmd.expected_anchor_revision
                || a.get::<String, _>("anchor_plan_state") != "ready"
            {
                return Err(MigrationError::Conflict);
            }
            let confirmed = a.get::<Option<Uuid>, _>("confirmed_admission_plan_id");
            let state = a.get::<String, _>("state");
            if remainder {
                if !is_recovery(&a)
                    || state != "cancelled"
                    || confirmed != Some(plan_id)
                    || a.get::<Uuid, _>("report_id") != cmd.report_id
                {
                    return Err(MigrationError::Conflict);
                }
            } else if confirmed.is_some() {
                if confirmed != Some(plan_id)
                    || !matches!(state.as_str(), "completed" | "cancelled")
                {
                    return Err(MigrationError::Conflict);
                }
            } else {
                if state != "ready" {
                    return Err(MigrationError::Conflict);
                }
                let latest:Uuid=sqlx::query_scalar("SELECT id FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 ORDER BY revision DESC LIMIT 1").bind(admission_id).bind(org).fetch_one(&mut *conn).await?;
                if latest != plan_id {
                    return Err(MigrationError::Conflict);
                }
                super::store::require_admin(conn, ctx).await?;
                sqlx::query("UPDATE migration_people_admission SET state='cancelled',cancelled_at=clock_timestamp(),lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(admission_id).bind(org).execute(&mut *conn).await?;
                s::release_control(conn, ctx.organization_id, admission_id).await?;
            }
            let source=sqlx::query("SELECT started_at,completed_at FROM migration_snapshot WHERE id=$1 AND organization_id=$2").bind(report.get::<Uuid,_>("newer_snapshot_id")).bind(org).fetch_one(&mut *conn).await?;
            let same = report.get::<Uuid, _>("newer_snapshot_id")
                == a.get::<Uuid, _>("newer_snapshot_id")
                && report.get::<i64, _>("newer_sequence") == a.get::<i64, _>("newer_sequence");
            if !same
                && source
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at")
                    .is_none_or(|t| {
                        t <= a.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at")
                    })
            {
                return Err(MigrationError::SourceNotEligible);
            }
        }
    }
    Ok(())
}

pub(crate) async fn check_boundary(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: &PgRow,
) -> Result<(), MigrationError> {
    let previous=sqlx::query("SELECT newer_snapshot_id,newer_sequence,newer_started_at,newer_completed_at FROM migration_people_admission WHERE organization_id=$1 AND parent_import_id=$2 AND id<>$3 AND confirmed_admission_plan_id IS NOT NULL ORDER BY confirmed_completed_at DESC,created_at DESC,id DESC LIMIT 1")
        .bind(org.0).bind(run.get::<Uuid,_>("parent_import_id")).bind(run.get::<Uuid,_>("id")).fetch_optional(&mut *conn).await?;
    if let Some(p) = previous {
        let same = p.get::<Uuid, _>("newer_snapshot_id") == run.get::<Uuid, _>("newer_snapshot_id")
            && p.get::<i64, _>("newer_sequence") == run.get::<i64, _>("newer_sequence")
            && p.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
                == run.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
            && p.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at")
                == run.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at");
        if !same
            && run.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
                <= p.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at")
        {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    Ok(())
}

pub(crate) fn frozen(run: &PgRow) -> Value {
    json!({"mode":"mapping_recovery","original_plan_id":run.get::<Option<Uuid>,_>("recovery_original_plan_id"),"anchor_admission_id":run.get::<Option<Uuid>,_>("recovery_admission_id"),"anchor_plan_id":run.get::<Option<Uuid>,_>("recovery_admission_plan_id"),"remainder":run.get::<bool,_>("recovery_remainder"),"candidate_count":run.get::<i64,_>("recovery_candidate_count").to_string(),"mapping_revision":run.get::<Option<i64>,_>("recovery_frozen_revision").map(|v|v.to_string()),"mapping_digest":run.get::<Option<Vec<u8>>,_>("recovery_choices_digest")})
}

/// A durable successful result remains a tombstone even if an identity row was
/// accidentally removed. Recovery must not turn a deleted Person into a new one.
pub(crate) async fn previously_materialized(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: &PgRow,
    source: &str,
) -> Result<bool, MigrationError> {
    Ok(
        sqlx::query_scalar("SELECT crm_people_recovery_previously_materialized($1,$2,$3)")
            .bind(org.0)
            .bind(run.get::<i64, _>("source_account_id"))
            .bind(source)
            .fetch_one(conn)
            .await?,
    )
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAcknowledgement {
    pub mapping_digest: Vec<u8>,
    pub candidate_count: i64,
    pub contact_count: i64,
    pub unassigned_count: i64,
    pub acknowledged_creation: bool,
}
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: admission::ConfirmPeopleAdmission,
    ack: RecoveryAcknowledgement,
    readiness: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    admission::confirm_internal(pool, key, ctx, id, cmd, Some(ack), readiness).await
}
pub(crate) async fn validate_confirmation(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    run: &PgRow,
    plan: &PgRow,
    ack: Option<&RecoveryAcknowledgement>,
) -> Result<(), MigrationError> {
    if !is_recovery(run) {
        return if ack.is_none() {
            Ok(())
        } else {
            Err(MigrationError::InvalidInput)
        };
    }
    let ack = ack.ok_or(MigrationError::Conflict)?;
    if !ack.acknowledged_creation
        || run
            .get::<Option<Vec<u8>>, _>("recovery_choices_digest")
            .as_deref()
            != Some(ack.mapping_digest.as_slice())
        || ack.candidate_count != plan.get::<i64, _>("recovery_candidate_count")
        || ack.candidate_count != run.get::<i64, _>("recovery_candidate_count")
        || ack.contact_count != plan.get::<i64, _>("intended_contact_count")
        || ack.unassigned_count != plan.get::<i64, _>("recovery_unassigned_count")
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("INSERT INTO migration_people_recovery_requirement(organization_id,capability) VALUES($1,'fub-people-recovery-v1') ON CONFLICT(organization_id) DO NOTHING").bind(ctx.organization_id.0).execute(conn).await?;
    Ok(())
}

/// Initial recovery approval is available only through the exact committed
/// Person/result/identity tuple. Drafts and held candidates confer no authority.
pub(crate) async fn initial_approval(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    admission: Uuid,
    person: Uuid,
    source: &super::people_mapping_repair::SourceKey,
) -> Result<Option<(Uuid, Option<Uuid>)>, MigrationError> {
    let row=sqlx::query("SELECT c.* FROM migration_people_admission_result z JOIN migration_people_admission_item i ON i.id=z.item_id AND i.admission_id=z.admission_id AND i.organization_id=z.organization_id JOIN migration_people_admission a ON a.id=z.admission_id AND a.organization_id=z.organization_id JOIN migration_import_identity mi ON mi.admission_result_id=z.id AND mi.admission_item_id=i.id AND mi.admission_id=a.id AND mi.organization_id=a.organization_id AND mi.source_account_id=a.source_account_id AND mi.family='people' AND mi.source_id=z.source_id AND mi.target_id=z.person_id JOIN migration_people_recovery_choice c ON c.id=CASE WHEN $4='stage' THEN i.recovery_stage_choice_id ELSE i.recovery_assignee_choice_id END AND c.admission_id=a.id AND c.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$2 AND z.person_id=$3 AND z.disposition='settled' AND a.mode='mapping_recovery' AND a.confirmed_admission_plan_id=i.plan_id AND c.kind=$4 AND c.source_key_hmac=$5 AND c.revision<=a.recovery_frozen_revision")
        .bind(admission).bind(org.0).bind(person).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).fetch_optional(&mut *conn).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let body: Value = s::open(
        key,
        org,
        admission,
        row.get("id"),
        "recovery-choice",
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let target: Option<Uuid> = row.get("target_id");
    if body["source"] != serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?
        || body["target"]
            != super::people_mapping_repair::target_snapshot(
                conn,
                org,
                source.kind(),
                &row.get::<String, _>("disposition"),
                target,
            )
            .await?
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(Some((row.get("id"), target)))
}

pub(crate) async fn observation_matches(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &PgRow,
    source_id: &str,
    record: &super::people_admission_source::RetainedRecord,
) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT id,nonce,ciphertext FROM migration_people_recovery_candidate WHERE admission_id=$1 AND organization_id=$2 AND source_id=$3").bind(run.get::<Uuid,_>("id")).bind(org.0).bind(source_id).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let evidence: Value = s::open(
        key,
        org,
        run.get("id"),
        candidate.get("id"),
        "recovery-candidate",
        candidate.get("nonce"),
        candidate.get("ciphertext"),
    )?;
    let semantic = super::crypto::snapshot_hmac(
        key,
        org,
        &format!(
            "semantic:{}",
            super::snapshot_source::Stream::People.representation()
        ),
        &record.record.canonical,
    );
    Ok(evidence["anchor_qualified"] == true
        && evidence["source_id"] == source_id
        && evidence["capture_id"] == json!(record.capture_id)
        && evidence["ordinal"] == json!(record.ordinal)
        && evidence["semantic_hmac"] == json!(semantic.to_vec()))
}

pub(crate) mod coverage;

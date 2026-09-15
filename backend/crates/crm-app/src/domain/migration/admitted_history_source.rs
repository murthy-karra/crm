//! Retained-only source qualification and exact terminal admission linkage.
use super::{crypto, history_capture_store as capture, history_import_source, MigrationError};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub(crate) async fn binding(
    conn: &mut PgConnection,
    org: OrganizationId,
    admission: Uuid,
    capture_id: Uuid,
) -> Result<Value, MigrationError> {
    let a = sqlx::query("SELECT a.*,s.state AS snapshot_state,s.capture_sequence AS snapshot_sequence,s.source_account_id AS snapshot_account,s.completed_at AS snapshot_completed FROM migration_people_admission a JOIN migration_snapshot s ON s.id=a.newer_snapshot_id AND s.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$2")
        .bind(admission).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)?;
    if !matches!(
        a.get::<String, _>("state").as_str(),
        "completed" | "cancelled"
    ) || a
        .get::<Option<Uuid>, _>("confirmed_admission_plan_id")
        .is_none()
        || !matches!(
            a.get::<String, _>("snapshot_state").as_str(),
            "completed" | "completed_with_gaps"
        )
        || a.get::<i64, _>("newer_sequence") != a.get::<i64, _>("snapshot_sequence")
        || a.get::<i64, _>("source_account_id") != a.get::<i64, _>("snapshot_account")
        || Some(a.get::<DateTime<Utc>, _>("newer_completed_at"))
            != a.get::<Option<DateTime<Utc>>, _>("snapshot_completed")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let has_cohort: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_people_admission_item i ON i.id=r.item_id AND i.admission_id=r.admission_id AND i.organization_id=r.organization_id AND i.settled_result_id=r.id WHERE r.admission_id=$1 AND r.organization_id=$2 AND i.plan_id=$3 AND r.disposition='settled' AND r.person_id=i.prospective_person_id)")
        .bind(admission).bind(org.0).bind(a.get::<Uuid,_>("confirmed_admission_plan_id")).fetch_one(&mut *conn).await?;
    if !has_cohort {
        return Err(MigrationError::SourceNotEligible);
    }
    let c = sqlx::query(
        "SELECT * FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2",
    )
    .bind(capture_id)
    .bind(org.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if !capture::current_profile(&c)
        || c.get::<String, _>("state") != "completed_with_gaps"
        || c.get::<Uuid, _>("parent_import_id") != a.get::<Uuid, _>("parent_import_id")
        || c.get::<Uuid, _>("parent_plan_id") != a.get::<Uuid, _>("parent_plan_id")
        || c.get::<i64, _>("source_account_id") != a.get::<i64, _>("source_account_id")
        || c.get::<i64, _>("workspace_revision") != a.get::<i64, _>("workspace_revision")
        || c.get::<Option<DateTime<Utc>>, _>("started_at")
            .is_none_or(|v| v <= a.get::<DateTime<Utc>, _>("newer_completed_at"))
        || c.get::<Option<DateTime<Utc>>, _>("completed_at").is_none()
        || c.get::<Option<i64>, _>("parent_source_user_id")
            != Some(c.get::<i64, _>("source_user_id"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    capture::verify_parent(conn, org, &c).await?;
    let streams = sqlx::query("SELECT * FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 ORDER BY family")
        .bind(capture_id).bind(org.0).fetch_all(&mut *conn).await?;
    if streams.len() != 3
        || streams.iter().any(|s| {
            s.get::<String, _>("state") != "enumerated"
                || s.get::<Option<String>, _>("reported_total").as_deref()
                    != Some(s.get::<i64, _>("unique_ids").to_string().as_str())
        })
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let coverage: Vec<Value> = streams.iter().map(|s| json!({"family":s.get::<String,_>("family"),"state":"enumerated","reported_total":s.get::<Option<String>,_>("reported_total"),"occurrences":s.get::<i64,_>("occurrences").to_string(),"unique_ids":s.get::<i64,_>("unique_ids").to_string(),"invalid_occurrences":s.get::<i64,_>("invalid_occurrences").to_string(),"checkpoint":s.get::<i64,_>("checkpoint").to_string()})).collect();
    Ok(json!({
        "admission_id":admission,"admission_plan_id":a.get::<Uuid,_>("confirmed_admission_plan_id"),
        "report_id":a.get::<Uuid,_>("report_id"),"admission_revision":a.get::<i64,_>("lifecycle_revision").to_string(),
        "newer_snapshot_id":a.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":a.get::<i64,_>("newer_sequence").to_string(),
        "newer_completed_at":a.get::<DateTime<Utc>,_>("newer_completed_at"),
        "parent_import_id":c.get::<Uuid,_>("parent_import_id"),"parent_plan_id":c.get::<Uuid,_>("parent_plan_id"),
        "snapshot_id":c.get::<Uuid,_>("snapshot_id"),"parent_capture_sequence":c.get::<i64,_>("parent_capture_sequence").to_string(),
        "workspace_revision":c.get::<i64,_>("workspace_revision").to_string(),
        "capture_id":capture_id,"capture_revision":c.get::<i64,_>("revision").to_string(),"capture_sequence":c.get::<i64,_>("capture_sequence").to_string(),
        "started_at":c.get::<DateTime<Utc>,_>("started_at"),"completed_at":c.get::<DateTime<Utc>,_>("completed_at"),
        "source_account_id":c.get::<i64,_>("source_account_id").to_string(),"source_access_user_id":c.get::<i64,_>("source_user_id").to_string(),
        "profile_version":c.get::<String,_>("profile_version"),"parser_version":c.get::<String,_>("parser_version"),"schema_version":c.get::<String,_>("schema_version"),
        "interpretation_version":history_import_source::INTERPRETATION,"identity_version":"timeline-import-identity-v1",
        "coverage":{"streams":coverage,"warnings":["api_restricted_records_unknown","detail_content_not_fetched","not_atomic_snapshot","external_facts_only"],"enumeration_is_complete_account_history":false}
    }))
}

pub(crate) fn hash(
    key: &RawPayloadKey,
    org: OrganizationId,
    binding: &Value,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::snapshot_hmac(
        key,
        org,
        "admitted-history-plan-v1",
        &serde_json::to_vec(binding).map_err(|_| MigrationError::Crypto)?,
    ))
}

/// Resolve only the exact settled admission result, independent of the old
/// capture's original-parent linkage. Erasure is never a new target candidate.
pub(crate) async fn target(
    conn: &mut PgConnection,
    org: OrganizationId,
    admission: Uuid,
    plan: Uuid,
    account: i64,
    source: Option<&str>,
) -> Result<(Option<Uuid>, Option<&'static str>), MigrationError> {
    let Some(source) = source else {
        return Ok((None, Some("ambiguous_relationship")));
    };
    let r=sqlx::query("SELECT r.person_id,p.id AS live_id,x.target_id AS identity_person FROM migration_people_admission_result r JOIN migration_people_admission_item i ON i.id=r.item_id AND i.admission_id=r.admission_id AND i.organization_id=r.organization_id AND i.settled_result_id=r.id LEFT JOIN person p ON p.id=r.person_id AND p.organization_id=r.organization_id LEFT JOIN migration_import_identity x ON x.organization_id=r.organization_id AND x.source_account_id=$5 AND x.family='people' AND x.source_id=r.source_id AND x.admission_result_id=r.id WHERE r.admission_id=$1 AND i.plan_id=$2 AND r.organization_id=$3 AND r.source_id=$4 AND r.disposition='settled' AND r.person_id=i.prospective_person_id")
        .bind(admission).bind(plan).bind(org.0).bind(source).bind(account).fetch_optional(conn).await?;
    let Some(r) = r else {
        return Ok((None, Some("out_of_cohort")));
    };
    let person = r.get::<Option<Uuid>, _>("person_id");
    let reason = if r.get::<Option<Uuid>, _>("live_id").is_none() {
        Some("target_erased")
    } else if r.get::<Option<Uuid>, _>("identity_person") != person {
        Some("target_identity_mismatch")
    } else {
        None
    };
    Ok((person, reason))
}

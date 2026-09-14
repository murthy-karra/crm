//! Bounded headers use persisted counters; polling never scans the cohort.
use super::super::metadata_model::Counts;
use super::*;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgConnection};

pub(super) async fn detail(
    c: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let r=sqlx::query("SELECT i.*,COALESCE(cr.state='ready',false) AS shared_claims_ready FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness cr ON cr.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2").bind(id).bind(org.0).fetch_optional(&mut *c).await?.ok_or(MigrationError::NotFound)?;
    row(c, org, &r).await
}
pub(super) async fn row(
    c: &mut PgConnection,
    org: OrganizationId,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    let id: Uuid = r.get("id");
    let state: String = r.get("state");
    let plan=sqlx::query("SELECT * FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3").bind(r.get::<Option<Uuid>,_>("latest_plan_id")).bind(id).bind(org.0).fetch_optional(&mut *c).await?;
    let counts = match r.get::<Option<Value>, _>("counts") {
        Some(v) => Counts::load(v)?,
        None => Counts::default(),
    };
    let mut ready = false;
    let mut executable = false;
    let latest = if let Some(p) = &plan {
        let stored: Value = p.get("counts");
        let pc = if p.get::<String, _>("state") == "building" {
            Counts::default()
        } else {
            Counts::load(stored)?
        };
        ready = p.get::<String, _>("state") == "ready"
            && p.get::<Option<DateTime<Utc>>, _>("expires_at")
                .is_some_and(|t| t > Utc::now());
        executable = pc.tags.eligible
            + pc.fields.eligible
            + pc.options.eligible
            + pc.tag_links.eligible
            + pc.values.eligible
            > 0;
        json!({"id":p.get::<Uuid,_>("id"),"revision":p.get::<i64,_>("revision").to_string(),"state":p.get::<String,_>("state"),"phase":p.get::<String,_>("preparation_phase"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"expires_at":p.get::<Option<DateTime<Utc>>,_>("expires_at"),"confirmation_digest":p.get::<Option<Vec<u8>>,_>("digest").map(|v|metadata_store::hex(&v)),"counts":pc.wire(),"max_added_byte_bound":p.get::<i64,_>("max_added_byte_bound").to_string()})
    } else {
        Value::Null
    };
    let policy = policy();
    let s=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *c).await?;
    let cancellation:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_admitted_metadata_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='cancel'").bind(id).bind(org.0).fetch_one(&mut *c).await?;
    let boundary = core_change_store::boundary(c, org, r.get("snapshot_id")).await?;
    let progress = if r.get::<String, _>("phase") == "preparation" {
        match plan
            .as_ref()
            .map(|p| p.get::<String, _>("preparation_phase"))
            .as_deref()
        {
            Some("cohort") => "cohort",
            Some("values") => "baselines",
            Some("seal" | "complete") => "seal",
            _ => "fields",
        }
    } else {
        match r.get::<String, _>("phase").as_str() {
            "catalog" => "catalog",
            "people" => "people",
            _ => "complete",
        }
    };
    let confirmed: Option<Uuid> = r.get("confirmed_plan_id");
    let remaining = counts
        .people
        .eligible
        .saturating_sub(r.get::<i64, _>("settled_eligible_people"));
    let remainder = json!({"available":state=="cancelled" && confirmed.is_some() && r.get::<Option<Uuid>,_>("successor_import_id").is_none() && (remaining>0 || counts.tags.eligible+counts.fields.eligible+counts.options.eligible>0),"predecessor_import_id":r.get::<Option<Uuid>,_>("predecessor_import_id"),"successor_import_id":r.get::<Option<Uuid>,_>("successor_import_id"),"remaining_catalog":(counts.tags.eligible+counts.fields.eligible+counts.options.eligible).to_string(),"remaining_people":remaining.to_string(),"excluded_settled_catalog":(counts.tags.planned-counts.tags.pending+counts.fields.planned-counts.fields.pending+counts.options.planned-counts.options.pending).to_string(),"excluded_settled_people":(counts.people.settled-r.get::<i64,_>("held_settled_people")).to_string(),"excluded_held_people":(counts.people.excluded+r.get::<i64,_>("held_settled_people")-counts.people.settled+r.get::<i64,_>("settled_eligible_people")).to_string()});
    let source_gaps: Vec<String> = boundary["streams"]
        .as_array()
        .ok_or(MigrationError::Crypto)?
        .iter()
        .filter(|v| v["state"] != "completed" || v["content_gaps"] != "0")
        .map(|v| {
            format!(
                "{}:{}",
                v["stream"].as_str().unwrap_or("unknown"),
                v["state"].as_str().unwrap_or("unknown")
            )
        })
        .collect();
    let mut body = serde_json::Map::new();
    if let Value::Object(part) = json!({"id":id,"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"admission_id":r.get::<Uuid,_>("admission_id"),"admission_plan_id":r.get::<Uuid,_>("admission_plan_id"),"source_report_id":r.get::<Uuid,_>("source_report_id"),"source_output_revision":plan.as_ref().map(|p|p.get::<Uuid,_>("source_output_revision")),"snapshot_id":r.get::<Uuid,_>("snapshot_id"),"source_capture_interval":boundary,"source_account_id":r.get::<i64,_>("source_account_id").to_string(),"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"workspace_revision":r.get::<i64,_>("workspace_revision").to_string(),"engine_version":ENGINE})
    {
        body.extend(part);
    }
    if let Value::Object(part) = json!({"state":state,"phase":r.get::<String,_>("phase"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"confirmed_at":r.get::<Option<DateTime<Utc>>,_>("confirmed_at"),"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"confirmed_plan_id":confirmed,"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),"cancellation_reserved_bytes":cancellation.to_string(),"release_ready":true,"shared_claims_ready":r.get::<bool,_>("shared_claims_ready"),"counts":counts.wire(),"latest_plan":latest})
    {
        body.extend(part);
    }
    if let Value::Object(part) = json!({"cohort_counts":{"settled_people":r.get::<i64,_>("settled_people").to_string(),"eligible_people":counts.people.eligible.to_string(),"excluded_people":counts.people.excluded.to_string(),"settled_metadata_people":counts.people.settled.to_string(),"remaining_people":remaining.to_string()},"progress":{"phase":progress,"fields_processed":plan.as_ref().map_or(0,|p|p.get::<i64,_>("fields_processed")).to_string(),"people_processed":plan.as_ref().map_or(0,|p|p.get::<i64,_>("people_processed")).to_string()},"remainder":remainder,"actions":{"replan":state=="proposed" && ready,"confirm":state=="proposed" && ready && executable,"retry":state=="paused","cancel":matches!(state.as_str(),"proposed"|"queued"|"running"|"paused"),"remainder":remainder["available"]}})
    {
        body.extend(part);
    }
    if let Value::Object(part) = json!({"policy":{"run_byte_limit":s.get::<i64,_>("run_byte_limit").to_string(),"org_byte_limit":s.get::<i64,_>("byte_limit").to_string(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"run_retained_bytes":s.get::<i64,_>("retained_bytes").to_string(),"run_reserved_bytes":s.get::<i64,_>("reserved_bytes").to_string(),"org_retained_bytes":s.get::<i64,_>("org_retained").to_string(),"org_reserved_bytes":s.get::<i64,_>("org_reserved").to_string(),"unit_byte_limit":metadata_store::UNIT.to_string(),"policy_revision":policy.revision()},"coverage":{"embedded_tags_only":true,"custom_fields_complete":true,"metadata_excluded_people":counts.people.excluded.to_string(),"remaining_data":["history","notes","tasks","conversations","attachments","deals","activation"],"source_gaps":source_gaps}})
    {
        body.extend(part);
    }
    Ok(Value::Object(body))
}

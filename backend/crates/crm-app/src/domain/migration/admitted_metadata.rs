//! D-082 admitted-People metadata root.  This deliberately has no path through
//! the original metadata child: its cohort is terminal admission results.
use super::MigrationError;
use crate::{domain::envelope::CommandContext, ids::OrganizationId};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const ENGINE: &str = "fub-admitted-metadata-v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub request_id: Uuid,
    pub admission_id: Uuid,
    pub source_report_id: Uuid,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub cursor: Option<Uuid>,
    pub limit: Option<u16>,
    pub admission_id: Option<Uuid>,
}
impl Page {
    fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(50);
        if !(1..=50).contains(&n) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: Uuid,
}

fn detail(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"parent_import_id":row.get::<Uuid,_>("parent_import_id"),
      "admission_id":row.get::<Uuid,_>("admission_id"),"source_report_id":row.get::<Uuid,_>("source_report_id"),
      "snapshot_id":row.get::<Uuid,_>("snapshot_id"),"source_account_id":row.get::<i64,_>("source_account_id").to_string(),
      "capture_sequence":row.get::<i64,_>("capture_sequence").to_string(),"workspace_revision":row.get::<i64,_>("workspace_revision").to_string(),
      "engine_version":row.get::<String,_>("engine_version"),"state":row.get::<String,_>("state"),"phase":row.get::<String,_>("phase"),
      "shared_claims_ready":row.get::<bool,_>("shared_claims_ready"),"cohort_counts":{"settled_people":row.get::<i64,_>("settled_people").to_string()},
      "remainder":{"available":row.get::<bool,_>("remainder_available")},"actions":{"replan":false,"confirm":false,"retry":false,"cancel":row.get::<String,_>("state")=="proposed","remainder":false}})
}
async fn find(
    pool: &PgPool,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
 .bind(id).bind(org.0).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)
}
pub async fn list(pool: &PgPool, ctx: &CommandContext, q: Page) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    let after = q.cursor.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.organization_id=$1 AND i.id>$2 AND ($3::uuid IS NULL OR i.admission_id=$3) ORDER BY i.id LIMIT $4")
 .bind(ctx.organization_id.0).bind(after).bind(q.admission_id).bind(limit+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > limit;
    let items = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| detail(&r))
        .collect::<Vec<_>>();
    let next = if more {
        items
            .last()
            .and_then(|v| v["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(json!({"imports":items,"next_cursor":next}))
}
pub async fn get(pool: &PgPool, ctx: &CommandContext, id: Uuid) -> Result<Value, MigrationError> {
    Ok(detail(&find(pool, ctx.organization_id, id).await?))
}
pub async fn prepare(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: Prepare,
) -> Result<Value, MigrationError> {
    let mut tx = pool.begin().await?;
    let a=sqlx::query("SELECT a.parent_import_id,a.parent_plan_id,a.source_account_id,a.newer_snapshot_id,a.newer_sequence,a.workspace_revision,a.state,r.newer_snapshot_id AS report_snapshot,r.newer_sequence AS report_sequence,r.state AS report_state FROM migration_people_admission a JOIN migration_core_change_report r ON r.id=$2 AND r.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$3 FOR UPDATE")
 .bind(cmd.admission_id).bind(cmd.source_report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::SourceNotEligible)?;
    if !matches!(
        a.get::<String, _>("state").as_str(),
        "completed" | "cancelled"
    ) || a.get::<String, _>("report_state") != "completed"
        || a.get::<Uuid, _>("report_snapshot") != a.get::<Uuid, _>("newer_snapshot_id")
        || a.get::<i64, _>("report_sequence") != a.get::<i64, _>("newer_sequence")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let settled:i64=sqlx::query_scalar("SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND disposition='settled' AND person_id IS NOT NULL").bind(cmd.admission_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if settled == 0 {
        return Err(MigrationError::SourceNotEligible);
    }
    let id = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let inserted=sqlx::query("INSERT INTO migration_admitted_metadata_import(id,organization_id,parent_import_id,parent_plan_id,admission_id,source_report_id,snapshot_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,engine_version,state,latest_plan_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'proposed',$13) ON CONFLICT(organization_id,admission_id) DO NOTHING").bind(id).bind(ctx.organization_id.0).bind(a.get::<Uuid,_>("parent_import_id")).bind(a.get::<Uuid,_>("parent_plan_id")).bind(cmd.admission_id).bind(cmd.source_report_id).bind(a.get::<Uuid,_>("newer_snapshot_id")).bind(a.get::<i64,_>("source_account_id")).bind(a.get::<i64,_>("newer_sequence")).bind(a.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(ENGINE).bind(plan).execute(&mut *tx).await?;
    if inserted.rows_affected() == 1 {
        sqlx::query("INSERT INTO migration_admitted_metadata_plan(id,import_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext,counts) VALUES($1,$2,$3,1,'building',$4,$5,$6)").bind(plan).bind(id).bind(ctx.organization_id.0).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(json!({"people":{"source":settled,"eligible":settled,"excluded":0,"settled":0}})).execute(&mut *tx).await?;
    }
    let resolved = if inserted.rows_affected() == 1 {
        id
    } else {
        sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_import WHERE organization_id=$1 AND admission_id=$2").bind(ctx.organization_id.0).bind(cmd.admission_id).fetch_one(&mut *tx).await?
    };
    tx.commit().await?;
    Ok(json!({"import":get(pool,ctx,resolved).await?,"request_id":cmd.request_id}))
}

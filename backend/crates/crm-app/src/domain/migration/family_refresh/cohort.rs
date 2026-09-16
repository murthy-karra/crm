//! A bounded retained-only preparation transaction, dispatched by the existing
//! worker in the later integration stage. No network access or polling loop.
use crate::{
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
    ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const PAGE_SQL: &str = include_str!("sql/cohort_page.sql");
const PAGE_BYTES: i64 = 64 * 1024;
/// Server-owned lease claim; never deserialize this from an HTTP request.
pub struct Claim {
    pub organization: OrganizationId,
    pub bundle: Uuid,
    pub plan: Uuid,
    pub token: Uuid,
    pub epoch: i64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced {
        examined: usize,
        included: usize,
        finished: bool,
    },
    Finished,
    Capacity,
}
/// Freeze at most 50 identity proofs. A failed unit leaves no rows, checkpoint,
/// counters or charge behind. Replaying an already-finished phase is a no-op.
pub async fn freeze_page(
    pool: &PgPool,
    claim: &Claim,
    policy: &SnapshotPolicy,
    limit: u16,
) -> Result<Progress, MigrationError> {
    if limit == 0 || limit > 50 || claim.epoch <= 0 {
        return Err(MigrationError::InvalidInput);
    }
    let (mut tx, b, p) = super::preparation::begin(pool, claim).await?;
    if b.get::<Option<Uuid>, _>("payer_plan_id") != Some(claim.plan) {
        return Err(MigrationError::Conflict);
    }
    if p.get::<String, _>("phase") != "cohort" {
        return Ok(Progress::Finished);
    }
    let Some(reservation) = super::preparation::reserve(&mut tx, claim, policy, PAGE_BYTES).await?
    else {
        return Ok(Progress::Capacity);
    };
    let after: String = p.get("cohort_after");
    let rows = sqlx::query(PAGE_SQL)
        .bind(claim.organization.0)
        .bind(b.get::<i64, _>("source_account_id"))
        .bind(b.get::<Uuid, _>("parent_import_id"))
        .bind(&after)
        .bind(b.get::<DateTime<Utc>, _>("created_at"))
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await?;
    let finished = rows.len() <= usize::from(limit);
    let mut counts: Value = p
        .get::<Option<Value>, _>("cohort_counts")
        .unwrap_or_else(|| json!({}));
    if !counts.is_object() {
        return Err(MigrationError::Crypto);
    }
    let mut last = after;
    let mut included = 0;
    let examined = rows.len().min(usize::from(limit));
    for row in rows.iter().take(examined) {
        last = row.get("source_id");
        let disposition: String = row.get("disposition");
        let previous = match counts.get(&disposition) {
            None => 0,
            Some(v) => v
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(MigrationError::Crypto)?,
        };
        counts[&disposition] = json!(previous
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(MigrationError::StorageLimit)?
            .to_string());
        if matches!(disposition.as_str(), "original" | "admitted" | "recovery") {
            sqlx::query("INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,admission_id,admission_result_id,creation_snapshot_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(Uuid::new_v4()).bind(claim.bundle).bind(claim.organization.0).bind(&last).bind(row.get::<Uuid,_>("person_id"))
                .bind(row.get::<Option<Uuid>,_>("original_result_id")).bind(row.get::<Option<Uuid>,_>("admission_id")).bind(row.get::<Option<Uuid>,_>("admission_result_id")).bind(row.get::<Option<Uuid>,_>("creation_snapshot_id"))
                .execute(&mut *tx).await?;
            included += 1;
        }
    }
    // Recheck expiry at the final checkpoint and again during settlement.
    let updated=sqlx::query("UPDATE migration_family_refresh_plan SET cohort_after=$4,cohort_counts=$5,checkpoint=checkpoint+$6,phase=$7 WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$8 AND lease_expires_at>clock_timestamp() AND state='preparing' AND phase='cohort'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(last).bind(counts).bind(examined as i64).bind(if finished {"capture"} else {"cohort"}).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if updated != 1 {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Progress::Advanced {
        examined,
        included,
        finished,
    })
}

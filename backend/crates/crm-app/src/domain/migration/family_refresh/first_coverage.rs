//! First-coverage prerequisites for genuinely new identities. Old held results
//! are never turned into baselines. All confirmed roots remain source boundaries.
use super::model::{Family, Hold};
use crate::{domain::migration::MigrationError, ids::OrganizationId};
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgConnection, Row};

pub(super) async fn qualify(
    conn: &mut PgConnection,
    org: OrganizationId,
    bundle: &PgRow,
    plan: &PgRow,
    cohort: &PgRow,
    family: Family,
) -> Result<Result<(), Hold>, MigrationError> {
    let admitted = cohort
        .get::<Option<uuid::Uuid>, _>("admission_result_id")
        .is_some();
    let (sql, selected_column, source_table) = match family {
        Family::Activity => {
            let (prefix, cohort_column, root_filter) = if admitted {
                (
                    "migration_admitted_activity",
                    "admission_result_id",
                    "a.admission_id=$5",
                )
            } else {
                ("migration_activity", "parent_result_id", "$5::uuid IS NULL")
            };
            (
                include_str!("sql/activity_coverage.sql")
                    .replace("__PREFIX__", prefix)
                    .replace("__COHORT__", cohort_column)
                    .replace("__ROOT_FILTER__", root_filter),
                "source_snapshot_id",
                "migration_snapshot",
            )
        }
        Family::History => (
            if admitted {
                include_str!("sql/admitted_history_coverage.sql")
            } else {
                include_str!("sql/history_coverage.sql")
            }
            .to_owned(),
            "history_capture_id",
            "migration_history_capture_run",
        ),
        Family::Metadata => return Err(MigrationError::InvalidInput),
    };
    let selected: uuid::Uuid = plan.get(selected_column);
    let selected_row = sqlx::query(&format!(
        "SELECT started_at FROM {source_table} WHERE id=$1 AND organization_id=$2"
    ))
    .bind(selected)
    .bind(org.0)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(started) = selected_row.and_then(|r| r.get::<Option<DateTime<Utc>>, _>("started_at"))
    else {
        return Ok(Err(Hold::SourceUnavailable));
    };
    // Return a fixed-size summary regardless of the number of prior roots.
    // This compares every accepted interval, never selects a winning source value.
    let summary = format!("SELECT count(*) AS roots,COALESCE(bool_or(proven),false) AS proven,COALESCE(bool_and(owner_state IN ('completed','cancelled') AND terminal_at IS NOT NULL AND terminal_at<=$9),false) AS terminal,COALESCE(bool_and(source_account_id=$4),false) AS account,COALESCE(bool_and(started_at IS NOT NULL AND completed_at IS NOT NULL AND completed_at>=started_at AND state IN ('completed','completed_with_gaps')),false) AS available,COALESCE(bool_and(id<>$10 AND completed_at<$11),false) AS older FROM ({sql}) coverage");
    let row = sqlx::query(&summary)
        .bind(org.0)
        .bind(bundle.get::<uuid::Uuid, _>("parent_import_id"))
        .bind(bundle.get::<uuid::Uuid, _>("parent_plan_id"))
        .bind(bundle.get::<i64, _>("source_account_id"))
        .bind(cohort.get::<Option<uuid::Uuid>, _>("admission_id"))
        .bind(cohort.get::<uuid::Uuid, _>("person_id"))
        .bind(cohort.get::<Option<uuid::Uuid>, _>(if admitted {
            "admission_result_id"
        } else {
            "original_result_id"
        }))
        .bind(cohort.get::<String, _>("source_person_id"))
        .bind(bundle.get::<DateTime<Utc>, _>("created_at"))
        .bind(selected)
        .bind(started)
        .fetch_one(conn)
        .await?;
    Ok(
        if row.get::<i64, _>("roots") == 0 || !row.get::<bool, _>("terminal") {
            Err(Hold::FirstCoverageRequired)
        } else if !row.get::<bool, _>("account") {
            Err(Hold::IdentityMismatch)
        } else if !row.get::<bool, _>("available") {
            Err(Hold::SourceUnavailable)
        } else if !row.get::<bool, _>("older") {
            Err(Hold::SourceNotNewer)
        } else if !row.get::<bool, _>("proven") {
            Err(Hold::FirstCoverageRequired)
        } else {
            Ok(())
        },
    )
}

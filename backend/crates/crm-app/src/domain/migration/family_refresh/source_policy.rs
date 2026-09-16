//! Retained-source ordering and occurrence reconciliation. Database discovery
//! supplies these facts from authenticated captures before cohort filtering.
use super::model::{Family, Hold};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct Boundary {
    pub capture: Uuid,
    pub account: i64,
    pub started: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
    pub terminal: bool,
}
impl Boundary {
    fn complete(self) -> Result<DateTime<Utc>, Hold> {
        self.completed
            .filter(|completed| self.terminal && *completed >= self.started)
            .ok_or(Hold::SourceUnavailable)
    }
}
/// Previous is the last accepted family capture, including first coverage.
/// History-only refresh passes the frozen cohort's core anchor as `core_anchor`;
/// combined refresh passes the selected newer core capture instead.
pub fn qualify_boundary(
    family: Family,
    selected: Boundary,
    previous: Boundary,
    core_anchor: Option<Boundary>,
) -> Result<(), Hold> {
    qualify_newer_capture(selected, previous)?;
    if family == Family::History {
        let core = core_anchor.ok_or(Hold::FirstCoverageRequired)?;
        let core_end = core.complete()?;
        if core.account != selected.account {
            return Err(Hold::IdentityMismatch);
        }
        if selected.started <= core_end {
            return Err(Hold::SourceNotNewer);
        }
    }
    Ok(())
}

/// Common ordering check, independent of the additional history/core anchor.
pub fn qualify_newer_capture(selected: Boundary, previous: Boundary) -> Result<(), Hold> {
    selected.complete()?;
    let previous_end = previous.complete()?;
    if selected.account != previous.account {
        return Err(Hold::IdentityMismatch);
    }
    if selected.capture == previous.capture || selected.started <= previous_end {
        return Err(Hold::SourceNotNewer);
    }
    Ok(())
}

/// Compare retained snapshot intervals inside the caller's preparation transaction.
/// Report completion alone does not establish ordering after a family's baseline.
pub(super) async fn qualify_core_snapshots(
    conn: &mut sqlx::PgConnection,
    organization: crate::ids::OrganizationId,
    account: i64,
    selected: Uuid,
    previous: Uuid,
) -> Result<Result<(), Hold>, crate::domain::migration::MigrationError> {
    use sqlx::Row;
    let rows=sqlx::query("SELECT id,source_account_id,started_at,completed_at,state FROM migration_snapshot WHERE organization_id=$1 AND id=ANY($2)")
        .bind(organization.0).bind(vec![selected,previous]).fetch_all(conn).await?;
    let boundary = |id: Uuid| -> Option<Boundary> {
        let row = rows.iter().find(|r| r.get::<Uuid, _>("id") == id)?;
        Some(Boundary {
            capture: id,
            account: row.get("source_account_id"),
            started: row.get::<Option<DateTime<Utc>>, _>("started_at")?,
            completed: row.get("completed_at"),
            terminal: matches!(
                row.get::<String, _>("state").as_str(),
                "completed" | "completed_with_gaps"
            ),
        })
    };
    let (Some(selected), Some(previous)) = (boundary(selected), boundary(previous)) else {
        return Ok(Err(Hold::SourceUnavailable));
    };
    if selected.account != account || previous.account != account {
        return Ok(Err(Hold::IdentityMismatch));
    }
    Ok(qualify_newer_capture(selected, previous))
}

/// Fresh preparation cannot reinterpret a capture already accepted for this
/// cohort/family, even if every native unit was held or execution was cancelled.
/// Exact remainders copy frozen manifests; they do not run fresh discovery.
pub(super) async fn qualify_accepted_scan(
    conn: &mut sqlx::PgConnection,
    organization: crate::ids::OrganizationId,
    bundle: &sqlx::postgres::PgRow,
    plan: &sqlx::postgres::PgRow,
    cohort: &sqlx::postgres::PgRow,
) -> Result<Result<(), Hold>, crate::domain::migration::MigrationError> {
    use sqlx::Row;
    let family: String = plan.get("family");
    let (table, column) = match family.as_str() {
        "metadata" | "activity" => ("migration_snapshot", "source_snapshot_id"),
        "history" => ("migration_history_capture_run", "history_capture_id"),
        _ => return Err(crate::domain::migration::MigrationError::InvalidInput),
    };
    let selected: Uuid = plan.get(column);
    let source = sqlx::query(&format!("SELECT id,source_account_id,started_at,completed_at,state FROM {table} WHERE organization_id=$1 AND id=$2"))
        .bind(organization.0).bind(selected).fetch_optional(&mut *conn).await?;
    let Some(source) = source else {
        return Ok(Err(Hold::SourceUnavailable));
    };
    let Some(started) = source.get::<Option<DateTime<Utc>>, _>("started_at") else {
        return Ok(Err(Hold::SourceUnavailable));
    };
    if source.get::<i64, _>("source_account_id") != bundle.get::<i64, _>("source_account_id") {
        return Ok(Err(Hold::IdentityMismatch));
    }
    let boundary = Boundary {
        capture: selected,
        account: source.get("source_account_id"),
        started,
        completed: source.get("completed_at"),
        terminal: matches!(
            source.get::<String, _>("state").as_str(),
            "completed" | "completed_with_gaps"
        ),
    };
    if let Err(hold) = boundary.complete() {
        return Ok(Err(hold));
    }
    let sql = include_str!("sql/accepted_scan.sql")
        .replace("__SOURCE_TABLE__", table)
        .replace("__SOURCE_COLUMN__", column);
    let reason: Option<String> = sqlx::query_scalar(&sql)
        .bind(organization.0)
        .bind(bundle.get::<Uuid, _>("parent_import_id"))
        .bind(bundle.get::<Uuid, _>("id"))
        .bind(boundary.account)
        .bind(bundle.get::<Uuid, _>("parent_plan_id"))
        .bind(family)
        .bind(cohort.get::<Uuid, _>("person_id"))
        .bind(cohort.get::<Option<Uuid>, _>("original_result_id"))
        .bind(cohort.get::<Option<Uuid>, _>("admission_result_id"))
        .bind(bundle.get::<DateTime<Utc>, _>("created_at"))
        .bind(selected)
        .bind(started)
        .bind(cohort.get::<Uuid, _>("creation_snapshot_id"))
        .bind(cohort.get::<String, _>("source_person_id"))
        .fetch_optional(conn)
        .await?;
    Ok(match reason.as_deref() {
        None => Ok(()),
        Some("identity_mismatch") => Err(Hold::IdentityMismatch),
        Some("baseline_unproven") => Err(Hold::BaselineUnproven),
        Some("source_unavailable") => Err(Hold::SourceUnavailable),
        Some("source_not_newer") => Err(Hold::SourceNotNewer),
        _ => return Err(crate::domain::migration::MigrationError::Crypto),
    })
}

pub struct Occurrence<'a> {
    pub identity: &'a [u8],
    pub semantic: &'a [u8],
    pub representation: &'a str,
    pub source_person: Option<&'a str>,
    pub qualified: bool,
}
/// Streams an identity group from the source index; memory stays constant.
/// The caller must include every occurrence, including unqualified occurrences
/// and observations whose Person is outside the selected cohort.
pub fn reconcile_occurrences<'a>(
    occurrences: impl IntoIterator<Item = Occurrence<'a>>,
) -> Result<u64, Hold> {
    let mut occurrences = occurrences.into_iter();
    let first = occurrences.next().ok_or(Hold::SourceNotObserved)?;
    if !first.qualified || first.identity.len() != 32 || first.semantic.len() != 32 {
        return Err(Hold::UnsupportedSource);
    }
    let mut count = 1_u64;
    for observation in occurrences {
        if !observation.qualified
            || observation.identity != first.identity
            || observation.semantic != first.semantic
            || observation.representation != first.representation
            || observation.source_person != first.source_person
        {
            return Err(Hold::SourceConflict);
        }
        count = count.checked_add(1).ok_or(Hold::UnitTooLarge)?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boundary(start: i64, end: i64) -> Boundary {
        Boundary {
            capture: Uuid::new_v4(),
            account: 1,
            started: DateTime::from_timestamp(start, 0).unwrap(),
            completed: Some(DateTime::from_timestamp(end, 0).unwrap()),
            terminal: true,
        }
    }
    #[test]
    fn source_selection_requires_new_complete_same_account_capture() {
        let old = boundary(1, 2);
        let new = boundary(3, 4);
        assert_eq!(qualify_boundary(Family::Activity, new, old, None), Ok(()));
        assert_eq!(
            qualify_boundary(Family::Metadata, old, old, None),
            Err(Hold::SourceNotNewer)
        );
        assert_eq!(
            qualify_boundary(Family::Activity, boundary(2, 4), old, None),
            Err(Hold::SourceNotNewer)
        );
        assert_eq!(
            qualify_boundary(Family::Activity, Boundary { account: 2, ..new }, old, None),
            Err(Hold::IdentityMismatch)
        );
        assert_eq!(
            qualify_boundary(
                Family::Activity,
                Boundary {
                    terminal: false,
                    ..new
                },
                old,
                None
            ),
            Err(Hold::SourceUnavailable)
        );
    }
    #[test]
    fn history_cannot_precede_selected_core_or_omit_its_anchor() {
        let old = boundary(1, 2);
        let core = boundary(3, 4);
        assert_eq!(
            qualify_boundary(Family::History, boundary(5, 6), old, Some(core)),
            Ok(())
        );
        assert_eq!(
            qualify_boundary(Family::History, boundary(3, 6), old, Some(core)),
            Err(Hold::SourceNotNewer)
        );
        assert_eq!(
            qualify_boundary(Family::History, boundary(5, 6), old, None),
            Err(Hold::FirstCoverageRequired)
        );
    }
    fn observation(person: &'static str, semantic: &'static [u8]) -> Occurrence<'static> {
        Occurrence {
            identity: &[1; 32],
            semantic,
            representation: "note-detail-v1",
            source_person: Some(person),
            qualified: true,
        }
    }
    #[test]
    fn repeated_source_is_equal_only_when_every_occurrence_agrees() {
        assert_eq!(
            reconcile_occurrences([observation("101", &[2; 32]), observation("101", &[2; 32])]),
            Ok(2)
        );
        assert_eq!(
            reconcile_occurrences([observation("101", &[2; 32]), observation("999", &[2; 32])]),
            Err(Hold::SourceConflict)
        );
        assert_eq!(
            reconcile_occurrences([observation("101", &[2; 32]), observation("101", &[3; 32])]),
            Err(Hold::SourceConflict)
        );
        let mut bad = observation("101", &[2; 32]);
        bad.qualified = false;
        assert_eq!(
            reconcile_occurrences([observation("101", &[2; 32]), bad]),
            Err(Hold::SourceConflict)
        );
        assert_eq!(reconcile_occurrences([]), Err(Hold::SourceNotObserved));
    }
}

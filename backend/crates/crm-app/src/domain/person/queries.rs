//! `person`/`contact_method` persistence and the People / Person-detail read
//! models (docs/specs/SLICE_002.md §4, §5).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::contact::{normalize_email, normalize_phone};
use crate::domain::inquiry::parse::ParsedLead;
use crate::domain::person::filter::PersonFilterParams;
use crate::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crate::domain::person::sort::{PersonSort, SortDirection, SortKey};
use crate::domain::person::visibility::PersonVisibilityScope;
use crate::ids::{CorrelationId, OrganizationId, PersonId, StageId, UserId};

// --- Command-side helpers ------------------------------------------------

pub struct LockedPerson {
    pub id: PersonId,
    pub stage_id: StageId,
    pub assigned_user_id: Option<UserId>,
}

/// The direct `query_as!` decode target for `lock_person` — bare `Uuid`
/// per the sqlx strategy (private row-boundary struct; `LockedPerson`
/// itself carries the typed id).
struct LockedPersonRow {
    id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
}

/// `SELECT … FOR UPDATE` scoped to the Organization — used both by intake's
/// identify match and by the assign/stage commands' check-then-act
/// (docs/specs/SLICE_002.md §3, §4).
///
/// Deliberately left as a plain blocking row lock, not given the same
/// bounded try/backoff treatment as `receive_inquiry`'s per-Organization
/// advisory lock: unlike that lock (held by every concurrent intake for a
/// whole Organization, so a burst can queue arbitrarily many waiters
/// behind one connection-holding transaction each), this lock is scoped to
/// one specific Person row. It contends with at most one other
/// transaction at a time — another request racing to match/update the
/// *same* Person — and is held only across a handful of small statements,
/// not a whole Organization's backlog. Once a request holds the intake
/// advisory lock, it is in practice the sole intake writer for that
/// Organization, so this can only still contend with a concurrent manual
/// `assign_person`/`change_person_stage` on that exact Person — a narrow,
/// short-lived, non-cascading wait, not a pool-starvation vector.
pub async fn lock_person(
    conn: &mut PgConnection,
    person_id: PersonId,
    organization_id: OrganizationId,
) -> Result<Option<LockedPerson>, sqlx::Error> {
    let row = sqlx::query_as!(
        LockedPersonRow,
        r#"SELECT id, stage_id, assigned_user_id
           FROM person WHERE id = $1 AND organization_id = $2 FOR UPDATE"#,
        person_id.0,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| LockedPerson {
        id: PersonId::new(r.id),
        stage_id: StageId::new(r.stage_id),
        assigned_user_id: r.assigned_user_id.map(UserId::new),
    }))
}

pub async fn insert_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    first_name: Option<&str>,
    last_name: Option<&str>,
    stage_id: StageId,
    assigned_user_id: Option<UserId>,
) -> Result<PersonId, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO person (organization_id, first_name, last_name, stage_id, assigned_user_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id"#,
        organization_id.0,
        first_name,
        last_name,
        stage_id.0,
        assigned_user_id.map(|id| id.0),
    )
    .fetch_one(conn)
    .await?;
    Ok(PersonId::new(row.id))
}

pub async fn update_assignment(
    conn: &mut PgConnection,
    person_id: PersonId,
    organization_id: OrganizationId,
    assigned_user_id: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE person SET assigned_user_id = $3, updated_at = now()
           WHERE id = $1 AND organization_id = $2"#,
        person_id.0,
        organization_id.0,
        assigned_user_id.map(|id| id.0),
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_stage(
    conn: &mut PgConnection,
    person_id: PersonId,
    organization_id: OrganizationId,
    stage_id: StageId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE person SET stage_id = $3, updated_at = now()
           WHERE id = $1 AND organization_id = $2"#,
        person_id.0,
        organization_id.0,
        stage_id.0,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Used both to validate an explicit `assign_to_user_id` on intake and an
/// assignment command's target (docs/specs/SLICE_002.md §3, §6: identical
/// `invalid_assignee` for nonexistent and other-Organization users).
pub async fn is_organization_member(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// Inserts any of `parsed`'s normalizable contact methods that are not
/// already on `person_id` (docs/specs/SLICE_002.md §3: "add any payload
/// contact methods not already on that Person"). Safe to call for both a
/// brand-new Person and a matched one.
pub async fn upsert_contact_methods(
    conn: &mut PgConnection,
    person_id: PersonId,
    organization_id: OrganizationId,
    parsed: &ParsedLead,
) -> Result<(), sqlx::Error> {
    if let Some(normalized) = parsed.normalized_email() {
        let raw = parsed.raw_email.as_deref().unwrap_or(normalized.as_str());
        sqlx::query!(
            r#"INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
               VALUES ($1, $2, 'email', $3, $4)
               ON CONFLICT (person_id, kind, normalized_value) DO NOTHING"#,
            organization_id.0,
            person_id.0,
            raw,
            normalized.as_str(),
        )
        .execute(&mut *conn)
        .await?;
    }
    if let Some(normalized) = parsed.normalized_phone() {
        let raw = parsed.raw_phone.as_deref().unwrap_or(normalized.as_str());
        sqlx::query!(
            r#"INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
               VALUES ($1, $2, 'phone', $3, $4)
               ON CONFLICT (person_id, kind, normalized_value) DO NOTHING"#,
            organization_id.0,
            person_id.0,
            raw,
            normalized.as_str(),
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

// --- Read models -----------------------------------------------------

struct PersonSummaryRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    created_at: DateTime<Utc>,
    stage_id: Uuid,
    stage_name: String,
    assigned_user_id: Option<Uuid>,
    assigned_user_display_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
    inquiry_count: i64,
    last_inquiry_at: Option<DateTime<Utc>>,
}

impl From<PersonSummaryRow> for PersonSummary {
    fn from(row: PersonSummaryRow) -> Self {
        let display_name = compute_display_name(
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.primary_email.as_deref(),
            row.primary_phone.as_deref(),
        );
        let assigned_user = match (row.assigned_user_id, row.assigned_user_display_name) {
            (Some(id), Some(display_name)) => Some(UserRef {
                id: UserId::new(id),
                display_name,
            }),
            _ => None,
        };
        PersonSummary {
            id: PersonId::new(row.id),
            first_name: row.first_name,
            last_name: row.last_name,
            display_name,
            stage: StageRef {
                id: StageId::new(row.stage_id),
                name: row.stage_name,
            },
            assigned_user,
            primary_email: row.primary_email,
            primary_phone: row.primary_phone,
            inquiry_count: row.inquiry_count,
            last_inquiry_at: row.last_inquiry_at,
            created_at: row.created_at,
        }
    }
}

/// `GET /api/people`: `created_at DESC, id` order, capped at 500 with a
/// `truncated` flag (docs/specs/SLICE_002.md §5).
pub async fn list_summaries(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    let organization_id = scope.organization_id();
    let mut rows = sqlx::query_as!(
        PersonSummaryRow,
        r#"SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i WHERE i.person_id = p.id) as "inquiry_count!",
             (SELECT max(i.received_at) FROM inquiry i WHERE i.person_id = p.id) as "last_inquiry_at?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           WHERE p.organization_id = $1
           ORDER BY p.created_at DESC, p.id ASC
           LIMIT 501"#,
        organization_id.0,
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((
        rows.into_iter().map(PersonSummary::from).collect(),
        truncated,
    ))
}

/// `GET /api/people?filter=<...>` (docs/specs/SLICE_011a.md §4e): ONE
/// static `query_as!` string, the fixed matrix — every clause axis is a
/// NULL-guarded optional predicate over bound params, disabled by binding
/// NULL. Same projection, ordering, and cap as [`list_summaries`] (the
/// filter narrows rows, nothing else); every correlated subselect/LATERAL
/// carries `AND x.organization_id = p.organization_id`, unlike the
/// existing subselects above (recorded planner debt, untouched here). The
/// `organization_id` boundary itself stays literal text in the `WHERE`,
/// never behind a guard.
pub async fn filtered_summaries(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    filtered_summaries_with_reference_now(conn, scope, params, None).await
}

/// Test-only common-clock seam for Today source parity fixtures. The normal
/// query keeps PostgreSQL `now()` as its authoritative evaluation clock;
/// callers outside test support cannot provide a reference time.
#[cfg(feature = "test-support")]
pub async fn filtered_summaries_at(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
    reference_now: DateTime<Utc>,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    filtered_summaries_with_reference_now(conn, scope, params, Some(reference_now)).await
}

async fn filtered_summaries_with_reference_now(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
    reference_now: Option<DateTime<Utc>>,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    #[cfg(feature = "test-support")]
    let _frozen = crate::domain::person::filter_test_support::frozen_selected();
    #[cfg(feature = "test-support")]
    if let Some(adapter) = crate::domain::person::filter_test_support::frozen_adapter() {
        return adapter
            .summaries(conn, scope, params, PersonSort::DEFAULT, reference_now)
            .await;
    }
    #[cfg(feature = "test-support")]
    crate::domain::person::filter_test_support::live_adapter_executed(
        crate::domain::person::filter_test_support::FilterStatementFamily::SummaryCreatedDesc,
    );
    let organization_id = scope.organization_id();
    let mut rows = sqlx::query_file_as!(
        PersonSummaryRow,
        "src/domain/person/sql/filtered_summaries.sql",
        organization_id.0,
        params.stage_ids.as_deref(),
        params.assigned_user_ids.as_deref(),
        params.assigned_include_unassigned,
        params.sources.as_deref(),
        params.created_within_days,
        params.created_not_within_days,
        params.created_never,
        params.last_inquiry_within_days,
        params.last_inquiry_not_within_days,
        params.last_inquiry_never,
        params.last_contact_within_days,
        params.last_contact_not_within_days,
        params.last_contact_never,
        params.last_inbound_within_days,
        params.last_inbound_not_within_days,
        params.last_inbound_never,
        params.has_replied,
        params.has_phone,
        params.has_email,
        reference_now,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
        params.custom_slot(0).map(|slot| slot.field_id),
        params.custom_slot(0).map(|slot| slot.field_type),
        params.custom_slot(0).map(|slot| slot.operation),
        params
            .custom_slot(0)
            .and_then(|slot| slot.text_operand.as_deref()),
        params
            .custom_slot(0)
            .and_then(|slot| slot.number_min.as_deref()),
        params
            .custom_slot(0)
            .and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(0).and_then(|slot| slot.date_min),
        params.custom_slot(0).and_then(|slot| slot.date_max),
        params
            .custom_slot(0)
            .and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(1).map(|slot| slot.field_id),
        params.custom_slot(1).map(|slot| slot.field_type),
        params.custom_slot(1).map(|slot| slot.operation),
        params
            .custom_slot(1)
            .and_then(|slot| slot.text_operand.as_deref()),
        params
            .custom_slot(1)
            .and_then(|slot| slot.number_min.as_deref()),
        params
            .custom_slot(1)
            .and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(1).and_then(|slot| slot.date_min),
        params.custom_slot(1).and_then(|slot| slot.date_max),
        params
            .custom_slot(1)
            .and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(2).map(|slot| slot.field_id),
        params.custom_slot(2).map(|slot| slot.field_type),
        params.custom_slot(2).map(|slot| slot.operation),
        params
            .custom_slot(2)
            .and_then(|slot| slot.text_operand.as_deref()),
        params
            .custom_slot(2)
            .and_then(|slot| slot.number_min.as_deref()),
        params
            .custom_slot(2)
            .and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(2).and_then(|slot| slot.date_min),
        params.custom_slot(2).and_then(|slot| slot.date_max),
        params
            .custom_slot(2)
            .and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(3).map(|slot| slot.field_id),
        params.custom_slot(3).map(|slot| slot.field_type),
        params.custom_slot(3).map(|slot| slot.operation),
        params
            .custom_slot(3)
            .and_then(|slot| slot.text_operand.as_deref()),
        params
            .custom_slot(3)
            .and_then(|slot| slot.number_min.as_deref()),
        params
            .custom_slot(3)
            .and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(3).and_then(|slot| slot.date_min),
        params.custom_slot(3).and_then(|slot| slot.date_max),
        params
            .custom_slot(3)
            .and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(4).map(|slot| slot.field_id),
        params.custom_slot(4).map(|slot| slot.field_type),
        params.custom_slot(4).map(|slot| slot.operation),
        params
            .custom_slot(4)
            .and_then(|slot| slot.text_operand.as_deref()),
        params
            .custom_slot(4)
            .and_then(|slot| slot.number_min.as_deref()),
        params
            .custom_slot(4)
            .and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(4).and_then(|slot| slot.date_min),
        params.custom_slot(4).and_then(|slot| slot.date_max),
        params
            .custom_slot(4)
            .and_then(|slot| slot.option_ids.as_deref()),
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((
        rows.into_iter().map(PersonSummary::from).collect(),
        truncated,
    ))
}

/// `GET /api/people?sort=<...>` / a saved list's stored sort
/// (docs/specs/SLICE_011b_SORT.md §4): dispatches to one of the seven
/// static sorted statements, or — for [`PersonSort::DEFAULT`] (`created.desc`,
/// which [`PersonSort::normalized`] maps to `None`) — falls back to the
/// UNCHANGED [`filtered_summaries`] statement, so `sort=created.desc` stays
/// byte-identical to an absent sort (spec §6). A sort with no ad-hoc filter
/// still calls this with all-NULL clause parameters (`PersonFilterParams::
/// default()`), which 011a pins as equal to the full list (spec §4).
pub async fn filtered_summaries_sorted(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
    sort: PersonSort,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    #[cfg(feature = "test-support")]
    let _frozen = crate::domain::person::filter_test_support::frozen_selected();
    #[cfg(feature = "test-support")]
    if let Some(adapter) = crate::domain::person::filter_test_support::frozen_adapter() {
        return adapter.summaries(conn, scope, params, sort, None).await;
    }
    let Some(sort) = sort.normalized() else {
        return filtered_summaries(conn, scope, params).await;
    };
    #[cfg(feature = "test-support")]
    crate::domain::person::filter_test_support::live_adapter_executed(
        crate::domain::person::filter_test_support::FilterStatementFamily::summary(sort),
    );
    let organization_id = scope.organization_id();

    macro_rules! run_sorted {
        ($path:literal) => {
            sqlx::query_file_as!(
                PersonSummaryRow,
                $path,
                organization_id.0,
                params.stage_ids.as_deref(),
                params.assigned_user_ids.as_deref(),
                params.assigned_include_unassigned,
                params.sources.as_deref(),
                params.created_within_days,
                params.created_not_within_days,
                params.created_never,
                params.last_inquiry_within_days,
                params.last_inquiry_not_within_days,
                params.last_inquiry_never,
                params.last_contact_within_days,
                params.last_contact_not_within_days,
                params.last_contact_never,
                params.last_inbound_within_days,
                params.last_inbound_not_within_days,
                params.last_inbound_never,
                params.has_replied,
                params.has_phone,
                params.has_email,
                None::<DateTime<Utc>>,
                params.awaiting_response,
                params.client_replied_unanswered,
                params.awaiting_call_outcome,
                params.viewer_id,
                params.tag_ids_any.as_deref(),
                params.tag_ids_none.as_deref(),
                params.custom_slot(0).map(|slot| slot.field_id),
                params.custom_slot(0).map(|slot| slot.field_type),
                params.custom_slot(0).map(|slot| slot.operation),
                params
                    .custom_slot(0)
                    .and_then(|slot| slot.text_operand.as_deref()),
                params
                    .custom_slot(0)
                    .and_then(|slot| slot.number_min.as_deref()),
                params
                    .custom_slot(0)
                    .and_then(|slot| slot.number_max.as_deref()),
                params.custom_slot(0).and_then(|slot| slot.date_min),
                params.custom_slot(0).and_then(|slot| slot.date_max),
                params
                    .custom_slot(0)
                    .and_then(|slot| slot.option_ids.as_deref()),
                params.custom_slot(1).map(|slot| slot.field_id),
                params.custom_slot(1).map(|slot| slot.field_type),
                params.custom_slot(1).map(|slot| slot.operation),
                params
                    .custom_slot(1)
                    .and_then(|slot| slot.text_operand.as_deref()),
                params
                    .custom_slot(1)
                    .and_then(|slot| slot.number_min.as_deref()),
                params
                    .custom_slot(1)
                    .and_then(|slot| slot.number_max.as_deref()),
                params.custom_slot(1).and_then(|slot| slot.date_min),
                params.custom_slot(1).and_then(|slot| slot.date_max),
                params
                    .custom_slot(1)
                    .and_then(|slot| slot.option_ids.as_deref()),
                params.custom_slot(2).map(|slot| slot.field_id),
                params.custom_slot(2).map(|slot| slot.field_type),
                params.custom_slot(2).map(|slot| slot.operation),
                params
                    .custom_slot(2)
                    .and_then(|slot| slot.text_operand.as_deref()),
                params
                    .custom_slot(2)
                    .and_then(|slot| slot.number_min.as_deref()),
                params
                    .custom_slot(2)
                    .and_then(|slot| slot.number_max.as_deref()),
                params.custom_slot(2).and_then(|slot| slot.date_min),
                params.custom_slot(2).and_then(|slot| slot.date_max),
                params
                    .custom_slot(2)
                    .and_then(|slot| slot.option_ids.as_deref()),
                params.custom_slot(3).map(|slot| slot.field_id),
                params.custom_slot(3).map(|slot| slot.field_type),
                params.custom_slot(3).map(|slot| slot.operation),
                params
                    .custom_slot(3)
                    .and_then(|slot| slot.text_operand.as_deref()),
                params
                    .custom_slot(3)
                    .and_then(|slot| slot.number_min.as_deref()),
                params
                    .custom_slot(3)
                    .and_then(|slot| slot.number_max.as_deref()),
                params.custom_slot(3).and_then(|slot| slot.date_min),
                params.custom_slot(3).and_then(|slot| slot.date_max),
                params
                    .custom_slot(3)
                    .and_then(|slot| slot.option_ids.as_deref()),
                params.custom_slot(4).map(|slot| slot.field_id),
                params.custom_slot(4).map(|slot| slot.field_type),
                params.custom_slot(4).map(|slot| slot.operation),
                params
                    .custom_slot(4)
                    .and_then(|slot| slot.text_operand.as_deref()),
                params
                    .custom_slot(4)
                    .and_then(|slot| slot.number_min.as_deref()),
                params
                    .custom_slot(4)
                    .and_then(|slot| slot.number_max.as_deref()),
                params.custom_slot(4).and_then(|slot| slot.date_min),
                params.custom_slot(4).and_then(|slot| slot.date_max),
                params
                    .custom_slot(4)
                    .and_then(|slot| slot.option_ids.as_deref()),
            )
            .fetch_all(&mut *conn)
            .await?
        };
    }

    let mut rows = match (sort.key, sort.direction) {
        (SortKey::Created, SortDirection::Asc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_created_asc.sql")
        }
        (SortKey::Created, SortDirection::Desc) => {
            unreachable!("created.desc normalizes to None and is handled above")
        }
        (SortKey::Name, SortDirection::Asc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_name_asc.sql")
        }
        (SortKey::Name, SortDirection::Desc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_name_desc.sql")
        }
        (SortKey::Stage, SortDirection::Asc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_stage_asc.sql")
        }
        (SortKey::Stage, SortDirection::Desc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_stage_desc.sql")
        }
        (SortKey::Assignee, SortDirection::Asc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_assignee_asc.sql")
        }
        (SortKey::Assignee, SortDirection::Desc) => {
            run_sorted!("src/domain/person/sql/filtered_summaries_assignee_desc.sql")
        }
    };

    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((
        rows.into_iter().map(PersonSummary::from).collect(),
        truncated,
    ))
}

/// Capped saved-list membership count (Slice 011b). This is deliberately a
/// second fixed SQL matrix rather than a call to `filtered_summaries`: list
/// indexes need only IDs and must not build up to 500 complete summaries per
/// definition. Keep predicates in lockstep with `filtered_summaries`; the
/// bounded subquery makes 500+ honest without a potentially unbounded count.
pub async fn count_filtered_matches(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
) -> Result<(i64, bool), sqlx::Error> {
    #[cfg(feature = "test-support")]
    let _frozen = crate::domain::person::filter_test_support::frozen_selected();
    #[cfg(feature = "test-support")]
    if let Some(adapter) = crate::domain::person::filter_test_support::frozen_adapter() {
        return adapter.count(conn, scope, params).await;
    }
    #[cfg(feature = "test-support")]
    crate::domain::person::filter_test_support::live_adapter_executed(
        crate::domain::person::filter_test_support::FilterStatementFamily::Count,
    );
    let organization_id = scope.organization_id();
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!"
           FROM (
             SELECT p.id
             FROM person p
             JOIN stage s ON s.id = p.stage_id
             LEFT JOIN LATERAL (
                 SELECT i2.source
                 FROM inquiry i2
                 WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
                 ORDER BY i2.received_at DESC, i2.id DESC
                 LIMIT 1
             ) latest_src ON true
             WHERE p.organization_id = $1
               AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
               AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
                    OR ($4::boolean AND p.assigned_user_id IS NULL))
               AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
               AND ($6::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => $6))
               AND ($7::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => $7))
               AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
               AND ($9::int IS NULL
                    OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > now() - make_interval(days => $9))
               AND ($10::int IS NULL
                    OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= now() - make_interval(days => $10))
               AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
               AND ($12::int IS NULL
                    OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > now() - make_interval(days => $12))
               AND ($13::int IS NULL
                    OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= now() - make_interval(days => $13))
               AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
               AND ($15::int IS NULL
                    OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > now() - make_interval(days => $15))
               AND ($16::int IS NULL
                    OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= now() - make_interval(days => $16))
               AND ($17::boolean IS NULL OR (p.last_inbound_at IS NULL) = $17)
               AND ($18::boolean IS NULL OR (p.last_inbound_at IS NOT NULL) = $18)
               AND ($19::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM contact_method cm3
                     WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
                       AND cm3.kind = 'phone'
                   )) = $19)
               AND ($20::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM contact_method cm4
                     WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
                       AND cm4.kind = 'email'
                   )) = $20)
               -- docs/specs/SLICE_011d.md §2: same three derived predicates
               -- and identical positions as filtered_summaries.sql, offset
               -- by the absence of that statement's reference_now param.
               AND ($21::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM inquiry ia
                     WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
                       AND ia.received_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
                   )) = $21)
               AND ($22::boolean IS NULL OR (
                     p.last_inbound_at IS NOT NULL
                     AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
                     AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
                   ) = $22)
               AND ($23::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM call c
                     WHERE c.organization_id = p.organization_id
                       AND c.person_id = p.id
                       AND c.caller_user_id = $24
                       AND c.status IN ('ended', 'failed')
                       AND c.ended_at IS NOT NULL
                       AND EXISTS (
                           SELECT 1 FROM contact_attempted root
                           WHERE root.organization_id = c.organization_id
                             AND root.causation_id = c.id
                             AND root.corrects_id IS NULL
                             AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
                       )
                   )) = $23)
               -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags
               -- (none-of), same positions as filtered_summaries.sql.
               AND ($25::uuid[] IS NULL OR EXISTS (
                     SELECT 1 FROM person_tag pt
                     WHERE pt.organization_id = $1
                       AND pt.person_id = p.id AND pt.tag_id = ANY($25)))
               AND ($26::uuid[] IS NULL OR NOT EXISTS (
                     SELECT 1 FROM person_tag pt2
                     WHERE pt2.organization_id = $1
                       AND pt2.person_id = p.id AND pt2.tag_id = ANY($26)))
               AND ($27::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $27 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $28) AND (($29 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28)) OR ($29 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28)) OR ($28 = 'text' AND $29 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($30), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($28 = 'text' AND $29 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($30), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($28 = 'number' AND $29 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND ($31::text IS NULL OR v.number_value >= $31::numeric) AND ($32::text IS NULL OR v.number_value <= $32::numeric))) OR ($28 = 'number' AND $29 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND ($31::text IS NULL OR v.number_value >= $31::numeric) AND ($32::text IS NULL OR v.number_value <= $32::numeric))) OR ($28 = 'date' AND $29 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND ($33::date IS NULL OR v.date_value >= $33) AND ($34::date IS NULL OR v.date_value <= $34))) OR ($28 = 'date' AND $29 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND ($33::date IS NULL OR v.date_value >= $33) AND ($34::date IS NULL OR v.date_value <= $34))) OR ($28 = 'choice' AND $29 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND v.option_id = ANY($35))) OR ($28 = 'choice' AND $29 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $27 AND v.field_type = $28 AND v.option_id = ANY($35))))))
               AND ($36::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $36 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $37) AND (($38 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37)) OR ($38 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37)) OR ($37 = 'text' AND $38 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($39), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($37 = 'text' AND $38 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($39), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($37 = 'number' AND $38 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND ($40::text IS NULL OR v.number_value >= $40::numeric) AND ($41::text IS NULL OR v.number_value <= $41::numeric))) OR ($37 = 'number' AND $38 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND ($40::text IS NULL OR v.number_value >= $40::numeric) AND ($41::text IS NULL OR v.number_value <= $41::numeric))) OR ($37 = 'date' AND $38 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND ($42::date IS NULL OR v.date_value >= $42) AND ($43::date IS NULL OR v.date_value <= $43))) OR ($37 = 'date' AND $38 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND ($42::date IS NULL OR v.date_value >= $42) AND ($43::date IS NULL OR v.date_value <= $43))) OR ($37 = 'choice' AND $38 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND v.option_id = ANY($44))) OR ($37 = 'choice' AND $38 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $36 AND v.field_type = $37 AND v.option_id = ANY($44))))))
               AND ($45::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $45 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $46) AND (($47 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46)) OR ($47 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46)) OR ($46 = 'text' AND $47 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($48), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($46 = 'text' AND $47 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($48), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($46 = 'number' AND $47 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND ($49::text IS NULL OR v.number_value >= $49::numeric) AND ($50::text IS NULL OR v.number_value <= $50::numeric))) OR ($46 = 'number' AND $47 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND ($49::text IS NULL OR v.number_value >= $49::numeric) AND ($50::text IS NULL OR v.number_value <= $50::numeric))) OR ($46 = 'date' AND $47 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND ($51::date IS NULL OR v.date_value >= $51) AND ($52::date IS NULL OR v.date_value <= $52))) OR ($46 = 'date' AND $47 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND ($51::date IS NULL OR v.date_value >= $51) AND ($52::date IS NULL OR v.date_value <= $52))) OR ($46 = 'choice' AND $47 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND v.option_id = ANY($53))) OR ($46 = 'choice' AND $47 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $45 AND v.field_type = $46 AND v.option_id = ANY($53))))))
               AND ($54::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $54 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $55) AND (($56 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55)) OR ($56 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55)) OR ($55 = 'text' AND $56 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($57), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($55 = 'text' AND $56 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($57), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($55 = 'number' AND $56 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND ($58::text IS NULL OR v.number_value >= $58::numeric) AND ($59::text IS NULL OR v.number_value <= $59::numeric))) OR ($55 = 'number' AND $56 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND ($58::text IS NULL OR v.number_value >= $58::numeric) AND ($59::text IS NULL OR v.number_value <= $59::numeric))) OR ($55 = 'date' AND $56 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND ($60::date IS NULL OR v.date_value >= $60) AND ($61::date IS NULL OR v.date_value <= $61))) OR ($55 = 'date' AND $56 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND ($60::date IS NULL OR v.date_value >= $60) AND ($61::date IS NULL OR v.date_value <= $61))) OR ($55 = 'choice' AND $56 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND v.option_id = ANY($62))) OR ($55 = 'choice' AND $56 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $54 AND v.field_type = $55 AND v.option_id = ANY($62))))))
               AND ($63::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $63 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $64) AND (($65 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64)) OR ($65 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64)) OR ($64 = 'text' AND $65 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($66), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($64 = 'text' AND $65 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($66), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($64 = 'number' AND $65 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND ($67::text IS NULL OR v.number_value >= $67::numeric) AND ($68::text IS NULL OR v.number_value <= $68::numeric))) OR ($64 = 'number' AND $65 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND ($67::text IS NULL OR v.number_value >= $67::numeric) AND ($68::text IS NULL OR v.number_value <= $68::numeric))) OR ($64 = 'date' AND $65 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND ($69::date IS NULL OR v.date_value >= $69) AND ($70::date IS NULL OR v.date_value <= $70))) OR ($64 = 'date' AND $65 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND ($69::date IS NULL OR v.date_value >= $69) AND ($70::date IS NULL OR v.date_value <= $70))) OR ($64 = 'choice' AND $65 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND v.option_id = ANY($71))) OR ($64 = 'choice' AND $65 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $63 AND v.field_type = $64 AND v.option_id = ANY($71))))))
             LIMIT 501
           ) capped"#,
        organization_id.0,
        params.stage_ids.as_deref(),
        params.assigned_user_ids.as_deref(),
        params.assigned_include_unassigned,
        params.sources.as_deref(),
        params.created_within_days,
        params.created_not_within_days,
        params.created_never,
        params.last_inquiry_within_days,
        params.last_inquiry_not_within_days,
        params.last_inquiry_never,
        params.last_contact_within_days,
        params.last_contact_not_within_days,
        params.last_contact_never,
        params.last_inbound_within_days,
        params.last_inbound_not_within_days,
        params.last_inbound_never,
        params.has_replied,
        params.has_phone,
        params.has_email,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
        params.custom_slot(0).map(|slot| slot.field_id),
        params.custom_slot(0).map(|slot| slot.field_type),
        params.custom_slot(0).map(|slot| slot.operation),
        params.custom_slot(0).and_then(|slot| slot.text_operand.as_deref()),
        params.custom_slot(0).and_then(|slot| slot.number_min.as_deref()),
        params.custom_slot(0).and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(0).and_then(|slot| slot.date_min),
        params.custom_slot(0).and_then(|slot| slot.date_max),
        params.custom_slot(0).and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(1).map(|slot| slot.field_id),
        params.custom_slot(1).map(|slot| slot.field_type),
        params.custom_slot(1).map(|slot| slot.operation),
        params.custom_slot(1).and_then(|slot| slot.text_operand.as_deref()),
        params.custom_slot(1).and_then(|slot| slot.number_min.as_deref()),
        params.custom_slot(1).and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(1).and_then(|slot| slot.date_min),
        params.custom_slot(1).and_then(|slot| slot.date_max),
        params.custom_slot(1).and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(2).map(|slot| slot.field_id),
        params.custom_slot(2).map(|slot| slot.field_type),
        params.custom_slot(2).map(|slot| slot.operation),
        params.custom_slot(2).and_then(|slot| slot.text_operand.as_deref()),
        params.custom_slot(2).and_then(|slot| slot.number_min.as_deref()),
        params.custom_slot(2).and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(2).and_then(|slot| slot.date_min),
        params.custom_slot(2).and_then(|slot| slot.date_max),
        params.custom_slot(2).and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(3).map(|slot| slot.field_id),
        params.custom_slot(3).map(|slot| slot.field_type),
        params.custom_slot(3).map(|slot| slot.operation),
        params.custom_slot(3).and_then(|slot| slot.text_operand.as_deref()),
        params.custom_slot(3).and_then(|slot| slot.number_min.as_deref()),
        params.custom_slot(3).and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(3).and_then(|slot| slot.date_min),
        params.custom_slot(3).and_then(|slot| slot.date_max),
        params.custom_slot(3).and_then(|slot| slot.option_ids.as_deref()),
        params.custom_slot(4).map(|slot| slot.field_id),
        params.custom_slot(4).map(|slot| slot.field_type),
        params.custom_slot(4).map(|slot| slot.operation),
        params.custom_slot(4).and_then(|slot| slot.text_operand.as_deref()),
        params.custom_slot(4).and_then(|slot| slot.number_min.as_deref()),
        params.custom_slot(4).and_then(|slot| slot.number_max.as_deref()),
        params.custom_slot(4).and_then(|slot| slot.date_min),
        params.custom_slot(4).and_then(|slot| slot.date_max),
        params.custom_slot(4).and_then(|slot| slot.option_ids.as_deref()),
    )
    .fetch_one(conn)
    .await?;

    Ok((row.count.min(500), row.count > 500))
}

/// Escapes `%`, `_`, and `\` so a search term is matched literally by
/// `ILIKE ... ESCAPE '\'` (docs/specs/SLICE_005.md §2).
pub fn escape_like(term: &str) -> String {
    let mut out = String::with_capacity(term.len() + 4);
    for ch in term.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Operator `search_people` (docs/specs/SLICE_005.md §2; tool-only in this
/// slice — not `GET /api/people?q=`). Same projection and Organization
/// predicate as `list_summaries`. Matches a case-insensitive substring of
/// `first_name`, `last_name`, or `concat_ws(' ', first_name, last_name)`
/// (the term is LIKE-escaped), **or** an exact `contact_method.
/// normalized_value` when the term normalizes as an email or phone.
/// Ordered `last_name, first_name, id`; `limit + 1` rows are fetched so
/// `truncated` is exact.
pub async fn search_summaries(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    term: &str,
    limit: i64,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    let organization_id = scope.organization_id();
    let pattern = format!("%{}%", escape_like(term.trim()));
    let normalized_email = normalize_email(term);
    let normalized_phone = normalize_phone(term);
    let fetch = limit.max(0) + 1;

    let mut rows = sqlx::query_as!(
        PersonSummaryRow,
        r#"SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i WHERE i.person_id = p.id) as "inquiry_count!",
             (SELECT max(i.received_at) FROM inquiry i WHERE i.person_id = p.id) as "last_inquiry_at?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           WHERE p.organization_id = $1
             AND (
               p.first_name ILIKE $2 ESCAPE '\'
               OR p.last_name ILIKE $2 ESCAPE '\'
               OR concat_ws(' ', p.first_name, p.last_name) ILIKE $2 ESCAPE '\'
               OR EXISTS (
                 SELECT 1 FROM contact_method cm2
                 WHERE cm2.person_id = p.id
                   AND cm2.organization_id = p.organization_id
                   AND (
                     (cm2.kind = 'email' AND cm2.normalized_value = $3)
                     OR (cm2.kind = 'phone' AND cm2.normalized_value = $4)
                   )
               )
             )
           ORDER BY p.last_name ASC NULLS LAST, p.first_name ASC NULLS LAST, p.id ASC
           LIMIT $5"#,
        organization_id.0,
        pattern,
        normalized_email.as_ref().map(|e| e.as_str()),
        normalized_phone.as_ref().map(|p| p.as_str()),
        fetch,
    )
    .fetch_all(conn)
    .await?;

    let limit = usize::try_from(limit.max(0)).unwrap_or(0);
    let truncated = rows.len() > limit;
    rows.truncate(limit);
    Ok((
        rows.into_iter().map(PersonSummary::from).collect(),
        truncated,
    ))
}

/// A single Person's summary, scoped to the Organization
/// (docs/specs/SLICE_002.md §5: the assignment/stage command responses).
pub async fn summary_by_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Option<PersonSummary>, sqlx::Error> {
    let row = sqlx::query_as!(
        PersonSummaryRow,
        r#"SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i WHERE i.person_id = p.id) as "inquiry_count!",
             (SELECT max(i.received_at) FROM inquiry i WHERE i.person_id = p.id) as "last_inquiry_at?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           WHERE p.organization_id = $1 AND p.id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(PersonSummary::from))
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactMethodItem {
    pub id: Uuid,
    pub kind: String,
    pub value: String,
}

/// `contact_methods` in `GET /api/people/{id}`, by `created_at`
/// (docs/specs/SLICE_002.md §5).
pub async fn contact_methods_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<ContactMethodItem>, sqlx::Error> {
    sqlx::query_as!(
        ContactMethodItem,
        r#"SELECT id, kind, value FROM contact_method
           WHERE organization_id = $1 AND person_id = $2
           ORDER BY created_at ASC"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await
}

// --- History ---------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub kind: &'static str,
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub actor: Option<UserRef>,
    pub origin: String,
    pub correlation_id: CorrelationId,
    pub detail: serde_json::Value,
    /// `inquiry_received` = 0 … `stage_changed` = 3
    /// (docs/specs/SLICE_002.md §5), `contact_attempted` = 4 (SLICE_003),
    /// `call_completed` = 5 (SLICE_006). Not part of the response shape.
    #[serde(skip)]
    pub kind_rank: u8,
}

fn actor_ref(user_id: Option<Uuid>, display_name: Option<String>) -> Option<UserRef> {
    match (user_id, display_name) {
        (Some(id), Some(display_name)) => Some(UserRef {
            id: UserId::new(id),
            display_name,
        }),
        _ => None,
    }
}

struct InquiryReceivedHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    inquiry_id: Uuid,
    source: String,
    person_created: bool,
    matched_by: Option<String>,
}

async fn inquiry_received_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        InquiryReceivedHistoryRow,
        r#"SELECT ir.id, ir.occurred_at, ir.recorded_at, ir.origin, ir.correlation_id,
                  ir.actor_user_id, au.display_name as "actor_display_name?",
                  ir.inquiry_id, ir.source, ir.person_created, ir.matched_by
           FROM inquiry_received ir
           LEFT JOIN app_user au ON au.id = ir.actor_user_id
           WHERE ir.organization_id = $1 AND ir.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            kind: "inquiry_received",
            kind_rank: 0,
            id: r.id,
            occurred_at: r.occurred_at,
            recorded_at: r.recorded_at,
            actor: actor_ref(r.actor_user_id, r.actor_display_name),
            origin: r.origin,
            correlation_id: CorrelationId::new(r.correlation_id),
            detail: serde_json::json!({
                "inquiry_id": r.inquiry_id,
                "source": r.source,
                "person_created": r.person_created,
                "matched_by": r.matched_by,
            }),
        })
        .collect())
}

struct RoutingDecisionHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    inquiry_id: Uuid,
    strategy: String,
    assignee_user_id: Option<Uuid>,
    assignee_display_name: Option<String>,
}

async fn routing_decision_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        RoutingDecisionHistoryRow,
        r#"SELECT rd.id, rd.occurred_at, rd.recorded_at, rd.origin, rd.correlation_id,
                  rd.actor_user_id, au.display_name as "actor_display_name?",
                  rd.inquiry_id, rd.strategy,
                  rd.assignee_user_id, au2.display_name as "assignee_display_name?"
           FROM routing_decision rd
           LEFT JOIN app_user au ON au.id = rd.actor_user_id
           LEFT JOIN app_user au2 ON au2.id = rd.assignee_user_id
           WHERE rd.organization_id = $1 AND rd.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let assignee = actor_ref(r.assignee_user_id, r.assignee_display_name);
            HistoryEntry {
                kind: "routing_decision",
                kind_rank: 1,
                id: r.id,
                occurred_at: r.occurred_at,
                recorded_at: r.recorded_at,
                actor: actor_ref(r.actor_user_id, r.actor_display_name),
                origin: r.origin,
                correlation_id: CorrelationId::new(r.correlation_id),
                detail: serde_json::json!({
                    "inquiry_id": r.inquiry_id,
                    "strategy": r.strategy,
                    "assignee": assignee,
                }),
            }
        })
        .collect())
}

struct AssignmentChangedHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    from_user_id: Option<Uuid>,
    from_display_name: Option<String>,
    to_user_id: Option<Uuid>,
    to_display_name: Option<String>,
    reason: String,
}

async fn assignment_changed_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        AssignmentChangedHistoryRow,
        r#"SELECT ac.id, ac.occurred_at, ac.recorded_at, ac.origin, ac.correlation_id,
                  ac.actor_user_id, au.display_name as "actor_display_name?",
                  ac.from_user_id, fu.display_name as "from_display_name?",
                  ac.to_user_id, tu.display_name as "to_display_name?",
                  ac.reason
           FROM assignment_changed ac
           LEFT JOIN app_user au ON au.id = ac.actor_user_id
           LEFT JOIN app_user fu ON fu.id = ac.from_user_id
           LEFT JOIN app_user tu ON tu.id = ac.to_user_id
           WHERE ac.organization_id = $1 AND ac.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let from = actor_ref(r.from_user_id, r.from_display_name);
            let to = actor_ref(r.to_user_id, r.to_display_name);
            HistoryEntry {
                kind: "assignment_changed",
                kind_rank: 2,
                id: r.id,
                occurred_at: r.occurred_at,
                recorded_at: r.recorded_at,
                actor: actor_ref(r.actor_user_id, r.actor_display_name),
                origin: r.origin,
                correlation_id: CorrelationId::new(r.correlation_id),
                detail: serde_json::json!({
                    "from": from,
                    "to": to,
                    "reason": r.reason,
                }),
            }
        })
        .collect())
}

struct StageChangedHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    from_stage_id: Option<Uuid>,
    from_stage_name: Option<String>,
    to_stage_id: Uuid,
    to_stage_name: String,
    reason: String,
}

async fn stage_changed_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        StageChangedHistoryRow,
        r#"SELECT sc.id, sc.occurred_at, sc.recorded_at, sc.origin, sc.correlation_id,
                  sc.actor_user_id, au.display_name as "actor_display_name?",
                  sc.from_stage_id, fs.name as "from_stage_name?",
                  sc.to_stage_id, ts.name as "to_stage_name!",
                  sc.reason
           FROM stage_changed sc
           LEFT JOIN app_user au ON au.id = sc.actor_user_id
           LEFT JOIN stage fs ON fs.id = sc.from_stage_id
           JOIN stage ts ON ts.id = sc.to_stage_id
           WHERE sc.organization_id = $1 AND sc.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let from_stage = match (r.from_stage_id, r.from_stage_name) {
                (Some(id), Some(name)) => Some(StageRef {
                    id: StageId::new(id),
                    name,
                }),
                _ => None,
            };
            let to_stage = StageRef {
                id: StageId::new(r.to_stage_id),
                name: r.to_stage_name,
            };
            HistoryEntry {
                kind: "stage_changed",
                kind_rank: 3,
                id: r.id,
                occurred_at: r.occurred_at,
                recorded_at: r.recorded_at,
                actor: actor_ref(r.actor_user_id, r.actor_display_name),
                origin: r.origin,
                correlation_id: CorrelationId::new(r.correlation_id),
                detail: serde_json::json!({
                    "from_stage": from_stage,
                    "to_stage": to_stage,
                    "reason": r.reason,
                }),
            }
        })
        .collect())
}

struct ContactAttemptedHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    channel: String,
    outcome: String,
    call_id: Option<Uuid>,
    corrects_id: Option<Uuid>,
    superseded: bool,
}

/// `contact_attempted` history entries (docs/specs/SLICE_003.md §5, the
/// declared additive SLICE_002 §5 contract change): `kind_rank` 4,
/// `detail: {"channel", "outcome", "call_id", "corrects_id", "superseded"}`
/// — the last three added by docs/specs/SLICE_006c.md §2. `call_id` is the
/// row's `causation_id` when it names a `call` of this Organization
/// (call-derived attempts and their corrections); a manual attempt has
/// `causation_id` NULL and so `call_id: null`. `superseded` = a corrector
/// exists. A correction's history `occurred_at` is its `recorded_at` — the
/// moment the agent corrected it — so the timeline reads "call, then the
/// correction" (user decision 2026-08-23, SLICE_006c §2); the stored fact
/// keeps the inherited `occurred_at` for Today.
async fn contact_attempted_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ContactAttemptedHistoryRow,
        r#"SELECT ca.id, ca.occurred_at, ca.recorded_at, ca.origin, ca.correlation_id,
                  ca.actor_user_id, au.display_name as "actor_display_name?",
                  ca.channel, ca.outcome,
                  cl.id as "call_id?", ca.corrects_id,
                  EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id = ca.id)
                      as "superseded!"
           FROM contact_attempted ca
           LEFT JOIN app_user au ON au.id = ca.actor_user_id
           LEFT JOIN call cl ON cl.id = ca.causation_id AND cl.organization_id = ca.organization_id
           WHERE ca.organization_id = $1 AND ca.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            kind: "contact_attempted",
            kind_rank: 4,
            id: r.id,
            occurred_at: if r.corrects_id.is_some() {
                r.recorded_at
            } else {
                r.occurred_at
            },
            recorded_at: r.recorded_at,
            actor: actor_ref(r.actor_user_id, r.actor_display_name),
            origin: r.origin,
            correlation_id: CorrelationId::new(r.correlation_id),
            detail: serde_json::json!({
                "channel": r.channel,
                "outcome": r.outcome,
                "call_id": r.call_id,
                "corrects_id": r.corrects_id,
                "superseded": r.superseded,
            }),
        })
        .collect())
}

struct CallCompletedHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_display_name: Option<String>,
    call_id: Uuid,
    outcome: String,
    talk_seconds: Option<i32>,
    answered_at: Option<DateTime<Utc>>,
}

/// `call_completed` history entries (docs/specs/SLICE_006.md §2, the
/// declared additive SLICE_002 §5 contract change): `kind_rank` 5,
/// `detail: {"call_id", "outcome", "talk_seconds", "answered_at"}`.
/// PII-free by construction (the fact table holds no number); reads
/// with telephony disabled.
async fn call_completed_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        CallCompletedHistoryRow,
        r#"SELECT cc.id, cc.occurred_at, cc.recorded_at, cc.origin, cc.correlation_id,
                  cc.actor_user_id, au.display_name as "actor_display_name?",
                  cc.call_id, cc.outcome, cc.talk_seconds, cc.answered_at
           FROM call_completed cc
           LEFT JOIN app_user au ON au.id = cc.actor_user_id
           WHERE cc.organization_id = $1 AND cc.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            kind: "call_completed",
            kind_rank: 5,
            id: r.id,
            occurred_at: r.occurred_at,
            recorded_at: r.recorded_at,
            actor: actor_ref(r.actor_user_id, r.actor_display_name),
            origin: r.origin,
            correlation_id: CorrelationId::new(r.correlation_id),
            detail: serde_json::json!({
                "call_id": r.call_id,
                "outcome": r.outcome,
                "talk_seconds": r.talk_seconds,
                "answered_at": r.answered_at,
            }),
        })
        .collect())
}

struct CorrespondenceHistoryRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    on_behalf_of_user_id: Option<Uuid>,
    agent_display_name: Option<String>,
    direction: String,
    via: String,
    backdated: bool,
}

/// `correspondence` history entries (Slice 009, docs/specs/SLICE_009.md
/// §8, the declared additive SLICE_002 §5 contract change): `kind_rank`
/// 6, `detail: {"direction", "agent", "captured_at", "via", "backdated"}`
/// — deliberately NO address/subject/message-id (D-042.1/2). Org-wide
/// visible (D-042.1): no `agent_user_id`/attribution filter here, unlike
/// the held-queue reads in `domain/capture/store.rs`.
///
/// The top-level `actor` field is always `None` for these rows (`Actor::
/// System` per spec §4 — the agent's CC/forward caused an UNATTENDED
/// capture, so there is no human `actor_user_id`); the attributed agent
/// instead lives in `detail.agent`, read from `on_behalf_of_user_id`
/// (spec §4: "on_behalf_of_user_id = agent … agent_user_id is the queried
/// attribution column, mirrored into on_behalf_of") via the SAME
/// `actor_ref` shape every other history kind's actor uses. Always
/// `Some` in practice (the capture pipeline never omits it) — a `None`
/// here is a data-integrity surprise, so this fails closed
/// (`sqlx::Error::Decode`) rather than silently rendering a missing
/// agent.
async fn correspondence_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as!(
        CorrespondenceHistoryRow,
        r#"SELECT cc.id, cc.occurred_at, cc.recorded_at, cc.origin, cc.correlation_id,
                  cc.on_behalf_of_user_id, au.display_name as "agent_display_name?",
                  cc.direction, cc.via, cc.backdated
           FROM correspondence_captured cc
           LEFT JOIN app_user au ON au.id = cc.on_behalf_of_user_id
           WHERE cc.organization_id = $1 AND cc.person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;

    rows.into_iter()
        .map(|r| {
            let agent =
                actor_ref(r.on_behalf_of_user_id, r.agent_display_name).ok_or_else(|| {
                    sqlx::Error::Decode(
                        "correspondence_captured: on_behalf_of_user_id must always be set".into(),
                    )
                })?;
            Ok(HistoryEntry {
                kind: "correspondence",
                kind_rank: 6,
                id: r.id,
                occurred_at: r.occurred_at,
                recorded_at: r.recorded_at,
                actor: None,
                origin: r.origin,
                correlation_id: CorrelationId::new(r.correlation_id),
                detail: serde_json::json!({
                    "direction": r.direction,
                    "agent": agent,
                    "captured_at": r.recorded_at,
                    "via": r.via,
                    "backdated": r.backdated,
                }),
            })
        })
        .collect()
}

/// `note` history entries (Slice 015, docs/specs/SLICE_015.md §5): kind_rank
/// 7, `detail: {"body","updated_at","edited","can_manage"}` —
/// `can_manage` is emitted `false` here unconditionally; the people
/// detail route, which knows the viewer's role and id, overwrites it for
/// `note` entries (the tags-route pattern). `occurred_at`/`recorded_at`
/// are both the note's `created_at`: an edit does not move the timeline
/// position (spec §1 rule 4). The raw row read itself lives in
/// `domain::note::queries` (the note module owns its own table); this
/// function only shapes it into a `HistoryEntry`.
async fn note_history_entries(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows = crate::domain::note::note_history(conn, organization_id, person_id).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let edited = r.updated_at > r.created_at;
            HistoryEntry {
                kind: "note",
                kind_rank: 7,
                id: r.id,
                occurred_at: r.created_at,
                recorded_at: r.created_at,
                actor: r.actor,
                origin: r.origin,
                correlation_id: CorrelationId::new(r.correlation_id),
                detail: serde_json::json!({
                    "body": r.body,
                    "updated_at": r.updated_at,
                    "edited": edited,
                    "can_manage": false,
                }),
            }
        })
        .collect())
}

/// `task_completed` history entries (Slice 016a, docs/specs/SLICE_016.md
/// §1 rule 5, §4): kind_rank 8, `detail: {"title","kind","due_at",
/// "assignee","created_by","can_manage"}` — `can_manage` is emitted
/// `false` here unconditionally; the people detail route, which knows the
/// viewer's role and id, overwrites it for `task_completed` entries (the
/// tags/note-route pattern). `occurred_at`/`recorded_at` are both the
/// task's `completed_at`: reopening removes the entry, and completing
/// again re-adds it at the new time. The raw row read itself lives in
/// `domain::task::queries` (the task module owns its own table); this
/// function only shapes it into a `HistoryEntry`.
async fn task_completed_history_entries(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let rows =
        crate::domain::task::task_completed_history(conn, organization_id, person_id).await?;
    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            kind: "task_completed",
            kind_rank: 8,
            id: r.id,
            occurred_at: r.completed_at,
            recorded_at: r.completed_at,
            actor: r.completed_by,
            origin: r.origin,
            correlation_id: CorrelationId::new(r.correlation_id),
            detail: serde_json::json!({
                "title": r.title,
                "kind": r.kind,
                "due_at": r.due_at,
                "assignee": r.assignee,
                "created_by": r.created_by,
                "can_manage": false,
            }),
        })
        .collect())
}

/// The full history timeline for `GET /api/people/{id}`, ordered
/// `occurred_at, recorded_at, kind_rank, id` (docs/specs/SLICE_002.md §5:
/// required because intake's four facts otherwise share both timestamps).
pub async fn history_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let mut entries = Vec::new();
    entries.extend(inquiry_received_history(conn, organization_id, person_id).await?);
    entries.extend(routing_decision_history(conn, organization_id, person_id).await?);
    entries.extend(assignment_changed_history(conn, organization_id, person_id).await?);
    entries.extend(stage_changed_history(conn, organization_id, person_id).await?);
    entries.extend(contact_attempted_history(conn, organization_id, person_id).await?);
    entries.extend(call_completed_history(conn, organization_id, person_id).await?);
    entries.extend(correspondence_history(conn, organization_id, person_id).await?);
    entries.extend(note_history_entries(conn, organization_id, person_id).await?);
    entries.extend(task_completed_history_entries(conn, organization_id, person_id).await?);

    entries.sort_by_key(|e| (e.occurred_at, e.recorded_at, e.kind_rank, e.id));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::escape_like;

    #[test]
    fn escape_like_escapes_wildcards_and_backslash() {
        assert_eq!(escape_like("100%_done\\"), "100\\%\\_done\\\\");
        assert_eq!(escape_like("grace"), "grace");
    }
}

/// Service-free text-parity check (docs/specs/SLICE_011b_SORT.md §4, §11.2):
/// every sorted statement, with its top-level `ORDER BY` line (the one
/// immediately preceding `LIMIT 501`) removed, is textually identical to
/// the canonical `filtered_summaries.sql` after per-line trimming. Other
/// `ORDER BY` lines inside subselects/LATERAL probes are left untouched by
/// this strip and so must match verbatim between every file.
#[cfg(test)]
mod sort_sql_parity_tests {
    const CANONICAL: &str = include_str!("sql/filtered_summaries.sql");
    const CREATED_ASC: &str = include_str!("sql/filtered_summaries_created_asc.sql");
    const NAME_ASC: &str = include_str!("sql/filtered_summaries_name_asc.sql");
    const NAME_DESC: &str = include_str!("sql/filtered_summaries_name_desc.sql");
    const STAGE_ASC: &str = include_str!("sql/filtered_summaries_stage_asc.sql");
    const STAGE_DESC: &str = include_str!("sql/filtered_summaries_stage_desc.sql");
    const ASSIGNEE_ASC: &str = include_str!("sql/filtered_summaries_assignee_asc.sql");
    const ASSIGNEE_DESC: &str = include_str!("sql/filtered_summaries_assignee_desc.sql");

    /// Removes exactly the top-level `ORDER BY ...` line (the one
    /// immediately preceding the statement's `LIMIT 501`), returning every
    /// other line unchanged and in order.
    fn without_top_level_order_by(sql: &str) -> Vec<&str> {
        let lines: Vec<&str> = sql.lines().collect();
        let limit_idx = lines
            .iter()
            .position(|line| line.trim() == "LIMIT 501")
            .expect("statement must end with a top-level LIMIT 501");
        assert!(limit_idx > 0, "LIMIT 501 must not be the first line");
        let order_idx = limit_idx - 1;
        assert!(
            lines[order_idx].trim_start().starts_with("ORDER BY"),
            "the line immediately preceding LIMIT 501 must be the top-level ORDER BY"
        );
        let mut out = lines;
        out.remove(order_idx);
        out
    }

    fn trimmed_lines(sql: &str) -> Vec<String> {
        without_top_level_order_by(sql)
            .into_iter()
            .map(|line| line.trim().to_string())
            .collect()
    }

    #[test]
    fn every_sorted_statement_matches_the_canonical_text_apart_from_its_order_by() {
        let canonical = trimmed_lines(CANONICAL);
        for (name, text) in [
            ("created.asc", CREATED_ASC),
            ("name.asc", NAME_ASC),
            ("name.desc", NAME_DESC),
            ("stage.asc", STAGE_ASC),
            ("stage.desc", STAGE_DESC),
            ("assignee.asc", ASSIGNEE_ASC),
            ("assignee.desc", ASSIGNEE_DESC),
        ] {
            assert_eq!(
                trimmed_lines(text),
                canonical,
                "{name}'s body (minus its ORDER BY line) must match the canonical text"
            );
        }
    }

    /// Pins each file's own top-level `ORDER BY` line to the exact text
    /// spec §4 declares — the one permitted difference from canonical.
    #[test]
    fn each_sorted_statement_has_its_declared_order_by_line() {
        let expected: [(&str, &str); 7] = [
            (CREATED_ASC, "ORDER BY p.created_at ASC, p.id ASC"),
            (
                NAME_ASC,
                "ORDER BY p.last_name ASC  NULLS LAST, p.first_name ASC  NULLS LAST, p.created_at DESC, p.id ASC",
            ),
            (
                NAME_DESC,
                "ORDER BY p.last_name DESC NULLS LAST, p.first_name DESC NULLS LAST, p.created_at DESC, p.id ASC",
            ),
            (STAGE_ASC, "ORDER BY s.position ASC,  p.created_at DESC, p.id ASC"),
            (STAGE_DESC, "ORDER BY s.position DESC, p.created_at DESC, p.id ASC"),
            (
                ASSIGNEE_ASC,
                "ORDER BY u.display_name ASC  NULLS LAST, p.created_at DESC, p.id ASC",
            ),
            (
                ASSIGNEE_DESC,
                "ORDER BY u.display_name DESC NULLS LAST, p.created_at DESC, p.id ASC",
            ),
        ];
        for (text, expected_order_by) in expected {
            let lines: Vec<&str> = text.lines().collect();
            let limit_idx = lines
                .iter()
                .position(|line| line.trim() == "LIMIT 501")
                .unwrap();
            assert_eq!(lines[limit_idx - 1].trim(), expected_order_by);
        }
        // The canonical file's own (unsorted-request) ORDER BY is unchanged.
        let canonical_lines: Vec<&str> = CANONICAL.lines().collect();
        let limit_idx = canonical_lines
            .iter()
            .position(|line| line.trim() == "LIMIT 501")
            .unwrap();
        assert_eq!(
            canonical_lines[limit_idx - 1].trim(),
            "ORDER BY p.created_at DESC, p.id ASC"
        );
    }
}

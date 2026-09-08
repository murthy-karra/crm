//! Per-actor saved-list configuration for Today (Slice 011c). A source is a
//! reference to a live definition, never a copied filter or a materialized
//! membership set. The public read shape deliberately omits the definition,
//! creator and Organization identifiers.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::auth::AuthContext;
use crate::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crate::domain::envelope::CommandContext;
use crate::domain::person::filter::PersonFilterParams;
use crate::domain::person::filter::{FilterDefinition, FilterError};
use crate::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crate::domain::saved_list::{
    self, validate_expected_revision, SavedListError, SavedListFilterError, SavedListScope,
};
use crate::domain::today::InquiryRef;
use crate::ids::{InquiryId, OrganizationId, PersonId, SavedListId, StageId, UserId};

pub const TODAY_SOURCE_LIMIT: i64 = 5;

#[derive(Debug, Clone, Serialize)]
pub struct TodaySource {
    pub list_id: SavedListId,
    pub name: String,
    pub scope: SavedListScope,
    pub revision: i64,
    pub filter_error: Option<SavedListFilterError>,
}

#[derive(Debug, Clone)]
pub(crate) struct EvaluatedTodaySource {
    pub source: TodaySource,
    pub filter: Option<FilterDefinition>,
}

/// A hydrated, source-only candidate. The query returns no more than the
/// caller's requested bound and uses the same fixed v1 predicate matrix as
/// People; it never reads an Organization's full membership into Rust.
///
/// `pub`, not `pub(crate)` (Slice 012, docs/specs/SLICE_012.md §4): the
/// frozen-vs-live statement equivalence gate in the `crm-api` crate
/// (`tests/db_statement_equivalence.rs`) needs to call
/// [`source_candidates`] directly, from outside this crate — a visibility
/// widening only, no behavior or signature change.
#[derive(Debug, Clone)]
pub struct SourceCandidate {
    pub person: PersonSummary,
    pub latest_inquiry: Option<InquiryRef>,
    pub last_contact_attempt: Option<ContactAttemptRef>,
    pub last_contact_at: Option<DateTime<Utc>>,
}

fn decode_contact(
    id: Option<uuid::Uuid>,
    channel: Option<String>,
    outcome: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
) -> Result<Option<ContactAttemptRef>, sqlx::Error> {
    match (id, channel, outcome, occurred_at) {
        (Some(id), Some(channel), Some(outcome), Some(occurred_at)) => {
            let channel = ContactChannel::decode(&channel).ok_or_else(|| {
                sqlx::Error::Decode("today source candidate has an invalid contact channel".into())
            })?;
            let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                sqlx::Error::Decode("today source candidate has an invalid contact outcome".into())
            })?;
            Ok(Some(ContactAttemptRef {
                id,
                channel,
                outcome,
                occurred_at,
            }))
        }
        (None, None, None, None) => Ok(None),
        _ => Err(sqlx::Error::Decode(
            "today source candidate contact columns must be all null or all set".into(),
        )),
    }
}

/// SQLx-checked projection for `source_candidates.sql`. The file selects the
/// ordered `K + 1` prefix before it performs detailed inquiry/contact
/// hydration; keep its column names in lock-step with this row type.
#[derive(Debug)]
struct SourceCandidateRow {
    id: uuid::Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    created_at: DateTime<Utc>,
    stage_id: uuid::Uuid,
    stage_name: String,
    assigned_user_id: Option<uuid::Uuid>,
    assigned_user_display_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
    inquiry_count: i64,
    latest_inquiry_id: Option<uuid::Uuid>,
    latest_inquiry_source: Option<String>,
    latest_inquiry_received_at: Option<DateTime<Utc>>,
    last_attempt_id: Option<uuid::Uuid>,
    last_attempt_channel: Option<String>,
    last_attempt_outcome: Option<String>,
    last_attempt_occurred_at: Option<DateTime<Utc>>,
    last_contact_at: Option<DateTime<Utc>>,
}

/// `pub` (Slice 012, docs/specs/SLICE_012.md §4): visibility widening only,
/// for the same reason as [`SourceCandidate`] above.
pub async fn source_candidates(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    params: &PersonFilterParams,
    now: DateTime<Utc>,
    builtin_ids: &[uuid::Uuid],
    members_only: bool,
    limit: i64,
) -> Result<Vec<SourceCandidate>, sqlx::Error> {
    let rows = sqlx::query_file_as!(
        SourceCandidateRow,
        "src/domain/today/source_candidates.sql",
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
        now,
        builtin_ids,
        members_only,
        limit,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter()
        .map(|row| {
            let id = row.id;
            let first_name = row.first_name;
            let last_name = row.last_name;
            let primary_email = row.primary_email;
            let primary_phone = row.primary_phone;
            let assigned_id = row.assigned_user_id;
            let assigned_name = row.assigned_user_display_name;
            let person = PersonSummary {
                id: PersonId::new(id),
                first_name: first_name.clone(),
                last_name: last_name.clone(),
                display_name: compute_display_name(
                    first_name.as_deref(),
                    last_name.as_deref(),
                    primary_email.as_deref(),
                    primary_phone.as_deref(),
                ),
                stage: StageRef {
                    id: StageId::new(row.stage_id),
                    name: row.stage_name,
                },
                assigned_user: match (assigned_id, assigned_name) {
                    (Some(id), Some(display_name)) => Some(UserRef {
                        id: UserId::new(id),
                        display_name,
                    }),
                    _ => None,
                },
                primary_email,
                primary_phone,
                inquiry_count: row.inquiry_count,
                last_inquiry_at: row.latest_inquiry_received_at,
                created_at: row.created_at,
            };
            let latest_inquiry = match (
                row.latest_inquiry_id,
                row.latest_inquiry_source,
                row.latest_inquiry_received_at,
            ) {
                (Some(id), Some(source), Some(received_at)) => Some(InquiryRef {
                    id: InquiryId::new(id),
                    source,
                    received_at,
                }),
                (None, None, None) => None,
                _ => {
                    return Err(sqlx::Error::Decode(
                        "today source candidate latest inquiry columns must agree".into(),
                    ))
                }
            };
            Ok(SourceCandidate {
                person,
                latest_inquiry,
                last_contact_attempt: decode_contact(
                    row.last_attempt_id,
                    row.last_attempt_channel,
                    row.last_attempt_outcome,
                    row.last_attempt_occurred_at,
                )?,
                last_contact_at: row.last_contact_at,
            })
        })
        .collect()
}

/// Matches the retained built-in ID set against one source without hydrating
/// those People again. Built-in payloads are already captured in this
/// snapshot, so this preserves the measured B-membership shape and keeps the
/// whole-source budget for actual non-B candidates.
/// `pub` (Slice 012, docs/specs/SLICE_012.md §4): visibility widening only,
/// for the same reason as [`SourceCandidate`] above.
pub async fn source_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    params: &PersonFilterParams,
    now: DateTime<Utc>,
    builtin_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    let rows = sqlx::query_file!(
        "src/domain/today/source_membership.sql",
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
        now,
        builtin_ids,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}

#[derive(Debug, Clone, Copy)]
pub struct EnableTodayWorkSource {
    pub list_id: SavedListId,
    pub expected_list_revision: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct DisableTodayWorkSource {
    pub list_id: SavedListId,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TodaySourceChange {
    pub enabled: bool,
    pub changed: bool,
}

fn filter_error(
    filter: &FilterDefinition,
    error: FilterError,
) -> Result<SavedListFilterError, SavedListError> {
    let _ = filter;
    match error {
        FilterError::InvalidStage => Ok(SavedListFilterError::InvalidStage),
        FilterError::InvalidAssignee => Ok(SavedListFilterError::InvalidAssignee),
        FilterError::InvalidTag => Ok(SavedListFilterError::InvalidTag),
        FilterError::Malformed => Ok(SavedListFilterError::UnsupportedFilter),
        FilterError::Database(error) => Err(SavedListError::Database(error)),
    }
}

/// Enumerates only the caller's current, live and visible preferences. This
/// defensive live join makes orphaned, hidden and rollback-tombstoned rows
/// inert without a write on the read path.
pub(crate) async fn evaluated_sources_raw(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Vec<EvaluatedTodaySource>, SavedListError> {
    let rows = sqlx::query!(
        r#"SELECT l.id, l.name AS "name!", l.scope, l.revision, l.filter::text AS "filter!"
           FROM today_work_source tws
           JOIN saved_list l
             ON l.organization_id = tws.organization_id AND l.id = tws.list_id
           WHERE tws.organization_id = $1
             AND tws.user_id = $2
             AND l.deleted_at IS NULL
             AND (l.scope = 'shared' OR l.created_by_user_id = $2)
           ORDER BY l.id ASC"#,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let name = row.name;
        let scope_text = row.scope;
        let scope = SavedListScope::from_db(&scope_text)?;
        let revision = row.revision;
        let raw_filter = row.filter;
        let parsed = serde_json::from_str::<serde_json::Value>(&raw_filter)
            .ok()
            .and_then(|value| saved_list::decode_structural_filter(&value));
        let (filter, filter_error) = match parsed {
            None => (None, Some(SavedListFilterError::UnsupportedFilter)),
            // Reference validation is intentionally deferred for Today
            // reads: each source gets its own savepoint and 500 ms whole
            // source allowance, so one broken reference lookup cannot turn
            // every configured source into an unavailable feed.
            Some(filter) => (Some(filter), None),
        };
        result.push(EvaluatedTodaySource {
            source: TodaySource {
                list_id: SavedListId::new(id),
                name,
                scope,
                revision,
                filter_error,
            },
            filter,
        });
    }
    Ok(result)
}

pub async fn list_today_work_sources(
    conn: &mut PgConnection,
    auth: &AuthContext,
) -> Result<Vec<TodaySource>, SavedListError> {
    let mut sources =
        evaluated_sources_raw(conn, auth.active_organization_id, auth.actor_user_id).await?;
    for source in &mut sources {
        if let Some(filter) = &source.filter {
            source.source.filter_error = match filter
                .validate_references(conn, auth.active_organization_id)
                .await
            {
                Ok(()) => None,
                Err(error) => Some(filter_error(filter, error)?),
            };
        }
    }
    Ok(sources.into_iter().map(|source| source.source).collect())
}

#[tracing::instrument(
    name = "today_source.enable",
    skip_all,
    fields(organization_id = %ctx.organization_id, actor_id = %ctx.actor_user_id, list_id = %cmd.list_id)
)]
pub async fn enable_today_work_source(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: EnableTodayWorkSource,
) -> Result<TodaySourceChange, SavedListError> {
    validate_expected_revision(cmd.expected_list_revision)?;
    let mut tx = pool.begin().await?;
    saved_list::lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    saved_list::acquire_saved_lists_lock(&mut tx, ctx.organization_id).await?;
    let list = saved_list::visible_live_row_for_update(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.list_id,
    )
    .await?
    .ok_or(SavedListError::NotFound)?;
    if list.revision != cmd.expected_list_revision {
        return Err(SavedListError::Conflict);
    }
    let filter = list
        .filter
        .as_ref()
        .and_then(saved_list::decode_structural_filter)
        .ok_or(SavedListError::UnsupportedFilter)?;
    filter
        .validate_references(&mut tx, ctx.organization_id)
        .await
        .map_err(SavedListError::from)?;

    let already_enabled = sqlx::query_scalar!(
        r#"SELECT EXISTS(
             SELECT 1 FROM today_work_source
             WHERE organization_id = $1 AND user_id = $2 AND list_id = $3
           ) AS "already_enabled!""#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        cmd.list_id.0,
    )
    .fetch_one(&mut *tx)
    .await?;
    if already_enabled {
        tx.commit().await?;
        return Ok(TodaySourceChange {
            enabled: true,
            changed: false,
        });
    }

    // Count the same live, visible join that configuration reads use. A
    // preference orphaned by a rollback-era soft-delete consumes no slot.
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!"
           FROM today_work_source tws
           JOIN saved_list l
             ON l.organization_id = tws.organization_id AND l.id = tws.list_id
           WHERE tws.organization_id = $1
             AND tws.user_id = $2
             AND l.deleted_at IS NULL
             AND (l.scope = 'shared' OR l.created_by_user_id = $2)"#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
    )
    .fetch_one(&mut *tx)
    .await?;
    if count >= TODAY_SOURCE_LIMIT {
        return Err(SavedListError::TodaySourceLimitReached);
    }
    sqlx::query!(
        r#"INSERT INTO today_work_source (organization_id, user_id, list_id)
           VALUES ($1, $2, $3)"#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        cmd.list_id.0,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(TodaySourceChange {
        enabled: true,
        changed: true,
    })
}

#[tracing::instrument(
    name = "today_source.disable",
    skip_all,
    fields(organization_id = %ctx.organization_id, actor_id = %ctx.actor_user_id, list_id = %cmd.list_id)
)]
pub async fn disable_today_work_source(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DisableTodayWorkSource,
) -> Result<TodaySourceChange, SavedListError> {
    let mut tx = pool.begin().await?;
    saved_list::lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    saved_list::acquire_saved_lists_lock(&mut tx, ctx.organization_id).await?;
    // Tombstones remain visible to their creator/shared viewers specifically
    // so an uncertain disable can complete after a simultaneous deletion.
    let visible = sqlx::query!(
        r#"SELECT 1 AS "visible!" FROM saved_list
           WHERE id = $1 AND organization_id = $2
             AND (scope = 'shared' OR created_by_user_id = $3)
           FOR UPDATE"#,
        cmd.list_id.0,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !visible {
        return Err(SavedListError::NotFound);
    }
    let deleted = sqlx::query!(
        r#"DELETE FROM today_work_source
           WHERE organization_id = $1 AND user_id = $2 AND list_id = $3"#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        cmd.list_id.0,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(TodaySourceChange {
        enabled: false,
        changed: deleted.rows_affected() == 1,
    })
}

//! The fixed built-in task axis (docs/specs/SLICE_016.md §5, D-054 §1):
//! two static statements against `task_org_assignee_due_open_idx`, no
//! dynamic SQL, no rule-7 inquiry constraint, literal Organization
//! predicate, `now` bound as a parameter. Query-binding and row-shaping
//! only — the savepoint, its own `SOURCE_BUDGET`, the failure-recovery
//! discipline and the tier/order merge all live in `today::mod`, beside
//! the call feed's identical mechanism (`evaluate_feeds_builtins`).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crate::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crate::domain::task::TaskKind;
use crate::domain::today::model::{RecommendedAction, TodayItem, TodayPriority, TodayReason};
use crate::ids::{OrganizationId, PersonId, StageId, TaskId, UserId};

fn decode_contact(
    id: Option<Uuid>,
    channel: Option<String>,
    outcome: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
) -> Result<Option<ContactAttemptRef>, sqlx::Error> {
    match (id, channel, outcome, occurred_at) {
        (Some(id), Some(channel), Some(outcome), Some(occurred_at)) => {
            let channel = ContactChannel::decode(&channel).ok_or_else(|| {
                sqlx::Error::Decode("task axis query: invalid contact channel".into())
            })?;
            let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                sqlx::Error::Decode("task axis query: invalid contact outcome".into())
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
            "task axis query: contact columns must be all null or all set".into(),
        )),
    }
}

fn decode_kind(raw: &str) -> Result<TaskKind, sqlx::Error> {
    TaskKind::from_db_str(raw)
        .ok_or_else(|| sqlx::Error::Decode("task axis query: unrecognized task kind".into()))
}

/// One row of statement (a): the viewer's earliest open, dated task on a
/// retained Person (docs/specs/SLICE_016.md §5).
pub(super) struct TaskMembershipRow {
    pub person_id: Uuid,
    pub task_id: Uuid,
    pub title: String,
    pub kind: String,
    pub due_at: DateTime<Utc>,
}

/// docs/specs/SLICE_016.md §5 statement (a): for every retained Person id
/// (P ∪ call-only — `retained_ids`), the viewer's earliest open, dated
/// task with `due_at <= now + 24h`, filtered by assignee before choosing
/// the earliest. One row per Person. Empty `retained_ids` short-circuits
/// (matching `call_membership`'s precedent) rather than binding an empty
/// array.
pub(super) async fn task_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    viewer: UserId,
    now: DateTime<Utc>,
    retained_ids: &[Uuid],
) -> Result<Vec<TaskMembershipRow>, sqlx::Error> {
    if retained_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_file_as!(
        TaskMembershipRow,
        "src/domain/today/sql/task_membership.sql",
        organization_id.0,
        viewer.0,
        now,
        retained_ids,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// Builds the `TaskOverdue`/`TaskDue` reason for one membership row
/// (docs/specs/SLICE_016.md §5: overdue is `due_at < now`, strict).
pub(super) fn membership_reason(
    row: &TaskMembershipRow,
    now: DateTime<Utc>,
) -> Result<(TodayReason, bool), sqlx::Error> {
    let kind = decode_kind(&row.kind)?;
    let overdue = row.due_at < now;
    let reason = if overdue {
        TodayReason::TaskOverdue {
            task_id: TaskId::new(row.task_id),
            title: row.title.clone(),
            kind,
            due_at: row.due_at,
        }
    } else {
        TodayReason::TaskDue {
            task_id: TaskId::new(row.task_id),
            title: row.title.clone(),
            kind,
            due_at: row.due_at,
        }
    };
    Ok((reason, overdue))
}

struct TaskOnlyRow {
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
    last_attempt_id: Option<Uuid>,
    last_attempt_channel: Option<String>,
    last_attempt_outcome: Option<String>,
    last_attempt_occurred_at: Option<DateTime<Utc>>,
    task_id: Uuid,
    title: String,
    kind: String,
    due_at: DateTime<Utc>,
}

/// docs/specs/SLICE_016.md §5 statement (b): the task-only prefix, joined
/// to `person` for the summary, ordered `due_at ASC, id ASC`, limited to
/// `(200 - |retained|) + 1`. Only called when neither the person-state
/// statement nor the call prefix was truncated.
pub(super) async fn task_only_prefix(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    viewer: UserId,
    now: DateTime<Utc>,
    retained_ids: &[Uuid],
    limit: i64,
) -> Result<Vec<TodayItem>, sqlx::Error> {
    let rows = sqlx::query_file_as!(
        TaskOnlyRow,
        "src/domain/today/sql/task_only.sql",
        organization_id.0,
        viewer.0,
        now,
        retained_ids,
        limit,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter().map(|row| task_only_item(row, now)).collect()
}

/// Shapes one task-only row into a `TodayItem` directly (docs/specs/
/// SLICE_016.md §5): `latest_inquiry: null`, `waiting_since: null`,
/// `last_contact_attempt` hydrated from the same effective-attempt
/// pattern every other Today statement uses, `priority` `high` when
/// overdue else `normal`, `recommended_action` from `kind` (§5: `email` ->
/// `Email` if an email exists else `Call` if a phone else `ReviewPerson`;
/// every other kind -> the list-only chain, `Call` if a phone else
/// `Email` if an email else `ReviewPerson`).
fn task_only_item(row: TaskOnlyRow, now: DateTime<Utc>) -> Result<TodayItem, sqlx::Error> {
    let display_name = compute_display_name(
        row.first_name.as_deref(),
        row.last_name.as_deref(),
        row.primary_email.as_deref(),
        row.primary_phone.as_deref(),
    );
    let assigned_user = match (row.assigned_user_id, row.assigned_user_display_name.clone()) {
        (Some(id), Some(display_name)) => Some(UserRef {
            id: UserId::new(id),
            display_name,
        }),
        _ => None,
    };
    let person = PersonSummary {
        id: PersonId::new(row.id),
        first_name: row.first_name.clone(),
        last_name: row.last_name.clone(),
        display_name,
        stage: StageRef {
            id: StageId::new(row.stage_id),
            name: row.stage_name.clone(),
        },
        assigned_user,
        primary_email: row.primary_email.clone(),
        primary_phone: row.primary_phone.clone(),
        inquiry_count: row.inquiry_count,
        // docs/specs/SLICE_016.md §1 rule 8, §5: the task axis's own
        // `TodayItem.latest_inquiry` is unconditionally null for a
        // task-only item; `PersonSummary.last_inquiry_at` follows the
        // same rule here (the SQL never selects it), consistent with the
        // Person carrying no inquiry-derived state on this item.
        last_inquiry_at: None,
        created_at: row.created_at,
    };
    let last_contact_attempt = decode_contact(
        row.last_attempt_id,
        row.last_attempt_channel,
        row.last_attempt_outcome,
        row.last_attempt_occurred_at,
    )?;
    let kind = decode_kind(&row.kind)?;
    let overdue = row.due_at < now;
    let priority = if overdue {
        TodayPriority::High
    } else {
        TodayPriority::Normal
    };
    let reason = if overdue {
        TodayReason::TaskOverdue {
            task_id: TaskId::new(row.task_id),
            title: row.title.clone(),
            kind,
            due_at: row.due_at,
        }
    } else {
        TodayReason::TaskDue {
            task_id: TaskId::new(row.task_id),
            title: row.title,
            kind,
            due_at: row.due_at,
        }
    };
    let has_email = person.primary_email.is_some();
    let has_phone = person.primary_phone.is_some();
    let recommended_action = if matches!(kind, TaskKind::Email) {
        if has_email {
            RecommendedAction::Email
        } else if has_phone {
            RecommendedAction::Call
        } else {
            RecommendedAction::ReviewPerson
        }
    } else if has_phone {
        RecommendedAction::Call
    } else if has_email {
        RecommendedAction::Email
    } else {
        RecommendedAction::ReviewPerson
    };
    Ok(TodayItem {
        person,
        priority,
        recommended_action,
        reasons: vec![reason],
        waiting_since: None,
        latest_inquiry: None,
        last_contact_attempt,
    })
}

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

use crate::domain::admin::Role;
use crate::domain::envelope::CommandContext;
use crate::domain::person::queries as person_queries;
use crate::ids::{OrganizationId, PersonId, TaskId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

use super::error::TaskError;
use super::model::{Task, TaskKind, TaskTitle};
use super::queries::{self, TaskRowFull};

/// The five mutating commands' own-membership re-check (docs/specs/
/// SLICE_016.md §3, §9): a `FOR SHARE` re-read of the actor's OWN
/// membership row, inside the same transaction as the task row lock, so a
/// concurrent demotion or deactivation cannot interleave with the rule-1
/// permission decision. Returns `None` for a missing or inactive
/// membership — the actor already holds a valid authenticated session
/// (`AuthContext`), so this is a concurrent-demotion/deactivation defense,
/// not an authentication check; the caller folds `None` into the same
/// `Forbidden` every other failed verdict produces. Duplicated from
/// `note::commands::lock_current_membership` (the brief's stated lane
/// choice) — identical shape, no task-specific behavior.
async fn lock_current_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Option<Role>, TaskError> {
    let row = sqlx::query!(
        r#"SELECT role, status
           FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2
           FOR SHARE"#,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.status != "active" {
        return Ok(None);
    }
    Ok(Some(
        Role::from_db_str(&row.role).ok_or(TaskError::Corrupt)?,
    ))
}

/// Rule 1 (docs/specs/SLICE_016.md §1): admin, the task's assignee, or its
/// creator. `role` is `None` for a missing/inactive membership (concurrent
/// removal/deactivation) — always `Forbidden`, never a different code, so
/// a demoted or deactivated actor cannot distinguish "you no longer
/// belong" from "you never had the right".
fn permitted(role: Option<Role>, actor_user_id: UserId, row: &TaskRowFull) -> bool {
    match role {
        Some(role) => {
            role == Role::Admin
                || row.assignee_user_id == Some(actor_user_id)
                || row.created_by_user_id == Some(actor_user_id)
        }
        None => false,
    }
}

fn record_outcome<T>(result: &Result<T, TaskError>) {
    match result {
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "task command failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
}

async fn publish_task_changed(publisher: &Publisher, ctx: &CommandContext, person_id: PersonId) {
    let event = RealtimeEvent::person_changed(
        ctx.organization_id,
        chrono::Utc::now(),
        ctx.correlation_id,
        person_id,
        PersonChange::TaskChanged,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;
}

/// The five mutating commands' shared lookup, lock, and rule-1 decision
/// (docs/specs/SLICE_016.md §3): `lock_person` (else `NotFound`), then the
/// task row `FOR UPDATE` (else `NotFound`), then the actor's own
/// membership `FOR SHARE`, then permission (else `Forbidden`) — in that
/// order, so 403 precedes any later 422 on a changed assignee.
async fn lock_person_task_and_authorize(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    person_id: PersonId,
    task_id: TaskId,
) -> Result<(TaskRowFull, Role), TaskError> {
    person_queries::lock_person(tx, person_id, organization_id)
        .await?
        .ok_or(TaskError::NotFound)?;
    let row = queries::lock_task_for_update(tx, organization_id, person_id, task_id)
        .await?
        .ok_or(TaskError::NotFound)?;
    let role = lock_current_membership(tx, organization_id, actor_user_id).await?;
    if !permitted(role, actor_user_id, &row) {
        return Err(TaskError::Forbidden);
    }
    // `permitted` above already required `role.is_some()`; unwrap is safe.
    Ok((row, role.expect("permitted() requires Some(role)")))
}

// --- CreateTask ----------------------------------------------------------

/// No `Debug` derive: this struct carries a task title, so `?cmd` can
/// never put one in a span or log (docs/specs/SLICE_016.md §1 rule 7).
pub struct CreateTask {
    pub person_id: PersonId,
    pub title: String,
    pub kind: TaskKind,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee_user_id: Option<UserId>,
}

#[tracing::instrument(
    name = "task.create",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = tracing::field::Empty,
        title_chars = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn create_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CreateTask,
) -> Result<Task, TaskError> {
    let result = create_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(task) => {
            let span = tracing::Span::current();
            span.record("task_id", task.id.to_string());
            span.record("outcome", "created");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn create_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CreateTask,
) -> Result<Task, TaskError> {
    let title = TaskTitle::parse(&cmd.title)?;
    tracing::Span::current().record("title_chars", title.chars().count());

    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(TaskError::NotFound)?;

    let assignee_user_id = cmd.assignee_user_id.unwrap_or(ctx.actor_user_id);
    // Validated only for an EXPLICITLY supplied assignee (docs/specs/
    // SLICE_016.md §3): the default-to-actor path relies on the actor's
    // own already-established session membership, exactly like every
    // other command that acts as "the current active member".
    if cmd.assignee_user_id.is_some() {
        let active =
            queries::assignee_is_active_member(&mut tx, ctx.organization_id, assignee_user_id)
                .await?;
        if !active {
            return Err(TaskError::InvalidAssignee);
        }
    }

    let row = queries::insert_task(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        &title,
        cmd.kind,
        cmd.due_at,
        assignee_user_id,
        ctx.actor_user_id,
        ctx.origin.as_str(),
        ctx.correlation_id.0,
    )
    .await?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    // The creator of a brand-new task can always manage it (rule 1: they
    // are `created_by_user_id`, regardless of who the assignee is).
    queries::task_from_row(row, cmd.person_id, true)
}

// --- UpdateTask ------------------------------------------------------------

/// No `Debug` derive (carries a title): see [`CreateTask`]. `UpdateTask`
/// full-replaces all four mutable fields (docs/specs/SLICE_016.md §3);
/// `assignee_user_id` is required (unlike `CreateTask`, an existing task
/// always has an explicit assignee to keep or change).
pub struct UpdateTask {
    pub person_id: PersonId,
    pub task_id: TaskId,
    pub title: String,
    pub kind: TaskKind,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee_user_id: UserId,
}

#[derive(Debug, Clone)]
pub struct UpdateTaskOutcome {
    pub task: Task,
    pub changed: bool,
}

#[tracing::instrument(
    name = "task.update",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = %cmd.task_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn update_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: UpdateTask,
) -> Result<UpdateTaskOutcome, TaskError> {
    let result = update_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn update_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: UpdateTask,
) -> Result<UpdateTaskOutcome, TaskError> {
    let title = TaskTitle::parse(&cmd.title)?;

    let mut tx = pool.begin().await?;
    let (row, role) = lock_person_task_and_authorize(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.person_id,
        cmd.task_id,
    )
    .await?;

    let assignee_changed = Some(cmd.assignee_user_id) != row.assignee_user_id;
    // The assignee is re-validated as an active member ONLY when it
    // changes (docs/specs/SLICE_016.md §3): a task held by a deactivated
    // member can still be retitled.
    if assignee_changed {
        let active =
            queries::assignee_is_active_member(&mut tx, ctx.organization_id, cmd.assignee_user_id)
                .await?;
        if !active {
            return Err(TaskError::InvalidAssignee);
        }
    }

    let unchanged = row.title == title
        && row.kind == cmd.kind.as_str()
        && row.due_at == cmd.due_at
        && !assignee_changed;

    if unchanged {
        tx.commit().await?;
        let can_manage = permitted(Some(role), ctx.actor_user_id, &row);
        return Ok(UpdateTaskOutcome {
            task: queries::task_from_row(row, cmd.person_id, can_manage)?,
            changed: false,
        });
    }

    queries::update_task_full(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.task_id,
        &title,
        cmd.kind,
        cmd.due_at,
        cmd.assignee_user_id,
    )
    .await?;
    let updated_row =
        queries::lock_task_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.task_id)
            .await?
            .ok_or(TaskError::Corrupt)?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    // Recomputed against the UPDATED row (docs/specs/SLICE_016.md §9): an
    // assignee who reassigns a task away from themselves loses
    // `can_manage` in this very response — the creator keeps it.
    let can_manage = permitted(Some(role), ctx.actor_user_id, &updated_row);
    Ok(UpdateTaskOutcome {
        task: queries::task_from_row(updated_row, cmd.person_id, can_manage)?,
        changed: true,
    })
}

// --- CompleteTask ----------------------------------------------------------

pub struct CompleteTask {
    pub person_id: PersonId,
    pub task_id: TaskId,
}

#[derive(Debug, Clone)]
pub struct CompleteTaskOutcome {
    pub task: Task,
    pub changed: bool,
}

#[tracing::instrument(
    name = "task.complete",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = %cmd.task_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn complete_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CompleteTask,
) -> Result<CompleteTaskOutcome, TaskError> {
    let result = complete_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn complete_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: CompleteTask,
) -> Result<CompleteTaskOutcome, TaskError> {
    let mut tx = pool.begin().await?;
    let (row, _role) = lock_person_task_and_authorize(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.person_id,
        cmd.task_id,
    )
    .await?;

    if row.completed_at.is_some() {
        tx.commit().await?;
        return Ok(CompleteTaskOutcome {
            task: queries::task_from_row(row, cmd.person_id, true)?,
            changed: false,
        });
    }

    queries::complete_task_write(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.task_id,
        ctx.actor_user_id,
    )
    .await?;
    let updated_row =
        queries::lock_task_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.task_id)
            .await?
            .ok_or(TaskError::Corrupt)?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    Ok(CompleteTaskOutcome {
        task: queries::task_from_row(updated_row, cmd.person_id, true)?,
        changed: true,
    })
}

// --- ReopenTask ------------------------------------------------------------

pub struct ReopenTask {
    pub person_id: PersonId,
    pub task_id: TaskId,
}

#[derive(Debug, Clone)]
pub struct ReopenTaskOutcome {
    pub task: Task,
    pub changed: bool,
}

#[tracing::instrument(
    name = "task.reopen",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = %cmd.task_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn reopen_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ReopenTask,
) -> Result<ReopenTaskOutcome, TaskError> {
    let result = reopen_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn reopen_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ReopenTask,
) -> Result<ReopenTaskOutcome, TaskError> {
    let mut tx = pool.begin().await?;
    let (row, _role) = lock_person_task_and_authorize(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.person_id,
        cmd.task_id,
    )
    .await?;

    if row.completed_at.is_none() {
        tx.commit().await?;
        return Ok(ReopenTaskOutcome {
            task: queries::task_from_row(row, cmd.person_id, true)?,
            changed: false,
        });
    }

    queries::reopen_task_write(&mut tx, ctx.organization_id, cmd.person_id, cmd.task_id).await?;
    let updated_row =
        queries::lock_task_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.task_id)
            .await?
            .ok_or(TaskError::Corrupt)?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    Ok(ReopenTaskOutcome {
        task: queries::task_from_row(updated_row, cmd.person_id, true)?,
        changed: true,
    })
}

// --- SnoozeTask ------------------------------------------------------------

#[derive(Debug)]
pub struct SnoozeTask {
    pub person_id: PersonId,
    pub task_id: TaskId,
    pub due_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SnoozeTaskOutcome {
    pub task: Task,
    pub changed: bool,
}

/// `SnoozeTask` (docs/specs/SLICE_016.md §1 rule 3, §3): implemented
/// internally as a one-field `UpdateTask` — the same lock order and
/// permission decision, but writing only `due_at` — rather than calling
/// `update_task` itself, so the Today panel (016b) never has to
/// full-replace title/kind/assignee from a row it may not hold fresh.
#[tracing::instrument(
    name = "task.snooze",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = %cmd.task_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn snooze_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: SnoozeTask,
) -> Result<SnoozeTaskOutcome, TaskError> {
    let result = snooze_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn snooze_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: SnoozeTask,
) -> Result<SnoozeTaskOutcome, TaskError> {
    let mut tx = pool.begin().await?;
    let (row, _role) = lock_person_task_and_authorize(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.person_id,
        cmd.task_id,
    )
    .await?;

    // Rule 3: snooze applies to OPEN tasks only. A completed task returns
    // `changed: false` with the current (completed) row, unconditionally
    // — regardless of `due_at` — and writes nothing; a snooze never
    // reopens.
    if row.completed_at.is_some() {
        tx.commit().await?;
        return Ok(SnoozeTaskOutcome {
            task: queries::task_from_row(row, cmd.person_id, true)?,
            changed: false,
        });
    }

    if row.due_at == Some(cmd.due_at) {
        tx.commit().await?;
        return Ok(SnoozeTaskOutcome {
            task: queries::task_from_row(row, cmd.person_id, true)?,
            changed: false,
        });
    }

    queries::snooze_task_write(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.task_id,
        cmd.due_at,
    )
    .await?;
    let updated_row =
        queries::lock_task_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.task_id)
            .await?
            .ok_or(TaskError::Corrupt)?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    Ok(SnoozeTaskOutcome {
        task: queries::task_from_row(updated_row, cmd.person_id, true)?,
        changed: true,
    })
}

// --- DeleteTask ------------------------------------------------------------

pub struct DeleteTask {
    pub person_id: PersonId,
    pub task_id: TaskId,
}

#[derive(Debug, Clone)]
pub struct DeleteTaskOutcome {
    pub deleted: bool,
}

#[tracing::instrument(
    name = "task.delete",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        task_id = %cmd.task_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn delete_task(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: DeleteTask,
) -> Result<DeleteTaskOutcome, TaskError> {
    let result = delete_task_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "deleted");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn delete_task_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: DeleteTask,
) -> Result<DeleteTaskOutcome, TaskError> {
    let mut tx = pool.begin().await?;
    let (_row, _role) = lock_person_task_and_authorize(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.person_id,
        cmd.task_id,
    )
    .await?;

    queries::tombstone_task(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.task_id,
        ctx.actor_user_id,
    )
    .await?;
    tx.commit().await?;

    publish_task_changed(publisher, ctx, cmd.person_id).await;

    Ok(DeleteTaskOutcome { deleted: true })
}

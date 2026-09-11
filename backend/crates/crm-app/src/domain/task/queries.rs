use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::person::model::{compute_display_name, UserRef};
use crate::ids::{OrganizationId, PersonId, TaskId, UserId};

use super::error::TaskError;
use super::model::{PersonRef, Task, TaskKind, TaskWithPerson};

/// A full task row for command-internal lookups (update/complete/reopen/
/// snooze/delete's `FOR UPDATE` load) and for the open-tasks reads.
/// `pub(crate)`, mirroring `note::queries::NoteRowFull` — never carries
/// anything that must be logged (the title is a plain field like every
/// other row type; the discipline is "never log", not "never hold in
/// memory").
pub(crate) struct TaskRowFull {
    pub id: TaskId,
    pub title: String,
    pub kind: String,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee_user_id: Option<UserId>,
    pub assignee_display_name: Option<String>,
    pub created_by_user_id: Option<UserId>,
    pub created_by_display_name: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by_user_id: Option<UserId>,
    pub completed_by_display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

struct TaskRowFullDb {
    id: Uuid,
    title: String,
    kind: String,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: Option<Uuid>,
    assignee_display_name: Option<String>,
    created_by_user_id: Option<Uuid>,
    created_by_display_name: Option<String>,
    completed_at: Option<DateTime<Utc>>,
    completed_by_user_id: Option<Uuid>,
    completed_by_display_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<TaskRowFullDb> for TaskRowFull {
    fn from(value: TaskRowFullDb) -> Self {
        TaskRowFull {
            id: TaskId::new(value.id),
            title: value.title,
            kind: value.kind,
            due_at: value.due_at,
            assignee_user_id: value.assignee_user_id.map(UserId::new),
            assignee_display_name: value.assignee_display_name,
            created_by_user_id: value.created_by_user_id.map(UserId::new),
            created_by_display_name: value.created_by_display_name,
            completed_at: value.completed_at,
            completed_by_user_id: value.completed_by_user_id.map(UserId::new),
            completed_by_display_name: value.completed_by_display_name,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// The `Update`/`Complete`/`Reopen`/`Snooze`/`Delete` lookup and lock
/// (docs/specs/SLICE_016.md §3): binds `id`, `organization_id` **and**
/// `person_id` — a task reached through another Person's path in the same
/// Organization is invisible here, byte-identical to a foreign or
/// nonexistent id. Excludes tombstones (`deleted_at IS NULL`): a second
/// delete, or any write to a tombstone, is `NotFound`, identical to a
/// nonexistent id. `FOR UPDATE OF t` scopes the row lock to `task` only —
/// the three `LEFT JOIN app_user` sides are read, never locked.
pub(crate) async fn lock_task_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
) -> Result<Option<TaskRowFull>, TaskError> {
    let row = sqlx::query_as!(
        TaskRowFullDb,
        r#"SELECT t.id, t.title, t.kind, t.due_at,
                  t.assignee_user_id, au.display_name as "assignee_display_name?",
                  t.created_by_user_id, cu.display_name as "created_by_display_name?",
                  t.completed_at,
                  t.completed_by_user_id, ku.display_name as "completed_by_display_name?",
                  t.created_at, t.updated_at
           FROM task t
           LEFT JOIN app_user au ON au.id = t.assignee_user_id
           LEFT JOIN app_user cu ON cu.id = t.created_by_user_id
           LEFT JOIN app_user ku ON ku.id = t.completed_by_user_id
           WHERE t.id = $1 AND t.organization_id = $2 AND t.person_id = $3
             AND t.deleted_at IS NULL
           FOR UPDATE OF t"#,
        task_id.0,
        organization_id.0,
        person_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(TaskRowFull::from))
}

/// `CreateTask`/`UpdateTask`'s assignee validation (docs/specs/
/// SLICE_016.md §3): a `FOR SHARE` read of the candidate assignee's OWN
/// membership row, serialising against a concurrent deactivation —
/// whichever commits first wins. `false` for a missing membership (a
/// foreign or nonexistent user) exactly like an inactive one, so both
/// collapse to the same `InvalidAssignee` 422 at the call site.
pub(crate) async fn assignee_is_active_member(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<bool, TaskError> {
    let row = sqlx::query!(
        r#"SELECT status FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2
           FOR SHARE"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some_and(|r| r.status == "active"))
}

/// `AddTask`'s insert (docs/specs/SLICE_016.md §3): both the assignee's
/// and the creator's display names are joined in the same round trip —
/// both are guaranteed active members at this point (validated by the
/// caller, or the session's own actor).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_task(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    title: &str,
    kind: TaskKind,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: UserId,
    created_by_user_id: UserId,
    origin: &str,
    correlation_id: Uuid,
) -> Result<TaskRowFull, TaskError> {
    struct InsertedTaskRow {
        id: Uuid,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        due_at: Option<DateTime<Utc>>,
        title: String,
        kind: String,
        assignee_display_name: String,
        created_by_display_name: String,
    }
    // RETURNING (and re-selecting) `due_at`/`title`/`kind` from the row
    // rather than echoing the caller's own arguments: sqlx stores
    // `DateTime<Utc>` at Postgres's microsecond precision, so a
    // sub-microsecond (nanosecond) `due_at` the caller passed would
    // otherwise be echoed untruncated in the receipt — a client that
    // re-sends that exact receipt value on the next PUT would then see
    // `changed: true` instead of `changed: false` (spec §1 rule 2: an
    // unmodified round trip must be a no-op). Reading the STORED value
    // back guarantees the receipt is always the byte-identical value a
    // subsequent read would see.
    let row = sqlx::query_as!(
        InsertedTaskRow,
        r#"WITH inserted AS (
               INSERT INTO task (organization_id, person_id, title, kind, due_at,
                                  assignee_user_id, created_by_user_id, origin, correlation_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING id, created_at, updated_at, due_at, title, kind
           )
           SELECT inserted.id, inserted.created_at, inserted.updated_at,
                  inserted.due_at, inserted.title, inserted.kind,
                  au.display_name as "assignee_display_name!",
                  cu.display_name as "created_by_display_name!"
           FROM inserted
           JOIN app_user au ON au.id = $6
           JOIN app_user cu ON cu.id = $7"#,
        organization_id.0,
        person_id.0,
        title,
        kind.as_str(),
        due_at,
        assignee_user_id.0,
        created_by_user_id.0,
        origin,
        correlation_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(TaskRowFull {
        id: TaskId::new(row.id),
        title: row.title,
        kind: row.kind,
        due_at: row.due_at,
        assignee_user_id: Some(assignee_user_id),
        assignee_display_name: Some(row.assignee_display_name),
        created_by_user_id: Some(created_by_user_id),
        created_by_display_name: Some(row.created_by_display_name),
        completed_at: None,
        completed_by_user_id: None,
        completed_by_display_name: None,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// `UpdateTask`'s full-replace write on a genuine change (docs/specs/
/// SLICE_016.md §3): the four fields plus `updated_at = now()`; the
/// completion columns are never touched.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_task_full(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
    title: &str,
    kind: TaskKind,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: UserId,
) -> Result<(), TaskError> {
    sqlx::query!(
        r#"UPDATE task
           SET title = $4, kind = $5, due_at = $6, assignee_user_id = $7, updated_at = now()
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        task_id.0,
        organization_id.0,
        person_id.0,
        title,
        kind.as_str(),
        due_at,
        assignee_user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `CompleteTask`'s write (docs/specs/SLICE_016.md §3): `completed_at`,
/// `completed_by_user_id` only; `updated_at` is deliberately left
/// untouched — completion is not an edit of the task's own fields.
pub(crate) async fn complete_task_write(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
    completed_by_user_id: UserId,
) -> Result<(), TaskError> {
    sqlx::query!(
        r#"UPDATE task SET completed_at = now(), completed_by_user_id = $4
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        task_id.0,
        organization_id.0,
        person_id.0,
        completed_by_user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `ReopenTask`'s write (docs/specs/SLICE_016.md §3): clears both
/// completion columns.
pub(crate) async fn reopen_task_write(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
) -> Result<(), TaskError> {
    sqlx::query!(
        r#"UPDATE task SET completed_at = NULL, completed_by_user_id = NULL
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        task_id.0,
        organization_id.0,
        person_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `SnoozeTask`'s write (docs/specs/SLICE_016.md §3): `due_at` only, plus
/// `updated_at = now()` — never the completion columns (snooze on a
/// completed task is `changed: false` before this is ever called).
pub(crate) async fn snooze_task_write(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
    due_at: DateTime<Utc>,
) -> Result<(), TaskError> {
    sqlx::query!(
        r#"UPDATE task SET due_at = $4, updated_at = now()
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        task_id.0,
        organization_id.0,
        person_id.0,
        due_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `DeleteTask`'s tombstone (docs/specs/SLICE_016.md §1 rule 4, §3): title
/// emptied, `deleted_at`/`deleted_by_user_id` stamped, `updated_at`
/// deliberately **not** touched — every other column, including the
/// completion columns and the import provenance, stays byte-identical.
pub(crate) async fn tombstone_task(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    task_id: TaskId,
    deleted_by_user_id: UserId,
) -> Result<(), TaskError> {
    sqlx::query!(
        r#"UPDATE task SET title = '', deleted_at = now(), deleted_by_user_id = $4
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        task_id.0,
        organization_id.0,
        person_id.0,
        deleted_by_user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn user_ref(id: Option<Uuid>, display_name: Option<String>) -> Option<UserRef> {
    match (id, display_name) {
        (Some(id), Some(display_name)) => Some(UserRef {
            id: UserId::new(id),
            display_name,
        }),
        _ => None,
    }
}

/// Shapes a locked/read row into the wire `Task` (docs/specs/SLICE_016.md
/// §4). `can_manage` is supplied by the caller: the open-tasks list read
/// has no viewer context and always passes `false` (the people detail
/// route overwrites it per viewer, the tags/note pattern), while a
/// command already knows the acting viewer's rule-1 verdict.
pub(crate) fn task_from_row(
    row: TaskRowFull,
    person_id: PersonId,
    can_manage: bool,
) -> Result<Task, TaskError> {
    Ok(Task {
        id: row.id,
        person_id,
        title: row.title,
        kind: TaskKind::from_db_str(&row.kind).ok_or(TaskError::Corrupt)?,
        due_at: row.due_at,
        assignee: user_ref(row.assignee_user_id.map(|u| u.0), row.assignee_display_name),
        created_by: user_ref(
            row.created_by_user_id.map(|u| u.0),
            row.created_by_display_name,
        ),
        completed_at: row.completed_at,
        completed_by: user_ref(
            row.completed_by_user_id.map(|u| u.0),
            row.completed_by_display_name,
        ),
        created_at: row.created_at,
        updated_at: row.updated_at,
        can_manage,
    })
}

/// The Person detail's `tasks[]` and the Operator's `PersonDetail.tasks`
/// source (docs/specs/SLICE_016.md §3, §4, §7): open (uncompleted,
/// undeleted) tasks for one Person, ordered `due_at ASC NULLS LAST,
/// created_at, id`.
pub async fn open_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<Task>, TaskError> {
    let mut workspace_read = crate::auth::workspace::read(conn, organization_id).await?;
    let conn = &mut *workspace_read;
    let rows = sqlx::query_as!(
        TaskRowFullDb,
        r#"SELECT t.id, t.title, t.kind, t.due_at,
                  t.assignee_user_id, au.display_name as "assignee_display_name?",
                  t.created_by_user_id, cu.display_name as "created_by_display_name?",
                  t.completed_at,
                  t.completed_by_user_id, ku.display_name as "completed_by_display_name?",
                  t.created_at, t.updated_at
           FROM task t
           LEFT JOIN app_user au ON au.id = t.assignee_user_id
           LEFT JOIN app_user cu ON cu.id = t.created_by_user_id
           LEFT JOIN app_user ku ON ku.id = t.completed_by_user_id
           WHERE t.organization_id = $1 AND t.person_id = $2
             AND t.completed_at IS NULL AND t.deleted_at IS NULL
           ORDER BY t.due_at ASC NULLS LAST, t.created_at ASC, t.id ASC"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter()
        .map(|r| task_from_row(TaskRowFull::from(r), person_id, false))
        .collect()
}

/// One completed task, in the shape `history_for_person`'s `task_completed`
/// kind needs (docs/specs/SLICE_016.md §4). `pub(crate)` — an internal
/// wiring type between `domain::task` and `domain::person::queries`, not
/// part of either module's public (cross-crate) surface.
pub(crate) struct TaskHistoryRow {
    pub id: Uuid,
    pub completed_at: DateTime<Utc>,
    pub origin: String,
    pub correlation_id: Uuid,
    pub completed_by: Option<UserRef>,
    pub title: String,
    pub kind: String,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee: Option<UserRef>,
    pub created_by: Option<UserRef>,
}

struct TaskHistoryRowDb {
    id: Uuid,
    completed_at: Option<DateTime<Utc>>,
    origin: String,
    correlation_id: Uuid,
    completed_by_user_id: Option<Uuid>,
    completed_by_display_name: Option<String>,
    title: String,
    kind: String,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: Option<Uuid>,
    assignee_display_name: Option<String>,
    created_by_user_id: Option<Uuid>,
    created_by_display_name: Option<String>,
}

/// `history_for_person`'s ninth source (docs/specs/SLICE_016.md §1 rule 5,
/// §4): every completed, live (undeleted) task for one Person. Reopening a
/// task removes it from this read (no `completed_at`); a tombstoned task
/// is excluded even if it was completed before deletion (rule 4: a
/// tombstone is invisible to every read).
pub(crate) async fn task_completed_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<TaskHistoryRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        TaskHistoryRowDb,
        r#"SELECT t.id, t.completed_at, t.origin, t.correlation_id,
                  t.completed_by_user_id, ku.display_name as "completed_by_display_name?",
                  t.title, t.kind, t.due_at,
                  t.assignee_user_id, au.display_name as "assignee_display_name?",
                  t.created_by_user_id, cu.display_name as "created_by_display_name?"
           FROM task t
           LEFT JOIN app_user ku ON ku.id = t.completed_by_user_id
           LEFT JOIN app_user au ON au.id = t.assignee_user_id
           LEFT JOIN app_user cu ON cu.id = t.created_by_user_id
           WHERE t.organization_id = $1 AND t.person_id = $2
             AND t.completed_at IS NOT NULL AND t.deleted_at IS NULL"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| TaskHistoryRow {
            id: r.id,
            // `completed_at IS NOT NULL` is the WHERE clause above; the
            // column is still nullable to sqlx's eyes, so unwrap here is
            // safe by construction, not a guess.
            completed_at: r
                .completed_at
                .expect("filtered by WHERE completed_at IS NOT NULL"),
            origin: r.origin,
            correlation_id: r.correlation_id,
            completed_by: user_ref(r.completed_by_user_id, r.completed_by_display_name),
            title: r.title,
            kind: r.kind,
            due_at: r.due_at,
            assignee: user_ref(r.assignee_user_id, r.assignee_display_name),
            created_by: user_ref(r.created_by_user_id, r.created_by_display_name),
        })
        .collect())
}

struct TaskWithPersonRowDb {
    id: Uuid,
    title: String,
    kind: String,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: Option<Uuid>,
    assignee_display_name: Option<String>,
    created_by_user_id: Option<Uuid>,
    created_by_display_name: Option<String>,
    completed_at: Option<DateTime<Utc>>,
    completed_by_user_id: Option<Uuid>,
    completed_by_display_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    person_id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
}

/// `GET /api/tasks?scope=mine`'s source (docs/specs/SLICE_016.md §3, §4,
/// 016b) and the built-in task axis's set-based-parity fixture: the
/// viewer's open, dated tasks with `due_at <= now + 24h`, ordered
/// `due_at ASC, id ASC`, fetch 201 (the route returns 200 and reports
/// `truncated` on the 201st row — the `open_for_person`/D-052-adjacent
/// `task_org_assignee_due_open_idx` precedent). Literal Organization
/// predicate, `now` bound as a parameter, no dynamic SQL. `can_manage` is
/// always `true` here: every returned task is one the viewer is the
/// assignee of (rule 1), so no separate re-decision is needed the way the
/// Person detail's open-tasks read needs one per viewer.
pub async fn open_for_assignee(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<Vec<TaskWithPerson>, TaskError> {
    let mut workspace_read = crate::auth::workspace::read(conn, organization_id).await?;
    let conn = &mut *workspace_read;
    let rows = sqlx::query_as!(
        TaskWithPersonRowDb,
        r#"SELECT t.id, t.title, t.kind, t.due_at,
                  t.assignee_user_id, au.display_name as "assignee_display_name?",
                  t.created_by_user_id, cu.display_name as "created_by_display_name?",
                  t.completed_at,
                  t.completed_by_user_id, ku.display_name as "completed_by_display_name?",
                  t.created_at, t.updated_at,
                  p.id as person_id, p.first_name, p.last_name,
                  (SELECT cm.value FROM contact_method cm
                     WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id
                       AND cm.kind = 'email'
                     ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS "primary_email?",
                  (SELECT cm.value FROM contact_method cm
                     WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id
                       AND cm.kind = 'phone'
                     ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS "primary_phone?"
           FROM task t
           JOIN person p ON p.id = t.person_id AND p.organization_id = t.organization_id
           LEFT JOIN app_user au ON au.id = t.assignee_user_id
           LEFT JOIN app_user cu ON cu.id = t.created_by_user_id
           LEFT JOIN app_user ku ON ku.id = t.completed_by_user_id
           WHERE t.organization_id = $1 AND t.assignee_user_id = $2
             AND t.completed_at IS NULL AND t.deleted_at IS NULL
             AND t.due_at IS NOT NULL AND t.due_at <= $3::timestamptz + interval '24 hours'
           ORDER BY t.due_at ASC, t.id ASC
           LIMIT 201"#,
        organization_id.0,
        user_id.0,
        now,
    )
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter()
        .map(|row| {
            let person_id = PersonId::new(row.person_id);
            let display_name = compute_display_name(
                row.first_name.as_deref(),
                row.last_name.as_deref(),
                row.primary_email.as_deref(),
                row.primary_phone.as_deref(),
            );
            let full = TaskRowFull {
                id: TaskId::new(row.id),
                title: row.title,
                kind: row.kind,
                due_at: row.due_at,
                assignee_user_id: row.assignee_user_id.map(UserId::new),
                assignee_display_name: row.assignee_display_name,
                created_by_user_id: row.created_by_user_id.map(UserId::new),
                created_by_display_name: row.created_by_display_name,
                completed_at: row.completed_at,
                completed_by_user_id: row.completed_by_user_id.map(UserId::new),
                completed_by_display_name: row.completed_by_display_name,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            Ok(TaskWithPerson {
                task: task_from_row(full, person_id, true)?,
                person: PersonRef {
                    id: person_id,
                    display_name,
                },
            })
        })
        .collect()
}

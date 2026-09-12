//! Frozen cd3b010 operational Person-detail comparison, test-support only.
//! The body between markers is byte-exact; see the retained source manifest.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static ENTRY_HITS: AtomicUsize = AtomicUsize::new(0);

// BEGIN FROZEN CD3B010
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
// END FROZEN CD3B010

pub async fn run(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<Task>, TaskError> {
    ENTRY_HITS.fetch_add(1, Ordering::SeqCst);
    open_for_person(conn, organization_id, person_id).await
}

pub fn entry_hits() -> usize {
    ENTRY_HITS.load(Ordering::SeqCst)
}

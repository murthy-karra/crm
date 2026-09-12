//! Frozen cd3b010 operational Person-detail comparison, test-support only.
//! The body between markers is byte-exact; see the retained source manifest.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static ENTRY_HITS: AtomicUsize = AtomicUsize::new(0);

// BEGIN FROZEN CD3B010
pub async fn history_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    let mut workspace_read = crate::auth::workspace::read(conn, organization_id).await?;
    let conn = &mut *workspace_read;
    let mut entries = Vec::new();
    entries.extend(imported_history(conn, organization_id, person_id).await?);
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
// END FROZEN CD3B010

pub async fn run(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    ENTRY_HITS.fetch_add(1, Ordering::SeqCst);
    history_for_person(conn, organization_id, person_id).await
}

pub fn entry_hits() -> usize {
    ENTRY_HITS.load(Ordering::SeqCst)
}

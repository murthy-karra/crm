use sqlx::{PgConnection, PgPool};

use crate::domain::admin::Role;
use crate::domain::envelope::CommandContext;
use crate::domain::person::model::UserRef;
use crate::domain::person::queries as person_queries;
use crate::ids::{NoteId, OrganizationId, PersonId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

use super::error::NoteError;
use super::model::{Note, NoteBody};
use super::queries::{self, NoteRowFull};

/// `EditNote`/`DeleteNote`'s membership re-check (docs/specs/SLICE_015.md
/// §3, §6): a `FOR SHARE` re-read of the actor's OWN membership row,
/// inside the same transaction as the note row lock, so a concurrent
/// demotion or deactivation cannot interleave with the rule-1 permission
/// decision. Returns `None` for a missing or inactive membership — the
/// actor already holds a valid authenticated session (`AuthContext`), so
/// this is a concurrent-demotion/deactivation defense, not an
/// authentication check; the caller folds `None` into the same
/// `Forbidden` every other failed verdict produces. Duplicated from
/// `tag::commands::lock_current_membership` rather than lifted to a
/// shared location (brief's stated lane choice) — identical shape, no
/// note-specific behavior.
async fn lock_current_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Option<Role>, NoteError> {
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
    Ok(Some(Role::from_db_str(&row.role).ok_or(NoteError::Corrupt)?))
}

/// Rule 1 (D-053 §4, docs/specs/SLICE_015.md §1): admin, or the note's
/// author. `role` is `None` for a missing/inactive membership (concurrent
/// removal/deactivation) — always `Forbidden`, never a different code, so
/// a demoted or deactivated actor cannot distinguish "you no longer
/// belong" from "you never had the right".
fn permitted(role: Option<Role>, actor_user_id: UserId, row: &NoteRowFull) -> bool {
    match role {
        Some(role) => role == Role::Admin || row.author_user_id == Some(actor_user_id),
        None => false,
    }
}

fn note_view(row: &NoteRowFull, person_id: PersonId, can_manage: bool) -> Note {
    let edited = row.updated_at > row.created_at;
    Note {
        id: row.id,
        person_id,
        body: row.body.clone(),
        author: match (row.author_user_id, &row.author_display_name) {
            (Some(id), Some(display_name)) => Some(UserRef {
                id,
                display_name: display_name.clone(),
            }),
            _ => None,
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
        edited,
        can_manage,
    }
}

fn record_outcome<T>(result: &Result<T, NoteError>) {
    match result {
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "note command failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
}

async fn publish_note_changed(publisher: &Publisher, ctx: &CommandContext, person_id: PersonId) {
    let event = RealtimeEvent::person_changed(
        ctx.organization_id,
        chrono::Utc::now(),
        ctx.correlation_id,
        person_id,
        PersonChange::NoteChanged,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;
}

// --- AddNote -----------------------------------------------------------

/// No `Debug` derive: this struct carries a note body, so `?cmd` can never
/// put one in a span or log (docs/specs/SLICE_015.md §1 rule 7).
pub struct AddNote {
    pub person_id: PersonId,
    pub body: String,
}

#[tracing::instrument(
    name = "note.add",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        note_id = tracing::field::Empty,
        body_chars = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn add_note(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: AddNote,
) -> Result<Note, NoteError> {
    let result = add_note_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(note) => {
            let span = tracing::Span::current();
            span.record("note_id", note.id.to_string());
            span.record("outcome", "added");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn add_note_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: AddNote,
) -> Result<Note, NoteError> {
    let body = NoteBody::parse(&cmd.body)?;
    tracing::Span::current().record("body_chars", body.chars().count());

    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(NoteError::NotFound)?;

    let (note_id, created_at, updated_at, author_display_name) = queries::insert_note(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        ctx.actor_user_id,
        &body,
        ctx.origin.as_str(),
        ctx.correlation_id.0,
    )
    .await?;
    tx.commit().await?;

    publish_note_changed(publisher, ctx, cmd.person_id).await;

    Ok(Note {
        id: note_id,
        person_id: cmd.person_id,
        body,
        author: Some(UserRef {
            id: ctx.actor_user_id,
            display_name: author_display_name,
        }),
        created_at,
        updated_at,
        edited: false,
        // The author of a brand-new note can always manage it (they still
        // hold the membership that just let this command run).
        can_manage: true,
    })
}

// --- EditNote ------------------------------------------------------------

/// No `Debug` derive (carries a body): see [`AddNote`].
pub struct EditNote {
    pub person_id: PersonId,
    pub note_id: NoteId,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EditNoteOutcome {
    pub note: Note,
    pub changed: bool,
}

#[tracing::instrument(
    name = "note.edit",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        note_id = %cmd.note_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn edit_note(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: EditNote,
) -> Result<EditNoteOutcome, NoteError> {
    let result = edit_note_attempt(pool, publisher, ctx, cmd).await;
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

async fn edit_note_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: EditNote,
) -> Result<EditNoteOutcome, NoteError> {
    let body = NoteBody::parse(&cmd.body)?;

    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(NoteError::NotFound)?;
    let row = queries::lock_note_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.note_id)
        .await?
        .ok_or(NoteError::NotFound)?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;

    if !permitted(role, ctx.actor_user_id, &row) {
        return Err(NoteError::Forbidden);
    }

    if row.body == body {
        tx.commit().await?;
        return Ok(EditNoteOutcome {
            note: note_view(&row, cmd.person_id, true),
            changed: false,
        });
    }

    let updated_at = queries::update_note_body(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.note_id,
        &body,
    )
    .await?;
    tx.commit().await?;

    publish_note_changed(publisher, ctx, cmd.person_id).await;

    let mut updated_row = row;
    updated_row.body = body;
    updated_row.updated_at = updated_at;
    Ok(EditNoteOutcome {
        note: note_view(&updated_row, cmd.person_id, true),
        changed: true,
    })
}

// --- DeleteNote ----------------------------------------------------------

pub struct DeleteNote {
    pub person_id: PersonId,
    pub note_id: NoteId,
}

#[derive(Debug, Clone)]
pub struct DeleteNoteOutcome {
    pub deleted: bool,
}

#[tracing::instrument(
    name = "note.delete",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        note_id = %cmd.note_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn delete_note(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: DeleteNote,
) -> Result<DeleteNoteOutcome, NoteError> {
    let result = delete_note_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "deleted");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn delete_note_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: DeleteNote,
) -> Result<DeleteNoteOutcome, NoteError> {
    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(NoteError::NotFound)?;
    let row = queries::lock_note_for_update(&mut tx, ctx.organization_id, cmd.person_id, cmd.note_id)
        .await?
        .ok_or(NoteError::NotFound)?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;

    if !permitted(role, ctx.actor_user_id, &row) {
        return Err(NoteError::Forbidden);
    }

    queries::tombstone_note(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.note_id,
        ctx.actor_user_id,
    )
    .await?;
    tx.commit().await?;

    publish_note_changed(publisher, ctx, cmd.person_id).await;

    Ok(DeleteNoteOutcome { deleted: true })
}

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::person::model::UserRef;
use crate::ids::{NoteId, OrganizationId, PersonId, UserId};

use super::error::NoteError;

/// A full note row for command-internal lookups (edit/delete's
/// `FOR UPDATE` load). Kept separate from the public [`super::model::Note`]
/// response shape, mirroring `tag::queries`'s `TagRowFull`/`Tag` split.
/// `pub(crate)`, like the row struct it mirrors — never carries anything
/// that must be logged (the body is a plain field like every other row
/// type; the discipline is "never log", not "never hold in memory").
pub(crate) struct NoteRowFull {
    pub id: NoteId,
    pub author_user_id: Option<UserId>,
    pub author_display_name: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

struct NoteRowFullDb {
    id: Uuid,
    author_user_id: Option<Uuid>,
    author_display_name: Option<String>,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<NoteRowFullDb> for NoteRowFull {
    fn from(value: NoteRowFullDb) -> Self {
        NoteRowFull {
            id: NoteId::new(value.id),
            author_user_id: value.author_user_id.map(UserId::new),
            author_display_name: value.author_display_name,
            body: value.body,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// `EditNote`/`DeleteNote`'s lookup and lock (docs/specs/SLICE_015.md §3):
/// binds `id`, `organization_id` **and** `person_id` — a note reached
/// through another Person's path in the same Organization is invisible
/// here, byte-identical to a foreign or nonexistent id. Excludes
/// tombstones (`deleted_at IS NULL`): a second delete, or any write to a
/// tombstone, is `NotFound`, identical to a nonexistent id. `FOR UPDATE OF
/// n` scopes the row lock to `note` only — the `LEFT JOIN app_user` side
/// is read, never locked.
pub(crate) async fn lock_note_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    note_id: NoteId,
) -> Result<Option<NoteRowFull>, NoteError> {
    let row = sqlx::query_as!(
        NoteRowFullDb,
        r#"SELECT n.id, n.author_user_id, au.display_name as "author_display_name?",
                  n.body, n.created_at, n.updated_at
           FROM note n
           LEFT JOIN app_user au ON au.id = n.author_user_id
           WHERE n.id = $1 AND n.organization_id = $2 AND n.person_id = $3
             AND n.deleted_at IS NULL
           FOR UPDATE OF n"#,
        note_id.0,
        organization_id.0,
        person_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(NoteRowFull::from))
}

struct InsertedNoteRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    display_name: String,
}

/// `AddNote`'s insert (docs/specs/SLICE_015.md §3): the author's own
/// display name is joined in the same round trip — the actor is always an
/// active member at this point (`AuthContext` already established it), so
/// the join is expected to always hit.
pub(crate) async fn insert_note(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    author_user_id: UserId,
    body: &str,
    origin: &str,
    correlation_id: Uuid,
) -> Result<(NoteId, DateTime<Utc>, DateTime<Utc>, String), NoteError> {
    let row = sqlx::query_as!(
        InsertedNoteRow,
        r#"WITH inserted AS (
               INSERT INTO note (organization_id, person_id, author_user_id, body, origin, correlation_id)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, created_at, updated_at
           )
           SELECT inserted.id, inserted.created_at, inserted.updated_at, au.display_name
           FROM inserted
           JOIN app_user au ON au.id = $3"#,
        organization_id.0,
        person_id.0,
        author_user_id.0,
        body,
        origin,
        correlation_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((
        NoteId::new(row.id),
        row.created_at,
        row.updated_at,
        row.display_name,
    ))
}

/// `EditNote`'s update on a genuine change (docs/specs/SLICE_015.md §3):
/// `updated_at = now()`, everything else about the row untouched. Returns
/// the fresh `updated_at`.
pub(crate) async fn update_note_body(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    note_id: NoteId,
    body: &str,
) -> Result<DateTime<Utc>, NoteError> {
    let row = sqlx::query!(
        r#"UPDATE note SET body = $4, updated_at = now()
           WHERE id = $1 AND organization_id = $2 AND person_id = $3
           RETURNING updated_at"#,
        note_id.0,
        organization_id.0,
        person_id.0,
        body,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.updated_at)
}

/// `DeleteNote`'s tombstone (docs/specs/SLICE_015.md §1 rule 3, §3): body
/// emptied, `deleted_at`/`deleted_by_user_id` stamped, `updated_at`
/// deliberately **not** touched — every other column, including the
/// import provenance, stays byte-identical.
pub(crate) async fn tombstone_note(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    note_id: NoteId,
    deleted_by_user_id: UserId,
) -> Result<(), NoteError> {
    sqlx::query!(
        r#"UPDATE note SET body = '', deleted_at = now(), deleted_by_user_id = $4
           WHERE id = $1 AND organization_id = $2 AND person_id = $3"#,
        note_id.0,
        organization_id.0,
        person_id.0,
        deleted_by_user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// --- History and Operator reads --------------------------------------

/// One live note, in the shape `history_for_person`'s `note` kind needs
/// (docs/specs/SLICE_015.md §5). `pub(crate)` — an internal wiring type
/// between `domain::note` and `domain::person::queries`, not part of
/// either module's public (cross-crate) surface.
pub(crate) struct NoteHistoryRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub actor: Option<UserRef>,
    pub origin: String,
    pub correlation_id: Uuid,
    pub body: String,
}

struct NoteHistoryRowDb {
    id: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    origin: String,
    correlation_id: Uuid,
    author_user_id: Option<Uuid>,
    author_display_name: Option<String>,
    body: String,
}

/// `history_for_person`'s eighth source (docs/specs/SLICE_015.md §5):
/// every live note for one Person, ordered `(created_at, id)`. Tombstones
/// are excluded (`deleted_at IS NULL`). An unmatched imported author
/// (`author_user_id IS NULL`) renders with `actor: None` — never a
/// decode failure, unlike `correspondence_captured`'s always-set agent:
/// a NULL author here is a normal, documented shape (§1 rule 1, §2), not
/// a data-integrity surprise.
pub(crate) async fn note_history(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<NoteHistoryRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        NoteHistoryRowDb,
        r#"SELECT n.id, n.created_at, n.updated_at, n.origin, n.correlation_id,
                  n.author_user_id, au.display_name as "author_display_name?",
                  n.body
           FROM note n
           LEFT JOIN app_user au ON au.id = n.author_user_id
           WHERE n.organization_id = $1 AND n.person_id = $2 AND n.deleted_at IS NULL
           ORDER BY n.created_at, n.id"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| NoteHistoryRow {
            id: r.id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            actor: match (r.author_user_id, r.author_display_name) {
                (Some(id), Some(display_name)) => Some(UserRef {
                    id: UserId::new(id),
                    display_name,
                }),
                _ => None,
            },
            origin: r.origin,
            correlation_id: r.correlation_id,
            body: r.body,
        })
        .collect())
}

/// One note in the Operator's `PersonDetail.notes` shape (docs/specs/
/// SLICE_015.md §5): `{author_display_name, created_at, body}` — exactly
/// what `crm_operator::NoteView` needs, so the caller performs no further
/// row shaping.
pub struct NoteSummary {
    pub author_display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub body: String,
}

struct NoteSummaryDb {
    author_display_name: Option<String>,
    created_at: DateTime<Utc>,
    body: String,
}

/// The Operator's `get_person` read (docs/specs/SLICE_015.md §3, §5, §8):
/// the latest `limit` live notes, in creation order (oldest of the
/// selected window first) — the `MAX_INQUIRIES` precedent. Ordered and
/// limited in SQL (`ORDER BY created_at DESC, id DESC LIMIT $3`, served by
/// `note_org_person_created_idx`), then reversed in Rust back to ascending
/// creation order.
pub async fn latest_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    limit: i64,
) -> Result<Vec<NoteSummary>, NoteError> {
    let mut workspace_read = crate::auth::workspace::read(conn, organization_id).await?;
    let conn = &mut *workspace_read;
    let mut rows = sqlx::query_as!(
        NoteSummaryDb,
        r#"SELECT au.display_name as "author_display_name?", n.created_at, n.body
           FROM note n
           LEFT JOIN app_user au ON au.id = n.author_user_id
           WHERE n.organization_id = $1 AND n.person_id = $2 AND n.deleted_at IS NULL
           ORDER BY n.created_at DESC, n.id DESC
           LIMIT $3"#,
        organization_id.0,
        person_id.0,
        limit,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.reverse();
    Ok(rows
        .into_iter()
        .map(|r| NoteSummary {
            author_display_name: r.author_display_name,
            created_at: r.created_at,
            body: r.body,
        })
        .collect())
}

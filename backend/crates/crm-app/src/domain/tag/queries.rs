use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use crate::ids::{OrganizationId, PersonId, TagId, UserId};

use super::error::TagError;
use super::model::TagRef;

/// The `list_for_organization` row: `person_count` and `created_by_user_id`
/// let the caller (the `GET /api/tags` route) compute each row's
/// per-viewer `can_manage` (docs/specs/SLICE_011e.md §3, §5).
#[derive(Debug, Clone)]
pub struct TagRow {
    pub id: TagId,
    pub name: String,
    pub person_count: i64,
    pub created_by_user_id: UserId,
}

struct TagRowDb {
    id: Uuid,
    name: String,
    person_count: i64,
    created_by_user_id: Uuid,
}

/// Full index, ordered `lower(name), id` (docs/specs/SLICE_011e.md §5).
/// Unpaginated — the 200-tag-per-Organization cap (§3) already bounds this
/// result.
pub async fn list_for_organization(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Vec<TagRow>, TagError> {
    let rows = sqlx::query_as!(
        TagRowDb,
        r#"SELECT t.id, t.name,
                  count(pt.person_id) as "person_count!",
                  t.created_by_user_id
           FROM tag t
           LEFT JOIN person_tag pt
             ON pt.tag_id = t.id AND pt.organization_id = t.organization_id
           WHERE t.organization_id = $1
           GROUP BY t.id, t.name, t.created_by_user_id
           ORDER BY lower(t.name), t.id"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TagRow {
            id: TagId::new(r.id),
            name: r.name,
            person_count: r.person_count,
            created_by_user_id: UserId::new(r.created_by_user_id),
        })
        .collect())
}

struct TagRefDb {
    id: Uuid,
    name: String,
}

/// A Person's full tag list, ordered `lower(name), id`
/// (docs/specs/SLICE_011e.md §3, §5) — used by the detail read, the
/// Operator view, and the `AddPersonTag`/`RemovePersonTag` response.
pub async fn list_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<TagRef>, TagError> {
    let rows = sqlx::query_as!(
        TagRefDb,
        r#"SELECT t.id, t.name
           FROM tag t
           JOIN person_tag pt
             ON pt.tag_id = t.id AND pt.organization_id = t.organization_id
           WHERE pt.organization_id = $1 AND pt.person_id = $2
           ORDER BY lower(t.name), t.id"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TagRef {
            id: TagId::new(r.id),
            name: r.name,
        })
        .collect())
}

/// The `stage::exists` twin (docs/specs/SLICE_011e.md §3): used by the e2
/// filter's reference validation (`tag_ids` -> `invalid_tag`).
pub async fn exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<bool, TagError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM tag WHERE id = $1 AND organization_id = $2"#,
        tag_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

/// Bulk name resolution for `FilterNames.tag_names` (e2, docs/specs/
/// SLICE_011e.md §4): an id absent from the result (deleted or foreign) is
/// left for the caller to render as "an unknown tag".
pub async fn names_for(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    ids: &[TagId],
) -> Result<HashMap<TagId, String>, TagError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let raw_ids: Vec<Uuid> = ids.iter().map(|id| id.0).collect();
    let rows = sqlx::query_as!(
        TagRefDb,
        r#"SELECT id, name FROM tag WHERE organization_id = $1 AND id = ANY($2)"#,
        organization_id.0,
        &raw_ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (TagId::new(r.id), r.name))
        .collect())
}

/// A full tag row for command-internal lookups (create-or-get, rename,
/// delete). Kept separate from the public [`TagRow`]/[`super::model::Tag`]
/// shapes, mirroring `saved_list`'s `SavedListStoredRow`/`SavedListMetadata`
/// split.
pub(crate) struct TagRowFull {
    pub id: TagId,
    pub name: String,
    pub created_by_user_id: UserId,
}

struct TagRowFullDb {
    id: Uuid,
    name: String,
    created_by_user_id: Uuid,
}

impl From<TagRowFullDb> for TagRowFull {
    fn from(value: TagRowFullDb) -> Self {
        TagRowFull {
            id: TagId::new(value.id),
            name: value.name,
            created_by_user_id: UserId::new(value.created_by_user_id),
        }
    }
}

/// `CreateTag`'s create-or-get lookup (docs/specs/SLICE_011e.md §3): plain
/// `SELECT`, no row lock — the `tags:<org>` advisory lock already
/// serializes every writer in the Organization, so no concurrent insert
/// can race this read.
pub(crate) async fn find_by_lower_name(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    name: &str,
) -> Result<Option<TagRowFull>, TagError> {
    let row = sqlx::query_as!(
        TagRowFullDb,
        r#"SELECT id, name, created_by_user_id
           FROM tag
           WHERE organization_id = $1 AND lower(name) = lower($2)"#,
        organization_id.0,
        name,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(TagRowFull::from))
}

/// `RenameTag`/`DeleteTag`'s `FOR UPDATE` load (docs/specs/SLICE_011e.md
/// §3): waits for any in-flight `AddPersonTag`/`RemovePersonTag` holding
/// the row `FOR SHARE`, so the usage-count read taken right after is exact.
pub(crate) async fn lock_tag_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<Option<TagRowFull>, TagError> {
    let row = sqlx::query_as!(
        TagRowFullDb,
        r#"SELECT id, name, created_by_user_id
           FROM tag
           WHERE id = $1 AND organization_id = $2
           FOR UPDATE"#,
        tag_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(TagRowFull::from))
}

/// `AddPersonTag`/`RemovePersonTag`'s `FOR SHARE` existence check
/// (docs/specs/SLICE_011e.md §3): makes a concurrent `DeleteTag` wait
/// behind this command instead of turning the insert into an FK failure.
pub(crate) async fn lock_tag_for_share_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<bool, TagError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM tag
           WHERE id = $1 AND organization_id = $2
           FOR SHARE"#,
        tag_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

/// A live tag other than `exclude_id` with the same case-insensitive name
/// (`RenameTag`'s collision check, docs/specs/SLICE_011e.md §3).
pub(crate) async fn other_tag_with_lower_name_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    name: &str,
    exclude_id: TagId,
) -> Result<bool, TagError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM tag
           WHERE organization_id = $1 AND lower(name) = lower($2) AND id <> $3"#,
        organization_id.0,
        name,
        exclude_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

/// Live tag count for the 200-per-Organization cap (§3).
pub(crate) async fn count_live_tags(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<i64, TagError> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM tag WHERE organization_id = $1"#,
        organization_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.count)
}

/// `person_tag` row count for one tag (rule-1 "unused" check and each
/// row's `person_count`, §3). Served by `person_tag_org_tag_person_idx`.
pub(crate) async fn count_person_tags_for_tag(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<i64, TagError> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM person_tag
           WHERE organization_id = $1 AND tag_id = $2"#,
        organization_id.0,
        tag_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.count)
}

/// `person_tag` row count for one Person (the 20-per-Person cap, §3).
/// Served by the `person_tag` primary key.
pub(crate) async fn count_person_tags_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<i64, TagError> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM person_tag
           WHERE organization_id = $1 AND person_id = $2"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.count)
}

pub(crate) async fn person_tag_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    tag_id: TagId,
) -> Result<bool, TagError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM person_tag
           WHERE organization_id = $1 AND person_id = $2 AND tag_id = $3"#,
        organization_id.0,
        person_id.0,
        tag_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

pub(crate) async fn insert_tag(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    created_by_user_id: UserId,
    name: &str,
) -> Result<TagRowFull, TagError> {
    let row = sqlx::query_as!(
        TagRowFullDb,
        r#"INSERT INTO tag (organization_id, created_by_user_id, name)
           VALUES ($1, $2, $3)
           RETURNING id, name, created_by_user_id"#,
        organization_id.0,
        created_by_user_id.0,
        name,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(TagRowFull::from(row))
}

pub(crate) async fn update_tag_name(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
    name: &str,
) -> Result<(), TagError> {
    sqlx::query!(
        r#"UPDATE tag SET name = $3, updated_at = now()
           WHERE id = $1 AND organization_id = $2"#,
        tag_id.0,
        organization_id.0,
        name,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Explicit `person_tag` cleanup ahead of the `tag` row delete (rule 4,
/// docs/specs/SLICE_011e.md §3) — the FK `ON DELETE CASCADE` stays as
/// belt, this is the counted removal the response reports.
pub(crate) async fn delete_person_tag_rows_for_tag(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<i64, TagError> {
    let result = sqlx::query!(
        r#"DELETE FROM person_tag WHERE organization_id = $1 AND tag_id = $2"#,
        organization_id.0,
        tag_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() as i64)
}

pub(crate) async fn delete_tag_row(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    tag_id: TagId,
) -> Result<(), TagError> {
    sqlx::query!(
        r#"DELETE FROM tag WHERE id = $1 AND organization_id = $2"#,
        tag_id.0,
        organization_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `ON CONFLICT DO NOTHING` (docs/specs/SLICE_011e.md §3): returns whether
/// a row was actually inserted, so the caller publishes `tags_changed`
/// only on a real change (spec §5, §6).
pub(crate) async fn insert_person_tag(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    tag_id: TagId,
    added_by_user_id: UserId,
) -> Result<bool, TagError> {
    let result = sqlx::query!(
        r#"INSERT INTO person_tag (organization_id, person_id, tag_id, added_by_user_id)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT DO NOTHING"#,
        organization_id.0,
        person_id.0,
        tag_id.0,
        added_by_user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn delete_person_tag(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    tag_id: TagId,
) -> Result<bool, TagError> {
    let result = sqlx::query!(
        r#"DELETE FROM person_tag
           WHERE organization_id = $1 AND person_id = $2 AND tag_id = $3"#,
        organization_id.0,
        person_id.0,
        tag_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

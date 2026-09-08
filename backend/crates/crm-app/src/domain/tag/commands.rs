use chrono::Utc;
use sqlx::{PgConnection, PgPool};

use crate::domain::admin::Role;
use crate::domain::envelope::CommandContext;
use crate::domain::person::queries as person_queries;
use crate::ids::{OrganizationId, PersonId, TagId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

use super::error::TagError;
use super::model::{can_manage, Tag, TagRef};
use super::queries::{self, TagRowFull};

/// The person_tag/tag_org_lower_name_key rule-2 shape (trim, 1–40 chars, no
/// control characters — docs/specs/SLICE_011e.md §1 rule 2, the
/// saved-list name rule with a shorter cap because tags render as chips).
/// Shared by every command entry point so a direct Rust caller cannot
/// bypass it, mirroring `saved_list::normalize_and_validate_input`.
pub fn normalize_and_validate_name(raw: &str) -> Result<String, TagError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(TagError::MalformedRequest);
    }
    Ok(name.to_owned())
}

/// One namespace for all tag writes in an Organization (docs/specs/
/// SLICE_011e.md §3): makes `CreateTag`'s create-or-get atomic with the
/// 200-tag quota check, and gives create/rename/delete a single lock
/// order so a rename cannot race a create into a duplicate.
async fn acquire_tags_lock(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), TagError> {
    let organization_id_text = organization_id.to_string();
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended('tags:' || $1::text, 0))"#,
        organization_id_text,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `RenameTag`/`DeleteTag`'s membership re-check (docs/specs/SLICE_011e.md
/// §3, §6): a `FOR SHARE` re-read of the actor's OWN membership row,
/// inside the same transaction as the tag row lock, so a concurrent
/// demotion or deactivation cannot interleave with the rule-1 permission
/// decision. Returns `None` for a missing or inactive membership — the
/// actor already holds a valid authenticated session (`AuthContext`), so
/// this is a concurrent-demotion defense, not an authentication check;
/// the caller folds `None` into the same `Forbidden` every other failed
/// verdict produces (spec §9.4: "admin demoted or deactivated inside the
/// transaction: 403 and no write").
async fn lock_current_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Option<Role>, TagError> {
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
    Ok(Some(Role::from_db_str(&row.role).ok_or(TagError::Corrupt)?))
}

/// A non-enforcing role read for shaping a response's `can_manage` hint
/// (docs/specs/SLICE_011e.md §5: "a display hint; the command re-decides
/// under the row lock"). Unlike [`lock_current_membership`], this never
/// gates a write, so it takes no lock and treats a missing/inactive
/// membership as `Member` (the conservative "not visibly manageable"
/// default) rather than failing the whole command over a display detail.
async fn actor_role_hint(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Role, TagError> {
    let row = sqlx::query!(
        r#"SELECT role, status FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(row) if row.status == "active" => {
            Role::from_db_str(&row.role).ok_or(TagError::Corrupt)
        }
        _ => Ok(Role::Member),
    }
}

fn tag_view(row: &TagRowFull, person_count: i64, role: Role, actor_user_id: UserId) -> Tag {
    Tag {
        id: row.id,
        name: row.name.clone(),
        person_count,
        can_manage: can_manage(role, actor_user_id, row.created_by_user_id, person_count),
    }
}

fn record_outcome<T>(result: &Result<T, TagError>) {
    match result {
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "tag command failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
}

// --- CreateTag -------------------------------------------------------------

pub struct CreateTag {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct CreateTagOutcome {
    pub tag: Tag,
    pub created: bool,
}

#[tracing::instrument(
    name = "tag.create",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn create_tag(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateTag,
) -> Result<CreateTagOutcome, TagError> {
    let result = create_tag_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.created {
                    "created"
                } else {
                    "existing"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn create_tag_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateTag,
) -> Result<CreateTagOutcome, TagError> {
    let name = normalize_and_validate_name(&cmd.name)?;

    let mut tx = pool.begin().await?;
    // The membership check some sibling commands perform is unnecessary
    // here: creation is "any active member", which `AuthContext` already
    // established (docs/specs/SLICE_011e.md §3 table).
    acquire_tags_lock(&mut tx, ctx.organization_id).await?;

    if let Some(existing) = queries::find_by_lower_name(&mut tx, ctx.organization_id, &name).await?
    {
        // The first spelling is kept (rule 2): an existing hit is
        // returned verbatim, never overwritten with the new spelling.
        let person_count =
            queries::count_person_tags_for_tag(&mut tx, ctx.organization_id, existing.id).await?;
        let role = actor_role_hint(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
        tx.commit().await?;
        return Ok(CreateTagOutcome {
            tag: tag_view(&existing, person_count, role, ctx.actor_user_id),
            created: false,
        });
    }

    let count = queries::count_live_tags(&mut tx, ctx.organization_id).await?;
    if count >= 200 {
        return Err(TagError::TagLimitReached);
    }

    let inserted =
        queries::insert_tag(&mut tx, ctx.organization_id, ctx.actor_user_id, &name).await?;
    tx.commit().await?;
    Ok(CreateTagOutcome {
        // The creator can always manage their brand-new, unused tag,
        // independent of their Organization role.
        tag: Tag {
            id: inserted.id,
            name: inserted.name,
            person_count: 0,
            can_manage: true,
        },
        created: true,
    })
}

// --- AddPersonTag / RemovePersonTag ----------------------------------------

pub struct AddPersonTag {
    pub person_id: PersonId,
    pub tag_id: TagId,
}

pub struct RemovePersonTag {
    pub person_id: PersonId,
    pub tag_id: TagId,
}

#[derive(Debug, Clone)]
pub struct PersonTagOutcome {
    pub tags: Vec<TagRef>,
    pub changed: bool,
}

#[tracing::instrument(
    name = "person_tag.add",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        tag_id = %cmd.tag_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn add_person_tag(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: AddPersonTag,
) -> Result<PersonTagOutcome, TagError> {
    let result = add_person_tag_attempt(pool, publisher, ctx, cmd).await;
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

async fn add_person_tag_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: AddPersonTag,
) -> Result<PersonTagOutcome, TagError> {
    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(TagError::NotFound)?;
    let tag_present =
        queries::lock_tag_for_share_exists(&mut tx, ctx.organization_id, cmd.tag_id).await?;
    if !tag_present {
        return Err(TagError::NotFound);
    }

    let already_applied =
        queries::person_tag_exists(&mut tx, ctx.organization_id, cmd.person_id, cmd.tag_id).await?;
    if !already_applied {
        let count =
            queries::count_person_tags_for_person(&mut tx, ctx.organization_id, cmd.person_id)
                .await?;
        if count >= 20 {
            return Err(TagError::PersonTagLimitReached);
        }
    }

    let changed = queries::insert_person_tag(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.tag_id,
        ctx.actor_user_id,
    )
    .await?;
    let tags = queries::list_for_person(&mut tx, ctx.organization_id, cmd.person_id).await?;
    tx.commit().await?;

    if changed {
        publish_tags_changed(publisher, ctx, cmd.person_id).await;
    }
    Ok(PersonTagOutcome { tags, changed })
}

#[tracing::instrument(
    name = "person_tag.remove",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        tag_id = %cmd.tag_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn remove_person_tag(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: RemovePersonTag,
) -> Result<PersonTagOutcome, TagError> {
    let result = remove_person_tag_attempt(pool, publisher, ctx, cmd).await;
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

async fn remove_person_tag_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: RemovePersonTag,
) -> Result<PersonTagOutcome, TagError> {
    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(TagError::NotFound)?;
    let tag_present =
        queries::lock_tag_for_share_exists(&mut tx, ctx.organization_id, cmd.tag_id).await?;
    if !tag_present {
        return Err(TagError::NotFound);
    }

    let changed =
        queries::delete_person_tag(&mut tx, ctx.organization_id, cmd.person_id, cmd.tag_id).await?;
    let tags = queries::list_for_person(&mut tx, ctx.organization_id, cmd.person_id).await?;
    tx.commit().await?;

    if changed {
        publish_tags_changed(publisher, ctx, cmd.person_id).await;
    }
    Ok(PersonTagOutcome { tags, changed })
}

async fn publish_tags_changed(publisher: &Publisher, ctx: &CommandContext, person_id: PersonId) {
    let event = RealtimeEvent::person_changed(
        ctx.organization_id,
        Utc::now(),
        ctx.correlation_id,
        person_id,
        PersonChange::TagsChanged,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;
}

// --- RenameTag ---------------------------------------------------------------

pub struct RenameTag {
    pub tag_id: TagId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RenameTagOutcome {
    pub tag: Tag,
    pub changed: bool,
}

/// Rule 1 (D-051): admin, or creator while `person_count == 0`. `role` is
/// `None` for a missing/inactive membership (concurrent removal/
/// deactivation) — always `Forbidden`, never a different code, so a
/// demoted or deactivated actor cannot distinguish "you no longer belong"
/// from "you never had the right" (docs/specs/SLICE_011e.md §6).
fn permitted(
    role: Option<Role>,
    actor_user_id: UserId,
    row: &TagRowFull,
    person_count: i64,
) -> bool {
    match role {
        Some(role) => can_manage(role, actor_user_id, row.created_by_user_id, person_count),
        None => false,
    }
}

#[tracing::instrument(
    name = "tag.rename",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        tag_id = %cmd.tag_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn rename_tag(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: RenameTag,
) -> Result<RenameTagOutcome, TagError> {
    let result = rename_tag_attempt(pool, ctx, cmd).await;
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

async fn rename_tag_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: RenameTag,
) -> Result<RenameTagOutcome, TagError> {
    let name = normalize_and_validate_name(&cmd.name)?;

    let mut tx = pool.begin().await?;
    acquire_tags_lock(&mut tx, ctx.organization_id).await?;
    let row = queries::lock_tag_for_update(&mut tx, ctx.organization_id, cmd.tag_id)
        .await?
        .ok_or(TagError::NotFound)?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    let person_count =
        queries::count_person_tags_for_tag(&mut tx, ctx.organization_id, cmd.tag_id).await?;

    if !permitted(role, ctx.actor_user_id, &row, person_count) {
        return Err(TagError::Forbidden);
    }
    let role = role.expect("permitted() requires Some(role)");

    // Byte-equal to the current name after trim: a true no-op, nothing
    // written. A case-only rename ("investor" -> "Investor") of the SAME
    // tag differs here and falls through to a real update (rule 2).
    if row.name == name {
        tx.commit().await?;
        return Ok(RenameTagOutcome {
            tag: tag_view(&row, person_count, role, ctx.actor_user_id),
            changed: false,
        });
    }
    if queries::other_tag_with_lower_name_exists(&mut tx, ctx.organization_id, &name, cmd.tag_id)
        .await?
    {
        return Err(TagError::TagNameTaken);
    }

    queries::update_tag_name(&mut tx, ctx.organization_id, cmd.tag_id, &name).await?;
    tx.commit().await?;
    Ok(RenameTagOutcome {
        tag: Tag {
            id: row.id,
            name,
            person_count,
            can_manage: true,
        },
        changed: true,
    })
}

// --- DeleteTag ---------------------------------------------------------------

pub struct DeleteTag {
    pub tag_id: TagId,
}

#[derive(Debug, Clone)]
pub struct DeleteTagOutcome {
    pub deleted: bool,
    pub removed_from_people: i64,
}

#[tracing::instrument(
    name = "tag.delete",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        tag_id = %cmd.tag_id,
        removed_count = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn delete_tag(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DeleteTag,
) -> Result<DeleteTagOutcome, TagError> {
    let result = delete_tag_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record("removed_count", outcome.removed_from_people);
            tracing::Span::current().record("outcome", "deleted");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn delete_tag_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DeleteTag,
) -> Result<DeleteTagOutcome, TagError> {
    let mut tx = pool.begin().await?;
    acquire_tags_lock(&mut tx, ctx.organization_id).await?;
    let row = queries::lock_tag_for_update(&mut tx, ctx.organization_id, cmd.tag_id)
        .await?
        .ok_or(TagError::NotFound)?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    let person_count =
        queries::count_person_tags_for_tag(&mut tx, ctx.organization_id, cmd.tag_id).await?;

    if !permitted(role, ctx.actor_user_id, &row, person_count) {
        return Err(TagError::Forbidden);
    }

    // Rule 4: explicit, counted removal ahead of the tag delete. The FK
    // `ON DELETE CASCADE` stays as belt; no referencing definition is
    // rewritten.
    let removed_from_people =
        queries::delete_person_tag_rows_for_tag(&mut tx, ctx.organization_id, cmd.tag_id).await?;
    queries::delete_tag_row(&mut tx, ctx.organization_id, cmd.tag_id).await?;
    tx.commit().await?;

    Ok(DeleteTagOutcome {
        deleted: true,
        removed_from_people,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_validate_name_trims_and_bounds() {
        assert_eq!(
            normalize_and_validate_name("  Investor  ").unwrap(),
            "Investor"
        );
        assert!(normalize_and_validate_name("").is_err());
        assert!(normalize_and_validate_name("   ").is_err());
        assert!(normalize_and_validate_name(&"a".repeat(41)).is_err());
        assert!(normalize_and_validate_name(&"a".repeat(40)).is_ok());
        assert!(normalize_and_validate_name("bad\ttab").is_err());
        assert!(normalize_and_validate_name("bad\nline").is_err());
    }
}

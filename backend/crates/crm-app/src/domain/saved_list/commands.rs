use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::admin::Role;
use crate::domain::envelope::CommandContext;
use crate::domain::person::filter::FilterDefinition;
use crate::ids::{SavedListId, UserId};

use super::error::SavedListError;
use super::queries::{
    metadata_for, visible_live_row_for_update, SavedListMetadata, SavedListScope,
    SavedListStoredRow, SavedListStoredRowDb,
};

/// JavaScript's exact-integer ceiling, frozen by the HTTP contract. Database
/// revisions are BIGINT, but a server increment beyond this value must fail
/// rather than return a value a client cannot faithfully round-trip.
pub const MAX_WIRE_REVISION: i64 = 9_007_199_254_740_991;

pub struct CreateSavedList {
    pub request_id: Uuid,
    pub scope: SavedListScope,
    pub name: String,
    pub filter: FilterDefinition,
}

pub struct UpdateSavedList {
    pub list_id: SavedListId,
    pub expected_revision: i64,
    pub name: String,
    pub filter: FilterDefinition,
}

pub struct DeleteSavedList {
    pub list_id: SavedListId,
    pub expected_revision: i64,
}

#[derive(Debug, Clone)]
pub struct CreateSavedListOutcome {
    pub list: SavedListMetadata,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateSavedListOutcome {
    pub list: SavedListMetadata,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct DeleteSavedListOutcome {
    pub deleted: bool,
}

/// Shared structural validation used by HTTP before the command (to retain
/// 400-before-authorization ordering) and repeated by every command entry
/// point so direct Rust callers cannot bypass it.
pub fn normalize_and_validate_input(
    raw_name: &str,
    filter: &FilterDefinition,
) -> Result<String, SavedListError> {
    let name = raw_name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(SavedListError::MalformedRequest);
    }
    // `FilterDefinition`'s serde decoder checks its version on wire input,
    // but a public Rust caller can build the struct directly. Keep this
    // explicit before `.validate()` per Slice 011b A1.
    if filter.version != 1 {
        return Err(SavedListError::MalformedRequest);
    }
    filter.validate().map_err(SavedListError::from)?;
    Ok(name.to_owned())
}

pub fn validate_expected_revision(revision: i64) -> Result<(), SavedListError> {
    if !(1..=MAX_WIRE_REVISION).contains(&revision) {
        return Err(SavedListError::MalformedRequest);
    }
    Ok(())
}

#[tracing::instrument(
    name = "saved_list.create",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        scope = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn create_saved_list(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateSavedList,
) -> Result<CreateSavedListOutcome, SavedListError> {
    tracing::Span::current().record("scope", cmd.scope.as_str());
    let result = create_saved_list_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.created {
                    "created"
                } else {
                    "replayed"
                },
            );
        }
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "saved-list create failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
    result
}

async fn create_saved_list_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateSavedList,
) -> Result<CreateSavedListOutcome, SavedListError> {
    let normalized_name = normalize_and_validate_input(&cmd.name, &cmd.filter)?;
    let fingerprint = creation_fingerprint(cmd.scope, &normalized_name, &cmd.filter)?;
    let filter_value =
        serde_json::to_value(&cmd.filter).map_err(|_| SavedListError::MalformedRequest)?;
    let filter_json =
        serde_json::to_string(&filter_value).map_err(|_| SavedListError::MalformedRequest)?;

    let mut tx = pool.begin().await?;
    // Membership is deliberately re-read and share-locked inside this
    // transaction before the advisory lock. An admin demotion/deactivation
    // cannot interleave with authorization and a subsequent write.
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_saved_lists_lock(&mut tx, ctx.organization_id).await?;

    if cmd.scope == SavedListScope::Shared && role != Role::Admin {
        return Err(SavedListError::Forbidden);
    }

    // Exact retry identity is actor- and Organization-scoped, and comes
    // before reference validation/quota so a delayed replay never consumes a
    // second slot or gets reinterpreted against current references.
    if let Some(retry) = retry_row_for_update(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.request_id,
    )
    .await?
    {
        if retry.create_fingerprint != fingerprint {
            return Err(SavedListError::RequestConflict);
        }
        if retry.deleted_at.is_some() {
            return Err(SavedListError::Deleted);
        }
        let row = retry.into_live()?;
        tx.commit().await?;
        return Ok(CreateSavedListOutcome {
            list: metadata_for(&row, ctx.actor_user_id, role),
            created: false,
        });
    }

    cmd.filter
        .validate_references(&mut tx, ctx.organization_id)
        .await
        .map_err(SavedListError::from)?;
    if quota_reached(&mut tx, ctx.organization_id, ctx.actor_user_id, cmd.scope).await? {
        return Err(SavedListError::LimitReached);
    }

    let scope = cmd.scope.as_str();
    let row = sqlx::query_as!(
        SavedListStoredRowDb,
        r#"INSERT INTO saved_list
              (organization_id, created_by_user_id, scope, name, filter,
               create_request_id, create_fingerprint)
           VALUES ($1, $2, $3, $4, CAST($5 AS text)::jsonb, $6, $7)
           RETURNING id, created_by_user_id, scope,
                     name as "name!", filter::text as "filter!",
                     revision, created_at, updated_at"#,
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        scope,
        normalized_name,
        filter_json,
        cmd.request_id,
        &fingerprint,
    )
    .fetch_one(&mut *tx)
    .await?;
    let row = SavedListStoredRow::try_from(row)?;
    tx.commit().await?;

    Ok(CreateSavedListOutcome {
        list: metadata_for(&row, ctx.actor_user_id, role),
        created: true,
    })
}

#[tracing::instrument(
    name = "saved_list.update",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        saved_list_id = %cmd.list_id,
        scope = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn update_saved_list(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateSavedList,
) -> Result<UpdateSavedListOutcome, SavedListError> {
    let result = update_saved_list_attempt(pool, ctx, cmd).await;
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
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "saved-list update failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
    result
}

async fn update_saved_list_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateSavedList,
) -> Result<UpdateSavedListOutcome, SavedListError> {
    validate_expected_revision(cmd.expected_revision)?;
    let normalized_name = normalize_and_validate_input(&cmd.name, &cmd.filter)?;
    let filter_value =
        serde_json::to_value(&cmd.filter).map_err(|_| SavedListError::MalformedRequest)?;
    let filter_json =
        serde_json::to_string(&filter_value).map_err(|_| SavedListError::MalformedRequest)?;

    let mut tx = pool.begin().await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_saved_lists_lock(&mut tx, ctx.organization_id).await?;
    let row =
        visible_live_row_for_update(&mut tx, ctx.organization_id, ctx.actor_user_id, cmd.list_id)
            .await?
            .ok_or(SavedListError::NotFound)?;
    tracing::Span::current().record("scope", row.scope.as_str());

    // The visibility lookup precedes this shared-write rejection so a hidden
    // personal list still looks exactly like a missing resource to admins.
    if row.scope == SavedListScope::Shared && role != Role::Admin {
        return Err(SavedListError::Forbidden);
    }
    if row.revision != cmd.expected_revision {
        return Err(SavedListError::Conflict);
    }
    cmd.filter
        .validate_references(&mut tx, ctx.organization_id)
        .await
        .map_err(SavedListError::from)?;

    if row.name == normalized_name && row.filter.as_ref() == Some(&filter_value) {
        tx.commit().await?;
        return Ok(UpdateSavedListOutcome {
            list: metadata_for(&row, ctx.actor_user_id, role),
            changed: false,
        });
    }
    if row.revision >= MAX_WIRE_REVISION {
        return Err(SavedListError::RevisionExhausted);
    }

    let updated = sqlx::query_as!(
        SavedListStoredRowDb,
        r#"UPDATE saved_list
           SET name = $4, filter = CAST($5 AS text)::jsonb, revision = revision + 1, updated_at = now()
           WHERE id = $1
             AND organization_id = $2
             AND deleted_at IS NULL
             AND revision = $3
             AND (scope = 'shared' OR created_by_user_id = $6)
           RETURNING id, created_by_user_id, scope,
                     name as "name!", filter::text as "filter!",
                     revision, created_at, updated_at"#,
        cmd.list_id.0,
        ctx.organization_id.0,
        cmd.expected_revision,
        normalized_name,
        filter_json,
        ctx.actor_user_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(SavedListError::Conflict)?;
    let updated = SavedListStoredRow::try_from(updated)?;
    tx.commit().await?;
    Ok(UpdateSavedListOutcome {
        list: metadata_for(&updated, ctx.actor_user_id, role),
        changed: true,
    })
}

#[tracing::instrument(
    name = "saved_list.delete",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        saved_list_id = %cmd.list_id,
        scope = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn delete_saved_list(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DeleteSavedList,
) -> Result<DeleteSavedListOutcome, SavedListError> {
    let result = delete_saved_list_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "deleted");
        }
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "saved-list delete failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
    result
}

async fn delete_saved_list_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: DeleteSavedList,
) -> Result<DeleteSavedListOutcome, SavedListError> {
    validate_expected_revision(cmd.expected_revision)?;
    let mut tx = pool.begin().await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_saved_lists_lock(&mut tx, ctx.organization_id).await?;
    let row = visible_row_or_tombstone_for_update(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        cmd.list_id,
    )
    .await?
    .ok_or(SavedListError::NotFound)?;
    let scope = row.scope()?;
    tracing::Span::current().record("scope", scope.as_str());

    // A visible tombstone is the one idempotent resource mutation: it does
    // not compare an old revision or write it a second time. A shared
    // tombstone is deliberately invisible to a non-admin: live shared rows
    // still yield 403 after lookup, but a deleted definition has no mutable
    // resource to disclose or retry without current deletion authority.
    if row.deleted_at.is_some() {
        if scope == SavedListScope::Shared && role != Role::Admin {
            return Err(SavedListError::NotFound);
        }
        tx.commit().await?;
        return Ok(DeleteSavedListOutcome { deleted: true });
    }
    if scope == SavedListScope::Shared && role != Role::Admin {
        return Err(SavedListError::Forbidden);
    }
    if row.revision != cmd.expected_revision {
        return Err(SavedListError::Conflict);
    }
    if row.revision >= MAX_WIRE_REVISION {
        return Err(SavedListError::RevisionExhausted);
    }

    let changed = sqlx::query!(
        r#"UPDATE saved_list
           SET name = NULL, filter = NULL, revision = revision + 1,
               deleted_at = now(), updated_at = now()
           WHERE id = $1
             AND organization_id = $2
             AND deleted_at IS NULL
             AND revision = $3
             AND (scope = 'shared' OR created_by_user_id = $4)"#,
        cmd.list_id.0,
        ctx.organization_id.0,
        cmd.expected_revision,
        ctx.actor_user_id.0,
    )
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(SavedListError::Conflict);
    }
    tx.commit().await?;
    Ok(DeleteSavedListOutcome { deleted: true })
}

/// Membership authorization must be decided inside every mutation
/// transaction; the `FOR SHARE` lock remains held through commit and blocks
/// a concurrent role/status UPDATE from slipping between check and write.
async fn lock_current_membership(
    conn: &mut PgConnection,
    organization_id: crate::ids::OrganizationId,
    actor_user_id: UserId,
) -> Result<Role, SavedListError> {
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
        return Err(SavedListError::Unauthenticated);
    };
    if row.status != "active" {
        return Err(SavedListError::Unauthenticated);
    }
    Role::from_db_str(&row.role).ok_or(SavedListError::Corrupt)
}

/// One namespace for all saved-list writes in an Organization. This makes
/// quota count+insert atomic and gives create/update/delete a single lock
/// order after membership, without contending with admin or intake locks.
async fn acquire_saved_lists_lock(
    conn: &mut PgConnection,
    organization_id: crate::ids::OrganizationId,
) -> Result<(), SavedListError> {
    let organization_id_text = organization_id.to_string();
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended('saved-lists:' || $1::text, 0))"#,
        organization_id_text,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn creation_fingerprint(
    scope: SavedListScope,
    normalized_name: &str,
    filter: &FilterDefinition,
) -> Result<Vec<u8>, SavedListError> {
    // Tuple serializes as the required deterministic JSON array
    // `[scope, normalized_name, filter]`; FilterDefinition's declared field
    // and clause serializers preserve their stable order. This is request
    // equality, deliberately not Boolean-equivalence normalization.
    let bytes = serde_json::to_vec(&(scope.as_str(), normalized_name, filter))
        .map_err(|_| SavedListError::MalformedRequest)?;
    Ok(Sha256::digest(bytes).to_vec())
}

struct RetryRow {
    id: Uuid,
    created_by_user_id: Uuid,
    scope: String,
    name: Option<String>,
    filter: Option<String>,
    revision: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    create_fingerprint: Vec<u8>,
}

impl RetryRow {
    fn into_live(self) -> Result<SavedListStoredRow, SavedListError> {
        if self.deleted_at.is_some() {
            return Err(SavedListError::Deleted);
        }
        let name = self.name.ok_or(SavedListError::Corrupt)?;
        // Replay returns metadata only. A structurally unsupported JSONB
        // payload must neither leak raw content nor break the idempotency
        // guarantee for a live row.
        let filter = serde_json::from_str(&self.filter.ok_or(SavedListError::Corrupt)?).ok();
        Ok(SavedListStoredRow {
            id: SavedListId::new(self.id),
            created_by_user_id: UserId::new(self.created_by_user_id),
            scope: SavedListScope::from_db(&self.scope)?,
            name,
            filter,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

async fn retry_row_for_update(
    conn: &mut PgConnection,
    organization_id: crate::ids::OrganizationId,
    actor_user_id: UserId,
    request_id: Uuid,
) -> Result<Option<RetryRow>, SavedListError> {
    let row = sqlx::query_as!(
        RetryRow,
        r#"SELECT id, created_by_user_id, scope, name,
                  filter::text as "filter: String",
                  revision, created_at, updated_at, deleted_at, create_fingerprint
           FROM saved_list
           WHERE organization_id = $1
             AND created_by_user_id = $2
             AND create_request_id = $3
           FOR UPDATE"#,
        organization_id.0,
        actor_user_id.0,
        request_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

async fn quota_reached(
    conn: &mut PgConnection,
    organization_id: crate::ids::OrganizationId,
    actor_user_id: UserId,
    scope: SavedListScope,
) -> Result<bool, SavedListError> {
    let count = match scope {
        SavedListScope::Personal => {
            sqlx::query!(
                r#"SELECT count(*) as "count!"
                   FROM saved_list
                   WHERE organization_id = $1
                     AND created_by_user_id = $2
                     AND scope = 'personal'
                     AND deleted_at IS NULL"#,
                organization_id.0,
                actor_user_id.0,
            )
            .fetch_one(&mut *conn)
            .await?
            .count
                >= 50
        }
        SavedListScope::Shared => {
            sqlx::query!(
                r#"SELECT count(*) as "count!"
                   FROM saved_list
                   WHERE organization_id = $1
                     AND scope = 'shared'
                     AND deleted_at IS NULL"#,
                organization_id.0,
            )
            .fetch_one(&mut *conn)
            .await?
            .count
                >= 200
        }
    };
    Ok(count)
}

struct DeletableRow {
    scope: String,
    revision: i64,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl DeletableRow {
    fn scope(&self) -> Result<SavedListScope, SavedListError> {
        SavedListScope::from_db(&self.scope)
    }
}

async fn visible_row_or_tombstone_for_update(
    conn: &mut PgConnection,
    organization_id: crate::ids::OrganizationId,
    actor_user_id: UserId,
    list_id: SavedListId,
) -> Result<Option<DeletableRow>, SavedListError> {
    let row = sqlx::query_as!(
        DeletableRow,
        r#"SELECT scope, revision, deleted_at
           FROM saved_list
           WHERE id = $1
             AND organization_id = $2
             AND (scope = 'shared' OR created_by_user_id = $3)
           FOR UPDATE"#,
        list_id.0,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

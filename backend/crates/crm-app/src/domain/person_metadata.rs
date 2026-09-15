//! Atomic metadata edits, composed from the ordinary tag/value commands.
//! The caller owns the transaction and publishes the returned changes only
//! after committing. Receipt identity remains the mobile adapter's concern.
use std::collections::HashSet;

use sqlx::PgConnection;

use crate::{
    domain::{admin::Role, custom_field, envelope::CommandContext, person, tag},
    ids::{CustomFieldId, PersonId, TagId},
};

pub enum MetadataAction {
    AddTag {
        tag_id: TagId,
    },
    RemoveTag {
        tag_id: TagId,
    },
    SetField {
        field_id: CustomFieldId,
        value: custom_field::CustomFieldValue,
    },
    ClearField {
        field_id: CustomFieldId,
    },
}

// Deliberately no Debug: custom values are customer content.
pub struct UpdatePersonMetadata {
    pub person_id: PersonId,
    pub expected_metadata_revision: i64,
    pub expected_catalog_revision: i64,
    pub actions: Vec<MetadataAction>,
}

pub struct MetadataChange {
    pub tags: bool,
    pub fields: bool,
}

#[derive(Debug)]
pub enum MetadataError {
    InvalidMetadata,
    NotFound,
    Forbidden,
    RevisionConflict,
    CatalogRevisionConflict,
    Tag(tag::TagError),
    Field(custom_field::CustomFieldError),
    Database(sqlx::Error),
}
impl From<sqlx::Error> for MetadataError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
impl From<tag::TagError> for MetadataError {
    fn from(error: tag::TagError) -> Self {
        Self::Tag(error)
    }
}
impl From<custom_field::CustomFieldError> for MetadataError {
    fn from(error: custom_field::CustomFieldError) -> Self {
        Self::Field(error)
    }
}

/// Rechecks current authority and both optimistic tokens even for a no-op.
/// All action validation completes before the first write. Person and catalog
/// locks retain those validations through commit; a SQL failure aborts the
/// caller's transaction. Direct callers receive the same workspace/tenant gate.
#[tracing::instrument(name = "person_metadata.update", skip_all, fields(
    organization_id = %ctx.organization_id,
    actor_id = %ctx.actor_user_id,
    person_id = %cmd.person_id,
    correlation_id = %ctx.correlation_id,
))]
pub async fn update_person_metadata_in_transaction(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    cmd: UpdatePersonMetadata,
) -> Result<MetadataChange, MetadataError> {
    crate::auth::workspace::ordinary(conn, ctx.organization_id).await?;
    if cmd.expected_metadata_revision <= 0
        || cmd.expected_catalog_revision <= 0
        || cmd.actions.is_empty()
        || cmd.actions.len() > 50
    {
        return Err(MetadataError::InvalidMetadata);
    }
    let mut tag_ids = HashSet::new();
    let mut field_ids = HashSet::new();
    for action in &cmd.actions {
        let unique = match action {
            MetadataAction::AddTag { tag_id } | MetadataAction::RemoveTag { tag_id } => {
                tag_ids.insert(*tag_id)
            }
            MetadataAction::SetField { field_id, .. } | MetadataAction::ClearField { field_id } => {
                field_ids.insert(*field_id)
            }
        };
        if !unique {
            return Err(MetadataError::InvalidMetadata);
        }
    }
    crate::domain::mobile::metadata::acquire_shared(conn, ctx.organization_id).await?;
    person::queries::lock_person(conn, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(MetadataError::NotFound)?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' FOR SHARE",
    ).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).fetch_optional(&mut *conn).await?;
    if role.as_deref().and_then(Role::from_db_str).is_none() {
        return Err(MetadataError::Forbidden);
    }
    let revision: i64 = sqlx::query_scalar(
        "SELECT metadata_revision FROM person WHERE organization_id=$1 AND id=$2",
    )
    .bind(ctx.organization_id.0)
    .bind(cmd.person_id.0)
    .fetch_one(&mut *conn)
    .await?;
    if revision != cmd.expected_metadata_revision {
        return Err(MetadataError::RevisionConflict);
    }
    let catalog: i64 =
        sqlx::query_scalar("SELECT revision FROM mobile_metadata_catalog WHERE organization_id=$1")
            .bind(ctx.organization_id.0)
            .fetch_one(&mut *conn)
            .await?;
    if catalog != cmd.expected_catalog_revision {
        return Err(MetadataError::CatalogRevisionConflict);
    }

    let mut tags = Vec::new();
    let mut fields = Vec::new();
    for action in cmd.actions {
        match action {
            MetadataAction::AddTag { tag_id } => {
                tags.push(tag::prepare_person_tag(conn, ctx, cmd.person_id, tag_id, true).await?)
            }
            MetadataAction::RemoveTag { tag_id } => {
                tags.push(tag::prepare_person_tag(conn, ctx, cmd.person_id, tag_id, false).await?)
            }
            MetadataAction::SetField { field_id, value } => fields.push(
                custom_field::prepare_set_person_value(
                    conn,
                    ctx,
                    custom_field::SetPersonCustomFieldValue {
                        person_id: cmd.person_id,
                        field_id,
                        value,
                    },
                )
                .await?,
            ),
            MetadataAction::ClearField { field_id } => fields.push(
                custom_field::prepare_clear_person_value(
                    conn,
                    ctx,
                    custom_field::ClearPersonCustomFieldValue {
                        person_id: cmd.person_id,
                        field_id,
                    },
                )
                .await?,
            ),
        }
    }
    tag::validate_person_tag_capacity(conn, ctx, cmd.person_id, &tags).await?;
    let (removals, additions): (Vec<_>, Vec<_>) =
        tags.into_iter().partition(|tag| tag.is_removal());
    let mut changed = MetadataChange {
        tags: false,
        fields: false,
    };
    for prepared in removals.into_iter().chain(additions) {
        changed.tags |= tag::apply_prepared_person_tag(conn, prepared).await?;
    }
    for prepared in fields {
        changed.fields |= custom_field::apply_prepared_person_value(conn, prepared).await?;
    }
    Ok(changed)
}

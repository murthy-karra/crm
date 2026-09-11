use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::admin::{queries as admin_queries, Role};
use crate::domain::custom_field;
use crate::domain::person::filter::{FilterDefinition, FilterError, FilterNames};
use crate::domain::person::queries as person_queries;
use crate::domain::person::sort::{PersonSort, SortDecodeResult};
use crate::domain::person::PersonVisibilityScope;
use crate::domain::stage;
use crate::domain::tag;
use crate::ids::{OrganizationId, SavedListId, UserId};

use super::error::SavedListError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedListScope {
    Personal,
    Shared,
}

impl SavedListScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
        }
    }

    pub(crate) fn from_db(value: &str) -> Result<Self, SavedListError> {
        match value {
            "personal" => Ok(Self::Personal),
            "shared" => Ok(Self::Shared),
            _ => Err(SavedListError::Corrupt),
        }
    }
}

/// The exact public metadata shape. It intentionally omits organization,
/// owner, retry token and filter content.
#[derive(Debug, Clone, Serialize)]
pub struct SavedListMetadata {
    pub id: SavedListId,
    pub name: String,
    pub scope: SavedListScope,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub can_edit: bool,
    pub can_delete: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedListFilterError {
    UnsupportedFilter,
    InvalidStage,
    InvalidAssignee,
    /// docs/specs/SLICE_011e.md §4b.
    InvalidTag,
    InvalidField,
    InvalidOption,
}

impl SavedListFilterError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedFilter => "unsupported_filter",
            Self::InvalidStage => "invalid_stage",
            Self::InvalidAssignee => "invalid_assignee",
            Self::InvalidTag => "invalid_tag",
            Self::InvalidField => "invalid_field",
            Self::InvalidOption => "invalid_option",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SavedListDetail {
    pub list: SavedListMetadata,
    /// `None` only for structurally unsupported stored content. Reference
    /// stale typed definitions remain visible so their writer can repair.
    pub filter: Option<FilterDefinition>,
    /// `None` for the default order OR an unreadable stored sort (spec §5;
    /// the latter also forces `filter: None` and
    /// `filter_error: Some(UnsupportedFilter)` even when the filter itself
    /// is otherwise valid — one fail-closed disposition for the whole
    /// definition).
    pub sort: Option<PersonSort>,
    pub description: Vec<String>,
    pub filter_error: Option<SavedListFilterError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedListCount {
    pub list_id: SavedListId,
    pub revision: i64,
    pub count: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SavedListStoredRow {
    pub id: SavedListId,
    pub created_by_user_id: UserId,
    pub scope: SavedListScope,
    pub name: String,
    /// A JSONB value may be valid PostgreSQL JSON yet exceed serde_json's
    /// supported depth/number representation. Keep that decode failure local
    /// to definition evaluation so metadata, tombstone protection, and a
    /// create replay never turn into an availability error.
    pub filter: Option<serde_json::Value>,
    /// Raw stored sort columns (docs/specs/SLICE_011b_SORT.md §5) — decode
    /// on demand via [`Self::sort_decode`], mirroring `filter`'s
    /// decode-on-demand `serde_json::Value` above.
    pub sort_key: Option<String>,
    pub sort_direction: Option<String>,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SavedListStoredRow {
    pub(crate) fn sort_decode(&self) -> SortDecodeResult {
        PersonSort::decode_stored(self.sort_key.as_deref(), self.sort_direction.as_deref())
    }
}

pub(crate) struct SavedListStoredRowDb {
    pub(crate) id: Uuid,
    pub(crate) created_by_user_id: Uuid,
    pub(crate) scope: String,
    pub(crate) name: String,
    pub(crate) filter: String,
    pub(crate) sort_key: Option<String>,
    pub(crate) sort_direction: Option<String>,
    pub(crate) revision: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

impl TryFrom<SavedListStoredRowDb> for SavedListStoredRow {
    type Error = SavedListError;

    fn try_from(value: SavedListStoredRowDb) -> Result<Self, Self::Error> {
        Ok(Self {
            id: SavedListId::new(value.id),
            created_by_user_id: UserId::new(value.created_by_user_id),
            scope: SavedListScope::from_db(&value.scope)?,
            name: value.name,
            filter: serde_json::from_str(&value.filter).ok(),
            sort_key: value.sort_key,
            sort_direction: value.sort_direction,
            revision: value.revision,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

struct SavedListMetadataRowDb {
    id: Uuid,
    created_by_user_id: Uuid,
    scope: String,
    name: String,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn metadata_from_metadata_row(
    value: SavedListMetadataRowDb,
    actor_user_id: UserId,
    role: Role,
) -> Result<SavedListMetadata, SavedListError> {
    let scope = SavedListScope::from_db(&value.scope)?;
    let can_manage = match scope {
        SavedListScope::Personal => value.created_by_user_id == actor_user_id.0,
        SavedListScope::Shared => role == Role::Admin,
    };
    Ok(SavedListMetadata {
        id: SavedListId::new(value.id),
        name: value.name,
        scope,
        revision: value.revision,
        created_at: value.created_at,
        updated_at: value.updated_at,
        can_edit: can_manage,
        can_delete: can_manage,
    })
}

pub(crate) fn metadata_for(
    row: &SavedListStoredRow,
    actor_user_id: UserId,
    role: Role,
) -> SavedListMetadata {
    let can_manage = match row.scope {
        SavedListScope::Personal => row.created_by_user_id == actor_user_id,
        SavedListScope::Shared => role == Role::Admin,
    };
    SavedListMetadata {
        id: row.id,
        name: row.name.clone(),
        scope: row.scope,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
        can_edit: can_manage,
        can_delete: can_manage,
    }
}

/// Metadata-only index: no filter deserialization, validation, or membership
/// evaluation occurs here.
pub async fn list_saved_lists(
    conn: &mut PgConnection,
    auth: &AuthContext,
) -> Result<Vec<SavedListMetadata>, SavedListError> {
    let mut workspace_read =
        crate::auth::workspace::read(conn, auth.active_organization_id).await?;
    let conn = &mut *workspace_read;
    let rows = sqlx::query_as!(
        SavedListMetadataRowDb,
        r#"SELECT id, created_by_user_id, scope,
                  name as "name!",
                  revision, created_at, updated_at
           FROM saved_list
           WHERE organization_id = $1
             AND deleted_at IS NULL
             AND (scope = 'shared' OR created_by_user_id = $2)
           ORDER BY created_at ASC, id ASC
           LIMIT 250"#,
        auth.active_organization_id.0,
        auth.actor_user_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter()
        .map(|row| metadata_from_metadata_row(row, auth.actor_user_id, auth.role))
        .collect()
}

/// Normal visible live lookup. Literal tenant and scope predicates make
/// foreign, hidden and nonexistent IDs observationally identical.
pub(crate) async fn visible_live_row(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    list_id: SavedListId,
) -> Result<Option<SavedListStoredRow>, SavedListError> {
    let row = sqlx::query_as!(
        SavedListStoredRowDb,
        r#"SELECT id, created_by_user_id, scope,
                  name as "name!", filter::text as "filter!",
                  sort_key, sort_direction,
                  revision, created_at, updated_at
           FROM saved_list
           WHERE id = $1
             AND organization_id = $2
             AND deleted_at IS NULL
             AND (scope = 'shared' OR created_by_user_id = $3)"#,
        list_id.0,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    row.map(SavedListStoredRow::try_from).transpose()
}

/// Mutation lookup, used only after command-local membership and advisory
/// locks are held.
pub(crate) async fn visible_live_row_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    list_id: SavedListId,
) -> Result<Option<SavedListStoredRow>, SavedListError> {
    let row = sqlx::query_as!(
        SavedListStoredRowDb,
        r#"SELECT id, created_by_user_id, scope,
                  name as "name!", filter::text as "filter!",
                  sort_key, sort_direction,
                  revision, created_at, updated_at
           FROM saved_list
           WHERE id = $1
             AND organization_id = $2
             AND deleted_at IS NULL
             AND (scope = 'shared' OR created_by_user_id = $3)
           FOR UPDATE"#,
        list_id.0,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    row.map(SavedListStoredRow::try_from).transpose()
}

/// Detail is fail-closed by stored definition. Metadata stays available for
/// an unsupported row, but raw invalid JSON never reaches a client.
pub async fn saved_list_detail(
    conn: &mut PgConnection,
    auth: &AuthContext,
    list_id: SavedListId,
) -> Result<Option<SavedListDetail>, SavedListError> {
    let mut workspace_read =
        crate::auth::workspace::read(conn, auth.active_organization_id).await?;
    let conn = &mut *workspace_read;
    let Some(row) = visible_live_row(
        conn,
        auth.active_organization_id,
        auth.actor_user_id,
        list_id,
    )
    .await?
    else {
        return Ok(None);
    };

    let list = metadata_for(&row, auth.actor_user_id, auth.role);

    // A stored sort this binary cannot read fails the WHOLE definition
    // closed (spec §5): `filter: null, sort: null,
    // filter_error: unsupported_filter`, even when the filter itself is
    // otherwise perfectly valid — one fail-closed disposition, checked
    // before the filter is even decoded.
    let sort = match row.sort_decode() {
        SortDecodeResult::Unreadable => {
            return Ok(Some(SavedListDetail {
                list,
                filter: None,
                sort: None,
                description: Vec::new(),
                filter_error: Some(SavedListFilterError::UnsupportedFilter),
            }));
        }
        SortDecodeResult::Default => None,
        SortDecodeResult::Sort(sort) => Some(sort),
    };

    let Some(filter) = row.filter.as_ref().and_then(decode_structural_filter) else {
        return Ok(Some(SavedListDetail {
            list,
            filter: None,
            sort: None,
            description: Vec::new(),
            filter_error: Some(SavedListFilterError::UnsupportedFilter),
        }));
    };

    // Resolve labels before references so stale IDs produce neutral
    // placeholders rather than disappearing from a repairable definition.
    let names = filter_names(conn, auth.active_organization_id, &filter).await?;
    let description = filter.describe(&names);
    let filter_error = match filter
        .validate_references(conn, auth.active_organization_id)
        .await
    {
        Ok(()) => None,
        Err(FilterError::InvalidStage) => Some(SavedListFilterError::InvalidStage),
        Err(FilterError::InvalidAssignee) => Some(SavedListFilterError::InvalidAssignee),
        Err(FilterError::InvalidTag) => Some(SavedListFilterError::InvalidTag),
        Err(FilterError::InvalidField) => Some(SavedListFilterError::InvalidField),
        Err(FilterError::InvalidOption) => Some(SavedListFilterError::InvalidOption),
        Err(FilterError::Database(error)) => return Err(SavedListError::Database(error)),
        Err(FilterError::Malformed) => Some(SavedListFilterError::UnsupportedFilter),
    };

    Ok(Some(SavedListDetail {
        list,
        filter: Some(filter),
        sort,
        description,
        filter_error,
    }))
}

/// The index never invokes this. Detail uses existing Organization-scoped
/// lookups solely to turn stable IDs into display labels.
async fn filter_names(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    filter: &FilterDefinition,
) -> Result<FilterNames, SavedListError> {
    let stages = stage::list(conn, organization_id).await?;
    let members = admin_queries::members(conn, organization_id).await?;
    let tags = tag::list_for_organization(conn, organization_id)
        .await
        .map_err(|err| match err {
            tag::TagError::Database(error) => SavedListError::Database(error),
            _ => SavedListError::Database(sqlx::Error::Decode(
                "tag::list_for_organization returned an unexpected TagError".into(),
            )),
        })?;
    let stage_names = stages
        .into_iter()
        .map(|stage| (stage.id, stage.name))
        .collect::<HashMap<_, _>>();
    let user_names = members
        .into_iter()
        .map(|member| (member.user_id, member.display_name))
        .collect::<HashMap<_, _>>();
    let tag_names = tags
        .into_iter()
        .map(|tag| (tag.id, tag.name))
        .collect::<HashMap<_, _>>();
    let (custom_field_names, custom_option_names) =
        custom_field::filter_names_for_fields(conn, organization_id, &filter.custom_field_ids())
            .await
            .map_err(SavedListError::Database)?;
    Ok(FilterNames {
        stage_names,
        user_names,
        tag_names,
        custom_field_names,
        custom_option_names,
    })
}

/// Stored malformed, unknown-version, or unknown-clause JSON is never
/// interpreted as an empty/all-People filter.
pub(crate) fn decode_structural_filter(value: &serde_json::Value) -> Option<FilterDefinition> {
    let filter = serde_json::from_value::<FilterDefinition>(value.clone()).ok()?;
    if filter.version != 1 || filter.validate().is_err() {
        return None;
    }
    Some(filter)
}

/// Count after a fresh visible lookup and revision check. `me` resolves from
/// the requesting actor at read time just as it does for ad-hoc People.
pub async fn count_saved_list_matches(
    conn: &mut PgConnection,
    auth: &AuthContext,
    list_id: SavedListId,
    expected_revision: i64,
) -> Result<Option<SavedListCount>, SavedListError> {
    let mut workspace_read =
        crate::auth::workspace::read(conn, auth.active_organization_id).await?;
    let conn = &mut *workspace_read;
    let Some(row) = visible_live_row(
        conn,
        auth.active_organization_id,
        auth.actor_user_id,
        list_id,
    )
    .await?
    else {
        return Ok(None);
    };

    if row.revision != expected_revision {
        return Err(SavedListError::Conflict);
    }
    // Count ignores sort's VALUE, but an unreadable stored sort still fails
    // the whole definition closed (spec §5) — 422, never `count: 0`.
    if row.sort_decode() == SortDecodeResult::Unreadable {
        return Err(SavedListError::UnsupportedFilter);
    }
    let filter = row
        .filter
        .as_ref()
        .and_then(decode_structural_filter)
        .ok_or(SavedListError::UnsupportedFilter)?;
    match filter
        .validate_references(conn, auth.active_organization_id)
        .await
    {
        Ok(()) => {}
        Err(FilterError::InvalidStage) => return Err(SavedListError::InvalidStage),
        Err(FilterError::InvalidAssignee) => return Err(SavedListError::InvalidAssignee),
        Err(FilterError::InvalidTag) => return Err(SavedListError::InvalidTag),
        Err(FilterError::InvalidField) => return Err(SavedListError::InvalidField),
        Err(FilterError::InvalidOption) => return Err(SavedListError::InvalidOption),
        Err(FilterError::Database(error)) => return Err(SavedListError::Database(error)),
        Err(FilterError::Malformed) => return Err(SavedListError::UnsupportedFilter),
    }

    // Count evaluation deliberately records only the static filter-kind
    // vocabulary. It never records the definition JSON, values, ids,
    // sources, or the resulting membership count.
    tracing::Span::current().record("filter_kinds", filter.kinds_field());
    tracing::Span::current().record("filter_clause_count", filter.clauses.len());

    let scope = PersonVisibilityScope::from_auth(auth);
    let params = filter.to_query_params(auth.actor_user_id);
    let (count, truncated) = person_queries::count_filtered_matches(conn, &scope, &params).await?;
    Ok(Some(SavedListCount {
        list_id: row.id,
        revision: row.revision,
        count,
        truncated,
    }))
}

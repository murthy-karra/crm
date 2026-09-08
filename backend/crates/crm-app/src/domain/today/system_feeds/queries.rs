//! Read shapes for the admin and member system-feed surfaces
//! (docs/specs/SLICE_011d.md §6). Distinct from [`super::ResolvedFeed`]
//! (today-evaluation semantics, always canonical-substituting a fallback):
//! the admin view shows the actual STORED definition (even when it is
//! reference-invalid) so an admin can see and repair it, while `filter`
//! is `null` only when the stored JSON itself is unsupported.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;

use crate::domain::admin::queries as admin_queries;
use crate::domain::person::filter::{FilterDefinition, FilterError, FilterNames};
use crate::domain::saved_list::decode_structural_filter;
use crate::domain::stage;
use crate::domain::tag;
use crate::ids::{OrganizationId, UserId};

use super::error::TodayFeedError;
use super::{canonical_default, canonical_fresh_within_hours, FeedKey, ALL_FEED_KEYS};

#[derive(Debug, Clone, Serialize)]
pub struct UserRef {
    pub id: UserId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefaultFeedDefinition {
    pub filter: FilterDefinition,
    pub fresh_within_hours: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Feed {
    pub feed_key: &'static str,
    pub enabled: bool,
    pub revision: i64,
    pub is_default: bool,
    /// The stored typed definition (the canonical one when `is_default`);
    /// `null` only when the stored JSON is `unsupported_filter`.
    pub filter: Option<FilterDefinition>,
    pub fresh_within_hours: Option<i32>,
    pub description: Vec<String>,
    pub filter_error: Option<&'static str>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserRef>,
    pub default: DefaultFeedDefinition,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemberFeed {
    pub feed_key: &'static str,
    pub enabled: bool,
    pub is_default: bool,
    pub description: Vec<String>,
}

/// The raw stored columns a `Feed` view is built from — shared between the
/// admin index read (one row per feed, loaded fresh) and a command's
/// response (built from the row it just locked/wrote, inside the SAME
/// transaction, without a second round trip).
pub(crate) struct StoredFeedFields {
    pub enabled: bool,
    pub filter: Option<String>,
    pub fresh_within_hours: Option<i32>,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
    pub updated_by_user_id: Option<uuid::Uuid>,
}

struct StoredFeedRow {
    feed_key: String,
    enabled: bool,
    filter: Option<String>,
    fresh_within_hours: Option<i32>,
    revision: i64,
    updated_at: DateTime<Utc>,
    updated_by_user_id: Option<uuid::Uuid>,
}

async fn load_stored_rows(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<HashMap<&'static str, StoredFeedFields>, sqlx::Error> {
    let rows = sqlx::query_as!(
        StoredFeedRow,
        r#"SELECT feed_key, enabled, filter::text as filter, fresh_within_hours,
                  revision, updated_at, updated_by_user_id
           FROM today_system_feed
           WHERE organization_id = $1"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut by_key = HashMap::new();
    for row in rows {
        if let Some(key) = FeedKey::decode(&row.feed_key) {
            by_key.insert(
                key.as_str(),
                StoredFeedFields {
                    enabled: row.enabled,
                    filter: row.filter,
                    fresh_within_hours: row.fresh_within_hours,
                    revision: row.revision,
                    updated_at: row.updated_at,
                    updated_by_user_id: row.updated_by_user_id,
                },
            );
        }
    }
    Ok(by_key)
}

/// The names `describe()` needs for stage/assignee placeholders (011b
/// pattern): Organization-scoped, DB-free once resolved.
pub(crate) async fn filter_names(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<FilterNames, sqlx::Error> {
    let stages = stage::list(conn, organization_id).await?;
    let members = admin_queries::members(conn, organization_id).await?;
    let tags = tag::list_for_organization(conn, organization_id)
        .await
        .map_err(|err| match err {
            tag::TagError::Database(error) => error,
            _ => sqlx::Error::Decode(
                "tag::list_for_organization returned an unexpected TagError".into(),
            ),
        })?;
    Ok(FilterNames {
        stage_names: stages.into_iter().map(|s| (s.id, s.name)).collect(),
        user_names: members
            .into_iter()
            .map(|m| (m.user_id, m.display_name))
            .collect(),
        tag_names: tags.into_iter().map(|t| (t.id, t.name)).collect(),
    })
}

/// Builds one feed's `Feed` view from its raw stored columns (or `None` for
/// a missing row = canonical enabled). Shared by [`admin_feed_view`] and
/// every command's response, so the response returned from a write is
/// built from the SAME transaction that wrote it, never a second read.
pub(crate) async fn build_feed_view(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    feed_key: FeedKey,
    row: Option<&StoredFeedFields>,
    names: &FilterNames,
) -> Result<Feed, TodayFeedError> {
    let canonical_filter = canonical_default(feed_key);
    let canonical_window = canonical_fresh_within_hours(feed_key);
    let default = DefaultFeedDefinition {
        filter: canonical_filter.clone(),
        fresh_within_hours: canonical_window,
    };
    let Some(row) = row else {
        // Missing row = canonical enabled (spec §3), same as evaluation.
        return Ok(Feed {
            feed_key: feed_key.as_str(),
            enabled: true,
            revision: 1,
            is_default: true,
            filter: Some(canonical_filter.clone()),
            fresh_within_hours: canonical_window,
            description: canonical_filter.describe(names),
            filter_error: None,
            updated_at: Utc::now(),
            updated_by: None,
            default,
        });
    };
    let is_default = row.filter.is_none() && row.fresh_within_hours.is_none();
    let (filter, filter_error) = match &row.filter {
        None => (Some(canonical_filter.clone()), None),
        Some(raw) => match decode_stored(raw) {
            None => (None, Some("unsupported_filter")),
            Some(typed) => match typed.validate_references(conn, organization_id).await {
                Ok(()) => (Some(typed), None),
                Err(FilterError::InvalidStage) => (Some(typed), Some("invalid_stage")),
                Err(FilterError::InvalidAssignee) => (Some(typed), Some("invalid_assignee")),
                Err(FilterError::InvalidTag) => (Some(typed), Some("invalid_tag")),
                Err(FilterError::Malformed) => (None, Some("unsupported_filter")),
                Err(FilterError::Database(error)) => return Err(TodayFeedError::Database(error)),
            },
        },
    };
    let description = filter
        .as_ref()
        .map(|f| f.describe(names))
        .unwrap_or_default();
    let fresh_within_hours = if is_default {
        canonical_window
    } else {
        row.fresh_within_hours.or(canonical_window)
    };
    let updated_by = row.updated_by_user_id.map(|id| UserRef {
        id: UserId::new(id),
        display_name: names
            .user_names
            .get(&UserId::new(id))
            .cloned()
            .unwrap_or_default(),
    });
    Ok(Feed {
        feed_key: feed_key.as_str(),
        enabled: row.enabled,
        revision: row.revision,
        is_default,
        filter,
        fresh_within_hours,
        description,
        filter_error,
        updated_at: row.updated_at,
        updated_by,
        default,
    })
}

/// docs/specs/SLICE_011d.md §6: `GET /api/organization/today-feeds`, in the
/// fixed key order. A database failure propagates (503 at the caller).
pub async fn admin_feed_view(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<[Feed; 3], TodayFeedError> {
    let stored = load_stored_rows(conn, organization_id).await?;
    let names = filter_names(conn, organization_id).await?;

    let mut out = Vec::with_capacity(3);
    for feed_key in ALL_FEED_KEYS {
        let feed = build_feed_view(
            conn,
            organization_id,
            feed_key,
            stored.get(feed_key.as_str()),
            &names,
        )
        .await?;
        out.push(feed);
    }
    Ok(out.try_into().unwrap_or_else(|_| unreachable!()))
}

/// docs/specs/SLICE_011d.md §6: `GET /api/today/feeds` — the EFFECTIVE
/// rule (canonical under fallback), for any active member.
pub async fn member_feed_view(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<[MemberFeed; 3], sqlx::Error> {
    let names = filter_names(conn, organization_id).await?;
    let resolved = super::load_feed_rows(conn, organization_id).await?;
    let mut out = Vec::with_capacity(3);
    for feed in resolved {
        out.push(MemberFeed {
            feed_key: feed.feed_key.as_str(),
            enabled: feed.enabled,
            is_default: feed.is_default,
            description: feed.filter.describe(&names),
        });
    }
    Ok(out.try_into().unwrap_or_else(|_| unreachable!()))
}

fn decode_stored(raw: &str) -> Option<FilterDefinition> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| decode_structural_filter(&value))
}

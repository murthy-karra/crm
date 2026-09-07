//! The three per-Organization system feeds that replace the compiled-in
//! Today built-ins (docs/specs/SLICE_011d.md §§1-3). This module owns the
//! feed vocabulary (`FeedKey`), the canonical (code-defined) default
//! definition per feed, seeding a new Organization's three rows, the
//! feed-row loader with its fallback-to-canonical semantics, typed admin
//! commands and preview (spec §4), and the admin/member read shapes
//! (spec §6).

pub mod commands;
pub mod error;
pub(crate) mod evaluate;
pub mod queries;

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, FilterError,
};
use crate::domain::saved_list::decode_structural_filter;
use crate::ids::{OrganizationId, UserId};

/// The fixed vocabulary of system feeds (spec §3 table), in the fixed key
/// order every response/route uses (`GET /api/organization/today-feeds`
/// spec §6: "in fixed key order").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedKey {
    UnansweredInquiry,
    ClientReplied,
    CallOutcomeNeeded,
}

/// The fixed key order (spec §6).
pub const ALL_FEED_KEYS: [FeedKey; 3] = [
    FeedKey::UnansweredInquiry,
    FeedKey::ClientReplied,
    FeedKey::CallOutcomeNeeded,
];

/// The canonical freshness window for the two person-state feeds (spec §3).
pub const CANONICAL_FRESH_WITHIN_HOURS: i32 = 24;

impl FeedKey {
    /// The wire/DB token (the `today_system_feed.feed_key` CHECK values).
    pub fn as_str(self) -> &'static str {
        match self {
            FeedKey::UnansweredInquiry => "unanswered_inquiry",
            FeedKey::ClientReplied => "client_replied",
            FeedKey::CallOutcomeNeeded => "call_outcome_needed",
        }
    }

    /// Decodes a stored/wire feed key. `None` for anything else — a read
    /// path fails closed rather than guessing (AGENTS.md convention).
    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "unanswered_inquiry" => Some(FeedKey::UnansweredInquiry),
            "client_replied" => Some(FeedKey::ClientReplied),
            "call_outcome_needed" => Some(FeedKey::CallOutcomeNeeded),
            _ => None,
        }
    }

    /// Whether this feed carries a freshness window at all (the two
    /// person-state feeds do; the call feed never does — spec §3's
    /// `CHECK (feed_key <> 'call_outcome_needed' OR fresh_within_hours IS
    /// NULL)`).
    pub fn has_freshness_window(self) -> bool {
        !matches!(self, FeedKey::CallOutcomeNeeded)
    }
}

fn bool_clause(value: bool) -> BoolClause {
    BoolClause { value }
}

/// The canonical default definition, regenerated from code (spec §3 table).
/// Never touches the database — pure, so an unedited Organization's
/// effective rule always tracks a future canonical change without a
/// migration.
pub fn canonical_default(feed_key: FeedKey) -> FilterDefinition {
    match feed_key {
        FeedKey::UnansweredInquiry => FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me],
                }),
                Clause::AwaitingResponse(bool_clause(true)),
            ],
        },
        FeedKey::ClientReplied => FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me],
                }),
                Clause::ClientRepliedUnanswered(bool_clause(true)),
            ],
        },
        FeedKey::CallOutcomeNeeded => FilterDefinition {
            version: 1,
            clauses: vec![Clause::AwaitingCallOutcome(bool_clause(true))],
        },
    }
}

/// The canonical freshness window: 24 hours for the two person-state feeds,
/// `None` (no window) for the call feed (spec §3 table).
pub fn canonical_fresh_within_hours(feed_key: FeedKey) -> Option<i32> {
    if feed_key.has_freshness_window() {
        Some(CANONICAL_FRESH_WITHIN_HOURS)
    } else {
        None
    }
}

/// Seeds the three feed rows for a new Organization, beside stage seeding
/// (spec §3; `create_organization.rs`). `ON CONFLICT DO NOTHING` matches
/// `stage::seed_defaults` and makes this safe to call from both
/// `create_organization` and any future backfill/repair path.
pub async fn seed_defaults(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    for feed_key in ALL_FEED_KEYS {
        sqlx::query!(
            r#"INSERT INTO today_system_feed (organization_id, feed_key)
               VALUES ($1, $2)
               ON CONFLICT (organization_id, feed_key) DO NOTHING"#,
            organization_id.0,
            feed_key.as_str(),
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// A feed row resolved to its EFFECTIVE definition (spec §5 rule 1, §1 rule
/// 6): the stored definition when present, structurally valid and
/// reference-valid, otherwise the canonical default with `fallback: true`.
#[derive(Debug, Clone)]
pub struct ResolvedFeed {
    pub feed_key: FeedKey,
    pub enabled: bool,
    /// The effective definition Today evaluates: the stored typed
    /// definition, or the canonical default when `is_default` or
    /// `fallback`.
    pub filter: FilterDefinition,
    /// The effective freshness window (hours), always `Some` for the two
    /// person-state feeds and always `None` for the call feed.
    pub fresh_within_hours: Option<i32>,
    /// `true` iff the STORED row has both `filter` and `fresh_within_hours`
    /// NULL (spec §3: "is_default is filter IS NULL AND fresh_within_hours
    /// IS NULL"). Independent of `fallback` — an edited-but-now-invalid row
    /// is neither default nor evaluated as stored.
    pub is_default: bool,
    /// `true` iff a NON-NULL stored definition could not be decoded or
    /// failed reference validation, so the canonical default was
    /// substituted (spec §1 rule 6, §5 rule 1).
    pub fallback: bool,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
    pub updated_by_user_id: Option<UserId>,
}

struct FeedRow {
    feed_key: String,
    enabled: bool,
    filter: Option<String>,
    fresh_within_hours: Option<i32>,
    revision: i64,
    updated_at: DateTime<Utc>,
    updated_by_user_id: Option<uuid::Uuid>,
}

/// Loads and resolves all three feed rows for `organization_id` (spec §5
/// step 1): a missing row (an Organization created by a binary older than
/// this migration, or any other gap) reads as canonical-enabled — never a
/// 404 or a synthesized error. Decode/reference failures on a present
/// definition fall back to canonical with `fallback: true`; a genuine
/// database failure propagates (a 503 at the caller, exactly like a
/// built-in failure today).
pub async fn load_feed_rows(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<[ResolvedFeed; 3], sqlx::Error> {
    let rows = sqlx::query_as!(
        FeedRow,
        r#"SELECT feed_key, enabled, filter::text as filter, fresh_within_hours,
                  revision, updated_at, updated_by_user_id
           FROM today_system_feed
           WHERE organization_id = $1"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut by_key: std::collections::HashMap<&'static str, FeedRow> =
        std::collections::HashMap::new();
    for row in rows {
        if let Some(key) = FeedKey::decode(&row.feed_key) {
            by_key.insert(key.as_str(), row);
        }
        // An unrecognized feed_key value cannot exist under the table's own
        // CHECK constraint; skip defensively rather than fail the whole
        // read on a row this binary does not understand.
    }

    let mut resolved = Vec::with_capacity(3);
    for feed_key in ALL_FEED_KEYS {
        let canonical_filter = canonical_default(feed_key);
        let canonical_window = canonical_fresh_within_hours(feed_key);
        let resolved_feed = match by_key.remove(feed_key.as_str()) {
            None => {
                // Missing row = canonical enabled (spec §3).
                ResolvedFeed {
                    feed_key,
                    enabled: true,
                    filter: canonical_filter,
                    fresh_within_hours: canonical_window,
                    is_default: true,
                    fallback: false,
                    revision: 1,
                    updated_at: Utc::now(),
                    updated_by_user_id: None,
                }
            }
            Some(row) => {
                let is_default = row.filter.is_none() && row.fresh_within_hours.is_none();
                let (effective_filter, fallback) = match &row.filter {
                    None => (canonical_filter.clone(), false),
                    Some(raw) => match resolve_stored_filter(conn, organization_id, raw).await? {
                        Some(typed) => (typed, false),
                        None => (canonical_filter.clone(), true),
                    },
                };
                let effective_window = if is_default || fallback {
                    canonical_window
                } else {
                    row.fresh_within_hours.or(canonical_window)
                };
                ResolvedFeed {
                    feed_key,
                    enabled: row.enabled,
                    filter: effective_filter,
                    fresh_within_hours: effective_window,
                    is_default,
                    fallback,
                    revision: row.revision,
                    updated_at: row.updated_at,
                    updated_by_user_id: row.updated_by_user_id.map(UserId::new),
                }
            }
        };
        resolved.push(resolved_feed);
    }

    Ok(resolved
        .try_into()
        .unwrap_or_else(|_| unreachable!("exactly ALL_FEED_KEYS.len() == 3 feeds resolved")))
}

/// Decodes and reference-validates a stored (non-NULL) filter JSON string.
/// `Ok(None)` for anything invalid (structural decode failure OR a stale
/// reference) — both collapse to the same fallback disposition (spec §1
/// rule 6 groups "a referenced stage was deleted" and "unsupported by the
/// running binary" together). `Err` only for a genuine database failure.
async fn resolve_stored_filter(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    raw: &str,
) -> Result<Option<FilterDefinition>, sqlx::Error> {
    let Some(value) = serde_json::from_str::<serde_json::Value>(raw).ok() else {
        return Ok(None);
    };
    let Some(filter) = decode_structural_filter(&value) else {
        return Ok(None);
    };
    match filter.validate_references(conn, organization_id).await {
        Ok(()) => Ok(Some(filter)),
        Err(FilterError::InvalidStage) | Err(FilterError::InvalidAssignee) => Ok(None),
        Err(FilterError::Malformed) => Ok(None),
        Err(FilterError::Database(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_defaults_match_the_spec_3_table() {
        let unanswered = canonical_default(FeedKey::UnansweredInquiry);
        assert_eq!(unanswered.clauses.len(), 2);
        assert!(matches!(unanswered.clauses[0], Clause::AssignedTo(_)));
        assert!(matches!(
            unanswered.clauses[1],
            Clause::AwaitingResponse(BoolClause { value: true })
        ));
        assert_eq!(
            canonical_fresh_within_hours(FeedKey::UnansweredInquiry),
            Some(24)
        );

        let replied = canonical_default(FeedKey::ClientReplied);
        assert_eq!(replied.clauses.len(), 2);
        assert!(matches!(replied.clauses[0], Clause::AssignedTo(_)));
        assert!(matches!(
            replied.clauses[1],
            Clause::ClientRepliedUnanswered(BoolClause { value: true })
        ));
        assert_eq!(
            canonical_fresh_within_hours(FeedKey::ClientReplied),
            Some(24)
        );

        let call = canonical_default(FeedKey::CallOutcomeNeeded);
        assert_eq!(call.clauses.len(), 1);
        assert!(matches!(
            call.clauses[0],
            Clause::AwaitingCallOutcome(BoolClause { value: true })
        ));
        assert_eq!(
            canonical_fresh_within_hours(FeedKey::CallOutcomeNeeded),
            None
        );
    }

    #[test]
    fn canonical_definitions_validate() {
        for feed_key in ALL_FEED_KEYS {
            assert!(canonical_default(feed_key).validate().is_ok());
        }
    }

    #[test]
    fn feed_key_round_trips_through_as_str_and_decode() {
        for feed_key in ALL_FEED_KEYS {
            assert_eq!(FeedKey::decode(feed_key.as_str()), Some(feed_key));
        }
        assert_eq!(FeedKey::decode("bogus"), None);
    }

    #[test]
    fn only_the_two_person_state_feeds_have_a_freshness_window() {
        assert!(FeedKey::UnansweredInquiry.has_freshness_window());
        assert!(FeedKey::ClientReplied.has_freshness_window());
        assert!(!FeedKey::CallOutcomeNeeded.has_freshness_window());
    }
}

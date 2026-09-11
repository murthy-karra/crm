//! Typed admin commands and preview for the three system feeds
//! (docs/specs/SLICE_011d.md §4). Each mutating command runs in one
//! transaction: lock and re-check the actor's current active membership
//! with the `saved_list` `FOR SHARE` posture and require `Role::Admin`
//! from that locked row, take the Organization advisory lock in a new
//! `today-feeds:` namespace, then read the feed row `FOR UPDATE`. Typed
//! callers cannot bypass authorization or validation.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

use crate::domain::admin::Role;
use crate::domain::custom_field;
use crate::domain::envelope::{Actor, CommandContext};
use crate::domain::facts::{self, TodayFeedChangedFact};
use crate::domain::person::filter::{Assignee, Clause, FilterDefinition};
use crate::domain::saved_list::MAX_WIRE_REVISION;
use crate::domain::today::model::TodayItem;
use crate::domain::today::rank::rank;
use crate::ids::{OrganizationId, UserId};

use super::error::TodayFeedError;
use super::queries::{build_feed_view, filter_names, Feed, StoredFeedFields};
use super::{canonical_default, canonical_fresh_within_hours, evaluate, FeedKey};

pub fn validate_expected_revision(revision: i64) -> Result<(), TodayFeedError> {
    if !(1..=MAX_WIRE_REVISION).contains(&revision) {
        return Err(TodayFeedError::MalformedRequest);
    }
    Ok(())
}

fn anchor_clause_present(feed_key: FeedKey, filter: &FilterDefinition) -> bool {
    filter.clauses.iter().any(|clause| {
        matches!(
            (feed_key, clause),
            (FeedKey::UnansweredInquiry, Clause::AwaitingResponse(b))
                | (FeedKey::ClientReplied, Clause::ClientRepliedUnanswered(b))
                | (FeedKey::CallOutcomeNeeded, Clause::AwaitingCallOutcome(b))
                if b.value
        )
    })
}

fn assigned_to_me_present(filter: &FilterDefinition) -> bool {
    filter.clauses.iter().any(|clause| {
        matches!(clause, Clause::AssignedTo(assigned) if assigned.assignees.contains(&Assignee::Me))
    })
}

/// docs/specs/SLICE_011d.md §4/§6: version 1 and structural `validate()`
/// ONLY — no database access, no revision/reference dependency. Review
/// round 1, F3: this stays split from
/// [`validate_feed_definition_references_and_rules`] and runs BEFORE the
/// row lock/revision check, so a genuinely malformed body still fails
/// closed at 400 before anything DB-dependent runs; the reference and §1
/// rule checks (422-class) are deliberately NOT here — see F3 below for
/// why they must run AFTER the revision check instead.
fn validate_feed_definition_structural(filter: &FilterDefinition) -> Result<(), TodayFeedError> {
    if filter.version != 1 {
        return Err(TodayFeedError::MalformedRequest);
    }
    filter.validate().map_err(TodayFeedError::from)?;
    Ok(())
}

/// docs/specs/SLICE_011d.md §4: Organization-scoped `validate_references`,
/// then the §1 feed rules — in exactly this order. The anchor clause (§1
/// rule 4) can never be negated or removed; the two person-state feeds
/// must keep `assigned_to: [me]` (§1 rule 3); `fresh_within_hours` is
/// required `1..=8760` for those two feeds and forbidden for the call feed
/// (§3).
///
/// Review round 1, F3: for `update_today_system_feed`, this must run
/// AFTER the `FOR UPDATE` row lock and its revision check (spec §6 error
/// precedence: 409 `today_feed_conflict` before 422 `invalid_feed_rule`,
/// mirroring `saved_list`'s command shape) — a stale `expected_revision`
/// must fail with `Conflict` even when the submitted definition would
/// ALSO have failed 422 for an unrelated reason; validating first would
/// silently prefer 422 over the caller's real problem, a stale revision.
/// For `preview_today_system_feed`, spec §4 has a different order: the
/// subject's membership (404) is checked AFTER this — see that function.
async fn validate_feed_definition_references_and_rules(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    feed_key: FeedKey,
    filter: &FilterDefinition,
    fresh_within_hours: Option<i32>,
) -> Result<(), TodayFeedError> {
    filter
        .validate_references(conn, organization_id)
        .await
        .map_err(TodayFeedError::from)?;
    if !anchor_clause_present(feed_key, filter) {
        return Err(TodayFeedError::InvalidFeedRule);
    }
    if feed_key.has_freshness_window() {
        if !assigned_to_me_present(filter) {
            return Err(TodayFeedError::InvalidFeedRule);
        }
        match fresh_within_hours {
            Some(hours) if (1..=8760).contains(&hours) => {}
            _ => return Err(TodayFeedError::InvalidFeedRule),
        }
    } else if fresh_within_hours.is_some() {
        return Err(TodayFeedError::InvalidFeedRule);
    }
    Ok(())
}

/// Locks and re-checks the actor's CURRENT active membership (the
/// `saved_list` `FOR SHARE` posture, spec §4) and requires `Role::Admin`
/// from that SAME locked row — a concurrent demotion cannot interleave
/// with authorization and a subsequent write.
async fn lock_current_membership_require_admin(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<(), TodayFeedError> {
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
        return Err(TodayFeedError::Unauthenticated);
    };
    if row.status != "active" {
        return Err(TodayFeedError::Unauthenticated);
    }
    let role = Role::from_db_str(&row.role).ok_or(TodayFeedError::Corrupt)?;
    if role != Role::Admin {
        return Err(TodayFeedError::Forbidden);
    }
    Ok(())
}

/// Membership-only re-check (no admin requirement) for preview's subject —
/// spec §4: "verifies the subject is a current active member of the
/// actor's Organization" — a different actor from the one authenticating
/// the request.
async fn require_current_active_member(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    subject_user_id: UserId,
) -> Result<(), TodayFeedError> {
    let row = sqlx::query!(
        r#"SELECT status FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        subject_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(row) if row.status == "active" => Ok(()),
        _ => Err(TodayFeedError::NotFound),
    }
}

/// One namespace for all system-feed writes in an Organization (spec §4),
/// distinct from `saved-lists:` and `intake:` — atomic revision check +
/// write per feed, single lock order after membership.
async fn acquire_today_feeds_lock(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), TodayFeedError> {
    let organization_id_text = organization_id.to_string();
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended('today-feeds:' || $1::text, 0))"#,
        organization_id_text,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Defensively creates the row if missing (seed + backfill should always
/// have created it, but a command must not fail closed on a genuinely
/// absent row) before locking it `FOR UPDATE`.
async fn lock_feed_row_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    feed_key: FeedKey,
) -> Result<StoredFeedFields, TodayFeedError> {
    sqlx::query!(
        r#"INSERT INTO today_system_feed (organization_id, feed_key)
           VALUES ($1, $2)
           ON CONFLICT (organization_id, feed_key) DO NOTHING"#,
        organization_id.0,
        feed_key.as_str(),
    )
    .execute(&mut *conn)
    .await?;
    let row = sqlx::query!(
        r#"SELECT enabled, filter::text as filter, fresh_within_hours, revision,
                  updated_at, updated_by_user_id
           FROM today_system_feed
           WHERE organization_id = $1 AND feed_key = $2
           FOR UPDATE"#,
        organization_id.0,
        feed_key.as_str(),
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(StoredFeedFields {
        enabled: row.enabled,
        filter: row.filter,
        fresh_within_hours: row.fresh_within_hours,
        revision: row.revision,
        updated_at: row.updated_at,
        updated_by_user_id: row.updated_by_user_id,
    })
}

/// Per-column canonical collapse to NULL (spec §4): `filter` stores NULL
/// iff it equals the canonical definition by typed, order-sensitive
/// equality (`FilterDefinition` derives `PartialEq` over its ordered
/// `Vec<Clause>`); `fresh_within_hours` stores NULL iff it equals the
/// canonical window.
fn collapse_for_storage(
    feed_key: FeedKey,
    filter: &FilterDefinition,
    fresh_within_hours: Option<i32>,
) -> (Option<serde_json::Value>, Option<i32>) {
    let canonical_filter = canonical_default(feed_key);
    let canonical_window = canonical_fresh_within_hours(feed_key);
    let filter_value = if *filter == canonical_filter {
        None
    } else {
        Some(serde_json::to_value(filter).expect("a validated FilterDefinition always serializes"))
    };
    let window = if fresh_within_hours == canonical_window {
        None
    } else {
        fresh_within_hours
    };
    (filter_value, window)
}

fn stored_matches(stored_json: &Option<String>, collapsed: &Option<serde_json::Value>) -> bool {
    match (stored_json, collapsed) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(raw), Some(value)) => serde_json::from_str::<serde_json::Value>(raw)
            .map(|stored| &stored == value)
            .unwrap_or(false),
    }
}

pub struct UpdateTodaySystemFeed {
    pub feed_key: FeedKey,
    pub expected_revision: i64,
    pub filter: FilterDefinition,
    pub fresh_within_hours: Option<i32>,
}

pub struct RevertTodaySystemFeed {
    pub feed_key: FeedKey,
    pub expected_revision: i64,
}

pub struct SetTodaySystemFeedEnabled {
    pub feed_key: FeedKey,
    pub expected_revision: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct TodaySystemFeedOutcome {
    pub feed: Feed,
    pub changed: bool,
}

fn record_command_outcome<T>(result: &Result<T, TodayFeedError>) {
    match result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "ok");
        }
        Err(error) => {
            tracing::warn!(error_kind = error.kind(), "today-feed command failed");
            tracing::Span::current().record("outcome", error.kind());
        }
    }
}

#[tracing::instrument(
    name = "today_feed.update",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        feed_key = cmd.feed_key.as_str(),
        filter_kinds = %cmd.filter.kinds_field(),
        outcome = tracing::field::Empty,
    )
)]
pub async fn update_today_system_feed(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateTodaySystemFeed,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    let result = update_today_system_feed_attempt(pool, ctx, cmd).await;
    record_command_outcome(&result);
    result
}

async fn update_today_system_feed_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateTodaySystemFeed,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    validate_expected_revision(cmd.expected_revision)?;
    validate_feed_definition_structural(&cmd.filter)?;

    let mut tx = crate::auth::workspace::begin(pool, ctx.organization_id).await?;
    lock_current_membership_require_admin(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_today_feeds_lock(&mut tx, ctx.organization_id).await?;

    let row = lock_feed_row_for_update(&mut tx, ctx.organization_id, cmd.feed_key).await?;
    if row.revision != cmd.expected_revision {
        return Err(TodayFeedError::Conflict);
    }

    // F3: reference validation and the §1 rules run HERE, after the
    // revision check — a stale revision must win over an unrelated 422.
    validate_feed_definition_references_and_rules(
        &mut tx,
        ctx.organization_id,
        cmd.feed_key,
        &cmd.filter,
        cmd.fresh_within_hours,
    )
    .await?;

    let (filter_value, window) =
        collapse_for_storage(cmd.feed_key, &cmd.filter, cmd.fresh_within_hours);

    if stored_matches(&row.filter, &filter_value) && row.fresh_within_hours == window {
        let names = filter_names(&mut tx, ctx.organization_id).await?;
        let feed = build_feed_view(
            &mut tx,
            ctx.organization_id,
            cmd.feed_key,
            Some(&row),
            &names,
        )
        .await?;
        tx.commit().await?;
        return Ok(TodaySystemFeedOutcome {
            feed,
            changed: false,
        });
    }
    if row.revision >= MAX_WIRE_REVISION {
        return Err(TodayFeedError::RevisionExhausted);
    }

    let filter_value_text = filter_value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| TodayFeedError::MalformedRequest)?;

    let updated = sqlx::query!(
        r#"UPDATE today_system_feed
           SET filter = CAST($4 AS text)::jsonb, fresh_within_hours = $5, revision = revision + 1,
               updated_by_user_id = $6, updated_at = now()
           WHERE organization_id = $1 AND feed_key = $2 AND revision = $3
           RETURNING enabled, filter::text as filter, fresh_within_hours, revision,
                     updated_at, updated_by_user_id"#,
        ctx.organization_id.0,
        cmd.feed_key.as_str(),
        cmd.expected_revision,
        filter_value_text,
        window,
        ctx.actor_user_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TodayFeedError::Conflict)?;

    write_fact(
        &mut tx,
        ctx,
        cmd.feed_key,
        "updated",
        row.revision,
        updated.revision,
        updated.enabled,
        updated.filter.clone(),
        window,
    )
    .await?;

    let names = filter_names(&mut tx, ctx.organization_id).await?;
    let updated_fields = StoredFeedFields {
        enabled: updated.enabled,
        filter: updated.filter,
        fresh_within_hours: updated.fresh_within_hours,
        revision: updated.revision,
        updated_at: updated.updated_at,
        updated_by_user_id: updated.updated_by_user_id,
    };
    let feed = build_feed_view(
        &mut tx,
        ctx.organization_id,
        cmd.feed_key,
        Some(&updated_fields),
        &names,
    )
    .await?;
    tx.commit().await?;
    Ok(TodaySystemFeedOutcome {
        feed,
        changed: true,
    })
}

#[tracing::instrument(
    name = "today_feed.revert",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        feed_key = cmd.feed_key.as_str(),
        outcome = tracing::field::Empty,
    )
)]
pub async fn revert_today_system_feed(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: RevertTodaySystemFeed,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    let result = revert_today_system_feed_attempt(pool, ctx, cmd).await;
    record_command_outcome(&result);
    result
}

async fn revert_today_system_feed_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: RevertTodaySystemFeed,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    validate_expected_revision(cmd.expected_revision)?;

    let mut tx = crate::auth::workspace::begin(pool, ctx.organization_id).await?;
    lock_current_membership_require_admin(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_today_feeds_lock(&mut tx, ctx.organization_id).await?;

    let row = lock_feed_row_for_update(&mut tx, ctx.organization_id, cmd.feed_key).await?;
    if row.revision != cmd.expected_revision {
        return Err(TodayFeedError::Conflict);
    }

    // Reverting an already-default feed is a no-op (spec §4).
    if row.filter.is_none() && row.fresh_within_hours.is_none() {
        let names = filter_names(&mut tx, ctx.organization_id).await?;
        let feed = build_feed_view(
            &mut tx,
            ctx.organization_id,
            cmd.feed_key,
            Some(&row),
            &names,
        )
        .await?;
        tx.commit().await?;
        return Ok(TodaySystemFeedOutcome {
            feed,
            changed: false,
        });
    }
    if row.revision >= MAX_WIRE_REVISION {
        return Err(TodayFeedError::RevisionExhausted);
    }

    let updated = sqlx::query!(
        r#"UPDATE today_system_feed
           SET filter = NULL, fresh_within_hours = NULL, revision = revision + 1,
               updated_by_user_id = $3, updated_at = now()
           WHERE organization_id = $1 AND feed_key = $2 AND revision = $4
           RETURNING enabled, filter::text as filter, fresh_within_hours, revision,
                     updated_at, updated_by_user_id"#,
        ctx.organization_id.0,
        cmd.feed_key.as_str(),
        ctx.actor_user_id.0,
        cmd.expected_revision,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TodayFeedError::Conflict)?;

    write_fact(
        &mut tx,
        ctx,
        cmd.feed_key,
        "reverted",
        row.revision,
        updated.revision,
        updated.enabled,
        None,
        None,
    )
    .await?;

    let names = filter_names(&mut tx, ctx.organization_id).await?;
    let updated_fields = StoredFeedFields {
        enabled: updated.enabled,
        filter: updated.filter,
        fresh_within_hours: updated.fresh_within_hours,
        revision: updated.revision,
        updated_at: updated.updated_at,
        updated_by_user_id: updated.updated_by_user_id,
    };
    let feed = build_feed_view(
        &mut tx,
        ctx.organization_id,
        cmd.feed_key,
        Some(&updated_fields),
        &names,
    )
    .await?;
    tx.commit().await?;
    Ok(TodaySystemFeedOutcome {
        feed,
        changed: true,
    })
}

#[tracing::instrument(
    name = "today_feed.set_enabled",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        feed_key = cmd.feed_key.as_str(),
        outcome = tracing::field::Empty,
    )
)]
pub async fn set_today_system_feed_enabled(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: SetTodaySystemFeedEnabled,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    let result = set_today_system_feed_enabled_attempt(pool, ctx, cmd).await;
    record_command_outcome(&result);
    result
}

async fn set_today_system_feed_enabled_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: SetTodaySystemFeedEnabled,
) -> Result<TodaySystemFeedOutcome, TodayFeedError> {
    validate_expected_revision(cmd.expected_revision)?;

    let mut tx = crate::auth::workspace::begin(pool, ctx.organization_id).await?;
    lock_current_membership_require_admin(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    acquire_today_feeds_lock(&mut tx, ctx.organization_id).await?;

    let row = lock_feed_row_for_update(&mut tx, ctx.organization_id, cmd.feed_key).await?;
    if row.revision != cmd.expected_revision {
        return Err(TodayFeedError::Conflict);
    }

    // Setting the current state is a no-op (spec §4).
    if row.enabled == cmd.enabled {
        let names = filter_names(&mut tx, ctx.organization_id).await?;
        let feed = build_feed_view(
            &mut tx,
            ctx.organization_id,
            cmd.feed_key,
            Some(&row),
            &names,
        )
        .await?;
        tx.commit().await?;
        return Ok(TodaySystemFeedOutcome {
            feed,
            changed: false,
        });
    }
    if row.revision >= MAX_WIRE_REVISION {
        return Err(TodayFeedError::RevisionExhausted);
    }

    let updated = sqlx::query!(
        r#"UPDATE today_system_feed
           SET enabled = $3, revision = revision + 1, updated_by_user_id = $4, updated_at = now()
           WHERE organization_id = $1 AND feed_key = $2 AND revision = $5
           RETURNING enabled, filter::text as filter, fresh_within_hours, revision,
                     updated_at, updated_by_user_id"#,
        ctx.organization_id.0,
        cmd.feed_key.as_str(),
        cmd.enabled,
        ctx.actor_user_id.0,
        cmd.expected_revision,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TodayFeedError::Conflict)?;

    write_fact(
        &mut tx,
        ctx,
        cmd.feed_key,
        if cmd.enabled { "enabled" } else { "disabled" },
        row.revision,
        updated.revision,
        updated.enabled,
        updated.filter.clone(),
        updated.fresh_within_hours,
    )
    .await?;

    let names = filter_names(&mut tx, ctx.organization_id).await?;
    let updated_fields = StoredFeedFields {
        enabled: updated.enabled,
        filter: updated.filter,
        fresh_within_hours: updated.fresh_within_hours,
        revision: updated.revision,
        updated_at: updated.updated_at,
        updated_by_user_id: updated.updated_by_user_id,
    };
    let feed = build_feed_view(
        &mut tx,
        ctx.organization_id,
        cmd.feed_key,
        Some(&updated_fields),
        &names,
    )
    .await?;
    tx.commit().await?;
    Ok(TodaySystemFeedOutcome {
        feed,
        changed: true,
    })
}

#[allow(clippy::too_many_arguments)]
async fn write_fact(
    tx: &mut PgConnection,
    ctx: &CommandContext,
    feed_key: FeedKey,
    change: &'static str,
    from_revision: i64,
    to_revision: i64,
    enabled_after: bool,
    filter_after: Option<String>,
    fresh_within_hours_after: Option<i32>,
) -> Result<(), TodayFeedError> {
    let envelope = crate::domain::envelope::FactEnvelope {
        organization_id: ctx.organization_id,
        actor: Actor::User(ctx.actor_user_id),
        on_behalf_of_user_id: None,
        origin: ctx.origin,
        occurred_at: Utc::now(),
        correlation_id: ctx.correlation_id,
        causation_id: None,
    };
    facts::insert_today_feed_changed(
        tx,
        &envelope,
        TodayFeedChangedFact {
            feed_key: feed_key.as_str(),
            change,
            from_revision,
            to_revision,
            enabled_after,
            filter_after,
            fresh_within_hours_after,
        },
    )
    .await?;
    Ok(())
}

// --- Preview (spec §4) ------------------------------------------------------

pub struct PreviewTodaySystemFeed {
    pub feed_key: FeedKey,
    pub filter: FilterDefinition,
    pub fresh_within_hours: Option<i32>,
    pub subject: UserId,
}

#[derive(Debug, Clone)]
pub struct PreviewOutcome {
    pub subject: UserId,
    pub subject_display_name: String,
    pub items: Vec<TodayItem>,
    pub truncated: bool,
    pub description: Vec<String>,
}

const PREVIEW_STATEMENT_TIMEOUT_MS: i64 = 1_250;

/// docs/specs/SLICE_011d.md §4: admin-only, validates like Update (the
/// definition need not be saved), verifies the subject is a current active
/// member of the actor's Organization, evaluates THAT ONE feed alone for
/// the subject (the other person-state feed treated as disabled), same
/// statements as §5, capped at 200 with `truncated`, ordered as the feed
/// orders inside Today. Read-only transaction with the two `SET LOCAL`
/// settings and a 1,250 ms statement timeout that is an error (503), never
/// a partial result. Persists nothing.
#[tracing::instrument(
    name = "today_feed.preview",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        feed_key = cmd.feed_key.as_str(),
        filter_kinds = %cmd.filter.kinds_field(),
        outcome = tracing::field::Empty,
    )
)]
pub async fn preview_today_system_feed(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: PreviewTodaySystemFeed,
) -> Result<PreviewOutcome, TodayFeedError> {
    let result = preview_today_system_feed_attempt(pool, ctx, cmd).await;
    record_command_outcome(&result);
    result
}

async fn preview_today_system_feed_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: PreviewTodaySystemFeed,
) -> Result<PreviewOutcome, TodayFeedError> {
    let mut tx = pool.begin().await?;
    // PostgreSQL requires `SET TRANSACTION ISOLATION LEVEL` to be the FIRST
    // statement of a transaction ("SET TRANSACTION ISOLATION LEVEL must be
    // called before any query", error 25001), so it runs alone, before the
    // membership lock. The READ ONLY access mode is set SEPARATELY, only
    // after the membership `FOR SHARE` lock and the validation/subject
    // reads below — `SELECT ... FOR SHARE` is itself rejected once a
    // transaction is already read-only ("cannot execute SELECT FOR SHARE
    // in a read-only transaction", error 25006), so those checks must run
    // in the still-read-write window. Only the heavy evaluation queries
    // after this point need the read-only/timeout guarantees.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    crate::auth::workspace::ordinary(&mut tx, ctx.organization_id).await?;

    lock_current_membership_require_admin(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    validate_feed_definition_structural(&cmd.filter)?;
    // F3: the subject's membership (404) is checked BEFORE reference
    // validation/the §1 rules (422) — spec §4 orders preview's subject
    // check ahead of definition-reference validation, the opposite of
    // update's revision-before-references ordering above (preview has no
    // revision to protect; its distinguishing early check is WHO the
    // preview is for).
    require_current_active_member(&mut tx, ctx.organization_id, cmd.subject).await?;
    validate_feed_definition_references_and_rules(
        &mut tx,
        ctx.organization_id,
        cmd.feed_key,
        &cmd.filter,
        cmd.fresh_within_hours,
    )
    .await?;

    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL jit = off").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL enable_mergejoin = off")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(PREVIEW_STATEMENT_TIMEOUT_MS.to_string())
        .execute(&mut *tx)
        .await?;

    #[cfg(feature = "test-support")]
    crate::domain::today::test_support::checkpoint(
        crate::domain::today::test_support::TodayQueryPhase::PreviewBeforeEvaluation,
        None,
        None,
        &mut tx,
    )
    .await?;

    let now: DateTime<Utc> = sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut *tx)
        .await?;

    let (items, truncated) = match cmd.feed_key {
        FeedKey::UnansweredInquiry | FeedKey::ClientReplied => {
            // The candidate definition stands in for ITS feed; the other
            // person-state feed is treated as disabled (spec §4) — never
            // both enabled together, so no cross-feed precedence applies
            // during preview.
            let disabled_other_key = if cmd.feed_key == FeedKey::UnansweredInquiry {
                FeedKey::ClientReplied
            } else {
                FeedKey::UnansweredInquiry
            };
            let disabled_other = super::ResolvedFeed {
                feed_key: disabled_other_key,
                enabled: false,
                filter: canonical_default(disabled_other_key),
                fresh_within_hours: canonical_fresh_within_hours(disabled_other_key),
                is_default: true,
                fallback: false,
                revision: 1,
                updated_at: now,
                updated_by_user_id: None,
            };
            let candidate = super::ResolvedFeed {
                feed_key: cmd.feed_key,
                enabled: true,
                filter: cmd.filter.clone(),
                fresh_within_hours: cmd.fresh_within_hours,
                is_default: false,
                fallback: false,
                revision: 1,
                updated_at: now,
                updated_by_user_id: None,
            };
            let (feed_a, feed_b) = if cmd.feed_key == FeedKey::UnansweredInquiry {
                (&candidate, &disabled_other)
            } else {
                (&disabled_other, &candidate)
            };
            let (candidates, truncated) = evaluate::person_state_candidates(
                &mut tx,
                ctx.organization_id,
                cmd.subject,
                now,
                feed_a,
                feed_b,
            )
            .await?;
            let mut items = rank(candidates, now);
            let extra_truncated = items.len() > 200;
            items.truncate(200);
            (items, truncated || extra_truncated)
        }
        FeedKey::CallOutcomeNeeded => {
            // The candidate definition is bound as the call feed's full
            // predicate matrix (spec §1 rule 4, §5 step 4 "feed C matrix
            // params") — an admin-added clause narrows the call-only
            // candidates exactly as it would in Today evaluation.
            let candidate = super::ResolvedFeed {
                feed_key: cmd.feed_key,
                enabled: true,
                filter: cmd.filter.clone(),
                fresh_within_hours: cmd.fresh_within_hours,
                is_default: false,
                fallback: false,
                revision: 1,
                updated_at: now,
                updated_by_user_id: None,
            };
            let call_only = evaluate::call_only_candidates(
                &mut tx,
                ctx.organization_id,
                cmd.subject,
                &candidate,
                &[],
                201,
                now,
            )
            .await?;
            let truncated = call_only.len() > 200;
            let mut items = rank(call_only, now);
            items.truncate(200);
            (items, truncated)
        }
    };

    let mut names = filter_names(&mut tx, ctx.organization_id).await?;
    let field_ids = cmd.filter.custom_field_ids();
    let (custom_field_names, custom_option_names) =
        custom_field::filter_names_for_fields(&mut tx, ctx.organization_id, &field_ids).await?;
    names.custom_field_names = custom_field_names;
    names.custom_option_names = custom_option_names;
    let description = cmd.filter.describe(&names);
    let subject_display_name = names
        .user_names
        .get(&cmd.subject)
        .cloned()
        .unwrap_or_default();

    // Read-only preview; nothing to commit but roll back cleanly so the
    // pooled connection returns unlocked.
    tx.rollback().await?;

    Ok(PreviewOutcome {
        subject: cmd.subject,
        subject_display_name,
        items,
        truncated,
        description,
    })
}

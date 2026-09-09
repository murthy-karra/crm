//! DB-backed tests for Slice 011d step 4's typed admin commands
//! (`UpdateTodaySystemFeed`, `RevertTodaySystemFeed`,
//! `SetTodaySystemFeedEnabled`), their no-op/canonical-collapse/fact
//! rules, revision-conflict precedence, feed rule validation, and
//! authorization/tenant isolation (docs/specs/SLICE_011d.md §§4, 9.3,
//! 9.4). Split from a single db_today_system_feed_commands.rs (item 3 of
//! the LATER batch, docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies
//! changed): the evaluation cases (§9.5) moved to
//! db_today_system_feed_evaluation.rs, and preview/telemetry (§§9.6, 9.9)
//! plus the tester-round dual-membership/stale-resend cases moved to
//! db_today_system_feed_preview.rs. The shared fixture helpers
//! (`create_org_with_admin`, `command_context`, `insert_person`, etc.)
//! moved to tests/common/today_system_feed.rs, used by all three files.
//! Run only via ./scripts/check-db.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::Barrier;
use tokio::time::timeout;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::today::system_feeds::commands::{
    self, PreviewTodaySystemFeed, RevertTodaySystemFeed, SetTodaySystemFeedEnabled,
    UpdateTodaySystemFeed,
};
use crm_api::domain::today::system_feeds::error::TodayFeedError;
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::{StageId, UserId};

use crate::common::today_system_feed::*;

// --- Tester round 1, T4: a deterministic revert-fact sequence ---------------

/// update (1→2, a stage clause and window 48) → revert (2→3): exactly two
/// facts ordered by `to_revision`, the second `change = 'reverted'` with
/// `filter_after`/`fresh_within_hours_after` NULL; then disable (3→4)
/// with `enabled_after = false`. Every fact's envelope columns
/// (actor_kind, actor_user_id, origin, correlation_id) equal the
/// `CommandContext` used for the write that produced it.
#[sqlx::test]
#[ignore]
async fn a_deterministic_update_revert_disable_sequence_writes_exactly_three_ordered_facts(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d t4 revert facts",
        "admin@d011-t4-revert-facts.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;

    let update_ctx = command_context(organization_id, admin_id);
    let customized = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_id)],
            }),
        ],
    };
    let update_outcome = commands::update_today_system_feed(
        &app_pool,
        &update_ctx,
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: customized,
            fresh_within_hours: Some(48),
        },
    )
    .await
    .unwrap();
    assert!(update_outcome.changed);
    assert_eq!(update_outcome.feed.revision, 2);

    let revert_ctx = command_context(organization_id, admin_id);
    let revert_outcome = commands::revert_today_system_feed(
        &app_pool,
        &revert_ctx,
        commands::RevertTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 2,
        },
    )
    .await
    .unwrap();
    assert!(revert_outcome.changed);
    assert_eq!(revert_outcome.feed.revision, 3);

    let disable_ctx = command_context(organization_id, admin_id);
    let disable_outcome = commands::set_today_system_feed_enabled(
        &app_pool,
        &disable_ctx,
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 3,
            enabled: false,
        },
    )
    .await
    .unwrap();
    assert!(disable_outcome.changed);
    assert_eq!(disable_outcome.feed.revision, 4);

    #[derive(sqlx::FromRow, Debug)]
    struct FactRow {
        change: String,
        from_revision: i64,
        to_revision: i64,
        enabled_after: bool,
        filter_after: Option<String>,
        fresh_within_hours_after: Option<i32>,
        actor_kind: String,
        actor_user_id: Option<Uuid>,
        origin: String,
        correlation_id: Uuid,
    }
    let facts: Vec<FactRow> = sqlx::query_as(
        "SELECT change, from_revision, to_revision, enabled_after, \
                filter_after::text as filter_after, \
                fresh_within_hours_after, actor_kind, actor_user_id, origin, correlation_id \
         FROM today_feed_changed WHERE organization_id = $1 ORDER BY to_revision",
    )
    .bind(organization_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    assert_eq!(facts.len(), 3, "exactly three facts, one per real change");

    let update_fact = &facts[0];
    assert_eq!(update_fact.change, "updated");
    assert_eq!(update_fact.from_revision, 1);
    assert_eq!(update_fact.to_revision, 2);
    assert!(update_fact.enabled_after);
    assert!(update_fact.filter_after.is_some());
    assert_eq!(update_fact.fresh_within_hours_after, Some(48));
    assert_eq!(update_fact.actor_kind, "user");
    assert_eq!(update_fact.actor_user_id, Some(admin_id));
    assert_eq!(update_fact.origin, "web_session");
    assert_eq!(update_fact.correlation_id, update_ctx.correlation_id.0);

    let revert_fact = &facts[1];
    assert_eq!(revert_fact.change, "reverted");
    assert_eq!(revert_fact.from_revision, 2);
    assert_eq!(revert_fact.to_revision, 3);
    assert!(revert_fact.enabled_after);
    assert!(
        revert_fact.filter_after.is_none(),
        "reverted collapses filter_after to NULL"
    );
    assert!(
        revert_fact.fresh_within_hours_after.is_none(),
        "reverted collapses fresh_within_hours_after to NULL"
    );
    assert_eq!(revert_fact.actor_kind, "user");
    assert_eq!(revert_fact.actor_user_id, Some(admin_id));
    assert_eq!(revert_fact.origin, "web_session");
    assert_eq!(revert_fact.correlation_id, revert_ctx.correlation_id.0);

    let disable_fact = &facts[2];
    assert_eq!(disable_fact.change, "disabled");
    assert_eq!(disable_fact.from_revision, 3);
    assert_eq!(disable_fact.to_revision, 4);
    assert!(!disable_fact.enabled_after);
    assert_eq!(disable_fact.actor_kind, "user");
    assert_eq!(disable_fact.actor_user_id, Some(admin_id));
    assert_eq!(disable_fact.origin, "web_session");
    assert_eq!(disable_fact.correlation_id, disable_ctx.correlation_id.0);
}

// --- §9.3: no-op, canonical collapse, revision, facts -----------------------

#[sqlx::test]
#[ignore]
async fn update_no_op_detection_canonical_collapse_and_one_fact_per_real_change(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands no-op",
        "admin@d011-commands-noop.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    assert_eq!(fact_count(&app_pool, organization_id).await, 0);

    // Submitting the CANONICAL definition explicitly is a successful no-op
    // (it collapses to the already-NULL stored state) — spec §4.
    let outcome = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();
    assert!(!outcome.changed);
    assert_eq!(outcome.feed.revision, 1);
    assert!(outcome.feed.is_default);
    assert_eq!(fact_count(&app_pool, organization_id).await, 0);
    let (_, filter, hours, revision) =
        feed_row(&app_pool, organization_id, FeedKey::UnansweredInquiry).await;
    assert!(filter.is_none());
    assert!(hours.is_none());
    assert_eq!(revision, 1);

    // A genuine change (adds a stage clause) bumps the revision and writes
    // exactly one fact.
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let mut customized = canonical_unanswered_filter();
    customized.clauses.push(Clause::Stage(StageClause {
        stage_ids: vec![StageId::new(stage_id)],
    }));
    let outcome = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: customized.clone(),
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.feed.revision, 2);
    assert!(!outcome.feed.is_default);
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);
    let (_, filter, hours, revision) =
        feed_row(&app_pool, organization_id, FeedKey::UnansweredInquiry).await;
    assert!(
        filter.is_some(),
        "a non-canonical filter is stored, not collapsed to NULL"
    );
    // fresh_within_hours EQUALS the canonical 24, so it still collapses to
    // NULL per-column even though the filter itself changed.
    assert!(
        hours.is_none(),
        "fresh_within_hours collapses to NULL independently of filter"
    );
    assert_eq!(revision, 2);

    let fact: (String, i64, i64, bool) = sqlx::query_as(
        "SELECT change, from_revision, to_revision, enabled_after FROM today_feed_changed \
         WHERE organization_id = $1",
    )
    .bind(organization_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    assert_eq!(fact.0, "updated");
    assert_eq!(fact.1, 1);
    assert_eq!(fact.2, 2);
    assert!(fact.3);

    // Re-submitting the SAME (now-stored) definition at the new revision is
    // a no-op again: no new fact.
    let outcome = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 2,
            filter: customized,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();
    assert!(!outcome.changed);
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);
}

#[sqlx::test]
#[ignore]
async fn revert_and_enabled_no_op_detection_and_facts(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands revert enable",
        "admin@d011-commands-revert.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    // Revert on an already-default feed is a no-op.
    let outcome = commands::revert_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        RevertTodaySystemFeed {
            feed_key: FeedKey::ClientReplied,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(!outcome.changed);
    assert_eq!(fact_count(&app_pool, organization_id).await, 0);

    // Disable, then re-disable (no-op), then enable, then revert (real,
    // since it was disabled — enabled state changed, so revert changes
    // enabled_after implicitly through a prior disable).
    let disabled = commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::ClientReplied,
            expected_revision: 1,
            enabled: false,
        },
    )
    .await
    .unwrap();
    assert!(disabled.changed);
    assert_eq!(disabled.feed.revision, 2);
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);

    let noop = commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::ClientReplied,
            expected_revision: 2,
            enabled: false,
        },
    )
    .await
    .unwrap();
    assert!(!noop.changed);
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);

    let reverted = commands::revert_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        RevertTodaySystemFeed {
            feed_key: FeedKey::ClientReplied,
            expected_revision: 2,
        },
    )
    .await
    .unwrap();
    // Revert only clears filter/fresh_within_hours (already NULL here), so
    // this is ALSO a no-op — enabled is untouched by revert.
    assert!(!reverted.changed);
    assert!(!reverted.feed.enabled, "revert does not touch enabled");
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);
}

#[sqlx::test]
#[ignore]
async fn update_and_revert_racing_the_same_expected_revision_exactly_one_wins(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands race",
        "admin@d011-commands-race.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let mut customized = canonical_unanswered_filter();
    customized.clauses.push(Clause::Stage(StageClause {
        stage_ids: vec![StageId::new(stage_id)],
    }));

    // Bring the feed to a REAL non-default state first (revision 2), so
    // Update and Revert can race as two REAL, mutually exclusive writes at
    // the SAME `expected_revision` — a plain no-op revert (spec-legitimate
    // but non-deterministic under a race) would leave the outcome
    // ambiguous, so this starts past that case deliberately.
    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: customized.clone(),
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();

    // A DIFFERENT valid customization from `customized` above — a wider
    // `assigned_to` set, not a second `AssignedTo` clause (a filter may
    // carry at most one clause per kind).
    let other_customized = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::Unassigned],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    };

    let update_pool = app_pool.clone();
    let revert_pool = app_pool.clone();
    let update_ctx = command_context(organization_id, admin_id);
    let revert_ctx = command_context(organization_id, admin_id);

    let update_task = tokio::spawn(async move {
        commands::update_today_system_feed(
            &update_pool,
            &update_ctx,
            UpdateTodaySystemFeed {
                feed_key: FeedKey::UnansweredInquiry,
                expected_revision: 2,
                filter: other_customized,
                fresh_within_hours: Some(24),
            },
        )
        .await
    });
    let revert_task = tokio::spawn(async move {
        commands::revert_today_system_feed(
            &revert_pool,
            &revert_ctx,
            RevertTodaySystemFeed {
                feed_key: FeedKey::UnansweredInquiry,
                expected_revision: 2,
            },
        )
        .await
    });

    let (update_result, revert_result) = tokio::join!(update_task, revert_task);
    let update_result = update_result.unwrap();
    let revert_result = revert_result.unwrap();

    // The `today-feeds:` advisory lock (spec §4) fully serializes the two
    // commands: whichever acquires it first reads revision 2, matches its
    // `expected_revision`, writes a REAL change, and commits; the other
    // then reads the bumped revision, no longer matches, and gets
    // `Conflict`. Exactly one of the two ever succeeds.
    let outcomes = [
        matches!(&update_result, Ok(o) if o.changed),
        matches!(&revert_result, Ok(o) if o.changed),
    ];
    assert_eq!(
        outcomes.iter().filter(|&&won| won).count(),
        1,
        "exactly one of update/revert must win the race: update={update_result:?} revert={revert_result:?}"
    );
    for result in [&update_result, &revert_result] {
        assert!(
            matches!(result, Ok(o) if o.changed) || matches!(result, Err(TodayFeedError::Conflict)),
            "the loser must see Conflict, not any other error: {result:?}"
        );
    }
    let (_, _, _, revision) =
        feed_row(&app_pool, organization_id, FeedKey::UnansweredInquiry).await;
    assert_eq!(
        revision, 3,
        "exactly one real write landed on top of the seeded revision 2"
    );
}

// --- Review round 1, F3: 409-before-422 / 404-before-422 precedence ---------

/// Update: a stale `expected_revision` must return `Conflict` (409) even
/// when the submitted definition would ALSO fail the §1 rules (422) — the
/// revision check now runs (and this test proves it runs) BEFORE
/// reference/rule validation. Real revision is 1; the command claims 2 AND
/// removes the anchor clause; if validation ran first, this would
/// (wrongly) surface `InvalidFeedRule` instead of `Conflict`.
#[sqlx::test]
#[ignore]
async fn update_returns_conflict_before_invalid_feed_rule_on_a_stale_revision(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d f3 update precedence",
        "admin@d011-f3-update.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let no_anchor = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::Me],
        })],
    };
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 2, // stale: the real revision is 1
            filter: no_anchor,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, TodayFeedError::Conflict),
        "the stale revision must win over the ALSO-invalid definition: {err:?}"
    );
}

/// Preview: an inactive/unknown subject must return `NotFound` (404) even
/// when the submitted definition would ALSO fail reference validation
/// (422 `invalid_stage`) — the subject check now runs (and this test
/// proves it runs) BEFORE reference validation.
#[sqlx::test]
#[ignore]
async fn preview_returns_not_found_before_invalid_stage_on_an_inactive_subject(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d f3 preview precedence",
        "admin@d011-f3-preview.test",
    )
    .await;
    let inactive_id = crate::common::create_user(
        &migrator_pool,
        "inactive@d011-f3-preview.test",
        "Inactive",
        PW,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        inactive_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let invalid_stage_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(Uuid::new_v4())],
            }),
        ],
    };
    let err = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: invalid_stage_filter,
            fresh_within_hours: Some(24),
            subject: UserId::new(inactive_id),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, TodayFeedError::NotFound),
        "the inactive subject must win over the ALSO-invalid stage reference: {err:?}"
    );
}

// --- §4 feed rule validation -------------------------------------------------

#[sqlx::test]
#[ignore]
async fn feed_rule_violations_are_rejected_before_any_write(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands rule violations",
        "admin@d011-rule-violations.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    // Anchor clause removed.
    let no_anchor = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::Me],
        })],
    };
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: no_anchor,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::InvalidFeedRule));

    // Anchor negated (value: false) — still counts as "not present".
    let negated_anchor = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: false }),
        ],
    };
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: negated_anchor,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::InvalidFeedRule));

    // `me` removed from assigned_to.
    let no_me = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Unassigned],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    };
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: no_me,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::InvalidFeedRule));

    // fresh_within_hours out of bounds / absent for a person-state feed.
    for bad_hours in [None, Some(0), Some(8761)] {
        let err = commands::update_today_system_feed(
            &app_pool,
            &command_context(organization_id, admin_id),
            UpdateTodaySystemFeed {
                feed_key: FeedKey::UnansweredInquiry,
                expected_revision: 1,
                filter: canonical_unanswered_filter(),
                fresh_within_hours: bad_hours,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, TodayFeedError::InvalidFeedRule),
            "{bad_hours:?}"
        );
    }

    // fresh_within_hours present for the call feed (forbidden).
    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
    };
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: 1,
            filter: call_filter,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::InvalidFeedRule));

    // No writes occurred from any of the rejected attempts.
    assert_eq!(fact_count(&app_pool, organization_id).await, 0);
    let (_, _, _, revision) =
        feed_row(&app_pool, organization_id, FeedKey::UnansweredInquiry).await;
    assert_eq!(revision, 1);
}

// --- §9.4: authorization and tenant isolation --------------------------------

#[sqlx::test]
#[ignore]
async fn member_is_forbidden_on_every_direct_command(migrator_pool: PgPool) {
    let (organization_id, member_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d commands member forbidden",
        "member@d011-member-forbidden.test",
        "Member",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, member_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::Forbidden));

    let err = commands::revert_today_system_feed(
        &app_pool,
        &command_context(organization_id, member_id),
        RevertTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::Forbidden));

    let err = commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, member_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            enabled: false,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::Forbidden));

    let err = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, member_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(member_id),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::Forbidden));
}

#[sqlx::test]
#[ignore]
async fn admin_of_organization_b_cannot_read_edit_preview_or_affect_organization_a(
    migrator_pool: PgPool,
) {
    let (org_a, admin_a) = create_org_with_admin(
        &migrator_pool,
        "011d commands tenant A",
        "admin@d011-tenant-a-cmd.test",
    )
    .await;
    let (org_b, admin_b) = create_org_with_admin(
        &migrator_pool,
        "011d commands tenant B",
        "admin@d011-tenant-b-cmd.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    // Admin B, scoped to org A by mistake (a forged/foreign context) — the
    // membership check uses org_a + admin_b, which does not exist, so the
    // SAME 404-shaped Unauthenticated outcome as any unknown actor.
    let err = commands::update_today_system_feed(
        &app_pool,
        &command_context(org_a, admin_b),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::Unauthenticated));

    // Org A's feed state is untouched.
    let (_, _, _, revision) = feed_row(&app_pool, org_a, FeedKey::UnansweredInquiry).await;
    assert_eq!(revision, 1);

    // Preview subject from a DIFFERENT organization is 404 (same as
    // unknown), even though admin_a is legitimately admin of org_a.
    let err = commands::preview_today_system_feed(
        &app_pool,
        &command_context(org_a, admin_a),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(admin_b),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::NotFound));

    let _ = org_b;
}

#[sqlx::test]
#[ignore]
async fn preview_subject_inactive_is_not_found(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands inactive subject",
        "admin@d011-inactive-subject.test",
    )
    .await;
    let inactive_id = crate::common::create_user(
        &migrator_pool,
        "inactive@d011-inactive-subject.test",
        "Inactive",
        PW,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        inactive_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let err = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(inactive_id),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TodayFeedError::NotFound));
}

/// spec §9.4: a demotion committed by a concurrent transaction, while this
/// command is genuinely blocked waiting on the SAME membership row's lock,
/// must be observed by the re-check — never a stale pre-demotion role.
/// Technique per `db_saved_lists.rs`'s
/// `membership_for_share_lock_rechecks_role_and_status_after_a_privilege_race`.
#[sqlx::test]
#[ignore]
async fn demoted_admin_loses_access_inside_the_command_transaction(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d commands demotion race",
        "admin@d011-demotion-race.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let mut role_tx = migrator_pool.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM organization_membership WHERE organization_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(organization_id)
    .bind(admin_id)
    .fetch_one(&mut *role_tx)
    .await
    .unwrap();

    let release = Arc::new(Barrier::new(2));
    let task_pool = app_pool.clone();
    let task_release = release.clone();
    let ctx = command_context(organization_id, admin_id);
    let mut command_task = tokio::spawn(async move {
        task_release.wait().await;
        commands::update_today_system_feed(
            &task_pool,
            &ctx,
            UpdateTodaySystemFeed {
                feed_key: FeedKey::UnansweredInquiry,
                expected_revision: 1,
                filter: canonical_unanswered_filter(),
                fresh_within_hours: Some(24),
            },
        )
        .await
    });
    release.wait().await;
    assert!(
        timeout(Duration::from_millis(75), &mut command_task)
            .await
            .is_err(),
        "the command must wait on the held membership lock"
    );

    sqlx::query(
        "UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&mut *role_tx)
    .await
    .unwrap();
    role_tx.commit().await.unwrap();

    let result = command_task.await.unwrap();
    assert!(matches!(result, Err(TodayFeedError::Forbidden)));
}

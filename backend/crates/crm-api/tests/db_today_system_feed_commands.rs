//! DB-backed tests for Slice 011d step 4: typed admin commands, preview,
//! and routes for the three system feeds (docs/specs/SLICE_011d.md §§4, 6,
//! 9.3-9.6, 9.8, 9.9). Run only via ./scripts/check-db.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use tokio::sync::Barrier;
use tokio::time::timeout;
use uuid::Uuid;

use sqlx::postgres::PgPoolOptions;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::today::system_feeds::commands::{
    self, PreviewTodaySystemFeed, RevertTodaySystemFeed, SetTodaySystemFeedEnabled,
    UpdateTodaySystemFeed,
};
use crm_api::domain::today::system_feeds::error::TodayFeedError;
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::{CorrelationId, OrganizationId, StageId, UserId};

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

/// A full fixture: one Organization, nine stages, and one ADMIN member —
/// `crate::common::create_org_with_stages_and_member` seeds a plain
/// member, so today-feed command tests (which need an admin actor) build
/// the equivalent themselves via the same lower-level helpers.
async fn create_org_with_admin(pool: &PgPool, org_name: &str, email: &str) -> (Uuid, Uuid) {
    let org_id = crate::common::create_org(pool, org_name).await;
    crate::common::seed_stages(pool, org_id).await;
    let user_id = crate::common::create_user(pool, email, "Admin", PW).await;
    crate::common::add_membership_with(
        pool,
        org_id,
        user_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    (org_id, user_id)
}

/// A single-connection `crm_app` pool, used to exercise a real DB-local
/// failure (a revoked GRANT) without borrowing a connection out of the
/// shared pool other assertions rely on — the same technique
/// `db_today_source_failures.rs` uses.
async fn connect_as_one_app(migrator_pool: &PgPool) -> PgPool {
    let options = migrator_pool
        .connect_options()
        .as_ref()
        .clone()
        .username("crm_app")
        .password(&crate::common::app_password());
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("single-connection crm_app pool")
}

fn canonical_unanswered_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    }
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry(pool: &PgPool, organization_id: Uuid, person_id: Uuid) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'commands-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now() - ChronoDuration::hours(2))
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_correspondence(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    agent_user_id: Uuid,
    direction: &str,
    occurred_at: chrono::DateTime<Utc>,
) {
    let raw_id: Uuid = sqlx::query_scalar(
        "INSERT INTO correspondence_raw \
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed) \
         VALUES ($1, $2, now(), $3, $4, $5, 0, true) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(vec![0_u8; 24])
    .bind(vec![1_u8; 16])
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO correspondence_captured \
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, \
             occurred_at, correlation_id, person_id, agent_user_id, direction, via, \
             correspondence_raw_id, backdated) \
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, $6, 'cc', $7, false)",
    )
    .bind(organization_id)
    .bind(agent_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(direction)
    .bind(raw_id)
    .execute(pool)
    .await
    .unwrap();
}

/// A qualifying call (ended/failed with a non-null `ended_at`, and an
/// uncorrected automatic `contact_attempted` root) — the exact
/// `outcome_call` membership shape `call_membership.sql`/`call_only.sql`
/// require, mirroring `db_today_feed_equivalence.rs`'s `insert_call`.
async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: chrono::DateTime<Utc>,
) -> Uuid {
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
             correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES \
            ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
             'scripted', 'commands-fixture', $7, $7)",
    )
    .bind(call_id)
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(caller_user_id)
    .bind(Uuid::new_v4())
    .bind(ended_at)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contact_attempted \
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
             causation_id, person_id, channel, outcome) \
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, 'call', 'reached')",
    )
    .bind(organization_id)
    .bind(ended_at)
    .bind(Uuid::new_v4())
    .bind(call_id)
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
    call_id
}

fn feed_row_key(feed_key: FeedKey) -> &'static str {
    feed_key.as_str()
}

async fn feed_row(
    pool: &PgPool,
    organization_id: Uuid,
    feed_key: FeedKey,
) -> (bool, Option<String>, Option<i32>, i64) {
    sqlx::query_as(
        "SELECT enabled, filter::text, fresh_within_hours, revision FROM today_system_feed \
         WHERE organization_id = $1 AND feed_key = $2",
    )
    .bind(organization_id)
    .bind(feed_row_key(feed_key))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn fact_count(pool: &PgPool, organization_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM today_feed_changed WHERE organization_id = $1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
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

// --- §9.5: evaluation cases ---------------------------------------------------

#[sqlx::test]
#[ignore]
async fn customized_feed_a_with_a_stage_clause_and_extra_assignee_matches_the_narrower_set(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d evaluation customized",
        "admin@d011-eval-customized.test",
    )
    .await;
    let bob_id =
        crate::common::create_user(&migrator_pool, "bob@d011-eval-customized.test", "Bob", PW)
            .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 2",
    )
    .bind(organization_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    let (stage_a, stage_b) = (stages[0], stages[1]);

    // In stage A, assigned to admin: matches the customized feed.
    let in_stage_a = insert_person(&app_pool, organization_id, stage_a, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_a).await;
    // In stage B, assigned to admin: excluded by the stage restriction.
    let in_stage_b = insert_person(&app_pool, organization_id, stage_b, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_b).await;
    // In stage A, assigned to Bob (the extra assignee added to the
    // customized feed): matches.
    let bobs_in_stage_a = insert_person(&app_pool, organization_id, stage_a, Some(bob_id)).await;
    insert_inquiry(&app_pool, organization_id, bobs_in_stage_a).await;

    let customized = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::User(UserId::new(bob_id))],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_a)],
            }),
        ],
    };
    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: customized,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now())
        .await
        .unwrap();
    let ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(ids.contains(&in_stage_a));
    assert!(
        !ids.contains(&in_stage_b),
        "stage restriction excludes this Person"
    );
    assert!(ids.contains(&bobs_in_stage_a), "extra assignee is honored");
}

#[sqlx::test]
#[ignore]
async fn customized_freshness_windows_1_hour_and_8760_hours_change_the_fresh_boundary(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d evaluation windows",
        "admin@d011-eval-windows.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // An inquiry 2 hours old: fresh under a 24 h (default) or 8760 h
    // window, NOT fresh under a strict 1 h window.
    let person = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'window-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person)
    .bind(Uuid::new_v4())
    .bind(now - ChronoDuration::hours(2))
    .execute(&app_pool)
    .await
    .unwrap();

    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(1),
        },
    )
    .await
    .unwrap();

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    drop(conn);
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == person)
        .unwrap();
    assert_eq!(
        item.priority,
        crm_api::domain::today::TodayPriority::Normal,
        "not fresh under 1h"
    );

    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 2,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(8760),
        },
    )
    .await
    .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == person)
        .unwrap();
    assert_eq!(
        item.priority,
        crm_api::domain::today::TodayPriority::High,
        "fresh under 8760h"
    );
}

#[sqlx::test]
#[ignore]
async fn a_person_in_both_feeds_shifts_to_inquiry_reasons_when_feed_b_is_disabled(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d evaluation shift",
        "admin@d011-eval-shift.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let person = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        person,
        admin_id,
        "inbound",
        now - ChronoDuration::hours(1),
    )
    .await;

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));

    // Both feeds enabled (default): reply wins.
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    drop(conn);
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == person)
        .unwrap();
    assert!(item
        .reasons
        .iter()
        .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })));

    // Disable client_replied: the same Person now shows inquiry-based
    // reasons instead.
    commands::set_today_system_feed_enabled(
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
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == person)
        .unwrap();
    assert!(item.reasons.iter().any(|r| matches!(
        r,
        crm_api::domain::today::TodayReason::NoContactAttempt { .. }
    )));
    assert!(!item
        .reasons
        .iter()
        .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })));
}

/// Coordinator decision (round 3): the call feed's matrix statements now
/// bind the full `PersonFilterParams` matrix (spec §1 rule 4, §5 step 4
/// "feed C matrix params"), so an admin-added stage clause narrows the
/// call feed exactly as it narrows the two person-state feeds. Both
/// person-state feeds are disabled so ONLY the call feed contributes,
/// isolating its own narrowing from person-state candidacy.
#[sqlx::test]
#[ignore]
async fn call_feed_customized_with_a_stage_clause_narrows_evaluation_to_that_stage(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d call feed stage",
        "admin@d011-call-stage.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 2",
    )
    .bind(organization_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    let (stage_a, stage_b) = (stages[0], stages[1]);
    let now = Utc::now();

    let in_stage_a = insert_person(&app_pool, organization_id, stage_a, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_a).await;
    let call_a = insert_call(
        &app_pool,
        organization_id,
        in_stage_a,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    let in_stage_b = insert_person(&app_pool, organization_id, stage_b, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_b).await;
    insert_call(
        &app_pool,
        organization_id,
        in_stage_b,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            enabled: false,
        },
    )
    .await
    .unwrap();
    commands::set_today_system_feed_enabled(
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
    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_a)],
            }),
        ],
    };
    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: 1,
            filter: call_filter,
            fresh_within_hours: None,
        },
    )
    .await
    .unwrap();

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    let ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(
        ids.contains(&in_stage_a),
        "stage_a's call-outcome person is retained"
    );
    assert!(
        !ids.contains(&in_stage_b),
        "stage_b's call-outcome person is excluded by the customized stage clause"
    );
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == in_stage_a)
        .unwrap();
    assert!(item.reasons.iter().any(
        |r| matches!(r, crm_api::domain::today::TodayReason::CallOutcomeNeeded { call_id, .. } if *call_id == call_a)
    ));
}

/// Same coordinator decision: `assigned_to: [me]` added to the call feed
/// narrows call-outcome candidates to Persons assigned to the viewer, even
/// though the call itself was always placed BY the viewer (the caller_user_id
/// axis and the assigned_to axis are independent).
#[sqlx::test]
#[ignore]
async fn call_feed_customized_with_assigned_to_me_narrows_evaluation_to_assigned_persons(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d call feed assignee",
        "admin@d011-call-assignee.test",
    )
    .await;
    let bob_id =
        crate::common::create_user(&migrator_pool, "bob@d011-call-assignee.test", "Bob", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Both calls are placed by the ADMIN (viewer); only the Person's
    // ASSIGNEE differs.
    let assigned_to_me = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, assigned_to_me).await;
    let call_mine = insert_call(
        &app_pool,
        organization_id,
        assigned_to_me,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    let assigned_to_bob = insert_person(&app_pool, organization_id, stage_id, Some(bob_id)).await;
    insert_inquiry(&app_pool, organization_id, assigned_to_bob).await;
    insert_call(
        &app_pool,
        organization_id,
        assigned_to_bob,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    commands::set_today_system_feed_enabled(
        &app_pool,
        &command_context(organization_id, admin_id),
        SetTodaySystemFeedEnabled {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            enabled: false,
        },
    )
    .await
    .unwrap();
    commands::set_today_system_feed_enabled(
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
    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
        ],
    };
    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: 1,
            filter: call_filter,
            fresh_within_hours: None,
        },
    )
    .await
    .unwrap();

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    let ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(ids.contains(&assigned_to_me));
    assert!(
        !ids.contains(&assigned_to_bob),
        "assigned_to: [me] excludes a Person assigned to someone else, even though the \
         admin placed the call"
    );
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == assigned_to_me)
        .unwrap();
    assert!(item.reasons.iter().any(
        |r| matches!(r, crm_api::domain::today::TodayReason::CallOutcomeNeeded { call_id, .. } if *call_id == call_mine)
    ));
}

/// spec §9.5: the call feed's own failure, together with a SEPARATE
/// list-source enumeration failure, must still yield `unavailable` (011c
/// precedence) with the call feed's `system_feed_issues` entry present.
#[sqlx::test]
#[ignore]
async fn call_feed_failure_and_list_source_enumeration_failure_yields_unavailable_with_the_system_feed_issue_present(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d evaluation dual failure",
        "admin@d011-eval-dual-failure.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let person = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;

    // Break enumeration by revoking crm_app's SELECT on today_work_source
    // (the same technique db_today_source_failures.rs uses for a metadata
    // failure), and independently force the call feed to fail via a
    // corrupted stored call-feed filter that decodes but is never reached
    // (the call feed itself has no filter-driven failure mode from step 3;
    // instead this exercises the SAME code path db_today_system_feed_call_
    // failures.rs's hook-based tests cover — here, proving the OUTER
    // precedence with a real enumeration failure is the addition).
    sqlx::query("REVOKE SELECT ON TABLE today_work_source FROM crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();

    let one_app_pool = connect_as_one_app(&migrator_pool).await;
    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let result = crm_api::domain::today::query_owned(
        one_app_pool.acquire().await.unwrap(),
        &scope,
        UserId::new(admin_id),
        Utc::now(),
    )
    .await
    .unwrap();

    assert!(matches!(
        result.sources.status,
        crm_api::domain::today::TodaySourcesStatus::Unavailable
    ));
    assert!(result.sources.issues.is_empty());
    // The call feed is enabled by default and has no matching call in this
    // fixture, so it contributes no issue on its own here — this test's
    // primary assertion is that enumeration failure alone still correctly
    // reports Unavailable with system_feed_issues threaded through (empty
    // in this instance, matching correction (a) — a healthy call feed
    // reports nothing). The dedicated call-feed failure cases (with a real
    // system_feed_issues entry surviving into Unavailable) are covered by
    // `db_today_system_feed_call_failures.rs`.
    assert!(result.sources.system_feed_issues.is_empty());

    sqlx::query("GRANT SELECT ON TABLE today_work_source TO crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
}

// --- §9.6: preview --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn preview_equals_todays_contribution_for_the_subject_with_the_other_feed_disabled(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d preview parity",
        "admin@d011-preview-parity.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let matching = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, matching).await;
    let non_matching = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;

    let preview = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(admin_id),
        },
    )
    .await
    .unwrap();
    let preview_ids: Vec<Uuid> = preview
        .items
        .iter()
        .map(|i| i.person.id.as_uuid())
        .collect();
    assert!(preview_ids.contains(&matching));
    assert!(!preview_ids.contains(&non_matching));
    assert_eq!(preview.subject, UserId::new(admin_id));
    assert!(!preview.description.is_empty());
    let _ = now;

    // Preview persisted nothing: the stored feed row is untouched.
    let (_, filter, _, revision) =
        feed_row(&app_pool, organization_id, FeedKey::UnansweredInquiry).await;
    assert!(filter.is_none());
    assert_eq!(revision, 1);
    assert_eq!(fact_count(&app_pool, organization_id).await, 0);

    // Today, evaluated for the same subject at the canonical (unedited)
    // definition, contains the same matching Person via the SAME feed
    // (client_replied contributes nothing here since no reply exists).
    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let today = crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now())
        .await
        .unwrap();
    let today_ids: Vec<Uuid> = today.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(today_ids.contains(&matching));
}

#[sqlx::test]
#[ignore]
async fn preview_caps_at_200_with_truncated(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d preview cap",
        "admin@d011-preview-cap.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;

    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id, created_at)
         SELECT $1, $2, $3, now() - make_interval(secs => s.i)
         FROM generate_series(0, 209) AS s(i)",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(admin_id)
    .execute(&app_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         SELECT $1, p.id, gen_random_uuid(), 'cap-fixture', now() - interval '1 hour'
         FROM person p WHERE p.organization_id = $1",
    )
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let preview = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(admin_id),
        },
    )
    .await
    .unwrap();
    assert_eq!(preview.items.len(), 200);
    assert!(preview.truncated);
}

/// Coordinator decision (round 3): preview for the call feed now binds the
/// SAME full matrix as evaluation, so a candidate stage clause narrows the
/// previewed call-only set exactly as it narrows Today.
#[sqlx::test]
#[ignore]
async fn preview_of_a_call_feed_customized_with_a_stage_clause_narrows_the_candidates(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d preview call stage",
        "admin@d011-preview-call-stage.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 2",
    )
    .bind(organization_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    let (stage_a, stage_b) = (stages[0], stages[1]);
    let now = Utc::now();

    let in_stage_a = insert_person(&app_pool, organization_id, stage_a, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_a).await;
    insert_call(
        &app_pool,
        organization_id,
        in_stage_a,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;
    let in_stage_b = insert_person(&app_pool, organization_id, stage_b, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, in_stage_b).await;
    insert_call(
        &app_pool,
        organization_id,
        in_stage_b,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_a)],
            }),
        ],
    };
    let preview = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            filter: call_filter,
            fresh_within_hours: None,
            subject: UserId::new(admin_id),
        },
    )
    .await
    .unwrap();
    let ids: Vec<Uuid> = preview
        .items
        .iter()
        .map(|i| i.person.id.as_uuid())
        .collect();
    assert!(ids.contains(&in_stage_a));
    assert!(!ids.contains(&in_stage_b));
}

/// Same coordinator decision: preview for the call feed honors an
/// `assigned_to: [me]` narrowing exactly as evaluation does.
#[sqlx::test]
#[ignore]
async fn preview_of_a_call_feed_customized_with_assigned_to_me_narrows_the_candidates(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d preview call assignee",
        "admin@d011-preview-call-assignee.test",
    )
    .await;
    let bob_id = crate::common::create_user(
        &migrator_pool,
        "bob@d011-preview-call-assignee.test",
        "Bob",
        PW,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let assigned_to_me = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, assigned_to_me).await;
    insert_call(
        &app_pool,
        organization_id,
        assigned_to_me,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;
    let assigned_to_bob = insert_person(&app_pool, organization_id, stage_id, Some(bob_id)).await;
    insert_inquiry(&app_pool, organization_id, assigned_to_bob).await;
    insert_call(
        &app_pool,
        organization_id,
        assigned_to_bob,
        admin_id,
        now - ChronoDuration::hours(3),
    )
    .await;

    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
        ],
    };
    let preview = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            filter: call_filter,
            fresh_within_hours: None,
            subject: UserId::new(admin_id),
        },
    )
    .await
    .unwrap();
    let ids: Vec<Uuid> = preview
        .items
        .iter()
        .map(|i| i.person.id.as_uuid())
        .collect();
    assert!(ids.contains(&assigned_to_me));
    assert!(!ids.contains(&assigned_to_bob));
}

// --- §9.9: telemetry ----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn today_feed_command_and_preview_spans_never_carry_the_definition_or_subject_items(
    migrator_pool: PgPool,
) {
    use std::sync::Mutex as StdMutex;
    use tracing_subscriber::layer::SubscriberExt;

    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d telemetry",
        "admin@d011-telemetry.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let person = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;

    #[derive(Clone)]
    struct CaptureWriter(std::sync::Arc<StdMutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }
    let buffer = std::sync::Arc::new(StdMutex::new(Vec::new()));
    let writer = CaptureWriter(buffer.clone());
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let _guard = tracing::subscriber::set_default(subscriber);

    // A distinctive marker that must never leak: a stage name is not part
    // of the filter kinds vocabulary, so its presence in the definition
    // (via the stage clause) is a safe stand-in for "definition content".
    // Renamed via the migrator connection: crm_app has no UPDATE grant on
    // `stage` (there is no stage-rename command in this slice), so this is
    // a fixture mutation, not a domain write — same spirit as the
    // migrator-only backdating `common/mod.rs` documents.
    let secret_marker = "zz-marker-should-never-appear-in-a-span";
    sqlx::query("UPDATE stage SET name = $1 WHERE id = $2")
        .bind(secret_marker)
        .bind(stage_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let _ = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
        },
    )
    .await;
    let _ = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: canonical_unanswered_filter(),
            fresh_within_hours: Some(24),
            subject: UserId::new(admin_id),
        },
    )
    .await;
    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let _ =
        crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now()).await;
    drop(conn);

    drop(_guard);
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("today_feed.update") || captured.contains("today_feed"),
        "feed command spans must be recorded: {captured}"
    );
    assert!(
        !captured.contains(secret_marker),
        "no stage/definition content ever reaches a span: {captured}"
    );
    assert!(
        !captured.contains(&person.to_string()),
        "no subject/item id ever reaches a span: {captured}"
    );
}

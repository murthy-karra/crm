//! DB-backed tests for Slice 011d step 4's system-feed evaluation cases
//! (§9.5, docs/specs/SLICE_011d.md): customized filter clauses narrowing
//! feed A/feed B, freshness-window boundaries, unassigned-person 503
//! avoidance, the call feed's bound-clock last-contact window and stage/
//! assignee narrowing, and fallback to canonical on a deleted stage or
//! unsupported stored JSON. Split from a single
//! db_today_system_feed_commands.rs (item 3 of the LATER batch,
//! docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies changed): the
//! typed admin commands (§§4, 9.3, 9.4) stayed in
//! db_today_system_feed_commands.rs, and preview/telemetry (§§9.6, 9.9)
//! plus the tester-round dual-membership/stale-resend cases moved to
//! db_today_system_feed_preview.rs. The shared fixture helpers
//! (`create_org_with_admin`, `command_context`, `insert_person`, etc.)
//! moved to tests/common/today_system_feed.rs, used by all three files.
//! Run only via ./scripts/check-db.

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::person::filter::{
    AgeClause, AgeSpec, AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    StageClause,
};
use crm_api::domain::today::system_feeds::commands::{
    self, SetTodaySystemFeedEnabled, UpdateTodaySystemFeed,
};
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::{OrganizationId, StageId, UserId};

use crate::common::today_system_feed::*;

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

/// Review round 1, F1 (BLOCKING): for an unassigned Person, the
/// `assigned_to` matrix term (`p.assigned_user_id = ANY($n) OR ($m AND
/// p.assigned_user_id IS NULL)`) evaluates to SQL NULL, not false, whenever
/// the OTHER feed's assignee list does not include `unassigned` — NULL
/// poisons that feed's whole matrix, and `by_inquiry!`/`by_reply!`'s
/// non-null projection then fails row decode, a 503 for the whole
/// Organization. Person qualifies via feed A (customized to admit
/// unassigned) with an inquiry AND an unanswered reply; feed B stays
/// canonical (`assigned_to: [me]` only, no `unassigned`) — its matrix is
/// NULL for this same unassigned Person, exercising exactly the fixed
/// `COALESCE(r.matrix_b, false)` path. Today must succeed with
/// inquiry-based reasons (feed A wins since feed B's by_reply is false,
/// not NULL).
#[sqlx::test]
#[ignore]
async fn an_unassigned_person_matching_only_the_customized_feed_does_not_503_feed_a(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d f1 unassigned feed a",
        "admin@d011-f1-unassigned-a.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let unassigned = insert_person(&app_pool, organization_id, stage_id, None).await;
    insert_inquiry(&app_pool, organization_id, unassigned).await; // received_at = now - 2h
    insert_correspondence(
        &app_pool,
        organization_id,
        unassigned,
        admin_id,
        "inbound",
        now - ChronoDuration::hours(1),
    )
    .await;

    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me, Assignee::Unassigned],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(24),
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
        .expect("Today must succeed, not 503, for this unassigned Person");
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == unassigned)
        .expect("the unassigned Person qualifies via the customized feed A");
    assert!(
        item.reasons.iter().any(|r| matches!(
            r,
            crm_api::domain::today::TodayReason::NewInquiry { .. }
        ) || matches!(
            r,
            crm_api::domain::today::TodayReason::RepeatInquiry { .. }
        ) || matches!(
            r,
            crm_api::domain::today::TodayReason::NoContactAttempt { .. }
        )),
        "feed A (customized, admits unassigned) wins: inquiry-based reasons"
    );
    assert!(
        !item
            .reasons
            .iter()
            .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })),
        "feed B (canonical, excludes unassigned) must NOT contribute — its matrix was NULL, \
         now correctly coalesced to false, not true"
    );
}

/// F1 mirror case: feed B customized to admit unassigned, feed A stays
/// canonical — exercises the fixed `COALESCE(r.matrix_a, false)` path.
#[sqlx::test]
#[ignore]
async fn an_unassigned_person_matching_only_the_customized_feed_does_not_503_feed_b(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d f1 unassigned feed b",
        "admin@d011-f1-unassigned-b.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let unassigned = insert_person(&app_pool, organization_id, stage_id, None).await;
    insert_inquiry(&app_pool, organization_id, unassigned).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        unassigned,
        admin_id,
        "inbound",
        now - ChronoDuration::hours(1),
    )
    .await;

    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::ClientReplied,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me, Assignee::Unassigned],
                    }),
                    Clause::ClientRepliedUnanswered(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(24),
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
        .expect("Today must succeed, not 503, for this unassigned Person");
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == unassigned)
        .expect("the unassigned Person qualifies via the customized feed B");
    assert!(
        item.reasons
            .iter()
            .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })),
        "feed B (customized, admits unassigned) wins: client_replied reason"
    );
}

/// Review round 1, F2: `call_membership.sql`/`call_only.sql` used
/// `now()` for age clauses instead of a bound clock, so `query_at`'s fixed
/// test clock was silently ignored for the call feed's own age axes.
/// `fixed_now` is deliberately far from real wall-clock time (3 real days
/// in the past) so the bug (real `now()` instead of the bound clock) would
/// visibly change the outcome. Covers both `call_membership` (the Person
/// is ALSO independently retained via person-state, so the call feed only
/// appends a reason) and `call_only` (the Person's inquiry is already
/// answered — not retained via person-state — so the call feed is the
/// SOLE reason the Person appears at all).
#[sqlx::test]
#[ignore]
async fn call_feed_customized_last_contact_window_honors_the_bound_clock_not_real_now(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d f2 call clock",
        "admin@d011-f2-call-clock.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now_real = Utc::now();
    let fixed_now = now_real - ChronoDuration::days(3);

    commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AwaitingCallOutcome(BoolClause { value: true }),
                    Clause::LastContact(AgeClause {
                        age: AgeSpec::WithinDays(1),
                    }),
                ],
            },
            fresh_within_hours: None,
        },
    )
    .await
    .unwrap();

    // call_membership: retained via feed A (fresh, unanswered inquiry at
    // real "now"), independent of the call's own age.
    let membership_included =
        insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, membership_included).await;
    let membership_included_call = insert_call(
        &app_pool,
        organization_id,
        membership_included,
        admin_id,
        fixed_now - ChronoDuration::days(1) + ChronoDuration::seconds(1),
    )
    .await;

    let membership_excluded =
        insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, membership_excluded).await;
    insert_call(
        &app_pool,
        organization_id,
        membership_excluded,
        admin_id,
        fixed_now - ChronoDuration::days(1) - ChronoDuration::seconds(1),
    )
    .await;

    // call_only: the inquiry PRECEDES the call's own automatic contact, so
    // awaiting_response is false and the Person is not retained via
    // person-state at all — the call feed is the only path onto Today.
    let call_only_included =
        insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'f2-call-only', $4)",
    )
    .bind(organization_id)
    .bind(call_only_included)
    .bind(Uuid::new_v4())
    .bind(now_real - ChronoDuration::days(10))
    .execute(&app_pool)
    .await
    .unwrap();
    let call_only_included_call = insert_call(
        &app_pool,
        organization_id,
        call_only_included,
        admin_id,
        fixed_now - ChronoDuration::days(1) + ChronoDuration::seconds(1),
    )
    .await;

    let call_only_excluded =
        insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'f2-call-only', $4)",
    )
    .bind(organization_id)
    .bind(call_only_excluded)
    .bind(Uuid::new_v4())
    .bind(now_real - ChronoDuration::days(10))
    .execute(&app_pool)
    .await
    .unwrap();
    insert_call(
        &app_pool,
        organization_id,
        call_only_excluded,
        admin_id,
        fixed_now - ChronoDuration::days(1) - ChronoDuration::seconds(1),
    )
    .await;

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list =
        crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), fixed_now)
            .await
            .unwrap();

    let membership_included_item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == membership_included)
        .expect("retained via person-state regardless of the call feed");
    assert!(membership_included_item.reasons.iter().any(
        |r| matches!(r, crm_api::domain::today::TodayReason::CallOutcomeNeeded { call_id, .. } if *call_id == membership_included_call)
    ), "the call, 1s inside the 1-day window from the BOUND clock, is included");

    let membership_excluded_item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == membership_excluded)
        .expect("still retained via person-state");
    assert!(
        !membership_excluded_item.reasons.iter().any(|r| matches!(
            r,
            crm_api::domain::today::TodayReason::CallOutcomeNeeded { .. }
        )),
        "the call, 1s outside the 1-day window from the BOUND clock, must be excluded"
    );

    let call_only_ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(
        call_only_ids.contains(&call_only_included),
        "call_only: 1s inside the window, the Person's only path onto Today"
    );
    let call_only_item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == call_only_included)
        .unwrap();
    assert!(call_only_item.reasons.iter().any(
        |r| matches!(r, crm_api::domain::today::TodayReason::CallOutcomeNeeded { call_id, .. } if *call_id == call_only_included_call)
    ));
    assert!(
        !call_only_ids.contains(&call_only_excluded),
        "call_only: 1s outside the window, excluded entirely (no person-state path either)"
    );
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

/// Coverage gap (1): a dedicated EVALUATION-path test (not just
/// `load_feed_rows`'s unit-level read, covered by
/// `db_today_system_feeds.rs`'s `invalid_stage_reference_falls_back_to_
/// canonical`) proving that a stored feed definition referencing a stage
/// which is later deleted falls back to the CANONICAL rule inside
/// `today::query_at` itself — a Person who matches the canonical rule but
/// NOT the (now-broken) stage restriction still appears — with
/// `system_feed_issues` carrying `fallback: true`.
#[sqlx::test]
#[ignore]
async fn a_stored_feed_referencing_a_deleted_stage_falls_back_to_canonical_in_evaluation(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d fallback deleted stage",
        "admin@d011-fallback-stage.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stages: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position")
            .bind(organization_id)
            .fetch_all(&app_pool)
            .await
            .unwrap();
    let referenced_stage = stages[8];
    let other_stage = stages[0];

    // Store a valid definition referencing `referenced_stage` (it exists
    // at write time, so this passes reference validation).
    let customized = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(referenced_stage)],
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

    // No Person is ever placed in `referenced_stage`, so deleting it hits
    // no foreign key.
    sqlx::query("DELETE FROM stage WHERE id = $1")
        .bind(referenced_stage)
        .execute(&migrator_pool)
        .await
        .unwrap();

    // A Person matching the CANONICAL rule (assigned to admin, fresh
    // unanswered inquiry) but in a DIFFERENT stage than the one the
    // (now-broken) stored definition referenced.
    let person = insert_person(&app_pool, organization_id, other_stage, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now())
        .await
        .unwrap();
    let ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.as_uuid()).collect();
    assert!(
        ids.contains(&person),
        "the canonical rule (no stage restriction) is evaluated under fallback"
    );
    assert_eq!(list.sources.system_feed_issues.len(), 1);
    let issue = &list.sources.system_feed_issues[0];
    assert_eq!(issue.feed_key, "unanswered_inquiry");
    assert!(matches!(
        issue.error,
        crm_api::domain::today::SystemFeedIssueError::InvalidDefinition
    ));
    assert!(issue.fallback);
}

/// Coverage gap (1), sibling case: unsupported stored JSON (a version this
/// binary does not decode) falls back the same way, through the same
/// evaluation path.
#[sqlx::test]
#[ignore]
async fn a_stored_feed_with_unsupported_json_falls_back_to_canonical_in_evaluation(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d fallback unsupported json",
        "admin@d011-fallback-json.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;

    sqlx::query(
        "UPDATE today_system_feed SET filter = '{\"version\":99,\"clauses\":[]}'::jsonb, \
         revision = revision + 1 WHERE organization_id = $1 AND feed_key = 'client_replied'",
    )
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let person = insert_person(&app_pool, organization_id, stage_id, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, person).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        person,
        admin_id,
        "inbound",
        Utc::now() - ChronoDuration::hours(1),
    )
    .await;

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now())
        .await
        .unwrap();
    let item = list
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == person)
        .unwrap();
    assert!(
        item.reasons
            .iter()
            .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })),
        "the canonical client_replied rule is evaluated under fallback"
    );
    assert_eq!(list.sources.system_feed_issues.len(), 1);
    let issue = &list.sources.system_feed_issues[0];
    assert_eq!(issue.feed_key, "client_replied");
    assert!(matches!(
        issue.error,
        crm_api::domain::today::SystemFeedIssueError::InvalidDefinition
    ));
    assert!(issue.fallback);
}

/// Tester round 1, T5 (part): revoking SELECT on `today_system_feed`
/// itself must surface as a genuine `today::query` error (not a silent
/// canonical fallback), and the same failure must reach `GET /api/today`
/// as 503 `unavailable`.
#[sqlx::test]
#[ignore]
async fn revoking_select_on_today_system_feed_is_a_query_error_and_a_503(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d t5 revoke today_system_feed",
        "admin@d011-t5-revoke-feed.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    sqlx::query("REVOKE SELECT ON TABLE today_system_feed FROM crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();

    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let result =
        crm_api::domain::today::query(&mut conn, &scope, UserId::new(admin_id), Utc::now()).await;
    assert!(
        result.is_err(),
        "today::query must surface the revoked-grant failure, not fall back"
    );
    drop(conn);

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-t5-revoke-feed.test", PW).await;
    let response = crate::common::get_with_cookie(&router, "/api/today", &cookie).await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "unavailable");

    sqlx::query("GRANT SELECT ON TABLE today_system_feed TO crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
}

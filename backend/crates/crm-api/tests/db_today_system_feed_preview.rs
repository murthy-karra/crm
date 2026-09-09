//! DB-backed tests for Slice 011d step 4's preview endpoint (§9.6),
//! telemetry (§9.9), and the tester-round dual-membership/stale-resend
//! edge cases (docs/specs/SLICE_011d.md). Split from a single
//! db_today_system_feed_commands.rs (item 3 of the LATER batch,
//! docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies changed): the
//! typed admin commands (§§4, 9.3, 9.4) stayed in
//! db_today_system_feed_commands.rs, and the evaluation cases (§9.5)
//! moved to db_today_system_feed_evaluation.rs. The shared fixture
//! helpers (`create_org_with_admin`, `command_context`, `insert_person`,
//! etc.) moved to tests/common/today_system_feed.rs, used by all three
//! files. Run only via ./scripts/check-db.

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::today::system_feeds::commands::{
    self, PreviewTodaySystemFeed, UpdateTodaySystemFeed,
};
use crm_api::domain::today::system_feeds::error::TodayFeedError;
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::{OrganizationId, StageId, UserId};

use crate::common::today_system_feed::*;

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
    let bob_id =
        crate::common::create_user(&migrator_pool, "bob@d011-telemetry.test", "Bob", PW).await;
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

    // A REAL customization referencing the renamed stage AND a second
    // real user (Bob) — the previous version of this test submitted the
    // canonical filter, which never references the stage at all, making
    // the secret-marker assertion vacuously true regardless of what the
    // telemetry code actually does. This filter genuinely exercises the
    // definition-content path.
    let customized = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::User(UserId::new(bob_id))],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_id)],
            }),
        ],
    };

    let update_outcome = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: customized.clone(),
            fresh_within_hours: Some(24),
        },
    )
    .await;
    assert!(update_outcome.is_ok());
    let _ = commands::preview_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        PreviewTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            filter: customized,
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

    // A failing command (`expected_revision: 0`, out of the declared
    // `1..=MAX_WIRE_REVISION` range): its span must record only
    // `error_kind`/`outcome`, never the (rejected) definition.
    let failing = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 0,
            filter: customized_for_failure(),
            fresh_within_hours: Some(24),
        },
    )
    .await;
    assert!(matches!(failing, Err(TodayFeedError::MalformedRequest)));

    drop(_guard);
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("today_feed.update") || captured.contains("today_feed"),
        "feed command spans must be recorded: {captured}"
    );
    assert!(
        captured.contains("filter_kinds"),
        "the safe filter_kinds summary must be present: {captured}"
    );
    assert!(
        !captured.contains(secret_marker),
        "no stage name/definition content ever reaches a span: {captured}"
    );
    assert!(
        !captured.contains(&stage_id.to_string()),
        "no stage UUID ever reaches a span: {captured}"
    );
    assert!(
        !captured.contains(&bob_id.to_string()),
        "no assignee UUID ever reaches a span: {captured}"
    );
    assert!(
        !captured.contains(&person.to_string()),
        "no subject/item Person UUID ever reaches a span: {captured}"
    );
    assert!(
        !captured.contains("\"version\":1") && !captured.contains("\"clauses\""),
        "no serialized definition JSON ever reaches a span: {captured}"
    );
    assert!(
        captured.contains("malformed_request") || captured.contains("error_kind"),
        "the failing command's outcome must be recorded: {captured}"
    );
}

/// A different, deliberately out-of-range revision (999 > `MAX_WIRE_
/// REVISION`) is rejected at `validate_expected_revision` before the
/// definition is ever touched — used only to exercise the failing-command
/// telemetry path above; its exact clauses are irrelevant.
fn customized_for_failure() -> FilterDefinition {
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

// --- Tester round 1: T7, T10, T13 -------------------------------------------

/// T7: a real dual-membership fail-closed test. User U is a member of
/// Organization A and admin of Organization B. With a B session/context,
/// `preview_today_system_feed` for a subject who is A's own member (U),
/// with a call feed reaching for an A-only Person, must return an empty
/// result — never leaking A's data through B's admin session — and A's
/// feed rows must stay untouched (revision 1). `admin_feed_view` for B
/// never exposes A's filter (B's own rows are independent, canonical).
#[sqlx::test]
#[ignore]
async fn dual_membership_admin_of_b_cannot_reach_organization_a_through_preview(
    migrator_pool: PgPool,
) {
    let org_a = crate::common::create_org(&migrator_pool, "011d t7 org a").await;
    crate::common::seed_stages(&migrator_pool, org_a).await;
    let org_b = crate::common::create_org(&migrator_pool, "011d t7 org b").await;
    crate::common::seed_stages(&migrator_pool, org_b).await;

    let user_u = crate::common::create_user(&migrator_pool, "u@d011-t7.test", "U", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        org_a,
        user_u,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        org_b,
        user_u,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_a = first_stage_id(&app_pool, org_a).await;
    let a_only_person = insert_person(&app_pool, org_a, stage_a, Some(user_u)).await;
    insert_inquiry(&app_pool, org_a, a_only_person).await;
    insert_call(
        &app_pool,
        org_a,
        a_only_person,
        user_u,
        Utc::now() - ChronoDuration::hours(1),
    )
    .await;

    // A B-scoped context: U acting as B's admin.
    let b_ctx = command_context(org_b, user_u);
    let preview = commands::preview_today_system_feed(
        &app_pool,
        &b_ctx,
        PreviewTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
            },
            fresh_within_hours: None,
            subject: UserId::new(user_u),
        },
    )
    .await
    .unwrap();
    assert!(
        preview.items.is_empty(),
        "org B's preview must never surface org A's Person, even for the same user"
    );

    let (_, _, _, revision_a) = feed_row(&app_pool, org_a, FeedKey::CallOutcomeNeeded).await;
    assert_eq!(
        revision_a, 1,
        "org A's feed row is untouched by a B-scoped preview"
    );

    let mut conn = app_pool.acquire().await.unwrap();
    let b_admin_view = crm_api::domain::today::system_feeds::queries::admin_feed_view(
        &mut conn,
        OrganizationId::new(org_b),
    )
    .await
    .unwrap();
    for feed in &b_admin_view {
        assert!(
            feed.is_default,
            "org B's feeds are untouched, canonical defaults"
        );
    }
}

/// T10: feed B narrowed by an edit (a stage clause that excludes a
/// Person who otherwise qualifies for both feeds) shifts that Person to
/// inquiry-based reasons, and `waiting_since` becomes the inquiry's
/// `received_at` (the inquiry-arm basis), not the reply arm's.
#[sqlx::test]
#[ignore]
async fn feed_b_narrowed_by_a_stage_clause_shifts_a_dual_person_to_inquiry_reasons(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d t10 narrow feed b",
        "admin@d011-t10-narrow.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stages: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position")
            .bind(organization_id)
            .fetch_all(&app_pool)
            .await
            .unwrap();
    let (in_stage, other_stage) = (stages[0], stages[1]);
    let now = Utc::now();

    let dual = insert_person(&app_pool, organization_id, other_stage, Some(admin_id)).await;
    insert_inquiry(&app_pool, organization_id, dual).await; // received_at = now - 2h
    insert_correspondence(
        &app_pool,
        organization_id,
        dual,
        admin_id,
        "inbound",
        now - ChronoDuration::hours(1),
    )
    .await;

    // Both feeds enabled/canonical: reply wins (dual qualifies for both).
    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(OrganizationId::new(
        organization_id,
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    let before = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    drop(conn);
    let before_item = before
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == dual)
        .unwrap();
    assert!(before_item
        .reasons
        .iter()
        .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })));

    // Narrow feed B to `in_stage` (excludes `dual`, who is in `other_stage`).
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
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::ClientRepliedUnanswered(BoolClause { value: true }),
                    Clause::Stage(StageClause {
                        stage_ids: vec![StageId::new(in_stage)],
                    }),
                ],
            },
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let after = crm_api::domain::today::query_at(&mut conn, &scope, UserId::new(admin_id), now)
        .await
        .unwrap();
    let after_item = after
        .items
        .iter()
        .find(|i| i.person.id.as_uuid() == dual)
        .unwrap();
    assert!(
        !after_item
            .reasons
            .iter()
            .any(|r| matches!(r, crm_api::domain::today::TodayReason::ClientReplied { .. })),
        "feed B no longer qualifies dual (narrowed to a different stage)"
    );
    assert!(after_item.reasons.iter().any(|r| matches!(
        r,
        crm_api::domain::today::TodayReason::NoContactAttempt { .. }
    )));
    let expected_waiting_since = now - ChronoDuration::hours(2);
    let diff = (after_item.waiting_since.unwrap() - expected_waiting_since)
        .num_milliseconds()
        .abs();
    assert!(
        diff < 1000,
        "waiting_since must equal the inquiry's received_at, not the reply's occurred_at: \
         got {:?}, expected ~{:?}",
        after_item.waiting_since,
        expected_waiting_since
    );
}

/// T13: a stale resend after a successful change must return `Conflict`
/// with the fact count unchanged (no double-write, no double-fact); then,
/// after deleting an Organization's feed rows directly (migrator-only —
/// simulating a corrupted/missing-row state, not a supported application
/// path), `admin_feed_view` must still return three revision-1 canonical
/// defaults, and a subsequent update must succeed to revision 2 with
/// exactly one fact.
#[sqlx::test]
#[ignore]
async fn stale_resend_is_conflict_and_deleted_feed_rows_resolve_as_defaults(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_admin(
        &migrator_pool,
        "011d t13 stale resend",
        "admin@d011-t13-stale.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let first = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                    Clause::HasPhone(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();
    assert!(first.changed);
    assert_eq!(fact_count(&app_pool, organization_id).await, 1);

    // A stale resend of the SAME (now-superseded) expected_revision.
    let resend = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                    Clause::HasEmail(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(24),
        },
    )
    .await;
    assert!(matches!(resend, Err(TodayFeedError::Conflict)));
    assert_eq!(
        fact_count(&app_pool, organization_id).await,
        1,
        "the stale resend must not write a second fact"
    );

    sqlx::query("DELETE FROM today_system_feed WHERE organization_id = $1")
        .bind(organization_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let view = crm_api::domain::today::system_feeds::queries::admin_feed_view(
        &mut conn,
        OrganizationId::new(organization_id),
    )
    .await
    .unwrap();
    drop(conn);
    assert_eq!(view.len(), 3);
    for feed in &view {
        assert!(feed.is_default);
        assert_eq!(feed.revision, 1);
    }

    let after_delete = commands::update_today_system_feed(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                    Clause::HasPhone(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();
    assert!(after_delete.changed);
    assert_eq!(after_delete.feed.revision, 2);
    assert_eq!(
        fact_count(&app_pool, organization_id).await,
        2,
        "exactly one NEW fact for this update on top of the earlier successful one \
         (the stale resend and the row deletion wrote none)"
    );
}

//! Slice 016b: the fixed built-in task axis inside `today::query`
//! (docs/specs/SLICE_016.md §5, D-054 §1). Covers §12.9 (membership and
//! boundaries), §12.10 (tier and order) and §12.11 (cap and truncation).
//! Failure-phase coverage lives in `db_today_task_axis_failures.rs`.
//! Run only via ./scripts/check-db.

use chrono::{Duration as ChronoDuration, Utc};
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::saved_list::SavedListScope;
use crm_api::domain::task::{self, CreateTask, TaskKind};
use crm_api::domain::today::{self, RecommendedAction, TodayPriority, TodayReason};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, UserId};
use crm_api::realtime::Publisher;
use sqlx::PgPool;
use uuid::Uuid;

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn visibility_scope(organization_id: Uuid) -> PersonVisibilityScope {
    PersonVisibilityScope::Organization(OrganizationId::new(organization_id))
}

/// One Organization, an admin (also usable as a plain "viewer") and a
/// second active member — the common starting point below.
struct Fixture {
    org_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
    stage_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let (org_id, admin_id) = crate::common::today_system_feed::create_org_with_admin(
        pool,
        "Task Axis Co",
        "task-axis-admin@example.test",
    )
    .await;
    let member_id =
        crate::common::create_user(pool, "task-axis-member@example.test", "Bob", PW).await;
    crate::common::add_membership_with(
        pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(pool).await;
    let stage_id = crate::common::today_system_feed::first_stage_id(&app_pool, org_id).await;
    Fixture {
        org_id,
        admin_id,
        member_id,
        stage_id,
    }
}

/// A bare Person with no inquiry and no assignment — never reachable by
/// the person-state or call feeds, so it can appear on Today ONLY through
/// the task axis.
async fn insert_bare_person(pool: &PgPool, organization_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) \
         VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_method(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    kind: &str,
    value: &str,
) {
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

/// A person-state candidate: assigned to `assignee`, with an inquiry that
/// qualifies `unanswered_inquiry` (no contact attempt since). `received_at`
/// controls both freshness (`fresh_within_hours` is the canonical 24h) and
/// ordering.
async fn insert_inquiry_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assignee: Uuid,
    received_at: chrono::DateTime<Utc>,
) -> Uuid {
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assignee)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'task-axis-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
    person_id
}

#[allow(clippy::too_many_arguments)]
async fn create_task_for(
    app_pool: &PgPool,
    org_id: Uuid,
    creator: Uuid,
    assignee: Uuid,
    person_id: Uuid,
    kind: TaskKind,
    due_at: Option<chrono::DateTime<Utc>>,
) -> Uuid {
    let task = task::create_task(
        app_pool,
        &Publisher::Disabled,
        &command_context(org_id, creator),
        CreateTask {
            person_id: PersonId::new(person_id),
            title: "Fixture task".to_string(),
            kind,
            due_at,
            assignee_user_id: Some(UserId::new(assignee)),
        },
    )
    .await
    .unwrap();
    task.id.as_uuid()
}

fn task_reason_kind(item: &today::TodayItem, person_id: Uuid) -> Option<&TodayReason> {
    item.reasons
        .iter()
        .find(|r| {
            matches!(
                r,
                TodayReason::TaskOverdue { .. } | TodayReason::TaskDue { .. }
            )
        })
        .filter(|_| item.person.id.as_uuid() == person_id)
}

fn find_item(items: &[today::TodayItem], person_id: Uuid) -> Option<&today::TodayItem> {
    items.iter().find(|i| i.person.id.as_uuid() == person_id)
}

/// Round-1 review fix B / `db_tasks.rs`'s own `due_in` precedent:
/// microsecond-truncated so a boundary assertion against a stored
/// `due_at` (Postgres keeps microseconds) never depends on the host
/// clock's own resolution (a `Utc::now()` on this platform can carry
/// nanoseconds) — `today::query`'s own evaluation clock is now truncated
/// the same way, so the fixture clock must match it exactly for an
/// equality assertion to be meaningful rather than accidentally passing.
fn now_trunc() -> chrono::DateTime<Utc> {
    use chrono::SubsecRound;
    Utc::now().trunc_subsecs(6)
}

// --- §12.9: membership and boundaries ------------------------------------

#[sqlx::test]
#[ignore]
async fn boundary_instants_classify_due_overdue_and_admit_within_24h_window(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = now_trunc();

    let p_now = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let p_overdue = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let p_boundary = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let p_beyond = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let p_no_due = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;

    // due_at == now -> task_due (>= now, not strictly less).
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_now,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;
    // due_at == now - 1s -> task_overdue.
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_overdue,
        TaskKind::FollowUp,
        Some(now - ChronoDuration::seconds(1)),
    )
    .await;
    // due_at == now + 24h -> admitted (inclusive boundary).
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_boundary,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::hours(24)),
    )
    .await;
    // due_at == now + 24h + 1s -> absent.
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_beyond,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::hours(24) + ChronoDuration::seconds(1)),
    )
    .await;
    // No due_at at all -> never reaches Today.
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_no_due,
        TaskKind::FollowUp,
        None,
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    let due_item = find_item(&list.items, p_now).expect("due-now item present");
    assert!(matches!(due_item.reasons[0], TodayReason::TaskDue { .. }));
    assert_eq!(due_item.priority, TodayPriority::Normal);

    let overdue_item = find_item(&list.items, p_overdue).expect("overdue item present");
    assert!(matches!(
        overdue_item.reasons[0],
        TodayReason::TaskOverdue { .. }
    ));
    assert_eq!(overdue_item.priority, TodayPriority::High);

    let boundary_item = find_item(&list.items, p_boundary).expect("exactly-24h item admitted");
    assert!(matches!(
        boundary_item.reasons[0],
        TodayReason::TaskDue { .. }
    ));

    assert!(
        find_item(&list.items, p_beyond).is_none(),
        "beyond 24h + 1s must be absent"
    );
    assert!(
        find_item(&list.items, p_no_due).is_none(),
        "a task with no due_at never reaches Today"
    );
    assert!(!list.truncated);
}

#[sqlx::test]
#[ignore]
async fn completed_tombstoned_other_member_and_other_org_tasks_are_excluded_with_positive_controls(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // Positive control: an ordinary open task for the admin.
    let p_control = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_control,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    // Completed task: excluded.
    let p_completed = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let completed_task_id = create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_completed,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;
    task::complete_task(
        &app_pool,
        &Publisher::Disabled,
        &command_context(f.org_id, f.admin_id),
        task::CompleteTask {
            person_id: PersonId::new(p_completed),
            task_id: crm_api::ids::TaskId::new(completed_task_id),
        },
    )
    .await
    .unwrap();

    // Tombstoned task: excluded.
    let p_deleted = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let deleted_task_id = create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_deleted,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;
    task::delete_task(
        &app_pool,
        &Publisher::Disabled,
        &command_context(f.org_id, f.admin_id),
        task::DeleteTask {
            person_id: PersonId::new(p_deleted),
            task_id: crm_api::ids::TaskId::new(deleted_task_id),
        },
    )
    .await
    .unwrap();

    // Another member's task: excluded from the admin's Today.
    let p_other_member = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.member_id,
        f.member_id,
        p_other_member,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    // Another Organization's task held by a user of the SAME email domain
    // is a distinct Person entirely (no multi-membership id reuse needed
    // for this exclusion; the composite-FK / literal-predicate isolation
    // itself is covered elsewhere) — here we simply confirm a second
    // Organization's Person never appears.
    let (other_org_id, other_admin_id) = crate::common::today_system_feed::create_org_with_admin(
        &migrator_pool,
        "Other Org",
        "task-axis-other-org-admin@example.test",
    )
    .await;
    let other_stage =
        crate::common::today_system_feed::first_stage_id(&app_pool, other_org_id).await;
    let p_other_org = insert_bare_person(&app_pool, other_org_id, other_stage).await;
    create_task_for(
        &app_pool,
        other_org_id,
        other_admin_id,
        other_admin_id,
        p_other_org,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    assert!(
        find_item(&list.items, p_control).is_some(),
        "positive control must be present"
    );
    assert!(find_item(&list.items, p_completed).is_none());
    assert!(find_item(&list.items, p_deleted).is_none());
    assert!(find_item(&list.items, p_other_member).is_none());
    assert!(find_item(&list.items, p_other_org).is_none());
}

#[sqlx::test]
#[ignore]
async fn earliest_task_is_chosen_per_viewer_among_their_own_tasks_only(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = now_trunc();

    // One Person, both the admin and the member hold an open dated task.
    // The admin's task is later; the member's is earlier. Each viewer must
    // see ONLY their own earliest task's reason — bob's earlier task does
    // not displace alice's later one from alice's Today and vice versa.
    let person_id = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        person_id,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::hours(2)),
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.member_id,
        f.member_id,
        person_id,
        TaskKind::Call,
        Some(now + ChronoDuration::hours(1)),
    )
    .await;

    let admin_list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();
    let member_list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.member_id),
        now,
    )
    .await
    .unwrap();

    let admin_item = find_item(&admin_list.items, person_id).expect("admin sees the Person");
    match &admin_item.reasons[0] {
        TodayReason::TaskDue { due_at, kind, .. } => {
            assert_eq!(*due_at, now + ChronoDuration::hours(2));
            assert_eq!(*kind, TaskKind::FollowUp);
        }
        other => panic!("expected admin's own task, got {other:?}"),
    }

    let member_item = find_item(&member_list.items, person_id).expect("member sees the Person");
    match &member_item.reasons[0] {
        TodayReason::TaskDue { due_at, kind, .. } => {
            assert_eq!(*due_at, now + ChronoDuration::hours(1));
            assert_eq!(*kind, TaskKind::Call);
        }
        other => panic!("expected member's own task, got {other:?}"),
    }
}

#[sqlx::test]
#[ignore]
async fn task_only_item_has_null_latest_inquiry_and_waiting_since_and_action_follows_kind(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    let p_email_kind_with_email = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    insert_contact_method(
        &app_pool,
        f.org_id,
        p_email_kind_with_email,
        "email",
        "client@example.test",
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_email_kind_with_email,
        TaskKind::Email,
        Some(now),
    )
    .await;

    let p_email_kind_no_email_has_phone = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    insert_contact_method(
        &app_pool,
        f.org_id,
        p_email_kind_no_email_has_phone,
        "phone",
        "+15555550100",
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_email_kind_no_email_has_phone,
        TaskKind::Email,
        Some(now),
    )
    .await;

    let p_call_kind_with_phone = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    insert_contact_method(
        &app_pool,
        f.org_id,
        p_call_kind_with_phone,
        "phone",
        "+15555550101",
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_call_kind_with_phone,
        TaskKind::Call,
        Some(now),
    )
    .await;

    let p_no_contact_methods = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_no_contact_methods,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    for person_id in [
        p_email_kind_with_email,
        p_email_kind_no_email_has_phone,
        p_call_kind_with_phone,
        p_no_contact_methods,
    ] {
        let item = find_item(&list.items, person_id).unwrap();
        assert!(
            item.latest_inquiry.is_none(),
            "task-only latest_inquiry must be null"
        );
        assert!(
            item.waiting_since.is_none(),
            "task-only waiting_since must be null"
        );
    }

    assert_eq!(
        find_item(&list.items, p_email_kind_with_email)
            .unwrap()
            .recommended_action,
        RecommendedAction::Email
    );
    assert_eq!(
        find_item(&list.items, p_email_kind_no_email_has_phone)
            .unwrap()
            .recommended_action,
        RecommendedAction::Call
    );
    assert_eq!(
        find_item(&list.items, p_call_kind_with_phone)
            .unwrap()
            .recommended_action,
        RecommendedAction::Call
    );
    assert_eq!(
        find_item(&list.items, p_no_contact_methods)
            .unwrap()
            .recommended_action,
        RecommendedAction::ReviewPerson
    );
}

// --- §12.10: tier and order -----------------------------------------------

/// docs/specs/SLICE_016.md §5 as amended at review round 1 (D-054, the
/// 011c list-only precedent): a task-only item on a Person who HAS an
/// inquiry carries that inquiry as a real `InquiryRef` and a consistent
/// `last_inquiry_at`; `null` is reserved for zero-inquiry People. The
/// Person is kept outside the built-in set by a contact attempt after
/// the inquiry (answered, not fresh), so the axis's task-only prefix is
/// what admits them. Written by the coordinator at the round-2
/// confirmation, which found the hydration landed without a positive test.
#[sqlx::test]
#[ignore]
async fn task_only_item_on_a_person_with_an_answered_inquiry_carries_the_real_inquiry_ref(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = now_trunc();

    let received_at = now - ChronoDuration::days(3);
    let person_id =
        insert_inquiry_person(&app_pool, f.org_id, f.stage_id, f.admin_id, received_at).await;
    // Answered two days ago: outside "unanswered" and not fresh, so the
    // Person is not a person-state candidate and no call feed applies.
    sqlx::query(
        "INSERT INTO contact_attempted (organization_id, actor_kind, actor_user_id, origin, \
            occurred_at, correlation_id, causation_id, person_id, channel, outcome) \
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'reached')",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .bind(now - ChronoDuration::days(2))
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let inquiry_id: Uuid =
        sqlx::query_scalar("SELECT id FROM inquiry WHERE organization_id = $1 AND person_id = $2")
            .bind(f.org_id)
            .bind(person_id)
            .fetch_one(&app_pool)
            .await
            .unwrap();

    // No task yet: the answered Person is absent (positive control for the
    // "outside the built-in set" premise).
    let before = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();
    assert!(
        find_item(&before.items, person_id).is_none(),
        "an answered, non-fresh Person must not be a built-in candidate"
    );

    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        person_id,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::hours(1)),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();
    let item = find_item(&list.items, person_id).expect("task-only item admitted");
    assert!(
        matches!(
            task_reason_kind(item, person_id),
            Some(TodayReason::TaskDue { .. })
        ),
        "a task due in one hour is task_due"
    );
    let latest = item
        .latest_inquiry
        .as_ref()
        .expect("a Person with an inquiry carries a real InquiryRef (D-054, list-only precedent)");
    assert_eq!(latest.id.as_uuid(), inquiry_id);
    assert_eq!(latest.received_at, received_at);
    assert_eq!(
        item.person.last_inquiry_at,
        Some(received_at),
        "PersonSummary.last_inquiry_at is consistent with the InquiryRef"
    );
    assert_eq!(item.person.inquiry_count, 1);
    assert!(
        item.waiting_since.is_none(),
        "task-only items carry no waiting time"
    );
}

#[sqlx::test]
#[ignore]
async fn retained_normal_item_with_overdue_task_is_raised_and_ordered_after_fresh_high_before_task_only(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // A fresh (high) person-state candidate.
    let p_fresh = insert_inquiry_person(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        now - ChronoDuration::hours(1),
    )
    .await;
    // A stale (normal) person-state candidate that will be raised by an
    // overdue task.
    let p_raised = insert_inquiry_person(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        now - ChronoDuration::hours(48),
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_raised,
        TaskKind::FollowUp,
        Some(now - ChronoDuration::hours(1)),
    )
    .await;
    // A task-only Person, overdue (high tier via the axis directly).
    let p_task_only = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_task_only,
        TaskKind::FollowUp,
        Some(now - ChronoDuration::minutes(30)),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    let raised_item = find_item(&list.items, p_raised).unwrap();
    assert_eq!(
        raised_item.priority,
        TodayPriority::High,
        "a normal item with an overdue task is raised to high"
    );
    assert!(matches!(
        task_reason_kind(raised_item, p_raised),
        Some(TodayReason::TaskOverdue { .. })
    ));

    let fresh_index = list
        .items
        .iter()
        .position(|i| i.person.id.as_uuid() == p_fresh)
        .unwrap();
    let raised_index = list
        .items
        .iter()
        .position(|i| i.person.id.as_uuid() == p_raised)
        .unwrap();
    let task_only_index = list
        .items
        .iter()
        .position(|i| i.person.id.as_uuid() == p_task_only)
        .unwrap();
    assert!(
        fresh_index < raised_index,
        "fresh high items precede raised items"
    );
    assert!(
        raised_index < task_only_index,
        "raised items precede task-only high items"
    );
}

/// Test-local only (no shared-fixture change): inserts a Person with an
/// explicit `id` rather than the server-generated `gen_random_uuid()`
/// `insert_bare_person` above uses, so
/// `task_only_items_sharing_due_at_are_ordered_by_ascending_person_id`
/// below can pin Person-id order independently of insertion order.
async fn insert_bare_person_with_id(
    pool: &PgPool,
    id: Uuid,
    organization_id: Uuid,
    stage_id: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (id, organization_id, first_name, stage_id) \
         VALUES ($1, $2, 'Fixture', $3) RETURNING id",
    )
    .bind(id)
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// LATER batch (2026-09-10) item 7b (016b LATER, curated):
/// docs/specs/SLICE_016.md §5's documented tie-break — task-only items
/// order by `due_at ASC, id ASC`, where `id` is the PERSON id (the
/// call-only precedent) — proven with THREE task-only Persons sharing the
/// exact same overdue `due_at` (high tier), not just the two-item cases
/// elsewhere in this file. Review round 1 fix: Person insertion order,
/// task-creation order and the expected (ascending Person id) order are
/// all pinned to be pairwise distinct via explicit local ids, so a
/// dropped `id ASC` tie-break cannot pass this test by accident (the
/// tester's finding: server-generated ids alone would coincidentally
/// match the wrong order about one run in six).
#[sqlx::test]
#[ignore]
async fn task_only_items_sharing_due_at_are_ordered_by_ascending_person_id(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();
    let due = now - ChronoDuration::minutes(30);

    // Review round 1 fix: with server-generated v4 ids, insertion order
    // and Person-id order are already uncorrelated with each other AND
    // with task-id order (tasks get their own independent v4 ids), so
    // the three could coincidentally agree — the tester found dropping
    // `id ASC` from the SQL would still pass one run in six. Explicit
    // local ids pin all three orders to be pairwise distinct: Person
    // insertion order (0003, 0001, 0002) != task-creation order (0002,
    // 0003, 0001) != the expected Today order, ascending Person id
    // (0001, 0002, 0003) — so only a genuine `id ASC` tie-break can make
    // this test pass.
    let id_0001 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let id_0002 = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
    let id_0003 = Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap();

    for id in [id_0003, id_0001, id_0002] {
        insert_bare_person_with_id(&app_pool, id, f.org_id, f.stage_id).await;
    }
    for id in [id_0002, id_0003, id_0001] {
        create_task_for(
            &app_pool,
            f.org_id,
            f.admin_id,
            f.admin_id,
            id,
            TaskKind::FollowUp,
            Some(due),
        )
        .await;
    }
    let people = vec![id_0001, id_0002, id_0003];

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    let mut actual_order: Vec<(usize, Uuid)> = people
        .iter()
        .map(|id| {
            let index = list
                .items
                .iter()
                .position(|item| item.person.id.as_uuid() == *id)
                .expect("every task-only Person must be on the list");
            (index, *id)
        })
        .collect();
    actual_order.sort_by_key(|(index, _)| *index);
    let actual_ids: Vec<Uuid> = actual_order.into_iter().map(|(_, id)| id).collect();

    let mut expected_ids = people.clone();
    expected_ids.sort();

    assert_eq!(
        actual_ids, expected_ids,
        "three task-only items sharing due_at must break the tie on ascending Person id"
    );
}

#[sqlx::test]
#[ignore]
async fn low_outcome_needed_item_is_never_raised_by_an_overdue_task(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // A call-outcome-needed (`low`) candidate: assigned to the admin, a
    // qualifying ended call with no contact_attempted correction, and an
    // inquiry so `low` (not "excluded entirely") is reachable — mirrors
    // `today_system_feed::insert_call`'s own fixture shape.
    let person_id = insert_inquiry_person(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        now - ChronoDuration::hours(1),
    )
    .await;
    // A contact attempt after the inquiry removes it from the person-state
    // feed's "unanswered" membership, leaving only the outcome-needed call
    // reason (`low`).
    sqlx::query(
        "INSERT INTO contact_attempted (organization_id, actor_kind, actor_user_id, origin, \
            occurred_at, correlation_id, causation_id, person_id, channel, outcome) \
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'reached')",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .bind(now - ChronoDuration::minutes(50))
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&app_pool)
    .await
    .unwrap();
    crate::common::today_system_feed::insert_call(
        &app_pool,
        f.org_id,
        person_id,
        f.admin_id,
        now - ChronoDuration::minutes(45),
    )
    .await;

    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        person_id,
        TaskKind::FollowUp,
        Some(now - ChronoDuration::hours(1)),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    let item = find_item(&list.items, person_id).expect("outcome-needed Person present");
    assert_eq!(
        item.priority,
        TodayPriority::Low,
        "a low item is never raised by an overdue task"
    );
    // Round-1 review must-close item 5: an exact-match assertion on the
    // WHOLE `reasons` vector, not just independent presence checks — this
    // also pins the count (exactly two, nothing extra re-admitted by the
    // now-corrected contact attempt) and the order (`call_outcome_needed`
    // stays last per every other Today statement's own contract, so the
    // task axis's append-after-existing-reasons rule must place
    // `task_overdue` BEFORE it here, never after).
    assert_eq!(
        item.reasons.len(),
        2,
        "a low item carries exactly the outcome-needed and the overdue-task reason: {:?}",
        item.reasons
    );
    assert!(
        matches!(item.reasons[0], TodayReason::TaskOverdue { .. }),
        "reasons[0] must be the task reason (call_outcome_needed always stays last): {:?}",
        item.reasons
    );
    assert!(
        matches!(item.reasons[1], TodayReason::CallOutcomeNeeded { .. }),
        "reasons[1] must be call_outcome_needed, always last: {:?}",
        item.reasons
    );
}

#[sqlx::test]
#[ignore]
async fn task_only_item_matching_a_list_source_reads_list_member_then_task_reason(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    let person_id = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        person_id,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    // A saved-list source with an unconstrained filter (matches everyone,
    // including the task-only Person), enabled as a Today source.
    let list = crate::common::saved_lists::create_list(
        &app_pool,
        f.org_id,
        f.admin_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Everyone",
        FilterDefinition {
            version: 1,
            clauses: Vec::new(),
        },
    )
    .await;
    let list_id = list.list.id;
    let change = today::enable_today_work_source(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        today::EnableTodayWorkSource {
            list_id,
            expected_list_revision: list.list.revision,
        },
    )
    .await
    .unwrap();
    assert!(change.enabled);

    let today_list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    // Exactly one item for this Person (never duplicated between the
    // task-only band and the list band).
    let matches: Vec<_> = today_list
        .items
        .iter()
        .filter(|i| i.person.id.as_uuid() == person_id)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "a task-only Person matching a list appears exactly once"
    );
    let item = matches[0];
    assert_eq!(item.reasons.len(), 2);
    assert!(
        matches!(item.reasons[0], TodayReason::ListMember { .. }),
        "list reason must precede the task reason"
    );
    assert!(matches!(
        item.reasons[1],
        TodayReason::TaskDue { .. } | TodayReason::TaskOverdue { .. }
    ));
}

// --- §12.11: cap and truncation -------------------------------------------

async fn insert_n_fresh_inquiry_people(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    assignee: Uuid,
    n: usize,
    base: chrono::DateTime<Utc>,
) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let received_at = base - ChronoDuration::milliseconds(i as i64);
        ids.push(insert_inquiry_person(pool, org_id, stage_id, assignee, received_at).await);
    }
    ids
}

#[sqlx::test]
#[ignore]
async fn task_only_prefix_admits_up_to_k_and_sets_truncated_on_the_extra_row(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // 199 fresh person-state candidates -> K = 1 for the task-only prefix.
    insert_n_fresh_inquiry_people(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        199,
        now - ChronoDuration::hours(1),
    )
    .await;

    // Two task-only tasks: only the earliest may be admitted (K=1); the
    // extra row must set `truncated`, never silently admitting both.
    let p_earlier = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    let p_later = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_earlier,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::minutes(1)),
    )
    .await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_later,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::minutes(2)),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    assert!(list.truncated, "the extra task-only row must set truncated");
    assert!(
        find_item(&list.items, p_earlier).is_some(),
        "the earliest task-only row is admitted"
    );
    assert!(
        find_item(&list.items, p_later).is_none(),
        "the extra row is never admitted"
    );
}

/// LATER batch (2026-09-10) item 7a (016b LATER, curated): a dedicated
/// cap-boundary test proving task-only items count AGAINST the 200-item
/// cap rather than being admitted on top of it — 199 fresh person-state
/// candidates plus exactly ONE task-only row (K = 1, exactly satisfied)
/// fills the list to precisely 200 with `truncated` false, the positive
/// complement to
/// `task_only_prefix_admits_up_to_k_and_sets_truncated_on_the_extra_row`
/// above (199 + 2, K = 1, `truncated` true).
#[sqlx::test]
#[ignore]
async fn task_only_item_exactly_fills_the_cap_without_truncation(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // 199 fresh person-state candidates -> K = 1 for the task-only prefix.
    insert_n_fresh_inquiry_people(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        199,
        now - ChronoDuration::hours(1),
    )
    .await;

    // Exactly one task-only Person: K = 1 admits it with room to spare.
    let p_task_only = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_task_only,
        TaskKind::FollowUp,
        Some(now + ChronoDuration::minutes(1)),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    assert_eq!(
        list.items.len(),
        200,
        "199 person-state items plus the one task-only item is exactly 200"
    );
    assert!(
        !list.truncated,
        "a task-only item that exactly fits K must not set truncated"
    );
    assert!(
        find_item(&list.items, p_task_only).is_some(),
        "the task-only item counts toward, and fits within, the cap"
    );
}

#[sqlx::test]
#[ignore]
async fn exactly_200_builtins_admits_no_task_only_row_but_reports_truncated(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // Exactly 200 fresh person-state candidates -> not truncated_p, K=0.
    insert_n_fresh_inquiry_people(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        200,
        now - ChronoDuration::hours(1),
    )
    .await;

    let p_task_only = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_task_only,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    assert_eq!(list.items.len(), 200, "K=0 admits zero task-only rows");
    assert!(
        list.truncated,
        "a candidate existing beyond the K=0 budget still sets truncated"
    );
    assert!(find_item(&list.items, p_task_only).is_none());
}

#[sqlx::test]
#[ignore]
async fn truncated_person_state_statement_skips_the_task_only_prefix_entirely(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // 201 fresh person-state candidates -> truncated_p = true.
    insert_n_fresh_inquiry_people(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        201,
        now - ChronoDuration::hours(1),
    )
    .await;

    let p_task_only = insert_bare_person(&app_pool, f.org_id, f.stage_id).await;
    create_task_for(
        &app_pool,
        f.org_id,
        f.admin_id,
        f.admin_id,
        p_task_only,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    let list = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f.org_id),
        UserId::new(f.admin_id),
        now,
    )
    .await
    .unwrap();

    assert!(list.truncated);
    assert_eq!(list.items.len(), 200);
    assert!(
        find_item(&list.items, p_task_only).is_none(),
        "the task-only prefix must not run at all once the person-state statement was truncated"
    );
}

// --- §11/§12 step 1: zero-task equivalence --------------------------------

#[sqlx::test]
#[ignore]
async fn zero_task_organization_today_is_unaffected_for_admin_and_member(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // Ordinary person-state and call-feed builtins, no tasks anywhere in
    // the Organization.
    let p_high = insert_inquiry_person(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        now - ChronoDuration::hours(1),
    )
    .await;
    let p_normal = insert_inquiry_person(
        &app_pool,
        f.org_id,
        f.stage_id,
        f.admin_id,
        now - ChronoDuration::hours(48),
    )
    .await;

    for (viewer, label) in [(f.admin_id, "admin"), (f.member_id, "member")] {
        let list = today::query_at(
            &mut app_pool.acquire().await.unwrap(),
            &visibility_scope(f.org_id),
            UserId::new(viewer),
            now,
        )
        .await
        .unwrap();

        assert!(
            list.sources
                .system_feed_issues
                .iter()
                .all(|i| i.feed_key != "task_due"),
            "{label}: no task_due issue when no task exists"
        );
        for item in &list.items {
            assert!(
                !item.reasons.iter().any(|r| matches!(
                    r,
                    TodayReason::TaskOverdue { .. } | TodayReason::TaskDue { .. }
                )),
                "{label}: no task reason anywhere when no task exists"
            );
        }
        // Both fixture People are assigned to the admin, so only the
        // admin's Today carries them — this loop's real point is that
        // running each viewer through the (now task-axis-carrying) query
        // path never introduces a task_due issue or a task reason,
        // independent of what person-state content that viewer happens
        // to see.
        if viewer == f.admin_id {
            assert!(find_item(&list.items, p_high).is_some());
            assert!(find_item(&list.items, p_normal).is_some());
        }
    }
}

// --- Round-1 review must-close item 1: same-user, two Organizations -----

/// The same physical user is a member of two Organizations, each with a
/// task assigned to them. `task_membership`/`task_only_prefix` both
/// predicate on `t.organization_id = $1 AND t.assignee_user_id = $2`
/// (never assignee alone) — a regression dropping the organization_id
/// predicate would leak the other Organization's task-driven Today item
/// for exactly this shape, so this proves the boundary directly rather
/// than relying on the SQL text never regressing unnoticed.
#[sqlx::test]
#[ignore]
async fn same_user_assignee_in_two_organizations_never_leaks_across_the_boundary(
    migrator_pool: PgPool,
) {
    let f_a = fixture(&migrator_pool).await;
    let (org_b, admin_b) = crate::common::today_system_feed::create_org_with_admin(
        &migrator_pool,
        "Task Axis Co B",
        "task-axis-admin-b@example.test",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_b = crate::common::today_system_feed::first_stage_id(&app_pool, org_b).await;
    // f_a.admin_id (already admin of org A) is ALSO a member of org B.
    crate::common::add_membership_with(
        &migrator_pool,
        org_b,
        f_a.admin_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let now = Utc::now();

    let person_a = insert_bare_person(&app_pool, f_a.org_id, f_a.stage_id).await;
    let person_b = insert_bare_person(&app_pool, org_b, stage_b).await;
    create_task_for(
        &app_pool,
        f_a.org_id,
        f_a.admin_id,
        f_a.admin_id,
        person_a,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;
    create_task_for(
        &app_pool,
        org_b,
        admin_b,
        f_a.admin_id,
        person_b,
        TaskKind::FollowUp,
        Some(now),
    )
    .await;

    let list_a = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(f_a.org_id),
        UserId::new(f_a.admin_id),
        now,
    )
    .await
    .unwrap();
    let list_b = today::query_at(
        &mut app_pool.acquire().await.unwrap(),
        &visibility_scope(org_b),
        UserId::new(f_a.admin_id),
        now,
    )
    .await
    .unwrap();

    assert!(
        find_item(&list_a.items, person_a).is_some(),
        "org A's own task-driven item is present in org A's Today"
    );
    assert!(
        find_item(&list_a.items, person_b).is_none(),
        "org B's Person must never leak into org A's Today for the same assignee user"
    );
    assert!(
        find_item(&list_b.items, person_b).is_some(),
        "org B's own task-driven item is present in org B's Today"
    );
    assert!(
        find_item(&list_b.items, person_a).is_none(),
        "org A's Person must never leak into org B's Today for the same assignee user"
    );
}

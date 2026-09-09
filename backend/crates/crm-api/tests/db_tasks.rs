//! DB-backed task coverage (Slice 016a; docs/specs/SLICE_016.md
//! §12.3–12.5). Command-level tests exercise the production
//! `domain::task::*` path directly for authorization and concurrency
//! (mirroring `db_notes.rs`/`db_tags.rs`); HTTP-level tests cover the
//! route surface, detail shape, and error precedence. Run only via
//! ./scripts/check-db.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::task::{
    self, CompleteTask, CreateTask, DeleteTask, ReopenTask, SnoozeTask, TaskError, TaskKind,
    UpdateTask,
};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, TaskId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

async fn recorded(publisher: &Publisher) -> Vec<(String, serde_json::Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected recording publisher");
    };
    recorded.lock().await.clone()
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_bare_person(pool: &PgPool, organization_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn task_row_count(pool: &PgPool, organization_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM task WHERE organization_id = $1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn promote_to_admin(pool: &PgPool, organization_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin'
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

struct Fixture {
    org_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
    person_id: Uuid,
}

async fn fixture(migrator_pool: &PgPool) -> Fixture {
    let (org_id, admin_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice-tasks@acme.test",
        "Alice",
        PW,
    )
    .await;
    promote_to_admin(migrator_pool, org_id, admin_id).await;
    let member_id =
        crate::common::create_user(migrator_pool, "bob-tasks@acme.test", "Bob", PW).await;
    crate::common::add_membership_with(
        migrator_pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, org_id).await;
    let person_id = insert_bare_person(&app_pool, org_id, stage_id).await;
    Fixture {
        org_id,
        admin_id,
        member_id,
        person_id,
    }
}

fn due_in(hours: i64) -> chrono::DateTime<Utc> {
    Utc::now() + Duration::hours(hours)
}

// --- §12.3: create -----------------------------------------------------

/// docs/specs/SLICE_016.md §4, §12.3: 201 shape with `can_manage: true`;
/// default assignee is the actor when omitted; an explicit active member
/// is accepted; the task appears in the detail's `tasks[]`.
#[sqlx::test]
#[ignore]
async fn create_task_shape_default_assignee_and_explicit_active_member(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Default assignee = actor.
    let ctx = command_context(f.org_id, f.member_id);
    let task = task::create_task(
        &app_pool,
        &publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "  Call the client  ".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due_in(24)),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(task.title, "Call the client");
    assert_eq!(task.kind, TaskKind::FollowUp);
    assert!(task.can_manage);
    assert_eq!(task.assignee.as_ref().unwrap().id, UserId::new(f.member_id));
    assert_eq!(
        task.created_by.as_ref().unwrap().id,
        UserId::new(f.member_id)
    );
    assert!(task.completed_at.is_none());

    // Explicit active member (admin) accepted.
    let task2 = task::create_task(
        &app_pool,
        &publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Send the disclosure packet".to_string(),
            kind: TaskKind::Email,
            due_at: Some(due_in(48)),
            assignee_user_id: Some(UserId::new(f.admin_id)),
        },
    )
    .await
    .unwrap();
    assert_eq!(task2.assignee.as_ref().unwrap().id, UserId::new(f.admin_id));
    // The creator (bob), not the assignee (alice), issued this command —
    // rule 1 still grants `can_manage` via `created_by_user_id`.
    assert!(task2.can_manage);

    // Detail `tasks[]`: due_at ASC order (task due in 24h before task2 due
    // in 48h).
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice-tasks@acme.test", PW).await;
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let tasks = detail["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["id"], task.id.to_string());
    assert_eq!(tasks[1]["id"], task2.id.to_string());
    assert_eq!(
        tasks[0]["can_manage"], true,
        "admin viewer can manage every task"
    );
}

/// docs/specs/SLICE_016.md §3, §12.3: an inactive member, a member of
/// another Organization, and a random uuid all give the byte-identical
/// 422 `invalid_assignee`, with no row landing.
#[sqlx::test]
#[ignore]
async fn create_task_invalid_assignee_variants_are_byte_identical_with_no_row(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let inactive_id =
        crate::common::create_user(&migrator_pool, "carol-inactive@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        inactive_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-invalid-assignee@best.test",
        "Erin",
        PW,
    )
    .await;
    let _ = other_org_id;

    let before = task_row_count(&migrator_pool, f.org_id).await;
    let ctx = command_context(f.org_id, f.member_id);

    let mut bodies = Vec::new();
    for assignee in [inactive_id, other_admin_id, Uuid::new_v4()] {
        let result = task::create_task(
            &app_pool,
            &publisher,
            &ctx,
            CreateTask {
                person_id: PersonId::new(f.person_id),
                title: "Invalid assignee attempt".to_string(),
                kind: TaskKind::default(),
                due_at: None,
                assignee_user_id: Some(UserId::new(assignee)),
            },
        )
        .await;
        assert!(matches!(result, Err(TaskError::InvalidAssignee)));
        bodies.push(format!("{result:?}"));
    }
    assert_eq!(
        task_row_count(&migrator_pool, f.org_id).await,
        before,
        "no row must land from any invalid-assignee attempt"
    );

    // Byte-identical over HTTP too.
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice-tasks@acme.test", PW).await;
    let events_before = recorded(&publisher).await.len();
    let mut http_bodies = Vec::new();
    for assignee in [inactive_id, other_admin_id, Uuid::new_v4()] {
        let resp = crate::common::post_json_with_cookie(
            &router,
            &format!("/api/people/{}/tasks", f.person_id),
            &alice,
            json!({ "title": "Invalid assignee", "assignee_user_id": assignee }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        http_bodies.push(crate::common::body_json(resp).await);
    }
    assert_eq!(http_bodies[0], http_bodies[1]);
    assert_eq!(http_bodies[1], http_bodies[2]);
    assert_eq!(http_bodies[0], json!({ "error": "invalid_assignee" }));
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish on any invalid-assignee rejection"
    );
}

/// docs/specs/SLICE_016.md §3, §9, §12.3: the `FOR SHARE` assignee check
/// genuinely waits on, and then observes, an in-flight deactivation on a
/// second connection — 422 only after that transaction commits, no row.
#[sqlx::test]
#[ignore]
async fn create_task_assignee_deactivated_in_flight_is_422_after_commit_with_no_row(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let target_id =
        crate::common::create_user(&migrator_pool, "dan-inflight@acme.test", "Dan", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        target_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let mut lock_tx = app_pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(target_id)
    .execute(&mut *lock_tx)
    .await
    .unwrap();

    let ctx = command_context(f.org_id, f.member_id);
    let create_fut = task::create_task(
        &app_pool,
        &publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Should never land".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(target_id)),
        },
    );
    let commit_fut = async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        lock_tx.commit().await.unwrap();
    };
    let (create_result, ()) = tokio::join!(create_fut, commit_fut);
    assert!(
        matches!(create_result, Err(TaskError::InvalidAssignee)),
        "must observe the deactivation once it commits: {create_result:?}"
    );
    assert_eq!(task_row_count(&migrator_pool, f.org_id).await, 0);
}

/// docs/specs/SLICE_016.md §12.3: a `tokio::join!` race between creating a
/// task for a member and deactivating that same member never yields 503 —
/// only ever `{201, 422}` in either order.
#[sqlx::test]
#[ignore]
async fn create_task_vs_deactivate_join_race_never_503(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let target_id =
        crate::common::create_user(&migrator_pool, "erin-race@acme.test", "Erin", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        target_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let ctx = command_context(f.org_id, f.member_id);
    let create_fut = task::create_task(
        &app_pool,
        &publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Race task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(target_id)),
        },
    );
    let deactivate_fut = sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(target_id)
    .execute(&app_pool);

    let (create_result, deactivate_result) = tokio::join!(create_fut, deactivate_fut);
    deactivate_result.unwrap();
    match create_result {
        Ok(_) | Err(TaskError::InvalidAssignee) => {}
        other => panic!("expected {{201, 422}}, never 503: {other:?}"),
    }
}

/// docs/specs/SLICE_016.md §9, §12.3: a real cross-Organization Person and
/// a random uuid are byte-identical 404s over HTTP, with no row landing
/// in either Organization and no publication.
#[sqlx::test]
#[ignore]
async fn create_task_foreign_or_nonexistent_person_is_byte_identical_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-create-fk@best.test",
        "Erin",
        PW,
    )
    .await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;
    let _ = other_admin_id;

    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice-tasks@acme.test", PW).await;

    let events_before = recorded(&publisher).await.len();
    let org_a_before = task_row_count(&migrator_pool, f.org_id).await;
    let org_b_before = task_row_count(&migrator_pool, other_org_id).await;

    let random_uuid_resp = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{}/tasks", Uuid::new_v4()),
        &alice,
        json!({ "title": "Nonexistent person" }),
    )
    .await;
    assert_eq!(random_uuid_resp.status(), StatusCode::NOT_FOUND);
    let random_uuid_body = crate::common::body_json(random_uuid_resp).await;

    let cross_org_resp = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{other_person_id}/tasks"),
        &alice,
        json!({ "title": "Nonexistent person" }),
    )
    .await;
    assert_eq!(cross_org_resp.status(), StatusCode::NOT_FOUND);
    let cross_org_body = crate::common::body_json(cross_org_resp).await;
    assert_eq!(
        random_uuid_body, cross_org_body,
        "a real cross-Organization Person id must be byte-identical to a random uuid 404"
    );
    assert_eq!(task_row_count(&migrator_pool, f.org_id).await, org_a_before);
    assert_eq!(
        task_row_count(&migrator_pool, other_org_id).await,
        org_b_before
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish on either 404"
    );
}

// --- §12.4: update / complete / reopen / snooze / delete ------------------

/// docs/specs/SLICE_016.md §3, §12.4: the assignee, the creator, and an
/// admin each get 200 on update; a third member is 403 with no write and
/// no publication.
#[sqlx::test]
#[ignore]
async fn update_task_assignee_creator_admin_succeed_third_member_forbidden(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-update@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    // Assignee = admin, creator = member (bob), so all three roles differ.
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Original".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due_in(24)),
            assignee_user_id: Some(UserId::new(f.admin_id)),
        },
    )
    .await
    .unwrap();

    // Third member: forbidden, no write.
    let forbidden = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Hijacked".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due_in(24)),
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await;
    assert!(matches!(forbidden, Err(TaskError::Forbidden)));
    let title: String = sqlx::query_scalar("SELECT title FROM task WHERE id = $1")
        .bind(task.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(title, "Original");

    // The assignee (admin) updates: 200.
    let assignee_update = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Assignee edit".to_string(),
            kind: TaskKind::Call,
            due_at: Some(due_in(24)),
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await
    .unwrap();
    assert!(assignee_update.changed);

    // The creator (bob) updates: 200.
    let creator_update = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Creator edit".to_string(),
            kind: TaskKind::Call,
            due_at: Some(due_in(24)),
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await
    .unwrap();
    assert!(creator_update.changed);

    // A genuine admin (not assignee, not creator) updates too: 200.
    let super_admin_id =
        crate::common::create_user(&migrator_pool, "sam-super-admin@acme.test", "Sam", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        super_admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let admin_update = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, super_admin_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Admin edit".to_string(),
            kind: TaskKind::Text,
            due_at: Some(due_in(24)),
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await
    .unwrap();
    assert!(admin_update.changed);
}

/// docs/specs/SLICE_016.md §3, §12.4: an equal PUT is `changed: false`
/// and writes nothing (positive control: a genuinely different PUT right
/// after is `changed: true` and does write).
#[sqlx::test]
#[ignore]
async fn update_task_changed_false_and_true_with_positive_control(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let due = due_in(24);
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Follow up".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    let updated_at_before: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let events_before = recorded(&publisher).await.len();

    let no_op = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: task.title.clone(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await
    .unwrap();
    assert!(!no_op.changed);
    let updated_at_after: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        updated_at_before, updated_at_after,
        "an equal PUT must write nothing"
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "an equal PUT must publish nothing"
    );

    // Positive control: a genuinely different PUT.
    let changed = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Follow up urgently".to_string(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await
    .unwrap();
    assert!(changed.changed);
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before + 1,
        "a genuinely different PUT must publish exactly one event"
    );
}

/// docs/specs/SLICE_016.md §3, §12.4: the assignee is re-validated as an
/// active member ONLY when it changes — a task held by a deactivated
/// member can still be retitled (unchanged assignee, random-looking value
/// notwithstanding), but changing the assignee to a random uuid is 422.
#[sqlx::test]
#[ignore]
async fn update_task_assignee_revalidated_only_when_it_changes(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Held by a soon-to-deactivate member".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(f.member_id)),
        },
    )
    .await
    .unwrap();

    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    // The admin retitles it, keeping the (now-inactive) assignee: allowed.
    let retitle = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Retitled while assignee is inactive".to_string(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await
    .unwrap();
    assert!(retitle.changed);

    // The admin now tries to REASSIGN to a random uuid: 422.
    let reassign_random = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Retitled while assignee is inactive".to_string(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(Uuid::new_v4()),
        },
    )
    .await;
    assert!(matches!(reassign_random, Err(TaskError::InvalidAssignee)));

    // And a third member's PUT with an UNCHANGED assignee is 403, not 422
    // (permission is decided before any assignee validation).
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-precedence@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let third_member = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Retitled while assignee is inactive".to_string(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(third_member, Err(TaskError::Forbidden)));
}

/// docs/specs/SLICE_016.md §1 rule 1, §12.4: an assignee who reassigns a
/// task away from themselves gets 200 with `can_manage: false` in that
/// very response, and 403 on their next PUT (the creator, if different,
/// keeps `can_manage`).
#[sqlx::test]
#[ignore]
async fn update_task_reassign_away_loses_can_manage_and_next_put_is_forbidden(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // bob creates a task assigned to himself (creator == assignee).
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Self-assigned".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    assert!(task.can_manage);

    // bob reassigns it to the admin.
    let reassigned = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: task.title.clone(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await
    .unwrap();
    assert!(reassigned.changed);
    // bob is STILL the creator, so rule 1 still grants can_manage via
    // `created_by_user_id` — this scenario needs bob to be neither
    // creator nor assignee to actually lose it. Use a fresh task created
    // by the admin and assigned to bob, so bob (assignee-only) loses
    // `can_manage` by reassigning it away.
    let admin_created = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Assigned to bob by admin".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(f.member_id)),
        },
    )
    .await
    .unwrap();

    let bob_reassigns = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: admin_created.id,
            title: admin_created.title.clone(),
            kind: admin_created.kind,
            due_at: admin_created.due_at,
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await
    .unwrap();
    assert!(bob_reassigns.changed);
    assert!(
        !bob_reassigns.task.can_manage,
        "bob (assignee-only, now reassigned away) must lose can_manage in this very response"
    );

    let bobs_next_put = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: admin_created.id,
            title: "Should be forbidden".to_string(),
            kind: admin_created.kind,
            due_at: admin_created.due_at,
            assignee_user_id: UserId::new(f.admin_id),
        },
    )
    .await;
    assert!(matches!(bobs_next_put, Err(TaskError::Forbidden)));
}

/// docs/specs/SLICE_016.md §1 rule 3, §12.4: `UpdateTask` is allowed on a
/// completed task, never touches the completion columns, and its new
/// title changes the `task_completed` history line.
#[sqlx::test]
#[ignore]
async fn update_task_on_completed_task_leaves_completion_columns_and_updates_history(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Original title".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due_in(1)),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();

    let (completed_at_before, completed_by_before): (chrono::DateTime<Utc>, Uuid) =
        sqlx::query_as("SELECT completed_at, completed_by_user_id FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let update = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Retitled after completion".to_string(),
            kind: task.kind,
            due_at: task.due_at,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await
    .unwrap();
    assert!(update.changed);
    assert_eq!(update.task.completed_at, Some(completed_at_before));

    let (completed_at_after, completed_by_after): (chrono::DateTime<Utc>, Uuid) =
        sqlx::query_as("SELECT completed_at, completed_by_user_id FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(completed_at_before, completed_at_after);
    assert_eq!(completed_by_before, completed_by_after);

    // The history line's title follows the retitle.
    let mut conn = app_pool.acquire().await.unwrap();
    let history = crm_api::domain::person::queries::history_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    let entry = history
        .iter()
        .find(|e| e.kind == "task_completed" && e.id == task.id.as_uuid())
        .unwrap();
    assert_eq!(entry.detail["title"], "Retitled after completion");
}

/// docs/specs/SLICE_016.md §3, §12.4: `CompleteTask` on an already
/// completed task is `changed: false` and writes nothing (positive
/// control: completing an open task is `changed: true`).
#[sqlx::test]
#[ignore]
async fn complete_task_changed_false_and_true_with_positive_control(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Complete me".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    let events_before = recorded(&publisher).await.len();
    let first = task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    assert!(first.changed);
    assert_eq!(recorded(&publisher).await.len(), events_before + 1);

    let events_before_second = recorded(&publisher).await.len();
    let second = task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    assert!(
        !second.changed,
        "completing an already-completed task must be changed:false"
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before_second,
        "no publish on the second complete"
    );
}

/// docs/specs/SLICE_016.md §3, §12.4: `ReopenTask` on an open task is
/// `changed: false` and writes nothing (positive control: reopening a
/// completed task is `changed: true` and removes it from history).
#[sqlx::test]
#[ignore]
async fn reopen_task_changed_false_and_true_with_positive_control(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Reopen me".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    // Reopening an already-open task: changed:false, no write, no publish.
    let events_before = recorded(&publisher).await.len();
    let no_op = task::reopen_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        ReopenTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    assert!(!no_op.changed);
    assert_eq!(recorded(&publisher).await.len(), events_before);

    // Positive control: complete then reopen.
    task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    let events_before_reopen = recorded(&publisher).await.len();
    let reopened = task::reopen_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        ReopenTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    assert!(reopened.changed);
    assert!(reopened.task.completed_at.is_none());
    assert_eq!(recorded(&publisher).await.len(), events_before_reopen + 1);

    let mut conn = app_pool.acquire().await.unwrap();
    let history = crm_api::domain::person::queries::history_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert!(
        history
            .iter()
            .all(|e| !(e.kind == "task_completed" && e.id == task.id.as_uuid())),
        "reopening must remove the task_completed history entry"
    );
}

/// docs/specs/SLICE_016.md §1 rule 3, §12.4: a snooze on a completed task
/// is `changed: false` with the current (completed) row and writes
/// nothing, regardless of the requested `due_at`; snoozing an open task to
/// its current instant is also `changed: false`; a genuinely different
/// `due_at` on an open task is `changed: true` (the positive control).
#[sqlx::test]
#[ignore]
async fn snooze_task_rule_3_completed_and_same_instant_are_changed_false(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let due = due_in(1);
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Snooze me".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    // Same instant: changed:false.
    let events_before = recorded(&publisher).await.len();
    let same = task::snooze_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        SnoozeTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            due_at: due,
        },
    )
    .await
    .unwrap();
    assert!(!same.changed);
    assert_eq!(recorded(&publisher).await.len(), events_before);

    // Positive control: a different instant on an open task.
    let new_due = due_in(2);
    let changed = task::snooze_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        SnoozeTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            due_at: new_due,
        },
    )
    .await
    .unwrap();
    assert!(changed.changed);
    assert_eq!(changed.task.due_at, Some(new_due));
    assert_eq!(recorded(&publisher).await.len(), events_before + 1);

    // Complete it, then snooze to a DIFFERENT due_at: changed:false, no
    // write, the receipt's completed_at explains why, due_at unchanged.
    task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    let events_before_completed_snooze = recorded(&publisher).await.len();
    let snooze_completed = task::snooze_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        SnoozeTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            due_at: due_in(99),
        },
    )
    .await
    .unwrap();
    assert!(
        !snooze_completed.changed,
        "snooze on a completed task must be changed:false"
    );
    assert!(snooze_completed.task.completed_at.is_some());
    assert_eq!(
        snooze_completed.task.due_at,
        Some(new_due),
        "due_at must be unchanged when snoozing a completed task"
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before_completed_snooze,
        "snooze on a completed task must publish nothing"
    );
}

/// docs/specs/SLICE_016.md §1 rule 4, §12.4: the tombstone leaves every
/// other column byte-identical, `updated_at` unchanged; a repeat delete,
/// an edit of the tombstone, and a delete through another Person's path
/// are all 404.
#[sqlx::test]
#[ignore]
async fn delete_task_tombstone_byte_identical_except_three_columns(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Delete me".to_string(),
            kind: TaskKind::Call,
            due_at: Some(due_in(3)),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    #[derive(sqlx::FromRow, Debug, PartialEq)]
    struct RowSnapshot {
        kind: String,
        due_at: Option<chrono::DateTime<Utc>>,
        assignee_user_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
        completed_at: Option<chrono::DateTime<Utc>>,
        completed_by_user_id: Option<Uuid>,
        origin: String,
        correlation_id: Uuid,
        source: Option<String>,
        source_external_id: Option<String>,
        created_at: chrono::DateTime<Utc>,
        updated_at: chrono::DateTime<Utc>,
    }
    let before: RowSnapshot = sqlx::query_as(
        "SELECT kind, due_at, assignee_user_id, created_by_user_id, completed_at,
                completed_by_user_id, origin, correlation_id, source, source_external_id,
                created_at, updated_at
         FROM task WHERE id = $1",
    )
    .bind(task.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    let deleted = task::delete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);

    let (title, deleted_at, deleted_by): (String, Option<chrono::DateTime<Utc>>, Option<Uuid>) =
        sqlx::query_as("SELECT title, deleted_at, deleted_by_user_id FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(title, "");
    assert!(deleted_at.is_some());
    assert_eq!(deleted_by, Some(f.member_id));

    let after: RowSnapshot = sqlx::query_as(
        "SELECT kind, due_at, assignee_user_id, created_by_user_id, completed_at,
                completed_by_user_id, origin, correlation_id, source, source_external_id,
                created_at, updated_at
         FROM task WHERE id = $1",
    )
    .bind(task.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(
        before, after,
        "every other column must stay byte-identical, incl. updated_at"
    );

    // Repeat delete, edit-of-tombstone, complete-of-tombstone: all 404.
    let repeat = task::delete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await;
    assert!(matches!(repeat, Err(TaskError::NotFound)));

    let edit_tombstone = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
            title: "Resurrect".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(edit_tombstone, Err(TaskError::NotFound)));

    let complete_tombstone = task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await;
    assert!(matches!(complete_tombstone, Err(TaskError::NotFound)));

    // Through another Person's path in the same Organization: 404.
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let other_person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let live_task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Another live task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    let wrong_path_delete = task::delete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteTask {
            person_id: PersonId::new(other_person_id),
            task_id: live_task.id,
        },
    )
    .await;
    assert!(matches!(wrong_path_delete, Err(TaskError::NotFound)));
}

/// docs/specs/SLICE_016.md §12.4: a `complete` blocked by an in-flight,
/// uncommitted `delete` on a second connection is 404 once that commits,
/// with no completion write; the `tokio::join!` version of the same race
/// never yields 503.
#[sqlx::test]
#[ignore]
async fn complete_task_blocked_by_in_flight_delete_then_join_race_never_503(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Race with delete".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    // Deterministic ordering first: an uncommitted delete on a second
    // connection blocks complete until it commits, then complete sees the
    // tombstone and 404s.
    let mut lock_tx = app_pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE task SET title = '', deleted_at = now(), deleted_by_user_id = $2 WHERE id = $1",
    )
    .bind(task.id.as_uuid())
    .bind(f.member_id)
    .execute(&mut *lock_tx)
    .await
    .unwrap();

    let ctx = command_context(f.org_id, f.member_id);
    let complete_fut = task::complete_task(
        &app_pool,
        &publisher,
        &ctx,
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    );
    let commit_fut = async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        lock_tx.commit().await.unwrap();
    };
    let (complete_result, ()) = tokio::join!(complete_fut, commit_fut);
    assert!(
        matches!(complete_result, Err(TaskError::NotFound)),
        "complete must observe the committed delete: {complete_result:?}"
    );
    let completed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM task WHERE id = $1")
            .bind(task.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(completed_at.is_none(), "no completion write must land");

    // The genuine `tokio::join!` race (both through the command layer):
    // never 503.
    let task2 = task::create_task(
        &app_pool,
        &publisher,
        &ctx,
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Race with delete 2".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    let complete_fut2 = task::complete_task(
        &app_pool,
        &publisher,
        &ctx,
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task2.id,
        },
    );
    let delete_fut2 = task::delete_task(
        &app_pool,
        &publisher,
        &ctx,
        DeleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task2.id,
        },
    );
    let (complete_result2, delete_result2) = tokio::join!(complete_fut2, delete_fut2);
    match (complete_result2, delete_result2) {
        (Ok(_), Ok(_)) => {}
        (Err(TaskError::NotFound), Ok(_)) => {}
        other => panic!("expected {{200,200}} or {{404,200}}, never 503: {other:?}"),
    }
}

/// docs/specs/SLICE_016.md §9, §12.4: an actor deactivated inside the
/// transaction (out-of-band, and in-flight on a second connection) and an
/// admin demoted inside the transaction (out-of-band) all get 403, no
/// write, no publication.
#[sqlx::test]
#[ignore]
async fn mutations_forbidden_for_actor_deactivated_or_admin_demoted_in_transaction(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Out-of-band deactivation before the command runs.
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Author task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let deactivated = task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await;
    assert!(matches!(deactivated, Err(TaskError::Forbidden)));

    // Admin demoted (role only, still active) out-of-band: forbidden too.
    let task2 = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Admin task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(f.admin_id)),
        },
    )
    .await
    .unwrap();
    let super_admin_id =
        crate::common::create_user(&migrator_pool, "sam-demote-check@acme.test", "Sam", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        super_admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(super_admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let events_before = recorded(&publisher).await.len();
    let demoted = task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, super_admin_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task2.id,
        },
    )
    .await;
    assert!(matches!(demoted, Err(TaskError::Forbidden)));
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "a forbidden complete must publish nothing"
    );
    let completed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM task WHERE id = $1")
            .bind(task2.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert!(completed_at.is_none());

    // In-flight, uncommitted deactivation on a second connection: the own-
    // membership FOR SHARE re-read must wait for and then observe it. bob
    // is already inactive from above; use carol instead for a clean
    // active-to-inactive-in-flight transition.
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-inflight@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let carols_task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Carol's task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    let mut lock_tx = app_pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(carol_id)
    .execute(&mut *lock_tx)
    .await
    .unwrap();
    let complete_ctx = command_context(f.org_id, carol_id);
    let complete_fut = task::complete_task(
        &app_pool,
        &publisher,
        &complete_ctx,
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: carols_task.id,
        },
    );
    let commit_fut = async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        lock_tx.commit().await.unwrap();
    };
    let (complete_result, ()) = tokio::join!(complete_fut, commit_fut);
    assert!(
        matches!(complete_result, Err(TaskError::Forbidden)),
        "must observe the in-flight deactivation once it commits: {complete_result:?}"
    );
}

/// docs/specs/SLICE_016.md §9, §12.4: another Person's path, another
/// Organization's id, the viewer's OWN task in another Organization
/// (multi-membership) through THIS Organization's Person path, and a
/// tombstone all give 404 identical to a random uuid.
#[sqlx::test]
#[ignore]
async fn mutations_404_for_wrong_paths_other_organization_and_multi_membership(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-multimember@best.test",
        "Erin",
        PW,
    )
    .await;
    // The SAME user (bob) also belongs to Organization B, active.
    crate::common::add_membership_with(
        &migrator_pool,
        other_org_id,
        f.member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;

    // bob's own task, created in Organization B.
    let bobs_task_in_b = task::create_task(
        &app_pool,
        &publisher,
        &command_context(other_org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(other_person_id),
            title: "Bob's task in B".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    // Reached through Organization A's context and A's own Person: 404
    // (multi-membership does not leak Organization B's row into A).
    let via_a = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: bobs_task_in_b.id,
            title: "Stolen".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(via_a, Err(TaskError::NotFound)));

    // Reached through Organization A's context, but with Organization B's
    // OWN Person id: still 404 (the task lookup binds organization_id).
    let via_a_with_b_person = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(other_person_id),
            task_id: bobs_task_in_b.id,
            title: "Stolen again".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(via_a_with_b_person, Err(TaskError::NotFound)));

    // Another Organization's actor/context reaching Organization A's live
    // task: 404.
    let a_task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "A's task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();
    let cross_org = task::update_task(
        &app_pool,
        &publisher,
        &command_context(other_org_id, other_admin_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: a_task.id,
            title: "Stolen from A".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(other_admin_id),
        },
    )
    .await;
    assert!(matches!(cross_org, Err(TaskError::NotFound)));

    // Through ANOTHER Person's path in the SAME Organization: 404.
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let sibling_person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let wrong_person_path = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(sibling_person_id),
            task_id: a_task.id,
            title: "Wrong path".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(wrong_person_path, Err(TaskError::NotFound)));

    // A tombstone: 404.
    task::delete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: a_task.id,
        },
    )
    .await
    .unwrap();
    let tombstoned = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: a_task.id,
            title: "Resurrect".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(tombstoned, Err(TaskError::NotFound)));
}

/// docs/specs/SLICE_016.md §4, §9, §12.4: HTTP error precedence — a third
/// member's PUT with a changed (invalid) assignee is 403, not 422 (403
/// precedes 422; the command's own lock/membership/permission order runs
/// before any assignee validation).
#[sqlx::test]
#[ignore]
async fn update_task_403_precedes_422_over_http(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let carol_id = crate::common::create_user(
        &migrator_pool,
        "carol-precedence-http@acme.test",
        "Carol",
        PW,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Precedence check".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let carol = crate::common::login_cookie(&router, "carol-precedence-http@acme.test", PW).await;
    let resp = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/people/{}/tasks/{}", f.person_id, task.id),
        &carol,
        json!({
            "title": "Hijack", "kind": "follow_up", "due_at": null,
            "assignee_user_id": Uuid::new_v4(),
        }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "403 (permission) must precede 422 (a changed, invalid assignee)"
    );
}

// --- §12.5: history ------------------------------------------------------

/// docs/specs/SLICE_016.md §1 rule 5, §12.5: `task_completed` is
/// projected at `completed_at`, tie-breaking against a `note` with equal
/// explicit timestamps by `kind_rank` (task_completed, 8, after note, 7);
/// reopening removes it; completing again re-adds it at the new time.
#[sqlx::test]
#[ignore]
async fn task_completed_history_rank8_tie_break_and_reopen_readd(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "Tie-break task".to_string(),
            kind: TaskKind::default(),
            due_at: Some(due_in(1)),
            assignee_user_id: None,
        },
    )
    .await
    .unwrap();

    let shared_ts = Utc::now();
    // Force the task's completed_at to a specific instant via a raw
    // update on the owner connection (the command always uses `now()`,
    // which could never reliably collide with the note's timestamp).
    sqlx::query("UPDATE task SET completed_at = $2, completed_by_user_id = $3 WHERE id = $1")
        .bind(task.id.as_uuid())
        .bind(shared_ts)
        .bind(f.member_id)
        .execute(&app_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, created_at, updated_at)
         VALUES ($1, $2, $3, 'Same instant note', 'web_session', gen_random_uuid(), $4, $4)",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .bind(f.member_id)
    .bind(shared_ts)
    .execute(&app_pool)
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let history = crm_api::domain::person::queries::history_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    let note_index = history.iter().position(|e| e.kind == "note").unwrap();
    let task_index = history
        .iter()
        .position(|e| e.kind == "task_completed" && e.id == task.id.as_uuid())
        .unwrap();
    assert_eq!(
        task_index,
        note_index + 1,
        "task_completed (kind_rank 8) must sort immediately after note (kind_rank 7) at the same instant: {history:?}"
    );
    drop(conn);

    // Reopen removes it.
    task::reopen_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        ReopenTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let history_after_reopen = crm_api::domain::person::queries::history_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert!(
        history_after_reopen
            .iter()
            .all(|e| !(e.kind == "task_completed" && e.id == task.id.as_uuid())),
        "reopen must remove the task_completed entry"
    );
    drop(conn);

    // Complete again: re-added at the NEW time.
    task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let history_after_recomplete = crm_api::domain::person::queries::history_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    let recompleted_entry = history_after_recomplete
        .iter()
        .find(|e| e.kind == "task_completed" && e.id == task.id.as_uuid())
        .expect("task_completed must be re-added");
    assert!(
        recompleted_entry.occurred_at > shared_ts,
        "the re-added entry must be at the new completion time, not the original"
    );
}

/// docs/specs/SLICE_016.md §4, §12.5: the `task_completed` detail carries
/// `assignee` and `created_by`; `can_manage` is overwritten true for the
/// assignee, the creator, and an admin, and false for a third member; an
/// imported-shape row (NULL assignee/creator) renders `actor: null` with
/// admin-only `can_manage`, and a member's `UpdateTask` on the underlying
/// task is 403.
#[sqlx::test]
#[ignore]
async fn task_completed_history_can_manage_overwrite_and_imported_shape(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-history@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    // bob creates, admin is the assignee.
    let task = task::create_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        CreateTask {
            person_id: PersonId::new(f.person_id),
            title: "History can_manage task".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: Some(UserId::new(f.admin_id)),
        },
    )
    .await
    .unwrap();
    task::complete_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        CompleteTask {
            person_id: PersonId::new(f.person_id),
            task_id: task.id,
        },
    )
    .await
    .unwrap();

    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    for (email, expect_can_manage) in [
        ("alice-tasks@acme.test", true),    // admin
        ("bob-tasks@acme.test", true),      // creator
        ("carol-history@acme.test", false), // third member
    ] {
        let cookie = crate::common::login_cookie(&router, email, PW).await;
        let detail = crate::common::body_json(
            crate::common::get_with_cookie(
                &router,
                &format!("/api/people/{}", f.person_id),
                &cookie,
            )
            .await,
        )
        .await;
        let entry = detail["history"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "task_completed" && e["id"] == task.id.to_string())
            .unwrap();
        assert_eq!(entry["detail"]["assignee"]["id"], f.admin_id.to_string());
        assert_eq!(entry["detail"]["created_by"]["id"], f.member_id.to_string());
        assert_eq!(
            entry["detail"]["can_manage"], expect_can_manage,
            "can_manage for {email}"
        );
    }

    // An imported-shape row: NULL assignee AND creator (origin =
    // 'migration'), inserted directly and marked completed.
    let imported_task_id: Uuid = sqlx::query_scalar(
        "INSERT INTO task (organization_id, person_id, title, origin, correlation_id,
                            completed_at, completed_by_user_id)
         VALUES ($1, $2, 'Imported task', 'migration', gen_random_uuid(), now(), NULL)
         RETURNING id",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();

    let admin_cookie = crate::common::login_cookie(&router, "alice-tasks@acme.test", PW).await;
    let detail_admin = crate::common::body_json(
        crate::common::get_with_cookie(
            &router,
            &format!("/api/people/{}", f.person_id),
            &admin_cookie,
        )
        .await,
    )
    .await;
    let imported_entry = detail_admin["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "task_completed" && e["detail"]["title"] == "Imported task")
        .expect("imported task_completed entry must be present");
    assert_eq!(imported_entry["actor"], serde_json::Value::Null);
    assert_eq!(
        imported_entry["detail"]["assignee"],
        serde_json::Value::Null
    );
    assert_eq!(
        imported_entry["detail"]["created_by"],
        serde_json::Value::Null
    );
    assert_eq!(
        imported_entry["detail"]["can_manage"], true,
        "admin can manage an imported task"
    );

    let bob_cookie = crate::common::login_cookie(&router, "bob-tasks@acme.test", PW).await;
    let detail_bob = crate::common::body_json(
        crate::common::get_with_cookie(
            &router,
            &format!("/api/people/{}", f.person_id),
            &bob_cookie,
        )
        .await,
    )
    .await;
    let imported_entry_for_bob = detail_bob["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "task_completed" && e["detail"]["title"] == "Imported task")
        .unwrap();
    assert_eq!(
        imported_entry_for_bob["detail"]["can_manage"], false,
        "a non-admin member can never manage an imported task"
    );

    // A member's UpdateTask attempt on the underlying imported task: 403.
    let member_edit = task::update_task(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        UpdateTask {
            person_id: PersonId::new(f.person_id),
            task_id: TaskId::new(imported_task_id),
            title: "Should be forbidden".to_string(),
            kind: TaskKind::default(),
            due_at: None,
            assignee_user_id: UserId::new(f.member_id),
        },
    )
    .await;
    assert!(matches!(member_edit, Err(TaskError::Forbidden)));
}

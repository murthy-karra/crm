//! DB-backed Today work-source coverage (Slice 011c). These tests exercise
//! the application command/read API against `crm_app`; direct SQL is limited
//! to adversarial persisted-state fixtures that a normal caller cannot create.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::Barrier;
use uuid::Uuid;

use crate::common::{
    add_membership_with, connect_as_app, create_org_with_stages_and_member, create_user,
};
use crm_api::auth::AuthContext;
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::person::sort::PersonSort;
use crm_api::domain::person::PersonVisibilityScope;
use crm_api::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListError, SavedListScope,
};
use crm_api::domain::today::{
    self, DisableTodayWorkSource, EnableTodayWorkSource, RecommendedAction, TodayPriority,
    TodayReason, TodaySource, TodaySourceChange,
};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};

const PW: &str = "correct horse battery staple";

fn empty_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: Vec::new(),
    }
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn auth_context(organization_id: Uuid, actor_user_id: Uuid, role: Role) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(actor_user_id),
        actor_email: "fixture@example.test".to_string(),
        actor_display_name: "Fixture".to_string(),
        active_organization_id: OrganizationId::new(organization_id),
        active_organization_name: "Fixture Organization".to_string(),
        role,
    }
}

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
) -> saved_list::CreateSavedListOutcome {
    create_list_with_sort(app_pool, organization_id, actor_user_id, scope, name, None).await
}

async fn create_list_with_sort(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
    sort: Option<PersonSort>,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope,
            name: name.to_string(),
            filter: empty_filter(),
            sort,
        },
    )
    .await
    .unwrap()
}

async fn list_sources(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    role: Role,
) -> Vec<TodaySource> {
    let mut conn = app_pool.acquire().await.unwrap();
    today::list_today_work_sources(
        &mut conn,
        &auth_context(organization_id, actor_user_id, role),
    )
    .await
    .unwrap()
}

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: crm_api::ids::SavedListId,
    expected_list_revision: i64,
) -> Result<TodaySourceChange, SavedListError> {
    today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
}

async fn disable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: crm_api::ids::SavedListId,
) -> Result<TodaySourceChange, SavedListError> {
    today::disable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        DisableTodayWorkSource { list_id },
    )
    .await
}

fn assert_change(change: TodaySourceChange, enabled: bool, changed: bool) {
    assert_eq!(change.enabled, enabled);
    assert_eq!(change.changed, changed);
}

/// Preferences are private to their actor and active Organization. A member
/// may opt into a shared list, but that never makes another member's personal
/// source discoverable or a foreign list addressable.
#[sqlx::test]
#[ignore]
async fn today_sources_are_private_and_organization_scoped(migrator_pool: PgPool) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source privacy",
        "alice@today-source-privacy.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(&migrator_pool, "bob@today-source-privacy.test", "Bob", PW).await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-privacy.test",
        "Admin",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let (foreign_organization_id, foreign_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Foreign Today source privacy",
        "owner@foreign-today-source-privacy.test",
        "Foreign owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let alice_personal = create_list(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Alice private queue",
    )
    .await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        SavedListScope::Shared,
        "Team queue",
    )
    .await;
    let foreign_personal = create_list(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        SavedListScope::Personal,
        "Foreign private queue",
    )
    .await;

    assert_change(
        enable(
            &app_pool,
            organization_id,
            alice_id,
            alice_personal.list.id,
            alice_personal.list.revision,
        )
        .await
        .unwrap(),
        true,
        true,
    );
    assert!(
        list_sources(&app_pool, organization_id, bob_id, Role::Member)
            .await
            .is_empty()
    );

    let hidden = enable(
        &app_pool,
        organization_id,
        bob_id,
        alice_personal.list.id,
        alice_personal.list.revision,
    )
    .await;
    let foreign = enable(
        &app_pool,
        organization_id,
        alice_id,
        foreign_personal.list.id,
        foreign_personal.list.revision,
    )
    .await;
    assert!(matches!(hidden, Err(SavedListError::NotFound)));
    assert!(matches!(foreign, Err(SavedListError::NotFound)));

    let bob_shared = enable(
        &app_pool,
        organization_id,
        bob_id,
        shared.list.id,
        shared.list.revision,
    )
    .await
    .unwrap();
    assert_change(bob_shared, true, true);

    let alice_sources = list_sources(&app_pool, organization_id, alice_id, Role::Member).await;
    assert_eq!(alice_sources.len(), 1);
    assert_eq!(alice_sources[0].list_id, alice_personal.list.id);
    assert_eq!(alice_sources[0].name, "Alice private queue");
    assert_eq!(alice_sources[0].scope, SavedListScope::Personal);
    assert!(alice_sources[0].filter_error.is_none());

    let bob_sources = list_sources(&app_pool, organization_id, bob_id, Role::Member).await;
    assert_eq!(bob_sources.len(), 1);
    assert_eq!(bob_sources[0].list_id, shared.list.id);
    assert_eq!(bob_sources[0].name, "Team queue");
    assert_eq!(bob_sources[0].scope, SavedListScope::Shared);
    assert!(bob_sources[0].filter_error.is_none());
}

/// Target-state commands are idempotent. A source that became invalid after
/// enabling is still visible to its owner and can always be disabled.
#[sqlx::test]
#[ignore]
async fn today_source_commands_are_idempotent_and_disable_stored_invalid_source(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source idempotence",
        "owner@today-source-idempotence.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Mutable queue",
    )
    .await;

    let first_enable = enable(
        &app_pool,
        organization_id,
        owner_id,
        list.list.id,
        list.list.revision,
    )
    .await
    .unwrap();
    let repeated_enable = enable(
        &app_pool,
        organization_id,
        owner_id,
        list.list.id,
        list.list.revision,
    )
    .await
    .unwrap();
    assert_change(first_enable, true, true);
    assert_change(repeated_enable, true, false);

    // Simulate a stale stored definition left by a prior version or direct
    // data repair. The source must be reported as invalid, not dropped, so
    // the owner retains a route to turn it off.
    sqlx::query(
        "UPDATE saved_list SET filter = '{\"version\":2,\"clauses\":[]}'::jsonb WHERE id = $1",
    )
    .bind(list.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();

    let configured = list_sources(&app_pool, organization_id, owner_id, Role::Member).await;
    assert_eq!(configured.len(), 1);
    assert_eq!(configured[0].list_id, list.list.id);
    assert!(matches!(
        configured[0].filter_error,
        Some(saved_list::SavedListFilterError::UnsupportedFilter)
    ));

    let first_disable = disable(&app_pool, organization_id, owner_id, list.list.id)
        .await
        .unwrap();
    let repeated_disable = disable(&app_pool, organization_id, owner_id, list.list.id)
        .await
        .unwrap();
    assert_change(first_disable, false, true);
    assert_change(repeated_disable, false, false);
    assert!(
        list_sources(&app_pool, organization_id, owner_id, Role::Member)
            .await
            .is_empty()
    );
}

/// docs/specs/SLICE_011e.md §9.14: `GET /api/today/sources` (via
/// `list_today_work_sources`) reports `filter_error:"invalid_tag"` -- never
/// `unsupported_filter` -- for a source naming a tag that has since been
/// deleted.
#[sqlx::test]
#[ignore]
async fn today_source_reports_invalid_tag_for_a_deleted_tag(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source invalid tag",
        "owner@today-source-invalid-tag.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Tag source",
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        owner_id,
        list.list.id,
        list.list.revision,
    )
    .await
    .unwrap();

    let tag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tag (organization_id, name, created_by_user_id) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(organization_id)
    .bind("Will be deleted")
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let filter_json =
        format!(r#"{{"version":1,"clauses":[{{"kind":"tags","tag_ids":["{tag_id}"]}}]}}"#);
    sqlx::query("UPDATE saved_list SET filter = $1::jsonb WHERE id = $2")
        .bind(filter_json)
        .bind(list.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tag WHERE id = $1")
        .bind(tag_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let configured = list_sources(&app_pool, organization_id, owner_id, Role::Member).await;
    assert_eq!(configured.len(), 1);
    assert_eq!(configured[0].list_id, list.list.id);
    assert!(matches!(
        configured[0].filter_error,
        Some(saved_list::SavedListFilterError::InvalidTag)
    ));
}

/// The quota is based on current live visible definitions. A retained source
/// row for a rollback-era tombstone does not appear in configuration and does
/// not consume one of the five slots.
#[sqlx::test]
#[ignore]
async fn today_source_cap_excludes_stale_tombstones(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source cap",
        "owner@today-source-cap.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let mut lists = Vec::new();
    for index in 1..=5 {
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            SavedListScope::Personal,
            &format!("Live queue {index}"),
        )
        .await;
        assert_change(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap(),
            true,
            true,
        );
        lists.push(list);
    }

    let stale = &lists[0];
    // A supported rollback can produce this old-style tombstone without the
    // current delete command's cleanup. Preserve its preference row to prove
    // the read and quota joins exclude it rather than counting raw rows.
    sqlx::query(
        r#"UPDATE saved_list
           SET name = NULL, filter = NULL, revision = revision + 1,
               deleted_at = now(), updated_at = now()
           WHERE id = $1"#,
    )
    .bind(stale.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();

    let sixth = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Live queue 6",
    )
    .await;
    assert_change(
        enable(
            &app_pool,
            organization_id,
            owner_id,
            sixth.list.id,
            sixth.list.revision,
        )
        .await
        .unwrap(),
        true,
        true,
    );

    let configured = list_sources(&app_pool, organization_id, owner_id, Role::Member).await;
    assert_eq!(configured.len(), 5);
    assert!(configured
        .iter()
        .all(|source| source.list_id != stale.list.id));
    assert!(configured
        .iter()
        .any(|source| source.list_id == sixth.list.id));
    let raw_row_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(
        raw_row_count, 6,
        "the retained tombstone preference is intentional"
    );

    let seventh = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Live queue 7",
    )
    .await;
    let over_cap = enable(
        &app_pool,
        organization_id,
        owner_id,
        seventh.list.id,
        seventh.list.revision,
    )
    .await;
    assert!(matches!(
        over_cap,
        Err(SavedListError::TodaySourceLimitReached)
    ));
}

/// The per-Organization source lock serializes competing target-state
/// commands. Starting from four sources, two simultaneous enables leave
/// exactly one fifth source; they can never both observe a stale count.
#[sqlx::test]
#[ignore]
async fn today_source_five_live_source_cap_holds_under_concurrent_enables(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source concurrent cap",
        "owner@today-source-concurrent-cap.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    for index in 1..=4 {
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            SavedListScope::Personal,
            &format!("Already enabled {index}"),
        )
        .await;
        assert_change(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap(),
            true,
            true,
        );
    }
    let left = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Concurrent fifth A",
    )
    .await;
    let right = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Concurrent fifth B",
    )
    .await;

    let barrier = Arc::new(Barrier::new(3));
    let left_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            enable(
                &pool,
                organization_id,
                owner_id,
                left.list.id,
                left.list.revision,
            )
            .await
        })
    };
    let right_task = {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            enable(
                &pool,
                organization_id,
                owner_id,
                right.list.id,
                right.list.revision,
            )
            .await
        })
    };
    barrier.wait().await;
    let outcomes = [left_task.await.unwrap(), right_task.await.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(change) if change.changed))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(SavedListError::TodaySourceLimitReached)))
            .count(),
        1
    );
    assert_eq!(
        list_sources(&app_pool, organization_id, owner_id, Role::Member)
            .await
            .len(),
        5
    );
}

/// Lists are allowed to supply People who have no Inquiry, contact method or
/// assignment. Those rows are list-tier work, retain nullable inquiry/contact
/// fields, and use the review action rather than pretending that email exists.
#[sqlx::test]
#[ignore]
async fn today_source_only_person_keeps_nullable_fields_and_review_action(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source list-only Person",
        "owner@today-source-list-only.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, first_name, created_at)\
         VALUES ($1, $2, 'List only', now()) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "No inquiry queue",
    )
    .await;
    assert_change(
        enable(
            &app_pool,
            organization_id,
            owner_id,
            list.list.id,
            list.list.revision,
        )
        .await
        .unwrap(),
        true,
        true,
    );

    let mut conn = app_pool.acquire().await.unwrap();
    let today = today::query(
        &mut conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(owner_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert_eq!(
        today.sources.status,
        crm_api::domain::today::TodaySourcesStatus::Complete,
        "source evaluation should be complete: {:#?}",
        today.sources
    );
    assert_eq!(today.items.len(), 1);
    let item = &today.items[0];
    assert_eq!(item.person.id.as_uuid(), person_id);
    assert_eq!(item.priority, TodayPriority::List);
    assert_eq!(item.recommended_action, RecommendedAction::ReviewPerson);
    assert!(item.waiting_since.is_none());
    assert!(item.latest_inquiry.is_none());
    assert!(item.last_contact_attempt.is_none());
    assert_eq!(
        item.reasons,
        vec![TodayReason::ListMember {
            list_id: list.list.id,
            name: "No inquiry queue".to_string(),
        }]
    );
}

/// Deleting a source definition removes every actor's preference for that
/// list in the same transaction, including member opt-ins to a shared list.
#[sqlx::test]
#[ignore]
async fn deleting_saved_list_removes_all_today_source_preferences(migrator_pool: PgPool) {
    let (organization_id, member_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source delete cleanup",
        "member@today-source-delete.test",
        "Member",
        PW,
    )
    .await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-delete.test",
        "Admin",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        SavedListScope::Shared,
        "Shared delete queue",
    )
    .await;

    for actor_user_id in [admin_id, member_id] {
        assert_change(
            enable(
                &app_pool,
                organization_id,
                actor_user_id,
                shared.list.id,
                shared.list.revision,
            )
            .await
            .unwrap(),
            true,
            true,
        );
    }
    let preference_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND list_id = $2",
    )
    .bind(organization_id)
    .bind(shared.list.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(preference_count, 2);

    let deleted = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        DeleteSavedList {
            list_id: shared.list.id,
            expected_revision: shared.list.revision,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);

    assert!(
        list_sources(&app_pool, organization_id, admin_id, Role::Admin)
            .await
            .is_empty()
    );
    assert!(
        list_sources(&app_pool, organization_id, member_id, Role::Member)
            .await
            .is_empty()
    );
    let remaining_preferences: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND list_id = $2",
    )
    .bind(organization_id)
    .bind(shared.list.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(remaining_preferences, 0);
}

/// docs/specs/SLICE_011b_SORT.md §7, §11.10: Today never uses list order —
/// source evaluation reads the filter only (`today/sources.rs`'s
/// `evaluated_sources_raw` selects `filter`, never `sort_key`/
/// `sort_direction`). A sorted list and its unsorted duplicate (same
/// filter, only the stored sort differs) are therefore indistinguishable
/// Today sources: identical items, identical `ListMember` reasons for both
/// list ids on every matching Person. A list whose stored sort is
/// unreadable still evaluates as a source, because evaluation never reads
/// sort at all.
#[sqlx::test]
#[ignore]
async fn sorted_list_and_its_unsorted_duplicate_produce_identical_today_bodies(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today sort parity",
        "owner@today-sort-parity.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let mut person_ids = Vec::new();
    for name in ["Amelia", "Zeke"] {
        let person_id: Uuid = sqlx::query_scalar(
            "INSERT INTO person (organization_id, stage_id, first_name, created_at)\
             VALUES ($1, $2, $3, now()) RETURNING id",
        )
        .bind(organization_id)
        .bind(stage_id)
        .bind(name)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        person_ids.push(person_id);
    }

    // Same (empty, matches-everything) filter; only the stored sort
    // differs between the two lists.
    let unsorted = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Unsorted duplicate",
    )
    .await;
    let sorted = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Sorted duplicate",
        Some(PersonSort::parse("name.asc").unwrap()),
    )
    .await;
    for list in [&unsorted, &sorted] {
        assert_change(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap(),
            true,
            true,
        );
    }

    let mut conn = app_pool.acquire().await.unwrap();
    let today = today::query(
        &mut conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(owner_id),
        Utc::now(),
    )
    .await
    .unwrap();
    drop(conn);
    assert_eq!(
        today.sources.status,
        crm_api::domain::today::TodaySourcesStatus::Complete,
        "source evaluation should be complete: {:#?}",
        today.sources
    );
    assert_eq!(today.items.len(), 2);
    for item in &today.items {
        assert!(person_ids.contains(&item.person.id.as_uuid()));
        let reason_list_ids: std::collections::HashSet<_> = item
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                TodayReason::ListMember { list_id, .. } => Some(*list_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            reason_list_ids,
            [unsorted.list.id, sorted.list.id].into_iter().collect(),
            "both the sorted list and its unsorted duplicate must match every Person identically"
        );
    }

    // A list whose stored sort is unreadable still evaluates as a Today
    // source — source evaluation reads the filter only.
    sqlx::query("ALTER TABLE saved_list DROP CONSTRAINT saved_list_sort_key_check")
        .execute(&migrator_pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE saved_list SET sort_key = 'distance', sort_direction = 'asc' WHERE id = $1",
    )
    .bind(sorted.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let today_after_skew = today::query(
        &mut conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(owner_id),
        Utc::now(),
    )
    .await
    .unwrap();
    assert_eq!(
        today_after_skew.sources.status,
        crm_api::domain::today::TodaySourcesStatus::Complete,
        "an unreadable stored sort must not degrade source evaluation"
    );
    assert_eq!(today_after_skew.items.len(), 2);
    for item in &today_after_skew.items {
        let reason_list_ids: std::collections::HashSet<_> = item
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                TodayReason::ListMember { list_id, .. } => Some(*list_id),
                _ => None,
            })
            .collect();
        assert!(
            reason_list_ids.contains(&sorted.list.id),
            "the unreadable-sort list must still evaluate as a source"
        );
    }
}

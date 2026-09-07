//! Acceptance coverage for Slice 011c's bounded Today source evaluation.
//!
//! These tests exercise persisted source configuration through the public
//! command API, then assert the real `today::query` result. Fixture SQL only
//! creates People and immutable facts needed to put the read model at its
//! cardinality and ordering boundaries.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::common::{
    add_membership_with, connect_as_app, create_org_with_stages_and_member, create_user,
};
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, Clause, FilterDefinition, StageClause,
};
use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayList, TodayPriority, TodayReason,
};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn stage_filter(stage_id: Uuid) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::Stage(StageClause {
            stage_ids: vec![StageId::new(stage_id)],
        })],
    }
}

fn assigned_filter(assignees: Vec<Assignee>) -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause { assignees })],
    }
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
    filter: FilterDefinition,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope,
            name: name.to_string(),
            filter,
            sort: None,
        },
    )
    .await
    .unwrap()
}

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: SavedListId,
    expected_list_revision: i64,
) {
    let change = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
    .unwrap();
    assert!(change.enabled);
}

async fn today_for(app_pool: &PgPool, organization_id: Uuid, viewer_id: Uuid) -> TodayList {
    let mut conn = app_pool.acquire().await.unwrap();
    today::query(
        &mut conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(viewer_id),
        Utc::now(),
    )
    .await
    .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    id: Uuid,
    assigned_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO person (id, organization_id, stage_id, assigned_user_id, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempt(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached')",
    )
    .bind(organization_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_fresh_inquiry(pool: &PgPool, organization_id: Uuid, person_id: Uuid) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, 'acceptance-fixture', statement_timestamp())",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_builtin_people(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Uuid,
    count: i64,
) {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id)
         SELECT $1, $2, $3 FROM generate_series(1, $4)
         RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .bind(count)
    .fetch_all(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         SELECT $1, p.id, gen_random_uuid(), 'acceptance-fixture', statement_timestamp()
         FROM person p
         WHERE p.organization_id = $1 AND p.id = ANY($2)",
    )
    .bind(organization_id)
    .bind(&ids)
    .execute(pool)
    .await
    .unwrap();
}

fn list_reason_ids(item: &TodayItem) -> Vec<SavedListId> {
    item.reasons
        .iter()
        .filter_map(|reason| match reason {
            TodayReason::ListMember { list_id, .. } => Some(*list_id),
            _ => None,
        })
        .collect()
}

fn has_list_reason(item: &TodayItem, list_id: SavedListId) -> bool {
    list_reason_ids(item).contains(&list_id)
}

/// List-only items use the source ordering contract, independent of fixture
/// insertion order: null contacts first, then effective contact time, then
/// the Person UUID. The later attempt for `latest_contact` proves that the
/// observed order follows the actual last contact rather than its first row.
#[sqlx::test]
#[ignore]
async fn today_source_orders_list_only_people_by_never_contacted_last_contact_and_uuid(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source contact ordering",
        "owner@today-source-contact-ordering.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let never_first = Uuid::from_u128(0x11);
    let never_second = Uuid::from_u128(0x22);
    let oldest_contact = Uuid::from_u128(0x33);
    let contact_tie_first = Uuid::from_u128(0x44);
    let contact_tie_second = Uuid::from_u128(0x55);
    let latest_contact = Uuid::from_u128(0x66);
    for id in [
        never_first,
        never_second,
        oldest_contact,
        contact_tie_first,
        contact_tie_second,
        latest_contact,
    ] {
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            id,
            None,
            now - ChronoDuration::days(1),
        )
        .await;
    }
    insert_contact_attempt(
        &app_pool,
        organization_id,
        oldest_contact,
        now - ChronoDuration::hours(5),
    )
    .await;
    let tied_contact_at = now - ChronoDuration::hours(3);
    insert_contact_attempt(
        &app_pool,
        organization_id,
        contact_tie_first,
        tied_contact_at,
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        contact_tie_second,
        tied_contact_at,
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        latest_contact,
        now - ChronoDuration::hours(6),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        latest_contact,
        now - ChronoDuration::hours(1),
    )
    .await;

    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Contact order queue",
        stage_filter(stage_id),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        owner_id,
        list.list.id,
        list.list.revision,
    )
    .await;

    let today = today_for(&app_pool, organization_id, owner_id).await;
    let actual_ids: Vec<Uuid> = today
        .items
        .iter()
        .map(|item| item.person.id.as_uuid())
        .collect();
    assert_eq!(
        actual_ids,
        vec![
            never_first,
            never_second,
            oldest_contact,
            contact_tie_first,
            contact_tie_second,
            latest_contact,
        ]
    );
    assert!(today.items.iter().all(|item| {
        item.priority == TodayPriority::List
            && item.waiting_since.is_none()
            && item.latest_inquiry.is_none()
            && has_list_reason(item, list.list.id)
    }));
    let latest_item = today
        .items
        .iter()
        .find(|item| item.person.id.as_uuid() == latest_contact)
        .unwrap();
    assert_eq!(
        latest_item
            .last_contact_attempt
            .as_ref()
            .map(|attempt| attempt.occurred_at),
        Some(now - ChronoDuration::hours(1))
    );
}

/// Built-in rows are excluded before source prefixes are limited, but every
/// retained built-in is independently checked for source membership. Two
/// identical sources make the deduplication and complete-reason requirement
/// observable for both a built-in and a list-only row at capacity.
#[sqlx::test]
#[ignore]
async fn today_source_deduplicates_overlap_and_keeps_builtin_reasons_beyond_prefix(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source overlap",
        "owner@today-source-overlap.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;

    // This UUID follows all 200 source-only UUIDs. It must still gain both
    // reasons through the bounded B-membership query, not source hydration.
    let builtin_id = Uuid::from_u128(u128::MAX);
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        builtin_id,
        Some(owner_id),
        Utc::now(),
    )
    .await;
    insert_fresh_inquiry(&app_pool, organization_id, builtin_id).await;
    for value in 1..=200u128 {
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            Uuid::from_u128(value),
            None,
            Utc::now(),
        )
        .await;
    }

    let first = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Overlap first",
        stage_filter(stage_id),
    )
    .await;
    let second = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Overlap second",
        stage_filter(stage_id),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        owner_id,
        first.list.id,
        first.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        owner_id,
        second.list.id,
        second.list.revision,
    )
    .await;

    let today = today_for(&app_pool, organization_id, owner_id).await;
    assert_eq!(today.items.len(), 200);
    assert!(
        today.truncated,
        "the 200th non-built-in match is a sentinel"
    );
    assert_eq!(
        today
            .items
            .iter()
            .filter(|item| item.person.id.as_uuid() == builtin_id)
            .count(),
        1,
        "overlap must never duplicate a built-in Person"
    );

    let mut expected_reason_ids = vec![first.list.id, second.list.id];
    expected_reason_ids.sort_by_key(|list_id| list_id.as_uuid());
    let builtin = today
        .items
        .iter()
        .find(|item| item.person.id.as_uuid() == builtin_id)
        .unwrap();
    assert_eq!(builtin.priority, TodayPriority::High);
    assert_eq!(list_reason_ids(builtin), expected_reason_ids);

    let overlap_list_only = today
        .items
        .iter()
        .find(|item| item.person.id.as_uuid() == Uuid::from_u128(1))
        .unwrap();
    assert_eq!(overlap_list_only.priority, TodayPriority::List);
    assert_eq!(list_reason_ids(overlap_list_only), expected_reason_ids);
}

/// Admission reserves built-ins before sources. At 199, one list-only Person
/// fills the remaining slot; at 200 it is only the truncation sentinel; at
/// 201 the existing built-in sentinel suppresses non-built-in source work.
#[sqlx::test]
#[ignore]
async fn today_source_reserves_builtin_capacity_at_199_200_and_201(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;
    for (builtin_count, expected_list_items, expected_truncated) in
        [(199_i64, 1_usize, false), (200, 0, true), (201, 0, true)]
    {
        let (organization_id, owner_id) = create_org_with_stages_and_member(
            &migrator_pool,
            &format!("Today source built-in reservation {builtin_count}"),
            &format!("owner@today-source-reservation-{builtin_count}.test"),
            "Owner",
            PW,
        )
        .await;
        let stage_id = first_stage_id(&app_pool, organization_id).await;
        insert_builtin_people(
            &app_pool,
            organization_id,
            stage_id,
            owner_id,
            builtin_count,
        )
        .await;
        let source_only_id = Uuid::new_v4();
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            source_only_id,
            None,
            Utc::now(),
        )
        .await;
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            SavedListScope::Personal,
            "Capacity source",
            stage_filter(stage_id),
        )
        .await;
        enable(
            &app_pool,
            organization_id,
            owner_id,
            list.list.id,
            list.list.revision,
        )
        .await;

        let today = today_for(&app_pool, organization_id, owner_id).await;
        assert_eq!(today.items.len(), 200, "case B={builtin_count}");
        assert_eq!(
            today.truncated, expected_truncated,
            "case B={builtin_count}"
        );
        assert_eq!(
            today
                .items
                .iter()
                .filter(|item| item.priority == TodayPriority::High)
                .count(),
            200 - expected_list_items,
            "case B={builtin_count}"
        );
        assert_eq!(
            today
                .items
                .iter()
                .filter(|item| item.person.id.as_uuid() == source_only_id)
                .count(),
            expected_list_items,
            "case B={builtin_count}"
        );
    }
}

/// A source consumes its own filtered membership, not the People endpoint's
/// 500-row display prefix. The target sorts 501st by People `created_at`, but
/// first by the source's list-only ordering and therefore must be admitted.
#[sqlx::test]
#[ignore]
async fn today_source_evaluates_a_match_beyond_the_people_500_display_cap(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source beyond People cap",
        "owner@today-source-people-cap.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let target_id = Uuid::from_u128(1);
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        target_id,
        None,
        Utc::now() - ChronoDuration::days(2),
    )
    .await;
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, created_at)
         SELECT $1, $2, statement_timestamp() - make_interval(secs => source.i)
         FROM generate_series(1, 500) AS source(i)",
    )
    .bind(organization_id)
    .bind(stage_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let filter = stage_filter(stage_id);
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let params = filter.to_query_params(UserId::new(owner_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let (people_prefix, people_truncated) =
        person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
    assert!(people_truncated);
    assert_eq!(people_prefix.len(), 500);
    assert!(
        people_prefix
            .iter()
            .all(|person| person.id.as_uuid() != target_id),
        "fixture target is outside the People display prefix"
    );
    drop(conn);

    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "All staged people",
        filter,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        owner_id,
        list.list.id,
        list.list.revision,
    )
    .await;
    let today = today_for(&app_pool, organization_id, owner_id).await;
    assert_eq!(today.items.len(), 200);
    assert!(today.truncated);
    assert_eq!(today.items[0].person.id.as_uuid(), target_id);
    assert!(has_list_reason(&today.items[0], list.list.id));
}

/// `me` resolves at source evaluation for the current viewer, while explicit
/// colleague and unassigned entries retain ordinary Organization visibility.
/// The matching static source is also compared with the public filtered-People
/// query to pin the shared v1 predicate matrix where that comparison is exact.
#[sqlx::test]
#[ignore]
async fn today_source_resolves_me_per_viewer_and_matches_other_and_unassigned_people(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source viewer-relative filters",
        "alice@today-source-viewer-filter.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(
        &migrator_pool,
        "bob@today-source-viewer-filter.test",
        "Bob",
        PW,
    )
    .await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-viewer-filter.test",
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
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let created_at = Utc::now();
    let alice_person = Uuid::from_u128(0x11);
    let bob_person = Uuid::from_u128(0x22);
    let unassigned_person = Uuid::from_u128(0x33);
    let admin_person = Uuid::from_u128(0x44);
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        alice_person,
        Some(alice_id),
        created_at,
    )
    .await;
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        bob_person,
        Some(bob_id),
        created_at,
    )
    .await;
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        unassigned_person,
        None,
        created_at,
    )
    .await;
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        admin_person,
        Some(admin_id),
        created_at,
    )
    .await;

    let me_list = create_list(
        &app_pool,
        organization_id,
        admin_id,
        SavedListScope::Shared,
        "My assigned people",
        assigned_filter(vec![Assignee::Me]),
    )
    .await;
    let other_filter = assigned_filter(vec![
        Assignee::User(UserId::new(bob_id)),
        Assignee::Unassigned,
    ]);
    let other_list = create_list(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Bob and unassigned",
        other_filter.clone(),
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        alice_id,
        me_list.list.id,
        me_list.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        bob_id,
        me_list.list.id,
        me_list.list.revision,
    )
    .await;
    enable(
        &app_pool,
        organization_id,
        alice_id,
        other_list.list.id,
        other_list.list.revision,
    )
    .await;

    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let params = other_filter.to_query_params(UserId::new(alice_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let (people_matches, people_truncated) =
        person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
    assert!(!people_truncated);
    let mut expected_other_ids: Vec<Uuid> = people_matches
        .iter()
        .map(|person| person.id.as_uuid())
        .collect();
    expected_other_ids.sort_unstable();
    assert_eq!(expected_other_ids, vec![bob_person, unassigned_person]);
    drop(conn);

    let alice_today = today_for(&app_pool, organization_id, alice_id).await;
    let bob_today = today_for(&app_pool, organization_id, bob_id).await;
    let mut alice_other_ids: Vec<Uuid> = alice_today
        .items
        .iter()
        .filter(|item| has_list_reason(item, other_list.list.id))
        .map(|item| item.person.id.as_uuid())
        .collect();
    alice_other_ids.sort_unstable();
    assert_eq!(alice_other_ids, expected_other_ids);
    assert!(alice_today
        .items
        .iter()
        .any(|item| item.person.id.as_uuid() == alice_person
            && has_list_reason(item, me_list.list.id)));
    assert!(alice_today
        .items
        .iter()
        .all(|item| item.person.id.as_uuid() != admin_person));

    assert_eq!(bob_today.items.len(), 1);
    assert_eq!(bob_today.items[0].person.id.as_uuid(), bob_person);
    assert!(has_list_reason(&bob_today.items[0], me_list.list.id));
    assert!(!has_list_reason(&bob_today.items[0], other_list.list.id));
}

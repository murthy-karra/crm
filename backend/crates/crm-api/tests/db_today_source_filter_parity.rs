//! Filter-parity coverage for Slice 011c Today sources.
//!
//! Every source assertion observes the list-member reason for that exact
//! definition, never all Today rows: a matching Person may already be a
//! built-in item. The ordinary parity fixtures keep every date far from an
//! age boundary; the final fixed-clock case pins the boundary itself.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::common::{
    add_membership_with, connect_as_app, create_org_with_stages_and_member, create_user,
};
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::capture::pipeline::{insert_fact_and_maybe_attempt, ParsedMetadata, Via};
use crm_api::domain::capture::Direction;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AgeClause, AgeSpec, AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    SourceClause, StageClause,
};
use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::{
    self, EnableTodayWorkSource, TodayItem, TodayPriority, TodayReason, TodaySourcesStatus,
};
use crm_api::ids::{
    CorrelationId, CorrespondenceRawId, OrganizationId, PersonId, SavedListId, StageId, UserId,
};

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn ids(values: impl IntoIterator<Item = Uuid>) -> BTreeSet<Uuid> {
    values.into_iter().collect()
}

async fn first_stage_ids(pool: &PgPool, organization_id: Uuid) -> (Uuid, Uuid) {
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 2",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .unwrap();
    (stages[0], stages[1])
}

async fn insert_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id, created_at) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempt(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
            person_id, channel, outcome) \
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached') \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_correction(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    actor_user_id: Uuid,
    corrects_id: Uuid,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
            corrects_id, person_id, channel, outcome) \
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'left_message')",
    )
    .bind(organization_id)
    .bind(actor_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(corrects_id)
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
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

async fn insert_raw(pool: &PgPool, organization_id: Uuid) -> CorrespondenceRawId {
    let id: Uuid = sqlx::query_scalar(
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
    CorrespondenceRawId::new(id)
}

/// The production capture helper writes the outbound auto-attempt. Direct
/// fixture SQL is used only for the raw encrypted envelope it references.
// The fixture helper mirrors the fact's columns, hence the many arguments.
#[allow(clippy::too_many_arguments)]
async fn capture_fact(
    app_pool: &PgPool,
    migrator_pool: &PgPool,
    organization_id: Uuid,
    agent_user_id: Uuid,
    person_id: Uuid,
    direction: Direction,
    occurred_at: DateTime<Utc>,
    backdated: bool,
) -> Uuid {
    let raw_id = insert_raw(migrator_pool, organization_id).await;
    let metadata = ParsedMetadata {
        via: Via::Cc,
        forward_style: None,
        forward_depth: 0,
        occurred_at,
        backdated,
        message_id: Some(format!("parity-{}@fixture.test", Uuid::new_v4())),
        thread_key: None,
        working_from: None,
        working_to_cc: Vec::new(),
    };
    let mut transaction = app_pool.begin().await.unwrap();
    let id = insert_fact_and_maybe_attempt(
        &mut transaction,
        OrganizationId::new(organization_id),
        UserId::new(agent_user_id),
        PersonId::new(person_id),
        direction,
        &metadata,
        raw_id,
        CorrelationId::new(Uuid::new_v4()),
    )
    .await
    .unwrap()
    .expect("fresh fixture correspondence must create a fact");
    transaction.commit().await.unwrap();
    id
}

async fn create_and_enable_source(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
    filter: FilterDefinition,
) -> SavedListId {
    let list = saved_list::create_saved_list(
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
    .unwrap();
    let result = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id: list.list.id,
            expected_list_revision: list.list.revision,
        },
    )
    .await
    .unwrap();
    assert!(result.changed);
    list.list.id
}

async fn enable_existing_source(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: SavedListId,
    revision: i64,
) {
    let result = today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision: revision,
        },
    )
    .await
    .unwrap();
    assert!(result.changed);
}

async fn filtered_ids(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    filter: &FilterDefinition,
) -> BTreeSet<Uuid> {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let params = filter.to_query_params(UserId::new(viewer_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let (people, truncated) = person_queries::filtered_summaries(&mut conn, &scope, &params)
        .await
        .unwrap();
    assert!(
        !truncated,
        "the parity fixture is intentionally below 200 rows"
    );
    ids(people.into_iter().map(|person| person.id.as_uuid()))
}

async fn filtered_ids_at(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    filter: &FilterDefinition,
    fixed_now: DateTime<Utc>,
) -> BTreeSet<Uuid> {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let params = filter.to_query_params(UserId::new(viewer_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let (people, truncated) =
        person_queries::filtered_summaries_at(&mut conn, &scope, &params, fixed_now)
            .await
            .unwrap();
    assert!(
        !truncated,
        "the parity fixture is intentionally below 200 rows"
    );
    ids(people.into_iter().map(|person| person.id.as_uuid()))
}

fn source_reason_ids(items: &[TodayItem], list_id: SavedListId) -> BTreeSet<Uuid> {
    ids(items.iter().filter_map(|item| {
        item.reasons.iter().any(|reason| {
            matches!(reason, TodayReason::ListMember { list_id: reason_list_id, .. } if *reason_list_id == list_id)
        })
        .then_some(item.person.id.as_uuid())
    }))
}

async fn today_items(app_pool: &PgPool, organization_id: Uuid, viewer_id: Uuid) -> Vec<TodayItem> {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = today::query(&mut conn, &scope, UserId::new(viewer_id), Utc::now())
        .await
        .unwrap();
    assert!(matches!(list.sources.status, TodaySourcesStatus::Complete));
    list.items
}

async fn today_reason_ids_at(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    fixed_now: DateTime<Utc>,
    list_id: SavedListId,
) -> BTreeSet<Uuid> {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = today::query_at(&mut conn, &scope, UserId::new(viewer_id), fixed_now)
        .await
        .unwrap();
    assert_eq!(list.generated_at, fixed_now);
    assert!(matches!(list.sources.status, TodaySourcesStatus::Complete));
    source_reason_ids(&list.items, list_id)
}

async fn assert_reason_parity(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    list_id: SavedListId,
    filter: &FilterDefinition,
    expected: BTreeSet<Uuid>,
) {
    let people = filtered_ids(app_pool, organization_id, viewer_id, filter).await;
    assert_eq!(people, expected, "known People-filter membership");
    let source = source_reason_ids(
        &today_items(app_pool, organization_id, viewer_id).await,
        list_id,
    );
    assert_eq!(source, expected, "known Today source membership");
    assert_eq!(source, people, "Today source and People filter must agree");
}

async fn assert_fixed_reason_parity(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    fixed_now: DateTime<Utc>,
    list_id: SavedListId,
    filter: &FilterDefinition,
    expected: BTreeSet<Uuid>,
) {
    let people = filtered_ids_at(app_pool, organization_id, viewer_id, filter, fixed_now).await;
    assert_eq!(
        people, expected,
        "known fixed-clock People-filter membership"
    );
    let source =
        today_reason_ids_at(app_pool, organization_id, viewer_id, fixed_now, list_id).await;
    assert_eq!(
        source, expected,
        "known fixed-clock Today source membership"
    );
    assert_eq!(
        source, people,
        "fixed-clock Today source and People filter must agree"
    );
}

/// A single valid source combines every v1 clause kind. Dedicated companion
/// sources pin inactive assignees, unassigned rows, and false boolean axes;
/// the known IDs make a shared predicate regression visible on both paths.
#[sqlx::test]
#[ignore]
async fn today_source_parity_covers_every_v1_clause_kind_and_built_in_and_prefix_paths(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source all clause parity",
        "alice@today-source-all-clause-parity.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(
        &migrator_pool,
        "bob@today-source-all-clause-parity.test",
        "Bob",
        PW,
    )
    .await;
    let inactive_id = create_user(
        &migrator_pool,
        "inactive@today-source-all-clause-parity.test",
        "Inactive",
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
        inactive_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let (stage_a, stage_b) = first_stage_ids(&app_pool, organization_id).await;
    let now = Utc::now();

    let all_axes = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(3),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        all_axes,
        Uuid::new_v4(),
        "present",
        now - ChronoDuration::days(2),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        all_axes,
        now - ChronoDuration::days(5),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        all_axes,
        Direction::Inbound,
        now - ChronoDuration::days(4),
        false,
    )
    .await;
    insert_contact_method(
        &migrator_pool,
        organization_id,
        all_axes,
        "phone",
        "+15555550101",
    )
    .await;
    insert_contact_method(
        &migrator_pool,
        organization_id,
        all_axes,
        "email",
        "all-axes@example.test",
    )
    .await;

    let wrong_stage = insert_person(
        &migrator_pool,
        organization_id,
        stage_b,
        Some(alice_id),
        now - ChronoDuration::days(3),
    )
    .await;
    let wrong_assignee = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(bob_id),
        now - ChronoDuration::days(3),
    )
    .await;
    for person_id in [wrong_stage, wrong_assignee] {
        insert_inquiry(
            &migrator_pool,
            organization_id,
            person_id,
            Uuid::new_v4(),
            "present",
            now - ChronoDuration::days(2),
        )
        .await;
        insert_contact_attempt(
            &migrator_pool,
            organization_id,
            person_id,
            now - ChronoDuration::days(5),
        )
        .await;
        capture_fact(
            &app_pool,
            &migrator_pool,
            organization_id,
            alice_id,
            person_id,
            Direction::Inbound,
            now - ChronoDuration::days(4),
            false,
        )
        .await;
        insert_contact_method(
            &migrator_pool,
            organization_id,
            person_id,
            "phone",
            &format!("+1555555{person_id}"),
        )
        .await;
        insert_contact_method(
            &migrator_pool,
            organization_id,
            person_id,
            "email",
            &format!("{person_id}@example.test"),
        )
        .await;
    }

    let inactive_assignee = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(inactive_id),
        now - ChronoDuration::days(90),
    )
    .await;
    let unassigned = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        None,
        now - ChronoDuration::days(90),
    )
    .await;
    let email_only = insert_person(
        &migrator_pool,
        organization_id,
        stage_b,
        Some(bob_id),
        now - ChronoDuration::days(90),
    )
    .await;
    insert_contact_method(
        &migrator_pool,
        organization_id,
        email_only,
        "email",
        "email-only@example.test",
    )
    .await;

    let all_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_a)],
            }),
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::Source(SourceClause {
                sources: vec!["present".to_string()],
            }),
            Clause::Created(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::HasReplied(BoolClause { value: true }),
            Clause::HasPhone(BoolClause { value: true }),
            Clause::HasEmail(BoolClause { value: true }),
        ],
    };
    let inactive_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Stage(StageClause {
                stage_ids: vec![StageId::new(stage_a)],
            }),
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::User(UserId::new(inactive_id))],
            }),
        ],
    };
    let unassigned_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::Unassigned],
        })],
    };
    let false_boolean_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::HasReplied(BoolClause { value: false }),
            Clause::HasPhone(BoolClause { value: false }),
            Clause::HasEmail(BoolClause { value: true }),
        ],
    };
    let no_methods_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::HasPhone(BoolClause { value: false }),
            Clause::HasEmail(BoolClause { value: false }),
        ],
    };
    let all_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "All v1 clauses",
        all_filter.clone(),
    )
    .await;
    let inactive_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Inactive assignee",
        inactive_filter.clone(),
    )
    .await;
    let unassigned_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Unassigned prefix",
        unassigned_filter.clone(),
    )
    .await;
    let false_boolean_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "False boolean axes",
        false_boolean_filter.clone(),
    )
    .await;
    let no_methods_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "No contact methods",
        no_methods_filter.clone(),
    )
    .await;

    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        all_list,
        &all_filter,
        ids([all_axes]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        inactive_list,
        &inactive_filter,
        ids([inactive_assignee]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        unassigned_list,
        &unassigned_filter,
        ids([unassigned]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        false_boolean_list,
        &false_boolean_filter,
        ids([email_only]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        no_methods_list,
        &no_methods_filter,
        ids([inactive_assignee, unassigned]),
    )
    .await;

    let items = today_items(&app_pool, organization_id, alice_id).await;
    let builtin = items
        .iter()
        .find(|item| item.person.id.as_uuid() == all_axes)
        .unwrap();
    assert_ne!(builtin.priority, TodayPriority::List);
    let prefix = items
        .iter()
        .find(|item| item.person.id.as_uuid() == unassigned)
        .unwrap();
    assert_eq!(prefix.priority, TodayPriority::List);
}

/// Source clauses observe only the latest inquiry (`received_at`, then UUID),
/// and a no-inquiry Person never matches. This exercises a retained built-in
/// and a source-only prefix in the same filter.
#[sqlx::test]
#[ignore]
async fn today_source_parity_uses_latest_source_ties_and_excludes_stale_or_absent_sources(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source latest-source parity",
        "alice@today-source-latest-source-parity.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let (stage_id, _) = first_stage_ids(&app_pool, organization_id).await;
    let now = Utc::now();

    let tie = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let (loser_id, winner_id) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let tied_at = now - ChronoDuration::days(30);
    insert_inquiry(
        &migrator_pool,
        organization_id,
        tie,
        loser_id,
        "older_source",
        tied_at,
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        tie,
        winner_id,
        "winner_source",
        tied_at,
    )
    .await;

    let source_only = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        source_only,
        Uuid::new_v4(),
        "older_source",
        now - ChronoDuration::days(15),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        source_only,
        Uuid::new_v4(),
        "winner_source",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        source_only,
        now - ChronoDuration::days(5),
    )
    .await;

    let stale = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        stale,
        Uuid::new_v4(),
        "winner_source",
        now - ChronoDuration::days(15),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        stale,
        Uuid::new_v4(),
        "newest_source",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        stale,
        now - ChronoDuration::days(5),
    )
    .await;
    let no_inquiry = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;

    let winner_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::Source(SourceClause {
            sources: vec!["winner_source".to_string()],
        })],
    };
    let absent_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::Source(SourceClause {
            sources: vec!["absent_source".to_string()],
        })],
    };
    let winner_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Latest winner source",
        winner_filter.clone(),
    )
    .await;
    let absent_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Absent source",
        absent_filter.clone(),
    )
    .await;

    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        winner_list,
        &winner_filter,
        ids([tie, source_only]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        absent_list,
        &absent_filter,
        BTreeSet::new(),
    )
    .await;
    let winner_members = source_reason_ids(
        &today_items(&app_pool, organization_id, alice_id).await,
        winner_list,
    );
    assert!(!winner_members.contains(&stale));
    assert!(!winner_members.contains(&no_inquiry));
}

/// `me` resolves for the viewer evaluating the source, while `unassigned`
/// remains literal. The shared definition is enabled separately by both
/// members and compared to their respective People query baselines.
#[sqlx::test]
#[ignore]
async fn today_source_parity_resolves_shared_me_and_unassigned_for_each_viewer(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source shared me parity",
        "alice@today-source-shared-me-parity.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(
        &migrator_pool,
        "bob@today-source-shared-me-parity.test",
        "Bob",
        PW,
    )
    .await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-shared-me-parity.test",
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
    let (stage_id, _) = first_stage_ids(&app_pool, organization_id).await;
    let created_at = Utc::now() - ChronoDuration::days(90);
    let alice_phone = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        created_at,
    )
    .await;
    let bob_phone = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(bob_id),
        created_at,
    )
    .await;
    let unassigned_phone =
        insert_person(&migrator_pool, organization_id, stage_id, None, created_at).await;
    let alice_without_phone = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        created_at,
    )
    .await;
    for (person_id, number) in [
        (alice_phone, "+15555550201"),
        (bob_phone, "+15555550202"),
        (unassigned_phone, "+15555550203"),
    ] {
        insert_contact_method(&migrator_pool, organization_id, person_id, "phone", number).await;
    }

    let filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me, Assignee::Unassigned],
            }),
            Clause::HasPhone(BoolClause { value: true }),
        ],
    };
    let shared = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Shared,
            name: "Viewer-relative phone queue".to_string(),
            filter: filter.clone(),
            sort: None,
        },
    )
    .await
    .unwrap();
    enable_existing_source(
        &app_pool,
        organization_id,
        alice_id,
        shared.list.id,
        shared.list.revision,
    )
    .await;
    enable_existing_source(
        &app_pool,
        organization_id,
        bob_id,
        shared.list.id,
        shared.list.revision,
    )
    .await;

    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        shared.list.id,
        &filter,
        ids([alice_phone, unassigned_phone]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        bob_id,
        shared.list.id,
        &filter,
        ids([bob_phone, unassigned_phone]),
    )
    .await;
    assert!(!source_reason_ids(
        &today_items(&app_pool, organization_id, alice_id).await,
        shared.list.id
    )
    .contains(&alice_without_phone));
}

/// Well-separated age facts compare the production source query with the
/// People projection. The capture helper supplies a real outbound auto-attempt,
/// while a correction and a backdated inbound fact exercise their historical
/// timestamps rather than insertion time.
#[sqlx::test]
#[ignore]
async fn today_source_parity_covers_null_ages_corrections_and_captured_facts(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source age fact parity",
        "alice@today-source-age-fact-parity.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let (stage_id, _) = first_stage_ids(&app_pool, organization_id).await;
    let now = Utc::now();

    let recent = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(2),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        recent,
        Uuid::new_v4(),
        "recent",
        now - ChronoDuration::days(2),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        recent,
        now - ChronoDuration::days(2),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        recent,
        Direction::Inbound,
        now - ChronoDuration::days(2),
        false,
    )
    .await;

    let old = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        old,
        Uuid::new_v4(),
        "old",
        now - ChronoDuration::days(90),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        old,
        now - ChronoDuration::days(90),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        old,
        Direction::Inbound,
        now - ChronoDuration::days(90),
        false,
    )
    .await;

    let never = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    let corrected = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    let corrected_at = now - ChronoDuration::days(90);
    let original =
        insert_contact_attempt(&migrator_pool, organization_id, corrected, corrected_at).await;
    insert_contact_correction(
        &migrator_pool,
        organization_id,
        corrected,
        alice_id,
        original,
        corrected_at,
    )
    .await;

    let outbound = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    let outbound_fact = capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        outbound,
        Direction::Outbound,
        now - ChronoDuration::days(2),
        false,
    )
    .await;
    let auto_attempt: (Option<Uuid>, DateTime<Utc>) = sqlx::query_as(
        "SELECT causation_id, occurred_at FROM contact_attempted \
         WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(organization_id)
    .bind(outbound)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(auto_attempt.0, Some(outbound_fact));
    assert_eq!(auto_attempt.1, now - ChronoDuration::days(2));

    let backdated = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        backdated,
        Direction::Inbound,
        now - ChronoDuration::days(90),
        true,
    )
    .await;
    let fresh_inbound = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        now - ChronoDuration::days(90),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        fresh_inbound,
        Direction::Inbound,
        now - ChronoDuration::days(2),
        false,
    )
    .await;

    let within_all = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Created(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
        ],
    };
    let never_events = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::Never,
            }),
        ],
    };
    let not_within_all = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Created(AgeClause {
                age: AgeSpec::NotWithinDays(30),
            }),
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::NotWithinDays(30),
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::NotWithinDays(30),
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::NotWithinDays(30),
            }),
        ],
    };
    let recent_contact = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastContact(AgeClause {
            age: AgeSpec::WithinDays(7),
        })],
    };
    let old_inbound = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInbound(AgeClause {
            age: AgeSpec::NotWithinDays(30),
        })],
    };
    let within_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Recent every age axis",
        within_all.clone(),
    )
    .await;
    let never_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Never event axes",
        never_events.clone(),
    )
    .await;
    let not_within_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Old every age axis",
        not_within_all.clone(),
    )
    .await;
    let recent_contact_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Recent effective contact",
        recent_contact.clone(),
    )
    .await;
    let old_inbound_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Old inbound timestamp",
        old_inbound.clone(),
    )
    .await;

    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        within_list,
        &within_all,
        ids([recent]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        never_list,
        &never_events,
        ids([never]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        not_within_list,
        &not_within_all,
        ids([old, never, corrected, backdated]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        recent_contact_list,
        &recent_contact,
        ids([recent, outbound]),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        old_inbound_list,
        &old_inbound,
        ids([old, never, corrected, outbound, backdated]),
    )
    .await;
    assert!(
        !source_reason_ids(
            &today_items(&app_pool, organization_id, alice_id).await,
            old_inbound_list
        )
        .contains(&fresh_inbound),
        "the fresh captured inbound must not be classified by capture insertion time"
    );
}

/// `within_days` is strictly later than the cutoff, while `not_within_days`
/// includes the exact cutoff. Paired test-only source and People query clocks
/// make this a common-clock parity assertion rather than a timing race.
#[sqlx::test]
#[ignore]
async fn today_source_fixed_clock_age_boundaries_are_strict_and_not_within_is_inclusive(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source fixed boundary",
        "alice@today-source-fixed-boundary.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let (stage_id, _) = first_stage_ids(&app_pool, organization_id).await;
    let fixed_now = Utc.with_ymd_and_hms(2040, 6, 1, 12, 0, 0).unwrap();
    let cutoff = fixed_now - ChronoDuration::days(7);
    let inside = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        cutoff + ChronoDuration::seconds(1),
    )
    .await;
    let exact_cutoff = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        cutoff,
    )
    .await;
    let never = insert_person(
        &migrator_pool,
        organization_id,
        stage_id,
        Some(alice_id),
        fixed_now - ChronoDuration::days(90),
    )
    .await;
    for person_id in [inside, exact_cutoff] {
        let timestamp = if person_id == inside {
            cutoff + ChronoDuration::seconds(1)
        } else {
            cutoff
        };
        insert_inquiry(
            &migrator_pool,
            organization_id,
            person_id,
            Uuid::new_v4(),
            "boundary",
            timestamp,
        )
        .await;
        insert_contact_attempt(&migrator_pool, organization_id, person_id, timestamp).await;
        capture_fact(
            &app_pool,
            &migrator_pool,
            organization_id,
            alice_id,
            person_id,
            Direction::Inbound,
            timestamp,
            false,
        )
        .await;
    }

    let within = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Created(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::WithinDays(7),
            }),
        ],
    };
    let not_within = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::Created(AgeClause {
                age: AgeSpec::NotWithinDays(7),
            }),
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::NotWithinDays(7),
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::NotWithinDays(7),
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::NotWithinDays(7),
            }),
        ],
    };
    let never_events = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::LastInquiry(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::Never,
            }),
            Clause::LastInbound(AgeClause {
                age: AgeSpec::Never,
            }),
        ],
    };
    let within_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Strictly inside boundary",
        within.clone(),
    )
    .await;
    let not_within_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Inclusive boundary",
        not_within.clone(),
    )
    .await;
    let never_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "No dated facts",
        never_events.clone(),
    )
    .await;

    assert_fixed_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        fixed_now,
        within_list,
        &within,
        ids([inside]),
    )
    .await;
    assert_fixed_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        fixed_now,
        not_within_list,
        &not_within,
        ids([exact_cutoff, never]),
    )
    .await;
    assert_fixed_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        fixed_now,
        never_list,
        &never_events,
        ids([never]),
    )
    .await;
}

// --- 011d: the three derived clause kinds (docs/specs/SLICE_011d.md §2, §9.1) ---

#[allow(clippy::too_many_arguments)]
async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: DateTime<Utc>,
    corrected: bool,
) -> Uuid {
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
             correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES \
            ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
             'scripted', 'derived-axis-parity', $7, $7)",
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

    let root_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contact_attempted \
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
             causation_id, person_id, channel, outcome) \
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, 'call', 'reached') \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(ended_at)
    .bind(Uuid::new_v4())
    .bind(call_id)
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap();

    if corrected {
        insert_contact_correction(
            pool,
            organization_id,
            person_id,
            caller_user_id,
            root_id,
            ended_at,
        )
        .await;
    }

    call_id
}

/// Per-axis present-true/present-false/absent parity (spec §9.1) for the
/// three 011d derived clause kinds, across the People-filter statement AND
/// both Today source statements (`assert_reason_parity` exercises
/// `filtered_summaries` and the full Today query, which internally uses
/// `source_membership`/`source_candidates`) — on one common-clock fixture
/// with corrections, a zero-inquiry Person, outbound-only correspondence,
/// calls by two callers and a corrected call attempt.
#[sqlx::test]
#[ignore]
async fn derived_axis_parity_awaiting_response_client_replied_and_call_outcome(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "011d derived axis parity",
        "alice@d011-derived-axis.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(&migrator_pool, "bob@d011-derived-axis.test", "Bob", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let (stage_a, _stage_b) = first_stage_ids(&app_pool, organization_id).await;
    let now = Utc::now();

    // --- awaiting_response ---------------------------------------------

    // true: an inquiry after the (only) contact attempt.
    let awaiting_true = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        awaiting_true,
        now - ChronoDuration::days(4),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        awaiting_true,
        Uuid::new_v4(),
        "zillow",
        now - ChronoDuration::days(3),
    )
    .await;

    // false: the (only) inquiry is answered by a LATER contact attempt.
    let awaiting_false = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        organization_id,
        awaiting_false,
        Uuid::new_v4(),
        "zillow",
        now - ChronoDuration::days(4),
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        awaiting_false,
        now - ChronoDuration::days(3),
    )
    .await;

    // zero-inquiry Person: never matches awaiting_response:true (no
    // inquiry exists to be "after" anything), and DOES match
    // awaiting_response:false.
    let zero_inquiry = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;

    let awaiting_response_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingResponse(BoolClause { value: true })],
    };
    let awaiting_response_true_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Awaiting response true",
        awaiting_response_true.clone(),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        awaiting_response_true_list,
        &awaiting_response_true,
        ids([awaiting_true]),
    )
    .await;

    let awaiting_response_false = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingResponse(BoolClause { value: false })],
    };
    let awaiting_response_false_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Awaiting response false",
        awaiting_response_false.clone(),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        awaiting_response_false_list,
        &awaiting_response_false,
        ids([awaiting_false, zero_inquiry]),
    )
    .await;

    // absent: unaffected by the new clause — the ordinary empty filter
    // matches everyone in this fixture (byte-identical to pre-011d).
    let absent = FilterDefinition {
        version: 1,
        clauses: vec![],
    };
    let all = filtered_ids(&app_pool, organization_id, alice_id, &absent).await;
    assert!(all.contains(&awaiting_true));
    assert!(all.contains(&awaiting_false));
    assert!(all.contains(&zero_inquiry));

    // --- client_replied_unanswered --------------------------------------

    // true: latest inbound is later than the latest outbound AND the last
    // contact attempt.
    let replied_true = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        replied_true,
        Direction::Outbound,
        now - ChronoDuration::days(4),
        false,
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        replied_true,
        Direction::Inbound,
        now - ChronoDuration::days(3),
        false,
    )
    .await;

    // false: outbound-only correspondence (no inbound at all).
    let replied_false_outbound_only = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        replied_false_outbound_only,
        Direction::Outbound,
        now - ChronoDuration::days(3),
        false,
    )
    .await;

    // false: the inbound reply was already answered by a later contact
    // attempt.
    let replied_false_answered = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    capture_fact(
        &app_pool,
        &migrator_pool,
        organization_id,
        alice_id,
        replied_false_answered,
        Direction::Inbound,
        now - ChronoDuration::days(4),
        false,
    )
    .await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        replied_false_answered,
        now - ChronoDuration::days(3),
    )
    .await;

    let client_replied_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::ClientRepliedUnanswered(BoolClause { value: true })],
    };
    let client_replied_true_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Client replied unanswered true",
        client_replied_true.clone(),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        client_replied_true_list,
        &client_replied_true,
        ids([replied_true]),
    )
    .await;

    let client_replied_false = FilterDefinition {
        version: 1,
        clauses: vec![Clause::ClientRepliedUnanswered(BoolClause { value: false })],
    };
    let client_replied_false_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Client replied unanswered false",
        client_replied_false.clone(),
    )
    .await;
    let false_ids = filtered_ids(&app_pool, organization_id, alice_id, &client_replied_false).await;
    assert!(false_ids.contains(&replied_false_outbound_only));
    assert!(false_ids.contains(&replied_false_answered));
    assert!(!false_ids.contains(&replied_true));
    let false_source = source_reason_ids(
        &today_items(&app_pool, organization_id, alice_id).await,
        client_replied_false_list,
    );
    assert_eq!(false_source, false_ids, "Today source must agree with People filter");

    // --- awaiting_call_outcome (viewer-relative: the caller is the viewer) ---

    // true for Alice: Alice called, ended, root attempt uncorrected.
    let call_true_for_alice = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_true_for_alice,
        alice_id,
        now - ChronoDuration::hours(2),
        false,
    )
    .await;

    // false for Alice: Bob called (a different caller) — must not appear
    // for Alice's viewer-relative axis, even though a qualifying call
    // exists in the Organization.
    let call_by_bob = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_by_bob,
        bob_id,
        now - ChronoDuration::hours(2),
        false,
    )
    .await;

    // false for Alice: Alice's call outcome was corrected (no longer
    // "needs an outcome").
    let call_corrected = insert_person(
        &migrator_pool,
        organization_id,
        stage_a,
        Some(alice_id),
        now - ChronoDuration::days(5),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_corrected,
        alice_id,
        now - ChronoDuration::hours(2),
        true,
    )
    .await;

    let call_outcome_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
    };
    let call_outcome_true_list = create_and_enable_source(
        &app_pool,
        organization_id,
        alice_id,
        SavedListScope::Personal,
        "Awaiting call outcome true (alice)",
        call_outcome_true.clone(),
    )
    .await;
    assert_reason_parity(
        &app_pool,
        organization_id,
        alice_id,
        call_outcome_true_list,
        &call_outcome_true,
        ids([call_true_for_alice]),
    )
    .await;

    // The SAME filter evaluated as Bob's own viewer-relative axis instead
    // matches Bob's call, proving the bound viewer id (never the wire) is
    // what resolves the axis.
    let bob_ids = filtered_ids(&app_pool, organization_id, bob_id, &call_outcome_true).await;
    assert_eq!(bob_ids, ids([call_by_bob]));

    let call_outcome_false = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: false })],
    };
    let alice_false_ids =
        filtered_ids(&app_pool, organization_id, alice_id, &call_outcome_false).await;
    assert!(!alice_false_ids.contains(&call_true_for_alice));
    assert!(alice_false_ids.contains(&call_by_bob));
    assert!(alice_false_ids.contains(&call_corrected));
}

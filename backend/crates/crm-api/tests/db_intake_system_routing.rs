//! DB-backed tests for Slice 007c's system-actor routing matrix
//! (docs/specs/SLICE_007c.md §11, acceptance criteria 3-9, 14): the
//! primary proof is calling `commands::receive_inquiry` directly with
//! `IntakeActor::System` — nothing in the HTTP surface triggers this path
//! yet (§4). Run only via ./scripts/check-db.
mod common;

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::commands::{self, ReceiveInquiry, ReceiveInquiryOutcome, RoutingStrategy};
use crm_api::domain::envelope::{ActorKind, Origin};
use crm_api::domain::inquiry::parse::Source;
use crm_api::domain::intake::{IntakeActor, IntakeRoutingMode};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "pw";

/// Slice 008 (D-041) note: `intake_routing_mode` dispatch replaced the old
/// implicit "an assignee is configured => organization_default" behavior
/// this file's 007c criteria were written against — so this fixture
/// helper now sets the mode alongside the assignee, mirroring the OLD
/// two-state implicit behavior exactly (`Some` => `default_assignee`
/// mode, `None` => `unassigned` mode). Every downstream assertion in this
/// file (fact/routing/Today pins) is unchanged; only this setup helper
/// adapted to the new schema.
async fn set_default(pool: &PgPool, org_id: Uuid, user_id: Option<Uuid>) {
    let mode = if user_id.is_some() {
        IntakeRoutingMode::DefaultAssignee
    } else {
        IntakeRoutingMode::Unassigned
    };
    admin_queries::update_intake_routing_settings(
        &mut pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
        mode,
        user_id.map(UserId::new),
    )
    .await
    .unwrap();
}

fn lead(email: &str) -> Value {
    json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "email": email,
        "message": "Interested in the listing",
    })
}

async fn system_intake(
    pool: &PgPool,
    org_id: Uuid,
    payload: &Value,
    publisher: &Publisher,
) -> ReceiveInquiryOutcome {
    let key = common::test_config().raw_payload_key;
    let actor = IntakeActor::System {
        on_behalf_of_user_id: None,
        organization_id: OrganizationId::new(org_id),
        origin: Origin::Cli,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let cmd = ReceiveInquiry {
        source: Source::parse("website").unwrap(),
        payload: serde_json::to_vec(payload).unwrap(),
        assign_to_user_id: None,
        received_at: chrono::Utc::now(),
    };
    commands::receive_inquiry(pool, &key, publisher, &actor, cmd)
        .await
        .unwrap()
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected Publisher::Recording");
    };
    recorded.lock().await.clone()
}

/// Criteria 3, 4, 14: a default set and active — `organization_default`,
/// all facts share one correlation id and `actor_kind='system'` /
/// `actor_user_id NULL` / `origin='cli'`, the Person lands on the
/// default's Today and nobody else's, and exactly one realtime event
/// publishes.
#[sqlx::test]
#[ignore]
async fn default_set_and_active_routes_organization_default_and_lands_on_their_today(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let bob = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    let alice = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    common::add_membership(&migrator_pool, org_id, bob).await;
    common::add_membership(&migrator_pool, org_id, alice).await;
    set_default(&migrator_pool, org_id, Some(bob)).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let outcome = system_intake(&app_pool, org_id, &lead("ada@example.com"), &publisher).await;

    let ReceiveInquiryOutcome::Resolved {
        person_id,
        person_created,
        routing_strategy,
        assigned_user_id,
        duplicate,
        inquiry_id,
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert!(person_created);
    assert!(!duplicate);
    assert_eq!(routing_strategy, RoutingStrategy::OrganizationDefault);
    assert_eq!(assigned_user_id, Some(UserId::new(bob)));

    // Every fact shares one correlation id and the system-actor shape.
    for table in [
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
    ] {
        let row: (String, Option<Uuid>, String, Uuid) = sqlx::query_as(&format!(
            "SELECT actor_kind, actor_user_id, origin, correlation_id FROM {table} WHERE person_id = $1"
        ))
        .bind(person_id.0)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(row.0, "system", "{table}.actor_kind");
        assert_eq!(row.1, None, "{table}.actor_user_id");
        assert_eq!(row.2, "cli", "{table}.origin");
    }
    let correlations: Vec<Uuid> = sqlx::query_scalar(
        "SELECT correlation_id FROM inquiry_received WHERE person_id = $1
         UNION SELECT correlation_id FROM routing_decision WHERE person_id = $1
         UNION SELECT correlation_id FROM assignment_changed WHERE person_id = $1
         UNION SELECT correlation_id FROM stage_changed WHERE person_id = $1",
    )
    .bind(person_id.0)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(correlations.len(), 1, "one shared correlation id");

    let (routing_decision_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM routing_decision WHERE inquiry_id = $1")
            .bind(inquiry_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let (from_user_id, to_user_id, causation_id): (Option<Uuid>, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT from_user_id, to_user_id, causation_id FROM assignment_changed WHERE person_id = $1",
        )
        .bind(person_id.0)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(from_user_id, None);
    assert_eq!(to_user_id, Some(bob));
    assert_eq!(causation_id, Some(routing_decision_id));

    // Today: bob only.
    let router = common::build_router(&migrator_pool).await;
    let bob_cookie = common::login_cookie(&router, "bob@acme.test", PW).await;
    let bob_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &bob_cookie).await).await;
    let bob_items = bob_today["items"].as_array().unwrap();
    assert_eq!(bob_items.len(), 1);
    assert_eq!(bob_items[0]["person"]["id"], person_id.to_string());

    let alice_cookie = common::login_cookie(&router, "alice@acme.test", PW).await;
    let alice_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(alice_today["items"].as_array().unwrap().len(), 0);

    // Exactly one realtime event, ids-only.
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["type"], "person.changed");
    assert_eq!(events[0].1["data"]["person_id"], person_id.to_string());
    assert_eq!(events[0].1["data"]["change"], "inquiry_received");
}

/// Criterion 5: no default set — `unassigned`, `person.assigned_user_id`
/// NULL, no `assignment_changed` fact, visible in People, on nobody's
/// Today.
#[sqlx::test]
#[ignore]
async fn no_default_set_routes_unassigned_with_no_assignment_fact(migrator_pool: PgPool) {
    let (org_id, _alice) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let outcome = system_intake(
        &app_pool,
        org_id,
        &lead("grace@example.com"),
        &Publisher::recording(),
    )
    .await;

    let ReceiveInquiryOutcome::Resolved {
        person_id,
        routing_strategy,
        assigned_user_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(routing_strategy, RoutingStrategy::Unassigned);
    assert_eq!(assigned_user_id, None);

    let (db_assigned,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assigned_user_id FROM person WHERE id = $1")
            .bind(person_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(db_assigned, None);

    let (assignee_user_id,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assignee_user_id FROM routing_decision WHERE person_id = $1")
            .bind(person_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assignee_user_id, None);

    let (assignment_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assignment_count, 0, "NULL->NULL assignment fact is noise");

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", PW).await;
    let people =
        common::body_json(common::get_with_cookie(&router, "/api/people", &cookie).await).await;
    assert!(people["people"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == person_id.to_string()));
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &cookie).await).await;
    assert_eq!(today["items"].as_array().unwrap().len(), 0);
}

/// Criterion 6: a default set, then that member deactivated — the next
/// system intake routes `unassigned` and nothing errors; the setting
/// itself is retained (not cleared as a side effect of deactivation).
#[sqlx::test]
#[ignore]
async fn default_member_deactivated_routes_unassigned_and_setting_is_retained(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let bob = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    common::add_membership(&migrator_pool, org_id, bob).await;
    set_default(&migrator_pool, org_id, Some(bob)).await;

    sqlx::query("UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(bob)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let outcome = system_intake(
        &app_pool,
        org_id,
        &lead("frank@example.com"),
        &Publisher::recording(),
    )
    .await;

    let ReceiveInquiryOutcome::Resolved {
        routing_strategy,
        assigned_user_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(routing_strategy, RoutingStrategy::Unassigned);
    assert_eq!(assigned_user_id, None);

    let stored = admin_queries::intake_default_assignee_user_id(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
    )
    .await
    .unwrap();
    assert_eq!(
        stored,
        Some(UserId::new(bob)),
        "setting is retained across deactivation"
    );
}

/// Criterion 7 (kept_existing leg): a system-actor intake matching an
/// already-assigned Person keeps the assignee and writes no new
/// assignment fact.
#[sqlx::test]
#[ignore]
async fn matching_already_assigned_person_keeps_existing(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let bob = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    common::add_membership(&migrator_pool, org_id, bob).await;
    set_default(&migrator_pool, org_id, Some(bob)).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let first = system_intake(
        &app_pool,
        org_id,
        &lead("grace@example.com"),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved { person_id, .. } = first else {
        panic!("expected Resolved");
    };

    let second = system_intake(
        &app_pool,
        org_id,
        &json!({
            "first_name": "Grace",
            "last_name": "Hopper",
            "email": "grace@example.com",
            "message": "Following up",
        }),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        person_id: second_person_id,
        person_created,
        routing_strategy,
        assigned_user_id,
        ..
    } = second
    else {
        panic!("expected Resolved");
    };
    assert_eq!(second_person_id, person_id, "same Person matched by email");
    assert!(!person_created);
    assert_eq!(routing_strategy, RoutingStrategy::KeptExisting);
    assert_eq!(assigned_user_id, Some(UserId::new(bob)));

    let (assignment_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        assignment_count, 1,
        "no new assignment fact on kept_existing"
    );
}

/// Criterion 7 (matched-but-unassigned leg): a system-actor intake
/// matching an existing unassigned Person applies the (now-set) default
/// with exactly one NULL->default `assignment_changed`.
#[sqlx::test]
#[ignore]
async fn matching_existing_unassigned_person_applies_the_default(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let bob = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    common::add_membership(&migrator_pool, org_id, bob).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let first = system_intake(
        &app_pool,
        org_id,
        &lead("carol@example.com"),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        person_id,
        routing_strategy: first_strategy,
        ..
    } = first
    else {
        panic!("expected Resolved");
    };
    assert_eq!(first_strategy, RoutingStrategy::Unassigned);

    set_default(&migrator_pool, org_id, Some(bob)).await;

    let second = system_intake(
        &app_pool,
        org_id,
        &json!({
            "first_name": "Carol",
            "last_name": "Danvers",
            "email": "carol@example.com",
            "message": "Still interested",
        }),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        person_id: second_person_id,
        routing_strategy,
        assigned_user_id,
        ..
    } = second
    else {
        panic!("expected Resolved");
    };
    assert_eq!(second_person_id, person_id);
    assert_eq!(routing_strategy, RoutingStrategy::OrganizationDefault);
    assert_eq!(assigned_user_id, Some(UserId::new(bob)));

    let assignments: Vec<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT from_user_id, to_user_id FROM assignment_changed WHERE person_id = $1 ORDER BY recorded_at",
    )
    .bind(person_id.0)
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(assignments, vec![(None, Some(bob))]);
}

/// Criterion 7 (§7 pin): "a system-actor intake into org A writes zero
/// rows in org B" — the routing matrix's new async DB lookup
/// (`active_intake_default_assignee`) is scoped by the actor's
/// server-supplied `organization_id`, never client input; this is the
/// only DB-backed test that runs the System-actor path against two
/// Organizations at once, so a copy/paste parameter-order regression in
/// that lookup would be caught here even though every other
/// system-routing test uses a single Organization.
#[sqlx::test]
#[ignore]
async fn system_actor_intake_into_org_a_writes_zero_rows_in_org_b(migrator_pool: PgPool) {
    let org_a = common::create_org(&migrator_pool, "Acme Realty").await;
    let org_b = common::create_org(&migrator_pool, "Best Realty").await;
    common::seed_stages(&migrator_pool, org_a).await;
    common::seed_stages(&migrator_pool, org_b).await;
    let alice = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let bob = common::create_user(&migrator_pool, "bob@best.test", "Bob", PW).await;
    common::add_membership(&migrator_pool, org_a, alice).await;
    common::add_membership(&migrator_pool, org_b, bob).await;
    set_default(&migrator_pool, org_a, Some(alice)).await;
    set_default(&migrator_pool, org_b, Some(bob)).await;

    let (org_b_people_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(org_b_people_before, 0);

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let outcome = system_intake(
        &app_pool,
        org_a,
        &lead("isolation-check@example.com"),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        person_id,
        assigned_user_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(
        assigned_user_id,
        Some(UserId::new(alice)),
        "routed within org A only"
    );

    for (table, count_col) in [
        ("person", "organization_id"),
        ("inquiry", "organization_id"),
        ("inquiry_received", "organization_id"),
        ("routing_decision", "organization_id"),
        ("assignment_changed", "organization_id"),
        ("stage_changed", "organization_id"),
        ("raw_payload", "organization_id"),
    ] {
        let (count,): (i64,) = sqlx::query_as(&format!(
            "SELECT count(*) FROM {table} WHERE {count_col} = $1"
        ))
        .bind(org_b)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "{table}: org B must have zero rows from org A's system intake"
        );
    }

    // Org B's own configured default is untouched, and its member's Today
    // stays empty — the cross-org write did not leak into org B's state.
    let org_b_default = admin_queries::intake_default_assignee_user_id(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_b),
    )
    .await
    .unwrap();
    assert_eq!(org_b_default, Some(UserId::new(bob)));

    let router = common::build_router(&migrator_pool).await;
    let bob_cookie = common::login_cookie(&router, "bob@best.test", PW).await;
    let bob_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &bob_cookie).await).await;
    assert_eq!(bob_today["items"].as_array().unwrap().len(), 0);

    // Sanity: the Person genuinely exists, just scoped to org A.
    let (person_org,): (Uuid,) = sqlx::query_as("SELECT organization_id FROM person WHERE id = $1")
        .bind(person_id.0)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(person_org, org_a);
}

/// Criterion 9: re-POSTing a system-routed payload's exact bytes via
/// `POST /api/inquiries` (a User actor, over HTTP) returns
/// `duplicate: true` with `routing_strategy: "organization_default"` —
/// pins the `RoutingStrategy::from_str` extension; no 500.
#[sqlx::test]
#[ignore]
async fn duplicate_repost_of_a_system_routed_payload_reports_its_strategy(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let bob = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    let alice = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    common::add_membership(&migrator_pool, org_id, bob).await;
    common::add_membership(&migrator_pool, org_id, alice).await;
    set_default(&migrator_pool, org_id, Some(bob)).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let payload = lead("dan@example.com");
    let outcome = system_intake(&app_pool, org_id, &payload, &Publisher::recording()).await;
    assert!(matches!(outcome, ReceiveInquiryOutcome::Resolved { .. }));

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", PW).await;
    let resp = common::post_inquiry(&router, &cookie, "website", payload, None).await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["duplicate"], true);
    assert_eq!(body["routing_strategy"], "organization_default");
    assert_eq!(body["assigned_user_id"], bob.to_string());
}

/// Criterion 8 regression pin: the actor-kind CHECK stays a hard
/// constraint — inserting a `system` fact with a non-NULL actor (or vice
/// versa) via the shared envelope machinery is unrepresentable. Exercised
/// indirectly above (every system fact's `actor_user_id` reads NULL); this
/// asserts `ActorKind::System.as_str()` is what the tables actually store.
#[test]
fn actor_kind_as_str_matches_the_stored_value() {
    assert_eq!(ActorKind::System.as_str(), "system");
    assert_eq!(ActorKind::User.as_str(), "user");
}

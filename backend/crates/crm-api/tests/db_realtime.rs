//! DB-backed publisher-contract tests (docs/specs/SLICE_003.md §13,
//! acceptance criteria 5 and 6's DB half). Run only via ./scripts/check-db.
//! Uses `Publisher::recording()` to pin the exact §6 envelope each command
//! publishes, and a `Publisher::Centrifugo` pointed at a closed loopback
//! port to prove a publish failure never fails the command.
mod common;

use std::time::Duration;

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
use crm_api::realtime::{CentrifugoTransport, Publisher};

/// Every recorded `(channel, data)` pair as JSON, for assertions that don't
/// care about the exact index.
async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded) = publisher else {
        panic!("expected Publisher::Recording");
    };
    recorded.lock().await.clone()
}

fn expected_person_changed(
    organization_id: Uuid,
    occurred_at: DateTime<Utc>,
    correlation_id: Uuid,
    person_id: Uuid,
    change: &str,
) -> Value {
    json!({
        "v": 1,
        "type": "person.changed",
        "organization_id": organization_id,
        "occurred_at": occurred_at,
        "correlation_id": correlation_id,
        "data": { "person_id": person_id, "change": change },
    })
}

/// Criterion 5: intake of a brand-new Person publishes exactly one
/// `person.changed{inquiry_received}`, with `occurred_at` equal to the
/// `inquiry_received` fact's own `occurred_at` and `correlation_id` equal
/// to that fact's `correlation_id`.
#[sqlx::test]
#[ignore]
async fn intake_new_person_publishes_exactly_one_event(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let resp = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "new-lead@example.com" }),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = common::body_json(resp).await;
    let person_id: Uuid = body["person_id"].as_str().unwrap().parse().unwrap();

    let (fact_occurred_at, fact_correlation_id): (DateTime<Utc>, Uuid) = sqlx::query_as(
        "SELECT occurred_at, correlation_id FROM inquiry_received WHERE person_id = $1",
    )
    .bind(person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1, "exactly one event must be published");
    assert_eq!(events[0].0, format!("org:{org_id}"));
    assert_eq!(
        events[0].1,
        expected_person_changed(
            org_id,
            fact_occurred_at,
            fact_correlation_id,
            person_id,
            "inquiry_received"
        )
    );
}

/// Criterion 5: a matched-Person intake (kept_existing routing) writes two
/// facts (`inquiry_received`, `routing_decision`) but publishes exactly
/// one event.
#[sqlx::test]
#[ignore]
async fn intake_matched_person_publishes_exactly_one_event_despite_two_facts(
    migrator_pool: PgPool,
) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let first = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "repeat-lead@example.com" }),
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let person_id: Uuid = common::body_json(first).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(recorded(&publisher).await.len(), 1);

    let second = common::post_inquiry(
        &router,
        &cookie,
        "referral",
        json!({ "email": "repeat-lead@example.com" }),
        None,
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    assert_eq!(
        common::body_json(second).await["routing_strategy"],
        "kept_existing"
    );

    let (fact_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry_received WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(fact_count, 2, "two inquiry_received facts by now");

    let events = recorded(&publisher).await;
    assert_eq!(
        events.len(),
        2,
        "one event per intake execution, not per fact"
    );
    assert_eq!(events[1].1["data"]["change"], "inquiry_received");
}

/// Criterion 5: an unresolved intake publishes `intake.unresolved_changed`
/// with `occurred_at` equal to the stored `raw_payload.received_at`.
#[sqlx::test]
#[ignore]
async fn intake_unresolved_publishes_with_raw_payload_received_at(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let resp = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "message": "no contact info here" }),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let raw_payload_id: Uuid = common::body_json(resp).await["raw_payload_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let (received_at,): (DateTime<Utc>,) =
        sqlx::query_as("SELECT received_at FROM raw_payload WHERE id = $1")
            .bind(raw_payload_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, format!("org:{org_id}"));
    assert_eq!(events[0].1["type"], "intake.unresolved_changed");
    assert_eq!(
        events[0].1["data"]["raw_payload_id"],
        raw_payload_id.to_string()
    );
    assert_eq!(
        events[0].1["occurred_at"],
        serde_json::to_value(received_at).unwrap()
    );
}

/// Criterion 5: a duplicate delivery publishes nothing additional.
#[sqlx::test]
#[ignore]
async fn duplicate_intake_publishes_nothing_additional(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let payload = json!({ "email": "dup-lead@example.com" });
    let first = common::post_inquiry(&router, &cookie, "zillow", payload.clone(), None).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(recorded(&publisher).await.len(), 1);

    let second = common::post_inquiry(&router, &cookie, "zillow", payload, None).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(common::body_json(second).await["duplicate"], true);
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "a duplicate delivery must publish nothing additional"
    );
}

/// Criterion 5: `assign_person`/`change_person_stage` publish an event
/// only when the fact was actually written; a no-op (unchanged) call
/// publishes nothing. `log_contact_attempt` always publishes (it always
/// writes a fact).
#[sqlx::test]
#[ignore]
async fn assign_stage_and_contact_commands_publish_only_when_changed(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id = common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    common::add_membership(&migrator_pool, org_id, carol_id).await;

    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let intake = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "assignable@example.com" }),
        None,
    )
    .await;
    let person_id: Uuid = common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "intake published one event"
    );

    // Reassign to Carol: changed -> a new event.
    let reassign = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/assignment"),
        &cookie,
        json!({ "assigned_user_id": carol_id }),
    )
    .await;
    assert_eq!(common::body_json(reassign).await["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].1["data"]["change"], "assignment_changed");

    // Reassign to the same person again: unchanged -> no new event.
    let noop_reassign = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/assignment"),
        &cookie,
        json!({ "assigned_user_id": carol_id }),
    )
    .await;
    assert_eq!(common::body_json(noop_reassign).await["changed"], false);
    assert_eq!(
        recorded(&publisher).await.len(),
        2,
        "unchanged assignment must not publish"
    );

    // Change stage: changed -> a new event.
    let stages_resp = common::get_with_cookie(&router, "/api/stages", &cookie).await;
    let stages = common::body_json(stages_resp).await;
    let stages_arr = stages["stages"].as_array().unwrap();
    let second_stage_id = stages_arr[1]["id"].as_str().unwrap();

    let stage_change = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/stage"),
        &cookie,
        json!({ "stage_id": second_stage_id }),
    )
    .await;
    assert_eq!(common::body_json(stage_change).await["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 3);
    assert_eq!(events[2].1["data"]["change"], "stage_changed");

    // Same stage again: unchanged -> no new event.
    let noop_stage = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/stage"),
        &cookie,
        json!({ "stage_id": second_stage_id }),
    )
    .await;
    assert_eq!(common::body_json(noop_stage).await["changed"], false);
    assert_eq!(
        recorded(&publisher).await.len(),
        3,
        "unchanged stage must not publish"
    );

    // Log a contact attempt: always writes a fact -> always publishes.
    let contact = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &cookie,
        json!({ "channel": "call", "outcome": "reached" }),
    )
    .await;
    assert_eq!(contact.status(), StatusCode::CREATED);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 4);
    assert_eq!(events[3].1["data"]["change"], "contact_attempted");
    assert_eq!(events[3].1["data"]["person_id"], person_id.to_string());

    let _ = alice_id; // used only to build the fixture Organization/member
}

/// Criterion 5: invalid assignee (422), invalid stage (422), and another
/// Organization's Person (404) publish nothing.
#[sqlx::test]
#[ignore]
async fn rejected_commands_publish_nothing(migrator_pool: PgPool) {
    let (org_a, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, bob_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        "pw",
    )
    .await;

    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let bob_cookie = common::login_cookie(&router, "bob@best.test", "pw").await;

    let intake = common::post_inquiry(
        &router,
        &alice_cookie,
        "zillow",
        json!({ "email": "target@example.com" }),
        None,
    )
    .await;
    let a_person_id: Uuid = common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(recorded(&publisher).await.len(), 1);

    // Invalid assignee (Bob is not in Acme).
    let invalid_assignee = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/assignment"),
        &alice_cookie,
        json!({ "assigned_user_id": bob_id }),
    )
    .await;
    assert_eq!(invalid_assignee.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(recorded(&publisher).await.len(), 1);

    // Invalid stage (Best Realty's stage on an Acme Person).
    let (best_stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let invalid_stage = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/stage"),
        &alice_cookie,
        json!({ "stage_id": best_stage_id }),
    )
    .await;
    assert_eq!(invalid_stage.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(recorded(&publisher).await.len(), 1);

    // Other-Organization Person on every one of the three commands.
    let cross_assign = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/assignment"),
        &bob_cookie,
        json!({ "assigned_user_id": bob_id }),
    )
    .await;
    assert_eq!(cross_assign.status(), StatusCode::NOT_FOUND);
    let cross_stage = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/stage"),
        &bob_cookie,
        json!({ "stage_id": best_stage_id }),
    )
    .await;
    assert_eq!(cross_stage.status(), StatusCode::NOT_FOUND);
    let cross_contact = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/contact-attempts"),
        &bob_cookie,
        json!({ "channel": "call", "outcome": "reached" }),
    )
    .await;
    assert_eq!(cross_contact.status(), StatusCode::NOT_FOUND);

    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "no rejected command may publish anything"
    );
    let _ = org_a;
}

/// Criterion 5: `IntakeBusy` (the per-Organization advisory-lock retry
/// budget exhausted) publishes nothing — the raw_payload row stays
/// `pending`, untouched by any of the publish call sites, which only run
/// after a commit that never happens on this path.
#[sqlx::test]
#[ignore]
async fn intake_busy_publishes_nothing(migrator_pool: PgPool) {
    let (org_id, _user_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Busy Org",
        "busy@busy.test",
        "Busy",
        "pw",
    )
    .await;

    let hold_duration = ADVISORY_LOCK_BUDGET + Duration::from_secs(2);
    let lock_key_text = org_id.to_string();
    let external_pool = migrator_pool.clone();
    let hold_task = tokio::spawn(async move {
        let mut tx = external_pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0))")
            .bind(&lock_key_text)
            .execute(&mut *tx)
            .await
            .unwrap();
        tokio::time::sleep(hold_duration).await;
        let _ = tx.rollback().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "busy@busy.test", "pw").await;

    let resp = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "blocked@example.com" }),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(common::body_json(resp).await["error"], "intake_busy");
    assert_eq!(recorded(&publisher).await.len(), 0);

    hold_task.abort();
}

/// Criterion 6 (DB half): a publish failure never fails the command — the
/// command's rows are committed and it returns success even though the
/// `Publisher::Centrifugo` transport points at a closed loopback port.
#[sqlx::test]
#[ignore]
async fn publish_failure_does_not_fail_the_command(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // nothing listening: connection refused

    let publisher = Publisher::Centrifugo(CentrifugoTransport::for_tests(
        format!("http://{addr}"),
        "unused-test-key",
    ));
    let router = common::build_router_with_publisher(&migrator_pool, publisher).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let resp = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "still-works@example.com" }),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let person_id: Uuid = common::body_json(resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM person WHERE id = $1")
        .bind(person_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "the command's rows must still be committed");
}

/// Coverage gap flagged by independent adversarial review: no test fired
/// two commands concurrently against the *same* Person. `lock_person`'s
/// `SELECT ... FOR UPDATE` (reused from Slice 002) should serialize the
/// two `LogContactAttempt` calls below so each writes its own fact and
/// publishes its own event — never a lost or duplicated one. `tokio::join!`
/// mirrors the existing race pattern in tests/db_intake.rs.
#[sqlx::test]
#[ignore]
async fn two_concurrent_log_contact_attempts_on_same_person_both_write_and_publish(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let intake = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "race-target@example.com" }),
        None,
    )
    .await;
    assert_eq!(intake.status(), StatusCode::CREATED);
    let person_id: Uuid = common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let events_before_race = recorded(&publisher).await.len();
    assert_eq!(events_before_race, 1, "intake published its one event");

    let contact_attempts_uri = format!("/api/people/{person_id}/contact-attempts");
    let fut_a = common::post_json_with_cookie(
        &router,
        &contact_attempts_uri,
        &cookie,
        json!({ "channel": "call", "outcome": "no_answer" }),
    );
    let fut_b = common::post_json_with_cookie(
        &router,
        &contact_attempts_uri,
        &cookie,
        json!({ "channel": "email", "outcome": "sent" }),
    );
    let (resp_a, resp_b) = tokio::join!(fut_a, fut_b);

    assert_eq!(
        resp_a.status(),
        StatusCode::CREATED,
        "both concurrent calls must complete"
    );
    assert_eq!(
        resp_b.status(),
        StatusCode::CREATED,
        "both concurrent calls must complete"
    );

    let (fact_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        fact_count, 2,
        "exactly one fact row per command execution, none lost or merged"
    );

    let events = recorded(&publisher).await;
    assert_eq!(
        events.len(),
        events_before_race + 2,
        "exactly one person.changed per command execution, none lost or duplicated"
    );
    let new_events = &events[events_before_race..];
    let expected_channel = format!("org:{org_id}");
    for (channel, data) in new_events {
        assert_eq!(channel, &expected_channel);
        assert_eq!(data["type"], "person.changed");
        assert_eq!(data["data"]["person_id"], person_id.to_string());
        assert_eq!(data["data"]["change"], "contact_attempted");
    }
}

/// Same coverage gap, the mixed-command leg: `LogContactAttempt` racing
/// `AssignPerson` on the same Person. Both commands take the same
/// `lock_person` row lock, so one must wait for the other's transaction to
/// commit — each still writes exactly its own fact and publishes exactly
/// its own event.
#[sqlx::test]
#[ignore]
async fn log_contact_attempt_racing_assign_person_on_same_person_both_write_and_publish(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id = common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    common::add_membership(&migrator_pool, org_id, carol_id).await;

    let publisher = Publisher::recording();
    let router = common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    // New-Person intake assigns to the actor (Alice) and writes its own
    // assignment_changed fact (NULL -> Alice) plus one person.changed
    // event — both are the race's baseline, not part of what we're
    // asserting on.
    let intake = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "mixed-race-target@example.com" }),
        None,
    )
    .await;
    assert_eq!(intake.status(), StatusCode::CREATED);
    let person_id: Uuid = common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let events_before_race = recorded(&publisher).await.len();

    let (contact_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let (assignment_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let contact_attempts_uri = format!("/api/people/{person_id}/contact-attempts");
    let assignment_uri = format!("/api/people/{person_id}/assignment");
    let fut_contact = common::post_json_with_cookie(
        &router,
        &contact_attempts_uri,
        &cookie,
        json!({ "channel": "call", "outcome": "reached" }),
    );
    let fut_assign = common::post_json_with_cookie(
        &router,
        &assignment_uri,
        &cookie,
        json!({ "assigned_user_id": carol_id }),
    );
    let (contact_resp, assign_resp) = tokio::join!(fut_contact, fut_assign);

    assert_eq!(contact_resp.status(), StatusCode::CREATED);
    let assign_status = assign_resp.status();
    let assign_body = common::body_json(assign_resp).await;
    assert_eq!(assign_status, StatusCode::OK);
    assert_eq!(
        assign_body["changed"], true,
        "Alice -> Carol is a real change"
    );

    let (contact_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let (assignment_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        contact_after - contact_before,
        1,
        "exactly one contact_attempted fact"
    );
    assert_eq!(
        assignment_after - assignment_before,
        1,
        "exactly one assignment_changed fact"
    );

    let events = recorded(&publisher).await;
    assert_eq!(
        events.len(),
        events_before_race + 2,
        "exactly one person.changed per command execution, none lost or duplicated"
    );
    let new_changes: Vec<&str> = events[events_before_race..]
        .iter()
        .map(|(_, data)| data["data"]["change"].as_str().unwrap())
        .collect();
    assert!(new_changes.contains(&"contact_attempted"));
    assert!(new_changes.contains(&"assignment_changed"));
    for (_, data) in &events[events_before_race..] {
        assert_eq!(data["data"]["person_id"], person_id.to_string());
    }
}

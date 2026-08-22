//! DB-backed tests for `GET /api/today` (docs/specs/SLICE_003.md §13,
//! acceptance criterion 3). Run only via ./scripts/check-db. Backdated
//! Inquiries and contact attempts are migrator-role fixtures (test setup,
//! not application data — spec §13).
mod common;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn first_stage_id(pool: &PgPool, org_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_method(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    kind: &str,
    value: &str,
) {
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_inquiry(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(received_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_attempt(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    channel: &str,
    outcome: &str,
    occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(org_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(channel)
    .bind(outcome)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn hours_ago(h: i64) -> DateTime<Utc> {
    Utc::now() - ChronoDuration::hours(h)
}

/// Criterion 3: three Organizations/assignees, each seeing exactly their
/// own unanswered lead; a user with zero assigned People gets an empty
/// list; recommended_action follows phone presence.
#[sqlx::test]
#[ignore]
async fn today_is_scoped_to_viewer_and_organization(migrator_pool: PgPool) {
    let (org_acme, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id = common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    common::add_membership(&migrator_pool, org_acme, carol_id).await;
    let dave_id = common::create_user(&migrator_pool, "dave@acme.test", "Dave", "pw").await;
    common::add_membership(&migrator_pool, org_acme, dave_id).await;

    let (org_best, bob_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        "pw",
    )
    .await;

    let acme_stage = first_stage_id(&migrator_pool, org_acme).await;
    let best_stage = first_stage_id(&migrator_pool, org_best).await;

    // Alice's Person: phone present -> recommended_action "call".
    let alice_person = insert_person(&migrator_pool, org_acme, acme_stage, Some(alice_id)).await;
    insert_contact_method(
        &migrator_pool,
        org_acme,
        alice_person,
        "phone",
        "+15555550100",
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        org_acme,
        alice_person,
        "zillow",
        hours_ago(1),
    )
    .await;

    // Carol's Person: email only -> recommended_action "email".
    let carol_person = insert_person(&migrator_pool, org_acme, acme_stage, Some(carol_id)).await;
    insert_contact_method(
        &migrator_pool,
        org_acme,
        carol_person,
        "email",
        "carol-lead@example.com",
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        org_acme,
        carol_person,
        "referral",
        hours_ago(1),
    )
    .await;

    // Bob's Person, in the other Organization.
    let bob_person = insert_person(&migrator_pool, org_best, best_stage, Some(bob_id)).await;
    insert_contact_method(
        &migrator_pool,
        org_best,
        bob_person,
        "phone",
        "+15555550199",
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        org_best,
        bob_person,
        "website",
        hours_ago(1),
    )
    .await;

    let router = common::build_router(&migrator_pool).await;

    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let alice_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    let alice_items = alice_today["items"].as_array().unwrap();
    assert_eq!(alice_items.len(), 1, "Alice must see exactly her own item");
    assert_eq!(alice_items[0]["person"]["id"], alice_person.to_string());
    assert_eq!(alice_items[0]["priority"], "high");
    assert_eq!(alice_items[0]["recommended_action"], "call");
    let alice_codes: Vec<&str> = alice_items[0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(alice_codes, vec!["new_inquiry", "no_contact_attempt"]);

    let carol_cookie = common::login_cookie(&router, "carol@acme.test", "pw").await;
    let carol_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &carol_cookie).await)
            .await;
    let carol_items = carol_today["items"].as_array().unwrap();
    assert_eq!(carol_items.len(), 1, "Carol must see exactly her own item");
    assert_eq!(carol_items[0]["person"]["id"], carol_person.to_string());
    assert_eq!(carol_items[0]["recommended_action"], "email");

    let bob_cookie = common::login_cookie(&router, "bob@best.test", "pw").await;
    let bob_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &bob_cookie).await).await;
    let bob_items = bob_today["items"].as_array().unwrap();
    assert_eq!(bob_items.len(), 1, "Bob must see exactly his own item");
    assert_eq!(bob_items[0]["person"]["id"], bob_person.to_string());

    // Dave has zero assigned People -> an empty list, not an error.
    let dave_cookie = common::login_cookie(&router, "dave@acme.test", "pw").await;
    let dave_today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &dave_cookie).await).await;
    assert_eq!(dave_today["items"].as_array().unwrap().len(), 0);
    assert_eq!(dave_today["truncated"], false);
}

/// Criterion 3: an unassigned Person with an unanswered Inquiry appears on
/// nobody's Today.
#[sqlx::test]
#[ignore]
async fn unassigned_person_is_on_nobodys_today(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let unassigned_person = insert_person(&migrator_pool, org_id, stage_id, None).await;
    insert_inquiry(
        &migrator_pool,
        org_id,
        unassigned_person,
        "zillow",
        hours_ago(1),
    )
    .await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(today["items"].as_array().unwrap().len(), 0);
}

/// Criterion 3: a contact attempt (by the assignee, or by any other
/// member) removes the row from the assignee's Today.
#[sqlx::test]
#[ignore]
async fn contact_attempt_by_anyone_removes_the_row(migrator_pool: PgPool) {
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
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, person_id, "zillow", hours_ago(1)).await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let carol_cookie = common::login_cookie(&router, "carol@acme.test", "pw").await;

    let before =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(before["items"].as_array().unwrap().len(), 1);

    // Carol (not the assignee) logs the contact attempt.
    let log_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &carol_cookie,
        json!({ "channel": "call", "outcome": "no_answer" }),
    )
    .await;
    assert_eq!(log_resp.status(), axum::http::StatusCode::CREATED);

    let after =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    assert_eq!(
        after["items"].as_array().unwrap().len(),
        0,
        "a contact attempt by any member must remove the row from the assignee's Today"
    );
}

/// Criterion 3: a repeat Inquiry after an earlier answered one re-adds the
/// row with `repeat_inquiry {inquiry_count: 2}`, `last_contact_attempt`
/// set to the earlier attempt, and `waiting_since` = the repeat Inquiry's
/// `received_at` (the earlier one was answered, so the clock does not
/// reset to it).
#[sqlx::test]
#[ignore]
async fn repeat_inquiry_after_an_answered_one_resets_waiting_since_to_the_repeat(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let first_inquiry_at = hours_ago(72);
    insert_inquiry(
        &migrator_pool,
        org_id,
        person_id,
        "zillow",
        first_inquiry_at,
    )
    .await;
    let attempt_at = hours_ago(48);
    let attempt_id = insert_contact_attempt(
        &migrator_pool,
        org_id,
        person_id,
        "call",
        "no_answer",
        attempt_at,
    )
    .await;
    let repeat_inquiry_at = hours_ago(1);
    insert_inquiry(
        &migrator_pool,
        org_id,
        person_id,
        "referral",
        repeat_inquiry_at,
    )
    .await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    let items = today["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];

    assert_eq!(item["last_contact_attempt"]["id"], attempt_id.to_string());
    let waiting_since: DateTime<Utc> = item["waiting_since"].as_str().unwrap().parse().unwrap();
    assert!(
        (waiting_since - repeat_inquiry_at).num_seconds().abs() < 2,
        "waiting_since must be the repeat inquiry's received_at, not the first (answered) one"
    );
    let has_repeat_reason = item["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["code"] == "repeat_inquiry" && r["inquiry_count"] == 2);
    assert!(
        has_repeat_reason,
        "expected repeat_inquiry {{inquiry_count: 2}}"
    );
}

/// Criterion 3: with no attempt at all, `waiting_since` is the earliest
/// Inquiry's `received_at`, not the latest.
#[sqlx::test]
#[ignore]
async fn waiting_since_is_the_earliest_unanswered_inquiry_not_the_latest(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let earliest_at = hours_ago(72);
    insert_inquiry(&migrator_pool, org_id, person_id, "referral", earliest_at).await;
    let latest_at = hours_ago(1);
    insert_inquiry(&migrator_pool, org_id, person_id, "zillow", latest_at).await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    let item = &today["items"].as_array().unwrap()[0];

    let waiting_since: DateTime<Utc> = item["waiting_since"].as_str().unwrap().parse().unwrap();
    assert!((waiting_since - earliest_at).num_seconds().abs() < 2);
    assert_eq!(item["latest_inquiry"]["source"], "zillow");
}

/// Criterion 3: a fixture-backdated Inquiry (> 24h) sorts after fresh ones
/// with `normal` priority and no `new_inquiry` reason.
#[sqlx::test]
#[ignore]
async fn stale_inquiry_has_normal_priority_and_sorts_after_fresh(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let stale_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &migrator_pool,
        org_id,
        stale_person,
        "zillow",
        hours_ago(25),
    )
    .await;

    let fresh_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, fresh_person, "zillow", hours_ago(1)).await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    let items = today["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // Fresh (high) tier before stale (normal) tier.
    assert_eq!(items[0]["person"]["id"], fresh_person.to_string());
    assert_eq!(items[0]["priority"], "high");
    assert_eq!(items[1]["person"]["id"], stale_person.to_string());
    assert_eq!(items[1]["priority"], "normal");
    let stale_codes: Vec<&str> = items[1]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert!(!stale_codes.contains(&"new_inquiry"));
}

/// Criterion 3: 201+ stale candidates plus one fresh Inquiry -> the fresh
/// Person is in the response and `truncated` is true (tier-before-LIMIT,
/// §3).
#[sqlx::test]
#[ignore]
async fn fresh_person_survives_the_cap_behind_many_stale_candidates(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    for i in 0..201 {
        let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
        // Distinct stale timestamps so ordering among them is deterministic
        // and none coincide with the fresh person below.
        insert_inquiry(
            &migrator_pool,
            org_id,
            person_id,
            "zillow",
            hours_ago(48) - ChronoDuration::seconds(i),
        )
        .await;
    }
    let fresh_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, fresh_person, "zillow", hours_ago(1)).await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;

    assert_eq!(today["truncated"], true);
    let items = today["items"].as_array().unwrap();
    assert_eq!(items.len(), 200);
    assert!(
        items
            .iter()
            .any(|item| item["person"]["id"] == fresh_person.to_string()),
        "the fresh Person must survive the cap, ranked ahead of the 201 stale candidates"
    );
}

/// Criterion 3: two candidates with identical `waiting_since` order by
/// `person.id`; two Inquiries with identical `received_at` on the same
/// Person pick the greater `id` as `latest_inquiry`.
#[sqlx::test]
#[ignore]
async fn tie_breaks_are_person_id_then_inquiry_id(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let shared_received_at = hours_ago(1);

    let mut person_ids = Vec::new();
    for _ in 0..2 {
        let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
        insert_inquiry(
            &migrator_pool,
            org_id,
            person_id,
            "zillow",
            shared_received_at,
        )
        .await;
        person_ids.push(person_id);
    }
    person_ids.sort();

    // A third Person with two Inquiries sharing the exact same received_at:
    // the greater inquiry id must be reported as latest_inquiry.
    let tie_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let inquiry_a = insert_inquiry(
        &migrator_pool,
        org_id,
        tie_person,
        "zillow",
        shared_received_at,
    )
    .await;
    let inquiry_b = insert_inquiry(
        &migrator_pool,
        org_id,
        tie_person,
        "referral",
        shared_received_at,
    )
    .await;
    let expected_latest = std::cmp::max(inquiry_a, inquiry_b);

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let today =
        common::body_json(common::get_with_cookie(&router, "/api/today", &alice_cookie).await)
            .await;
    let items = today["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);

    // All three Persons share the exact same waiting_since, so the tied
    // group is all three, not just the first two — the whole response
    // order must be person.id ascending across all of them.
    let mut expected_order = person_ids.clone();
    expected_order.push(tie_person);
    expected_order.sort();

    let actual_order: Vec<Uuid> = items
        .iter()
        .map(|item| item["person"]["id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        actual_order, expected_order,
        "identical waiting_since must order by person.id ascending"
    );

    let tie_item = items
        .iter()
        .find(|item| item["person"]["id"] == tie_person.to_string())
        .unwrap();
    assert_eq!(
        tie_item["latest_inquiry"]["id"],
        expected_latest.to_string()
    );
}

/// Criterion 3: a client-supplied `user_id`/`organization_id` in query,
/// header, or body is ignored — the viewer is always
/// `AuthContext.actor_user_id`.
#[sqlx::test]
#[ignore]
async fn client_supplied_viewer_or_organization_is_ignored(migrator_pool: PgPool) {
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (_org_best, bob_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, person_id, "zillow", hours_ago(1)).await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let request = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/api/today?user_id={bob_id}&organization_id={}",
            Uuid::new_v4()
        ))
        .header("cookie", &alice_cookie)
        .header("x-user-id", bob_id.to_string())
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .unwrap();
    let body = common::body_json(response).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "query/header spoofing must not change the viewer"
    );
    assert_eq!(items[0]["person"]["id"], person_id.to_string());
}

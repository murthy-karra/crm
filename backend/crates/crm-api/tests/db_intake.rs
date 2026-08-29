//! DB-backed tests for `ReceiveInquiry` (docs/specs/SLICE_002.md §13,
//! acceptance criteria 3–7, 10–12, 17–18). Run only via ./scripts/check-db.

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
use crm_api::domain::raw_payload::crypto;

/// Criterion 3: a brand-new lead creates Person, contact methods, Inquiry,
/// a resolved raw_payload, and four facts sharing one correlation_id, with
/// `assignment_changed.causation_id` = the routing decision and
/// `stage_changed.to_stage_id` = the Organization's position-1 stage.
#[sqlx::test]
#[ignore]
async fn new_lead_intake_creates_person_and_four_linked_facts(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({
            "first_name": "Ada",
            "last_name": "Lovelace",
            "email": "ada@example.com",
            "phone": "555-555-0100",
            "message": "Interested in the listing",
        }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["person_created"], true);
    assert_eq!(body["duplicate"], false);
    assert_eq!(body["routing_strategy"], "actor_default");
    assert_eq!(
        body["assigned_user_id"].as_str().unwrap(),
        alice_id.to_string()
    );
    let person_id: Uuid = body["person_id"].as_str().unwrap().parse().unwrap();
    let inquiry_id: Uuid = body["inquiry_id"].as_str().unwrap().parse().unwrap();

    let (stage_id, assigned_user_id): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT stage_id, assigned_user_id FROM person WHERE id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assigned_user_id, Some(alice_id));

    let (position_1_stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        stage_id, position_1_stage_id,
        "new Person lands on the position-1 stage"
    );

    let (contact_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_method WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(contact_count, 2, "email and phone both stored");

    let (resolution, resolved_inquiry_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT resolution, inquiry_id FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "resolved");
    assert_eq!(resolved_inquiry_id, Some(inquiry_id));

    let (ir_correlation,): (Uuid,) =
        sqlx::query_as("SELECT correlation_id FROM inquiry_received WHERE inquiry_id = $1")
            .bind(inquiry_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let (routing_decision_id, rd_correlation, rd_strategy): (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT id, correlation_id, strategy FROM routing_decision WHERE inquiry_id = $1",
    )
    .bind(inquiry_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let (ac_correlation, ac_causation): (Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT correlation_id, causation_id FROM assignment_changed WHERE person_id = $1",
    )
    .bind(person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let (sc_correlation, sc_to_stage_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT correlation_id, to_stage_id FROM stage_changed WHERE person_id = $1",
    )
    .bind(person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    assert_eq!(ir_correlation, rd_correlation);
    assert_eq!(rd_correlation, ac_correlation);
    assert_eq!(ac_correlation, sc_correlation);
    assert_eq!(rd_strategy, "actor_default");
    assert_eq!(
        ac_causation,
        Some(routing_decision_id),
        "assignment_changed.causation_id must be the routing_decision id"
    );
    assert_eq!(sc_to_stage_id, position_1_stage_id);
}

/// Criterion 4 (email leg): a repeat inquiry matching an already-assigned
/// Person by email creates no Person, no stage or assignment fact,
/// preserves the first Inquiry's source, and reports `kept_existing` with
/// the real assignee.
#[sqlx::test]
#[ignore]
async fn repeat_inquiry_matching_assigned_person_by_email_keeps_existing(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, carol_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let first = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "ada@example.com" }),
        Some(carol_id),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = crate::common::body_json(first).await;
    let person_id: Uuid = first_body["person_id"].as_str().unwrap().parse().unwrap();
    let first_inquiry_id: Uuid = first_body["inquiry_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        first_body["assigned_user_id"].as_str().unwrap(),
        carol_id.to_string()
    );

    let second = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "email": "Ada@Example.com" }),
        None,
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::CREATED,
        "a different source/content is a new Inquiry"
    );
    let second_body = crate::common::body_json(second).await;
    assert_eq!(second_body["person_created"], false);
    assert_eq!(second_body["routing_strategy"], "kept_existing");
    assert_eq!(
        second_body["assigned_user_id"].as_str().unwrap(),
        carol_id.to_string()
    );
    assert_eq!(
        second_body["person_id"].as_str().unwrap(),
        person_id.to_string()
    );

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 1, "no new Person");

    let (assignment_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        assignment_count, 1,
        "only the first intake's assignment_changed"
    );

    let (stage_change_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        stage_change_count, 1,
        "only the first intake's stage_changed"
    );

    let (rd_strategy,): (String,) = sqlx::query_as(
        "SELECT strategy FROM routing_decision WHERE inquiry_id != $1 AND person_id = $2",
    )
    .bind(first_inquiry_id)
    .bind(person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(rd_strategy, "kept_existing");

    let (first_source,): (String,) = sqlx::query_as("SELECT source FROM inquiry WHERE id = $1")
        .bind(first_inquiry_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(
        first_source, "zillow",
        "the first Inquiry's source attribution is untouched (D-006)"
    );
}

/// Criterion 4 (phone leg): the same, matching by phone instead of email.
#[sqlx::test]
#[ignore]
async fn repeat_inquiry_matching_assigned_person_by_phone_keeps_existing(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, carol_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let first = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "phone": "555-555-0177" }),
        Some(carol_id),
    )
    .await;
    let first_body = crate::common::body_json(first).await;
    let person_id: Uuid = first_body["person_id"].as_str().unwrap().parse().unwrap();

    let second = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "phone": "(555) 555-0177" }),
        None,
    )
    .await;
    let second_body = crate::common::body_json(second).await;
    assert_eq!(
        second_body["person_id"].as_str().unwrap(),
        person_id.to_string()
    );
    assert_eq!(second_body["routing_strategy"], "kept_existing");
    assert_eq!(
        second_body["assigned_user_id"].as_str().unwrap(),
        carol_id.to_string()
    );
}

/// Criterion 4 (unassigned leg): matching an existing Person with **no**
/// assignee writes exactly one `assignment_changed` from NULL — a repeat
/// lead must not leave a Person ownerless.
#[sqlx::test]
#[ignore]
async fn repeat_inquiry_matching_unassigned_person_writes_one_assignment_fact(
    migrator_pool: PgPool,
) {
    let (_org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    // First: unassign immediately so the Person is left with no assignee.
    let first = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "unowned@example.com" }),
        None,
    )
    .await;
    let first_body = crate::common::body_json(first).await;
    let person_id: Uuid = first_body["person_id"].as_str().unwrap().parse().unwrap();

    let unassign = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/assignment"),
        &cookie,
        json!({ "assigned_user_id": null }),
    )
    .await;
    assert_eq!(unassign.status(), StatusCode::OK);

    let second = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "email": "unowned@example.com" }),
        None,
    )
    .await;
    let second_body = crate::common::body_json(second).await;
    assert_eq!(second_body["routing_strategy"], "actor_default");
    assert_eq!(
        second_body["assigned_user_id"].as_str().unwrap(),
        alice_id.to_string()
    );

    let (assignment_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    // intake #1 (NULL -> alice), manual unassign (alice -> NULL), intake #2 (NULL -> alice)
    assert_eq!(assignment_count, 3);

    let (assigned_user_id,): (Option<Uuid>,) =
        sqlx::query_as("SELECT assigned_user_id FROM person WHERE id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assigned_user_id, Some(alice_id));
}

/// Criterion 5: when email and phone match *different* Persons, email wins.
#[sqlx::test]
#[ignore]
async fn email_wins_when_email_and_phone_match_different_persons(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let email_only = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "email-person@example.com" }),
        None,
    )
    .await;
    let email_person_id: Uuid = crate::common::body_json(email_only)
        .await
        .get("person_id")
        .unwrap()
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let phone_only = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "phone": "555-555-0199" }),
        None,
    )
    .await;
    let phone_person_id: Uuid = crate::common::body_json(phone_only)
        .await
        .get("person_id")
        .unwrap()
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(email_person_id, phone_person_id);

    let both = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "email": "email-person@example.com", "phone": "555-555-0199" }),
        None,
    )
    .await;
    let both_body = crate::common::body_json(both).await;
    assert_eq!(
        both_body["person_id"].as_str().unwrap(),
        email_person_id.to_string(),
        "email match must win over phone match"
    );

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 2, "no third Person created");
}

/// Criterion 6: a payload with no normalizable contact method is stored
/// (encrypted) and marked unresolved; zero Person/Inquiry/fact rows are
/// created; and the ciphertext does not contain the garbage email's bytes.
#[sqlx::test]
#[ignore]
async fn no_contact_method_payload_is_unresolved_and_ciphertext_hides_plaintext(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "not-a-real-email", "message": "no phone or valid email here" }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["status"], "unresolved");
    assert_eq!(body["reason"], "no_contact_method");
    assert_eq!(body["duplicate"], false);

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 0);
    let (inquiry_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(inquiry_count, 0);
    let (fact_count,): (i64,) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM inquiry_received) + (SELECT count(*) FROM routing_decision)
            + (SELECT count(*) FROM assignment_changed) + (SELECT count(*) FROM stage_changed)",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(fact_count, 0);

    let (resolution, unresolved_reason, ciphertext): (String, Option<String>, Vec<u8>) = sqlx::query_as(
        "SELECT resolution, unresolved_reason, ciphertext FROM raw_payload WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(resolution, "unresolved");
    assert_eq!(unresolved_reason.as_deref(), Some("no_contact_method"));
    let needle = b"not-a-real-email";
    assert!(
        !ciphertext.windows(needle.len()).any(|w| w == needle),
        "ciphertext must not contain the plaintext email bytes"
    );
}

/// Criterion 7: an identical payload delivered twice produces one
/// raw_payload row, one Inquiry, one set of facts, and a second response
/// carrying `duplicate: true`.
#[sqlx::test]
#[ignore]
async fn identical_payload_delivered_twice_is_idempotent(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;
    let payload = json!({ "email": "twice@example.com" });

    let first =
        crate::common::post_inquiry(&router, &cookie, "zillow", payload.clone(), None).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = crate::common::body_json(first).await;
    assert_eq!(first_body["duplicate"], false);

    let second = crate::common::post_inquiry(&router, &cookie, "zillow", payload, None).await;
    assert_eq!(second.status(), StatusCode::OK);
    let second_body = crate::common::body_json(second).await;
    assert_eq!(second_body["duplicate"], true);
    assert_eq!(second_body["inquiry_id"], first_body["inquiry_id"]);
    assert_eq!(second_body["person_id"], first_body["person_id"]);

    let (raw_payload_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(raw_payload_count, 1);
    let (inquiry_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(inquiry_count, 1);
    let (ir_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry_received WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(ir_count, 1);
}

/// Criterion 10: a `pending` row (fixture-inserted, encrypted with the test
/// key — as if Phase B previously failed transiently) resolves on a re-POST
/// of the same bytes, with `duplicate: false` (this is the row's first
/// successful resolution).
#[sqlx::test]
#[ignore]
async fn fixture_pending_row_resolves_on_repost_with_duplicate_false(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let payload = json!({ "email": "recovers@example.com" });
    let fixture_id = Uuid::new_v4();
    crate::common::insert_raw_payload_fixture(
        &migrator_pool,
        fixture_id,
        org_id,
        "zillow",
        "pending",
        &payload,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = crate::common::post_inquiry(&router, &cookie, "zillow", payload, None).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["duplicate"], false);

    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE id = $1")
            .bind(fixture_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "resolved");
}

/// Criterion 11: a wrong key or tampered ciphertext (simulated by a fixture
/// whose stored bytes are garbage but whose `content_hmac` matches a real
/// POST's plaintext, so the delivery collides with it via the idempotency
/// key) returns 500 `internal_error`; the row stays `pending` and is
/// visible in the unresolved queue.
#[sqlx::test]
#[ignore]
async fn tampered_ciphertext_returns_500_and_leaves_row_pending_and_queued(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;

    let payload = json!({ "email": "tampered@example.com" });
    let plaintext = serde_json::to_vec(&payload).unwrap();
    let config = crate::common::test_config();
    let real_hmac = crypto::content_hmac(&config.raw_payload_key, &plaintext);

    let fixture_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'zillow', 'generic_v1', 'web_session', now(), $3, $4, $5, $6, 'pending')"#,
    )
    .bind(fixture_id)
    .bind(org_id)
    .bind(vec![0xAAu8; 24])
    .bind(vec![0xBBu8; 40])
    .bind(real_hmac.to_vec())
    .bind(plaintext.len() as i32)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = crate::common::post_inquiry(&router, &cookie, "zillow", payload, None).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "internal_error");

    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE id = $1")
            .bind(fixture_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(resolution, "pending");

    let unresolved =
        crate::common::get_with_cookie(&router, "/api/intake/unresolved", &cookie).await;
    let unresolved_body = crate::common::body_json(unresolved).await;
    let ids: Vec<String> = unresolved_body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&fixture_id.to_string()));
}

/// Criterion 12: an invalid assignee is rejected before anything is
/// stored — 422, and no `raw_payload` row is written.
#[sqlx::test]
#[ignore]
async fn invalid_assignee_returns_422_and_writes_no_raw_payload_row(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let (count_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let bogus_user = Uuid::new_v4();
    let response = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "ada@example.com" }),
        Some(bogus_user),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "invalid_assignee");

    let (count_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count_before, count_after);
}

/// Criterion 17 (leg 1): two concurrent intakes for the same brand-new
/// email collapse to one Person (the per-Organization advisory lock
/// serializes them).
#[sqlx::test]
#[ignore]
async fn concurrent_new_intakes_for_the_same_email_create_one_person(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let fut_a = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "concurrent@example.com", "phone": "555-555-0111" }),
        None,
    );
    let fut_b = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "email": "concurrent@example.com" }),
        None,
    );
    let (resp_a, resp_b) = tokio::join!(fut_a, fut_b);
    assert_eq!(resp_a.status(), StatusCode::CREATED);
    assert_eq!(resp_b.status(), StatusCode::CREATED);
    let body_a = crate::common::body_json(resp_a).await;
    let body_b = crate::common::body_json(resp_b).await;
    assert_eq!(
        body_a["person_id"], body_b["person_id"],
        "concurrent intakes for the same email must collapse to one Person"
    );

    let (person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(person_count, 1);
}

/// Criterion 17 (leg 2): two concurrent identical deliveries yield one
/// Inquiry, one 201, and one 200.
#[sqlx::test]
#[ignore]
async fn concurrent_identical_deliveries_yield_one_201_and_one_200(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;
    let payload = json!({ "email": "duplicate-race@example.com" });

    let fut_a = crate::common::post_inquiry(&router, &cookie, "zillow", payload.clone(), None);
    let fut_b = crate::common::post_inquiry(&router, &cookie, "zillow", payload, None);
    let (resp_a, resp_b) = tokio::join!(fut_a, fut_b);

    let status_a = resp_a.status();
    let status_b = resp_b.status();
    let body_a = crate::common::body_json(resp_a).await;
    let body_b = crate::common::body_json(resp_b).await;

    let (created_status, created_body, ok_status, ok_body) = if status_a == StatusCode::CREATED {
        (status_a, body_a, status_b, body_b)
    } else {
        (status_b, body_b, status_a, body_a)
    };
    assert_eq!(created_status, StatusCode::CREATED);
    assert_eq!(ok_status, StatusCode::OK);
    assert_eq!(created_body["duplicate"], false);
    assert_eq!(ok_body["duplicate"], true);
    assert_eq!(created_body["inquiry_id"], ok_body["inquiry_id"]);
    assert_eq!(created_body["person_id"], ok_body["person_id"]);

    let (raw_payload_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(raw_payload_count, 1);
    let (inquiry_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM inquiry WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(inquiry_count, 1);
}

/// Criterion 18: an explicit `assign_to_user_id` on an already-assigned
/// Person is ignored — `kept_existing`, no new fact, and the 201 body
/// reports the existing assignee, not the requested one.
#[sqlx::test]
#[ignore]
async fn explicit_assignee_on_already_assigned_person_is_ignored(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, carol_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let first = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "ada@example.com" }),
        Some(carol_id),
    )
    .await;
    let first_body = crate::common::body_json(first).await;
    assert_eq!(first_body["routing_strategy"], "explicit");
    assert_eq!(
        first_body["assigned_user_id"].as_str().unwrap(),
        carol_id.to_string()
    );
    let person_id: Uuid = first_body["person_id"].as_str().unwrap().parse().unwrap();

    let second = crate::common::post_inquiry(
        &router,
        &cookie,
        "realtor_com",
        json!({ "email": "Ada@Example.com" }),
        Some(alice_id),
    )
    .await;
    let second_body = crate::common::body_json(second).await;
    assert_eq!(second_body["routing_strategy"], "kept_existing");
    assert_eq!(
        second_body["assigned_user_id"].as_str().unwrap(),
        carol_id.to_string(),
        "the existing assignee is reported, not the requested one"
    );
    assert_eq!(
        second_body["person_id"].as_str().unwrap(),
        person_id.to_string()
    );

    let (assignment_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        assignment_count, 1,
        "only the first intake's assignment_changed"
    );
}

// --- Malformed-request 400s on POST /api/inquiries ----------------------
//
// AuthContext is a required parameter on this route and (unlike the People
// routes' `{id}` handlers) there is no earlier FromRequestParts extractor
// to reorder ahead of it, so exercising the body/`source` validation at
// the HTTP layer requires a real authenticated session — hence these live
// in the DB-backed suite rather than tests/intake.rs (docs/specs/SLICE_002.md
// §13's "malformed/oversized/non-JSON bodies, bad source" requirement).

#[sqlx::test]
#[ignore]
async fn malformed_json_body_returns_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inquiries")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = crate::common::body_json(response).await;
    assert_eq!(body, json!({ "error": "malformed_request" }));
}

#[sqlx::test]
#[ignore]
async fn non_json_content_type_returns_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inquiries")
                .header("content-type", "text/plain")
                .header("cookie", &cookie)
                .body(Body::from("source=zillow"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn oversized_body_returns_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    // Comfortably over the 256 KiB cap (docs/specs/SLICE_002.md §5).
    let oversized = json!({
        "source": "zillow",
        "payload": { "message": "x".repeat(300 * 1024) },
    });
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inquiries")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(oversized.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = crate::common::body_json(response).await;
    assert_eq!(body, json!({ "error": "malformed_request" }));
}

#[sqlx::test]
#[ignore]
async fn bad_source_returns_400_and_writes_no_raw_payload_row(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", "pw").await;

    let response = crate::common::post_inquiry(
        &router,
        &cookie,
        "zillow.com/not-allowed",
        json!({ "email": "ada@example.com" }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = crate::common::body_json(response).await;
    assert_eq!(body, json!({ "error": "malformed_request" }));

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM raw_payload WHERE organization_id = $1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "an invalid source must leave no raw_payload row");
}

/// Adversarial: reproduces the cross-tenant pool-starvation finding and
/// disproves it against the bounded-retry fix. An external transaction
/// holds `busy_org`'s advisory lock for longer than `ADVISORY_LOCK_BUDGET`
/// — the same technique (a raw held `pg_advisory_xact_lock` transaction)
/// the adversarial pass used to find the original blocking-wait bug. A
/// request into `busy_org` must fail fast (503 `intake_busy`,
/// `Retry-After` present, `raw_payload` left `pending`) within roughly the
/// budget window rather than waiting out the full external hold; a
/// concurrent burst against a *different*, unrelated Organization during
/// that same hold must be completely unaffected — proving the shared pool
/// itself is no longer starved by one Organization's contention.
#[sqlx::test]
#[ignore]
async fn advisory_lock_contention_fails_fast_without_starving_other_organizations(
    migrator_pool: PgPool,
) {
    let (busy_org_id, _busy_user_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Busy Org",
        "busy@busy.test",
        "Busy",
        "pw",
    )
    .await;
    let (other_org_id, _other_user_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Other Org",
        "other@other.test",
        "Other",
        "pw",
    )
    .await;

    // Hold busy_org's advisory lock externally for comfortably longer than
    // the retry budget, so a request that merely waited out the full hold
    // (instead of failing fast) would be unambiguously distinguishable
    // from one that actually gave up within budget.
    let hold_duration = ADVISORY_LOCK_BUDGET + Duration::from_secs(4);
    let lock_key_text = busy_org_id.to_string();
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

    // Give the external hold a moment to actually acquire the lock before
    // racing a real request against it.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let busy_cookie = crate::common::login_cookie(&router, "busy@busy.test", "pw").await;

    let start = Instant::now();
    let response = crate::common::post_inquiry(
        &router,
        &busy_cookie,
        "zillow",
        json!({ "email": "blocked@example.com" }),
        None,
    )
    .await;
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let retry_after = response
        .headers()
        .get("retry-after")
        .map(|v| v.to_str().unwrap().to_string());
    let body = crate::common::body_json(response).await;
    assert_eq!(body, json!({ "error": "intake_busy" }));
    assert!(
        retry_after.is_some(),
        "503 intake_busy must carry a Retry-After header"
    );
    assert!(
        elapsed >= Duration::from_millis(500),
        "should have actually retried for a while, not failed instantly: {elapsed:?}"
    );
    assert!(
        elapsed < hold_duration,
        "must fail fast within roughly the retry budget, not wait out the full external hold \
         ({elapsed:?} should be well under {hold_duration:?})"
    );
    assert!(
        elapsed < ADVISORY_LOCK_BUDGET + Duration::from_secs(2),
        "elapsed {elapsed:?} should track the retry budget ({ADVISORY_LOCK_BUDGET:?}), not drift \
         far past it"
    );

    let (resolution,): (String,) =
        sqlx::query_as("SELECT resolution FROM raw_payload WHERE organization_id = $1")
            .bind(busy_org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        resolution, "pending",
        "Phase A's row must be left exactly as written; a re-POST retries from scratch"
    );

    // The actual point of this test: while busy_org's lock is still held
    // (hold_duration comfortably exceeds the budget above plus this whole
    // test so far), a burst of concurrent requests to a *different*,
    // unrelated Organization must be fast and unaffected — proving the
    // pool itself was never starved by busy_org's contention.
    let other_cookie = crate::common::login_cookie(&router, "other@other.test", "pw").await;
    let burst_start = Instant::now();
    let (r0, r1, r2, r3, r4) = tokio::join!(
        crate::common::post_inquiry(
            &router,
            &other_cookie,
            "zillow",
            json!({ "email": "burst0@example.com" }),
            None
        ),
        crate::common::post_inquiry(
            &router,
            &other_cookie,
            "zillow",
            json!({ "email": "burst1@example.com" }),
            None
        ),
        crate::common::post_inquiry(
            &router,
            &other_cookie,
            "zillow",
            json!({ "email": "burst2@example.com" }),
            None
        ),
        crate::common::post_inquiry(
            &router,
            &other_cookie,
            "zillow",
            json!({ "email": "burst3@example.com" }),
            None
        ),
        crate::common::post_inquiry(
            &router,
            &other_cookie,
            "zillow",
            json!({ "email": "burst4@example.com" }),
            None
        ),
    );
    let burst_elapsed = burst_start.elapsed();

    for (i, response) in [r0, r1, r2, r3, r4].into_iter().enumerate() {
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "unrelated Organization's request {i} must succeed while busy_org is contended"
        );
    }
    assert!(
        burst_elapsed < Duration::from_secs(2),
        "unrelated Organization's burst must complete quickly, proving no cross-tenant pool \
         starvation: took {burst_elapsed:?}"
    );

    let (other_person_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM person WHERE organization_id = $1")
            .bind(other_org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        other_person_count, 5,
        "all five unrelated-Organization requests must have actually completed successfully"
    );

    hold_task.abort();
}

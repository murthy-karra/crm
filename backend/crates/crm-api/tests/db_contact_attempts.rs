//! DB-backed tests for `LogContactAttempt` / `POST
//! /api/people/{id}/contact-attempts` (docs/specs/SLICE_003.md §13,
//! acceptance criterion 2). Run only via ./scripts/check-db.
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn create_person_with_inquiry(router: &axum::Router, cookie: &str, email: &str) -> Uuid {
    let resp = common::post_inquiry(
        router,
        cookie,
        "zillow",
        json!({ "email": email, "message": "hi" }),
        None,
    )
    .await;
    common::body_json(resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Criterion 2 (first half): `LogContactAttempt` writes exactly one fact
/// with the full envelope, and the response shape matches §5.
#[sqlx::test]
#[ignore]
async fn log_contact_attempt_writes_exactly_one_fact_with_full_envelope(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let person_id = create_person_with_inquiry(&router, &cookie, "lead@example.com").await;

    let resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &cookie,
        json!({ "channel": "call", "outcome": "no_answer" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = common::body_json(resp).await;
    assert_eq!(body["contact_attempt"]["channel"], "call");
    assert_eq!(body["contact_attempt"]["outcome"], "no_answer");
    assert!(body["contact_attempt"]["id"].is_string());
    assert!(body["contact_attempt"]["occurred_at"].is_string());
    assert_eq!(body["person"]["id"], person_id.to_string());

    let row: (String, Option<Uuid>, String, Uuid) = sqlx::query_as(
        r#"SELECT actor_kind, actor_user_id, origin, correlation_id
           FROM contact_attempted WHERE person_id = $1"#,
    )
    .bind(person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(row.0, "user");
    assert!(row.1.is_some());
    assert_eq!(row.2, "web_session");
    // A fresh correlation id: not nil, and distinct from anything else.
    assert_ne!(row.3, Uuid::nil());

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

/// Criterion 2: invalid `channel`/`outcome` and a non-JSON body both map
/// to 400 `malformed_request` (a serde rejection, not a new `ApiError`
/// variant), and write no fact.
#[sqlx::test]
#[ignore]
async fn invalid_channel_or_non_json_body_returns_400_and_writes_no_fact(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let person_id = create_person_with_inquiry(&router, &cookie, "lead2@example.com").await;

    let bad_channel = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &cookie,
        json!({ "channel": "carrier_pigeon", "outcome": "no_answer" }),
    )
    .await;
    assert_eq!(bad_channel.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::body_json(bad_channel).await["error"],
        "malformed_request"
    );

    let bad_outcome = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &cookie,
        json!({ "channel": "call", "outcome": "shrug" }),
    )
    .await;
    assert_eq!(bad_outcome.status(), StatusCode::BAD_REQUEST);

    let non_json = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/people/{person_id}/contact-attempts"))
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_json.status(), StatusCode::BAD_REQUEST);

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

/// Criterion 2: another Organization's Person returns 404 `not_found`,
/// byte-identical to a nonexistent id.
#[sqlx::test]
#[ignore]
async fn other_organization_person_returns_404_identical_to_nonexistent(migrator_pool: PgPool) {
    let (_org_a, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (_org_b, _bob_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        "pw",
    )
    .await;

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let bob_cookie = common::login_cookie(&router, "bob@best.test", "pw").await;

    let b_person_id =
        create_person_with_inquiry(&router, &bob_cookie, "bobs-lead@example.com").await;

    let cross_org_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{b_person_id}/contact-attempts"),
        &alice_cookie,
        json!({ "channel": "call", "outcome": "no_answer" }),
    )
    .await;
    let cross_org_status = cross_org_resp.status();
    let cross_org_body = common::body_json(cross_org_resp).await;

    let nonexistent_id = Uuid::new_v4();
    let nonexistent_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{nonexistent_id}/contact-attempts"),
        &alice_cookie,
        json!({ "channel": "call", "outcome": "no_answer" }),
    )
    .await;
    let nonexistent_status = nonexistent_resp.status();
    let nonexistent_body = common::body_json(nonexistent_resp).await;

    assert_eq!(cross_org_status, StatusCode::NOT_FOUND);
    assert_eq!(cross_org_status, nonexistent_status);
    assert_eq!(cross_org_body, nonexistent_body);
}

/// Criterion 2: a migrator fixture inserts a `stage_changed` and a
/// `contact_attempted` for the same Person with identical `occurred_at`
/// **in one transaction** (so `recorded_at` is identical too) -> the
/// detail history sorts the contact attempt last (kind_rank 4 > 3), and
/// its `detail` is exactly `{"channel", "outcome"}`.
#[sqlx::test]
#[ignore]
async fn history_sorts_contact_attempted_last_on_identical_timestamps(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let person_id = create_person_with_inquiry(&router, &cookie, "lead3@example.com").await;

    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let mut tx = migrator_pool.begin().await.unwrap();
    let shared_occurred_at = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT now()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let correlation_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO stage_changed
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, from_stage_id, to_stage_id, reason)
           VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, NULL, $5, 'manual')"#,
    )
    .bind(org_id)
    .bind(shared_occurred_at)
    .bind(correlation_id)
    .bind(person_id)
    .bind(stage_id)
    .execute(&mut *tx)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
           VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'text', 'sent')"#,
    )
    .bind(org_id)
    .bind(shared_occurred_at)
    .bind(correlation_id)
    .bind(person_id)
    .execute(&mut *tx)
    .await
    .unwrap();

    tx.commit().await.unwrap();

    let detail_resp =
        common::get_with_cookie(&router, &format!("/api/people/{person_id}"), &cookie).await;
    let detail_body = common::body_json(detail_resp).await;
    let history = detail_body["history"].as_array().unwrap();

    let stage_changed_index = history
        .iter()
        .position(|e| {
            e["kind"] == "stage_changed" && e["correlation_id"] == correlation_id.to_string()
        })
        .expect("stage_changed entry present");
    let contact_attempted_index = history
        .iter()
        .position(|e| {
            e["kind"] == "contact_attempted" && e["correlation_id"] == correlation_id.to_string()
        })
        .expect("contact_attempted entry present");
    assert!(
        contact_attempted_index > stage_changed_index,
        "contact_attempted (kind_rank 4) must sort after stage_changed (kind_rank 3) on identical timestamps"
    );

    let contact_entry = &history[contact_attempted_index];
    // `call_id`/`corrects_id`/`superseded` are the declared additive
    // SLICE_006c §2 change to the `contact_attempted` history detail.
    assert_eq!(
        contact_entry["detail"],
        json!({
            "channel": "text",
            "outcome": "sent",
            "call_id": null,
            "corrects_id": null,
            "superseded": false,
        })
    );
}

/// §7: contact attempts may be logged by any member on any Person, not
/// only the assignee — a non-assignee member (here, a second member of
/// the same Organization) can log one on someone else's Person.
#[sqlx::test]
#[ignore]
async fn any_organization_member_may_log_a_contact_attempt(migrator_pool: PgPool) {
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

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let carol_cookie = common::login_cookie(&router, "carol@acme.test", "pw").await;

    // Assigned to Alice by intake's actor-default routing.
    let person_id = create_person_with_inquiry(&router, &alice_cookie, "shared@example.com").await;

    let resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &carol_cookie,
        json!({ "channel": "email", "outcome": "sent" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// docs/specs/SLICE_006c.md §2: the manual route's vocabulary widens with
/// `ContactOutcome` — `busy` and `wrong_number` are accepted and stored.
#[sqlx::test]
#[ignore]
async fn manual_route_accepts_busy_and_wrong_number(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let person_id = create_person_with_inquiry(&router, &cookie, "lead-006c@example.com").await;

    for outcome in ["busy", "wrong_number"] {
        let resp = common::post_json_with_cookie(
            &router,
            &format!("/api/people/{person_id}/contact-attempts"),
            &cookie,
            json!({ "channel": "call", "outcome": outcome }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "{outcome}");
        let body = common::body_json(resp).await;
        assert_eq!(body["contact_attempt"]["outcome"], outcome);
        let id: Uuid = body["contact_attempt"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let (stored, corrects_id): (String, Option<Uuid>) =
            sqlx::query_as("SELECT outcome, corrects_id FROM contact_attempted WHERE id = $1")
                .bind(id)
                .fetch_one(&migrator_pool)
                .await
                .unwrap();
        assert_eq!(stored, outcome);
        assert!(corrects_id.is_none());
    }
}

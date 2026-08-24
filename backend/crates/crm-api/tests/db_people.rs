//! DB-backed tests for tenant isolation, the assign/stage commands, list
//! truncation, and seed idempotency (docs/specs/SLICE_002.md §13,
//! acceptance criteria 8, 9, 14, 19). Run only via ./scripts/check-db.
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// Criterion 8: two Organizations never leak into each other's People,
/// Stages, or Unresolved queue, in either direction; a client-supplied
/// Organization id is ignored.
#[sqlx::test]
#[ignore]
async fn cross_organization_people_stages_and_unresolved_are_isolated(migrator_pool: PgPool) {
    let (_org_a, _alice_id) = common::create_org_with_stages_and_member(
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

    let router = common::build_router(&migrator_pool).await;
    let alice_cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let bob_cookie = common::login_cookie(&router, "bob@best.test", "pw").await;

    // Same email in both Organizations -> two distinct Persons.
    let a_resp = common::post_inquiry(
        &router,
        &alice_cookie,
        "zillow",
        json!({ "email": "shared@example.com" }),
        None,
    )
    .await;
    let a_person_id: Uuid = common::body_json(a_resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let b_resp = common::post_inquiry(
        &router,
        &bob_cookie,
        "zillow",
        json!({ "email": "shared@example.com" }),
        None,
    )
    .await;
    let b_person_id: Uuid = common::body_json(b_resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(a_person_id, b_person_id);

    // Bob (org B) gets 404 on Alice's (org A's) Person — GET, assignment, stage.
    let get_resp =
        common::get_with_cookie(&router, &format!("/api/people/{a_person_id}"), &bob_cookie).await;
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);

    let assign_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/assignment"),
        &bob_cookie,
        json!({ "assigned_user_id": bob_id }),
    )
    .await;
    assert_eq!(assign_resp.status(), StatusCode::NOT_FOUND);

    let stage_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/stage"),
        &bob_cookie,
        json!({ "stage_id": Uuid::new_v4() }),
    )
    .await;
    assert_eq!(stage_resp.status(), StatusCode::NOT_FOUND);

    // Alice (org A) using org B's stage on her own Person -> 422, no fact.
    let (b_stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let (stage_fact_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage_changed WHERE person_id = $1")
            .bind(a_person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let cross_stage_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/stage"),
        &alice_cookie,
        json!({ "stage_id": b_stage_id }),
    )
    .await;
    assert_eq!(cross_stage_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        common::body_json(cross_stage_resp).await["error"],
        "invalid_stage"
    );
    let (stage_fact_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage_changed WHERE person_id = $1")
            .bind(a_person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(stage_fact_before, stage_fact_after);

    // Alice (org A) using org B's member as an assignee on her own Person
    // -> 422, no fact.
    let (assign_fact_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(a_person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let cross_assign_resp = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{a_person_id}/assignment"),
        &alice_cookie,
        json!({ "assigned_user_id": bob_id }),
    )
    .await;
    assert_eq!(cross_assign_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        common::body_json(cross_assign_resp).await["error"],
        "invalid_assignee"
    );
    let (assign_fact_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(a_person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assign_fact_before, assign_fact_after);

    // GET /api/people is Organization-scoped.
    let people_body =
        common::body_json(common::get_with_cookie(&router, "/api/people", &alice_cookie).await)
            .await;
    let people_ids: Vec<String> = people_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert!(people_ids.contains(&a_person_id.to_string()));
    assert!(!people_ids.contains(&b_person_id.to_string()));

    // GET /api/stages is Organization-scoped.
    let stages_body =
        common::body_json(common::get_with_cookie(&router, "/api/stages", &alice_cookie).await)
            .await;
    let stage_ids: Vec<String> = stages_body["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!stage_ids.contains(&b_stage_id.to_string()));

    // GET /api/intake/unresolved is Organization-scoped.
    common::post_inquiry(
        &router,
        &alice_cookie,
        "zillow",
        json!({ "email": "not-valid" }),
        None,
    )
    .await;
    common::post_inquiry(
        &router,
        &bob_cookie,
        "zillow",
        json!({ "email": "also-not-valid" }),
        None,
    )
    .await;
    let alice_unresolved = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &alice_cookie).await,
    )
    .await;
    let bob_unresolved = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &bob_cookie).await,
    )
    .await;
    assert_eq!(alice_unresolved["items"].as_array().unwrap().len(), 1);
    assert_eq!(bob_unresolved["items"].as_array().unwrap().len(), 1);

    // A client-supplied Organization id (query string) is ignored.
    let probe = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/people?organization_id={org_b}"))
                .header("cookie", &alice_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let probe_body = common::body_json(probe).await;
    let probe_ids: Vec<String> = probe_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!probe_ids.contains(&b_person_id.to_string()));
    assert!(probe_ids.contains(&a_person_id.to_string()));
}

/// Criterion 9: `AssignPerson`/`ChangePersonStage` write exactly one fact
/// when the value actually changes and none when it does not; the person
/// detail's history ordering is stable across intake facts and later manual
/// commands.
#[sqlx::test]
#[ignore]
async fn assign_and_stage_commands_are_fact_precise_and_history_orders_stably(
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

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;

    let intake = common::post_inquiry(
        &router,
        &cookie,
        "zillow",
        json!({ "email": "target@example.com" }),
        None,
    )
    .await;
    let person_id: Uuid = common::body_json(intake).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Reassign to carol: changed.
    let resp1 = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/assignment"),
        &cookie,
        json!({ "assigned_user_id": carol_id }),
    )
    .await;
    assert_eq!(common::body_json(resp1).await["changed"], true);

    // Reassign to carol again: no-op, no new fact.
    let (assign_count_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let resp2 = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/assignment"),
        &cookie,
        json!({ "assigned_user_id": carol_id }),
    )
    .await;
    assert_eq!(common::body_json(resp2).await["changed"], false);
    let (assign_count_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM assignment_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(assign_count_before, assign_count_after);

    // Change stage: changed.
    let stages_body =
        common::body_json(common::get_with_cookie(&router, "/api/stages", &cookie).await).await;
    let second_stage_id: Uuid = stages_body["stages"][1]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let stage_resp1 = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/stage"),
        &cookie,
        json!({ "stage_id": second_stage_id }),
    )
    .await;
    assert_eq!(common::body_json(stage_resp1).await["changed"], true);

    // Same stage again: no-op, no new fact.
    let (stage_count_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let stage_resp2 = common::post_json_with_cookie(
        &router,
        &format!("/api/people/{person_id}/stage"),
        &cookie,
        json!({ "stage_id": second_stage_id }),
    )
    .await;
    assert_eq!(common::body_json(stage_resp2).await["changed"], false);
    let (stage_count_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage_changed WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(stage_count_before, stage_count_after);

    // History ordering: intake's four facts (occurred_at/recorded_at tied,
    // ordered by kind_rank), then the manual assignment, then the manual
    // stage change — in call order.
    let detail_body = common::body_json(
        common::get_with_cookie(&router, &format!("/api/people/{person_id}"), &cookie).await,
    )
    .await;
    let history = detail_body["history"].as_array().unwrap();
    assert_eq!(
        history.len(),
        6,
        "4 intake facts + 1 manual assignment + 1 manual stage change"
    );
    let kinds: Vec<&str> = history
        .iter()
        .map(|h| h["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "inquiry_received",
            "routing_decision",
            "assignment_changed",
            "stage_changed",
            "assignment_changed",
            "stage_changed",
        ]
    );
    assert_eq!(history[4]["detail"]["reason"], "manual");
    assert_eq!(history[5]["detail"]["reason"], "manual");
}

async fn insert_bare_person(pool: &PgPool, org_id: Uuid, stage_id: Uuid, name: &str) {
    sqlx::query("INSERT INTO person (organization_id, first_name, stage_id) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(name)
        .bind(stage_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Criterion 19 (people list): `truncated: true` past 500 rows.
#[sqlx::test]
#[ignore]
async fn people_list_reports_truncated_past_500_rows(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    for i in 0..501 {
        insert_bare_person(&migrator_pool, org_id, stage_id, &format!("Person{i}")).await;
    }

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let body =
        common::body_json(common::get_with_cookie(&router, "/api/people", &cookie).await).await;
    assert_eq!(body["truncated"], true);
    assert_eq!(body["people"].as_array().unwrap().len(), 500);
}

/// Criterion 19 (people list): `truncated: false` at or under 500 rows.
#[sqlx::test]
#[ignore]
async fn people_list_reports_not_truncated_under_500_rows(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;
    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    for i in 0..3 {
        insert_bare_person(&migrator_pool, org_id, stage_id, &format!("Person{i}")).await;
    }

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let body =
        common::body_json(common::get_with_cookie(&router, "/api/people", &cookie).await).await;
    assert_eq!(body["truncated"], false);
    assert_eq!(body["people"].as_array().unwrap().len(), 3);
}

async fn insert_bare_unresolved_raw_payload(pool: &PgPool, org_id: Uuid, distinct_hmac: &[u8]) {
    sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution, unresolved_reason)
           VALUES ($1, $2, 'zillow', 'generic_v1', 'web_session', now(), $3, $4, $5, 10, 'unresolved', 'no_contact_method')"#,
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(vec![0u8; 24])
    .bind(vec![0u8; 26])
    .bind(distinct_hmac)
    .execute(pool)
    .await
    .unwrap();
}

/// Criterion 19 (unresolved queue): `truncated: true` past 500 rows,
/// `false` at or under.
#[sqlx::test]
#[ignore]
async fn unresolved_queue_reports_truncated_past_500_rows(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;

    for i in 0u32..501 {
        insert_bare_unresolved_raw_payload(&migrator_pool, org_id, &i.to_le_bytes()).await;
    }

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let body = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &cookie).await,
    )
    .await;
    assert_eq!(body["truncated"], true);
    assert_eq!(body["items"].as_array().unwrap().len(), 500);
}

#[sqlx::test]
#[ignore]
async fn unresolved_queue_reports_not_truncated_under_500_rows(migrator_pool: PgPool) {
    let (org_id, _alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        "pw",
    )
    .await;

    for i in 0u32..3 {
        insert_bare_unresolved_raw_payload(&migrator_pool, org_id, &i.to_le_bytes()).await;
    }

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "alice@acme.test", "pw").await;
    let body = common::body_json(
        common::get_with_cookie(&router, "/api/intake/unresolved", &cookie).await,
    )
    .await;
    assert_eq!(body["truncated"], false);
    assert_eq!(body["items"].as_array().unwrap().len(), 3);
}

/// Criterion 14: creating an Organization through the platform-admin HTTP
/// flow (`common::create_org_with_admin_and_member_via_api` — the same
/// sequence `scripts/dev-bootstrap` drives) seeds its nine D-019 default
/// stages, in order, and its two invited members; repeating the creation
/// is rejected, not a second set of stages or memberships.
#[sqlx::test]
#[ignore]
async fn organization_creation_seeds_nine_ordered_stages_and_two_members(migrator_pool: PgPool) {
    const PW: &str = "test-seed-password-123456";

    common::create_platform_admin(&migrator_pool, "owner@platform.test", "Platform Owner", PW)
        .await;
    let router = common::build_router(&migrator_pool).await;
    let platform_cookie = common::login_cookie(&router, "owner@platform.test", PW).await;

    let acme_id = common::create_org_with_admin_and_member_via_api(
        &router,
        &platform_cookie,
        PW,
        "Acme Realty",
        common::SeedPerson {
            email: "alice@acme.test",
            display_name: "Alice Anderson",
        },
        common::SeedPerson {
            email: "carol@acme.test",
            display_name: "Carol Chen",
        },
    )
    .await;
    let best_id = common::create_org_with_admin_and_member_via_api(
        &router,
        &platform_cookie,
        PW,
        "Best Realty",
        common::SeedPerson {
            email: "bob@best.test",
            display_name: "Bob Baker",
        },
        common::SeedPerson {
            email: "dave@best.test",
            display_name: "Dave Diaz",
        },
    )
    .await;

    let (org_count,): (i64,) = sqlx::query_as("SELECT count(*) FROM organization")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(org_count, 2);

    for org_id_str in [&acme_id, &best_id] {
        let org_id: Uuid = org_id_str.parse().unwrap();
        let (stage_count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM stage WHERE organization_id = $1")
                .bind(org_id)
                .fetch_one(&migrator_pool)
                .await
                .unwrap();
        assert_eq!(stage_count, 9);

        let stage_rows: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM stage WHERE organization_id = $1 ORDER BY position")
                .bind(org_id)
                .fetch_all(&migrator_pool)
                .await
                .unwrap();
        let stage_names: Vec<String> = stage_rows.into_iter().map(|(n,)| n).collect();
        assert_eq!(
            stage_names,
            vec![
                "Lead",
                "Hot Prospect",
                "Nurture",
                "Active Client",
                "Pending",
                "Closed",
                "Past Client",
                "Sphere",
                "Trash",
            ],
            "D-019 order"
        );

        let (member_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM organization_membership WHERE organization_id = $1",
        )
        .bind(org_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(member_count, 2);
    }

    // Repeating the creation is a clean rejection, never a second set of
    // stages or memberships (docs/specs/SLICE_004.md §4).
    let dup = common::post_json_with_cookie(
        &router,
        "/api/platform/organizations",
        &platform_cookie,
        json!({ "name": "Acme Realty" }),
    )
    .await;
    assert_eq!(dup.status(), StatusCode::CONFLICT);

    let acme_uuid: Uuid = acme_id.parse().unwrap();
    let (stage_count_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM stage WHERE organization_id = $1")
            .bind(acme_uuid)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        stage_count_after, 9,
        "no duplicate stages after a rejected repeat creation"
    );
}

// --- Malformed-request 400s requiring a real session ---------------------
//
// AuthContext follows Path but precedes the body extractor on these routes
// (routes/people.rs's ordering comment), so a malformed *body* — unlike a
// malformed path segment — can only be exercised with a working session;
// hence these live here rather than in tests/people.rs.

#[sqlx::test]
#[ignore]
async fn set_assignment_with_malformed_json_returns_400(migrator_pool: PgPool) {
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
    let person_id = Uuid::new_v4();

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/people/{person_id}/assignment"))
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn set_stage_with_malformed_json_returns_400(migrator_pool: PgPool) {
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
    let person_id = Uuid::new_v4();

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/people/{person_id}/stage"))
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"stage_id": "not-a-uuid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

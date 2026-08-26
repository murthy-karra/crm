//! DB-backed tests for Slice 007c's `GET`/`PUT
//! /api/organization/intake-settings` and its migration (docs/specs/
//! SLICE_007c.md §11, acceptance criteria 1, 2, 10, 11 —
//! `db_intake_address.rs` pattern). Run only via ./scripts/check-db.
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};

const PW: &str = "pw";

async fn stored_default(pool: &PgPool, org_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT intake_default_assignee_user_id FROM organization WHERE id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Criterion 11: an org admin can GET/PUT their own setting; a member is
/// 403 on both; the setting never crosses Organizations.
#[sqlx::test]
#[ignore]
async fn admins_manage_the_setting_members_are_403_and_orgs_never_cross(migrator_pool: PgPool) {
    let acme = common::create_org(&migrator_pool, "Acme Realty").await;
    let best = common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let bob_id = common::create_user(&migrator_pool, "bob@best.test", "Bob", PW).await;
    let carol_id = common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    common::add_membership_with(
        &migrator_pool,
        acme,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership_with(
        &migrator_pool,
        best,
        bob_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership(&migrator_pool, acme, carol_id).await;

    let router = common::build_router(&migrator_pool).await;
    let alice = common::login_cookie(&router, "alice@acme.test", PW).await;
    let bob = common::login_cookie(&router, "bob@best.test", PW).await;
    let carol = common::login_cookie(&router, "carol@acme.test", PW).await;

    // Unset initially.
    let resp = common::get_with_cookie(&router, "/api/organization/intake-settings", &alice).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "intake_default_assignee_user_id": null })
    );

    // Alice sets Acme's default to herself.
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_default_assignee_user_id": alice_id }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "intake_default_assignee_user_id": alice_id })
    );

    // Carol (member, not admin) is 403 on both.
    let resp = common::get_with_cookie(&router, "/api/organization/intake-settings", &carol).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(common::body_json(resp).await, json!({"error": "forbidden"}));
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &carol,
        json!({ "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Bob (Best's admin) never sees Acme's setting, and Best's own setting
    // is independently unset.
    let resp = common::get_with_cookie(&router, "/api/organization/intake-settings", &bob).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "intake_default_assignee_user_id": null })
    );
    assert_eq!(stored_default(&migrator_pool, best).await, None);
    assert_eq!(stored_default(&migrator_pool, acme).await, Some(alice_id));
}

/// Criterion 10: nonexistent, foreign, and inactive-member values all
/// produce the identical 422 `invalid_assignee` (no existence leak).
#[sqlx::test]
#[ignore]
async fn put_rejects_nonexistent_foreign_and_inactive_members_identically(migrator_pool: PgPool) {
    let acme = common::create_org(&migrator_pool, "Acme Realty").await;
    let best = common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let dave_id = common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    let erin_id = common::create_user(&migrator_pool, "erin@best.test", "Erin", PW).await;
    common::add_membership_with(
        &migrator_pool,
        acme,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership_with(
        &migrator_pool,
        acme,
        dave_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    common::add_membership_with(
        &migrator_pool,
        best,
        erin_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let router = common::build_router(&migrator_pool).await;
    let alice = common::login_cookie(&router, "alice@acme.test", PW).await;

    let mut bodies = Vec::new();
    for candidate in [Uuid::new_v4(), erin_id, dave_id] {
        let resp = common::put_json_with_cookie(
            &router,
            "/api/organization/intake-settings",
            &alice,
            json!({ "intake_default_assignee_user_id": candidate }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        bodies.push(common::body_json(resp).await);
    }
    assert!(bodies
        .iter()
        .all(|b| *b == json!({"error": "invalid_assignee"})));
    assert!(
        bodies.windows(2).all(|w| w[0] == w[1]),
        "byte-identical across nonexistent/foreign/inactive"
    );
    assert_eq!(
        stored_default(&migrator_pool, acme).await,
        None,
        "no write on rejection"
    );
}

/// Criterion 10 continued: explicit `null` clears; an absent key is 400
/// `malformed_request` (never a silent clear); a malformed UUID string is
/// 400; GET keeps reflecting the stored value after the member is later
/// deactivated (the UI's warning depends on this).
#[sqlx::test]
#[ignore]
async fn put_null_clears_absent_key_and_bad_uuid_are_400_get_survives_deactivation(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let alice_id = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    common::add_membership_with(
        &migrator_pool,
        org_id,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let bob_id = common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    common::add_membership(&migrator_pool, org_id, bob_id).await;

    let router = common::build_router(&migrator_pool).await;
    let alice = common::login_cookie(&router, "alice@acme.test", PW).await;

    // Absent key -> 400, no write.
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(stored_default(&migrator_pool, org_id).await, None);

    // Malformed UUID -> 400.
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_default_assignee_user_id": "not-a-uuid" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Set to bob, then deactivate bob: GET still reflects bob (retained).
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_default_assignee_user_id": bob_id }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let deactivate = common::put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{bob_id}/status"),
        &alice,
        json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivate.status(), StatusCode::OK);

    let resp = common::get_with_cookie(&router, "/api/organization/intake-settings", &alice).await;
    assert_eq!(
        common::body_json(resp).await,
        json!({ "intake_default_assignee_user_id": bob_id }),
        "deactivation does not clear the setting"
    );

    // Explicit null clears.
    let resp = common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(stored_default(&migrator_pool, org_id).await, None);
}

// --- Schema (docs/specs/SLICE_007c.md §3) -----------------------------------

/// Criterion 1: `crm_app`'s UPDATE grant on `organization` is exactly
/// `(intake_default_assignee_user_id, updated_at)` — it can write the new
/// column but stays denied on every other one, `intake_slug`/`intake_token`
/// included (007a's grant, unchanged by this migration).
#[sqlx::test]
#[ignore]
async fn crm_app_update_grant_is_scoped_to_the_new_column_and_updated_at(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    let app_pool = common::connect_as_app(&migrator_pool).await;

    // Allowed.
    sqlx::query("UPDATE organization SET intake_default_assignee_user_id = NULL, updated_at = now() WHERE id = $1")
        .bind(org_id)
        .execute(&app_pool)
        .await
        .unwrap();

    // Denied. (intake_token left this list in SLICE_007g — rotation
    // granted its UPDATE; pinned in db_intake_rotation.rs. Declared
    // amendment.)
    for (col, value) in [("name", "'Renamed'"), ("intake_slug", "'x'")] {
        let err = sqlx::query(&format!(
            "UPDATE organization SET {col} = {value} WHERE false"
        ))
        .execute(&app_pool)
        .await
        .unwrap_err();
        let db = err.as_database_error().expect("a permission error");
        assert_eq!(db.code().as_deref(), Some("42501"), "{col}");
    }
}

/// Criterion 2: `routing_decision.strategy` accepts both new values and
/// still rejects an unrecognized one.
#[sqlx::test]
#[ignore]
async fn routing_decision_strategy_check_accepts_new_values_and_rejects_unknown(
    migrator_pool: PgPool,
) {
    let (org_id, user_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id)
         SELECT $1, id, $2 FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1
         RETURNING id",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    // `inquiry.raw_payload_id` carries no FK (docs/specs/SLICE_002.md §2's
    // schema; `db_today.rs::insert_inquiry` relies on the same fact) — a
    // fresh random id is a valid fixture value here.
    let inquiry_id: Uuid = sqlx::query_scalar(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, 'website', now()) RETURNING id",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    for strategy in ["organization_default", "unassigned"] {
        sqlx::query(
            "INSERT INTO routing_decision
                (organization_id, actor_kind, actor_user_id, origin, occurred_at,
                 correlation_id, inquiry_id, person_id, strategy, assignee_user_id)
             VALUES ($1, 'system', NULL, 'cli', now(), gen_random_uuid(), $2, $3, $4, NULL)",
        )
        .bind(org_id)
        .bind(inquiry_id)
        .bind(person_id)
        .bind(strategy)
        .execute(&migrator_pool)
        .await
        .unwrap_or_else(|err| panic!("{strategy} must be accepted: {err}"));
    }

    let err = sqlx::query(
        "INSERT INTO routing_decision
            (organization_id, actor_kind, actor_user_id, origin, occurred_at,
             correlation_id, inquiry_id, person_id, strategy, assignee_user_id)
         VALUES ($1, 'system', NULL, 'cli', now(), gen_random_uuid(), $2, $3, 'bogus', NULL)",
    )
    .bind(org_id)
    .bind(inquiry_id)
    .bind(person_id)
    .execute(&migrator_pool)
    .await
    .unwrap_err();
    let db = err.as_database_error().expect("a CHECK violation");
    assert_eq!(db.code().as_deref(), Some("23514"));
    assert_eq!(db.constraint(), Some("routing_decision_strategy_check"));
}

//! DB-backed tests for the `GET`/`PUT /api/organization/intake-settings`
//! endpoints and their migrations (`db_intake_address.rs` pattern). Run
//! only via ./scripts/check-db.
//!
//! Slice 008 (docs/specs/SLICE_008.md §5, D-041) supersedes SLICE_007c
//! §5's single-key PUT body with a two-key, both-required body
//! (`intake_routing_mode` + `intake_default_assignee_user_id`) — the PUT
//! tests below are rewritten for the new shape and validation matrix
//! (declared amendment, SLICE_007c §5 pointer). The schema/CHECK tests at
//! the bottom of this file are UNCHANGED 007c pins (the regression gate);
//! new Slice 008 schema coverage (the `round_robin` CHECK value, the new
//! grants) is added as new, separate tests alongside them rather than
//! folded into the existing ones.

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

async fn stored_mode(pool: &PgPool, org_id: Uuid) -> String {
    sqlx::query_scalar("SELECT intake_routing_mode FROM organization WHERE id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Criterion 11: an org admin can GET/PUT their own setting; a member is
/// 403 on both; the setting never crosses Organizations. New orgs default
/// to `unassigned` mode (D-041; mirrors 007c's old no-default state).
#[sqlx::test]
#[ignore]
async fn admins_manage_the_setting_members_are_403_and_orgs_never_cross(migrator_pool: PgPool) {
    let acme = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let best = crate::common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@best.test", "Bob", PW).await;
    let carol_id = crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        acme,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        best,
        bob_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership(&migrator_pool, acme, carol_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob@best.test", PW).await;
    let carol = crate::common::login_cookie(&router, "carol@acme.test", PW).await;

    // Unset initially: unassigned mode, null assignee.
    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-settings", &alice).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "intake_routing_mode": "unassigned", "intake_default_assignee_user_id": null })
    );

    // Alice sets Acme's mode to default_assignee, herself as the assignee.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": alice_id,
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": alice_id,
        })
    );

    // Carol (member, not admin) is 403 on both.
    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-settings", &carol).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({"error": "forbidden"})
    );
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &carol,
        json!({ "intake_routing_mode": "unassigned", "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Bob (Best's admin) never sees Acme's setting, and Best's own setting
    // is independently unset.
    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-settings", &bob).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "intake_routing_mode": "unassigned", "intake_default_assignee_user_id": null })
    );
    assert_eq!(stored_default(&migrator_pool, best).await, None);
    assert_eq!(stored_default(&migrator_pool, acme).await, Some(alice_id));
    assert_eq!(stored_mode(&migrator_pool, acme).await, "default_assignee");
    assert_eq!(stored_mode(&migrator_pool, best).await, "unassigned");
}

/// Criterion 10 (`default_assignee` mode): a nonexistent user, another
/// Organization's member, an inactive member, and a null assignee (the
/// mode can never itself clear the assignee — spec §5) all produce the
/// identical 422 `invalid_assignee` (no existence leak), and none of them
/// writes anything.
#[sqlx::test]
#[ignore]
async fn put_default_assignee_mode_rejects_nonexistent_foreign_inactive_and_null_identically(
    migrator_pool: PgPool,
) {
    let acme = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let best = crate::common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    let erin_id = crate::common::create_user(&migrator_pool, "erin@best.test", "Erin", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        acme,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        acme,
        dave_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        best,
        erin_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let mut bodies = Vec::new();
    for candidate in [
        json!(Uuid::new_v4()),
        json!(erin_id),
        json!(dave_id),
        json!(null),
    ] {
        let resp = crate::common::put_json_with_cookie(
            &router,
            "/api/organization/intake-settings",
            &alice,
            json!({
                "intake_routing_mode": "default_assignee",
                "intake_default_assignee_user_id": candidate,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        bodies.push(crate::common::body_json(resp).await);
    }
    assert!(bodies
        .iter()
        .all(|b| *b == json!({"error": "invalid_assignee"})));
    assert!(
        bodies.windows(2).all(|w| w[0] == w[1]),
        "byte-identical across nonexistent/foreign/inactive/null"
    );
    assert_eq!(
        stored_default(&migrator_pool, acme).await,
        None,
        "no write on rejection"
    );
    assert_eq!(
        stored_mode(&migrator_pool, acme).await,
        "unassigned",
        "no write on rejection"
    );
}

/// Non-`default_assignee` modes (`round_robin`/`unassigned`, reviewer S1):
/// `null`, an active member, or the CURRENTLY-STORED value verbatim are
/// all accepted — the stale-echo case lets an org whose default
/// deactivated flip modes without first clearing it — but the echoed
/// value is checked against the ACTUAL stored value, not merely trusted
/// from the client, so a cross-org id, a nonexistent id, or a value that
/// simply isn't what's stored is 422 `invalid_assignee`, identically, and
/// persists nothing.
#[sqlx::test]
#[ignore]
async fn put_non_default_modes_accept_null_active_or_stale_echo_reject_anything_else(
    migrator_pool: PgPool,
) {
    let acme = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let best = crate::common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    let erin_id = crate::common::create_user(&migrator_pool, "erin@best.test", "Erin", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        acme,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        acme,
        dave_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        best,
        erin_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    // Establish dave as the stored default_assignee-mode value, then
    // deactivate him — a stale-but-stored value.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": dave_id,
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let deactivate = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{dave_id}/status"),
        &alice,
        json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivate.status(), StatusCode::OK);

    // Switching to round_robin, echoing dave (now inactive) back
    // verbatim: accepted.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "round_robin", "intake_default_assignee_user_id": dave_id }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a stale echo of the currently-stored value is accepted"
    );
    assert_eq!(stored_mode(&migrator_pool, acme).await, "round_robin");
    assert_eq!(stored_default(&migrator_pool, acme).await, Some(dave_id));

    // Switching to unassigned with null: accepted.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "unassigned", "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(stored_default(&migrator_pool, acme).await, None);

    // Switching to round_robin with an ACTIVE member: accepted.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "round_robin", "intake_default_assignee_user_id": alice_id }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // The stored value is now alice_id (active). A DIFFERENT non-matching
    // value — cross-org erin, a nonexistent uuid, and dave (inactive AND
    // no longer the stored value) — is 422 identically; nothing changes.
    let mut bodies = Vec::new();
    for candidate in [json!(erin_id), json!(Uuid::new_v4()), json!(dave_id)] {
        let resp = crate::common::put_json_with_cookie(
            &router,
            "/api/organization/intake-settings",
            &alice,
            json!({
                "intake_routing_mode": "round_robin",
                "intake_default_assignee_user_id": candidate,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        bodies.push(crate::common::body_json(resp).await);
    }
    assert!(bodies
        .iter()
        .all(|b| *b == json!({"error": "invalid_assignee"})));
    assert_eq!(
        stored_default(&migrator_pool, acme).await,
        Some(alice_id),
        "no write on rejection"
    );
    assert_eq!(
        stored_mode(&migrator_pool, acme).await,
        "round_robin",
        "no write on rejection"
    );
}

/// Both keys are required (either absent → 400, never a silent clear or a
/// silent mode-keep); the pre-008 SLICE_007c single-key body is 400
/// (declared breaking supersession — SLICE_007c §5 pointer amendment); an
/// unknown mode string and a malformed UUID string are both 400; clearing
/// is expressed by switching mode with `null` — remaining in
/// `default_assignee` mode can never itself clear (422); GET keeps
/// reflecting the stored value after the assignee is later deactivated
/// (the UI's warning depends on this).
#[sqlx::test]
#[ignore]
async fn put_requires_both_keys_rejects_the_old_007c_body_and_get_survives_deactivation(
    migrator_pool: PgPool,
) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        org_id,
        alice_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    crate::common::add_membership(&migrator_pool, org_id, bob_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    // Both keys absent.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Mode present, assignee key absent.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "unassigned" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // The pre-008 007c single-key body (mode key absent): 400, never
    // silently interpreted as "keep the current mode".
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_default_assignee_user_id": bob_id }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        stored_default(&migrator_pool, org_id).await,
        None,
        "no write from any rejected body"
    );

    // Unknown mode string -> 400.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "rules", "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Malformed UUID -> 400.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": "not-a-uuid",
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Set to bob (default_assignee mode), then deactivate bob: GET still
    // reflects bob (retained).
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": bob_id,
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let deactivate = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{bob_id}/status"),
        &alice,
        json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivate.status(), StatusCode::OK);

    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-settings", &alice).await;
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({
            "intake_routing_mode": "default_assignee",
            "intake_default_assignee_user_id": bob_id,
        }),
        "deactivation does not clear the setting"
    );

    // Remaining in default_assignee mode can never clear (422) — clearing
    // is expressed by switching mode.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "default_assignee", "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Switching mode WITH null clears.
    let resp = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/intake-settings",
        &alice,
        json!({ "intake_routing_mode": "unassigned", "intake_default_assignee_user_id": null }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(stored_default(&migrator_pool, org_id).await, None);
}

// --- Schema (docs/specs/SLICE_007c.md §3, docs/specs/SLICE_008.md §3) -----

/// Criterion 1 (007c, UNCHANGED pin): `crm_app`'s UPDATE grant on
/// `organization` covers `(intake_default_assignee_user_id, updated_at)`
/// — it can write those but stays denied on every other column,
/// `intake_slug`/`intake_token` included (007a's grant, unchanged by this
/// migration). Slice 008 adds a SEPARATE grant on `intake_routing_mode`
/// (tested below) rather than widening this one, so this pin is exactly
/// as it was.
#[sqlx::test]
#[ignore]
async fn crm_app_update_grant_is_scoped_to_the_new_column_and_updated_at(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

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

/// Criterion 2 (007c, UNCHANGED pin): `routing_decision.strategy` accepts
/// both 007c values and still rejects an unrecognized one.
#[sqlx::test]
#[ignore]
async fn routing_decision_strategy_check_accepts_new_values_and_rejects_unknown(
    migrator_pool: PgPool,
) {
    let (org_id, user_id) = crate::common::create_org_with_stages_and_member(
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

/// Slice 008 extension (new, does not touch the 007c pin above): the
/// widened CHECK also accepts `round_robin`, with a non-NULL assignee.
#[sqlx::test]
#[ignore]
async fn routing_decision_strategy_check_accepts_round_robin(migrator_pool: PgPool) {
    let (org_id, user_id) = crate::common::create_org_with_stages_and_member(
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

    sqlx::query(
        "INSERT INTO routing_decision
            (organization_id, actor_kind, actor_user_id, origin, occurred_at,
             correlation_id, inquiry_id, person_id, strategy, assignee_user_id)
         VALUES ($1, 'system', NULL, 'cli', now(), gen_random_uuid(), $2, $3, 'round_robin', $4)",
    )
    .bind(org_id)
    .bind(inquiry_id)
    .bind(person_id)
    .bind(user_id)
    .execute(&migrator_pool)
    .await
    .unwrap_or_else(|err| panic!("round_robin must be accepted: {err}"));
}

/// Slice 008 extension: `crm_app`'s new `intake_routing_mode` grant, and
/// the `intake_rotation` table's SELECT/INSERT/UPDATE grant.
#[sqlx::test]
#[ignore]
async fn crm_app_can_write_intake_routing_mode_and_the_rotation_table(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    crate::common::add_membership(&migrator_pool, org_id, alice_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    sqlx::query("UPDATE organization SET intake_routing_mode = 'round_robin' WHERE id = $1")
        .bind(org_id)
        .execute(&app_pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO intake_rotation (organization_id, last_assigned_user_id)
         VALUES ($1, $2)
         ON CONFLICT (organization_id)
         DO UPDATE SET last_assigned_user_id = excluded.last_assigned_user_id",
    )
    .bind(org_id)
    .bind(alice_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let (stored,): (Uuid,) = sqlx::query_as(
        "SELECT last_assigned_user_id FROM intake_rotation WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(stored, alice_id);
}

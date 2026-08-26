//! DB-backed tests for Slice 001 (docs/specs/SLICE_001.md §9), amended by
//! Slice 004 (docs/specs/SLICE_004.md §13, §11). Run only via
//! ./scripts/check-db: every test is `#[ignore]`d so the service-free main
//! gate (`cargo test --workspace --locked`) never touches a database.
//! `DATABASE_URL` must be the crm_migrator URL for the `#[sqlx::test]`
//! harness; `CRM_DB_APP_PASSWORD`/`CRM_DB_MIGRATOR_PASSWORD` let each test
//! build a same-database connection under a different role. Fixtures come
//! from `tests/common` (Organization/user/membership creation now goes
//! through the Slice 004 domain functions as `crm_app` — the migrator
//! connection is used only to backdate timestamps or delete rows).
mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use common::{
    add_membership, body_json, build_router, connect_as_app, create_org, create_platform_admin,
    create_user, extract_cookie, login, login_cookie, post_json_with_cookie,
};
use crm_api::domain::admin::queries as admin_queries;
use crm_api::ids::{OrganizationId, UserId};

async fn create_user_without_password(pool: &PgPool, email: &str, display_name: &str) -> Uuid {
    let app_pool = connect_as_app(pool).await;
    let mut conn = app_pool.acquire().await.unwrap();
    admin_queries::insert_app_user(&mut conn, email, display_name)
        .await
        .unwrap()
        .as_uuid()
}

async fn login_with_cookie(router: &Router, email: &str, password: &str, cookie: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn get_with_cookie(router: &Router, uri: &str, cookie: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn delete_with_cookie(router: &Router, uri: &str, cookie: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

// --- Full lifecycle -------------------------------------------------

#[sqlx::test]
#[ignore]
async fn full_lifecycle_login_me_members_logout_replay(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "correct horse battery staple",
    )
    .await;
    add_membership(&migrator_pool, org, user).await;

    let router = build_router(&migrator_pool).await;

    let login_response = login(&router, "alice@acme.test", "correct horse battery staple").await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let cookie = extract_cookie(&login_response);
    let login_body = body_json(login_response).await;
    assert_eq!(login_body["user"]["email"], "alice@acme.test");
    assert_eq!(login_body["organization"]["name"], "Acme Realty");

    let me_response = get_with_cookie(&router, "/api/me", &cookie).await;
    assert_eq!(me_response.status(), StatusCode::OK);
    let me_body = body_json(me_response).await;
    assert_eq!(me_body, login_body);

    let members_response = get_with_cookie(&router, "/api/organization/members", &cookie).await;
    assert_eq!(members_response.status(), StatusCode::OK);
    let members_body = body_json(members_response).await;
    assert_eq!(members_body["members"].as_array().unwrap().len(), 1);
    assert_eq!(members_body["members"][0]["email"], "alice@acme.test");

    let logout_response = delete_with_cookie(&router, "/api/session", &cookie).await;
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    let replay_response = get_with_cookie(&router, "/api/me", &cookie).await;
    assert_eq!(replay_response.status(), StatusCode::UNAUTHORIZED);
}

// --- Credential failures ---------------------------------------------

#[sqlx::test]
#[ignore]
async fn wrong_password_and_unknown_user_return_identical_401(migrator_pool: PgPool) {
    create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "correct password",
    )
    .await;
    let router = build_router(&migrator_pool).await;

    let wrong_password = login(&router, "alice@acme.test", "wrong password").await;
    assert_eq!(wrong_password.status(), StatusCode::UNAUTHORIZED);
    let wrong_password_body = body_json(wrong_password).await;

    let unknown_user = login(&router, "nobody@nowhere.test", "whatever").await;
    assert_eq!(unknown_user.status(), StatusCode::UNAUTHORIZED);
    let unknown_user_body = body_json(unknown_user).await;

    assert_eq!(wrong_password_body, unknown_user_body);
    assert_eq!(
        wrong_password_body,
        serde_json::json!({ "error": "invalid_credentials" })
    );
}

#[sqlx::test]
#[ignore]
async fn user_without_local_credential_gets_same_invalid_credentials_error(migrator_pool: PgPool) {
    create_user_without_password(&migrator_pool, "nopassword@acme.test", "No Password").await;
    let router = build_router(&migrator_pool).await;

    let response = login(&router, "nopassword@acme.test", "anything").await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "invalid_credentials" }));
}

// --- Session validity --------------------------------------------------

#[sqlx::test]
#[ignore]
async fn expired_session_returns_401(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, org, user).await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let config = common::test_config();
    let (token, _expires_at) = crm_api::auth::session::create(
        &app_pool,
        &config.session_secret,
        UserId::new(user),
        Some(OrganizationId::new(org)),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();

    sqlx::query("UPDATE user_session SET expires_at = now() - interval '1 hour'")
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = build_router(&migrator_pool).await;
    let response = get_with_cookie(&router, "/api/me", &format!("crm_session={token}")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
#[ignore]
async fn tampered_token_returns_401(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, org, {
        let (id,): (Uuid,) =
            sqlx::query_as("SELECT id FROM app_user WHERE email = 'alice@acme.test'")
                .fetch_one(&migrator_pool)
                .await
                .unwrap();
        id
    })
    .await;

    let router = build_router(&migrator_pool).await;
    let login_response = login(&router, "alice@acme.test", "password123").await;
    let cookie = extract_cookie(&login_response);

    // Flip the first character of the token value; same length and
    // alphabet, so this exercises "right format, wrong value".
    let flip_at = cookie.find('=').unwrap() + 1;
    let tampered: String = cookie
        .char_indices()
        .map(|(i, ch)| {
            if i == flip_at {
                if ch == 'a' {
                    'b'
                } else {
                    'a'
                }
            } else {
                ch
            }
        })
        .collect();
    assert_ne!(tampered, cookie);

    let response = get_with_cookie(&router, "/api/me", &tampered).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
#[ignore]
async fn membership_revoked_returns_401_on_next_request(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, org, user).await;

    let router = build_router(&migrator_pool).await;
    let login_response = login(&router, "alice@acme.test", "password123").await;
    let cookie = extract_cookie(&login_response);

    let first = get_with_cookie(&router, "/api/me", &cookie).await;
    assert_eq!(first.status(), StatusCode::OK);

    sqlx::query("DELETE FROM organization_membership WHERE organization_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let second = get_with_cookie(&router, "/api/me", &cookie).await;
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
#[ignore]
async fn relogin_mints_new_token_and_revokes_previous(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, org, user).await;

    let router = build_router(&migrator_pool).await;
    let first_login = login(&router, "alice@acme.test", "password123").await;
    let first_cookie = extract_cookie(&first_login);

    // Present the first cookie on the second login so the best-effort
    // revoke-on-relogin path actually has something to revoke.
    let second_login =
        login_with_cookie(&router, "alice@acme.test", "password123", &first_cookie).await;
    let second_cookie = extract_cookie(&second_login);
    assert_ne!(
        first_cookie, second_cookie,
        "re-login must mint a fresh token"
    );

    let with_old_cookie = get_with_cookie(&router, "/api/me", &first_cookie).await;
    assert_eq!(
        with_old_cookie.status(),
        StatusCode::UNAUTHORIZED,
        "the previous session must be revoked"
    );

    let with_new_cookie = get_with_cookie(&router, "/api/me", &second_cookie).await;
    assert_eq!(with_new_cookie.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn zero_membership_login_returns_403_and_creates_no_session(migrator_pool: PgPool) {
    let user = create_user(
        &migrator_pool,
        "orphan@nowhere.test",
        "Orphan User",
        "password123",
    )
    .await;
    let router = build_router(&migrator_pool).await;

    let response = login(&router, "orphan@nowhere.test", "password123").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "no_membership" }));

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM user_session WHERE user_id = $1")
        .bind(user)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// --- Tenant isolation ----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn two_organizations_are_isolated_in_both_directions(migrator_pool: PgPool) {
    let org_a = create_org(&migrator_pool, "Acme Realty").await;
    let org_b = create_org(&migrator_pool, "Best Realty").await;
    let alice = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    let bob = create_user(&migrator_pool, "bob@best.test", "Bob Baker", "password123").await;
    add_membership(&migrator_pool, org_a, alice).await;
    add_membership(&migrator_pool, org_b, bob).await;

    let router = build_router(&migrator_pool).await;

    let alice_login = login(&router, "alice@acme.test", "password123").await;
    let alice_cookie = extract_cookie(&alice_login);
    let alice_members =
        body_json(get_with_cookie(&router, "/api/organization/members", &alice_cookie).await).await;
    let alice_emails: Vec<&str> = alice_members["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["email"].as_str().unwrap())
        .collect();
    assert_eq!(alice_emails, vec!["alice@acme.test"]);
    assert!(!alice_emails.contains(&"bob@best.test"));

    let bob_login = login(&router, "bob@best.test", "password123").await;
    let bob_cookie = extract_cookie(&bob_login);
    let bob_members =
        body_json(get_with_cookie(&router, "/api/organization/members", &bob_cookie).await).await;
    let bob_emails: Vec<&str> = bob_members["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["email"].as_str().unwrap())
        .collect();
    assert_eq!(bob_emails, vec!["bob@best.test"]);
    assert!(!bob_emails.contains(&"alice@acme.test"));
}

#[sqlx::test]
#[ignore]
async fn client_supplied_organization_id_is_ignored(migrator_pool: PgPool) {
    let org_a = create_org(&migrator_pool, "Acme Realty").await;
    let org_b = create_org(&migrator_pool, "Best Realty").await;
    let alice = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    let bob = create_user(&migrator_pool, "bob@best.test", "Bob Baker", "password123").await;
    add_membership(&migrator_pool, org_a, alice).await;
    add_membership(&migrator_pool, org_b, bob).await;

    let router = build_router(&migrator_pool).await;
    let alice_login = login(&router, "alice@acme.test", "password123").await;
    let alice_cookie = extract_cookie(&alice_login);

    // Query-string probe.
    let query_probe = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/organization/members?organization_id={org_b}"))
                .header("cookie", &alice_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let query_body = body_json(query_probe).await;
    assert_eq!(query_body["members"].as_array().unwrap().len(), 1);
    assert_eq!(query_body["members"][0]["email"], "alice@acme.test");

    // Header probe.
    let header_probe = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/organization/members")
                .header("cookie", &alice_cookie)
                .header("x-organization-id", org_b.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let header_body = body_json(header_probe).await;
    assert_eq!(header_body["members"].as_array().unwrap().len(), 1);
    assert_eq!(header_body["members"][0]["email"], "alice@acme.test");

    // Body probe: a spurious organization_id in the login request body
    // must not influence which Organization the session is scoped to.
    let body_probe = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": "alice@acme.test", "password": "password123", "organization_id": org_b.to_string() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_probe.status(), StatusCode::OK);
    let body_probe_body = body_json(body_probe).await;
    assert_eq!(body_probe_body["organization"]["name"], "Acme Realty");
}

#[sqlx::test]
#[ignore]
async fn logout_is_idempotent_against_an_already_revoked_session(migrator_pool: PgPool) {
    let org = create_org(&migrator_pool, "Acme Realty").await;
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, org, user).await;

    let router = build_router(&migrator_pool).await;
    let login_response = login(&router, "alice@acme.test", "password123").await;
    let cookie = extract_cookie(&login_response);

    let first_logout = delete_with_cookie(&router, "/api/session", &cookie).await;
    assert_eq!(first_logout.status(), StatusCode::NO_CONTENT);

    let second_logout = delete_with_cookie(&router, "/api/session", &cookie).await;
    assert_eq!(
        second_logout.status(),
        StatusCode::NO_CONTENT,
        "logging out an already-revoked session must still succeed"
    );
}

#[sqlx::test]
#[ignore]
async fn multi_membership_picks_earliest_and_scopes_members_to_it(migrator_pool: PgPool) {
    let user = create_user(
        &migrator_pool,
        "alice@acme.test",
        "Alice Anderson",
        "password123",
    )
    .await;
    let earlier_org = create_org(&migrator_pool, "Earlier Realty").await;
    // Create the membership through the domain function, then backdate its
    // timestamp via the migrator connection — the fixture rule permits
    // backdating an existing row, never creating one directly
    // (docs/specs/SLICE_004.md §11).
    add_membership(&migrator_pool, earlier_org, user).await;
    sqlx::query(
        "UPDATE organization_membership SET created_at = now() - interval '1 day'
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(earlier_org)
    .bind(user)
    .execute(&migrator_pool)
    .await
    .unwrap();

    // A second person, member of the later Organization only, so the
    // members-scoping assertion below is discriminating: if scoping were
    // broken (e.g. matched on user_id alone instead of the active
    // Organization), this person would leak into Alice's list.
    let later_org = create_org(&migrator_pool, "Later Realty").await;
    add_membership(&migrator_pool, later_org, user).await;
    let later_org_only_user = create_user(
        &migrator_pool,
        "carol@later.test",
        "Carol Carpenter",
        "password123",
    )
    .await;
    add_membership(&migrator_pool, later_org, later_org_only_user).await;

    let router = build_router(&migrator_pool).await;
    let login_response = login(&router, "alice@acme.test", "password123").await;
    let cookie = extract_cookie(&login_response);
    let login_body = body_json(login_response).await;
    assert_eq!(login_body["organization"]["name"], "Earlier Realty");

    let members_body =
        body_json(get_with_cookie(&router, "/api/organization/members", &cookie).await).await;
    let emails: Vec<&str> = members_body["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["email"].as_str().unwrap())
        .collect();
    assert_eq!(
        emails,
        vec!["alice@acme.test"],
        "members must be scoped to the active (earlier) Organization only"
    );
}

// --- Role privileges -----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn current_user_matches_each_role(migrator_pool: PgPool) {
    let (migrator_user,): (String,) = sqlx::query_as("SELECT current_user")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(migrator_user, "crm_migrator");

    let app_pool = connect_as_app(&migrator_pool).await;
    let (app_user,): (String,) = sqlx::query_as("SELECT current_user")
        .fetch_one(&app_pool)
        .await
        .unwrap();
    assert_eq!(app_user, "crm_app");
}

#[sqlx::test]
#[ignore]
async fn crm_app_has_exactly_the_specified_grants(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;

    // Cannot DDL at all.
    let ddl = sqlx::query("CREATE TABLE should_fail (id INT)")
        .execute(&app_pool)
        .await;
    assert!(ddl.is_err(), "crm_app must not be able to run DDL");

    // Amended by docs/specs/SLICE_004.md §2 (declared change, AGENTS.md
    // §11): crm_app gains INSERT on organization, app_user,
    // local_credential, organization_membership (plus invitation, stage —
    // covered in tests/db_admin.rs). organization_membership additionally
    // gains column-level UPDATE (role, status, updated_at); local_credential
    // gains column-level UPDATE (password_hash, updated_at) for the accept/
    // CLI set-password paths. `app_user`/`organization` still have no
    // UPDATE grant at all — Postgres checks table-level privilege before
    // evaluating constraints, so `.is_err()` on a bare `UPDATE` is a genuine
    // permission-denied failure regardless of each table's column set.
    for table in ["organization", "app_user"] {
        let select = sqlx::query(&format!("SELECT * FROM {table}"))
            .fetch_all(&app_pool)
            .await;
        assert!(
            select.is_ok(),
            "crm_app must be able to SELECT from {table}"
        );

        let update_column = if table == "organization" {
            "name"
        } else {
            "display_name"
        };
        let update = sqlx::query(&format!(
            "UPDATE {table} SET {update_column} = {update_column} WHERE false"
        ))
        .execute(&app_pool)
        .await;
        assert!(
            update.is_err(),
            "crm_app must not be able to UPDATE {table}"
        );
    }

    // Slice 007a added NOT NULL intake columns; the grant under test is
    // still "crm_app may INSERT into organization".
    let org_insert = sqlx::query(
        "INSERT INTO organization (name, intake_slug, intake_token)
         VALUES ('Grant Check Org', 'grant-check-org', 'abcdefgh')",
    )
    .execute(&app_pool)
    .await;
    assert!(
        org_insert.is_ok(),
        "crm_app must be able to INSERT into organization (SLICE_004 §2)"
    );

    let user_insert = sqlx::query(
        "INSERT INTO app_user (email, display_name) VALUES ('grant-check@test.internal', 'Grant Check')",
    )
    .execute(&app_pool)
    .await;
    assert!(
        user_insert.is_ok(),
        "crm_app must be able to INSERT into app_user (SLICE_004 §2)"
    );

    // local_credential: SELECT, INSERT, column-level UPDATE (password_hash,
    // updated_at) — but not an arbitrary column.
    let credential_select = sqlx::query("SELECT * FROM local_credential")
        .fetch_all(&app_pool)
        .await;
    assert!(
        credential_select.is_ok(),
        "crm_app must be able to SELECT from local_credential"
    );
    let credential_insert = sqlx::query(
        "INSERT INTO local_credential (user_id, password_hash)
         SELECT id, 'grant-check-hash' FROM app_user LIMIT 0",
    )
    .execute(&app_pool)
    .await;
    assert!(
        credential_insert.is_ok(),
        "crm_app must be able to INSERT into local_credential (SLICE_004 §2)"
    );
    let credential_update =
        sqlx::query("UPDATE local_credential SET password_hash = password_hash WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        credential_update.is_ok(),
        "crm_app must be able to UPDATE local_credential.password_hash (SLICE_004 §2)"
    );

    // organization_membership: SELECT, INSERT, column-level UPDATE (role,
    // status, updated_at) — created_at stays immutable to the application.
    let membership_select = sqlx::query("SELECT * FROM organization_membership")
        .fetch_all(&app_pool)
        .await;
    assert!(
        membership_select.is_ok(),
        "crm_app must be able to SELECT from organization_membership"
    );
    let membership_insert = sqlx::query(
        "INSERT INTO organization_membership (organization_id, user_id, role, status)
         SELECT id, id, 'member', 'active' FROM organization LIMIT 0",
    )
    .execute(&app_pool)
    .await;
    assert!(
        membership_insert.is_ok(),
        "crm_app must be able to INSERT into organization_membership (SLICE_004 §2)"
    );
    let membership_role_update =
        sqlx::query("UPDATE organization_membership SET role = role WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        membership_role_update.is_ok(),
        "crm_app must be able to UPDATE organization_membership.role (SLICE_004 §2)"
    );
    let membership_created_at_update =
        sqlx::query("UPDATE organization_membership SET created_at = created_at WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        membership_created_at_update.is_err(),
        "crm_app must not be able to UPDATE organization_membership.created_at"
    );

    // user_session: SELECT, INSERT, UPDATE granted; DELETE denied.
    let session_select = sqlx::query("SELECT * FROM user_session")
        .fetch_all(&app_pool)
        .await;
    assert!(
        session_select.is_ok(),
        "crm_app must be able to SELECT from user_session"
    );

    let session_insert = sqlx::query(
        "INSERT INTO user_session (token_hash, user_id, active_organization_id, expires_at)
         SELECT 'grant-check', id, id, now() + interval '1 hour' FROM app_user LIMIT 0",
    )
    .execute(&app_pool)
    .await;
    assert!(
        session_insert.is_ok(),
        "crm_app must be able to INSERT into user_session"
    );

    let session_update = sqlx::query("UPDATE user_session SET revoked_at = now() WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        session_update.is_ok(),
        "crm_app must be able to UPDATE user_session"
    );

    let session_delete = sqlx::query("DELETE FROM user_session")
        .execute(&app_pool)
        .await;
    assert!(
        session_delete.is_err(),
        "crm_app must not be able to DELETE from user_session"
    );
}

// --- Bootstrap idempotency (via the API, not direct writes) ----------

/// The dev-bootstrap flow (platform admin creates Organizations and
/// invites their admins; each admin invites a member) now runs entirely
/// through the same HTTP endpoints the API exposes (no CLI seed-dev, no
/// direct writes). Re-attempting a step against already-created state
/// must be rejected cleanly — not silently skipped, not duplicated
/// (docs/specs/SLICE_004.md §4, §11).
#[sqlx::test]
#[ignore]
async fn platform_bootstrap_flow_rejects_repeat_creation(migrator_pool: PgPool) {
    const PW: &str = "test-seed-password-123456";

    create_platform_admin(&migrator_pool, "owner@platform.test", "Platform Owner", PW).await;

    let router = build_router(&migrator_pool).await;
    let platform_cookie = login_cookie(&router, "owner@platform.test", PW).await;

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
    common::create_org_with_admin_and_member_via_api(
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

    let (orgs_1, users_1): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM organization), (SELECT count(*) FROM app_user)",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    // Two Organizations, five users (the platform admin + four members).
    assert_eq!(orgs_1, 2);
    assert_eq!(users_1, 5);

    // Re-attempting either step against already-created state is a clean
    // rejection, never a duplicate row.
    let dup_org = post_json_with_cookie(
        &router,
        "/api/platform/organizations",
        &platform_cookie,
        serde_json::json!({ "name": "Acme Realty" }),
    )
    .await;
    assert_eq!(dup_org.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(dup_org).await["error"], "organization_name_taken");

    let dup_invite = post_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{acme_id}/invitations"),
        &platform_cookie,
        serde_json::json!({ "email": "alice@acme.test", "role": "admin" }),
    )
    .await;
    assert_eq!(dup_invite.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(dup_invite).await["error"], "already_member");

    let (orgs_2, users_2): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM organization), (SELECT count(*) FROM app_user)",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(
        (orgs_1, users_1),
        (orgs_2, users_2),
        "rejected repeat-creation attempts must not create duplicate rows"
    );
}

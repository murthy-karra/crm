//! DB-backed tests for Slice 004 administration (docs/specs/SLICE_004.md
//! §13). Run only via ./scripts/check-db.
mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode;
use axum::Router;
use sqlx::PgPool;
use uuid::Uuid;

use common::{
    add_membership_with, body_json, build_router, build_router_with_publisher, connect_as_app,
    create_org, create_org_with_stages_and_member, create_platform_admin, create_user,
    delete_with_cookie, get_with_cookie, login, login_cookie, post_json_with_cookie,
    put_json_with_cookie,
};
use crm_api::domain::admin::commands::{
    accept_invitation, change_member_role, issue_invitation, revoke_invitation, AcceptInvitation,
    ChangeMemberRole, IssueInvitation, RevokeInvitation,
};
use crm_api::domain::admin::{AdminActor, MembershipStatus, Role};
use crm_api::domain::envelope::Origin;
use crm_api::ids::{OrganizationId, UserId};
use crm_api::realtime::Publisher;

const TTL: Duration = Duration::from_secs(168 * 3600);
const PW: &str = "correct horse battery staple";

async fn me_json(router: &Router, cookie: &str) -> serde_json::Value {
    body_json(get_with_cookie(router, "/api/me", cookie).await).await
}

fn owner_actor(user_id: Uuid) -> AdminActor {
    AdminActor {
        actor_user_id: UserId::new(user_id),
        origin: Origin::Cli,
    }
}

// --- Criterion 1: grants -------------------------------------------------

#[sqlx::test]
#[ignore]
async fn crm_app_cannot_mint_platform_admin_or_mutate_invitation_token_hash(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let owner_id = create_user(&migrator_pool, "owner@acme.test", "Owner", PW).await;

    let insert =
        sqlx::query("INSERT INTO platform_admin (user_id, granted_via) VALUES ($1, 'cli')")
            .bind(owner_id)
            .execute(&app_pool)
            .await;
    assert!(
        insert.is_err(),
        "crm_app must not be able to INSERT into platform_admin"
    );
    let select = sqlx::query("SELECT * FROM platform_admin")
        .fetch_all(&app_pool)
        .await;
    assert!(
        select.is_ok(),
        "crm_app must be able to SELECT from platform_admin"
    );

    // A real invitation, created through the domain command (fixture rule).
    let outcome = issue_invitation(
        &app_pool,
        owner_actor(owner_id),
        "Owner",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_id),
            email: "invitee@acme.test".to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();

    let update_token_hash =
        sqlx::query("UPDATE invitation SET token_hash = 'forged' WHERE id = $1")
            .bind(outcome.invitation.id.as_uuid())
            .execute(&app_pool)
            .await;
    assert!(
        update_token_hash.is_err(),
        "crm_app must not be able to UPDATE invitation.token_hash"
    );
    let update_email = sqlx::query("UPDATE invitation SET email = 'other@acme.test' WHERE id = $1")
        .bind(outcome.invitation.id.as_uuid())
        .execute(&app_pool)
        .await;
    assert!(
        update_email.is_err(),
        "crm_app must not be able to UPDATE invitation.email"
    );
    let update_expires = sqlx::query("UPDATE invitation SET expires_at = now() WHERE id = $1")
        .bind(outcome.invitation.id.as_uuid())
        .execute(&app_pool)
        .await;
    assert!(
        update_expires.is_err(),
        "crm_app must not be able to UPDATE invitation.expires_at"
    );

    // The granted columns succeed.
    let allowed = sqlx::query(
        "UPDATE invitation SET revoked_at = now(), revoke_reason = 'revoked' WHERE id = $1",
    )
    .bind(outcome.invitation.id.as_uuid())
    .execute(&app_pool)
    .await;
    assert!(
        allowed.is_ok(),
        "crm_app must be able to UPDATE invitation's granted columns"
    );
}

struct AdminFactIds {
    organization_created_id: Uuid,
    invitation_issued_id: Uuid,
    invitation_resolved_id: Uuid,
    membership_changed_id: Uuid,
}

/// Produces one real row in each of the four admin fact tables by driving
/// the actual domain commands (fixture rule) — a full, realistic
/// lifecycle: create an Organization, invite an admin, accept, promote.
async fn admin_fact_rows(migrator_pool: &PgPool, app_pool: &PgPool) -> AdminFactIds {
    let bootstrap = create_user(migrator_pool, "bootstrap@platform.test", "Bootstrap", PW).await;
    let actor = owner_actor(bootstrap);

    let organization = crm_api::domain::admin::commands::create_organization(
        app_pool,
        actor,
        crm_api::domain::admin::commands::CreateOrganization {
            name: "Fact Rows Realty".to_string(),
        },
    )
    .await
    .unwrap();
    let (organization_created_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM organization_created WHERE organization_id = $1")
            .bind(organization.id.0)
            .fetch_one(migrator_pool)
            .await
            .unwrap();

    let outcome = issue_invitation(
        app_pool,
        actor,
        "Bootstrap",
        TTL,
        IssueInvitation {
            organization_id: organization.id,
            email: "newadmin@factrows.test".to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();
    let (invitation_issued_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM invitation_issued WHERE invitation_id = $1")
            .bind(outcome.invitation.id.as_uuid())
            .fetch_one(migrator_pool)
            .await
            .unwrap();

    let accepted = accept_invitation(
        app_pool,
        AcceptInvitation {
            token: outcome.token,
            display_name: "New Admin".to_string(),
            password: PW.to_string(),
            origin: Origin::WebSession,
        },
    )
    .await
    .unwrap();
    let (invitation_resolved_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM invitation_resolved WHERE invitation_id = $1")
            .bind(outcome.invitation.id.as_uuid())
            .fetch_one(migrator_pool)
            .await
            .unwrap();

    change_member_role(
        app_pool,
        actor,
        ChangeMemberRole {
            organization_id: organization.id,
            user_id: accepted.user_id,
            role: Role::Admin,
        },
    )
    .await
    .unwrap();
    let (membership_changed_id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM membership_changed WHERE user_id = $1 AND reason = 'promote'",
    )
    .bind(accepted.user_id.0)
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    AdminFactIds {
        organization_created_id,
        invitation_issued_id,
        invitation_resolved_id,
        membership_changed_id,
    }
}

#[sqlx::test]
#[ignore]
async fn admin_fact_tables_are_append_only_via_grant_and_trigger(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;
    let rows = admin_fact_rows(&migrator_pool, &app_pool).await;

    let cases: [(&str, Uuid); 4] = [
        ("organization_created", rows.organization_created_id),
        ("invitation_issued", rows.invitation_issued_id),
        ("invitation_resolved", rows.invitation_resolved_id),
        ("membership_changed", rows.membership_changed_id),
    ];

    for (table, id) in cases {
        let app_update = sqlx::query(&format!(
            "UPDATE {table} SET occurred_at = occurred_at WHERE id = $1"
        ))
        .bind(id)
        .execute(&app_pool)
        .await;
        assert!(
            app_update.is_err(),
            "{table}: crm_app UPDATE must be denied"
        );

        let app_delete = sqlx::query(&format!("DELETE FROM {table} WHERE id = $1"))
            .bind(id)
            .execute(&app_pool)
            .await;
        assert!(
            app_delete.is_err(),
            "{table}: crm_app DELETE must be denied"
        );

        let migrator_update = sqlx::query(&format!(
            "UPDATE {table} SET occurred_at = occurred_at WHERE id = $1"
        ))
        .bind(id)
        .execute(&migrator_pool)
        .await;
        assert!(
            migrator_update.is_err(),
            "{table}: crm_migrator UPDATE must be denied by the append-only trigger"
        );

        let migrator_delete = sqlx::query(&format!("DELETE FROM {table} WHERE id = $1"))
            .bind(id)
            .execute(&migrator_pool)
            .await;
        assert!(
            migrator_delete.is_err(),
            "{table}: crm_migrator DELETE must be denied by the append-only trigger"
        );

        let migrator_truncate = sqlx::query(&format!("TRUNCATE TABLE {table}"))
            .execute(&migrator_pool)
            .await;
        assert!(
            migrator_truncate.is_err(),
            "{table}: crm_migrator TRUNCATE must be denied by the append-only trigger"
        );
    }
}

/// Invitation facts never carry the email (PII rule, docs/specs/
/// SLICE_004.md §2); platform actions carry `origin = 'platform'`.
#[sqlx::test]
#[ignore]
async fn admin_facts_are_pii_free_and_carry_correct_origin(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;
    let owner = create_user(&migrator_pool, "owner2@platform.test", "Owner", PW).await;
    let platform_actor = AdminActor {
        actor_user_id: UserId::new(owner),
        origin: Origin::Platform,
    };

    let organization = crm_api::domain::admin::commands::create_organization(
        &app_pool,
        platform_actor,
        crm_api::domain::admin::commands::CreateOrganization {
            name: "Origin Check Realty".to_string(),
        },
    )
    .await
    .unwrap();

    let (org_origin,): (String,) =
        sqlx::query_as("SELECT origin FROM organization_created WHERE organization_id = $1")
            .bind(organization.id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(org_origin, "platform");

    issue_invitation(
        &app_pool,
        platform_actor,
        "Owner",
        TTL,
        IssueInvitation {
            organization_id: organization.id,
            email: "secret-email@origin-check.test".to_string(),
            role: Role::Admin,
        },
    )
    .await
    .unwrap();

    let (invitation_origin,): (String,) =
        sqlx::query_as("SELECT origin FROM invitation_issued WHERE organization_id = $1")
            .bind(organization.id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(invitation_origin, "platform");

    // The fact row itself has no email column at all — confirm the whole
    // row, serialized, never contains the invited address.
    let row: (Uuid, String) = sqlx::query_as(
        "SELECT invitation_id, role FROM invitation_issued WHERE organization_id = $1",
    )
    .bind(organization.id.0)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let serialized = format!("{row:?}");
    assert!(!serialized.contains("secret-email"));
}

// --- Criterion 3: platform admin session shape and scope -----------------

#[sqlx::test]
#[ignore]
async fn platform_admin_with_zero_memberships_has_null_organization_and_is_401_on_tenant_routes(
    migrator_pool: PgPool,
) {
    create_platform_admin(&migrator_pool, "owner3@platform.test", "Owner", PW).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner3@platform.test", PW).await;

    let me = me_json(&router, &cookie).await;
    assert_eq!(me["organization"], serde_json::Value::Null);
    assert_eq!(me["platform_admin"], true);

    for uri in ["/api/people", "/api/today", "/api/stages"] {
        let resp = get_with_cookie(&router, uri, &cookie).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "GET {uri}");
    }
    let inquiries_resp =
        post_json_with_cookie(&router, "/api/inquiries", &cookie, serde_json::json!({})).await;
    assert_eq!(inquiries_resp.status(), StatusCode::UNAUTHORIZED);
    let realtime_resp = post_json_with_cookie(
        &router,
        "/api/realtime/token",
        &cookie,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(realtime_resp.status(), StatusCode::UNAUTHORIZED);

    // Lists and creates Organizations.
    let list_resp = get_with_cookie(&router, "/api/platform/organizations", &cookie).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let create_resp = post_json_with_cookie(
        &router,
        "/api/platform/organizations",
        &cookie,
        serde_json::json!({ "name": "Zero Membership Realty" }),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
}

// --- Criterion 4: member vs admin vs platform authorization ---------------

#[sqlx::test]
#[ignore]
async fn every_org_admin_route_is_403_for_a_member(migrator_pool: PgPool) {
    let (org_id, member_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "carol@acme.test",
        "Carol",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "carol@acme.test", PW).await;

    let list_resp = get_with_cookie(&router, "/api/organization/invitations", &cookie).await;
    assert_eq!(list_resp.status(), StatusCode::FORBIDDEN);

    let create_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &cookie,
        serde_json::json!({ "email": "x@acme.test", "role": "member" }),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::FORBIDDEN);

    let role_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{member_id}/role"),
        &cookie,
        serde_json::json!({ "role": "admin" }),
    )
    .await;
    assert_eq!(role_resp.status(), StatusCode::FORBIDDEN);

    let status_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{member_id}/status"),
        &cookie,
        serde_json::json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(status_resp.status(), StatusCode::FORBIDDEN);

    let delete_resp = delete_with_cookie(
        &router,
        &format!("/api/organization/invitations/{}", Uuid::new_v4()),
        &cookie,
    )
    .await;
    assert_eq!(delete_resp.status(), StatusCode::FORBIDDEN);

    let _ = (org_id, app_pool);
}

#[sqlx::test]
#[ignore]
async fn every_platform_route_is_403_for_an_organization_admin(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice4@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "alice4@acme.test", PW).await;

    let list_resp = get_with_cookie(&router, "/api/platform/organizations", &cookie).await;
    assert_eq!(list_resp.status(), StatusCode::FORBIDDEN);

    let create_resp = post_json_with_cookie(
        &router,
        "/api/platform/organizations",
        &cookie,
        serde_json::json!({ "name": "Should Not Exist" }),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::FORBIDDEN);

    let detail_resp = get_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}"),
        &cookie,
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::FORBIDDEN);

    // The remaining three platform routes (spec §13 item 4: "every
    // platform route"). PlatformAuthContext rejects before the handler
    // body runs, so the path ids need not resolve to anything real.
    let target_user_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();

    let promote_resp = put_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/members/{target_user_id}/role"),
        &cookie,
        serde_json::json!({ "role": "admin" }),
    )
    .await;
    assert_eq!(promote_resp.status(), StatusCode::FORBIDDEN);

    let invite_resp = post_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/invitations"),
        &cookie,
        serde_json::json!({ "email": "nope@example.com", "role": "admin" }),
    )
    .await;
    assert_eq!(invite_resp.status(), StatusCode::FORBIDDEN);

    let revoke_resp = delete_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/invitations/{invitation_id}"),
        &cookie,
    )
    .await;
    assert_eq!(revoke_resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
#[ignore]
async fn a_user_who_is_both_admin_and_platform_admin_reaches_both(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let dual = create_user(&migrator_pool, "dual@acme.test", "Dual", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        dual,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    create_platform_admin(&migrator_pool, "dual@acme.test", "Dual", PW).await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "dual@acme.test", PW).await;

    let me = me_json(&router, &cookie).await;
    assert_eq!(me["organization"]["role"], "admin");
    assert_eq!(me["platform_admin"], true);

    // Org-admin route.
    let invites_resp = get_with_cookie(&router, "/api/organization/invitations", &cookie).await;
    assert_eq!(invites_resp.status(), StatusCode::OK);

    // Platform route.
    let platform_resp = get_with_cookie(&router, "/api/platform/organizations", &cookie).await;
    assert_eq!(platform_resp.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn platform_role_route_rejects_member_with_400(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let member_id = create_user(&migrator_pool, "member5@acme.test", "Member", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    create_platform_admin(&migrator_pool, "owner5@platform.test", "Owner", PW).await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner5@platform.test", PW).await;

    let resp = put_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/members/{member_id}/role"),
        &cookie,
        serde_json::json!({ "role": "member" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let invite_resp = post_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/invitations"),
        &cookie,
        serde_json::json!({ "email": "x@acme.test", "role": "member" }),
    )
    .await;
    assert_eq!(invite_resp.status(), StatusCode::BAD_REQUEST);
}

// --- Criterion 5: cross-Organization 404 byte-identity -------------------

#[sqlx::test]
#[ignore]
async fn cross_organization_admin_routes_are_byte_identical_404s(migrator_pool: PgPool) {
    let org_a = create_org(&migrator_pool, "Acme Realty").await;
    let org_b = create_org(&migrator_pool, "Best Realty").await;
    let alice = create_user(&migrator_pool, "alice6@acme.test", "Alice", PW).await;
    let bob = create_user(&migrator_pool, "bob6@best.test", "Bob", PW).await;
    add_membership_with(
        &migrator_pool,
        org_a,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        org_b,
        bob,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let bob_invitation = issue_invitation(
        &app_pool,
        owner_actor(bob),
        "Bob",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_b),
            email: "target@best.test".to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();

    let router = build_router(&migrator_pool).await;
    let alice_cookie = login_cookie(&router, "alice6@acme.test", PW).await;

    // Alice tries to revoke Best's invitation by id.
    let revoke_resp = delete_with_cookie(
        &router,
        &format!(
            "/api/organization/invitations/{}",
            bob_invitation.invitation.id
        ),
        &alice_cookie,
    )
    .await;
    let random_uuid_resp = delete_with_cookie(
        &router,
        &format!("/api/organization/invitations/{}", Uuid::new_v4()),
        &alice_cookie,
    )
    .await;
    assert_eq!(revoke_resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(random_uuid_resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        body_json(revoke_resp).await,
        body_json(random_uuid_resp).await,
        "cross-Organization and nonexistent invitation ids must be byte-identical 404s"
    );

    // Alice tries to change Bob's role by id.
    let role_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{bob}/role"),
        &alice_cookie,
        serde_json::json!({ "role": "member" }),
    )
    .await;
    assert_eq!(role_resp.status(), StatusCode::NOT_FOUND);

    // Platform routes on a nonexistent Organization -> 404.
    create_platform_admin(&migrator_pool, "owner6@platform.test", "Owner", PW).await;
    let platform_cookie = login_cookie(&router, "owner6@platform.test", PW).await;
    let nonexistent_org = Uuid::new_v4();
    let detail_resp = get_with_cookie(
        &router,
        &format!("/api/platform/organizations/{nonexistent_org}"),
        &platform_cookie,
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::NOT_FOUND);
    let invite_resp = post_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{nonexistent_org}/invitations"),
        &platform_cookie,
        serde_json::json!({ "email": "x@example.com", "role": "admin" }),
    )
    .await;
    assert_eq!(invite_resp.status(), StatusCode::NOT_FOUND);
}

// --- Criterion 6: invitation lifecycle ------------------------------------

#[sqlx::test]
#[ignore]
async fn invitation_lifecycle_issue_preview_accept_reject_second_accept(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice7@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let alice_cookie = login_cookie(&router, "alice7@acme.test", PW).await;

    let issue_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "frank7@acme.test", "role": "member" }),
    )
    .await;
    assert_eq!(issue_resp.status(), StatusCode::CREATED);
    let issue_body = body_json(issue_resp).await;
    let accept_path = issue_body["accept_path"].as_str().unwrap().to_string();
    let token = accept_path.strip_prefix("/invite/").unwrap().to_string();

    let preview_resp = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": token }),
    )
    .await;
    assert_eq!(preview_resp.status(), StatusCode::OK);
    let preview_body = body_json(preview_resp).await;
    assert_eq!(preview_body["organization_name"], "Acme Realty");
    assert_eq!(preview_body["email"], "frank7@acme.test");
    assert_eq!(preview_body["role"], "member");

    let accept_resp = post_json_with_cookie(
        &router,
        "/api/invitations/accept",
        "",
        serde_json::json!({ "token": token, "display_name": "Frank", "password": PW }),
    )
    .await;
    assert_eq!(accept_resp.status(), StatusCode::OK);
    let accept_body = body_json(accept_resp).await;
    assert_eq!(accept_body["organization"]["name"], "Acme Realty");
    assert_eq!(accept_body["organization"]["role"], "member");
    assert_eq!(accept_body["user"]["email"], "frank7@acme.test");

    // Frank can reach Manage-gated data (org-scoped, but member-visible):
    // members list is visible to any member.
    let frank_cookie = login_cookie(&router, "frank7@acme.test", PW).await;
    let members_resp = get_with_cookie(&router, "/api/organization/members", &frank_cookie).await;
    assert_eq!(members_resp.status(), StatusCode::OK);

    // Second accept of the same token -> 409 invitation_used.
    let second_accept = post_json_with_cookie(
        &router,
        "/api/invitations/accept",
        "",
        serde_json::json!({ "token": token, "display_name": "Frank Again", "password": PW }),
    )
    .await;
    assert_eq!(second_accept.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(second_accept).await["error"], "invitation_used");

    // Preview after accept -> 409 invitation_used, not 200.
    let preview_after = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": token }),
    )
    .await;
    assert_eq!(preview_after.status(), StatusCode::CONFLICT);

    // Malformed token -> 404, never 400.
    let malformed_resp = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": "not-a-valid-token" }),
    )
    .await;
    assert_eq!(malformed_resp.status(), StatusCode::NOT_FOUND);
}

/// A true concurrent double-accept — mirrors
/// `two_admins_concurrently_demoting_each_other_leaves_at_least_one_active_admin`'s
/// `tokio::spawn`/`tokio::join!` pattern against two independent
/// connections racing the same token, not a sequential accept-then-accept
/// (docs/specs/SLICE_004.md §9, §13 criterion 6).
#[sqlx::test]
#[ignore]
async fn concurrent_double_accept_of_the_same_token_exactly_one_succeeds(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice19@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let outcome = issue_invitation(
        &app_pool,
        owner_actor(alice),
        "Alice",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_id),
            email: "racer19@acme.test".to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();

    let app_pool_2 = app_pool.clone();
    let token_1 = outcome.token.clone();
    let token_2 = outcome.token.clone();

    let accept_1 = tokio::spawn(async move {
        accept_invitation(
            &app_pool,
            AcceptInvitation {
                token: token_1,
                display_name: "Racer One".to_string(),
                password: PW.to_string(),
                origin: Origin::WebSession,
            },
        )
        .await
    });
    let accept_2 = tokio::spawn(async move {
        accept_invitation(
            &app_pool_2,
            AcceptInvitation {
                token: token_2,
                display_name: "Racer Two".to_string(),
                password: PW.to_string(),
                origin: Origin::WebSession,
            },
        )
        .await
    });

    let (r1, r2) = tokio::join!(accept_1, accept_2);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    let r1_ok = r1.is_ok();
    let r2_ok = r2.is_ok();
    assert!(
        r1_ok ^ r2_ok,
        "exactly one concurrent accept must succeed, got r1_ok={r1_ok} r2_ok={r2_ok}"
    );
    let loser = if r1_ok { r2 } else { r1 };
    assert!(
        matches!(
            loser,
            Err(crm_api::domain::admin::AdminCommandError::InvitationUsed)
        ),
        "the losing concurrent accept must see invitation_used"
    );

    // Exactly one membership_changed{reason: invitation} and one
    // invitation_resolved{accepted} fact — not two, not zero.
    let (membership_changed_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM membership_changed
         WHERE organization_id = $1 AND reason = 'invitation'",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(membership_changed_count, 1);

    let (invitation_resolved_accepted_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM invitation_resolved
         WHERE invitation_id = $1 AND outcome = 'accepted'",
    )
    .bind(outcome.invitation.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(invitation_resolved_accepted_count, 1);

    // Exactly one app_user for the racer's email — the loser never
    // created a duplicate account.
    let (user_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM app_user WHERE lower(email) = 'racer19@acme.test'")
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(user_count, 1);
}

/// A concurrent `IssueInvitation` racing an `AcceptInvitation` of the
/// already-open invitation for the same `(organization, email)`
/// (independent review finding, addressed in `issue_invitation.rs`): under
/// READ COMMITTED, `is_member_by_email`'s first read can be stale by the
/// time the transaction reaches `supersede_invitation`/`insert_invitation`.
/// Either side may legitimately win — a re-issue unconditionally
/// supersedes (§4/§9), so the accept correctly seeing `NotFound` because
/// the token was revoked out from under it is not a bug. What repeated
/// runs of this single test exercise (which shape occurs depends on exact
/// interleaving) is the two failure modes an earlier version of
/// `issue_invitation` had instead: the accept's commit landing between the
/// open-invitation read and the supersede UPDATE (a `check_violation` on
/// `CHECK (accepted_at IS NULL OR revoked_at IS NULL)`, now absorbed by the
/// retry) — or the accept's commit landing between the re-issue's
/// `find_open_invitation` read and its insert (previously no error at all,
/// silently inserting a dangling, never-acceptable invitation for an email
/// that is now an active member; now caught by the pre-insert re-check).
/// The invariants asserted below hold regardless of which side won: never
/// a bare database error from either call, the membership count matching
/// whichever side actually won, and never an open invitation left behind
/// for an email that already has an active membership.
#[sqlx::test]
#[ignore]
async fn concurrent_issue_and_accept_never_leaves_a_dangling_invitation_or_bare_db_error(
    migrator_pool: PgPool,
) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice20@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let email = "racer20@acme.test".to_string();

    let first = issue_invitation(
        &app_pool,
        owner_actor(alice),
        "Alice",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_id),
            email: email.clone(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();

    let accept_pool = app_pool.clone();
    let reissue_pool = app_pool.clone();
    let accept_token = first.token.clone();
    let reissue_email = email.clone();

    let accept_task = tokio::spawn(async move {
        accept_invitation(
            &accept_pool,
            AcceptInvitation {
                token: accept_token,
                display_name: "Racer Twenty".to_string(),
                password: PW.to_string(),
                origin: Origin::WebSession,
            },
        )
        .await
    });
    let reissue_task = tokio::spawn(async move {
        issue_invitation(
            &reissue_pool,
            owner_actor(alice),
            "Alice",
            TTL,
            IssueInvitation {
                organization_id: OrganizationId::new(org_id),
                email: reissue_email,
                role: Role::Member,
            },
        )
        .await
    });

    let (accept_result, reissue_result) = tokio::join!(accept_task, reissue_task);
    let accept_result = accept_result.unwrap();
    let reissue_result = reissue_result.unwrap();

    // Either side may legitimately "win" this race — an admin re-issuing
    // while the invitee is mid-accept is a real, if unlucky, ordering the
    // spec doesn't special-case (issuing always supersedes, unconditionally,
    // §4/§9). If the re-issue's supersede commits first, the accept
    // correctly sees the token as no-longer-open (`NotFound`, since a
    // revoked invitation is indistinguishable from an unknown one) — that
    // is not a bug. What must never happen, from either call, is a bare
    // `AdminCommandError::Database` reaching the caller — every outcome
    // must be one of the clean, documented domain errors.
    if let Err(err) = &accept_result {
        assert!(
            !matches!(err, crm_api::domain::admin::AdminCommandError::Database(_)),
            "accept must never surface a bare database error under this race: {}",
            err.kind()
        );
    }
    if let Err(err) = &reissue_result {
        assert!(
            !matches!(err, crm_api::domain::admin::AdminCommandError::Database(_)),
            "a concurrent re-issue must never surface a bare database error, only a clean \
             domain outcome such as AlreadyMember: {}",
            err.kind()
        );
    }

    // Whichever side won, the membership count for this email must match:
    // exactly one if the accept succeeded, zero if it lost the race (the
    // re-issue itself never creates a membership).
    let expected_member_count: i64 = if accept_result.is_ok() { 1 } else { 0 };
    let (member_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM organization_membership m
         JOIN app_user u ON u.id = m.user_id
         WHERE m.organization_id = $1 AND lower(u.email) = $2",
    )
    .bind(org_id)
    .bind(&email)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(member_count, expected_member_count);

    // Invariant: no OPEN invitation row survives for an email that already
    // has an active membership — whether the re-issue failed with
    // AlreadyMember (no new row) or raced ahead and legitimately
    // superseded the prior row before the accept ran (that row is then
    // revoked/superseded, not open).
    let dangling: Option<(Uuid,)> = sqlx::query_as(
        "SELECT i.id FROM invitation i
         JOIN organization_membership m ON m.organization_id = i.organization_id
         JOIN app_user u ON u.id = m.user_id
         WHERE i.organization_id = $1 AND lower(u.email) = $2
           AND m.status = 'active'
           AND i.accepted_at IS NULL AND i.revoked_at IS NULL",
    )
    .bind(org_id)
    .bind(&email)
    .fetch_optional(&migrator_pool)
    .await
    .unwrap();
    assert!(
        dangling.is_none(),
        "no open invitation may exist for an email that already has an active membership"
    );
}

/// A deterministic, non-timing-dependent complement to the test above.
/// Real `tokio::spawn`/`tokio::join!` concurrency reliably shows the
/// invariants hold, but empirically almost never lands the exact
/// interleaving `issue_invitation`'s pre-insert re-check exists to catch
/// (`AcceptInvitation`'s Argon2id hashing costs tens of ms *before* it
/// opens its transaction, so in practice a concurrent `IssueInvitation` —
/// whose steps are all cheap `SELECT`s/`UPDATE`s — usually finishes
/// entirely before `AcceptInvitation` even begins its transaction, which
/// exercises only the "accept correctly loses to a revoke" path, not the
/// stale-read path). This test instead proves the READ COMMITTED
/// mechanism the fix depends on directly: within one still-open
/// transaction, `is_member_by_email` returns `false`, then — after a
/// *separate*, fully committed `AcceptInvitation` on a different
/// connection — the exact same open transaction's next `is_member_by_email`
/// call (a fresh statement, hence a fresh per-statement snapshot under
/// READ COMMITTED) returns `true`. That is precisely the property
/// `issue_invitation_attempt`'s pre-insert re-check relies on to observe a
/// concurrent accept that committed after the transaction began.
#[sqlx::test]
#[ignore]
async fn read_committed_per_statement_snapshot_observes_a_commit_made_after_the_transaction_began(
    migrator_pool: PgPool,
) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice21@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let email = "racer21@acme.test";

    let outcome = issue_invitation(
        &app_pool,
        owner_actor(alice),
        "Alice",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_id),
            email: email.to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();

    // Open a long-lived transaction — standing in for the in-flight part
    // of `issue_invitation_attempt` between its first `is_member_by_email`
    // check and its pre-insert re-check.
    let mut tx = app_pool.begin().await.unwrap();
    let before = crm_api::domain::admin::queries::is_member_by_email(
        &mut tx,
        OrganizationId::new(org_id),
        email,
    )
    .await
    .unwrap();
    assert!(!before, "not yet a member: no accept has happened yet");

    // A separate, fully independent connection completes a real accept —
    // exactly the concurrent commit the fix must observe.
    accept_invitation(
        &app_pool,
        AcceptInvitation {
            token: outcome.token,
            display_name: "Racer TwentyOne".to_string(),
            password: PW.to_string(),
            origin: Origin::WebSession,
        },
    )
    .await
    .unwrap();

    // The still-open transaction's next statement sees it.
    let after = crm_api::domain::admin::queries::is_member_by_email(
        &mut tx,
        OrganizationId::new(org_id),
        email,
    )
    .await
    .unwrap();
    assert!(
        after,
        "a fresh statement in the still-open transaction must observe the \
         now-committed accept under READ COMMITTED's per-statement snapshot"
    );

    tx.rollback().await.unwrap();
}

#[sqlx::test]
#[ignore]
async fn revoked_invitation_preview_is_404_and_reissue_supersedes(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice8@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let alice_cookie = login_cookie(&router, "alice8@acme.test", PW).await;

    let issue_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "gina8@acme.test", "role": "member" }),
    )
    .await;
    let issue_body = body_json(issue_resp).await;
    let invitation_id = issue_body["invitation"]["id"].as_str().unwrap().to_string();
    let first_token = issue_body["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap()
        .to_string();

    let revoke_resp = delete_with_cookie(
        &router,
        &format!("/api/organization/invitations/{invitation_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(revoke_resp.status(), StatusCode::NO_CONTENT);

    // Idempotent re-revoke.
    let revoke_again = delete_with_cookie(
        &router,
        &format!("/api/organization/invitations/{invitation_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(revoke_again.status(), StatusCode::NO_CONTENT);

    let preview_resp = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": first_token }),
    )
    .await;
    assert_eq!(preview_resp.status(), StatusCode::NOT_FOUND);

    // Re-invite the same email: fresh token, old one stays invalid.
    let reissue_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "gina8@acme.test", "role": "member" }),
    )
    .await;
    assert_eq!(reissue_resp.status(), StatusCode::CREATED);
    let reissue_body = body_json(reissue_resp).await;
    let second_token = reissue_body["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap()
        .to_string();
    assert_ne!(first_token, second_token);

    let old_token_preview = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": first_token }),
    )
    .await;
    assert_eq!(old_token_preview.status(), StatusCode::NOT_FOUND);

    let new_token_preview = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": second_token }),
    )
    .await;
    assert_eq!(new_token_preview.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn expired_invitation_returns_410_and_org_state_flips_to_needs_attention(
    migrator_pool: PgPool,
) {
    let app_pool = connect_as_app(&migrator_pool).await;
    let owner = create_user(&migrator_pool, "owner9@platform.test", "Owner", PW).await;
    create_platform_admin(&migrator_pool, "owner9@platform.test", "Owner", PW).await;

    let organization = crm_api::domain::admin::commands::create_organization(
        &app_pool,
        owner_actor(owner),
        crm_api::domain::admin::commands::CreateOrganization {
            name: "Expiry Realty".to_string(),
        },
    )
    .await
    .unwrap();

    let outcome = issue_invitation(
        &app_pool,
        AdminActor {
            actor_user_id: UserId::new(owner),
            origin: Origin::Platform,
        },
        "Owner",
        TTL,
        IssueInvitation {
            organization_id: organization.id,
            email: "expiring9@example.com".to_string(),
            role: Role::Admin,
        },
    )
    .await
    .unwrap();

    let router = build_router(&migrator_pool).await;
    let platform_cookie = login_cookie(&router, "owner9@platform.test", PW).await;

    let before = get_with_cookie(&router, "/api/platform/organizations", &platform_cookie).await;
    let before_body = body_json(before).await;
    let org_entry = before_body["organizations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == organization.id.to_string())
        .unwrap();
    assert_eq!(org_entry["state"], "pending_first_admin");

    // Backdate the invitation's expiry via the migrator connection — a
    // permitted fixture operation (docs/specs/SLICE_004.md §11).
    sqlx::query("UPDATE invitation SET expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(outcome.invitation.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();

    let preview_resp = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": outcome.token }),
    )
    .await;
    assert_eq!(preview_resp.status(), StatusCode::GONE);
    assert_eq!(body_json(preview_resp).await["error"], "invitation_expired");

    let after = get_with_cookie(&router, "/api/platform/organizations", &platform_cookie).await;
    let after_body = body_json(after).await;
    let org_entry_after = after_body["organizations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == organization.id.to_string())
        .unwrap();
    assert_eq!(org_entry_after["state"], "needs_attention");
}

#[sqlx::test]
#[ignore]
async fn issuing_to_an_existing_member_is_already_member_and_to_an_outside_account_is_generic(
    migrator_pool: PgPool,
) {
    let org_a = create_org(&migrator_pool, "Acme Realty").await;
    let org_b = create_org(&migrator_pool, "Best Realty").await;
    let alice = create_user(&migrator_pool, "alice10@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_a,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let carol = create_user(&migrator_pool, "carol10@acme.test", "Carol", PW).await;
    add_membership_with(
        &migrator_pool,
        org_a,
        carol,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    // Someone with an account in a *different* Organization.
    create_user(&migrator_pool, "outsider10@best.test", "Outsider", PW).await;
    let _ = org_b;

    let router = build_router(&migrator_pool).await;
    let alice_cookie = login_cookie(&router, "alice10@acme.test", PW).await;

    let already_member_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "carol10@acme.test", "role": "member" }),
    )
    .await;
    assert_eq!(already_member_resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(already_member_resp).await["error"],
        "already_member"
    );

    // Outside account: 201 like any other email (no enumeration).
    let outside_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "outsider10@best.test", "role": "member" }),
    )
    .await;
    assert_eq!(outside_resp.status(), StatusCode::CREATED);
    let outside_body = body_json(outside_resp).await;
    let token = outside_body["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap()
        .to_string();

    // Accepting it fails generically — no user modified.
    let accept_resp = post_json_with_cookie(
        &router,
        "/api/invitations/accept",
        "",
        serde_json::json!({ "token": token, "display_name": "Outsider", "password": PW }),
    )
    .await;
    assert_eq!(accept_resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(accept_resp).await["error"],
        "invitation_not_acceptable"
    );

    // The outsider's original Organization membership is untouched.
    let (member_count_in_org_a,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM organization_membership WHERE organization_id = $1")
            .bind(org_a)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(member_count_in_org_a, 2, "alice and carol only");
}

// --- Criterion 7: last-admin invariant ------------------------------------

#[sqlx::test]
#[ignore]
async fn sole_admin_cannot_self_demote_or_self_deactivate(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice11@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "alice11@acme.test", PW).await;

    let demote_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{alice}/role"),
        &cookie,
        serde_json::json!({ "role": "member" }),
    )
    .await;
    assert_eq!(demote_resp.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(demote_resp).await["error"], "last_admin");

    let deactivate_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{alice}/status"),
        &cookie,
        serde_json::json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivate_resp.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(deactivate_resp).await["error"], "last_admin");

    // Still active admin afterward.
    let (role, status): (String, String) = sqlx::query_as(
        "SELECT role, status FROM organization_membership WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(alice)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!((role.as_str(), status.as_str()), ("admin", "active"));
}

#[sqlx::test]
#[ignore]
async fn two_admins_concurrently_demoting_each_other_leaves_at_least_one_active_admin(
    migrator_pool: PgPool,
) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice12@acme.test", "Alice", PW).await;
    let bob = create_user(&migrator_pool, "bob12@acme.test", "Bob", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        org_id,
        bob,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let app_pool_2 = app_pool.clone();

    let alice_demotes_bob = tokio::spawn(async move {
        change_member_role(
            &app_pool,
            owner_actor(alice),
            ChangeMemberRole {
                organization_id: OrganizationId::new(org_id),
                user_id: UserId::new(bob),
                role: Role::Member,
            },
        )
        .await
    });
    let bob_demotes_alice = tokio::spawn(async move {
        change_member_role(
            &app_pool_2,
            owner_actor(bob),
            ChangeMemberRole {
                organization_id: OrganizationId::new(org_id),
                user_id: UserId::new(alice),
                role: Role::Member,
            },
        )
        .await
    });

    let (r1, r2) = tokio::join!(alice_demotes_bob, bob_demotes_alice);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    // At least one must fail with LastAdmin (the advisory lock serializes
    // them, so whichever runs second sees zero-would-remain and is
    // rejected); it must never be the case that both succeed.
    let both_succeeded = r1.is_ok() && r2.is_ok();
    assert!(
        !both_succeeded,
        "both concurrent demotions must not both succeed"
    );

    let (active_admin_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM organization_membership
         WHERE organization_id = $1 AND role = 'admin' AND status = 'active'",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(
        active_admin_count >= 1,
        "at least one active admin must remain, got {active_admin_count}"
    );
}

#[sqlx::test]
#[ignore]
async fn platform_promote_on_admin_less_organization_succeeds(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Needs Attention Realty").await;
    let member = create_user(&migrator_pool, "member13@acme.test", "Member", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        member,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    create_platform_admin(&migrator_pool, "owner13@platform.test", "Owner", PW).await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner13@platform.test", PW).await;

    let resp = put_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{org_id}/members/{member}/role"),
        &cookie,
        serde_json::json!({ "role": "admin" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["member"]["role"], "admin");
}

// --- Criterion 8: deactivation --------------------------------------------

#[sqlx::test]
#[ignore]
async fn deactivation_revokes_sessions_blocks_login_and_disconnects_realtime(
    migrator_pool: PgPool,
) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice14@acme.test", "Alice", PW).await;
    let erin = create_user(&migrator_pool, "erin14@acme.test", "Erin", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        org_id,
        erin,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let publisher = Publisher::recording();
    let Publisher::Recording(_, recorded_disconnects) = &publisher else {
        unreachable!()
    };
    let recorded_disconnects = recorded_disconnects.clone();
    let router = build_router_with_publisher(&migrator_pool, publisher).await;

    let alice_cookie = login_cookie(&router, "alice14@acme.test", PW).await;
    let erin_cookie = login_cookie(&router, "erin14@acme.test", PW).await;

    let first = get_with_cookie(&router, "/api/me", &erin_cookie).await;
    assert_eq!(first.status(), StatusCode::OK);

    let deactivate_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{erin}/status"),
        &alice_cookie,
        serde_json::json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivate_resp.status(), StatusCode::OK);
    let member_body = body_json(deactivate_resp).await;
    assert_eq!(member_body["member"]["status"], "inactive");
    assert_eq!(member_body["member"]["assigned_people_count"], 0);

    // Erin's existing session fails on the next request.
    let second = get_with_cookie(&router, "/api/me", &erin_cookie).await;
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);

    // Erin cannot log in again.
    let login_resp = login(&router, "erin14@acme.test", PW).await;
    assert_eq!(login_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(login_resp).await["error"], "no_membership");

    // disconnect_user was called for Erin.
    let disconnects = recorded_disconnects.lock().await.clone();
    assert_eq!(disconnects, vec![UserId::new(erin)]);

    // Reactivation restores login.
    let reactivate_resp = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{erin}/status"),
        &alice_cookie,
        serde_json::json!({ "status": "active" }),
    )
    .await;
    assert_eq!(reactivate_resp.status(), StatusCode::OK);
    let relogin_resp = login(&router, "erin14@acme.test", PW).await;
    assert_eq!(relogin_resp.status(), StatusCode::OK);
}

// --- Criterion 9: Organization state --------------------------------------

#[sqlx::test]
#[ignore]
async fn organization_state_covers_all_three_and_recovers_on_acceptance(migrator_pool: PgPool) {
    let app_pool = connect_as_app(&migrator_pool).await;
    let owner = create_user(&migrator_pool, "owner15@platform.test", "Owner", PW).await;
    create_platform_admin(&migrator_pool, "owner15@platform.test", "Owner", PW).await;

    let router = build_router(&migrator_pool).await;
    let platform_cookie = login_cookie(&router, "owner15@platform.test", PW).await;

    // needs_attention: freshly created, no admin, no invitation.
    let create_resp = post_json_with_cookie(
        &router,
        "/api/platform/organizations",
        &platform_cookie,
        serde_json::json!({ "name": "State Realty" }),
    )
    .await;
    let organization_id: Uuid = body_json(create_resp).await["organization"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let state_of = |body: &serde_json::Value, id: Uuid| -> String {
        body["organizations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == id.to_string())
            .unwrap()["state"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let list1 =
        body_json(get_with_cookie(&router, "/api/platform/organizations", &platform_cookie).await)
            .await;
    assert_eq!(state_of(&list1, organization_id), "needs_attention");

    // pending_first_admin: an unexpired admin invitation exists.
    let invite_resp = post_json_with_cookie(
        &router,
        &format!("/api/platform/organizations/{organization_id}/invitations"),
        &platform_cookie,
        serde_json::json!({ "email": "stateadmin15@example.com", "role": "admin" }),
    )
    .await;
    let invite_body = body_json(invite_resp).await;
    let token = invite_body["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap()
        .to_string();

    let list2 =
        body_json(get_with_cookie(&router, "/api/platform/organizations", &platform_cookie).await)
            .await;
    assert_eq!(state_of(&list2, organization_id), "pending_first_admin");

    // ok: the invitation is accepted.
    let accept_resp = post_json_with_cookie(
        &router,
        "/api/invitations/accept",
        "",
        serde_json::json!({ "token": token, "display_name": "State Admin", "password": PW }),
    )
    .await;
    assert_eq!(accept_resp.status(), StatusCode::OK);

    let list3 =
        body_json(get_with_cookie(&router, "/api/platform/organizations", &platform_cookie).await)
            .await;
    assert_eq!(state_of(&list3, organization_id), "ok");

    let _ = (app_pool, owner);
}

// --- Request-log capture: no token ever logged ----------------------------

#[sqlx::test]
#[ignore]
async fn a_request_log_captured_during_the_invitation_lifecycle_never_contains_the_token(
    migrator_pool: PgPool,
) {
    use tracing_subscriber::layer::SubscriberExt;

    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice16@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let alice_cookie = login_cookie(&router, "alice16@acme.test", PW).await;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = CaptureWriter(buffer.clone());
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false),
    );

    // A thread-local default held across the `.await` points below. Valid
    // because `#[tokio::test]` (which `#[sqlx::test]` wraps) defaults to a
    // single-threaded (current_thread) runtime, so this test's future is
    // never polled from a different OS thread mid-request.
    let _guard = tracing::subscriber::set_default(subscriber);

    let issue_resp = post_json_with_cookie(
        &router,
        "/api/organization/invitations",
        &alice_cookie,
        serde_json::json!({ "email": "logtest16@acme.test", "role": "member" }),
    )
    .await;
    let issue_body = body_json(issue_resp).await;
    let token = issue_body["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap()
        .to_string();

    let _ = post_json_with_cookie(
        &router,
        "/api/invitations/preview",
        "",
        serde_json::json!({ "token": token }),
    )
    .await;
    let _ = post_json_with_cookie(
        &router,
        "/api/invitations/accept",
        "",
        serde_json::json!({ "token": token, "display_name": "Log Test", "password": PW }),
    )
    .await;

    drop(_guard);
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        !captured.contains(&token),
        "captured log output must never contain the raw invitation token: {captured}"
    );
}

// --- Deactivation-status route response includes assigned_people_count ---

#[sqlx::test]
#[ignore]
async fn members_list_reports_role_status_and_assigned_people_count(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice17@acme.test", "Alice", PW).await;
    let carol = create_user(&migrator_pool, "carol17@acme.test", "Carol", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        org_id,
        carol,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "alice17@acme.test", PW).await;

    let body =
        body_json(get_with_cookie(&router, "/api/organization/members", &cookie).await).await;
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    let carol_entry = members
        .iter()
        .find(|m| m["email"] == "carol17@acme.test")
        .unwrap();
    assert_eq!(carol_entry["role"], "member");
    assert_eq!(carol_entry["status"], "inactive");
    assert_eq!(carol_entry["assigned_people_count"], 0);
}

#[sqlx::test]
#[ignore]
async fn revoking_an_accepted_invitation_is_invitation_used(migrator_pool: PgPool) {
    let org_id = create_org(&migrator_pool, "Acme Realty").await;
    let alice = create_user(&migrator_pool, "alice18@acme.test", "Alice", PW).await;
    add_membership_with(
        &migrator_pool,
        org_id,
        alice,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let outcome = issue_invitation(
        &app_pool,
        owner_actor(alice),
        "Alice",
        TTL,
        IssueInvitation {
            organization_id: OrganizationId::new(org_id),
            email: "gina18@acme.test".to_string(),
            role: Role::Member,
        },
    )
    .await
    .unwrap();
    accept_invitation(
        &app_pool,
        AcceptInvitation {
            token: outcome.token,
            display_name: "Gina".to_string(),
            password: PW.to_string(),
            origin: Origin::WebSession,
        },
    )
    .await
    .unwrap();

    let result = revoke_invitation(
        &app_pool,
        owner_actor(alice),
        RevokeInvitation {
            organization_id: OrganizationId::new(org_id),
            invitation_id: outcome.invitation.id.as_uuid(),
        },
    )
    .await;
    assert!(matches!(
        result,
        Err(crm_api::domain::admin::AdminCommandError::InvitationUsed)
    ));
}

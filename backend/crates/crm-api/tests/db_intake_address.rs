//! DB-backed tests for Slice 007a (docs/specs/SLICE_007a.md §11): the
//! Organization intake address — minting, slug collisions, tenant
//! isolation of the admin endpoint, the platform detail field, and the
//! schema assertions (CHECKs, unique index, no UPDATE grant). Run only via
//! ./scripts/check-db.
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::commands::{
    create_organization, AdminCommandError, CreateOrganization,
};
use crm_api::domain::admin::{AdminActor, MembershipStatus, Role};
use crm_api::domain::envelope::Origin;

const PW: &str = "pw";
/// `grant_platform_admin` enforces the password policy; org fixtures do not.
const PLATFORM_PW: &str = "correct-horse-battery-staple-9";

async fn intake_row(pool: &PgPool, org_id: Uuid) -> (String, String) {
    sqlx::query_as("SELECT intake_slug, intake_token FROM organization WHERE id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// --- Minting ----------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn creation_mints_a_slug_from_the_name_and_an_eight_char_token(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Cypress Bay Realty!").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    assert_eq!(slug, "cypress-bay-realty");
    assert_eq!(token.len(), 8);
    assert!(token
        .bytes()
        .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)));
}

#[sqlx::test]
#[ignore]
async fn names_that_slugify_identically_get_a_suffix_and_never_a_name_clash(migrator_pool: PgPool) {
    let first = common::create_org(&migrator_pool, "Acme Realty").await;
    let second = common::create_org(&migrator_pool, "Acme-Realty").await;
    assert_eq!(intake_row(&migrator_pool, first).await.0, "acme-realty");
    assert_eq!(intake_row(&migrator_pool, second).await.0, "acme-realty-2");

    // A genuine name clash is still reported as such (constraint
    // discrimination): same name → OrganizationNameTaken, not a slug error.
    let actor_id = common::fixture_platform_admin(&migrator_pool).await;
    let app_pool = common::connect_as_app(&migrator_pool).await;
    let err = create_organization(
        &app_pool,
        AdminActor {
            actor_user_id: actor_id,
            origin: Origin::Cli,
        },
        CreateOrganization {
            name: "acme realty".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, AdminCommandError::OrganizationNameTaken),
        "{err:?}"
    );
}

// --- The admin endpoint --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn admins_read_their_own_address_members_are_403_and_orgs_never_cross(migrator_pool: PgPool) {
    let acme = common::create_org(&migrator_pool, "Acme Realty").await;
    let best = common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let bob_id = common::create_user(&migrator_pool, "bob@best.test", "Bob", PW).await;
    let carol_id = common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    // Alice and Bob admin their orgs; Carol is a plain member of Acme.
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

    let (acme_slug, acme_token) = intake_row(&migrator_pool, acme).await;
    let (best_slug, best_token) = intake_row(&migrator_pool, best).await;

    let resp = common::get_with_cookie(&router, "/api/organization/intake-address", &alice).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(
        body,
        json!({
            "address": format!("leads-{acme_token}@{acme_slug}.elysianfeld.com"),
            "scheme": "subdomain",
        })
    );

    let resp = common::get_with_cookie(&router, "/api/organization/intake-address", &bob).await;
    let body = common::body_json(resp).await;
    assert_eq!(
        body["address"],
        json!(format!("leads-{best_token}@{best_slug}.elysianfeld.com"))
    );
    assert_ne!(acme_slug, best_slug);
    assert_ne!(acme_token, best_token);

    let resp = common::get_with_cookie(&router, "/api/organization/intake-address", &carol).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(common::body_json(resp).await, json!({"error": "forbidden"}));
}

// --- Platform detail -----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn platform_detail_carries_the_address_top_level_and_members_cannot_use_it(
    migrator_pool: PgPool,
) {
    let (acme, _) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    common::create_platform_admin(&migrator_pool, "owner@platform.test", "Owner", PLATFORM_PW)
        .await;
    let router = common::build_router(&migrator_pool).await;
    let owner = common::login_cookie(&router, "owner@platform.test", PLATFORM_PW).await;
    let alice = common::login_cookie(&router, "alice@acme.test", PW).await;

    let (slug, token) = intake_row(&migrator_pool, acme).await;
    let resp = common::get_with_cookie(
        &router,
        &format!("/api/platform/organizations/{acme}"),
        &owner,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(
        body["intake_address"],
        json!(format!("leads-{token}@{slug}.elysianfeld.com"))
    );
    assert!(
        body["organization"].get("intake_address").is_none(),
        "top-level, not inside"
    );

    let resp = common::get_with_cookie(
        &router,
        &format!("/api/platform/organizations/{acme}"),
        &alice,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// --- Schema (docs/specs/SLICE_007a.md §3) ----------------------------------------

#[sqlx::test]
#[ignore]
async fn intake_columns_have_their_checks_index_and_no_update_grant(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;

    // CHECKs: slug and token formats.
    for (col, bad, constraint) in [
        ("intake_slug", "-bad", "organization_intake_slug_format"),
        ("intake_slug", "Bad", "organization_intake_slug_format"),
        (
            "intake_slug",
            &"a".repeat(41),
            "organization_intake_slug_format",
        ),
        ("intake_token", "short", "organization_intake_token_format"),
        (
            "intake_token",
            "ABCDEFGH",
            "organization_intake_token_format",
        ),
    ] {
        let err = sqlx::query(&format!("UPDATE organization SET {col} = $1 WHERE id = $2"))
            .bind(bad)
            .bind(org_id)
            .execute(&migrator_pool)
            .await
            .unwrap_err();
        let db = err.as_database_error().expect("a CHECK violation");
        assert_eq!(db.code().as_deref(), Some("23514"), "{col} = {bad}");
        assert_eq!(db.constraint(), Some(constraint), "{col} = {bad}");
    }

    // Unique index on the slug.
    let (indexdef,): (String,) = sqlx::query_as(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'organization_intake_slug_idx'",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(indexdef.contains("UNIQUE"), "{indexdef}");

    // No UPDATE grant for crm_app on either column (rotation is a later rung).
    let app_pool = common::connect_as_app(&migrator_pool).await;
    for col in ["intake_slug", "intake_token"] {
        let err = sqlx::query(&format!("UPDATE organization SET {col} = $1 WHERE false"))
            .bind("whatever")
            .execute(&app_pool)
            .await
            .unwrap_err();
        let db = err.as_database_error().expect("a permission error");
        assert_eq!(db.code().as_deref(), Some("42501"), "{col}");
    }
}

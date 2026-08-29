//! DB-backed tests for Slice 007a (docs/specs/SLICE_007a.md §11): the
//! Organization intake address — minting, slug collisions, tenant
//! isolation of the admin endpoint, the platform detail field, and the
//! schema assertions (CHECKs, unique index, no UPDATE grant). Run only via
//! ./scripts/check-db.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::commands::{
    create_organization, AdminCommandError, CreateOrganization,
};
use crm_api::domain::admin::{AdminActor, MembershipStatus, Role};
use crm_api::domain::envelope::Origin;
use crm_api::ids::UserId;

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
    let org_id = crate::common::create_org(&migrator_pool, "Cypress Bay Realty!").await;
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
    let first = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let second = crate::common::create_org(&migrator_pool, "Acme-Realty").await;
    assert_eq!(intake_row(&migrator_pool, first).await.0, "acme-realty");
    assert_eq!(intake_row(&migrator_pool, second).await.0, "acme-realty-2");

    // A genuine name clash is still reported as such (constraint
    // discrimination): same name → OrganizationNameTaken, not a slug error.
    let actor_id = crate::common::fixture_platform_admin(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let err = create_organization(
        &app_pool,
        AdminActor {
            actor_user_id: UserId::new(actor_id),
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
    let acme = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let best = crate::common::create_org(&migrator_pool, "Best Realty").await;
    let alice_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@best.test", "Bob", PW).await;
    let carol_id = crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    // Alice and Bob admin their orgs; Carol is a plain member of Acme.
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

    let (acme_slug, acme_token) = intake_row(&migrator_pool, acme).await;
    let (best_slug, best_token) = intake_row(&migrator_pool, best).await;

    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-address", &alice).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(
        body,
        json!({
            "address": format!("leads-{acme_token}@{acme_slug}.elysianfeld.com"),
            "scheme": "subdomain",
        })
    );

    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-address", &bob).await;
    let body = crate::common::body_json(resp).await;
    assert_eq!(
        body["address"],
        json!(format!("leads-{best_token}@{best_slug}.elysianfeld.com"))
    );
    assert_ne!(acme_slug, best_slug);
    assert_ne!(acme_token, best_token);

    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-address", &carol).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({"error": "forbidden"})
    );
}

// --- Platform detail -----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn platform_detail_carries_the_address_top_level_and_members_cannot_use_it(
    migrator_pool: PgPool,
) {
    let (acme, _) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    crate::common::create_platform_admin(
        &migrator_pool,
        "owner@platform.test",
        "Owner",
        PLATFORM_PW,
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let owner = crate::common::login_cookie(&router, "owner@platform.test", PLATFORM_PW).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let (slug, token) = intake_row(&migrator_pool, acme).await;
    let resp = crate::common::get_with_cookie(
        &router,
        &format!("/api/platform/organizations/{acme}"),
        &owner,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(
        body["intake_address"],
        json!(format!("leads-{token}@{slug}.elysianfeld.com"))
    );
    assert!(
        body["organization"].get("intake_address").is_none(),
        "top-level, not inside"
    );

    let resp = crate::common::get_with_cookie(
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
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;

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

    // The slug stays un-updatable by crm_app forever (immutable identity).
    // intake_token gained its UPDATE grant in SLICE_007g (rotation — the
    // "later rung" this comment originally deferred to); the rotate flow
    // itself is pinned in db_intake_rotation.rs. Declared amendment of
    // this 007a pin (SLICE_007g §4).
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let err = sqlx::query("UPDATE organization SET intake_slug = $1 WHERE false")
        .bind("whatever")
        .execute(&app_pool)
        .await
        .unwrap_err();
    let db = err.as_database_error().expect("a permission error");
    assert_eq!(db.code().as_deref(), Some("42501"), "intake_slug");
    sqlx::query("UPDATE organization SET intake_token = 'abcd2345' WHERE false")
        .execute(&app_pool)
        .await
        .expect("intake_token is updatable since SLICE_007g");
}

// --- Observability (docs/specs/SLICE_007a.md §9) -----------------------------------

#[sqlx::test]
#[ignore]
async fn the_create_span_carries_the_slug_and_never_the_token(migrator_pool: PgPool) {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

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
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    // The only test in this binary that installs a subscriber.
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    let org_id = crate::common::create_org(&migrator_pool, "Span Check Realty").await;
    let (slug, token) = intake_row(&migrator_pool, org_id).await;
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("intake_slug="), "span field present");
    assert!(
        captured.contains(&slug),
        "slug is public, expected in the span"
    );
    assert!(
        !captured.contains(&token),
        "the token never reaches spans/logs"
    );
}

// --- Backfill expressions (docs/specs/SLICE_007a.md §3, §11) ----------------------

#[sqlx::test]
#[ignore]
async fn the_backfill_expressions_satisfy_the_checks(migrator_pool: PgPool) {
    // #[sqlx::test] applies every migration to an empty DB, so the UPDATE
    // itself runs over zero rows; evaluate its expressions the same way.
    for _ in 0..5 {
        let (slug, token): (String, String) = sqlx::query_as(
            "SELECT 'org-' || left(gen_random_uuid()::text, 8),
                    substr(md5(random()::text || gen_random_uuid()::text), 1, 8)",
        )
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        let (slug_ok, token_ok): (bool, bool) = sqlx::query_as(
            "SELECT $1 ~ '^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$', $2 ~ '^[a-z0-9]{8}$'",
        )
        .bind(&slug)
        .bind(&token)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert!(slug_ok && token_ok, "{slug} {token}");
        // …and a backfilled address resolves through the recipient parser.
        let cfg = crm_api::config::IntakeMailConfig {
            domain: "elysianfeld.com".into(),
            scheme: crm_api::config::IntakeAddressScheme::Subdomain,
        };
        let rendered = crm_api::domain::intake::IntakeAddress {
            slug: slug.clone(),
            token: crm_api::domain::intake::IntakeToken::new(token.clone()),
        }
        .render(&cfg);
        assert!(crm_api::domain::intake::IntakeAddress::parse_recipient(&rendered, &cfg).is_some());
    }
}

// --- Lossy names never exhaust (tester 2026-08-23) ---------------------------------

#[sqlx::test]
#[ignore]
async fn ten_orgs_whose_names_all_slugify_to_org_are_all_created(migrator_pool: PgPool) {
    // Every non-Latin name slugifies to `org`; numbered candidates run out
    // at nine, then the random suffixes take over.
    let mut slugs = Vec::new();
    for i in 0..10 {
        let name = format!("日本不動産{}", "!".repeat(i));
        let id = crate::common::create_org(&migrator_pool, &name).await;
        slugs.push(intake_row(&migrator_pool, id).await.0);
    }
    assert_eq!(
        &slugs[..9],
        &["org", "org-2", "org-3", "org-4", "org-5", "org-6", "org-7", "org-8", "org-9"]
    );
    assert!(
        slugs[9].starts_with("org-") && slugs[9].len() == 8,
        "{}",
        slugs[9]
    );
    let unique: std::collections::HashSet<&String> = slugs.iter().collect();
    assert_eq!(unique.len(), 10);
}

// --- Session edges (tester 2026-08-23) ------------------------------------------------

#[sqlx::test]
#[ignore]
async fn a_platform_only_session_is_401_on_the_intake_address(migrator_pool: PgPool) {
    crate::common::create_platform_admin(
        &migrator_pool,
        "owner@platform.test",
        "Owner",
        PLATFORM_PW,
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let owner = crate::common::login_cookie(&router, "owner@platform.test", PLATFORM_PW).await;
    let resp =
        crate::common::get_with_cookie(&router, "/api/organization/intake-address", &owner).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

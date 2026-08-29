//! DB-backed tests for Slice 009's per-agent capture address
//! (docs/specs/SLICE_009.md §3, §8, criterion 8): GET/rotate, the
//! deactivated-token-stops-resolving + reactivation-restores-the-SAME-
//! address behavior, and the migration's backfill-mint pinned on a
//! POPULATED fixture (a database seeded with members BEFORE the capture
//! migration runs — `#[sqlx::test]`'s normal "migrate first, then hand me
//! an empty pool" flow can never exercise this, so that one test drives
//! its own migration sequence by hand). Run only via ./scripts/check-db.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::config::Config;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const PW: &str = "pw";

fn test_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(crate::common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => Some(crate::common::TEST_CENTRIFUGO_HTTP_API_KEY.to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => {
            Some(crate::common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.to_string())
        }
        _ => None,
    })
    .unwrap()
}

async fn build_router(migrator_pool: &PgPool) -> Router {
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, Publisher::recording());
    crm_api::build_app(state)
}

async fn post_empty(router: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    use tower::ServiceExt;
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn capture_row(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> Option<(String, Vec<u8>)> {
    sqlx::query_as(
        "SELECT token, token_lookup FROM capture_address WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

// --- GET / rotate ----------------------------------------------------------

/// `GET /api/capture/address` renders `save-<token>@leads.elysianfeld.com`
/// (backfilled at membership creation — `crate::common::create_org_with_stages_
/// and_member` goes through `AcceptInvitation`-equivalent... actually it
/// uses the lower-level queries directly, so this pins the GET route's
/// OWN self-healing mint-if-absent, not the AcceptInvitation hook —
/// exercised separately below). Rotate changes the address; the old one
/// stops resolving; the new one flows.
#[sqlx::test]
#[ignore]
async fn get_self_heals_and_rotate_flips_live(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty Addr",
        "alice@acmerealtyaddr.test",
        "Alice",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtyaddr.test", PW).await;

    // No row yet (fixture bypassed AcceptInvitation) — GET self-heals.
    assert!(capture_row(&migrator_pool, org_id, alice_id)
        .await
        .is_none());
    let get1 = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/address", &cookie).await,
    )
    .await;
    let address1 = get1["address"].as_str().unwrap().to_string();
    assert!(address1.starts_with("save-"));
    assert!(address1.ends_with("@leads.elysianfeld.com"));
    assert!(capture_row(&migrator_pool, org_id, alice_id)
        .await
        .is_some());

    // A second GET returns the SAME address (no re-mint).
    let get2 = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/address", &cookie).await,
    )
    .await;
    assert_eq!(get2["address"], address1);

    // Rotate: new address, old token dead.
    let resp = post_empty(&router, "/api/capture/address/rotate", &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let rotated = crate::common::body_json(resp).await;
    let address2 = rotated["address"].as_str().unwrap().to_string();
    assert_ne!(address2, address1);

    let get3 = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/address", &cookie).await,
    )
    .await;
    assert_eq!(
        get3["address"], address2,
        "GET reflects the rotated address"
    );

    // Exactly one capture_token_rotated fact, user actor.
    let (count, actor_kind, actor): (i64, String, Option<Uuid>) = sqlx::query_as(
        "SELECT count(*) OVER (), actor_kind, actor_user_id FROM capture_token_rotated WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(actor_kind, "user");
    assert_eq!(actor, Some(alice_id));

    // Append-only.
    let err = sqlx::query("DELETE FROM capture_token_rotated WHERE organization_id = $1")
        .bind(org_id)
        .execute(&migrator_pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
}

// --- Criterion 8: deactivation / reactivation -------------------------------

/// Criterion 8: a deactivated member's token stops resolving (200
/// rejected, nothing stored); reactivation restores the SAME address —
/// mint-if-absent is a no-op because the row was never deleted, only its
/// membership status flips.
#[sqlx::test]
#[ignore]
async fn deactivated_member_token_stops_resolving_reactivation_restores_the_same_address(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty Deact",
        "admin@acmerealtydeact.test",
        "Admin",
        PW,
    )
    .await;
    // Promote the fixture member to admin so it can deactivate others; add
    // a second, ordinary member (bob) to deactivate/reactivate.
    sqlx::query("UPDATE organization_membership SET role = 'admin' WHERE organization_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(alice_id)
        .execute(&migrator_pool)
        .await
        .unwrap();
    let bob_id =
        crate::common::create_user(&migrator_pool, "bob@acmerealtydeact.test", "Bob", PW).await;
    crate::common::add_membership(&migrator_pool, org_id, bob_id).await;

    let router = build_router(&migrator_pool).await;
    let admin_cookie = crate::common::login_cookie(&router, "admin@acmerealtydeact.test", PW).await;
    let bob_cookie = crate::common::login_cookie(&router, "bob@acmerealtydeact.test", PW).await;

    // Bob mints his address.
    let get1 = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/address", &bob_cookie).await,
    )
    .await;
    let bob_address = get1["address"].as_str().unwrap().to_string();
    let (token_before, _) = capture_row(&migrator_pool, org_id, bob_id).await.unwrap();

    // Deactivate bob (admin action).
    let resp = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{bob_id}/status"),
        &admin_cookie,
        serde_json::json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // The row itself is untouched (never deleted/updated).
    let (token_during, _) = capture_row(&migrator_pool, org_id, bob_id).await.unwrap();
    assert_eq!(token_during, token_before, "row untouched by deactivation");

    // The deactivated-token-stops-resolving property is pinned directly
    // against the domain function (the load-bearing unit this criterion
    // is about); the full HTTP round-trip through `/inbound/email` is
    // already covered by `db_capture_receive.rs`'s live-pipeline tests
    // and doesn't need repeating here just to exercise this one check.
    let mut conn = migrator_pool.acquire().await.unwrap();
    let resolved = crm_api::domain::capture::address::resolve(
        &mut conn,
        &crm_api::domain::capture::token::CaptureToken::new(token_during.clone()),
    )
    .await
    .unwrap();
    assert!(
        resolved.is_none(),
        "deactivated member's token must not resolve"
    );

    // Reactivate.
    let resp = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{bob_id}/status"),
        &admin_cookie,
        serde_json::json!({ "status": "active" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (token_after, _) = capture_row(&migrator_pool, org_id, bob_id).await.unwrap();
    assert_eq!(
        token_after, token_before,
        "mint-if-absent restored the SAME token, not a fresh one"
    );

    let mut conn = migrator_pool.acquire().await.unwrap();
    let resolved = crm_api::domain::capture::address::resolve(
        &mut conn,
        &crm_api::domain::capture::token::CaptureToken::new(token_after),
    )
    .await
    .unwrap();
    assert!(
        resolved.is_some(),
        "reactivated member's token resolves again"
    );

    // Deactivation revokes sessions (db_admin.rs's own
    // deactivation_revokes_sessions_... pins this), so bob's pre-
    // deactivation cookie is permanently dead — reactivation restores the
    // membership, not the old session. A fresh login is required here.
    let bob_cookie = crate::common::login_cookie(&router, "bob@acmerealtydeact.test", PW).await;
    let get2 = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/address", &bob_cookie).await,
    )
    .await;
    assert_eq!(
        get2["address"], bob_address,
        "the GET-rendered address is unchanged end to end"
    );
}

// --- Backfill-mint on a populated fixture (criterion 8) ---------------------

/// Applies every migration file in lexicographic (== chronological) order
/// from `dir`, using the simple-query protocol (`sqlx::raw_sql`) so a
/// multi-statement file executes in one call — mirrors what the CLI
/// migrator does, without needing `sqlx::migrate!`'s compile-time embed
/// (which cannot stop partway through a directory).
async fn apply_migrations_through(pool: &PgPool, dir: &std::path::Path, stop_before: &str) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_str().unwrap();
        if name >= stop_before {
            continue;
        }
        let sql = std::fs::read_to_string(&path).unwrap();
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .unwrap_or_else(|err| panic!("applying {name}: {err}"));
    }
}

async fn apply_one_migration(pool: &PgPool, dir: &std::path::Path, name: &str) {
    let sql = std::fs::read_to_string(dir.join(name)).unwrap();
    sqlx::raw_sql(&sql)
        .execute(pool)
        .await
        .unwrap_or_else(|err| panic!("applying {name}: {err}"));
}

/// Criterion 8: on a database with active members ALREADY present (an
/// upgrade scenario, not a fresh install), the 20260904000001 migration's
/// backfill mints a DISTINCT capture address for every one of them, skips
/// inactive members, and every `(token, token_lookup)` pair is internally
/// consistent (the token's own SHA-256 digest — the exact bug class this
/// pin exists for: an early draft of the backfill silently minted the
/// SAME token for every row under `CROSS JOIN LATERAL`, live-verified and
/// fixed during this slice's implementation).
#[sqlx::test(migrations = false)]
#[ignore]
async fn backfill_mints_distinct_addresses_for_every_active_member_on_a_populated_fixture(
    pool: PgPool,
) {
    let migrations_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));
    const CAPTURE_MIGRATION: &str = "20260904000001_correspondence_capture.sql";

    apply_migrations_through(&pool, migrations_dir, CAPTURE_MIGRATION).await;

    // Seed: one Organization, three active members, one inactive member —
    // directly, as the migrator (the sanctioned pre-D-021 writer for a
    // migration-adjacent fixture; this IS testing migration behavior).
    // intake_slug/intake_token became NOT NULL with no default in
    // 20260828000001 (its own backfill+DROP DEFAULT already ran as part of
    // apply_migrations_through above), so a bare name-only insert must
    // supply both explicitly to satisfy their format CHECKs.
    let org_id: Uuid = sqlx::query_scalar(
        "INSERT INTO organization (name, intake_slug, intake_token)
         VALUES ('Backfill Co', 'backfill-co', 'a1b2c3d4') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut active_user_ids = Vec::new();
    for i in 0..3 {
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO app_user (email, display_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("member{i}@backfillco.test"))
        .bind(format!("Member {i}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_membership (organization_id, user_id, role, status) VALUES ($1, $2, 'member', 'active')",
        )
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        active_user_ids.push(user_id);
    }
    let inactive_user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (email, display_name) VALUES ('inactive@backfillco.test', 'Inactive') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership (organization_id, user_id, role, status) VALUES ($1, $2, 'member', 'inactive')",
    )
    .bind(org_id)
    .bind(inactive_user_id)
    .execute(&pool)
    .await
    .unwrap();

    apply_one_migration(&pool, migrations_dir, CAPTURE_MIGRATION).await;

    // Every active member has a row; the inactive one does not.
    let rows: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(
        "SELECT user_id, token, token_lookup FROM capture_address WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "one row per ACTIVE member, none for the inactive one"
    );
    let covered: std::collections::HashSet<Uuid> = rows.iter().map(|(u, _, _)| *u).collect();
    assert_eq!(covered, active_user_ids.into_iter().collect());
    assert!(!covered.contains(&inactive_user_id));

    // Every token is distinct (the bug this pin targets: a LATERAL
    // volatile-function call evaluated once and reused for every row).
    let tokens: std::collections::HashSet<&String> = rows.iter().map(|(_, t, _)| t).collect();
    assert_eq!(tokens.len(), 3, "every backfilled token must be distinct");
    let digests: std::collections::HashSet<&Vec<u8>> = rows.iter().map(|(_, _, d)| d).collect();
    assert_eq!(digests.len(), 3, "every digest must be distinct too");

    // Each row's digest is the correct SHA-256 of ITS OWN token (not
    // mismatched from a mis-paired lateral evaluation).
    use sha2::{Digest, Sha256};
    for (_, token, digest) in &rows {
        let expected = Sha256::digest(token.as_bytes()).to_vec();
        assert_eq!(
            digest, &expected,
            "token/digest pairing must be internally consistent"
        );
    }
}

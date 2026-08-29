//! DB-backed tests for Slice 009's unmatched held queue
//! (docs/specs/SLICE_009.md §8, §4.4's transition matrix, criterion 4),
//! plus §9's cross-org isolation pins and the link-vs-dismiss concurrent
//! race. Run only via ./scripts/check-db.

use axum::http::StatusCode;
use axum::Router;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::config::{Config, RawPayloadKey};
use crm_api::domain::raw_payload::crypto;
use crm_api::ids::{CorrespondenceRawId, OrganizationId};
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

async fn build_router(migrator_pool: &PgPool, publisher: Publisher) -> Router {
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, publisher);
    crm_api::build_app(state)
}

async fn first_stage_id(pool: &PgPool, org_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(pool: &PgPool, org_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The suite's fixed raw-payload key, decoded (mirrors
/// `crate::common::TEST_RAW_PAYLOAD_KEY_HEX`'s value: 32 bytes of `0x11`) — this
/// file seals real correspondence content directly rather than going
/// through the HTTP endpoint, so it needs the key `test_config()`'s
/// `AppState` will also use to decrypt it.
fn test_raw_payload_key() -> RawPayloadKey {
    RawPayloadKey::new([0x11; 32])
}

/// A raw-only fixture (Phase A only, `processed=true`) so held-row tests
/// don't need to run the full mime/ladder pipeline to reach the held
/// queue. The content is still a real, sealed, parseable minimal email —
/// `direction` and `counterparty_email` come from the stored
/// `capture_message` columns, never re-derived, but
/// `link_unmatched_attempt`'s Held branch always decrypts and parses the
/// raw row to derive occurred_at/message_id/thread_key for the fact it
/// writes, so opaque/garbage ciphertext 500s there. The fixture's own id
/// is folded into the Message-ID so every call also gets a distinct
/// `content_hmac` (the table's `(organization_id, content_hmac)` unique
/// index would otherwise collide across the 201 rows
/// `unmatched_list_caps_at_200_with_truncated_flag` inserts).
async fn insert_correspondence_raw_fixture(pool: &PgPool, org_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let plaintext = format!(
        "From: stranger@example.com\r\nTo: agent@example.com\r\nSubject: x\r\nMessage-ID: <{id}@example.test>\r\n\r\nbody\r\n"
    );
    let key = test_raw_payload_key();
    let sealed = crypto::seal_correspondence(
        &key,
        OrganizationId::new(org_id),
        CorrespondenceRawId::new(id),
        plaintext.as_bytes(),
    )
    .unwrap();
    let hmac = crypto::content_hmac(&key, plaintext.as_bytes());

    sqlx::query_scalar(
        "INSERT INTO correspondence_raw
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
         VALUES ($1, $2, now(), $3, $4, $5, $6, true) RETURNING id",
    )
    .bind(id)
    .bind(org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(hmac.to_vec())
    .bind(plaintext.len() as i32)
    .fetch_one(pool)
    .await
    .unwrap()
}

struct HeldFixture {
    org_id: Uuid,
    alice_id: Uuid,
    alice_email: String,
    person_a: Uuid,
    person_b: Uuid,
    held_id: Uuid,
    counterparty_email: String,
}

/// An org, active member alice (the attributed agent), two candidate
/// Persons to link to, and one held row with a counterparty email and a
/// given `direction_hint`.
async fn held_fixture(pool: &PgPool, org_name: &str, direction_hint: &str) -> HeldFixture {
    let slug: String = org_name.to_lowercase().replace(' ', "");
    let alice_email = format!("alice@{slug}.test");
    let (org_id, alice_id) =
        crate::common::create_org_with_stages_and_member(pool, org_name, &alice_email, "Alice", PW)
            .await;
    let stage_id = first_stage_id(pool, org_id).await;
    let person_a = insert_person(pool, org_id, stage_id).await;
    let person_b = insert_person(pool, org_id, stage_id).await;
    let raw_id = insert_correspondence_raw_fixture(pool, org_id).await;
    let counterparty_email = format!("stranger-{}@example.com", Uuid::new_v4());
    let held_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capture_message
            (organization_id, agent_user_id, correspondence_raw_id, counterparty_email,
             direction_hint, captured_at, status)
         VALUES ($1, $2, $3, $4, $5, now(), 'held') RETURNING id",
    )
    .bind(org_id)
    .bind(alice_id)
    .bind(raw_id)
    .bind(&counterparty_email)
    .bind(direction_hint)
    .fetch_one(pool)
    .await
    .unwrap();
    HeldFixture {
        org_id,
        alice_id,
        alice_email,
        person_a,
        person_b,
        held_id,
        counterparty_email,
    }
}

async fn link(
    router: &Router,
    cookie: &str,
    id: Uuid,
    person_id: Uuid,
    add_contact_method: bool,
) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        &format!("/api/capture/unmatched/{id}/link"),
        cookie,
        json!({ "person_id": person_id, "add_contact_method": add_contact_method }),
    )
    .await
}

async fn dismiss(router: &Router, cookie: &str, id: Uuid) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        &format!("/api/capture/unmatched/{id}/dismiss"),
        cookie,
        json!({}),
    )
    .await
}

async fn status_of(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, counterparty_email FROM capture_message WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// --- Criterion 4: visibility ----------------------------------------------

/// Criterion 4: the held row is visible ONLY to the attributed agent —
/// another active member (and an admin) see an empty list via the SAME
/// endpoint, since it is always scoped to the session's own user id.
#[sqlx::test]
#[ignore]
async fn held_row_is_visible_only_to_the_attributed_agent(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Held Vis", "inbound").await;
    let bob_id = crate::common::create_user(
        &migrator_pool,
        &format!("bob-{}@x.test", f.org_id),
        "Bob",
        PW,
    )
    .await;
    crate::common::add_membership(&migrator_pool, f.org_id, bob_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let alice_cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;
    let alice_list = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/unmatched", &alice_cookie).await,
    )
    .await;
    let items = alice_list["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], f.held_id.to_string());
    assert_eq!(items[0]["counterparty_email"], f.counterparty_email);
    assert_eq!(items[0]["direction_hint"], "inbound");
    assert_eq!(items[0]["status"], "held");

    let bob_cookie =
        crate::common::login_cookie(&router, &format!("bob-{}@x.test", f.org_id), PW).await;
    let bob_list = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/unmatched", &bob_cookie).await,
    )
    .await;
    assert_eq!(bob_list["items"].as_array().unwrap().len(), 0);
}

/// Criterion 4 + D-042.3: link/dismiss on another agent's row (or an
/// admin's) 404s exactly like a nonexistent id.
#[sqlx::test]
#[ignore]
async fn link_and_dismiss_are_404_for_non_attributed_agents_admins_included(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Held 404", "inbound").await;
    let admin_email = format!("admin-{}@x.test", f.org_id);
    let admin_id = crate::common::create_user(&migrator_pool, &admin_email, "Admin", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        admin_id,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let admin_cookie = crate::common::login_cookie(&router, &admin_email, PW).await;

    let resp = link(&router, &admin_cookie, f.held_id, f.person_a, false).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = dismiss(&router, &admin_cookie, f.held_id).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Untouched.
    let (status, _) = status_of(&migrator_pool, f.held_id).await;
    assert_eq!(status, "held");
}

// --- Direction from the stored hint, not re-matched ------------------------

/// Direction is read from the held row's own `direction_hint` — inbound
/// and outbound both write the correct direction on the fact row (and an
/// outbound link ALSO writes the D-042.4 auto-attempt, exactly as the
/// live pipeline does).
#[sqlx::test]
#[ignore]
async fn link_writes_direction_from_the_stored_hint_and_outbound_writes_the_auto_attempt(
    migrator_pool: PgPool,
) {
    for (hint, expect_attempt) in [("inbound", false), ("outbound", true)] {
        let f = held_fixture(&migrator_pool, &format!("Acme Realty Link {hint}"), hint).await;
        let router = build_router(&migrator_pool, Publisher::recording()).await;
        let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

        let resp = link(&router, &cookie, f.held_id, f.person_a, false).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let (direction,): (String,) = sqlx::query_as(
            "SELECT direction FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(f.org_id)
        .bind(f.person_a)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(direction, hint);

        let (attempt_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM contact_attempted WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(f.org_id)
        .bind(f.person_a)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(
            attempt_count,
            if expect_attempt { 1 } else { 0 },
            "hint={hint}"
        );

        let (status, counterparty) = status_of(&migrator_pool, f.held_id).await;
        assert_eq!(status, "linked");
        assert_eq!(
            counterparty, None,
            "D-015 §4: nulled on the terminal transition"
        );
    }
}

/// `add_contact_method: true` adds the held counterparty address as a
/// contact method on the linked Person.
#[sqlx::test]
#[ignore]
async fn link_optionally_adds_the_counterparty_as_a_contact_method(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Link CM", "inbound").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

    let resp = link(&router, &cookie, f.held_id, f.person_a, true).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (value, normalized): (String, String) = sqlx::query_as(
        "SELECT value, normalized_value FROM contact_method WHERE person_id = $1 AND kind = 'email'",
    )
    .bind(f.person_a)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(value, f.counterparty_email);
    assert_eq!(normalized, f.counterparty_email.to_lowercase());
}

// --- Transition matrix (spec §4.4/§8) --------------------------------------

/// Re-link with the SAME Person is an idempotent no-op (still exactly one
/// fact row); re-link with a DIFFERENT Person is 409, and creates nothing.
#[sqlx::test]
#[ignore]
async fn relink_same_person_is_a_noop_different_person_is_409(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Relink", "inbound").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

    let resp = link(&router, &cookie, f.held_id, f.person_a, false).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = link(&router, &cookie, f.held_id, f.person_a, false).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "same person: idempotent no-op"
    );
    let (count_a,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2")
            .bind(f.org_id)
            .bind(f.person_a)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count_a, 1, "no duplicate row from the no-op re-link");

    let resp = link(&router, &cookie, f.held_id, f.person_b, false).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let (count_b,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2")
            .bind(f.org_id)
            .bind(f.person_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count_b, 0, "the conflicting link creates nothing");
}

/// Dismiss-after-dismiss is an idempotent no-op; link-after-dismissed and
/// dismiss-after-linked are both 409 (the two remaining terminal-state
/// crossings, symmetric with the re-link case).
#[sqlx::test]
#[ignore]
async fn dismiss_is_idempotent_and_cross_terminal_transitions_conflict(migrator_pool: PgPool) {
    let f1 = held_fixture(&migrator_pool, "Acme Realty X1", "inbound").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie1 = crate::common::login_cookie(&router, &f1.alice_email, PW).await;

    let resp = dismiss(&router, &cookie1, f1.held_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = dismiss(&router, &cookie1, f1.held_id).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "dismiss-after-dismiss is a no-op"
    );

    let resp = link(&router, &cookie1, f1.held_id, f1.person_a, false).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT, "link-after-dismissed");

    let f2 = held_fixture(&migrator_pool, "Acme Realty X2", "outbound").await;
    let cookie2 = crate::common::login_cookie(&router, &f2.alice_email, PW).await;
    let resp = link(&router, &cookie2, f2.held_id, f2.person_a, false).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = dismiss(&router, &cookie2, f2.held_id).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT, "dismiss-after-linked");
}

/// D-015 §4: `counterparty_email` is NULLED on the dismiss terminal
/// transition too, not just link (a no-DELETE table must not retain
/// third-party PII forever past either terminal state).
#[sqlx::test]
#[ignore]
async fn counterparty_email_is_nulled_on_dismiss(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Null", "inbound").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

    let resp = dismiss(&router, &cookie, f.held_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (status, counterparty) = status_of(&migrator_pool, f.held_id).await;
    assert_eq!(status, "dismissed");
    assert_eq!(counterparty, None);
}

// --- Criterion 11: the 200 cap + truncated flag ---------------------------

/// The held list caps at 200 with a `truncated` flag (201 rows inserted
/// directly for speed — no need to run the full pipeline 201 times).
#[sqlx::test]
#[ignore]
async fn unmatched_list_caps_at_200_with_truncated_flag(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Cap", "inbound").await;
    for _ in 0..200 {
        let raw_id = insert_correspondence_raw_fixture(&migrator_pool, f.org_id).await;
        sqlx::query(
            "INSERT INTO capture_message
                (organization_id, agent_user_id, correspondence_raw_id, counterparty_email,
                 direction_hint, captured_at, status)
             VALUES ($1, $2, $3, $4, 'inbound', now(), 'held')",
        )
        .bind(f.org_id)
        .bind(f.alice_id)
        .bind(raw_id)
        .bind(format!("bulk-{}@example.com", Uuid::new_v4()))
        .execute(&migrator_pool)
        .await
        .unwrap();
    }
    // 1 (from held_fixture) + 200 = 201 total held rows.
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;
    let list = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/capture/unmatched", &cookie).await,
    )
    .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 200);
    assert_eq!(list["truncated"], true);
}

// --- Cross-org isolation (§9): a genuinely different Organization --------

/// §9 cross-org isolation, sharpened beyond the same-org "another agent"
/// 404 above (`link_and_dismiss_are_404_for_non_attributed_agents_admins_included`):
/// linking an org-A held row to an ORG-B Person 404s via H1's
/// `lock_person` organization guard in `commands.rs` and writes nothing
/// for that Person in EITHER org; dismissing an org-B held id from an
/// org-A session also 404s, since the row's own `organization_id` never
/// matches org A's scope in `store::lock_for_transition`.
#[sqlx::test]
#[ignore]
async fn cross_org_link_and_dismiss_are_404_and_write_nothing(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Cross A", "inbound").await;
    let org_b = held_fixture(&migrator_pool, "Acme Realty Cross B", "inbound").await;

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let alice_cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

    // (a) link org-A's held row to an ORG-B Person -> 404, nothing written
    // for that Person in either org.
    let resp = link(&router, &alice_cookie, f.held_id, org_b.person_a, false).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    for org_id in [f.org_id, org_b.org_id] {
        let (fact_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(org_id)
        .bind(org_b.person_a)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(fact_count, 0, "org={org_id}");

        let (attempt_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM contact_attempted WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(org_id)
        .bind(org_b.person_a)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(attempt_count, 0, "org={org_id}");
    }
    let (status, _) = status_of(&migrator_pool, f.held_id).await;
    assert_eq!(
        status, "held",
        "org-A's row untouched by the rejected cross-org link"
    );

    // (b) dismiss an ORG-B held id from the org-A session -> 404 too.
    let resp = dismiss(&router, &alice_cookie, org_b.held_id).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let (status_b, _) = status_of(&migrator_pool, org_b.held_id).await;
    assert_eq!(status_b, "held", "org-B's row untouched");
}

// --- Concurrency: link vs dismiss race on the same held row ---------------

/// Concurrent link+dismiss on the SAME held row (two tasks, `tokio::join!`,
/// the 007b race-test pattern): `store::lock_for_transition`'s `FOR
/// UPDATE` serializes the two transactions, so exactly one terminal
/// transition wins — the loser observes the winner's already-terminal
/// status (post-commit, once its lock wait releases) and 409s via the
/// SAME cross-terminal-conflict branch the sequential
/// `dismiss_is_idempotent_and_cross_terminal_transitions_conflict` test
/// exercises above. Final status is linked XOR dismissed, never held; the
/// fact row exists iff linked; `counterparty_email` is NULL either way
/// (D-015 §4, unconditional on both terminal transitions).
#[sqlx::test]
#[ignore]
async fn concurrent_link_and_dismiss_on_the_same_row_exactly_one_wins(migrator_pool: PgPool) {
    let f = held_fixture(&migrator_pool, "Acme Realty Race Link Dismiss", "inbound").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let cookie = crate::common::login_cookie(&router, &f.alice_email, PW).await;

    let r1 = router.clone();
    let r2 = router.clone();
    let cookie1 = cookie.clone();
    let cookie2 = cookie.clone();
    let held_id = f.held_id;
    let person_a = f.person_a;
    let (link_resp, dismiss_resp) = tokio::join!(
        async move { link(&r1, &cookie1, held_id, person_a, false).await },
        async move { dismiss(&r2, &cookie2, held_id).await },
    );

    let statuses = [link_resp.status(), dismiss_resp.status()];
    let ok_count = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();
    assert_eq!(ok_count, 1, "exactly one side wins: {statuses:?}");
    assert_eq!(
        conflict_count, 1,
        "the loser sees a cross-terminal conflict: {statuses:?}"
    );

    let (status, counterparty) = status_of(&migrator_pool, f.held_id).await;
    assert_ne!(status, "held", "must have left the held state");
    assert_eq!(
        counterparty, None,
        "D-015 §4: nulled on either terminal transition"
    );

    let (fact_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_a)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    if status == "linked" {
        assert_eq!(link_resp.status(), StatusCode::OK);
        assert_eq!(fact_count, 1, "the linked winner writes the fact row");
    } else {
        assert_eq!(status, "dismissed");
        assert_eq!(dismiss_resp.status(), StatusCode::OK);
        assert_eq!(fact_count, 0, "the dismissed winner writes nothing");
    }
}

//! DB-backed tests for the Today `client_replied` arm
//! (docs/specs/SLICE_009.md §6). Fixtures write `correspondence_captured`/
//! `correspondence_raw` rows directly (migrator-role, test setup — the
//! `db_today.rs` precedent for `contact_attempted`/`inquiry` fixtures),
//! so these tests isolate the SQL precedence logic from the live
//! mime/ladder pipeline (already covered end-to-end by
//! `db_capture_receive.rs`). Run only via ./scripts/check-db.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

async fn first_stage_id(pool: &PgPool, org_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    received_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, 'website', $4) RETURNING id",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(received_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_attempt(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached')",
    )
    .bind(org_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_correspondence_raw(pool: &PgPool, org_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO correspondence_raw
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
         VALUES ($1, $2, now(), $3, $4, $5, 0, true) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(vec![0u8; 24])
    .bind(vec![0u8; 16])
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn insert_correspondence(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    agent_user_id: Uuid,
    direction: &str,
    occurred_at: DateTime<Utc>,
) {
    let raw_id = insert_correspondence_raw(pool, org_id).await;
    sqlx::query(
        "INSERT INTO correspondence_captured
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, person_id, agent_user_id, direction, via,
             correspondence_raw_id, backdated)
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, $6, 'cc', $7, false)",
    )
    .bind(org_id)
    .bind(agent_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(direction)
    .bind(raw_id)
    .execute(pool)
    .await
    .unwrap();
}

fn hours_ago(h: i64) -> DateTime<Utc> {
    Utc::now() - ChronoDuration::hours(h)
}

async fn today_item(
    router: &axum::Router,
    cookie: &str,
    person_id: Uuid,
) -> Option<serde_json::Value> {
    let today = crate::common::body_json(
        crate::common::get_with_cookie(router, "/api/today", cookie).await,
    )
    .await;
    today["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["person"]["id"] == person_id.to_string())
        .cloned()
}

/// Criterion (spec §6): an unanswered inbound correspondence arms
/// `client_replied` with `waiting_since` = its own `occurred_at`; a LATER
/// contact_attempted clears it.
#[sqlx::test]
#[ignore]
async fn inbound_correspondence_arms_client_replied_and_a_later_attempt_clears_it(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR1",
        "alice@acmerealtycr1.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    // Every candidate needs an Inquiry regardless of which arm qualifies
    // (spec §6: the existing latest.id IS NOT NULL constraint is kept).
    insert_inquiry(&migrator_pool, org_id, person_id, hours_ago(240)).await;
    // An OLD attempt already answered the Inquiry-based arm, so ONLY
    // client_replied should qualify here.
    insert_contact_attempt(&migrator_pool, org_id, person_id, hours_ago(200)).await;
    // Computed once and reused below: `hours_ago` reads the real clock, so
    // calling it a second time for the assertion (after the intervening
    // inserts/router build/login) would compare against a DIFFERENT
    // instant, off by however much wall-clock time that setup took.
    let reply_at = hours_ago(2);
    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "inbound",
        reply_at,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtycr1.test", "pw").await;

    let item = today_item(&router, &cookie, person_id).await.unwrap();
    assert_eq!(item["priority"], "high", "a <24h reply ranks high");
    let codes: Vec<&str> = item["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, vec!["client_replied"]);
    assert_eq!(
        item["reasons"][0]["occurred_at"],
        reply_at.to_rfc3339().replace("+00:00", "Z")
    );

    // A later attempt clears it.
    insert_contact_attempt(&migrator_pool, org_id, person_id, hours_ago(1)).await;
    assert_eq!(today_item(&router, &cookie, person_id).await, None);
}

/// Cleared by a later CAPTURED OUTBOUND too, independent of any
/// `contact_attempted` row (the correspondence-level clearing path, spec
/// §6: "Cleared by any at-or-after attempt or captured outbound").
#[sqlx::test]
#[ignore]
async fn a_later_outbound_correspondence_clears_client_replied_without_a_contact_attempt(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR2",
        "alice@acmerealtycr2.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, person_id, hours_ago(240)).await;
    insert_contact_attempt(&migrator_pool, org_id, person_id, hours_ago(200)).await;
    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "inbound",
        hours_ago(5),
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtycr2.test", "pw").await;
    assert!(today_item(&router, &cookie, person_id).await.is_some());

    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "outbound",
        hours_ago(1),
    )
    .await;
    assert_eq!(today_item(&router, &cookie, person_id).await, None);
}

/// `client_replied` WINS the reason slot over the Inquiry-based trio: a
/// Person with BOTH an unanswered old Inquiry AND a later inbound reply
/// shows client_replied alone, and `waiting_since`/priority follow the
/// REPLY, not the Inquiry.
#[sqlx::test]
#[ignore]
async fn client_replied_wins_precedence_over_the_unanswered_inquiry_arm(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR3",
        "alice@acmerealtycr3.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    // NO contact_attempted at all: the Inquiry-based arm ALSO qualifies.
    insert_inquiry(&migrator_pool, org_id, person_id, hours_ago(72)).await;
    // Computed once and reused below — see the comment in
    // inbound_correspondence_arms_client_replied_and_a_later_attempt_
    // clears_it on why calling hours_ago twice would compare against two
    // different instants.
    let reply_at = hours_ago(1);
    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "inbound",
        reply_at,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtycr3.test", "pw").await;

    let item = today_item(&router, &cookie, person_id).await.unwrap();
    let codes: Vec<&str> = item["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        vec!["client_replied"],
        "wins over new_inquiry/no_contact_attempt/repeat_inquiry"
    );
    assert_eq!(
        item["priority"], "high",
        "the REPLY's freshness (1h ago), not the 72h-old Inquiry's"
    );
    assert_eq!(
        item["waiting_since"],
        reply_at.to_rfc3339().replace("+00:00", "Z"),
        "the reply's occurred_at, not the Inquiry's received_at"
    );
}

/// `client_replied` requires assignment to the VIEWER — a Person assigned
/// to someone else, with an inbound reply, never appears on alice's Today
/// via this arm (or any other).
#[sqlx::test]
#[ignore]
async fn client_replied_requires_assignment_to_the_viewer(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR4",
        "alice@acmerealtycr4.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol@acmerealtycr4.test", "Carol", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, carol_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    // Assigned to CAROL, not alice.
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(carol_id)).await;
    insert_inquiry(&migrator_pool, org_id, person_id, hours_ago(72)).await;
    // Captured under ALICE's token (agent_user_id = alice), but the
    // PERSON is assigned to carol — assignment governs Today ownership,
    // not which agent's address captured the mail.
    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "inbound",
        hours_ago(1),
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmerealtycr4.test", "pw").await;
    assert_eq!(
        today_item(&router, &alice_cookie, person_id).await,
        None,
        "not alice's"
    );

    let carol_cookie = crate::common::login_cookie(&router, "carol@acmerealtycr4.test", "pw").await;
    let item = today_item(&router, &carol_cookie, person_id).await.unwrap();
    assert_eq!(item["reasons"][0]["code"], "client_replied");
}

/// The kept `latest.id IS NOT NULL` constraint (spec §6, "every Person
/// has one; stated"): a Person with an inbound correspondence but ZERO
/// Inquiries is excluded entirely, never a client_replied item.
#[sqlx::test]
#[ignore]
async fn a_person_with_no_inquiry_never_qualifies_even_with_a_reply(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR5",
        "alice@acmerealtycr5.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    // No insert_inquiry call at all.
    insert_correspondence(
        &migrator_pool,
        org_id,
        person_id,
        alice_id,
        "inbound",
        hours_ago(1),
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtycr5.test", "pw").await;
    assert_eq!(today_item(&router, &cookie, person_id).await, None);
}

/// A genuinely-unanswered OLD backdated forward still arms Today (spec
/// §6: "Old backdated forwards arm Today only if genuinely unanswered" —
/// age alone never disqualifies it).
#[sqlx::test]
#[ignore]
async fn an_old_backdated_inbound_row_still_arms_today_if_genuinely_unanswered(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty CR6",
        "alice@acmerealtycr6.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, person_id, hours_ago(24 * 60)).await;
    // A retroactive forward's occurred_at is far in the past (backdated),
    // but nothing has answered it since.
    let raw_id = insert_correspondence_raw(&migrator_pool, org_id).await;
    sqlx::query(
        "INSERT INTO correspondence_captured
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, person_id, agent_user_id, direction, via,
             correspondence_raw_id, backdated)
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, 'inbound', 'forward', $6, true)",
    )
    .bind(org_id)
    .bind(alice_id)
    .bind(hours_ago(24 * 30))
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(raw_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmerealtycr6.test", "pw").await;
    let item = today_item(&router, &cookie, person_id).await.unwrap();
    assert_eq!(item["reasons"][0]["code"], "client_replied");
    assert_eq!(item["priority"], "normal", "30 days old is not <24h fresh");
}

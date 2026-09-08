//! DB-backed tests for the People filter vocabulary
//! (docs/specs/SLICE_011a.md §8, criteria 4-10). Harness + fixture style
//! per `db_people.rs` (isolation/cap patterns) and
//! `db_today_client_replied.rs` (direct migrator-pool fixture rows for
//! correspondence/contact-attempt facts). Run only via ./scripts/check-db.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};

// --- Fixture helpers --------------------------------------------------------

fn hours_ago(h: i64) -> DateTime<Utc> {
    Utc::now() - ChronoDuration::hours(h)
}

fn days_ago(d: i64) -> DateTime<Utc> {
    Utc::now() - ChronoDuration::days(d)
}

/// Minimal percent-encoding sufficient for a JSON filter value in a query
/// string — no dependency needed for the handful of characters JSON uses
/// (`{}[]":, ` etc.) that a bare URI string cannot carry.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn filter_uri(path: &str, filter: &Value) -> String {
    format!("{path}?filter={}", percent_encode(&filter.to_string()))
}

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

async fn insert_inquiry_with_id(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_inquiry(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) -> Uuid {
    let id = Uuid::new_v4();
    insert_inquiry_with_id(pool, org_id, person_id, id, source, received_at).await;
    id
}

/// Gate-speedup lever 4: single-statement equivalent of calling
/// `insert_inquiry` `count` times against the same person with sources
/// `source0000..source{count-1:04}` (matches the `format!("source{i:04}")`
/// naming the sequential version used) and the same `received_at` for
/// every row, exactly as the original loop's repeated `received_at`
/// argument did.
async fn insert_inquiries_batch(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    received_at: DateTime<Utc>,
    count: i64,
) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         SELECT $1, $2, gen_random_uuid(), 'source' || lpad(s.i::text, 4, '0'), $3
         FROM generate_series(0, $4 - 1) AS s(i)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(received_at)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempt(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached') RETURNING id",
    )
    .bind(org_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A correction row (docs/specs/SLICE_006c.md §2): a NEW `contact_attempted`
/// row whose `occurred_at` is INHERITED from `corrects_id`'s original
/// (`person/queries.rs` invariant) — passed explicitly here, matching the
/// original's own `occurred_at`, so the fixture mirrors production exactly.
async fn insert_contact_attempt_correction(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    actor_user_id: Uuid,
    corrects_id: Uuid,
    inherited_occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             corrects_id, person_id, channel, outcome)
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'left_message') RETURNING id",
    )
    .bind(org_id)
    .bind(actor_user_id)
    .bind(inherited_occurred_at)
    .bind(Uuid::new_v4())
    .bind(corrects_id)
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
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
    backdated: bool,
) {
    let raw_id = insert_correspondence_raw(pool, org_id).await;
    sqlx::query(
        "INSERT INTO correspondence_captured
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
             occurred_at, correlation_id, person_id, agent_user_id, direction, via,
             correspondence_raw_id, backdated)
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, $6, 'cc', $7, $8)",
    )
    .bind(org_id)
    .bind(agent_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(direction)
    .bind(raw_id)
    .bind(backdated)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_method(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    kind: &str,
    value: &str,
) {
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(kind)
    .bind(value)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

/// Gate-speedup lever 4: single-statement equivalent of `count` sequential
/// bare-person inserts, one per caller (`cap_truncates_to_500_...`). Each
/// row gets a distinct `created_at` (`count` seconds apart) so the
/// caller's `created_at DESC` ordering assertion stays meaningful — a
/// single-transaction batch would otherwise give every row an identical
/// `now()` and make that assertion vacuous.
async fn insert_bare_people_batch(pool: &PgPool, org_id: Uuid, stage_id: Uuid, count: i64) {
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, created_at)
         SELECT $1, $2, now() - make_interval(secs => s.i)
         FROM generate_series(0, $3 - 1) AS s(i)",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn people_ids(router: &axum::Router, cookie: &str, uri: &str) -> Vec<String> {
    let body =
        crate::common::body_json(crate::common::get_with_cookie(router, uri, cookie).await).await;
    body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

// --- 4. Per-axis positive + negative -----------------------------------

#[sqlx::test]
#[ignore]
async fn stage_clause_matches_only_the_named_stages(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F1",
        "alice@acmef1.test",
        "Alice",
        "pw",
    )
    .await;
    let stages: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM stage WHERE organization_id = $1 ORDER BY position")
            .bind(org_id)
            .fetch_all(&migrator_pool)
            .await
            .unwrap();
    let lead_id = stages[0].0;
    let hot_id = stages[1].0;
    let nurture_id = stages[2].0;

    let lead_person = insert_person(&migrator_pool, org_id, lead_id, Some(alice_id)).await;
    let hot_person = insert_person(&migrator_pool, org_id, hot_id, Some(alice_id)).await;
    let nurture_person = insert_person(&migrator_pool, org_id, nurture_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef1.test", "pw").await;

    let filter =
        json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [lead_id, hot_id]}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &filter)).await;
    assert!(ids.contains(&lead_person.to_string()));
    assert!(ids.contains(&hot_person.to_string()));
    assert!(!ids.contains(&nurture_person.to_string()));
}

#[sqlx::test]
#[ignore]
async fn assigned_to_clause_users_unassigned_mixed_and_me_are_viewer_relative(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F2",
        "alice@acmef2.test",
        "Alice",
        "pw",
    )
    .await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@acmef2.test", "Bob", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, bob_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let alice_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let bob_person = insert_person(&migrator_pool, org_id, stage_id, Some(bob_id)).await;
    let unassigned_person = insert_person(&migrator_pool, org_id, stage_id, None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmef2.test", "pw").await;
    let bob_cookie = crate::common::login_cookie(&router, "bob@acmef2.test", "pw").await;

    // Users only.
    let users_only = json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": [{"user_id": bob_id}]}]});
    let ids = people_ids(
        &router,
        &alice_cookie,
        &filter_uri("/api/people", &users_only),
    )
    .await;
    assert!(ids.contains(&bob_person.to_string()));
    assert!(!ids.contains(&alice_person.to_string()));
    assert!(!ids.contains(&unassigned_person.to_string()));

    // Unassigned only.
    let unassigned_only =
        json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": ["unassigned"]}]});
    let ids = people_ids(
        &router,
        &alice_cookie,
        &filter_uri("/api/people", &unassigned_only),
    )
    .await;
    assert!(ids.contains(&unassigned_person.to_string()));
    assert!(!ids.contains(&alice_person.to_string()));
    assert!(!ids.contains(&bob_person.to_string()));

    // Mixed users + unassigned.
    let mixed = json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": [{"user_id": bob_id}, "unassigned"]}]});
    let ids = people_ids(&router, &alice_cookie, &filter_uri("/api/people", &mixed)).await;
    assert!(ids.contains(&bob_person.to_string()));
    assert!(ids.contains(&unassigned_person.to_string()));
    assert!(!ids.contains(&alice_person.to_string()));

    // `me` — same URL, two members, different rows (the viewer-relative pin).
    let me_filter =
        json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": ["me"]}]});
    let uri = filter_uri("/api/people", &me_filter);
    let alice_ids = people_ids(&router, &alice_cookie, &uri).await;
    assert!(alice_ids.contains(&alice_person.to_string()));
    assert!(!alice_ids.contains(&bob_person.to_string()));
    let bob_ids = people_ids(&router, &bob_cookie, &uri).await;
    assert!(bob_ids.contains(&bob_person.to_string()));
    assert!(!bob_ids.contains(&alice_person.to_string()));
}

#[sqlx::test]
#[ignore]
async fn source_clause_matches_the_latest_inquiry_with_a_tie_break_and_never_a_zero_inquiry_person(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F3",
        "alice@acmef3.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    // Older zillow + newer website -> matches website, NOT zillow.
    let switched_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &migrator_pool,
        org_id,
        switched_person,
        "zillow",
        days_ago(30),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        org_id,
        switched_person,
        "website",
        days_ago(1),
    )
    .await;

    // Zero-inquiry person never matches ANY source clause.
    let zero_inquiry_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    // Tie-break: equal received_at -> higher id wins.
    let tie_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let tied_at = days_ago(5);
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let (lower_id, higher_id) = if id_a < id_b {
        (id_a, id_b)
    } else {
        (id_b, id_a)
    };
    insert_inquiry_with_id(
        &migrator_pool,
        org_id,
        tie_person,
        lower_id,
        "zillow",
        tied_at,
    )
    .await;
    insert_inquiry_with_id(
        &migrator_pool,
        org_id,
        tie_person,
        higher_id,
        "referral",
        tied_at,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef3.test", "pw").await;

    let website_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["website"]}]});
    let ids = people_ids(
        &router,
        &cookie,
        &filter_uri("/api/people", &website_filter),
    )
    .await;
    assert!(ids.contains(&switched_person.to_string()));
    assert!(!ids.contains(&zero_inquiry_person.to_string()));

    let zillow_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["zillow"]}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &zillow_filter)).await;
    assert!(
        !ids.contains(&switched_person.to_string()),
        "older zillow inquiry must not match — latest source is website"
    );
    assert!(!ids.contains(&zero_inquiry_person.to_string()));

    let referral_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["referral"]}]});
    let ids = people_ids(
        &router,
        &cookie,
        &filter_uri("/api/people", &referral_filter),
    )
    .await;
    assert!(
        ids.contains(&tie_person.to_string()),
        "equal received_at -> higher id (referral, inserted as higher_id) wins"
    );

    let tie_zillow_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["zillow"]}]});
    let ids = people_ids(
        &router,
        &cookie,
        &filter_uri("/api/people", &tie_zillow_filter),
    )
    .await;
    assert!(
        !ids.contains(&tie_person.to_string()),
        "the lower-id tied row (zillow) must lose the tie-break"
    );

    // A stale-but-well-formed source is VALID and simply matches nobody.
    let stale_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["a_source_nobody_used"]}]});
    let resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &stale_filter), &cookie)
            .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &stale_filter)).await;
    assert!(ids.is_empty());
}

#[sqlx::test]
#[ignore]
async fn created_age_axis_within_not_within_are_exact_complements(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F4",
        "alice@acmef4.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    // `person.created_at` cannot be backdated through this fixture helper
    // (it defaults to now()), so this pins the boundary the other
    // direction: freshly created rows ARE within_days:1 and are NOT
    // not_within_days:1.
    let fresh = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef4.test", "pw").await;

    let within = json!({"version": 1, "clauses": [{"kind": "created", "age": {"op": "within_days", "days": 1}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &within)).await;
    assert!(ids.contains(&fresh.to_string()));

    let not_within = json!({"version": 1, "clauses": [{"kind": "created", "age": {"op": "not_within_days", "days": 1}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &not_within)).await;
    assert!(
        !ids.contains(&fresh.to_string()),
        "exact complement of within_days"
    );
}

#[sqlx::test]
#[ignore]
async fn last_inquiry_age_axis_within_not_within_and_never(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F5",
        "alice@acmef5.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let recent = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, recent, "zillow", hours_ago(1)).await;

    let stale = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiry(&migrator_pool, org_id, stale, "zillow", days_ago(30)).await;

    let never = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    // Zero inquiries -> ts is NULL -> matches `never`.

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef5.test", "pw").await;

    let within = json!({"version": 1, "clauses": [{"kind": "last_inquiry", "age": {"op": "within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &within)).await;
    assert!(ids.contains(&recent.to_string()));
    assert!(!ids.contains(&stale.to_string()));
    assert!(
        !ids.contains(&never.to_string()),
        "within_days excludes never (COALESCE '-infinity')"
    );

    let not_within = json!({"version": 1, "clauses": [{"kind": "last_inquiry", "age": {"op": "not_within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &not_within)).await;
    assert!(!ids.contains(&recent.to_string()));
    assert!(ids.contains(&stale.to_string()));
    assert!(
        ids.contains(&never.to_string()),
        "not_within_days is the exact complement — includes never"
    );

    let never_filter =
        json!({"version": 1, "clauses": [{"kind": "last_inquiry", "age": {"op": "never"}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &never_filter)).await;
    assert!(ids.contains(&never.to_string()));
    assert!(!ids.contains(&recent.to_string()));
    assert!(!ids.contains(&stale.to_string()));
}

#[sqlx::test]
#[ignore]
async fn last_contact_age_axis_and_a_correction_fixture_prove_plain_max_equals_effective_attempt(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F6",
        "alice@acmef6.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    // Plain axis: recent vs stale vs never-contacted.
    let recent = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_contact_attempt(&migrator_pool, org_id, recent, hours_ago(1)).await;
    let stale = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_contact_attempt(&migrator_pool, org_id, stale, days_ago(30)).await;
    let never = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    // Correction fixture: original + a correction that inherits the SAME
    // occurred_at (docs/specs/SLICE_011a.md §4c) — the age filter's plain
    // MAX must still resolve to that shared occurred_at, exactly as if
    // only one row existed.
    let corrected = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let original_at = days_ago(30);
    let original_id = insert_contact_attempt(&migrator_pool, org_id, corrected, original_at).await;
    insert_contact_attempt_correction(
        &migrator_pool,
        org_id,
        corrected,
        alice_id,
        original_id,
        original_at,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef6.test", "pw").await;

    let within = json!({"version": 1, "clauses": [{"kind": "last_contact", "age": {"op": "within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &within)).await;
    assert!(ids.contains(&recent.to_string()));
    assert!(!ids.contains(&stale.to_string()));
    assert!(!ids.contains(&never.to_string()));
    assert!(!ids.contains(&corrected.to_string()));

    let not_within = json!({"version": 1, "clauses": [{"kind": "last_contact", "age": {"op": "not_within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &not_within)).await;
    assert!(!ids.contains(&recent.to_string()));
    assert!(ids.contains(&stale.to_string()));
    assert!(
        ids.contains(&never.to_string()),
        "the exact complement includes never-contacted"
    );
    assert!(
        ids.contains(&corrected.to_string()),
        "the correction row must not shift the computed max away from the shared occurred_at"
    );

    let never_filter =
        json!({"version": 1, "clauses": [{"kind": "last_contact", "age": {"op": "never"}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &never_filter)).await;
    assert!(ids.contains(&never.to_string()));
    assert!(
        !ids.contains(&corrected.to_string()),
        "corrected has two contact_attempted rows, not none"
    );
}

#[sqlx::test]
#[ignore]
async fn last_inbound_age_axis_and_a_backdated_capture_filters_by_its_own_inner_date(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F7",
        "alice@acmef7.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let recent = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &migrator_pool,
        org_id,
        recent,
        alice_id,
        "inbound",
        hours_ago(1),
        false,
    )
    .await;

    let never = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    // No inbound correspondence at all.

    // A backdated capture: the retroactive forward's occurred_at is the
    // email's own (old) date, D-042's whole point — the filter reads that
    // inner date, not the capture time.
    let backdated_person = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &migrator_pool,
        org_id,
        backdated_person,
        alice_id,
        "inbound",
        days_ago(60),
        true,
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef7.test", "pw").await;

    let within = json!({"version": 1, "clauses": [{"kind": "last_inbound", "age": {"op": "within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &within)).await;
    assert!(ids.contains(&recent.to_string()));
    assert!(
        !ids.contains(&backdated_person.to_string()),
        "backdated: old inner date, not capture time"
    );
    assert!(!ids.contains(&never.to_string()));

    let not_within = json!({"version": 1, "clauses": [{"kind": "last_inbound", "age": {"op": "not_within_days", "days": 7}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &not_within)).await;
    assert!(ids.contains(&backdated_person.to_string()));
    assert!(ids.contains(&never.to_string()));
    assert!(!ids.contains(&recent.to_string()));

    let never_filter =
        json!({"version": 1, "clauses": [{"kind": "last_inbound", "age": {"op": "never"}}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &never_filter)).await;
    assert!(ids.contains(&never.to_string()));
    assert!(!ids.contains(&backdated_person.to_string()));
}

#[sqlx::test]
#[ignore]
async fn has_replied_matches_a_person_whose_reply_was_already_answered(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F8",
        "alice@acmef8.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    // Replied AND answered by a later attempt: still `has_replied: true`
    // (deliberately the simple existence predicate, NOT the viewer-relative
    // "unanswered" axis — 011d's ClientRepliedUnanswered — the
    // 011a-vs-011d distinction pin).
    let answered = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &migrator_pool,
        org_id,
        answered,
        alice_id,
        "inbound",
        days_ago(10),
        false,
    )
    .await;
    insert_contact_attempt(&migrator_pool, org_id, answered, days_ago(1)).await;

    let never_replied = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef8.test", "pw").await;

    let has_replied_true =
        json!({"version": 1, "clauses": [{"kind": "has_replied", "value": true}]});
    let ids = people_ids(
        &router,
        &cookie,
        &filter_uri("/api/people", &has_replied_true),
    )
    .await;
    assert!(
        ids.contains(&answered.to_string()),
        "has_replied:true matches even a long-since-answered reply"
    );
    assert!(!ids.contains(&never_replied.to_string()));

    let has_replied_false =
        json!({"version": 1, "clauses": [{"kind": "has_replied", "value": false}]});
    let ids = people_ids(
        &router,
        &cookie,
        &filter_uri("/api/people", &has_replied_false),
    )
    .await;
    assert!(ids.contains(&never_replied.to_string()));
    assert!(!ids.contains(&answered.to_string()));
}

#[sqlx::test]
#[ignore]
async fn has_phone_and_has_email_axes(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F9",
        "alice@acmef9.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let both = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_contact_method(&migrator_pool, org_id, both, "phone", "+15555550100").await;
    insert_contact_method(&migrator_pool, org_id, both, "email", "both@example.com").await;

    let phone_only = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_contact_method(&migrator_pool, org_id, phone_only, "phone", "+15555550101").await;

    let neither = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef9.test", "pw").await;

    let has_phone = json!({"version": 1, "clauses": [{"kind": "has_phone", "value": true}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &has_phone)).await;
    assert!(ids.contains(&both.to_string()));
    assert!(ids.contains(&phone_only.to_string()));
    assert!(!ids.contains(&neither.to_string()));

    let no_email = json!({"version": 1, "clauses": [{"kind": "has_email", "value": false}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &no_email)).await;
    assert!(ids.contains(&phone_only.to_string()));
    assert!(ids.contains(&neither.to_string()));
    assert!(!ids.contains(&both.to_string()));
}

// --- 5. AND composition + empty clauses ---------------------------------

#[sqlx::test]
#[ignore]
async fn and_composition_intersects_two_clauses(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F10",
        "alice@acmef10.test",
        "Alice",
        "pw",
    )
    .await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@acmef10.test", "Bob", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, bob_id).await;
    let stages: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position")
            .bind(org_id)
            .fetch_all(&migrator_pool)
            .await
            .unwrap();
    let lead_id = stages[0].0;
    let hot_id = stages[1].0;

    let matches_both = insert_person(&migrator_pool, org_id, lead_id, Some(alice_id)).await;
    let wrong_stage = insert_person(&migrator_pool, org_id, hot_id, Some(alice_id)).await;
    let wrong_assignee = insert_person(&migrator_pool, org_id, lead_id, Some(bob_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef10.test", "pw").await;

    let filter = json!({
        "version": 1,
        "clauses": [
            {"kind": "stage", "stage_ids": [lead_id]},
            {"kind": "assigned_to", "assignees": [{"user_id": alice_id}]},
        ]
    });
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &filter)).await;
    assert!(ids.contains(&matches_both.to_string()));
    assert!(!ids.contains(&wrong_stage.to_string()));
    assert!(!ids.contains(&wrong_assignee.to_string()));
}

#[sqlx::test]
#[ignore]
async fn empty_clauses_is_the_full_list_through_the_filtered_path(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F11",
        "alice@acmef11.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef11.test", "pw").await;

    let empty = json!({"version": 1, "clauses": []});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &empty)).await;
    assert!(ids.contains(&person_id.to_string()));
}

// --- 6. Absent-param regression ------------------------------------------
// (existing db_people.rs pins already cover this; untouched by this slice —
// list_summaries is byte-identical, and its own regression tests remain
// green, verified by the full ./scripts/check-db run.)

// --- 7. Cap + ordering under a filter -------------------------------------

#[sqlx::test]
#[ignore]
async fn cap_truncates_to_500_with_created_at_desc_id_asc_ordering_under_a_filter(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F12",
        "alice@acmef12.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    insert_bare_people_batch(&migrator_pool, org_id, stage_id, 501).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef12.test", "pw").await;

    let filter = json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [stage_id]}]});
    let body = crate::common::body_json(
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &filter), &cookie).await,
    )
    .await;
    assert_eq!(body["truncated"], true);
    let people = body["people"].as_array().unwrap();
    assert_eq!(people.len(), 500);

    let created_ats: Vec<String> = people
        .iter()
        .map(|p| p["created_at"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = created_ats.clone();
    sorted.sort();
    sorted.reverse();
    assert_eq!(created_ats, sorted, "created_at DESC holds under a filter");
}

/// M4 (adversarial-review follow-up): only `created_at DESC` was pinned
/// above — this pins the `id ASC` TIE-BREAK specifically, via a fixture
/// `UPDATE` giving two rows the exact same `created_at` (the ordinary
/// insert path can't produce a tie; `created_at` defaults to `now()`).
#[sqlx::test]
#[ignore]
async fn ordering_ties_break_by_id_asc_under_a_filter(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F12b",
        "alice@acmef12b.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let a = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let b = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    let (lower_id, higher_id) = if a < b { (a, b) } else { (b, a) };

    let tied_at = Utc::now();
    sqlx::query("UPDATE person SET created_at = $1 WHERE id = ANY($2)")
        .bind(tied_at)
        .bind(vec![a, b])
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef12b.test", "pw").await;

    let filter = json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [stage_id]}]});
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &filter)).await;
    let lower_pos = ids
        .iter()
        .position(|id| id == &lower_id.to_string())
        .unwrap();
    let higher_pos = ids
        .iter()
        .position(|id| id == &higher_id.to_string())
        .unwrap();
    assert!(
        lower_pos < higher_pos,
        "same created_at -> id ASC: {lower_id} must sort before {higher_id}"
    );
}

// --- 8. Error contract -----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn unauthenticated_request_is_401_even_with_a_garbage_filter(migrator_pool: PgPool) {
    crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F13",
        "alice@acmef13.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/people?filter=not%20even%20json%7B")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
#[ignore]
async fn undecodable_json_filter_is_400_in_the_envelope_shape(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F14",
        "alice@acmef14.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef14.test", "pw").await;

    let response =
        crate::common::get_with_cookie(&router, "/api/people?filter=%7Bnot+valid+json", &cookie)
            .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(response).await["error"],
        "malformed_request"
    );
}

#[sqlx::test]
#[ignore]
async fn unknown_extra_query_param_is_ignored_exactly_as_today(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F15",
        "alice@acmef15.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef15.test", "pw").await;

    let response = crate::common::get_with_cookie(&router, "/api/people?foo=bar", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn present_but_empty_filter_param_is_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F16",
        "alice@acmef16.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef16.test", "pw").await;

    let with_equals = crate::common::get_with_cookie(&router, "/api/people?filter=", &cookie).await;
    assert_eq!(with_equals.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(with_equals).await["error"],
        "malformed_request"
    );

    let bare = crate::common::get_with_cookie(&router, "/api/people?filter", &cookie).await;
    assert_eq!(bare.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn cross_org_and_nonexistent_stage_ids_produce_byte_identical_422s(migrator_pool: PgPool) {
    let (_org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F17a",
        "alice@acmef17.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, _bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F17b",
        "bob@acmef17.test",
        "Bob",
        "pw",
    )
    .await;
    let _ = alice_id;
    let (org_b_stage,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef17.test", "pw").await;

    let cross_org =
        json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [org_b_stage]}]});
    let cross_resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &cross_org), &cookie)
            .await;
    assert_eq!(cross_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let cross_body = crate::common::body_json(cross_resp).await;
    assert_eq!(cross_body["error"], "invalid_stage");

    let nonexistent =
        json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [Uuid::new_v4()]}]});
    let non_resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &nonexistent), &cookie)
            .await;
    assert_eq!(non_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let non_body = crate::common::body_json(non_resp).await;
    assert_eq!(
        non_body, cross_body,
        "byte-identical bodies, no existence leak"
    );
}

#[sqlx::test]
#[ignore]
async fn cross_org_and_nonexistent_assignee_ids_produce_byte_identical_422s(migrator_pool: PgPool) {
    let (_org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F18a",
        "alice@acmef18.test",
        "Alice",
        "pw",
    )
    .await;
    let (_org_b, bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F18b",
        "bob@acmef18.test",
        "Bob",
        "pw",
    )
    .await;
    let _ = alice_id;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef18.test", "pw").await;

    let cross_org = json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": [{"user_id": bob_id}]}]});
    let cross_resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &cross_org), &cookie)
            .await;
    assert_eq!(cross_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let cross_body = crate::common::body_json(cross_resp).await;
    assert_eq!(cross_body["error"], "invalid_assignee");

    let nonexistent = json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": [{"user_id": Uuid::new_v4()}]}]});
    let non_resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &nonexistent), &cookie)
            .await;
    assert_eq!(non_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let non_body = crate::common::body_json(non_resp).await;
    assert_eq!(
        non_body, cross_body,
        "byte-identical bodies, no existence leak"
    );
}

#[sqlx::test]
#[ignore]
async fn deactivated_member_is_a_valid_assignee_d027(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F19",
        "alice@acmef19.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol@acmef19.test", "Carol", "pw").await;
    crate::common::add_membership_with(
        &migrator_pool,
        org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let carol_person = insert_person(&migrator_pool, org_id, stage_id, Some(carol_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef19.test", "pw").await;

    let filter = json!({"version": 1, "clauses": [{"kind": "assigned_to", "assignees": [{"user_id": carol_id}]}]});
    let response =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &filter), &cookie).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a deactivated member is still a valid assigned_to value"
    );
    let ids = people_ids(&router, &cookie, &filter_uri("/api/people", &filter)).await;
    assert!(ids.contains(&carol_person.to_string()));
}

#[sqlx::test]
#[ignore]
async fn first_failure_wins_by_clause_order(migrator_pool: PgPool) {
    let (_org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F20a",
        "alice@acmef20.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F20b",
        "bob@acmef20.test",
        "Bob",
        "pw",
    )
    .await;
    let _ = alice_id;
    let (org_b_stage,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef20.test", "pw").await;

    // assigned_to (bad) BEFORE stage (bad) -> invalid_assignee wins.
    let assignee_first = json!({
        "version": 1,
        "clauses": [
            {"kind": "assigned_to", "assignees": [{"user_id": bob_id}]},
            {"kind": "stage", "stage_ids": [org_b_stage]},
        ]
    });
    let resp = crate::common::get_with_cookie(
        &router,
        &filter_uri("/api/people", &assignee_first),
        &cookie,
    )
    .await;
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "invalid_assignee"
    );

    // stage (bad) BEFORE assigned_to (bad) -> invalid_stage wins.
    let stage_first = json!({
        "version": 1,
        "clauses": [
            {"kind": "stage", "stage_ids": [org_b_stage]},
            {"kind": "assigned_to", "assignees": [{"user_id": bob_id}]},
        ]
    });
    let resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &stage_first), &cookie)
            .await;
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "invalid_stage"
    );
}

/// M5 (adversarial-review follow-up): structural failures are validated
/// entirely BEFORE any org-scoped check runs (§4b), so a filter carrying
/// BOTH a structural violation (a >50-value array) AND an org-scoped-
/// invalid id (an org-B stage) must still 400, never 422 — the ordering
/// pin above only ever varied two 422-triggering clauses against each
/// other; this one crosses the structural/org-scoped class boundary.
#[sqlx::test]
#[ignore]
async fn structural_violation_beats_a_422_triggering_clause_in_the_same_filter(
    migrator_pool: PgPool,
) {
    let (_org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F5m",
        "alice@acmef5m.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, _bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F5n",
        "bob@acmef5m.test",
        "Bob",
        "pw",
    )
    .await;
    let _ = alice_id;
    let (org_b_stage,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef5m.test", "pw").await;

    let too_many_sources: Vec<String> = (0..51).map(|i| format!("source{i:03}")).collect();
    let filter = json!({
        "version": 1,
        "clauses": [
            {"kind": "source", "sources": too_many_sources},
            {"kind": "stage", "stage_ids": [org_b_stage]},
        ]
    });
    let resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &filter), &cookie).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "malformed_request"
    );
}

/// M8 (adversarial-review follow-up): the reject-side ceilings (>20
/// clauses, >50 values) were pinned, but never the accept side. Exactly
/// 20 clauses is unreachable as a genuinely valid filter (only 10 distinct
/// kinds exist — the 20 cap is a formal backstop above the one-per-kind
/// rule, §4b), so this smokes the two ceilings that ARE reachable: every
/// real kind present at once, and one value array at its exact 50-element
/// cap — both must 200.
#[sqlx::test]
#[ignore]
async fn all_ten_clause_kinds_at_once_and_a_fifty_value_array_are_both_accepted(
    migrator_pool: PgPool,
) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F8m",
        "alice@acmef8m.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef8m.test", "pw").await;

    let fifty_sources: Vec<String> = (0..50).map(|i| format!("source{i:03}")).collect();
    let filter = json!({
        "version": 1,
        "clauses": [
            {"kind": "stage", "stage_ids": [stage_id]},
            {"kind": "assigned_to", "assignees": ["me"]},
            {"kind": "source", "sources": fifty_sources},
            {"kind": "created", "age": {"op": "within_days", "days": 3650}},
            {"kind": "last_inquiry", "age": {"op": "never"}},
            {"kind": "last_contact", "age": {"op": "never"}},
            {"kind": "last_inbound", "age": {"op": "never"}},
            {"kind": "has_replied", "value": false},
            {"kind": "has_phone", "value": false},
            {"kind": "has_email", "value": false},
        ]
    });
    let resp =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &filter), &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- M9 (adversarial-review follow-up): more query-string edges --------

#[sqlx::test]
#[ignore]
async fn repeated_filter_param_is_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F9m",
        "alice@acmef9m.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef9m.test", "pw").await;

    let filter = json!({"version": 1, "clauses": []});
    let encoded = percent_encode(&filter.to_string());
    let uri = format!("/api/people?filter={encoded}&filter={encoded}");
    let resp = crate::common::get_with_cookie(&router, &uri, &cookie).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "malformed_request"
    );
}

#[sqlx::test]
#[ignore]
async fn invalid_percent_encoding_in_filter_param_is_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F9n",
        "alice@acmef9n.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef9n.test", "pw").await;

    let resp = crate::common::get_with_cookie(&router, "/api/people?filter=%FF", &cookie).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "malformed_request"
    );
}

#[sqlx::test]
#[ignore]
async fn invalid_percent_encoding_in_an_unrelated_param_is_ignored(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F9o",
        "alice@acmef9o.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef9o.test", "pw").await;

    let resp = crate::common::get_with_cookie(&router, "/api/people?foo=%FF", &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- 9. Tenant isolation ---------------------------------------------------

#[sqlx::test]
#[ignore]
async fn a_filter_never_returns_another_organizations_rows(migrator_pool: PgPool) {
    let (org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F21a",
        "alice@acmef21.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F21b",
        "bob@acmef21.test",
        "Bob",
        "pw",
    )
    .await;
    let stage_a = first_stage_id(&migrator_pool, org_a).await;
    let stage_b = first_stage_id(&migrator_pool, org_b).await;
    let _person_a = insert_person(&migrator_pool, org_a, stage_a, Some(alice_id)).await;
    let person_b = insert_person(&migrator_pool, org_b, stage_b, Some(bob_id)).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmef21.test", "pw").await;

    // An empty filter (matches everyone in scope) as alice must never see
    // org B's Person.
    let empty = json!({"version": 1, "clauses": []});
    let ids = people_ids(&router, &alice_cookie, &filter_uri("/api/people", &empty)).await;
    assert!(!ids.contains(&person_b.to_string()));
}

// --- 10. GET /api/inquiry-sources ------------------------------------------

#[sqlx::test]
#[ignore]
async fn inquiry_sources_endpoint_is_distinct_ascending_org_scoped_and_member_allowed(
    migrator_pool: PgPool,
) {
    let (org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F22a",
        "alice@acmef22.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best F22b",
        "bob@acmef22.test",
        "Bob",
        "pw",
    )
    .await;
    let stage_a = first_stage_id(&migrator_pool, org_a).await;
    let stage_b = first_stage_id(&migrator_pool, org_b).await;
    let person_a = insert_person(&migrator_pool, org_a, stage_a, Some(alice_id)).await;
    let person_b = insert_person(&migrator_pool, org_b, stage_b, Some(bob_id)).await;

    insert_inquiry(&migrator_pool, org_a, person_a, "zillow", days_ago(1)).await;
    insert_inquiry(&migrator_pool, org_a, person_a, "zillow", days_ago(2)).await; // duplicate source
    insert_inquiry(&migrator_pool, org_a, person_a, "website", days_ago(3)).await;
    insert_inquiry(
        &migrator_pool,
        org_b,
        person_b,
        "org_b_only_source",
        days_ago(1),
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice@acmef22.test", "pw").await;

    let body = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/inquiry-sources", &alice_cookie).await,
    )
    .await;
    let sources: Vec<String> = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        sources,
        vec!["website".to_string(), "zillow".to_string()],
        "distinct + ascending"
    );
    assert!(
        !sources.contains(&"org_b_only_source".to_string()),
        "org-scoped"
    );
    assert_eq!(body["truncated"], false);
}

#[sqlx::test]
#[ignore]
async fn inquiry_sources_endpoint_reports_truncated_past_500(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F23",
        "alice@acmef23.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = insert_person(&migrator_pool, org_id, stage_id, Some(alice_id)).await;
    insert_inquiries_batch(&migrator_pool, org_id, person_id, days_ago(1), 501).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef23.test", "pw").await;
    let body = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/inquiry-sources", &cookie).await,
    )
    .await;
    assert_eq!(body["truncated"], true);
    assert_eq!(body["sources"].as_array().unwrap().len(), 500);
}

#[sqlx::test]
#[ignore]
async fn inquiry_sources_endpoint_requires_auth(migrator_pool: PgPool) {
    crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F24",
        "alice@acmef24.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/inquiry-sources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- F1: the request span records PATH ONLY, query stripped ----------------

#[sqlx::test]
#[ignore]
async fn people_filter_request_span_records_path_only_with_the_query_string_stripped(
    migrator_pool: PgPool,
) {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme F25",
        "alice@acmef25.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@acmef25.test", "pw").await;

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
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let _guard = tracing::subscriber::set_default(subscriber);

    let filter = json!({"version": 1, "clauses": [{"kind": "source", "sources": ["zillow"]}]});
    let _ =
        crate::common::get_with_cookie(&router, &filter_uri("/api/people", &filter), &cookie).await;

    drop(_guard);
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("/api/people"),
        "the path must still be recorded: {captured}"
    );
    assert!(
        !captured.contains("filter="),
        "the query string (carrying the filter JSON) must be stripped from every span: {captured}"
    );
    assert!(
        !captured.contains("zillow"),
        "no filter value ever reaches a span: {captured}"
    );
}

// --- 011d correction (b): per-axis parity across People, count, every ------
// --- sort, for the three derived clause kinds (spec §9.1) ------------------

mod derived_axis_full_matrix_parity {
    use std::collections::HashSet;

    use chrono::{Duration as ChronoDuration, Utc};
    use crm_api::domain::admin::{MembershipStatus, Role};
    use crm_api::domain::person::filter::{BoolClause, Clause, FilterDefinition};
    use crm_api::domain::person::sort::{PersonSort, SortDirection, SortKey};
    use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
    use crm_api::ids::{OrganizationId, UserId};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{insert_contact_attempt, insert_correspondence, insert_inquiry, insert_person};

    #[allow(clippy::too_many_arguments)]
    async fn insert_call(
        pool: &PgPool,
        organization_id: Uuid,
        person_id: Uuid,
        caller_user_id: Uuid,
        ended_at: chrono::DateTime<Utc>,
        corrected: bool,
    ) -> Uuid {
        let call_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO call \
                (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
                 correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
             VALUES \
                ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
                 'scripted', 'axis-matrix-fixture', $7, $7)",
        )
        .bind(call_id)
        .bind(organization_id)
        .bind(person_id)
        .bind(Uuid::new_v4())
        .bind(caller_user_id)
        .bind(Uuid::new_v4())
        .bind(ended_at)
        .execute(pool)
        .await
        .unwrap();

        let root_id: Uuid = sqlx::query_scalar(
            "INSERT INTO contact_attempted \
                (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
                 causation_id, person_id, channel, outcome) \
             VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, 'call', 'reached') \
             RETURNING id",
        )
        .bind(organization_id)
        .bind(ended_at)
        .bind(Uuid::new_v4())
        .bind(call_id)
        .bind(person_id)
        .fetch_one(pool)
        .await
        .unwrap();

        if corrected {
            sqlx::query(
                "INSERT INTO contact_attempted \
                    (organization_id, actor_kind, actor_user_id, origin, occurred_at, \
                     correlation_id, corrects_id, person_id, channel, outcome) \
                 VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'left_message')",
            )
            .bind(organization_id)
            .bind(caller_user_id)
            .bind(ended_at)
            .bind(Uuid::new_v4())
            .bind(root_id)
            .bind(person_id)
            .execute(pool)
            .await
            .unwrap();
        }

        call_id
    }

    /// Every People-shaped read path a clause can be evaluated through
    /// (spec §9.1): `filtered_summaries` (People), `count_filtered_matches`
    /// (count), and all seven sorted statements — set-equality only (sort
    /// ORDER correctness is pinned elsewhere; this proves membership is
    /// identical across all eleven statements' shared predicate matrix).
    async fn assert_axis_parity(
        pool: &PgPool,
        organization_id: Uuid,
        viewer_id: Uuid,
        filter: &FilterDefinition,
        expected: &HashSet<Uuid>,
        context: &str,
    ) {
        let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
        let params = filter.to_query_params(UserId::new(viewer_id));
        let mut conn = pool.acquire().await.unwrap();

        let (summaries, truncated) = person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
        assert!(
            !truncated,
            "{context}: People fixture is well below the cap"
        );
        let people_ids: HashSet<Uuid> = summaries.into_iter().map(|p| p.id.as_uuid()).collect();
        assert_eq!(
            &people_ids, expected,
            "{context}: People (filtered_summaries)"
        );

        let (count, count_truncated) =
            person_queries::count_filtered_matches(&mut conn, &scope, &params)
                .await
                .unwrap();
        assert!(
            !count_truncated,
            "{context}: count fixture is well below the cap"
        );
        assert_eq!(
            count as usize,
            expected.len(),
            "{context}: count_filtered_matches"
        );

        for (key, direction) in [
            (SortKey::Created, SortDirection::Asc),
            (SortKey::Name, SortDirection::Asc),
            (SortKey::Name, SortDirection::Desc),
            (SortKey::Stage, SortDirection::Asc),
            (SortKey::Stage, SortDirection::Desc),
            (SortKey::Assignee, SortDirection::Asc),
            (SortKey::Assignee, SortDirection::Desc),
        ] {
            let sort = PersonSort { key, direction };
            let (sorted, sorted_truncated) =
                person_queries::filtered_summaries_sorted(&mut conn, &scope, &params, sort)
                    .await
                    .unwrap();
            assert!(
                !sorted_truncated,
                "{context}: sort {key:?}.{direction:?} fixture below cap"
            );
            let sorted_ids: HashSet<Uuid> = sorted.into_iter().map(|p| p.id.as_uuid()).collect();
            assert_eq!(
                &sorted_ids, expected,
                "{context}: sort {key:?}.{direction:?}"
            );
        }
    }

    fn bool_clause(kind: fn(BoolClause) -> Clause, value: bool) -> FilterDefinition {
        FilterDefinition {
            version: 1,
            clauses: vec![kind(BoolClause { value })],
        }
    }

    #[sqlx::test]
    #[ignore]
    async fn awaiting_response_present_true_false_and_absent(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Axis matrix awaiting_response",
            "alice@axis-matrix-awaiting.test",
            "Alice",
            "pw",
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;
        let now = Utc::now();

        // True: an inquiry after the (only) contact attempt.
        let awaiting_true =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_contact_attempt(
            &migrator_pool,
            organization_id,
            awaiting_true,
            now - ChronoDuration::hours(4),
        )
        .await;
        insert_inquiry(
            &migrator_pool,
            organization_id,
            awaiting_true,
            "zillow",
            now - ChronoDuration::hours(3),
        )
        .await;

        // False: the (only) inquiry is answered by a LATER contact attempt.
        let awaiting_false =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_inquiry(
            &migrator_pool,
            organization_id,
            awaiting_false,
            "zillow",
            now - ChronoDuration::hours(4),
        )
        .await;
        insert_contact_attempt(
            &migrator_pool,
            organization_id,
            awaiting_false,
            now - ChronoDuration::hours(3),
        )
        .await;

        // Zero-inquiry: never matches true; matches false.
        let zero_inquiry =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;

        let all_ids: HashSet<Uuid> = [awaiting_true, awaiting_false, zero_inquiry]
            .into_iter()
            .collect();

        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::AwaitingResponse, true),
            &HashSet::from([awaiting_true]),
            "awaiting_response:true",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::AwaitingResponse, false),
            &HashSet::from([awaiting_false, zero_inquiry]),
            "awaiting_response:false",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &FilterDefinition {
                version: 1,
                clauses: vec![],
            },
            &all_ids,
            "awaiting_response absent (empty filter)",
        )
        .await;
    }

    #[sqlx::test]
    #[ignore]
    async fn client_replied_unanswered_present_true_false_and_absent(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Axis matrix client_replied",
            "alice@axis-matrix-replied.test",
            "Alice",
            "pw",
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;
        let now = Utc::now();

        // True: inbound later than both the last contact attempt and the
        // latest outbound correspondence.
        let replied_true =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_correspondence(
            &migrator_pool,
            organization_id,
            replied_true,
            alice_id,
            "outbound",
            now - ChronoDuration::hours(4),
            false,
        )
        .await;
        insert_correspondence(
            &migrator_pool,
            organization_id,
            replied_true,
            alice_id,
            "inbound",
            now - ChronoDuration::hours(3),
            false,
        )
        .await;

        // False: outbound-only (no inbound at all).
        let replied_false_outbound_only =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_correspondence(
            &migrator_pool,
            organization_id,
            replied_false_outbound_only,
            alice_id,
            "outbound",
            now - ChronoDuration::hours(3),
            false,
        )
        .await;

        // False: the inbound reply was already answered by a later attempt.
        let replied_false_answered =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_correspondence(
            &migrator_pool,
            organization_id,
            replied_false_answered,
            alice_id,
            "inbound",
            now - ChronoDuration::hours(4),
            false,
        )
        .await;
        insert_contact_attempt(
            &migrator_pool,
            organization_id,
            replied_false_answered,
            now - ChronoDuration::hours(3),
        )
        .await;

        let all_ids: HashSet<Uuid> = [
            replied_true,
            replied_false_outbound_only,
            replied_false_answered,
        ]
        .into_iter()
        .collect();

        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::ClientRepliedUnanswered, true),
            &HashSet::from([replied_true]),
            "client_replied_unanswered:true",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::ClientRepliedUnanswered, false),
            &HashSet::from([replied_false_outbound_only, replied_false_answered]),
            "client_replied_unanswered:false",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &FilterDefinition {
                version: 1,
                clauses: vec![],
            },
            &all_ids,
            "client_replied_unanswered absent (empty filter)",
        )
        .await;
    }

    #[sqlx::test]
    #[ignore]
    async fn awaiting_call_outcome_present_true_false_and_absent(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Axis matrix call outcome",
            "alice@axis-matrix-call.test",
            "Alice",
            "pw",
        )
        .await;
        let bob_id =
            crate::common::create_user(&migrator_pool, "bob@axis-matrix-call.test", "Bob", "pw")
                .await;
        crate::common::add_membership_with(
            &migrator_pool,
            organization_id,
            bob_id,
            Role::Member,
            MembershipStatus::Active,
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;
        let now = Utc::now();

        // True for Alice: Alice called, ended, root attempt uncorrected.
        let call_true =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_call(
            &migrator_pool,
            organization_id,
            call_true,
            alice_id,
            now - ChronoDuration::hours(1),
            false,
        )
        .await;

        // False for Alice: Bob called (a different caller).
        let call_by_bob =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_call(
            &migrator_pool,
            organization_id,
            call_by_bob,
            bob_id,
            now - ChronoDuration::hours(1),
            false,
        )
        .await;

        // False for Alice: Alice's call outcome was corrected.
        let call_corrected =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        insert_call(
            &migrator_pool,
            organization_id,
            call_corrected,
            alice_id,
            now - ChronoDuration::hours(1),
            true,
        )
        .await;

        let all_ids: HashSet<Uuid> = [call_true, call_by_bob, call_corrected]
            .into_iter()
            .collect();

        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::AwaitingCallOutcome, true),
            &HashSet::from([call_true]),
            "awaiting_call_outcome:true (alice)",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &bool_clause(Clause::AwaitingCallOutcome, false),
            &HashSet::from([call_by_bob, call_corrected]),
            "awaiting_call_outcome:false (alice)",
        )
        .await;
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &FilterDefinition {
                version: 1,
                clauses: vec![],
            },
            &all_ids,
            "awaiting_call_outcome absent (empty filter)",
        )
        .await;

        // Viewer-relative check: the SAME true clause, evaluated as Bob's
        // own axis, matches Bob's call instead — never the wire, always
        // the bound viewer.
        assert_axis_parity(
            &migrator_pool,
            organization_id,
            bob_id,
            &bool_clause(Clause::AwaitingCallOutcome, true),
            &HashSet::from([call_by_bob]),
            "awaiting_call_outcome:true (bob)",
        )
        .await;
    }
}

// --- Slice 011e e2 §9.12/§9.13: `tags` / `not_tags` semantics and parity
// across People, count and every sort ---------------------------------

mod tags_and_not_tags {
    use std::collections::HashSet;

    use crm_api::domain::person::filter::{
        Clause, FilterDefinition, PersonFilterParams, TagIdsClause,
    };
    use crm_api::domain::person::sort::{PersonSort, SortDirection, SortKey};
    use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
    use crm_api::ids::{OrganizationId, TagId, UserId};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::insert_person;

    async fn insert_tag(
        pool: &PgPool,
        organization_id: Uuid,
        created_by: Uuid,
        name: &str,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO tag (organization_id, name, created_by_user_id) VALUES ($1, $2, $3) \
             RETURNING id",
        )
        .bind(organization_id)
        .bind(name)
        .bind(created_by)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn apply_tag(
        pool: &PgPool,
        organization_id: Uuid,
        person_id: Uuid,
        tag_id: Uuid,
        added_by: Uuid,
    ) {
        sqlx::query(
            "INSERT INTO person_tag (organization_id, person_id, tag_id, added_by_user_id) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(organization_id)
        .bind(person_id)
        .bind(tag_id)
        .bind(added_by)
        .execute(pool)
        .await
        .unwrap();
    }

    fn tags_clause(ids: &[Uuid]) -> FilterDefinition {
        FilterDefinition {
            version: 1,
            clauses: vec![Clause::Tags(TagIdsClause {
                tag_ids: ids.iter().copied().map(TagId::new).collect(),
            })],
        }
    }

    fn not_tags_clause(ids: &[Uuid]) -> FilterDefinition {
        FilterDefinition {
            version: 1,
            clauses: vec![Clause::NotTags(TagIdsClause {
                tag_ids: ids.iter().copied().map(TagId::new).collect(),
            })],
        }
    }

    fn tags_and_not_tags(any_ids: &[Uuid], none_ids: &[Uuid]) -> FilterDefinition {
        FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::Tags(TagIdsClause {
                    tag_ids: any_ids.iter().copied().map(TagId::new).collect(),
                }),
                Clause::NotTags(TagIdsClause {
                    tag_ids: none_ids.iter().copied().map(TagId::new).collect(),
                }),
            ],
        }
    }

    /// §9.13: every People-shaped statement that binds the parameter set
    /// except Today's two source statements and the three system-feed
    /// statements, which have their own dedicated parity coverage
    /// (`db_today_source_filter_parity.rs`, `db_today_feed_equivalence.rs`):
    /// `filtered_summaries`, `count_filtered_matches`, and all seven sorted
    /// statements.
    async fn assert_people_parity(
        pool: &PgPool,
        organization_id: Uuid,
        viewer_id: Uuid,
        filter: &FilterDefinition,
        expected: &HashSet<Uuid>,
        context: &str,
    ) {
        let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
        let params = filter.to_query_params(UserId::new(viewer_id));
        let mut conn = pool.acquire().await.unwrap();

        let (summaries, truncated) = person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
        assert!(
            !truncated,
            "{context}: People fixture is well below the cap"
        );
        let people_ids: HashSet<Uuid> = summaries.into_iter().map(|p| p.id.as_uuid()).collect();
        assert_eq!(
            &people_ids, expected,
            "{context}: People (filtered_summaries)"
        );

        let (count, count_truncated) =
            person_queries::count_filtered_matches(&mut conn, &scope, &params)
                .await
                .unwrap();
        assert!(
            !count_truncated,
            "{context}: count fixture is well below the cap"
        );
        assert_eq!(
            count as usize,
            expected.len(),
            "{context}: count_filtered_matches"
        );

        for (key, direction) in [
            (SortKey::Created, SortDirection::Asc),
            (SortKey::Name, SortDirection::Asc),
            (SortKey::Name, SortDirection::Desc),
            (SortKey::Stage, SortDirection::Asc),
            (SortKey::Stage, SortDirection::Desc),
            (SortKey::Assignee, SortDirection::Asc),
            (SortKey::Assignee, SortDirection::Desc),
        ] {
            let sort = PersonSort { key, direction };
            let (sorted, sorted_truncated) =
                person_queries::filtered_summaries_sorted(&mut conn, &scope, &params, sort)
                    .await
                    .unwrap();
            assert!(
                !sorted_truncated,
                "{context}: sort {key:?}.{direction:?} fixture below cap"
            );
            let sorted_ids: HashSet<Uuid> = sorted.into_iter().map(|p| p.id.as_uuid()).collect();
            assert_eq!(
                &sorted_ids, expected,
                "{context}: sort {key:?}.{direction:?}"
            );
        }
    }

    #[sqlx::test]
    #[ignore]
    async fn tags_any_of_one_and_several_ids_positive_and_negative(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Tags any-of",
            "alice@tags-any-of.test",
            "Alice",
            "pw",
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;

        let vip = insert_tag(&migrator_pool, organization_id, alice_id, "VIP").await;
        let past_client =
            insert_tag(&migrator_pool, organization_id, alice_id, "Past client").await;
        let spam = insert_tag(&migrator_pool, organization_id, alice_id, "Spam").await;

        let has_vip =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(&migrator_pool, organization_id, has_vip, vip, alice_id).await;

        let has_past_client =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(
            &migrator_pool,
            organization_id,
            has_past_client,
            past_client,
            alice_id,
        )
        .await;

        let has_spam_only =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(
            &migrator_pool,
            organization_id,
            has_spam_only,
            spam,
            alice_id,
        )
        .await;

        let _untagged =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;

        // One id: only the Person carrying it.
        assert_people_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &tags_clause(&[vip]),
            &HashSet::from([has_vip]),
            "tags:[VIP]",
        )
        .await;

        // Several ids: OR — VIP or Past client, never Spam or untagged.
        assert_people_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &tags_clause(&[vip, past_client]),
            &HashSet::from([has_vip, has_past_client]),
            "tags:[VIP, Past client]",
        )
        .await;
    }

    #[sqlx::test]
    #[ignore]
    async fn not_tags_none_of_matches_an_untagged_person(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Not tags none-of",
            "alice@not-tags-none-of.test",
            "Alice",
            "pw",
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;

        let spam = insert_tag(&migrator_pool, organization_id, alice_id, "Spam").await;

        let has_spam =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(&migrator_pool, organization_id, has_spam, spam, alice_id).await;

        let untagged =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;

        assert_people_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &not_tags_clause(&[spam]),
            &HashSet::from([untagged]),
            "not_tags:[Spam]",
        )
        .await;
    }

    #[sqlx::test]
    #[ignore]
    async fn tags_and_not_tags_together_intersect(migrator_pool: PgPool) {
        let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Tags and not_tags intersect",
            "alice@tags-intersect.test",
            "Alice",
            "pw",
        )
        .await;
        let stage_id = super::first_stage_id(&migrator_pool, organization_id).await;

        let vip = insert_tag(&migrator_pool, organization_id, alice_id, "VIP").await;
        let spam = insert_tag(&migrator_pool, organization_id, alice_id, "Spam").await;

        // Both VIP and Spam: excluded by not_tags despite matching tags.
        let both = insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(&migrator_pool, organization_id, both, vip, alice_id).await;
        apply_tag(&migrator_pool, organization_id, both, spam, alice_id).await;

        // VIP only: admitted.
        let vip_only =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(&migrator_pool, organization_id, vip_only, vip, alice_id).await;

        // Spam only: excluded (fails tags:[VIP]).
        let spam_only =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;
        apply_tag(&migrator_pool, organization_id, spam_only, spam, alice_id).await;

        // Untagged: excluded (fails tags:[VIP]).
        let _untagged =
            insert_person(&migrator_pool, organization_id, stage_id, Some(alice_id)).await;

        assert_people_parity(
            &migrator_pool,
            organization_id,
            alice_id,
            &tags_and_not_tags(&[vip], &[spam]),
            &HashSet::from([vip_only]),
            "tags:[VIP], not_tags:[Spam]",
        )
        .await;
    }

    /// AGENTS.md §4.3: persistence tests must attempt cross-Organization
    /// access. `to_query_params`/`filtered_summaries` never re-validate
    /// reference existence (that is `validate_references`'s job, already
    /// run before `to_query_params` on every real caller path) — this
    /// binds another Organization's tag id directly to prove the SQL
    /// join itself (`pt.organization_id = p.organization_id` plus the
    /// literal `p.organization_id = $1`) is what keeps org A's result
    /// empty, not merely that no caller happens to construct this filter.
    #[sqlx::test]
    #[ignore]
    async fn identically_named_tag_in_another_organization_never_matches(migrator_pool: PgPool) {
        let (org_a, alice_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Tag cross-org A",
            "alice@tag-cross-org-a.test",
            "Alice",
            "pw",
        )
        .await;
        let (org_b, bob_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Tag cross-org B",
            "bob@tag-cross-org-b.test",
            "Bob",
            "pw",
        )
        .await;
        let stage_a = super::first_stage_id(&migrator_pool, org_a).await;
        let stage_b = super::first_stage_id(&migrator_pool, org_b).await;

        let vip_b = insert_tag(&migrator_pool, org_b, bob_id, "VIP").await;
        let _person_a = insert_person(&migrator_pool, org_a, stage_a, Some(alice_id)).await;
        let person_b = insert_person(&migrator_pool, org_b, stage_b, Some(bob_id)).await;
        apply_tag(&migrator_pool, org_b, person_b, vip_b, bob_id).await;

        let scope = PersonVisibilityScope::Organization(OrganizationId::new(org_a));
        let params = PersonFilterParams {
            tag_ids_any: Some(vec![vip_b]),
            viewer_id: alice_id,
            ..Default::default()
        };
        let mut conn = migrator_pool.acquire().await.unwrap();
        let (summaries, truncated) = person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
        assert!(!truncated);
        assert!(
            summaries.is_empty(),
            "org A must never match org B's tag id, even though it is a \
             valid, identically-named tag for a different Organization"
        );
    }
}

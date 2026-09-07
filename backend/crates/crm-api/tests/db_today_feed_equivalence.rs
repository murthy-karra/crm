//! Slice 011d §9.2 equivalence gate: `today::query_at_with_provider(...,
//! Feeds)` must equal `..., Legacy)` in full item JSON, order, and
//! `truncated`, both with zero list sources and with sources enabled.
//! `Legacy` is the compiled-in `queries::candidates` statement, unchanged;
//! `Feeds` is the new person-state statement plus the call feed. Both sides
//! return the SAME `TodayList` type (unlike the frozen-fixture parity in
//! `db_today_builtin_parity.rs`), so comparison is a direct JSON equality
//! with no field-stripping.
//!
//! Every fixture Person here is deliberately at the CANONICAL feed
//! definition (`[assigned_to: [me], awaiting_response: true]` etc.) — no
//! command exists yet to store an edited definition (that lands with brief
//! step 4), so this file proves the feed PATH reproduces the built-in
//! item-for-item, not yet a customized-feed scenario (spec §9.5, a later
//! round).

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::today::{self, TodayProvider};
use crm_api::ids::{OrganizationId, UserId};

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempt(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
            person_id, channel, outcome) \
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached') \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_correction(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    actor_user_id: Uuid,
    corrects_id: Uuid,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
            corrects_id, person_id, channel, outcome) \
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'left_message')",
    )
    .bind(organization_id)
    .bind(actor_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(corrects_id)
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_correspondence(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    agent_user_id: Uuid,
    direction: &str,
    occurred_at: DateTime<Utc>,
) {
    let raw_id: Uuid = sqlx::query_scalar(
        "INSERT INTO correspondence_raw \
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed) \
         VALUES ($1, $2, now(), $3, $4, $5, 0, true) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(vec![0_u8; 24])
    .bind(vec![1_u8; 16])
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO correspondence_captured \
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, \
             occurred_at, correlation_id, person_id, agent_user_id, direction, via, \
             correspondence_raw_id, backdated) \
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, $6, 'cc', $7, false)",
    )
    .bind(organization_id)
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

#[allow(clippy::too_many_arguments)]
async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: DateTime<Utc>,
    corrected: bool,
) -> Uuid {
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
             correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES \
            ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
             'scripted', 'equivalence-fixture', $7, $7)",
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
        insert_contact_correction(
            pool,
            organization_id,
            person_id,
            caller_user_id,
            root_id,
            ended_at,
        )
        .await;
    }

    call_id
}

async fn compare_providers(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    fixed_now: DateTime<Utc>,
) -> (Value, Value) {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));

    let mut legacy_conn = app_pool.acquire().await.unwrap();
    let legacy = today::query_at_with_provider(
        &mut legacy_conn,
        &scope,
        UserId::new(viewer_id),
        fixed_now,
        TodayProvider::Legacy,
    )
    .await
    .unwrap();
    drop(legacy_conn);

    let mut feeds_conn = app_pool.acquire().await.unwrap();
    let feeds = today::query_at_with_provider(
        &mut feeds_conn,
        &scope,
        UserId::new(viewer_id),
        fixed_now,
        TodayProvider::Feeds,
    )
    .await
    .unwrap();

    assert_eq!(legacy.generated_at, fixed_now);
    assert_eq!(feeds.generated_at, fixed_now);
    (
        serde_json::to_value(legacy).unwrap(),
        serde_json::to_value(feeds).unwrap(),
    )
}

fn assert_providers_equal(legacy: &Value, feeds: &Value, context: &str) {
    assert_eq!(
        serde_json::to_string(legacy).unwrap(),
        serde_json::to_string(feeds).unwrap(),
        "{context}: Feeds must equal Legacy item-for-item, reason-for-reason, in order, at the cap"
    );
}

/// A rich, single fixture exercising every case spec §9.2 lists at once
/// (an inquiry-only and stale-inquiry Person; an answered-then-repeated
/// inquiry for `repeat_inquiry`; a fresh and a stale reply-only Person; a
/// reply-answered-by-a-later-attempt Person, EXCLUDED; the exact
/// reply == attempt boundary, EXCLUDED (strict `>`); a Person qualifying
/// for BOTH the inquiry and reply arms, reply winning precedence; a
/// zero-inquiry Person with a qualifying call, EXCLUDED by rule 7; a
/// Person with an answered inquiry plus a qualifying call, low tier only;
/// a Person qualifying by both inquiry and call, call reason appended
/// last; a correction chain; two qualifying calls for one Person by the
/// SAME viewer (most recent selected); a call by a DIFFERENT caller,
/// excluded for this viewer), all evaluated with zero list sources.
#[sqlx::test]
#[ignore]
async fn feeds_equals_legacy_for_a_rich_mixed_fixture_with_zero_sources(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence org",
        "alice@d011-equivalence.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let bob_id = crate::common::create_user(
        &migrator_pool,
        "bob@d011-equivalence.test",
        "Bob",
        "correct horse battery staple",
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Inquiry-only, fresh.
    let inquiry_fresh = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        inquiry_fresh,
        "zillow",
        now - ChronoDuration::hours(2),
    )
    .await;

    // Inquiry-only, stale (well past the 24 h window).
    let inquiry_stale = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        inquiry_stale,
        "website",
        now - ChronoDuration::hours(40),
    )
    .await;

    // Answered, then a repeat inquiry — repeat_inquiry (count >= 2), fresh.
    let repeat_inquiry = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        repeat_inquiry,
        "zillow",
        now - ChronoDuration::hours(20),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        repeat_inquiry,
        now - ChronoDuration::hours(10),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        repeat_inquiry,
        "zillow",
        now - ChronoDuration::hours(1),
    )
    .await;

    // Reply-only, fresh.
    let reply_fresh = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        reply_fresh,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(3),
    )
    .await;

    // Reply-only, stale.
    let reply_stale = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        reply_stale,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(30),
    )
    .await;

    // Reply answered by a LATER attempt — excluded entirely (no inquiry
    // either).
    let reply_answered = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        reply_answered,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(6),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        reply_answered,
        now - ChronoDuration::hours(4),
    )
    .await;

    // Reply exactly equal to the last attempt's timestamp — excluded
    // (strict `>`, not `>=`).
    let reply_boundary = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    let boundary_ts = now - ChronoDuration::hours(5);
    insert_contact_attempt(&app_pool, organization_id, reply_boundary, boundary_ts).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        reply_boundary,
        alice_id,
        "inbound",
        boundary_ts,
    )
    .await;

    // Qualifies for BOTH the inquiry and reply arms — reply wins
    // precedence (fresher signal).
    let both_inquiry_and_reply =
        insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        both_inquiry_and_reply,
        "zillow",
        now - ChronoDuration::hours(8),
    )
    .await;
    insert_correspondence(
        &app_pool,
        organization_id,
        both_inquiry_and_reply,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(1),
    )
    .await;

    // Zero-inquiry Person with a qualifying call — EXCLUDED by rule 7,
    // even though the compiled-in outcome_call arm has no such rule
    // (the equivalence gate's whole point: Feeds must still match Legacy
    // — Legacy's own `WHERE latest.id IS NOT NULL` clause already excludes
    // this, so this pins that both sides agree it is absent).
    let call_zero_inquiry =
        insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_call(
        &app_pool,
        organization_id,
        call_zero_inquiry,
        alice_id,
        now - ChronoDuration::hours(2),
        false,
    )
    .await;

    // An inquiry answered long ago (no built-in reason) plus a qualifying
    // call — low tier only.
    let call_low_only = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        call_low_only,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        call_low_only,
        now - ChronoDuration::days(9),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_low_only,
        alice_id,
        now - ChronoDuration::hours(7),
        false,
    )
    .await;

    // Qualifies by inquiry AND by call — call_outcome_needed appended last.
    let both_inquiry_and_call =
        insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        both_inquiry_and_call,
        "zillow",
        now - ChronoDuration::hours(3),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        both_inquiry_and_call,
        alice_id,
        now - ChronoDuration::hours(9),
        false,
    )
    .await;

    // A correction chain: the inquiry appears answered by the ORIGINAL
    // attempt, but that attempt is corrected to a later timestamp that
    // still precedes a second, genuinely later inquiry.
    let corrected_chain = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        corrected_chain,
        "zillow",
        now - ChronoDuration::hours(15),
    )
    .await;
    let original_attempt = insert_contact_attempt(
        &app_pool,
        organization_id,
        corrected_chain,
        now - ChronoDuration::hours(14),
    )
    .await;
    insert_contact_correction(
        &app_pool,
        organization_id,
        corrected_chain,
        alice_id,
        original_attempt,
        now - ChronoDuration::hours(13),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        corrected_chain,
        "zillow",
        now - ChronoDuration::hours(1),
    )
    .await;

    // Two qualifying calls for the SAME Person, same viewer — the most
    // recent (by ended_at DESC, id DESC) is selected.
    let two_calls = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        two_calls,
        "zillow",
        now - ChronoDuration::days(20),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        two_calls,
        now - ChronoDuration::days(19),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        two_calls,
        alice_id,
        now - ChronoDuration::hours(20),
        false,
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        two_calls,
        alice_id,
        now - ChronoDuration::hours(6),
        false,
    )
    .await;

    // A call by a DIFFERENT caller (Bob), for a Person not assigned to
    // Alice — excluded from Alice's Today entirely.
    let called_by_bob = insert_person(&app_pool, organization_id, stage_id, None).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        called_by_bob,
        "zillow",
        now - ChronoDuration::days(5),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        called_by_bob,
        now - ChronoDuration::days(4),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        called_by_bob,
        bob_id,
        now - ChronoDuration::hours(2),
        false,
    )
    .await;

    // A corrected call outcome — no longer needs an outcome, excluded.
    let call_corrected = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        call_corrected,
        "zillow",
        now - ChronoDuration::days(3),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        call_corrected,
        now - ChronoDuration::days(2),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_corrected,
        alice_id,
        now - ChronoDuration::hours(1),
        true,
    )
    .await;

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&legacy, &feeds, "alice's rich mixed fixture");

    // Bob's own view (caller of the excluded call, assignee of none of
    // this fixture's People) must also match.
    let (legacy_bob, feeds_bob) = compare_providers(&app_pool, organization_id, bob_id, now).await;
    assert_providers_equal(&legacy_bob, &feeds_bob, "bob's view of the same fixture");
}

/// A second, unrelated Organization must never see the first's People —
/// both providers agree per-tenant.
#[sqlx::test]
#[ignore]
async fn feeds_equals_legacy_across_two_tenants(migrator_pool: PgPool) {
    let (org_a, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence tenant A",
        "alice@d011-tenant-a.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let (org_b, carol_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence tenant B",
        "carol@d011-tenant-b.test",
        "Carol",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    let stage_a = first_stage_id(&app_pool, org_a).await;
    let person_a = insert_person(&app_pool, org_a, stage_a, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        org_a,
        person_a,
        "zillow",
        now - ChronoDuration::hours(1),
    )
    .await;

    let stage_b = first_stage_id(&app_pool, org_b).await;
    let person_b = insert_person(&app_pool, org_b, stage_b, Some(carol_id)).await;
    insert_inquiry(
        &app_pool,
        org_b,
        person_b,
        "website",
        now - ChronoDuration::hours(2),
    )
    .await;
    insert_call(
        &app_pool,
        org_b,
        person_b,
        carol_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    let (legacy_a, feeds_a) = compare_providers(&app_pool, org_a, alice_id, now).await;
    assert_providers_equal(&legacy_a, &feeds_a, "tenant A");
    let a_ids: Vec<&str> = feeds_a["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(a_ids, vec![person_a.to_string().as_str()]);

    let (legacy_b, feeds_b) = compare_providers(&app_pool, org_b, carol_id, now).await;
    assert_providers_equal(&legacy_b, &feeds_b, "tenant B");
    let b_ids: Vec<&str> = feeds_b["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(b_ids, vec![person_b.to_string().as_str()]);
}

/// A deactivated caller is still a valid `assigned_to`/caller reference
/// (D-027) — their historical call still counts.
#[sqlx::test]
#[ignore]
async fn feeds_equals_legacy_with_a_deactivated_caller(migrator_pool: PgPool) {
    let (organization_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence deactivated caller",
        "alice@d011-deactivated.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let dave_id = crate::common::create_user(
        &migrator_pool,
        "dave@d011-deactivated.test",
        "Dave",
        "correct horse battery staple",
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        dave_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let person_id = insert_person(&app_pool, organization_id, stage_id, Some(dave_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        person_id,
        "zillow",
        now - ChronoDuration::days(3),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        person_id,
        now - ChronoDuration::days(2),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        person_id,
        dave_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(dave_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, dave_id, now).await;
    assert_providers_equal(&legacy, &feeds, "deactivated caller still owed the outcome");
    assert_eq!(feeds["items"].as_array().unwrap().len(), 1);
}

/// With a saved list enabled as a Today source, the built-in band (which
/// Feeds now produces) must still match Legacy's, and the list-only band
/// stays correctly appended — proving the seam runs the WHOLE path,
/// list-source merge included, both ways.
#[sqlx::test]
#[ignore]
async fn feeds_equals_legacy_with_a_list_source_enabled(migrator_pool: PgPool) {
    use crm_api::domain::envelope::{CommandContext, Origin};
    use crm_api::domain::person::filter::{
        AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    };
    use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
    use crm_api::ids::CorrelationId;

    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence with source",
        "alice@d011-with-source.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // A built-in item (inquiry-based).
    let builtin_person = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        builtin_person,
        "zillow",
        now - ChronoDuration::hours(1),
    )
    .await;

    // A list-only Person: has a phone, no inquiry/reply/call qualification.
    let list_only_person =
        insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value) \
         VALUES ($1, $2, 'phone', '+15555550100', '+15555550100')",
    )
    .bind(organization_id)
    .bind(list_only_person)
    .execute(&app_pool)
    .await
    .unwrap();

    let ctx = CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(alice_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let list = saved_list::create_saved_list(
        &app_pool,
        &ctx,
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: "Has phone".to_string(),
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::HasPhone(BoolClause { value: true }),
                ],
            },
            sort: None,
        },
    )
    .await
    .unwrap();
    today::enable_today_work_source(
        &app_pool,
        &ctx,
        today::EnableTodayWorkSource {
            list_id: list.list.id,
            expected_list_revision: list.list.revision,
        },
    )
    .await
    .unwrap();

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&legacy, &feeds, "with a list source enabled");

    let ids: Vec<Value> = feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].clone())
        .collect();
    assert!(ids.contains(&Value::String(builtin_person.to_string())));
    assert!(ids.contains(&Value::String(list_only_person.to_string())));
}

/// spec §9.5 ("call feed disabled/enabled"): disabling the call feed is a
/// `Feeds`-only concept (`Legacy` has no such control), so this is not an
/// equality-with-Legacy case — it proves Feeds itself changes: a Person who
/// only qualifies by the call feed disappears entirely, and a Person who
/// also qualifies by inquiry keeps their inquiry-based reasons but loses
/// `call_outcome_needed`. Re-enabling restores both, matching Legacy again.
#[sqlx::test]
#[ignore]
async fn feeds_call_feed_disabled_omits_call_items_and_reasons_re_enabling_restores_them(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d equivalence call disable",
        "alice@d011-call-disable.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Call-only (low tier only).
    let call_only = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        call_only,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        call_only,
        now - ChronoDuration::days(9),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_only,
        alice_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    // Both inquiry and call.
    let both = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        both,
        "zillow",
        now - ChronoDuration::hours(2),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        both,
        alice_id,
        now - ChronoDuration::hours(5),
        false,
    )
    .await;

    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));

    // Enabled (the seeded default): matches Legacy, both items present.
    {
        let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
        assert_providers_equal(&legacy, &feeds, "call feed enabled (default)");
        let ids: Vec<Value> = feeds["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["person"]["id"].clone())
            .collect();
        assert!(ids.contains(&Value::String(call_only.to_string())));
        assert!(ids.contains(&Value::String(both.to_string())));
    }

    sqlx::query(
        "UPDATE today_system_feed SET enabled = false, revision = revision + 1
         WHERE organization_id = $1 AND feed_key = 'call_outcome_needed'",
    )
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();

    {
        let mut conn = app_pool.acquire().await.unwrap();
        let feeds_disabled = today::query_at_with_provider(
            &mut conn,
            &scope,
            UserId::new(alice_id),
            now,
            TodayProvider::Feeds,
        )
        .await
        .unwrap();
        let ids: Vec<String> = feeds_disabled
            .items
            .iter()
            .map(|item| item.person.id.to_string())
            .collect();
        assert!(
            !ids.contains(&call_only.to_string()),
            "a call-only Person must vanish entirely once the call feed is disabled"
        );
        assert!(
            ids.contains(&both.to_string()),
            "the dual-qualifying Person keeps their inquiry-based item"
        );
        let both_item = feeds_disabled
            .items
            .iter()
            .find(|item| item.person.id.to_string() == both.to_string())
            .unwrap();
        assert!(
            !both_item.reasons.iter().any(|r| matches!(
                r,
                crm_api::domain::today::TodayReason::CallOutcomeNeeded { .. }
            )),
            "call_outcome_needed must not appear while the call feed is disabled"
        );
        assert!(matches!(
            feeds_disabled.sources.status,
            crm_api::domain::today::TodaySourcesStatus::Complete
        ));
        assert!(
            feeds_disabled.sources.system_feed_issues.is_empty(),
            "a disabled feed contributes no work and no issue"
        );
    }

    sqlx::query(
        "UPDATE today_system_feed SET enabled = true, revision = revision + 1
         WHERE organization_id = $1 AND feed_key = 'call_outcome_needed'",
    )
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&legacy, &feeds, "call feed re-enabled");
}

/// Coordinator decision (round 3): `call_membership.sql`/`call_only.sql`
/// now bind the full `PersonFilterParams` matrix (spec §1 rule 4, §5 step
/// 4 "feed C matrix params"), not just the fixed outcome_call membership.
/// With the call feed left at its CANONICAL, unmodified definition (every
/// added-axis param NULL), the matrix collapses to exactly the original
/// fixed query, so `Feeds` must still equal `Legacy` byte-for-byte across
/// a call-heavy fixture: call-only Persons, a Person qualifying by both
/// inquiry and call (call reason appended last), a corrected call
/// (excluded), two qualifying calls for one Person (most recent selected),
/// and a call by a DIFFERENT caller (excluded for this viewer).
#[sqlx::test]
#[ignore]
async fn feeds_equals_legacy_for_the_call_feed_matrix_statements_with_no_extra_clauses(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d call matrix parity",
        "alice@d011-call-matrix-parity.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let bob_id = crate::common::create_user(
        &migrator_pool,
        "bob@d011-call-matrix-parity.test",
        "Bob",
        "correct horse battery staple",
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Call-only, no inquiry-based membership.
    let call_only = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        call_only,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_contact_attempt(
        &app_pool,
        organization_id,
        call_only,
        now - ChronoDuration::days(9),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        call_only,
        alice_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    // Both inquiry (fresh, unanswered) and a qualifying call — call reason
    // appended last, after the inquiry-based reason.
    let both = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        both,
        "zillow",
        now - ChronoDuration::hours(2),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        both,
        alice_id,
        now - ChronoDuration::hours(5),
        false,
    )
    .await;

    // A corrected call: the automatic root attempt is superseded, so the
    // call feed's `NOT EXISTS` correction guard excludes this Person.
    let corrected = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        corrected,
        "zillow",
        now - ChronoDuration::days(20),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        corrected,
        alice_id,
        now - ChronoDuration::hours(4),
        true,
    )
    .await;

    // Two qualifying calls for the SAME Person by the SAME viewer — the
    // most recent (by ended_at) must be the one carried in the reason.
    let two_calls = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        two_calls,
        "zillow",
        now - ChronoDuration::days(15),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        two_calls,
        alice_id,
        now - ChronoDuration::hours(9),
        false,
    )
    .await;
    let most_recent_call = insert_call(
        &app_pool,
        organization_id,
        two_calls,
        alice_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    // A call by a DIFFERENT caller (Bob) — excluded for Alice's viewer.
    let different_caller =
        insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        different_caller,
        "zillow",
        now - ChronoDuration::days(12),
    )
    .await;
    insert_call(
        &app_pool,
        organization_id,
        different_caller,
        bob_id,
        now - ChronoDuration::hours(2),
        false,
    )
    .await;

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(
        &legacy,
        &feeds,
        "call feed matrix statements, canonical definition",
    );

    let ids: Vec<Value> = feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].clone())
        .collect();
    assert!(ids.contains(&Value::String(call_only.to_string())));
    assert!(ids.contains(&Value::String(both.to_string())));
    assert!(
        !ids.contains(&Value::String(corrected.to_string())),
        "a corrected call must not qualify"
    );
    assert!(ids.contains(&Value::String(two_calls.to_string())));
    assert!(
        !ids.contains(&Value::String(different_caller.to_string())),
        "a call by a different caller must not qualify for this viewer"
    );
    let two_calls_item = feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["person"]["id"] == Value::String(two_calls.to_string()))
        .unwrap();
    let reasons = two_calls_item["reasons"].as_array().unwrap();
    let call_reason = reasons
        .iter()
        .find(|r| r["code"] == Value::String("call_outcome_needed".to_string()))
        .unwrap();
    assert_eq!(
        call_reason["call_id"],
        Value::String(most_recent_call.to_string()),
        "the most recent qualifying call is the one carried in the reason"
    );
}

/// Correction (a): spec §5 — "a disabled feed contributes nothing and no
/// issue" covers fallback reporting too. A disabled feed with an invalid
/// stored definition must produce no `system_feed_issues` entry; the
/// "invalid stored rule" belongs on the admin page via `Feed.filter_error`
/// (step 4), not here.
#[sqlx::test]
#[ignore]
async fn a_disabled_feed_with_an_invalid_stored_definition_reports_no_issue(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d correction a",
        "alice@d011-correction-a.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();

    // A stored definition referencing a stage that does not exist —
    // structurally valid, reference-invalid, so `load_feed_rows` marks it
    // `fallback: true` regardless of `enabled`.
    let broken_filter = serde_json::json!({
        "version": 1,
        "clauses": [
            {"kind": "assigned_to", "assignees": ["me"]},
            {"kind": "awaiting_response", "value": true},
            {"kind": "stage", "stage_ids": [Uuid::new_v4()]},
        ]
    });

    // Enabled + invalid: DOES report an issue (existing, already-tested
    // behavior — pinned again here as the contrasting baseline).
    sqlx::query(
        "UPDATE today_system_feed SET filter = $1, enabled = true, revision = revision + 1
         WHERE organization_id = $2 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(&broken_filter)
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let enabled_and_invalid = today::query_at(&mut conn, &scope, UserId::new(alice_id), now)
        .await
        .unwrap();
    drop(conn);
    assert_eq!(enabled_and_invalid.sources.system_feed_issues.len(), 1);
    assert_eq!(
        enabled_and_invalid.sources.system_feed_issues[0].feed_key,
        "unanswered_inquiry"
    );
    assert!(enabled_and_invalid.sources.system_feed_issues[0].fallback);

    // Disabled + the SAME invalid definition: no issue at all.
    sqlx::query(
        "UPDATE today_system_feed SET enabled = false, revision = revision + 1
         WHERE organization_id = $1 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(organization_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let disabled_and_invalid = today::query_at(&mut conn, &scope, UserId::new(alice_id), now)
        .await
        .unwrap();
    assert!(
        disabled_and_invalid.sources.system_feed_issues.is_empty(),
        "a disabled feed's invalid stored rule must not surface as a system_feed_issues entry"
    );
    assert!(matches!(
        disabled_and_invalid.sources.status,
        crm_api::domain::today::TodaySourcesStatus::Complete
    ));
}

// --- Review round 1, F5: equivalence cases to pin BEFORE deleting Legacy ---

use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
};
use crm_api::domain::today::system_feeds::commands::{self, UpdateTodaySystemFeed};
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::CorrelationId;

fn f5_command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

/// `create_org_with_stages_and_member` already inserted this user's
/// membership row as a plain member; UPDATE its role rather than
/// re-inserting (which would collide with the existing primary key).
async fn f5_promote_to_admin(pool: &PgPool, organization_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' \
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

/// F5.1: the fresh boundary is STRICT (`>`, not `>=`) for BOTH
/// person-state feeds at the canonical 24 h window — an inquiry/reply at
/// EXACTLY `now - 24h` is normal priority; one second later is high.
/// Legacy and Feeds must agree at both sides of the boundary.
#[sqlx::test]
#[ignore]
async fn fresh_boundary_at_exactly_24_hours_is_normal_one_second_later_is_high(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 fresh boundary 24h",
        "alice@d011-f5-boundary-24h.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Feed A (inquiry-based).
    let a_at_boundary = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        a_at_boundary,
        "zillow",
        now - ChronoDuration::hours(24),
    )
    .await;
    let a_inside = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        a_inside,
        "zillow",
        now - ChronoDuration::hours(24) + ChronoDuration::seconds(1),
    )
    .await;

    // Feed B (reply-based): each needs an inquiry too (rule 7) plus an
    // inbound reply at the boundary/inside it.
    let b_at_boundary = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        b_at_boundary,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_correspondence(
        &app_pool,
        organization_id,
        b_at_boundary,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(24),
    )
    .await;
    let b_inside = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        b_inside,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    insert_correspondence(
        &app_pool,
        organization_id,
        b_inside,
        alice_id,
        "inbound",
        now - ChronoDuration::hours(24) + ChronoDuration::seconds(1),
    )
    .await;

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&legacy, &feeds, "fresh boundary at exactly 24h, both feeds");

    let priority_of = |value: &Value, person_id: Uuid| -> String {
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["person"]["id"] == Value::String(person_id.to_string()))
            .unwrap_or_else(|| panic!("Person {person_id} missing from items"))["priority"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        priority_of(&feeds, a_at_boundary),
        "normal",
        "feed A at exactly the boundary"
    );
    assert_eq!(
        priority_of(&feeds, a_inside),
        "high",
        "feed A one second inside the boundary"
    );
    assert_eq!(
        priority_of(&feeds, b_at_boundary),
        "normal",
        "feed B at exactly the boundary"
    );
    assert_eq!(
        priority_of(&feeds, b_inside),
        "high",
        "feed B one second inside the boundary"
    );
}

/// F5.1 (customized window): the SAME strict boundary holds for a
/// customized 1 h window — Feeds-only (Legacy has no customizable
/// window), co-located here for the same boundary-family coverage.
#[sqlx::test]
#[ignore]
async fn fresh_boundary_at_exactly_a_customized_1_hour_window_is_normal_one_second_later_is_high(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 fresh boundary 1h",
        "alice@d011-f5-boundary-1h.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    f5_promote_to_admin(&migrator_pool, organization_id, alice_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    commands::update_today_system_feed(
        &app_pool,
        &f5_command_context(organization_id, alice_id),
        UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::AwaitingResponse(BoolClause { value: true }),
                ],
            },
            fresh_within_hours: Some(1),
        },
    )
    .await
    .unwrap();

    let at_boundary = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        at_boundary,
        "zillow",
        now - ChronoDuration::hours(1),
    )
    .await;
    let inside = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        inside,
        "zillow",
        now - ChronoDuration::hours(1) + ChronoDuration::seconds(1),
    )
    .await;

    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let mut conn = app_pool.acquire().await.unwrap();
    let list = today::query_at(&mut conn, &scope, UserId::new(alice_id), now)
        .await
        .unwrap();
    let priority_of = |person_id: Uuid| -> crm_api::domain::today::TodayPriority {
        list.items
            .iter()
            .find(|i| i.person.id.as_uuid() == person_id)
            .unwrap()
            .priority
    };
    assert_eq!(
        priority_of(at_boundary),
        crm_api::domain::today::TodayPriority::Normal
    );
    assert_eq!(
        priority_of(inside),
        crm_api::domain::today::TodayPriority::High
    );
}

/// F5.2: an inbound reply exactly EQUAL to the latest outbound must be
/// EXCLUDED (the predicate is strict `>`, not `>=`) — Legacy and Feeds
/// agree that this Person does not qualify by reply.
#[sqlx::test]
#[ignore]
async fn reply_equal_to_the_latest_outbound_is_excluded(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 reply eq outbound",
        "alice@d011-f5-reply-eq.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();
    let tie = now - ChronoDuration::hours(1);

    let person = insert_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        person,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    // Already answered (a contact attempt after the inquiry, before the
    // tie), so this Person does NOT independently qualify via feed A —
    // isolating the reply-vs-outbound tie as the only signal under test.
    insert_contact_attempt(
        &app_pool,
        organization_id,
        person,
        now - ChronoDuration::days(9),
    )
    .await;
    insert_correspondence(&app_pool, organization_id, person, alice_id, "inbound", tie).await;
    insert_correspondence(
        &app_pool,
        organization_id,
        person,
        alice_id,
        "outbound",
        tie,
    )
    .await;

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&legacy, &feeds, "reply equal to the latest outbound");
    let ids: Vec<Value> = feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].clone())
        .collect();
    assert!(
        !ids.contains(&Value::String(person.to_string())),
        "a reply exactly equal to the latest outbound must not qualify by reply, and the \
         already-answered inquiry must not qualify by inquiry either"
    );
}

/// F5.3: with more than 201 dual-qualifying (inquiry AND reply) People,
/// Legacy and Feeds must truncate to the identical top-200 set and agree
/// on `truncated: true` — proving the cap and tie-break ordering (`fresh
/// DESC, order_key ASC, id ASC`) behave identically past the boundary.
#[sqlx::test]
#[ignore]
async fn dual_qualifying_people_beyond_the_201_cap_truncate_identically(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 dual beyond cap",
        "alice@d011-f5-dual-cap.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // 205 People, each qualifying for BOTH feeds (reply wins), each with
    // a DISTINCT reply timestamp so the tie-break order is deterministic
    // and identical between the two providers.
    let person_ids: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id)
         SELECT $1, $2, $3 FROM generate_series(1, 205) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(alice_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    for (offset, person_id) in person_ids.iter().enumerate() {
        insert_inquiry(
            &app_pool,
            organization_id,
            *person_id,
            "zillow",
            now - ChronoDuration::days(10),
        )
        .await;
        insert_correspondence(
            &app_pool,
            organization_id,
            *person_id,
            alice_id,
            "inbound",
            now - ChronoDuration::minutes(offset as i64),
        )
        .await;
    }

    let (legacy, feeds) = compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(
        &legacy,
        &feeds,
        "205 dual-qualifying People beyond the 201 cap",
    );
    assert_eq!(feeds["truncated"], true);
    assert_eq!(feeds["items"].as_array().unwrap().len(), 200);
}

/// F5.4: person-state row counts at 199/200/201 (spanning the internal
/// 201-fetch/200-cap boundary) combined with 0..3 call-only rows (which
/// only ever run when person-state was NOT truncated, spec §5 step 4b) —
/// Legacy and Feeds must agree on the full item set and `truncated` at
/// every one of the twelve combinations.
#[sqlx::test]
#[ignore]
async fn person_state_and_call_only_counts_near_the_boundary_match_legacy(migrator_pool: PgPool) {
    for person_state_count in [199usize, 200, 201] {
        for call_only_count in [0usize, 1, 2, 3] {
            let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
                &migrator_pool,
                &format!("011d f5 boundary {person_state_count} {call_only_count}"),
                &format!("alice-{person_state_count}-{call_only_count}@d011-f5-boundary.test"),
                "Alice",
                "correct horse battery staple",
            )
            .await;
            let app_pool = crate::common::connect_as_app(&migrator_pool).await;
            let stage_id = first_stage_id(&app_pool, organization_id).await;
            let now = Utc::now();

            let person_ids: Vec<Uuid> = sqlx::query_scalar(
                "INSERT INTO person (organization_id, stage_id, assigned_user_id)
                 SELECT $1, $2, $3 FROM generate_series(1, $4) RETURNING id",
            )
            .bind(organization_id)
            .bind(stage_id)
            .bind(alice_id)
            .bind(person_state_count as i64)
            .fetch_all(&app_pool)
            .await
            .unwrap();
            for (offset, person_id) in person_ids.iter().enumerate() {
                insert_inquiry(
                    &app_pool,
                    organization_id,
                    *person_id,
                    "zillow",
                    now - ChronoDuration::hours(1) - ChronoDuration::seconds(offset as i64),
                )
                .await;
            }

            let call_only_person_ids: Vec<Uuid> = if call_only_count > 0 {
                sqlx::query_scalar(
                    "INSERT INTO person (organization_id, stage_id, assigned_user_id)
                     SELECT $1, $2, $3 FROM generate_series(1, $4) RETURNING id",
                )
                .bind(organization_id)
                .bind(stage_id)
                .bind(alice_id)
                .bind(call_only_count as i64)
                .fetch_all(&app_pool)
                .await
                .unwrap()
            } else {
                Vec::new()
            };
            for (offset, person_id) in call_only_person_ids.iter().enumerate() {
                // An answered inquiry (not awaiting_response) plus a
                // qualifying call: call-only, not person-state.
                insert_inquiry(
                    &app_pool,
                    organization_id,
                    *person_id,
                    "zillow",
                    now - ChronoDuration::days(10),
                )
                .await;
                insert_contact_attempt(
                    &app_pool,
                    organization_id,
                    *person_id,
                    now - ChronoDuration::days(9),
                )
                .await;
                insert_call(
                    &app_pool,
                    organization_id,
                    *person_id,
                    alice_id,
                    now - ChronoDuration::hours(2) - ChronoDuration::seconds(offset as i64),
                    false,
                )
                .await;
            }

            let (legacy, feeds) =
                compare_providers(&app_pool, organization_id, alice_id, now).await;
            assert_providers_equal(
                &legacy,
                &feeds,
                &format!("person_state={person_state_count} call_only={call_only_count}"),
            );
        }
    }
}

/// F5.5: the call feed's `caller_user_id` axis is independent of the
/// Person's `assigned_user_id` — across two viewers, a Person assigned to
/// one but called by the other qualifies ONLY for the actual caller's
/// Today, at both viewers, in agreement between Legacy and Feeds.
#[sqlx::test]
#[ignore]
async fn caller_not_equal_to_assignee_across_two_viewers(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 caller ne assignee",
        "alice@d011-f5-caller-ne.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let bob_id = crate::common::create_user(
        &migrator_pool,
        "bob@d011-f5-caller-ne.test",
        "Bob",
        "correct horse battery staple",
    )
    .await;
    crate::common::add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    // Assigned to Bob, but the call was placed by Alice.
    let callee = insert_person(&app_pool, organization_id, stage_id, Some(bob_id)).await;
    insert_inquiry(
        &app_pool,
        organization_id,
        callee,
        "zillow",
        now - ChronoDuration::days(10),
    )
    .await;
    let call_id = insert_call(
        &app_pool,
        organization_id,
        callee,
        alice_id,
        now - ChronoDuration::hours(1),
        false,
    )
    .await;

    let (alice_legacy, alice_feeds) =
        compare_providers(&app_pool, organization_id, alice_id, now).await;
    assert_providers_equal(&alice_legacy, &alice_feeds, "caller (Alice)'s Today");
    let alice_ids: Vec<Value> = alice_feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].clone())
        .collect();
    assert!(
        alice_ids.contains(&Value::String(callee.to_string())),
        "the CALLER sees the callout, regardless of assignment"
    );

    let (bob_legacy, bob_feeds) = compare_providers(&app_pool, organization_id, bob_id, now).await;
    assert_providers_equal(&bob_legacy, &bob_feeds, "assignee (Bob)'s Today");
    let bob_ids: Vec<Value> = bob_feeds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].clone())
        .collect();
    assert!(
        !bob_ids.contains(&Value::String(callee.to_string())),
        "the ASSIGNEE, who did not place the call, must not see the call_outcome_needed item"
    );
    let _ = call_id;
}

/// F5.7: preview's own call-only cap (200/truncated) is exercised on the
/// call feed specifically (existing coverage only exercised
/// `unanswered_inquiry`'s preview cap).
#[sqlx::test]
#[ignore]
async fn preview_of_the_call_feed_caps_at_200_with_truncated(migrator_pool: PgPool) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "011d f5 preview call cap",
        "alice@d011-f5-preview-call-cap.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    f5_promote_to_admin(&migrator_pool, organization_id, alice_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let now = Utc::now();

    let person_ids: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id)
         SELECT $1, $2, $3 FROM generate_series(1, 205) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(alice_id)
    .fetch_all(&app_pool)
    .await
    .unwrap();
    for (offset, person_id) in person_ids.iter().enumerate() {
        insert_inquiry(
            &app_pool,
            organization_id,
            *person_id,
            "zillow",
            now - ChronoDuration::days(10),
        )
        .await;
        insert_call(
            &app_pool,
            organization_id,
            *person_id,
            alice_id,
            now - ChronoDuration::hours(1) - ChronoDuration::seconds(offset as i64),
            false,
        )
        .await;
    }

    let preview = commands::preview_today_system_feed(
        &app_pool,
        &f5_command_context(organization_id, alice_id),
        crm_api::domain::today::system_feeds::commands::PreviewTodaySystemFeed {
            feed_key: FeedKey::CallOutcomeNeeded,
            filter: FilterDefinition {
                version: 1,
                clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
            },
            fresh_within_hours: None,
            subject: UserId::new(alice_id),
        },
    )
    .await
    .unwrap();
    assert_eq!(preview.items.len(), 200);
    assert!(preview.truncated);
}

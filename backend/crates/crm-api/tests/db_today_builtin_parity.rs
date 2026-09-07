//! Slice 011c AC6: a frozen pre-source Today projection stays byte-for-byte
//! equivalent to the source-aware projection when no saved-list source is
//! enabled. The only intentionally excluded response member is the new
//! `sources` envelope, which is asserted separately to be complete and empty.
//!
//! The frozen query is a reusable fixture rather than a reimplementation:
//! `fixtures/today_9d62e86` contains the original `9d62e86` DTO, SQL, and
//! ranking code. Both sides receive the same fixed fixture clock.

#[path = "fixtures/today_9d62e86/mod.rs"]
pub mod today_9d62e86;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::today;
use crm_api::ids::{OrganizationId, UserId};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

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
    person_id: Uuid,
    assigned_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO person (id, organization_id, stage_id, assigned_user_id, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(person_id)
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_method(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    kind: &str,
    value: &str,
) {
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_inquiry(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    inquiry_id: Uuid,
    source: &str,
    received_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(inquiry_id)
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
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
            corrects_id, person_id, channel, outcome) \
         VALUES ($1, 'user', $2, 'web_session', $3, $4, $5, $6, 'call', 'left_message') \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(actor_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(corrects_id)
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Creates the durable facts that the original projection recognizes as a
/// caller-owned outcome-needed item. This deliberately uses fixture facts;
/// the call state-machine itself is covered by `db_calls.rs`.
async fn insert_low_outcome(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: DateTime<Utc>,
) -> Uuid {
    insert_inquiry(
        pool,
        organization_id,
        person_id,
        Uuid::new_v4(),
        "older-before-call",
        ended_at - ChronoDuration::days(3),
    )
    .await;

    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
             correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES \
            ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
             'scripted', 'today-baseline-parity', $7, $7)",
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

    sqlx::query(
        "INSERT INTO contact_attempted \
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id, \
             causation_id, person_id, channel, outcome) \
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, $5, 'call', 'reached')",
    )
    .bind(organization_id)
    .bind(ended_at)
    .bind(Uuid::new_v4())
    .bind(call_id)
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();

    call_id
}

async fn insert_inbound_correspondence(
    migrator_pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    agent_user_id: Uuid,
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
    .fetch_one(migrator_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO correspondence_captured \
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, \
             occurred_at, correlation_id, person_id, agent_user_id, direction, via, \
             correspondence_raw_id, backdated) \
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $4, $5, $2, 'inbound', 'cc', $6, false)",
    )
    .bind(organization_id)
    .bind(agent_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(raw_id)
    .execute(migrator_pool)
    .await
    .unwrap();
}

fn item_ids(payload: &Value) -> Vec<Uuid> {
    payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["person"]["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

/// Runs the frozen query and the current production query at one fixture
/// clock. A zero-source result must differ only by the declared sources
/// metadata; returning normalized JSON ensures every item field, reason,
/// order, and truncation flag remains under comparison.
async fn original_and_current_payloads(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    fixed_now: DateTime<Utc>,
) -> (Value, Value) {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let mut original_conn = app_pool.acquire().await.unwrap();
    let original = today_9d62e86::query(
        &mut original_conn,
        &scope,
        UserId::new(viewer_id),
        fixed_now,
    )
    .await
    .unwrap();
    drop(original_conn);

    let mut current_conn = app_pool.acquire().await.unwrap();
    let current = today::query_at(&mut current_conn, &scope, UserId::new(viewer_id), fixed_now)
        .await
        .unwrap();
    assert_eq!(current.generated_at, fixed_now);

    let original_payload = serde_json::to_value(original).unwrap();
    let mut current_payload = serde_json::to_value(current).unwrap();
    let sources = current_payload
        .as_object_mut()
        .unwrap()
        .remove("sources")
        .expect("source-aware response must contain its declared envelope");
    assert_eq!(sources, json!({ "status": "complete", "issues": [] }));
    assert_eq!(
        serde_json::to_string(&current_payload).unwrap(),
        serde_json::to_string(&original_payload).unwrap(),
        "the serialized original payload, item order, and truncation must be unchanged"
    );
    (original_payload, current_payload)
}

async fn assert_zero_enabled_sources(app_pool: &PgPool, organization_id: Uuid, viewer_id: Uuid) {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(viewer_id)
    .fetch_one(app_pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "the baseline comparison has no enabled source");
}

/// AC6: with no source preference, the source-aware projection preserves the
/// original full Today item payload and order across fresh/repeat, normal,
/// corrected-contact, same-time UUID tie, and low outcome-needed facts.
#[sqlx::test]
#[ignore]
async fn today_zero_sources_preserves_frozen_full_builtin_payload_and_order(migrator_pool: PgPool) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today frozen parity",
        "today-frozen-parity@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let fixed_now = Utc.with_ymd_and_hms(2042, 6, 1, 12, 0, 0).unwrap();

    let repeated_high = Uuid::from_u128(0x101);
    let tied_high_low_id = Uuid::from_u128(0x102);
    let tied_high_high_id = Uuid::from_u128(0x103);
    let corrected_normal = Uuid::from_u128(0x104);
    let plain_normal = Uuid::from_u128(0x105);
    let low_outcome = Uuid::from_u128(0x106);

    for (person_id, assigned_user_id, created_at) in [
        (
            repeated_high,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(40),
        ),
        (
            tied_high_low_id,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(39),
        ),
        (
            tied_high_high_id,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(38),
        ),
        (
            corrected_normal,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(37),
        ),
        (
            plain_normal,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(36),
        ),
        (low_outcome, None, fixed_now - ChronoDuration::days(35)),
    ] {
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            person_id,
            assigned_user_id,
            created_at,
        )
        .await;
    }
    insert_contact_method(
        &app_pool,
        organization_id,
        repeated_high,
        "phone",
        "+15555550101",
    )
    .await;
    insert_contact_method(
        &app_pool,
        organization_id,
        tied_high_low_id,
        "email",
        "tied-low@example.test",
    )
    .await;

    insert_inquiry(
        &app_pool,
        organization_id,
        repeated_high,
        Uuid::from_u128(0x201),
        "historic-repeat",
        fixed_now - ChronoDuration::days(8),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        repeated_high,
        Uuid::from_u128(0x202),
        "fresh-repeat",
        fixed_now - ChronoDuration::hours(3),
    )
    .await;
    // Both high items wait from the same instant. Person UUID is the final
    // tie-break, while the first one also establishes latest-Inquiry UUID
    // tie behavior inside its own history.
    insert_inquiry(
        &app_pool,
        organization_id,
        tied_high_low_id,
        Uuid::from_u128(0x203),
        "tied-inquiry-old-id",
        fixed_now - ChronoDuration::hours(1),
    )
    .await;
    let tied_latest = Uuid::from_u128(0x204);
    insert_inquiry(
        &app_pool,
        organization_id,
        tied_high_low_id,
        tied_latest,
        "tied-inquiry-new-id",
        fixed_now - ChronoDuration::hours(1),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        tied_high_high_id,
        Uuid::from_u128(0x205),
        "same-wait-other-person",
        fixed_now - ChronoDuration::hours(1),
    )
    .await;

    let corrected_root = insert_contact_attempt(
        &app_pool,
        organization_id,
        corrected_normal,
        fixed_now - ChronoDuration::days(10),
    )
    .await;
    let corrected_effective = insert_contact_correction(
        &app_pool,
        organization_id,
        corrected_normal,
        viewer_id,
        corrected_root,
        fixed_now - ChronoDuration::days(10),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        corrected_normal,
        Uuid::from_u128(0x206),
        "before-effective-contact",
        fixed_now - ChronoDuration::days(12),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        corrected_normal,
        Uuid::from_u128(0x207),
        "after-effective-contact",
        fixed_now - ChronoDuration::days(8),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        plain_normal,
        Uuid::from_u128(0x208),
        "plain-normal",
        fixed_now - ChronoDuration::days(6),
    )
    .await;
    let low_call = insert_low_outcome(
        &app_pool,
        organization_id,
        low_outcome,
        viewer_id,
        fixed_now - ChronoDuration::hours(2),
    )
    .await;

    assert_zero_enabled_sources(&app_pool, organization_id, viewer_id).await;
    let (original, current) =
        original_and_current_payloads(&app_pool, organization_id, viewer_id, fixed_now).await;
    assert_eq!(
        current, original,
        "only the declared sources envelope may differ"
    );
    assert_eq!(
        item_ids(&current),
        vec![
            repeated_high,
            tied_high_low_id,
            tied_high_high_id,
            corrected_normal,
            plain_normal,
            low_outcome,
        ]
    );

    let items = current["items"].as_array().unwrap();
    assert_eq!(items[0]["priority"], "high");
    assert_eq!(
        items[0]["reasons"],
        json!([
            {
                "code": "new_inquiry",
                "source": "fresh-repeat",
                "received_at": fixed_now - ChronoDuration::hours(3),
            },
            {
                "code": "no_contact_attempt",
                "since": fixed_now - ChronoDuration::days(8),
            },
            { "code": "repeat_inquiry", "inquiry_count": 2 },
        ])
    );
    assert_eq!(items[1]["latest_inquiry"]["id"], tied_latest.to_string());
    assert_eq!(items[1]["latest_inquiry"]["source"], "tied-inquiry-new-id");
    assert_eq!(items[3]["priority"], "normal");
    assert_eq!(
        items[3]["last_contact_attempt"]["id"],
        corrected_effective.to_string(),
        "the correction, not its superseded root, is projected"
    );
    assert_eq!(items[5]["priority"], "low");
    assert_eq!(items[5]["recommended_action"], "set_outcome");
    assert_eq!(items[5]["reasons"][0]["call_id"], low_call.to_string());
}

/// AC6's cap boundary: the source-aware path keeps the original 201-row
/// sentinel behavior and 200 retained built-ins before any source work. The
/// fixture's deterministic age/UUID order makes the two discarded rows
/// explicit, rather than letting matching implementations hide the same bug.
#[sqlx::test]
#[ignore]
async fn today_zero_sources_preserves_frozen_builtin_cap_and_retained_ids(migrator_pool: PgPool) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today frozen cap parity",
        "today-frozen-cap@example.test",
        "Alice",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let fixed_now = Utc.with_ymd_and_hms(2042, 7, 1, 12, 0, 0).unwrap();

    let fresh = Uuid::from_u128(0x5000);
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        fresh,
        Some(viewer_id),
        fixed_now - ChronoDuration::days(90),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        fresh,
        Uuid::from_u128(0x6000),
        "first-fresh",
        fixed_now - ChronoDuration::hours(2),
    )
    .await;

    for offset in 0_u128..201 {
        let person_id = Uuid::from_u128(0x5100 + offset);
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            person_id,
            Some(viewer_id),
            fixed_now - ChronoDuration::days(100),
        )
        .await;
        insert_inquiry(
            &app_pool,
            organization_id,
            person_id,
            Uuid::from_u128(0x6100 + offset),
            "stale-cap-fixture",
            fixed_now - ChronoDuration::days(2) - ChronoDuration::seconds(offset as i64),
        )
        .await;
    }

    assert_zero_enabled_sources(&app_pool, organization_id, viewer_id).await;
    let (original, current) =
        original_and_current_payloads(&app_pool, organization_id, viewer_id, fixed_now).await;
    assert_eq!(
        current, original,
        "only the declared sources envelope may differ"
    );
    assert_eq!(current["truncated"], true);

    let expected_ids = std::iter::once(fresh)
        .chain(
            (2_u128..=200)
                .rev()
                .map(|offset| Uuid::from_u128(0x5100 + offset)),
        )
        .collect::<Vec<_>>();
    assert_eq!(item_ids(&current), expected_ids);
    assert_eq!(current["items"].as_array().unwrap().len(), 200);
}

/// AC6: the fixed-clock regression includes the complete precedence chain.
/// Alice is the assignee and sees a newer client reply; Carol placed the
/// unresolved call and sees its low outcome item. The source-free baseline and
/// final query must agree for both viewers, while neither tenant sees the
/// other organization's Person.
#[sqlx::test]
#[ignore]
async fn today_zero_sources_preserves_frozen_reply_inquiry_outcome_precedence_across_viewers_and_tenants(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Today frozen precedence",
        "today-frozen-precedence-alice@example.test",
        "Alice",
        "pw",
    )
    .await;
    let carol_id = crate::common::create_user(
        &migrator_pool,
        "today-frozen-precedence-carol@example.test",
        "Carol",
        "pw",
    )
    .await;
    crate::common::add_membership(&migrator_pool, organization_id, carol_id).await;
    let (foreign_organization_id, foreign_viewer_id) =
        crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            "Today frozen precedence foreign",
            "today-frozen-precedence-foreign@example.test",
            "Foreign",
            "pw",
        )
        .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let fixed_now = Utc.with_ymd_and_hms(2042, 8, 1, 12, 0, 0).unwrap();
    let stage_id = first_stage_id(&app_pool, organization_id).await;
    let foreign_stage_id = first_stage_id(&app_pool, foreign_organization_id).await;

    let precedence_person = Uuid::from_u128(0x80_001);
    insert_person(
        &app_pool,
        organization_id,
        stage_id,
        precedence_person,
        Some(alice_id),
        fixed_now - ChronoDuration::days(20),
    )
    .await;
    insert_inquiry(
        &app_pool,
        organization_id,
        precedence_person,
        Uuid::from_u128(0x81_001),
        "old-inquiry",
        fixed_now - ChronoDuration::days(4),
    )
    .await;
    let carol_call = insert_low_outcome(
        &app_pool,
        organization_id,
        precedence_person,
        carol_id,
        fixed_now - ChronoDuration::hours(2),
    )
    .await;
    // This post-call Inquiry re-arms Alice's ordinary Inquiry branch. The
    // later inbound reply must still win its reason, freshness, and waiting
    // timestamp; Carol remains the distinct caller-owned low viewer.
    insert_inquiry(
        &app_pool,
        organization_id,
        precedence_person,
        Uuid::from_u128(0x81_003),
        "fresh-after-call",
        fixed_now - ChronoDuration::minutes(90),
    )
    .await;
    insert_inbound_correspondence(
        &migrator_pool,
        organization_id,
        precedence_person,
        alice_id,
        fixed_now - ChronoDuration::hours(1),
    )
    .await;

    let foreign_person = Uuid::from_u128(0x80_002);
    insert_person(
        &app_pool,
        foreign_organization_id,
        foreign_stage_id,
        foreign_person,
        Some(foreign_viewer_id),
        fixed_now - ChronoDuration::days(2),
    )
    .await;
    insert_inquiry(
        &app_pool,
        foreign_organization_id,
        foreign_person,
        Uuid::from_u128(0x81_002),
        "foreign-inquiry",
        fixed_now - ChronoDuration::hours(2),
    )
    .await;

    assert_zero_enabled_sources(&app_pool, organization_id, alice_id).await;
    assert_zero_enabled_sources(&app_pool, organization_id, carol_id).await;
    assert_zero_enabled_sources(&app_pool, foreign_organization_id, foreign_viewer_id).await;

    let (_, alice) =
        original_and_current_payloads(&app_pool, organization_id, alice_id, fixed_now).await;
    assert_eq!(item_ids(&alice), vec![precedence_person]);
    let alice_item = &alice["items"][0];
    assert_eq!(alice_item["priority"], "high");
    assert_eq!(alice_item["latest_inquiry"]["source"], "fresh-after-call");
    assert_eq!(
        alice_item["waiting_since"],
        json!(fixed_now - ChronoDuration::hours(1))
    );
    assert_eq!(
        alice_item["reasons"],
        json!([{
            "code": "client_replied",
            "occurred_at": fixed_now - ChronoDuration::hours(1),
        }]),
        "the reply wins the old inquiry and another user's outcome call"
    );
    assert!(!item_ids(&alice).contains(&foreign_person));

    let (_, carol) =
        original_and_current_payloads(&app_pool, organization_id, carol_id, fixed_now).await;
    assert_eq!(item_ids(&carol), vec![precedence_person]);
    let carol_item = &carol["items"][0];
    assert_eq!(carol_item["priority"], "low");
    assert_eq!(carol_item["recommended_action"], "set_outcome");
    assert_eq!(
        carol_item["reasons"],
        json!([{
            "code": "call_outcome_needed",
            "call_id": carol_call,
            "ended_at": fixed_now - ChronoDuration::hours(2),
        }]),
        "the caller differs from the assignee and owns only the outcome work"
    );
    assert!(!item_ids(&carol).contains(&foreign_person));

    let (_, foreign) = original_and_current_payloads(
        &app_pool,
        foreign_organization_id,
        foreign_viewer_id,
        fixed_now,
    )
    .await;
    assert_eq!(item_ids(&foreign), vec![foreign_person]);
    assert!(!item_ids(&foreign).contains(&precedence_person));
}

/// AC6's exact 199/200/201 regression: inquiry work retains its relative
/// order, an outcome-needed low item fills only the 199th gap, and the
/// original 201-row sentinel makes that low item the first work discarded at
/// 200 built-ins.
#[sqlx::test]
#[ignore]
async fn today_zero_sources_preserves_frozen_199_200_201_boundary_and_low_cutoff(
    migrator_pool: PgPool,
) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let fixed_now = Utc.with_ymd_and_hms(2042, 9, 1, 12, 0, 0).unwrap();

    for (case_index, builtin_count) in [(0_u128, 199_usize), (1, 200), (2, 201)] {
        let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
            &migrator_pool,
            &format!("Today frozen low cutoff {builtin_count}"),
            &format!("today-frozen-low-cutoff-{builtin_count}@example.test"),
            "Alice",
            "pw",
        )
        .await;
        let stage_id = first_stage_id(&app_pool, organization_id).await;
        let person_base = 0x90_000_u128 + case_index * 0x1_000;
        let inquiry_base = 0xA0_000_u128 + case_index * 0x1_000;
        let builtin_ids = (0..builtin_count)
            .map(|offset| Uuid::from_u128(person_base + offset as u128))
            .collect::<Vec<_>>();
        for (offset, person_id) in builtin_ids.iter().copied().enumerate() {
            insert_person(
                &app_pool,
                organization_id,
                stage_id,
                person_id,
                Some(viewer_id),
                fixed_now - ChronoDuration::days(60),
            )
            .await;
            insert_inquiry(
                &app_pool,
                organization_id,
                person_id,
                Uuid::from_u128(inquiry_base + offset as u128),
                "stale-boundary-inquiry",
                fixed_now - ChronoDuration::days(2),
            )
            .await;
        }
        let low_person = Uuid::from_u128(person_base + 0xF00);
        insert_person(
            &app_pool,
            organization_id,
            stage_id,
            low_person,
            None,
            fixed_now - ChronoDuration::days(61),
        )
        .await;
        let low_call = insert_low_outcome(
            &app_pool,
            organization_id,
            low_person,
            viewer_id,
            fixed_now - ChronoDuration::hours(1),
        )
        .await;

        assert_zero_enabled_sources(&app_pool, organization_id, viewer_id).await;
        let (_, current) =
            original_and_current_payloads(&app_pool, organization_id, viewer_id, fixed_now).await;
        let mut expected_ids = builtin_ids.into_iter().take(200).collect::<Vec<_>>();
        if builtin_count == 199 {
            expected_ids.push(low_person);
        }
        assert_eq!(item_ids(&current), expected_ids, "B={builtin_count}");
        assert_eq!(
            current["truncated"],
            builtin_count >= 200,
            "B={builtin_count}"
        );
        let low_items = current["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["person"]["id"] == low_person.to_string())
            .collect::<Vec<_>>();
        if builtin_count == 199 {
            assert_eq!(low_items.len(), 1, "B=199 retains the last low item");
            assert_eq!(low_items[0]["priority"], "low");
            assert_eq!(low_items[0]["reasons"][0]["call_id"], low_call.to_string());
        } else {
            assert!(
                low_items.is_empty(),
                "B={builtin_count} discards low work before the response cap"
            );
        }
    }
}

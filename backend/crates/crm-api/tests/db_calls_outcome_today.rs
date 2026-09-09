//! DB-backed tests for the Slice 006c §5a / D-033 "outcome needed" Today
//! tier (a caller's own ended/failed call whose outcome is not yet set),
//! over the `ScriptedProvider`. Split from a single db_calls.rs (item 3
//! of the LATER batch, docs/tasks/LATER_BATCH_2026-09-08.md, no test
//! bodies changed): the core call routes/webhook/sweep stayed in
//! db_calls.rs, and the call-outcome correction tests moved to
//! db_calls_corrections.rs. The shared telephony fixture harness
//! (`Fixture`, `fixture()`, `dial`/`hangup`/`start`, `correct`, etc.)
//! moved to tests/common/calls.rs, used by all three files. Run only via
//! ./scripts/check-db.

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::telephony::{DialOutcome, ProviderError, SipFailure};

use crate::common::calls::*;

// --- Slice 006c §5a / D-033: the "outcome needed" Today tier ----------
//
// `busy_call` (a failed call — busy → automatic `no_answer` attempt — by
// alice to `person_id`) is the one in tests/common/calls.rs, shared with
// db_calls_corrections.rs; this file used to carry its own duplicate
// definition, which Rust's glob-import shadowing rules let compile
// silently instead of erroring on the conflict.

#[sqlx::test]
#[ignore]
async fn an_ended_call_without_an_outcome_is_a_low_item_for_the_caller_only(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, dave_id).await;
    let dave = crate::common::login_cookie(&f.router, "dave@acme.test", PW).await;

    // Assigned to carol, called by alice.
    let (call_id, person_id, _) =
        answered_and_ended(&f, "lead40@example.com", Some(f.carol_id)).await;
    let call = get_call(&f.router, &f.alice, call_id).await;
    assert_eq!(call["status"], "ended");
    let ended_at = call["ended_at"].as_str().unwrap().to_string();

    // Caller: a low item with exactly the §5a shape.
    let item = today_item(&f.router, &f.alice, person_id)
        .await
        .expect("the caller carries the outcome nag");
    assert_eq!(item["priority"], "low");
    assert_eq!(item["recommended_action"], "set_outcome");
    assert_eq!(
        item["reasons"],
        json!([{ "code": "call_outcome_needed", "call_id": call_id, "ended_at": ended_at }])
    );
    assert_eq!(item["waiting_since"], ended_at);
    assert_eq!(item["latest_inquiry"]["source"], "zillow");
    assert_eq!(item["last_contact_attempt"]["outcome"], "reached");
    assert_eq!(item["last_contact_attempt"]["channel"], "call");
    assert_eq!(item["person"]["id"], person_id.to_string());
    assert_eq!(
        item["person"]["assigned_user"]["id"],
        f.carol_id.to_string()
    );
    let mut keys: Vec<&str> = item
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "last_contact_attempt",
            "latest_inquiry",
            "person",
            "priority",
            "reasons",
            "recommended_action",
            "waiting_since"
        ],
        "TodayItem gains no field"
    );
    let today = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, "/api/today", &f.alice).await,
    )
    .await;
    assert_eq!(today["truncated"], false);

    // Assignee, another member, a foreign member: never.
    assert!(!today_has(&f.router, &f.carol, person_id).await);
    assert!(!today_has(&f.router, &dave, person_id).await);
    assert!(!today_has(&f.router, &f.bob, person_id).await);

    // Choosing an outcome removes the item; a second choice keeps it gone.
    assert_eq!(
        correct(&f.router, &f.alice, call_id, "left_message")
            .await
            .status(),
        StatusCode::OK
    );
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    assert_eq!(
        correct(&f.router, &f.alice, call_id, "busy").await.status(),
        StatusCode::OK
    );
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    assert!(!today_has(&f.router, &f.carol, person_id).await);
}

#[sqlx::test]
#[ignore]
async fn low_items_sort_under_every_inquiry_item_by_ended_at(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    // Two outcome-needed calls, in order, on People assigned to nobody.
    let (p_first, phone_first, _) =
        create_person_with_phone(&f.router, &f.alice, "lead41a@example.com", None).await;
    let (p_second, phone_second, _) =
        create_person_with_phone(&f.router, &f.alice, "lead41b@example.com", None).await;
    let call_first = busy_call(&f, p_first, phone_first).await;
    let call_second = busy_call(&f, p_second, phone_second).await;
    // Then a fresh Inquiry assigned to alice: newer than both calls, still
    // listed first.
    let (p_inquiry, _, _) =
        create_person_with_phone(&f.router, &f.alice, "lead41c@example.com", Some(f.alice_id))
            .await;

    let today = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, "/api/today", &f.alice).await,
    )
    .await;
    let items = today["items"].as_array().unwrap();
    let order: Vec<(String, String)> = items
        .iter()
        .map(|i| {
            (
                i["person"]["id"].as_str().unwrap().to_string(),
                i["priority"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        order,
        vec![
            (p_inquiry.to_string(), "high".to_string()),
            (p_first.to_string(), "low".to_string()),
            (p_second.to_string(), "low".to_string()),
        ],
        "{items:?}"
    );
    assert_eq!(items[1]["reasons"][0]["call_id"], call_first.to_string());
    assert_eq!(items[2]["reasons"][0]["call_id"], call_second.to_string());
    assert_eq!(today["truncated"], false);

    // A second incomplete call to the same Person yields one item, for the
    // most recent call.
    let call_third = busy_call(&f, p_first, phone_first).await;
    let today = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, "/api/today", &f.alice).await,
    )
    .await;
    let items = today["items"].as_array().unwrap();
    let for_first: Vec<&Value> = items
        .iter()
        .filter(|i| i["person"]["id"] == p_first.to_string())
        .collect();
    assert_eq!(for_first.len(), 1);
    assert_eq!(for_first[0]["reasons"].as_array().unwrap().len(), 1);
    assert_eq!(
        for_first[0]["reasons"][0]["call_id"],
        call_third.to_string()
    );
    // ...and its `ended_at` now sorts it after p_second.
    assert_eq!(items[1]["person"]["id"], p_second.to_string());
    assert_eq!(items[2]["person"]["id"], p_first.to_string());
}

#[sqlx::test]
#[ignore]
async fn a_person_qualifying_both_ways_keeps_the_inquiry_tier_with_the_reason_appended(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let ((person_id, phone, _), digits) = create_person_with_phone_digits(
        &f.router,
        &f.alice,
        "lead42@example.com",
        Some(f.alice_id),
    )
    .await;
    let call_id = busy_call(&f, person_id, phone).await;
    assert_eq!(
        today_priority(&f.router, &f.alice, person_id)
            .await
            .as_deref(),
        Some("low")
    );
    let ended_at = get_call(&f.router, &f.alice, call_id).await["ended_at"]
        .as_str()
        .unwrap()
        .to_string();

    // A repeat Inquiry after the call: back on Today by Inquiry.
    let resp = crate::common::post_inquiry(
        &f.router,
        &f.alice,
        "realtor",
        json!({ "email": "lead42@example.com", "phone": format!("555{digits}"), "message": "again" }),
        Some(f.alice_id),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let item = today_item(&f.router, &f.alice, person_id).await.unwrap();
    assert_eq!(item["priority"], "high");
    assert_eq!(item["recommended_action"], "call");
    let codes: Vec<&str> = item["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        vec![
            "new_inquiry",
            "no_contact_attempt",
            "repeat_inquiry",
            "call_outcome_needed"
        ]
    );
    assert_eq!(item["reasons"][3]["call_id"], call_id.to_string());
    assert_eq!(item["reasons"][3]["ended_at"], ended_at);
    assert_eq!(item["waiting_since"], item["latest_inquiry"]["received_at"]);
    assert_eq!(item["latest_inquiry"]["source"], "realtor");

    // Choosing the outcome drops the reason but not the Inquiry item.
    assert_eq!(
        correct(&f.router, &f.alice, call_id, "busy").await.status(),
        StatusCode::OK
    );
    let item = today_item(&f.router, &f.alice, person_id).await.unwrap();
    assert_eq!(item["priority"], "high");
    assert_eq!(item["reasons"].as_array().unwrap().len(), 3);
    assert!(item["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["code"] != "call_outcome_needed"));
}

#[sqlx::test]
#[ignore]
async fn a_call_with_no_attempt_creates_no_outcome_item(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead43@example.com", Some(f.carol_id)).await;
    f.provider
        .push_dial(Err(ProviderError::Rejected("trunk auth".into())));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
    assert_eq!(call["failure_reason"], "provider_error");
    assert!(attempt_rows(&migrator_pool, person_id).await.is_empty());
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    // The assignee's Inquiry item is untouched (nothing reached the callee).
    assert_eq!(
        today_priority(&f.router, &f.carol, person_id)
            .await
            .as_deref(),
        Some("high")
    );
}

/// D-033: the nag is per caller. Two members who each called one Person
/// each see their own `low` item, for their own call; one's choice does
/// not clear the other's.
#[sqlx::test]
#[ignore]
async fn two_callers_on_one_person_each_carry_their_own_outcome_nag(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead44@example.com", None).await;
    let alice_call = busy_call(&f, person_id, phone).await;
    f.provider
        .push_dial(Ok(DialOutcome::Failed(SipFailure::Busy)));
    let (carol_call, _) =
        start_as_with_agent_present(&f, &f.carol, f.carol_id, person_id, phone).await;
    assert_eq!(
        dial(&f.router, &f.carol, carol_call).await.status(),
        StatusCode::ACCEPTED
    );
    wait_for_status(&f.router, &f.carol, carol_call, "failed").await;

    let alice_item = today_item(&f.router, &f.alice, person_id)
        .await
        .expect("alice's nag");
    assert_eq!(alice_item["priority"], "low");
    assert_eq!(alice_item["reasons"][0]["call_id"], alice_call.to_string());
    let carol_item = today_item(&f.router, &f.carol, person_id)
        .await
        .expect("carol's nag");
    assert_eq!(carol_item["priority"], "low");
    assert_eq!(carol_item["reasons"][0]["call_id"], carol_call.to_string());

    // Carol chooses: only carol's item clears.
    assert_eq!(
        correct(&f.router, &f.carol, carol_call, "busy")
            .await
            .status(),
        StatusCode::OK
    );
    assert!(!today_has(&f.router, &f.carol, person_id).await);
    let alice_item = today_item(&f.router, &f.alice, person_id)
        .await
        .expect("alice's nag survives carol's choice");
    assert_eq!(alice_item["reasons"][0]["call_id"], alice_call.to_string());

    // Then alice's.
    assert_eq!(
        correct(&f.router, &f.alice, alice_call, "no_answer")
            .await
            .status(),
        StatusCode::OK
    );
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    assert!(!today_has(&f.router, &f.carol, person_id).await);
}

/// §5a: the cap applies to the merged list and `low` items fall off
/// first — 200 Inquiry items plus one outcome nag: the nag is absent and
/// `truncated` is true.
#[sqlx::test]
#[ignore]
async fn a_low_item_is_the_first_to_fall_off_the_cap(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (nagged, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead45@example.com", None).await;
    let call_id = busy_call(&f, nagged, phone).await;
    assert_eq!(
        today_priority(&f.router, &f.alice, nagged).await.as_deref(),
        Some("low")
    );

    // 200 unanswered Inquiries assigned to alice (fixture rows, as
    // db_today.rs's cap test).
    let stage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1",
    )
    .bind(f.org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let base = Utc::now() - chrono::Duration::hours(48);
    for i in 0..200 {
        let person_id: Uuid = sqlx::query_scalar(
            "INSERT INTO person (organization_id, stage_id, assigned_user_id)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(f.org_id)
        .bind(stage_id)
        .bind(f.alice_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
             VALUES ($1, $2, $3, 'zillow', $4)",
        )
        .bind(f.org_id)
        .bind(person_id)
        .bind(Uuid::new_v4())
        .bind(base - chrono::Duration::seconds(i))
        .execute(&migrator_pool)
        .await
        .unwrap();
    }

    let today = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, "/api/today", &f.alice).await,
    )
    .await;
    assert_eq!(today["truncated"], true);
    let items = today["items"].as_array().unwrap();
    assert_eq!(items.len(), 200);
    assert!(
        items.iter().all(|i| i["priority"] != "low"),
        "the nag fell off"
    );
    assert!(items
        .iter()
        .all(|i| i["person"]["id"] != nagged.to_string()));
    assert!(items.iter().all(|i| i["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["call_id"] != call_id.to_string())));
}

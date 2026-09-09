//! DB-backed tests for the Slice 006c call-outcome correction route and
//! command (`correct_call_outcome`), over the `ScriptedProvider`. Split
//! from a single db_calls.rs (item 3 of the LATER batch,
//! docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies changed): the
//! core call routes/webhook/sweep stayed in db_calls.rs, and the Today
//! "low"/outcome-needed items moved to db_calls_outcome_today.rs. The
//! shared telephony fixture harness (`Fixture`, `fixture()`,
//! `dial`/`hangup`/`start`, `correct`, `attempt_rows`, etc.) moved to
//! tests/common/calls.rs, used by all three files. Run only via
//! ./scripts/check-db.

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::realtime::Publisher;
use crm_api::telephony::{DialOutcome, ProviderError, SipFailure};

use crate::common::calls::*;

// --- Slice 006c: outcome correction (docs/specs/SLICE_006c.md §13 item 2) --

fn history_of_kind<'a>(history: &'a [Value], kind: &str) -> Vec<&'a Value> {
    history.iter().filter(|e| e["kind"] == kind).collect()
}

#[sqlx::test]
#[ignore]
async fn correcting_an_answered_call_writes_one_correction_row_with_the_call_envelope(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead30@example.com", None).await;
    let (_, _, _, correlation_id) = call_row(&migrator_pool, call_id).await;
    let original = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(original.len(), 1);
    let original = &original[0];
    assert_eq!(original.outcome, "reached");
    let before = recorded(&f.publisher).await.len();

    let resp = correct(&f.router, &f.alice, call_id, "left_message").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], true);
    let attempt = &body["attempt"];
    assert_eq!(attempt.as_object().unwrap().len(), 6, "{attempt}");
    assert_eq!(attempt["channel"], "call");
    assert_eq!(attempt["outcome"], "left_message");
    assert_eq!(attempt["corrects_id"], original.id.to_string());
    assert!(attempt["recorded_at"].is_string());
    assert_eq!(body.as_object().unwrap().len(), 2);
    let correction_id: Uuid = attempt["id"].as_str().unwrap().parse().unwrap();

    // Exactly two rows: the original untouched, the correction with the
    // §2 envelope.
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let (o, c) = (&rows[0], &rows[1]);
    assert_eq!(o.id, original.id);
    assert_eq!(o.outcome, "reached");
    assert_eq!(o.recorded_at, original.recorded_at);
    assert!(o.corrects_id.is_none());
    assert_eq!(c.id, correction_id);
    assert_eq!(c.channel, "call");
    assert_eq!(c.outcome, "left_message");
    assert_eq!(c.corrects_id, Some(o.id));
    assert_eq!(c.occurred_at, o.occurred_at, "occurred_at is inherited");
    assert!(c.recorded_at > o.recorded_at, "{c:?} vs {o:?}");
    assert_eq!(c.causation_id, Some(call_id));
    assert_eq!(c.correlation_id, correlation_id);
    assert_eq!(c.actor_kind, "user");
    assert_eq!(c.actor_user_id, Some(f.alice_id));
    assert_eq!(c.origin, "web_session");
    assert_eq!(
        attempt["occurred_at"]
            .as_str()
            .unwrap()
            .parse::<chrono::DateTime<Utc>>()
            .unwrap(),
        o.occurred_at
    );
    assert_eq!(
        attempt["recorded_at"]
            .as_str()
            .unwrap()
            .parse::<chrono::DateTime<Utc>>()
            .unwrap(),
        c.recorded_at
    );
    // No call.changed; the call row itself is untouched.
    let (status, _, end_reason, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "ended");
    assert_eq!(end_reason.as_deref(), Some("agent_hangup"));

    // Exactly one person.changed{contact_attempted}, with the call's
    // correlation id, on the Organization channel.
    let events = recorded(&f.publisher).await.split_off(before);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, format!("org:{}", f.org_id));
    assert_eq!(events[0].1["type"], "person.changed");
    assert_eq!(events[0].1["data"]["change"], "contact_attempted");
    assert_eq!(events[0].1["data"]["person_id"], person_id.to_string());
    assert_eq!(events[0].1["correlation_id"], correlation_id.to_string());

    // History: original (superseded, call_id) → correction (corrects_id,
    // call_id) → call_completed; same occurred_at for the two attempts.
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person_id}"), &f.alice)
            .await,
    )
    .await;
    let history = detail["history"].as_array().unwrap();
    let kinds: Vec<&str> = history
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    let first_attempt = kinds
        .iter()
        .position(|k| *k == "contact_attempted")
        .unwrap();
    assert_eq!(
        &kinds[first_attempt..],
        &["contact_attempted", "call_completed", "contact_attempted"],
        "a correction sits after the call it corrects: {kinds:?}"
    );
    let attempts = history_of_kind(history, "contact_attempted");
    assert_eq!(attempts[0]["id"], o.id.to_string());
    assert_eq!(
        attempts[0]["detail"],
        json!({
            "channel": "call",
            "outcome": "reached",
            "call_id": call_id,
            "corrects_id": null,
            "superseded": true,
        })
    );
    assert_eq!(attempts[1]["id"], c.id.to_string());
    assert_eq!(
        attempts[1]["detail"],
        json!({
            "channel": "call",
            "outcome": "left_message",
            "call_id": call_id,
            "corrects_id": o.id,
            "superseded": false,
        })
    );
    assert_eq!(attempts[1]["actor"]["id"], f.alice_id.to_string());
    assert_eq!(attempts[1]["correlation_id"], correlation_id.to_string());
    // History shows the correction at the moment it was made (its
    // recorded_at), after the call; the stored fact keeps the attempt's
    // occurred_at (asserted on the rows above).
    assert!(
        attempts[1]["occurred_at"].as_str().unwrap() > attempts[0]["occurred_at"].as_str().unwrap()
    );
    assert_eq!(attempts[1]["occurred_at"], attempts[1]["recorded_at"]);
}

#[sqlx::test]
#[ignore]
async fn same_outcome_is_unchanged_and_writes_and_publishes_nothing(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead31@example.com", None).await;
    let original = attempt_rows(&migrator_pool, person_id).await;

    // Start from an agent choice (D-033: the automatic root is never the
    // "same outcome" — see `choosing_the_observed_outcome_still_writes…`).
    let resp = correct(&f.router, &f.alice, call_id, "busy").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let correction_id = crate::common::body_json(resp).await["attempt"]["id"].clone();
    assert_eq!(attempt_rows(&migrator_pool, person_id).await.len(), 2);
    let before = recorded(&f.publisher).await.len();

    // Re-saving the chosen outcome is the no-op and returns the choice as
    // the head: nothing written, nothing published.
    let resp = correct(&f.router, &f.alice, call_id, "busy").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], false);
    assert_eq!(body["attempt"]["id"], correction_id);
    assert_eq!(body["attempt"]["outcome"], "busy");
    assert_eq!(body["attempt"]["corrects_id"], original[0].id.to_string());
    assert_eq!(attempt_rows(&migrator_pool, person_id).await.len(), 2);
    assert_eq!(recorded(&f.publisher).await.len(), before);
}

/// D-033: the automatic row is evidence, not an outcome. Choosing the
/// value the system observed still writes the agent's row (so the call is
/// complete and the "outcome needed" nag clears); only a repeat of an
/// agent choice is `changed: false`.
#[sqlx::test]
#[ignore]
async fn choosing_the_observed_outcome_still_writes_the_agents_row_and_clears_the_nag(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;

    // `reached` on an answered call.
    let (call_id, person_id, _) = answered_and_ended(&f, "lead31a@example.com", None).await;
    let original = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].outcome, "reached");
    assert_eq!(
        today_priority(&f.router, &f.alice, person_id)
            .await
            .as_deref(),
        Some("low")
    );
    let before = recorded(&f.publisher).await.len();

    let resp = correct(&f.router, &f.alice, call_id, "reached").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], true);
    assert_eq!(body["attempt"]["outcome"], "reached");
    assert_eq!(body["attempt"]["corrects_id"], original[0].id.to_string());
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].id, original[0].id);
    assert_eq!(rows[1].outcome, "reached");
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    assert_eq!(rows[1].actor_user_id, Some(f.alice_id));
    assert_eq!(body["attempt"]["id"], rows[1].id.to_string());
    assert_eq!(recorded(&f.publisher).await.len(), before + 1);
    // The Today `low` item is gone.
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    // History: the root is superseded, the agent's row is the chosen one.
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person_id}"), &f.alice)
            .await,
    )
    .await;
    let attempts = history_of_kind(detail["history"].as_array().unwrap(), "contact_attempted");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["id"], rows[0].id.to_string());
    assert_eq!(attempts[0]["detail"]["superseded"], true);
    assert_eq!(attempts[1]["id"], rows[1].id.to_string());
    assert_eq!(attempts[1]["detail"]["outcome"], "reached");
    assert_eq!(attempts[1]["detail"]["corrects_id"], rows[0].id.to_string());
    assert_eq!(attempts[1]["detail"]["superseded"], false);

    // A repeat of the agent's choice is the no-op.
    let before = recorded(&f.publisher).await.len();
    let resp = correct(&f.router, &f.alice, call_id, "reached").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], false);
    assert_eq!(body["attempt"]["id"], rows[1].id.to_string());
    assert_eq!(attempt_rows(&migrator_pool, person_id).await.len(), 2);
    assert_eq!(recorded(&f.publisher).await.len(), before);

    // `no_answer` on a busy call (observed `no_answer`, D-031 mapping).
    let (person2, phone2, _) =
        create_person_with_phone(&f.router, &f.alice, "lead31b@example.com", None).await;
    let call2 = busy_call(&f, person2, phone2).await;
    assert_eq!(
        today_priority(&f.router, &f.alice, person2)
            .await
            .as_deref(),
        Some("low")
    );
    let resp = correct(&f.router, &f.alice, call2, "no_answer").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], true);
    let rows = attempt_rows(&migrator_pool, person2).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].outcome, "no_answer");
    assert!(rows[0].corrects_id.is_none());
    assert_eq!(rows[1].outcome, "no_answer");
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    assert_eq!(body["attempt"]["id"], rows[1].id.to_string());
    assert!(!today_has(&f.router, &f.alice, person2).await);
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person2}"), &f.alice)
            .await,
    )
    .await;
    let attempts = history_of_kind(detail["history"].as_array().unwrap(), "contact_attempted");
    assert_eq!(attempts[1]["id"], rows[1].id.to_string());
    assert_eq!(attempts[1]["detail"]["superseded"], false);
    let body =
        crate::common::body_json(correct(&f.router, &f.alice, call2, "no_answer").await).await;
    assert_eq!(body["changed"], false);
    assert_eq!(attempt_rows(&migrator_pool, person2).await.len(), 2);
}

#[sqlx::test]
#[ignore]
async fn a_second_correction_chains_onto_the_first_with_strictly_increasing_recorded_at(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead32@example.com", None).await;

    let first =
        crate::common::body_json(correct(&f.router, &f.alice, call_id, "left_message").await).await;
    let second =
        crate::common::body_json(correct(&f.router, &f.alice, call_id, "wrong_number").await).await;
    assert_eq!(second["changed"], true);
    assert_eq!(second["attempt"]["corrects_id"], first["attempt"]["id"]);

    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].corrects_id, None);
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    assert_eq!(rows[2].corrects_id, Some(rows[1].id));
    assert_eq!(rows[2].outcome, "wrong_number");
    // LATER (011d): under full-suite load two sequential writes can land in
    // the same microsecond `recorded_at` (`clock_timestamp()` resolution),
    // which used to fail this assertion even though row order stayed
    // correct — `attempt_rows`'s own `ORDER BY recorded_at, id` (and the
    // production history sort, `person/queries.rs`'s `(occurred_at,
    // recorded_at, kind_rank, id)`, docs/specs/SLICE_002.md §5) already
    // tie-break on `id`; the assertion just wasn't checking the same key
    // it was sorted by. Compare the compound key so a `recorded_at` tie
    // does not fail a still-deterministic order.
    assert!((rows[0].recorded_at, rows[0].id) < (rows[1].recorded_at, rows[1].id));
    assert!((rows[1].recorded_at, rows[1].id) < (rows[2].recorded_at, rows[2].id));
    assert!(rows.iter().all(|r| r.occurred_at == rows[0].occurred_at));
    assert!(rows.iter().all(|r| r.causation_id == Some(call_id)));

    // The head lookup returns the second correction (a no-op save of the
    // same value echoes it).
    let head =
        crate::common::body_json(correct(&f.router, &f.alice, call_id, "wrong_number").await).await;
    assert_eq!(head["changed"], false);
    assert_eq!(head["attempt"]["id"], rows[2].id.to_string());

    // History: every attempt but the head is superseded; order is
    // original, first, second, then call_completed.
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person_id}"), &f.alice)
            .await,
    )
    .await;
    let history = detail["history"].as_array().unwrap();
    let attempts = history_of_kind(history, "contact_attempted");
    let ids: Vec<String> = attempts
        .iter()
        .map(|a| a["id"].as_str().unwrap().into())
        .collect();
    assert_eq!(
        ids,
        rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>()
    );
    let superseded: Vec<bool> = attempts
        .iter()
        .map(|a| a["detail"]["superseded"].as_bool().unwrap())
        .collect();
    assert_eq!(superseded, vec![true, true, false]);
    // The original attempt precedes the call's end; every correction follows it.
    let kinds: Vec<&str> = history
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    let end = kinds.iter().position(|k| *k == "call_completed").unwrap();
    assert_eq!(kinds[end - 1], "contact_attempted");
    assert_eq!(
        &kinds[end + 1..],
        &["contact_attempted", "contact_attempted"]
    );
}

#[sqlx::test]
#[ignore]
async fn two_concurrent_corrections_serialize_on_the_call_lock_into_a_chain(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead33@example.com", None).await;

    let (a, b) = tokio::join!(
        correct(&f.router, &f.alice, call_id, "busy"),
        correct(&f.router, &f.alice, call_id, "wrong_number"),
    );
    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);
    let a = crate::common::body_json(a).await;
    let b = crate::common::body_json(b).await;
    assert_eq!(a["changed"], true);
    assert_eq!(b["changed"], true);

    // One original plus two corrections forming a linear chain: never two
    // corrections of one head.
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0].corrects_id, None);
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    assert_eq!(rows[2].corrects_id, Some(rows[1].id));
    assert!(rows[1].recorded_at < rows[2].recorded_at);
    let later = if a["attempt"]["corrects_id"] == b["attempt"]["id"] {
        &a
    } else {
        assert_eq!(b["attempt"]["corrects_id"], a["attempt"]["id"]);
        &b
    };
    assert_eq!(later["attempt"]["id"], rows[2].id.to_string());
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_attempted WHERE corrects_id = $1")
            .bind(rows[0].id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
#[ignore]
async fn correction_is_caller_only_foreign_404_active_409_and_no_attempt_422(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, phone) = answered_and_ended(&f, "lead34@example.com", None).await;

    // Not the caller → 403; other Organization → 404; and the other
    // direction: alice on bob's call → 404.
    let resp = correct(&f.router, &f.carol, call_id, "left_message").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(crate::common::body_json(resp).await["error"], "forbidden");
    let resp = correct(&f.router, &f.bob, call_id, "left_message").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let foreign_body = resp.into_body().collect_bytes().await;
    let resp = correct(&f.router, &f.alice, Uuid::new_v4(), "left_message").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(resp.into_body().collect_bytes().await, foreign_body);
    let (bob_person, bob_phone, _) =
        create_person_with_phone(&f.router, &f.bob, "lead34b@example.com", None).await;
    let resp = start(&f.router, &f.bob, bob_person, bob_phone).await;
    let bob_call: Uuid = crate::common::body_json(resp).await["call"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let resp = correct(&f.router, &f.alice, bob_call, "left_message").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // `sent` and unknown fields are 400 before any command runs.
    let resp = correct(&f.router, &f.alice, call_id, "sent").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "malformed_request"
    );
    let resp = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/calls/{call_id}/outcome"),
        &f.alice,
        json!({ "outcome": "busy", "note": "x" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(attempt_rows(&migrator_pool, person_id).await.len(), 1);

    // An active (answered) call → 409 invalid_call_state.
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (active, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, active).await;
    wait_for_status(&f.router, &f.alice, active, "answered").await;
    let resp = correct(&f.router, &f.alice, active, "left_message").await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "invalid_call_state"
    );
    hangup(&f.router, &f.alice, active).await;

    // A call where nothing reached the callee → 422 no_contact_attempt.
    let (person2, phone2, _) =
        create_person_with_phone(&f.router, &f.alice, "lead34c@example.com", None).await;
    f.provider
        .push_dial(Err(ProviderError::Rejected("trunk auth".into())));
    let (failed, _) = start_with_agent_present(&f, person2, phone2).await;
    dial(&f.router, &f.alice, failed).await;
    let call = wait_for_status(&f.router, &f.alice, failed, "failed").await;
    assert_eq!(call["failure_reason"], "provider_error");
    let resp = correct(&f.router, &f.alice, failed, "left_message").await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "no_contact_attempt"
    );
    assert!(attempt_rows(&migrator_pool, person2).await.is_empty());
}

#[sqlx::test]
#[ignore]
async fn today_shows_the_effective_outcome_for_a_member_who_has_the_person_by_assignment(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    // Dave: a third member, neither caller nor carol.
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, dave_id).await;
    let dave = crate::common::login_cookie(&f.router, "dave@acme.test", PW).await;

    let ((person_id, phone, _), digits) =
        create_person_with_phone_digits(&f.router, &f.alice, "lead35@example.com", Some(dave_id))
            .await;
    assert!(today_has(&f.router, &dave, person_id).await);
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;
    hangup(&f.router, &f.alice, call_id).await;
    // Membership: the attempt answered the Inquiry for everyone; only the
    // caller carries the D-033 outcome nag.
    assert!(!today_has(&f.router, &dave, person_id).await);
    assert_eq!(
        today_priority(&f.router, &f.alice, person_id)
            .await
            .as_deref(),
        Some("low")
    );
    assert!(!today_has(&f.router, &f.carol, person_id).await);

    assert_eq!(
        correct(&f.router, &f.alice, call_id, "left_message")
            .await
            .status(),
        StatusCode::OK
    );
    // Membership unchanged by the correction.
    assert!(!today_has(&f.router, &dave, person_id).await);
    assert!(!today_has(&f.router, &f.alice, person_id).await);
    assert!(!today_has(&f.router, &f.carol, person_id).await);

    // A repeat Inquiry puts the Person back on dave's Today, whose
    // `last_contact_attempt` is the effective (corrected) row.
    let resp = crate::common::post_inquiry(
        &f.router,
        &f.alice,
        "zillow",
        json!({ "email": "lead35@example.com", "phone": format!("555{digits}"), "message": "again" }),
        Some(dave_id),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(
        crate::common::body_json(resp).await["person_id"],
        person_id.to_string()
    );
    let body = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, "/api/today", &dave).await,
    )
    .await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["person"]["id"] == person_id.to_string())
        .expect("the repeat Inquiry is on dave's Today");
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(item["last_contact_attempt"]["outcome"], "left_message");
    assert_eq!(item["last_contact_attempt"]["id"], rows[1].id.to_string());
    assert_eq!(item["last_contact_attempt"]["channel"], "call");
    assert_eq!(
        item["last_contact_attempt"].as_object().unwrap().len(),
        4,
        "ContactAttemptRef is unchanged"
    );
    assert!(!today_has(&f.router, &f.alice, person_id).await);
}

#[sqlx::test]
#[ignore]
async fn a_correction_row_is_append_only_for_crm_app_and_the_owner(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead36@example.com", None).await;
    let body = crate::common::body_json(correct(&f.router, &f.alice, call_id, "busy").await).await;
    let correction_id: Uuid = body["attempt"]["id"].as_str().unwrap().parse().unwrap();

    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    for (label, pool) in [("crm_app", &app_pool), ("owner", &migrator_pool)] {
        let update = sqlx::query("UPDATE contact_attempted SET outcome = 'reached' WHERE id = $1")
            .bind(correction_id)
            .execute(pool)
            .await;
        assert!(update.is_err(), "{label}: UPDATE must be rejected");
        let unlink = sqlx::query("UPDATE contact_attempted SET corrects_id = NULL WHERE id = $1")
            .bind(correction_id)
            .execute(pool)
            .await;
        assert!(
            unlink.is_err(),
            "{label}: UPDATE corrects_id must be rejected"
        );
        let delete = sqlx::query("DELETE FROM contact_attempted WHERE id = $1")
            .bind(correction_id)
            .execute(pool)
            .await;
        assert!(delete.is_err(), "{label}: DELETE must be rejected");
    }
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].outcome, "busy");
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
}

#[sqlx::test]
#[ignore]
async fn the_outcome_route_works_with_telephony_disabled(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (call_id, person_id, _) = answered_and_ended(&f, "lead37@example.com", None).await;

    let publisher = Publisher::recording();
    let disabled = build_router_with_telephony(&migrator_pool, publisher.clone(), None).await;
    let cookie = crate::common::login_cookie(&disabled, "alice@acme.test", PW).await;
    let resp = correct(&disabled, &cookie, call_id, "no_answer").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], true);
    assert_eq!(body["attempt"]["outcome"], "no_answer");
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["type"], "person.changed");
    assert_eq!(events[0].1["data"]["change"], "contact_attempted");
}

#[sqlx::test]
#[ignore]
async fn a_manual_attempt_has_no_call_id_and_is_never_a_correction_head(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, _, _) =
        create_person_with_phone(&f.router, &f.alice, "lead38@example.com", None).await;
    let resp = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/contact-attempts"),
        &f.alice,
        json!({ "channel": "call", "outcome": "wrong_number" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person_id}"), &f.alice)
            .await,
    )
    .await;
    let attempts = history_of_kind(detail["history"].as_array().unwrap(), "contact_attempted");
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0]["detail"],
        json!({
            "channel": "call",
            "outcome": "wrong_number",
            "call_id": null,
            "corrects_id": null,
            "superseded": false,
        })
    );
}

#[sqlx::test]
#[ignore]
async fn busy_call_correction_chains_onto_the_no_answer_row_and_cancelled_has_nothing_to_correct(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead39@example.com", None).await;
    f.provider
        .push_dial(Ok(DialOutcome::Failed(SipFailure::Busy)));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    let call = wait_for_status(&f.router, &f.alice, call_id, "failed").await;
    assert_eq!(call["failure_reason"], "busy");

    let resp = correct(&f.router, &f.alice, call_id, "busy").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["changed"], true);
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].outcome, "no_answer", "D-031 mapping untouched");
    assert_eq!(rows[1].outcome, "busy");
    assert_eq!(rows[1].corrects_id, Some(rows[0].id));
    assert_eq!(body["attempt"]["corrects_id"], rows[0].id.to_string());

    // The automatic attempt and call_completed share both timestamps, so
    // kind_rank orders them and the correction (later recorded_at) sorts
    // after call_completed — adjacency is not guaranteed (SLICE_006c §2).
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&f.router, &format!("/api/people/{person_id}"), &f.alice)
            .await,
    )
    .await;
    let history = detail["history"].as_array().unwrap();
    let kinds: Vec<&str> = history
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    let first = kinds
        .iter()
        .position(|k| *k == "contact_attempted")
        .unwrap();
    assert_eq!(
        &kinds[first..],
        &["contact_attempted", "call_completed", "contact_attempted"],
        "{kinds:?}"
    );
    let attempts = history_of_kind(history, "contact_attempted");
    assert_eq!(attempts[0]["id"], rows[0].id.to_string());
    assert_eq!(attempts[0]["detail"]["superseded"], true);
    assert_eq!(attempts[1]["id"], rows[1].id.to_string());
    assert_eq!(attempts[1]["detail"]["corrects_id"], rows[0].id.to_string());

    // placing → cancelled (hangup before ringing): no attempt → 422.
    let (person2, phone2, _) =
        create_person_with_phone(&f.router, &f.alice, "lead39b@example.com", None).await;
    let resp = start(&f.router, &f.alice, person2, phone2).await;
    let cancelled: Uuid = crate::common::body_json(resp).await["call"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let resp = hangup(&f.router, &f.alice, cancelled).await;
    assert_eq!(
        crate::common::body_json(resp).await["call"]["failure_reason"],
        "cancelled"
    );
    let resp = correct(&f.router, &f.alice, cancelled, "busy").await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        crate::common::body_json(resp).await["error"],
        "no_contact_attempt"
    );
    assert!(attempt_rows(&migrator_pool, person2).await.is_empty());
}

#[sqlx::test]
#[ignore]
async fn a_non_caller_on_an_active_call_is_403_not_409(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead40@example.com", None).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    let resp = correct(&f.router, &f.carol, call_id, "left_message").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(crate::common::body_json(resp).await["error"], "forbidden");
    assert_eq!(attempt_rows(&migrator_pool, person_id).await.len(), 1);
    hangup(&f.router, &f.alice, call_id).await;
}

#[sqlx::test]
#[ignore]
async fn concurrent_hangup_and_correction_yield_at_most_one_correction_of_the_reached_row(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let (person_id, phone, _) =
        create_person_with_phone(&f.router, &f.alice, "lead41@example.com", None).await;
    f.provider
        .push_dial(Ok(DialOutcome::Answered { call_ref: None }));
    let (call_id, _) = start_with_agent_present(&f, person_id, phone).await;
    dial(&f.router, &f.alice, call_id).await;
    wait_for_status(&f.router, &f.alice, call_id, "answered").await;

    let (h, c) = tokio::join!(
        hangup(&f.router, &f.alice, call_id),
        correct(&f.router, &f.alice, call_id, "left_message"),
    );
    assert_eq!(h.status(), StatusCode::OK);
    let rows = attempt_rows(&migrator_pool, person_id).await;
    assert_eq!(rows[0].outcome, "reached");
    match c.status() {
        StatusCode::CONFLICT => {
            assert_eq!(
                crate::common::body_json(c).await["error"],
                "invalid_call_state"
            );
            assert_eq!(rows.len(), 1);
        }
        StatusCode::OK => {
            let body = crate::common::body_json(c).await;
            assert_eq!(body["changed"], true);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1].corrects_id, Some(rows[0].id));
            assert_eq!(body["attempt"]["corrects_id"], rows[0].id.to_string());
        }
        other => panic!("unexpected status {other}"),
    }
    let (status, _, _, _) = call_row(&migrator_pool, call_id).await;
    assert_eq!(status, "ended");
}

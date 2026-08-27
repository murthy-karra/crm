//! DB-backed tests for Slice 008's round-robin routing mode
//! (docs/specs/SLICE_008.md §7): fairness, mid-rotation deactivation
//! continuity, newcomers, the empty-pool fail-safe, that only an actual
//! round-robin assignment ever advances the pointer, duplicate-replay
//! decoding, ~8-way concurrency, and cross-Organization isolation. Mirrors
//! `db_intake_system_routing.rs`'s pattern (007c): the primary proof is
//! calling `commands::receive_inquiry` directly with `IntakeActor::System`
//! — the same one change point every entry path funnels through. Run only
//! via ./scripts/check-db.
mod common;

use std::collections::HashMap;

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::commands::{self, ReceiveInquiry, ReceiveInquiryOutcome, RoutingStrategy};
use crm_api::domain::envelope::Origin;
use crm_api::domain::inquiry::parse::Source;
use crm_api::domain::intake::{IntakeActor, IntakeRoutingMode};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "pw";

fn lead(email: &str) -> Value {
    json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "email": email,
        "message": "Interested in the listing",
    })
}

async fn system_intake(
    pool: &PgPool,
    org_id: Uuid,
    payload: &Value,
    publisher: &Publisher,
) -> ReceiveInquiryOutcome {
    let key = common::test_config().raw_payload_key;
    let actor = IntakeActor::System {
        on_behalf_of_user_id: None,
        organization_id: OrganizationId::new(org_id),
        origin: Origin::Cli,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let cmd = ReceiveInquiry {
        source: Source::parse("website").unwrap(),
        payload: serde_json::to_vec(payload).unwrap(),
        assign_to_user_id: None,
        received_at: chrono::Utc::now(),
    };
    commands::receive_inquiry(pool, &key, publisher, &actor, cmd)
        .await
        .unwrap()
}

fn assigned(outcome: ReceiveInquiryOutcome) -> Option<UserId> {
    match outcome {
        ReceiveInquiryOutcome::Resolved {
            assigned_user_id, ..
        } => assigned_user_id,
        ReceiveInquiryOutcome::Unresolved { .. } => panic!("expected Resolved, got Unresolved"),
    }
}

async fn set_round_robin(pool: &PgPool, org_id: Uuid) {
    admin_queries::update_intake_routing_settings(
        &mut pool.acquire().await.unwrap(),
        OrganizationId::new(org_id),
        IntakeRoutingMode::RoundRobin,
        None,
    )
    .await
    .unwrap();
}

/// The one sanctioned migrator write for test fixtures (SLICE_004 §11):
/// backdate a membership's `created_at` so join order is deterministic
/// rather than relying on real insert timing (which could theoretically
/// tie at Postgres's clock resolution). Mirrors `db_calls.rs`'s
/// `backdate` helper.
async fn backdate_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid, seconds_ago: i64) {
    sqlx::query(
        "UPDATE organization_membership SET created_at = now() - make_interval(secs => $3)
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(seconds_ago as f64)
    .execute(pool)
    .await
    .unwrap();
}

/// Same status-flip fixture pattern `db_intake_system_routing.rs`'s
/// criterion-6 test already uses (raw SQL via the migrator connection).
async fn set_status(pool: &PgPool, org_id: Uuid, user_id: Uuid, status: &str) {
    sqlx::query(
        "UPDATE organization_membership SET status = $3
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn stored_pointer(pool: &PgPool, org_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar(
        "SELECT last_assigned_user_id FROM intake_rotation WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Fairness: a, b, c, a, b, c across 6 system intakes (join order fixed
/// by backdating), with full fact and Today assertions per intake. The
/// first draw (anchor `None`) doubles as the "first-rotation start"
/// criterion.
#[sqlx::test]
#[ignore]
async fn fairness_rotates_a_b_c_a_b_c_with_full_fact_and_today_assertions(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    let c = common::create_user(&migrator_pool, "c@acme.test", "Cid", PW).await;
    common::add_membership(&migrator_pool, org_id, a).await;
    common::add_membership(&migrator_pool, org_id, b).await;
    common::add_membership(&migrator_pool, org_id, c).await;
    backdate_membership(&migrator_pool, org_id, a, 3 * 3600).await;
    backdate_membership(&migrator_pool, org_id, b, 2 * 3600).await;
    backdate_membership(&migrator_pool, org_id, c, 3600).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let expected = [a, b, c, a, b, c];

    for (i, expected_user) in expected.iter().enumerate() {
        let outcome = system_intake(
            &app_pool,
            org_id,
            &lead(&format!("lead{i}@example.com")),
            &publisher,
        )
        .await;
        let ReceiveInquiryOutcome::Resolved {
            person_id,
            routing_strategy,
            assigned_user_id,
            inquiry_id,
            ..
        } = outcome
        else {
            panic!("expected Resolved");
        };
        assert_eq!(routing_strategy, RoutingStrategy::RoundRobin, "intake {i}");
        assert_eq!(
            assigned_user_id,
            Some(UserId::new(*expected_user)),
            "intake {i}"
        );

        let (strategy, fact_assignee): (String, Option<Uuid>) = sqlx::query_as(
            "SELECT strategy, assignee_user_id FROM routing_decision WHERE inquiry_id = $1",
        )
        .bind(inquiry_id.0)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(strategy, "round_robin", "intake {i}");
        assert_eq!(fact_assignee, Some(*expected_user), "intake {i}");

        let (routing_decision_id,): (Uuid,) =
            sqlx::query_as("SELECT id FROM routing_decision WHERE inquiry_id = $1")
                .bind(inquiry_id.0)
                .fetch_one(&migrator_pool)
                .await
                .unwrap();
        let (from_user_id, to_user_id, causation_id): (Option<Uuid>, Option<Uuid>, Option<Uuid>) =
            sqlx::query_as(
                "SELECT from_user_id, to_user_id, causation_id FROM assignment_changed
             WHERE person_id = $1",
            )
            .bind(person_id.0)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
        assert_eq!(from_user_id, None, "intake {i}");
        assert_eq!(to_user_id, Some(*expected_user), "intake {i}");
        assert_eq!(causation_id, Some(routing_decision_id), "intake {i}");

        assert_eq!(
            stored_pointer(&migrator_pool, org_id).await,
            Some(*expected_user),
            "pointer after intake {i}"
        );
    }

    // Today: each member ends up with exactly the two People routed to
    // them, and nobody else's.
    let router = common::build_router(&migrator_pool).await;
    for (user_id, email) in [(a, "a@acme.test"), (b, "b@acme.test"), (c, "c@acme.test")] {
        let cookie = common::login_cookie(&router, email, PW).await;
        let today =
            common::body_json(common::get_with_cookie(&router, "/api/today", &cookie).await).await;
        let items = today["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{email}'s Today");
        for item in items {
            assert_eq!(
                item["person"]["assigned_user"]["id"],
                user_id.to_string(),
                "{email}'s Today item"
            );
        }
    }
}

/// Mid-rotation deactivation continues the cycle rather than resetting —
/// both when the deactivated member is NOT the current pointer (b, while
/// a is the pointer) and when it IS the current pointer (c, once c
/// becomes the pointer).
#[sqlx::test]
#[ignore]
async fn mid_rotation_deactivation_continues_without_resetting_pointer_or_non_pointer(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    let c = common::create_user(&migrator_pool, "c@acme.test", "Cid", PW).await;
    common::add_membership(&migrator_pool, org_id, a).await;
    common::add_membership(&migrator_pool, org_id, b).await;
    common::add_membership(&migrator_pool, org_id, c).await;
    backdate_membership(&migrator_pool, org_id, a, 3 * 3600).await;
    backdate_membership(&migrator_pool, org_id, b, 2 * 3600).await;
    backdate_membership(&migrator_pool, org_id, c, 3600).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // First intake -> a (anchor None, first-rotation start); pointer = a.
    let outcome = system_intake(&app_pool, org_id, &lead("l0@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));

    // Deactivate b — the next member in line, NOT the current pointer's
    // own member.
    set_status(&migrator_pool, org_id, b, "inactive").await;

    // Continues past a, skips deactivated b, lands on c — not a reset to
    // the front.
    let outcome = system_intake(&app_pool, org_id, &lead("l1@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(c)));

    // Now deactivate c — this time the member the pointer itself just
    // named.
    set_status(&migrator_pool, org_id, c, "inactive").await;

    // Only a remains active: the cycle continues to a (not an error, not
    // a "no rotation happened" no-op).
    let outcome = system_intake(&app_pool, org_id, &lead("l2@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));
}

/// A newcomer's `created_at` is later than every existing member, so they
/// join at the END of the cycle, not immediately after whoever happens to
/// be the current pointer.
#[sqlx::test]
#[ignore]
async fn a_newcomer_joins_at_the_end_of_the_cycle(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    common::add_membership(&migrator_pool, org_id, a).await;
    common::add_membership(&migrator_pool, org_id, b).await;
    backdate_membership(&migrator_pool, org_id, a, 2 * 3600).await;
    backdate_membership(&migrator_pool, org_id, b, 3600).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let outcome = system_intake(&app_pool, org_id, &lead("l0@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));
    let outcome = system_intake(&app_pool, org_id, &lead("l1@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(b)));

    let newcomer = common::create_user(&migrator_pool, "newcomer@acme.test", "Newt", PW).await;
    common::add_membership(&migrator_pool, org_id, newcomer).await;

    // Next after b is the newcomer, at the end — not a or a mid-cycle
    // insertion.
    let outcome = system_intake(&app_pool, org_id, &lead("l2@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(newcomer)));

    // And the cycle wraps back to a.
    let outcome = system_intake(&app_pool, org_id, &lead("l3@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));
}

/// Empty pool (round_robin mode, zero active members): routes
/// `unassigned`, never errors, and writes no pointer row at all.
#[sqlx::test]
#[ignore]
async fn empty_pool_routes_unassigned_with_no_pointer_write(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let outcome = system_intake(
        &app_pool,
        org_id,
        &lead("l0@example.com"),
        &Publisher::recording(),
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        routing_strategy,
        assigned_user_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(routing_strategy, RoutingStrategy::Unassigned);
    assert_eq!(assigned_user_id, None);
    assert_eq!(
        stored_pointer(&migrator_pool, org_id).await,
        None,
        "no pointer write on an empty pool"
    );
}

/// The pointer advances ONLY on an actual round-robin assignment — never
/// on `kept_existing`, `explicit`, or `actor_default`, even though the
/// Organization is in `round_robin` mode throughout.
#[sqlx::test]
#[ignore]
async fn non_round_robin_strategies_never_advance_the_pointer(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    common::add_membership_with(
        &migrator_pool,
        org_id,
        a,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    common::add_membership(&migrator_pool, org_id, b).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let key = common::test_config().raw_payload_key;

    // explicit: a System-actor intake WITH assign_to_user_id set.
    let cmd_actor = IntakeActor::System {
        organization_id: OrganizationId::new(org_id),
        origin: Origin::Cli,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
        on_behalf_of_user_id: None,
    };
    let cmd = ReceiveInquiry {
        source: Source::parse("website").unwrap(),
        payload: serde_json::to_vec(&lead("explicit@example.com")).unwrap(),
        assign_to_user_id: Some(UserId::new(b)),
        received_at: chrono::Utc::now(),
    };
    let outcome = commands::receive_inquiry(&app_pool, &key, &publisher, &cmd_actor, cmd)
        .await
        .unwrap();
    let ReceiveInquiryOutcome::Resolved {
        routing_strategy,
        assigned_user_id,
        person_id: first_person_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(routing_strategy, RoutingStrategy::Explicit);
    assert_eq!(assigned_user_id, Some(UserId::new(b)));
    assert_eq!(
        stored_pointer(&migrator_pool, org_id).await,
        None,
        "explicit assignment must not consume a rotation turn"
    );

    // kept_existing: a second system intake matching the SAME Person (same
    // email, but a genuinely different payload — a byte-identical repeat
    // of the first call's exact bytes would hit Phase A's duplicate-
    // delivery short-circuit instead of exercising `kept_existing` at all,
    // same pitfall `db_intake_system_routing.rs`'s own kept_existing test
    // avoids the same way) keeps b, still no pointer write.
    let outcome = system_intake(
        &app_pool,
        org_id,
        &json!({
            "first_name": "Ex",
            "last_name": "Plicit",
            "email": "explicit@example.com",
            "message": "Following up",
        }),
        &publisher,
    )
    .await;
    let ReceiveInquiryOutcome::Resolved {
        routing_strategy,
        assigned_user_id,
        person_id,
        ..
    } = outcome
    else {
        panic!("expected Resolved");
    };
    assert_eq!(routing_strategy, RoutingStrategy::KeptExisting);
    assert_eq!(assigned_user_id, Some(UserId::new(b)));
    assert_eq!(person_id, first_person_id);
    assert_eq!(
        stored_pointer(&migrator_pool, org_id).await,
        None,
        "kept_existing must not consume a rotation turn"
    );

    // actor_default: a User-actor intake (via HTTP, brand-new Person) —
    // matrix step 3 returns before mode is ever consulted.
    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "a@acme.test", PW).await;
    let resp = common::post_inquiry(
        &router,
        &cookie,
        "website",
        lead("useractor@example.com"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = common::body_json(resp).await;
    assert_eq!(body["routing_strategy"], "actor_default");
    assert_eq!(body["assigned_user_id"], a.to_string());
    assert_eq!(
        stored_pointer(&migrator_pool, org_id).await,
        None,
        "actor_default must not consume a rotation turn"
    );
}

/// A byte-for-byte re-POST of a round-robin-routed payload via the
/// user-actor `POST /api/inquiries` endpoint decodes `round_robin` in the
/// `duplicate: true` replay — pins the `RoutingStrategy::from_str`
/// extension (no 500).
#[sqlx::test]
#[ignore]
async fn duplicate_repost_of_a_round_robin_routed_payload_reports_its_strategy(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    common::add_membership_with(
        &migrator_pool,
        org_id,
        a,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    // Adversarial M3: a SECOND member, so "the replay advanced the
    // pointer" and "the pointer stayed" are distinguishable states.
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    common::add_membership(&migrator_pool, org_id, b).await;
    backdate_membership(&migrator_pool, org_id, a, 2 * 3600).await;
    backdate_membership(&migrator_pool, org_id, b, 3600).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let payload = lead("dan@example.com");
    let outcome = system_intake(&app_pool, org_id, &payload, &Publisher::recording()).await;
    assert!(matches!(outcome, ReceiveInquiryOutcome::Resolved { .. }));
    assert_eq!(stored_pointer(&migrator_pool, org_id).await, Some(a));

    let router = common::build_router(&migrator_pool).await;
    let cookie = common::login_cookie(&router, "a@acme.test", PW).await;
    let resp = common::post_inquiry(&router, &cookie, "website", payload, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_json(resp).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["duplicate"], true);
    assert_eq!(body["routing_strategy"], "round_robin");
    assert_eq!(body["assigned_user_id"], a.to_string());
    // The replay never reached the rotation: pointer still a, not b.
    assert_eq!(
        stored_pointer(&migrator_pool, org_id).await,
        Some(a),
        "a duplicate replay must not consume a rotation turn"
    );
}

/// ~8-way concurrency against one Organization's round-robin pool of 3:
/// every attempt resolves within the advisory-lock retry budget (none
/// gives up as `IntakeBusy`), every resolution is genuinely
/// `RoundRobin`, and — because the per-Organization `intake:` lock
/// serializes the actual rotation regardless of wall-clock arrival order
/// — 8 draws over 3 members deterministically distribute {2, 3, 3}.
#[sqlx::test]
#[ignore]
async fn eight_way_concurrency_distributes_fairly_under_the_advisory_budget(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    let c = common::create_user(&migrator_pool, "c@acme.test", "Cid", PW).await;
    common::add_membership(&migrator_pool, org_id, a).await;
    common::add_membership(&migrator_pool, org_id, b).await;
    common::add_membership(&migrator_pool, org_id, c).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Bound to locals first: `tokio::join!` does not extend an inline
    // `&lead(...)` temporary's lifetime across the whole macro.
    let payloads: Vec<Value> = (0..8).map(|i| lead(&format!("c{i}@example.com"))).collect();
    let (o0, o1, o2, o3, o4, o5, o6, o7) = tokio::join!(
        system_intake(&app_pool, org_id, &payloads[0], &publisher),
        system_intake(&app_pool, org_id, &payloads[1], &publisher),
        system_intake(&app_pool, org_id, &payloads[2], &publisher),
        system_intake(&app_pool, org_id, &payloads[3], &publisher),
        system_intake(&app_pool, org_id, &payloads[4], &publisher),
        system_intake(&app_pool, org_id, &payloads[5], &publisher),
        system_intake(&app_pool, org_id, &payloads[6], &publisher),
        system_intake(&app_pool, org_id, &payloads[7], &publisher),
    );

    let mut counts: HashMap<Uuid, usize> = HashMap::new();
    for outcome in [o0, o1, o2, o3, o4, o5, o6, o7] {
        let ReceiveInquiryOutcome::Resolved {
            routing_strategy,
            assigned_user_id,
            ..
        } = outcome
        else {
            panic!("expected Resolved");
        };
        assert_eq!(routing_strategy, RoutingStrategy::RoundRobin);
        let user_id = assigned_user_id.expect("round_robin with a non-empty pool always assigns");
        *counts.entry(user_id.as_uuid()).or_insert(0) += 1;
    }
    assert_eq!(
        counts.len(),
        3,
        "all three members were assigned at least once"
    );
    let mut tally: Vec<usize> = counts.into_values().collect();
    tally.sort_unstable();
    assert_eq!(
        tally,
        vec![2, 3, 3],
        "8 draws over 3 members distribute {{2,3,3}}"
    );
}

/// Tenant isolation (§6): a user who is an active member of BOTH
/// Organizations rotates independently in each — org_a's activity writes
/// zero rows into org_b's `intake_rotation`, and vice versa.
#[sqlx::test]
#[ignore]
async fn dual_org_member_rotates_independently_and_org_b_gets_zero_rotation_rows(
    migrator_pool: PgPool,
) {
    let org_a = common::create_org(&migrator_pool, "Acme Realty").await;
    let org_b = common::create_org(&migrator_pool, "Best Realty").await;
    common::seed_stages(&migrator_pool, org_a).await;
    common::seed_stages(&migrator_pool, org_b).await;

    // A user who is an active member of BOTH organizations.
    let dual = common::create_user(&migrator_pool, "dual@shared.test", "Dual", PW).await;
    common::add_membership(&migrator_pool, org_a, dual).await;
    common::add_membership(&migrator_pool, org_b, dual).await;
    // A second org_a-only member so org_a's rotation is meaningfully
    // exercised (not just a single-member no-op cycle).
    let solo = common::create_user(&migrator_pool, "solo@acme.test", "Solo", PW).await;
    common::add_membership(&migrator_pool, org_a, solo).await;

    set_round_robin(&migrator_pool, org_a).await;
    set_round_robin(&migrator_pool, org_b).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Two intakes into org_a only; org_b left completely untouched.
    let o1 = system_intake(&app_pool, org_a, &lead("a1@example.com"), &publisher).await;
    let o2 = system_intake(&app_pool, org_a, &lead("a2@example.com"), &publisher).await;
    let assignee1 = assigned(o1).expect("org_a has active members");
    let assignee2 = assigned(o2).expect("org_a has active members");
    assert_ne!(
        assignee1, assignee2,
        "org_a's rotation alternates between dual and solo"
    );

    let org_b_rows_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM intake_rotation WHERE organization_id = $1")
            .bind(org_b)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        org_b_rows_before, 0,
        "org_b's rotation table is untouched by org_a's activity"
    );

    // Rotate org_b independently: dual is its only member there, so dual
    // gets every lead — org_b's cycle is entirely its own.
    let ob1 = system_intake(&app_pool, org_b, &lead("b1@example.com"), &publisher).await;
    let ob2 = system_intake(&app_pool, org_b, &lead("b2@example.com"), &publisher).await;
    assert_eq!(assigned(ob1), Some(UserId::new(dual)));
    assert_eq!(assigned(ob2), Some(UserId::new(dual)));

    // org_a's own pointer is unaffected by org_b's rotation activity.
    let org_a_pointer = stored_pointer(&migrator_pool, org_a)
        .await
        .expect("org_a has rotated");
    assert_eq!(UserId::new(org_a_pointer), assignee2);

    // A wholly unrelated third org proves an org_a/org_b intake never
    // leaks a row anywhere else.
    let org_c = common::create_org(&migrator_pool, "Cee Realty").await;
    let org_c_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM intake_rotation WHERE organization_id = $1")
            .bind(org_c)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(org_c_rows, 0);
}

/// Reviewer F2: the discriminating continue-vs-reset case. The pointer
/// member sits in the MIDDLE of the order when deactivated, so a
/// continue (past b's retained slot -> c) and a reset (anchor lost ->
/// first member a) produce DIFFERENT answers — a regression that
/// filtered the pointer's anchor join to active rows would fail here.
#[sqlx::test]
#[ignore]
async fn deactivating_the_mid_order_pointer_member_continues_to_the_next_slot(
    migrator_pool: PgPool,
) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    let b = common::create_user(&migrator_pool, "b@acme.test", "Bea", PW).await;
    let c = common::create_user(&migrator_pool, "c@acme.test", "Cid", PW).await;
    for user in [a, b, c] {
        common::add_membership(&migrator_pool, org_id, user).await;
    }
    backdate_membership(&migrator_pool, org_id, a, 3 * 3600).await;
    backdate_membership(&migrator_pool, org_id, b, 2 * 3600).await;
    backdate_membership(&migrator_pool, org_id, c, 3600).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Advance the pointer to b (the middle member).
    let outcome = system_intake(&app_pool, org_id, &lead("f2a@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));
    let outcome = system_intake(&app_pool, org_id, &lead("f2b@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(b)));

    // Deactivate the pointer member itself, mid-order.
    set_status(&migrator_pool, org_id, b, "inactive").await;

    // Continue: next after b's RETAINED slot is c. A reset would give a.
    let outcome = system_intake(&app_pool, org_id, &lead("f2c@example.com"), &publisher).await;
    assert_eq!(
        assigned(outcome),
        Some(UserId::new(c)),
        "must continue past the deactivated pointer's retained slot, not reset to the front"
    );
}

/// Reviewer F3 (spec §6's pinned claim): an `IntakeBusy` failure — the
/// advisory lock never acquired — leaves the rotation pointer untouched.
#[sqlx::test]
#[ignore]
async fn intake_busy_leaves_the_rotation_pointer_untouched(migrator_pool: PgPool) {
    use crm_api::domain::commands::receive_inquiry::ADVISORY_LOCK_BUDGET;
    use crm_api::domain::commands::CommandError;
    use std::time::Duration;

    let org_id = common::create_org(&migrator_pool, "Acme Realty").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let a = common::create_user(&migrator_pool, "a@acme.test", "Ann", PW).await;
    common::add_membership(&migrator_pool, org_id, a).await;
    set_round_robin(&migrator_pool, org_id).await;

    let app_pool = common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Seed the pointer with one successful rotation.
    let outcome = system_intake(&app_pool, org_id, &lead("seed@example.com"), &publisher).await;
    assert_eq!(assigned(outcome), Some(UserId::new(a)));
    let before = stored_pointer(&migrator_pool, org_id).await;
    assert_eq!(before, Some(a));

    // Hold the org's intake advisory lock externally past the budget.
    let hold = ADVISORY_LOCK_BUDGET + Duration::from_secs(2);
    let lock_key_text = org_id.to_string();
    let external_pool = migrator_pool.clone();
    let hold_task = tokio::spawn(async move {
        let mut tx = external_pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0))")
            .bind(&lock_key_text)
            .execute(&mut *tx)
            .await
            .unwrap();
        tokio::time::sleep(hold).await;
        let _ = tx.rollback().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let key = common::test_config().raw_payload_key;
    let actor = IntakeActor::System {
        on_behalf_of_user_id: None,
        organization_id: OrganizationId::new(org_id),
        origin: Origin::Cli,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let cmd = ReceiveInquiry {
        source: Source::parse("website").unwrap(),
        payload: serde_json::to_vec(&lead("busy@example.com")).unwrap(),
        assign_to_user_id: None,
        received_at: chrono::Utc::now(),
    };
    let result = commands::receive_inquiry(&app_pool, &key, &publisher, &actor, cmd).await;
    assert!(
        matches!(result, Err(CommandError::IntakeBusy)),
        "held lock past budget must yield IntakeBusy"
    );

    // The pointer is exactly where the seed left it.
    assert_eq!(stored_pointer(&migrator_pool, org_id).await, before);
    hold_task.abort();
}

/// Reviewer F4: the migration's D-041 backfill arm that `sqlx::test`'s
/// migrate-then-fixture model never exercises (fresh DBs have no
/// pre-existing rows when the migration runs). Recreate the pre-008
/// state — assignee set, mode still at the column DEFAULT 'unassigned' —
/// and re-execute the migration's exact UPDATE.
#[sqlx::test]
#[ignore]
async fn migration_backfill_maps_assignee_set_orgs_to_default_assignee_mode(migrator_pool: PgPool) {
    let (org_with, user) = common::create_org_with_stages_and_member(
        &migrator_pool,
        "Assigned Org",
        "w@assigned.test",
        "Wes",
        PW,
    )
    .await;
    let org_without = common::create_org(&migrator_pool, "Bare Org").await;
    admin_queries::update_intake_routing_settings(
        &mut migrator_pool.acquire().await.unwrap(),
        OrganizationId::new(org_with),
        IntakeRoutingMode::DefaultAssignee,
        Some(UserId::new(user)),
    )
    .await
    .unwrap();
    // Force both orgs back to the pre-backfill state (column DEFAULT).
    sqlx::query("UPDATE organization SET intake_routing_mode = 'unassigned' WHERE id = ANY($1)")
        .bind(vec![org_with, org_without])
        .execute(&migrator_pool)
        .await
        .unwrap();

    // The migration's backfill, verbatim (20260903000001 step 2).
    sqlx::query(
        "UPDATE organization SET intake_routing_mode = 'default_assignee'
         WHERE intake_default_assignee_user_id IS NOT NULL",
    )
    .execute(&migrator_pool)
    .await
    .unwrap();

    let mode = |org: Uuid| {
        let pool = migrator_pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT intake_routing_mode FROM organization WHERE id = $1",
            )
            .bind(org)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(mode(org_with).await, "default_assignee");
    assert_eq!(mode(org_without).await, "unassigned");
}

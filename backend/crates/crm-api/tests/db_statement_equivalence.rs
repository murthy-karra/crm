//! Slice 012 step 3 (docs/specs/SLICE_012.md §4, §8.5): the fourteen
//! pre-switch statements, frozen at `fixtures/statements_b45b04f` (see its
//! README), run against the SAME live text with IDENTICAL bindings over a
//! rich, two-tenant, fixed-clock fixture — corrections, a backdated inbound
//! capture, an outbound capture with its automatic attempt, a repeat
//! inquiry with an out-of-order earlier `received_at`, and zero-history
//! People. This file makes NO statement changes: the frozen and live texts
//! are byte-identical right now (proven in the fixture's README), so every
//! comparison below is, today, a tautology by construction — its job is to
//! prove the harness itself is wired correctly (row shapes, parameter
//! order, ordering) BEFORE step 4 (a later round) makes the live text
//! diverge one statement family at a time and re-runs this same file after
//! each change. Compares RESULTS (ordered id lists, counts, rows), never
//! text. Run only via ./scripts/check-db.

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::capture::pipeline::{insert_fact_and_maybe_attempt, ParsedMetadata, Via};
use crm_api::domain::capture::Direction;
use crm_api::domain::commands::{ContactChannel, ContactOutcome};
use crm_api::domain::person::filter::{
    AgeClause, AgeSpec, AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    SourceClause,
};
use crm_api::domain::person::model::PersonSummary;
use crm_api::domain::person::queries as person_queries;
use crm_api::domain::person::sort::{PersonSort, SortDirection, SortKey};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::today::model::TodayCandidate;
use crm_api::domain::today::sources as today_sources;
use crm_api::domain::today::system_feeds::evaluate as system_feeds_evaluate;
use crm_api::domain::today::system_feeds::{FeedKey, ResolvedFeed};
use crm_api::ids::{CorrelationId, CorrespondenceRawId, OrganizationId, PersonId, UserId};

#[path = "fixtures/statements_b45b04f/mod.rs"]
mod frozen;

use frozen::person_sql::SortedStatement;

// --- Fixture helpers (mirrors db_today_feed_equivalence.rs's own) ---------

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
           (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, \
            channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, $3, $4, 'call', 'no_answer') RETURNING id",
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

/// Through the real capture pipeline (not a raw fixture insert), so a
/// direction=`Outbound` row also writes its automatic `contact_attempted`
/// exactly as production does (D-042.4).
async fn capture_fact(
    app_pool: &PgPool,
    organization_id: Uuid,
    agent_user_id: Uuid,
    person_id: Uuid,
    direction: Direction,
    occurred_at: DateTime<Utc>,
    backdated: bool,
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
    .fetch_one(app_pool)
    .await
    .unwrap();
    let metadata = ParsedMetadata {
        via: if backdated { Via::Forward } else { Via::Cc },
        forward_style: None,
        forward_depth: if backdated { 1 } else { 0 },
        occurred_at,
        backdated,
        message_id: Some(format!("equiv-{}@fixture.test", Uuid::new_v4())),
        thread_key: None,
        working_from: None,
        working_to_cc: Vec::new(),
    };
    let mut tx = app_pool.begin().await.unwrap();
    insert_fact_and_maybe_attempt(
        &mut tx,
        OrganizationId::new(organization_id),
        UserId::new(agent_user_id),
        PersonId::new(person_id),
        direction,
        &metadata,
        CorrespondenceRawId::new(raw_id),
        CorrelationId::new(Uuid::new_v4()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: DateTime<Utc>,
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
    sqlx::query(
        "INSERT INTO contact_attempted \
            (organization_id, actor_kind, origin, occurred_at, correlation_id, \
             causation_id, person_id, channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, $3, $4, $5, 'call', 'reached')",
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

fn ts(day_offset: i64, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, hour, 0, 0).unwrap() + ChronoDuration::days(day_offset)
}

struct RichFixture {
    organization_id: Uuid,
    viewer_id: Uuid,
    now: DateTime<Utc>,
    /// `person_id`s in no particular order — used to build the call feed's
    /// `retained_ids` slices for `call_membership`/`call_only`.
    all_person_ids: Vec<Uuid>,
    call_person_id: Uuid,
    /// p7 — assigned to the viewer, client_replied-qualifying, latest
    /// inquiry source "zillow" (round 1 review fix 2's B-only pin).
    reply_person_id: Uuid,
}

/// Builds the rich, fixed-clock fixture in `organization_id`: a plain
/// baseline Person, a correction chain, a backdated inbound capture, an
/// outbound capture with its automatic attempt, a repeat inquiry with an
/// out-of-order earlier `received_at`, a zero-history Person, a
/// `client_replied`-qualifying Person, and a call-outcome-needed Person.
async fn build_rich_fixture(migrator_pool: &PgPool, app_pool: &PgPool) -> RichFixture {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Equivalence Realty",
        "viewer@equivalence.test",
        "Viewer",
        "correct horse battery staple",
    )
    .await;
    let bob_id = crate::common::create_user(
        migrator_pool,
        "bob@equivalence.test",
        "Bob",
        "correct horse battery staple",
    )
    .await;
    crate::common::add_membership(migrator_pool, organization_id, bob_id).await;
    let stage_id = first_stage_id(migrator_pool, organization_id).await;
    let now = ts(41, 12);
    let mut all_person_ids = Vec::new();

    // p1: plain baseline, source zillow, assigned to viewer.
    let p1 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p1, "zillow", ts(1, 9)).await;
    insert_contact_attempt(migrator_pool, organization_id, p1, ts(2, 9)).await;
    all_person_ids.push(p1);

    // p2: a correction chain — the original attempt is superseded, the
    // correction inherits its occurred_at (SLICE_006c §2).
    let p2 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p2, "website", ts(3, 9)).await;
    let original = insert_contact_attempt(migrator_pool, organization_id, p2, ts(4, 9)).await;
    insert_contact_correction(
        migrator_pool,
        organization_id,
        p2,
        viewer_id,
        original,
        ts(4, 9),
    )
    .await;
    all_person_ids.push(p2);

    // p3: a backdated inbound capture (D-042 retroactive forward) alongside
    // an ordinary one.
    let p3 = insert_person(migrator_pool, organization_id, stage_id, Some(bob_id)).await;
    insert_inquiry(migrator_pool, organization_id, p3, "referral", ts(5, 9)).await;
    capture_fact(
        app_pool,
        organization_id,
        bob_id,
        p3,
        Direction::Inbound,
        ts(7, 9),
        false,
    )
    .await;
    capture_fact(
        app_pool,
        organization_id,
        bob_id,
        p3,
        Direction::Inbound,
        ts(6, 9),
        true,
    )
    .await;
    all_person_ids.push(p3);

    // p4: an outbound capture with its automatic contact_attempted
    // (D-042.4) — advances both last_outbound and last_contact live.
    let p4 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p4, "zillow", ts(8, 9)).await;
    capture_fact(
        app_pool,
        organization_id,
        viewer_id,
        p4,
        Direction::Outbound,
        ts(9, 9),
        false,
    )
    .await;
    all_person_ids.push(p4);

    // p5: a repeat inquiry with an OUT-OF-ORDER EARLIER received_at — the
    // second INSERT names an earlier received_at than the first, so the
    // "latest" inquiry (by received_at DESC, id DESC) stays the first one.
    let p5 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p5, "website", ts(12, 9)).await;
    insert_inquiry(migrator_pool, organization_id, p5, "zillow", ts(10, 9)).await;
    all_person_ids.push(p5);

    // p6: zero-history — a bare Person row, no inquiry, no contact, no
    // correspondence at all.
    let p6 = insert_person(migrator_pool, organization_id, stage_id, None).await;
    all_person_ids.push(p6);

    // p7: client_replied-qualifying — assigned to viewer, an inbound
    // correspondence later than both the effective last contact attempt
    // and the latest outbound correspondence.
    let p7 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p7, "zillow", ts(13, 9)).await;
    insert_contact_attempt(migrator_pool, organization_id, p7, ts(14, 9)).await;
    capture_fact(
        app_pool,
        organization_id,
        viewer_id,
        p7,
        Direction::Inbound,
        ts(15, 9),
        false,
    )
    .await;
    all_person_ids.push(p7);

    // p8 (call_person_id): the viewer's ended call with an uncorrected
    // automatic "reached" attempt — awaiting_call_outcome for the viewer,
    // and the retained-set input for call_membership/call_only.
    let p8 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p8, "zillow", ts(16, 9)).await;
    insert_call(migrator_pool, organization_id, p8, viewer_id, ts(17, 9)).await;
    all_person_ids.push(p8);

    // p9: the client-replied TIE — an inbound reply at the EXACT SAME
    // instant as the effective last contact attempt (round 1 review fix
    // 3). client_replied_unanswered's `last_inbound_at > last_contact_at`
    // is strict, so this Person must never match it on either text.
    let p9 = insert_person(migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(migrator_pool, organization_id, p9, "zillow", ts(18, 9)).await;
    let tie_at = ts(19, 9);
    insert_contact_attempt(migrator_pool, organization_id, p9, tie_at).await;
    capture_fact(
        app_pool,
        organization_id,
        viewer_id,
        p9,
        Direction::Inbound,
        tie_at,
        false,
    )
    .await;
    all_person_ids.push(p9);

    RichFixture {
        organization_id,
        viewer_id,
        now,
        all_person_ids,
        call_person_id: p8,
        reply_person_id: p7,
    }
}

/// A second, smaller tenant, present concurrently — proves nothing above
/// leaks the Organization boundary and that the comparisons hold on a
/// second, differently shaped, book too.
async fn build_second_tenant(migrator_pool: &PgPool) -> (Uuid, Uuid) {
    let (organization_id, carol_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Equivalence Realty Two",
        "carol@equivalence-two.test",
        "Carol",
        "correct horse battery staple",
    )
    .await;
    let stage_id = first_stage_id(migrator_pool, organization_id).await;
    let person = insert_person(migrator_pool, organization_id, stage_id, Some(carol_id)).await;
    insert_inquiry(migrator_pool, organization_id, person, "zillow", ts(20, 9)).await;
    insert_contact_attempt(migrator_pool, organization_id, person, ts(21, 9)).await;
    (organization_id, carol_id)
}

// --- Comparison helpers ------------------------------------------------

fn people_json(rows: &[PersonSummary], truncated: bool) -> serde_json::Value {
    serde_json::json!({ "rows": rows, "truncated": truncated })
}

/// Signature for a `TodayCandidate` sufficient to prove result equivalence
/// without requiring `TodayCandidate`/`ContactAttemptRef` to implement
/// `PartialEq` (neither does): every field the frozen and live rows carry,
/// in a directly-comparable tuple.
#[derive(Debug, PartialEq)]
struct CandidateSignature {
    person_id: Uuid,
    inquiry_count: i64,
    latest_inquiry_id: Uuid,
    latest_inquiry_source: String,
    latest_inquiry_received_at: DateTime<Utc>,
    last_contact_attempt: Option<(Uuid, ContactChannel, ContactOutcome, DateTime<Utc>)>,
    waiting_since: DateTime<Utc>,
    fresh: bool,
    by_inquiry: bool,
    outcome_needed: Option<(Uuid, DateTime<Utc>)>,
    client_replied: Option<DateTime<Utc>>,
}

fn candidate_signature(c: &TodayCandidate) -> CandidateSignature {
    CandidateSignature {
        person_id: c.person.id.0,
        inquiry_count: c.inquiry_count,
        latest_inquiry_id: c.latest_inquiry.id.0,
        latest_inquiry_source: c.latest_inquiry.source.clone(),
        latest_inquiry_received_at: c.latest_inquiry.received_at,
        last_contact_attempt: c
            .last_contact_attempt
            .as_ref()
            .map(|a| (a.id, a.channel, a.outcome, a.occurred_at)),
        waiting_since: c.waiting_since,
        fresh: c.fresh,
        by_inquiry: c.by_inquiry,
        outcome_needed: c.outcome_needed.map(|o| (o.call_id, o.ended_at)),
        client_replied: c.client_replied,
    }
}

fn candidate_signatures(rows: &[TodayCandidate]) -> Vec<CandidateSignature> {
    rows.iter().map(candidate_signature).collect()
}

/// Every `filtered_summaries*` combination (the canonical statement plus
/// its seven sorted copies) and `count_filtered_matches` — nine of the
/// fourteen statements — for one `FilterDefinition`, one Organization, one
/// viewer.
async fn assert_people_family_equal(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    now: DateTime<Utc>,
    filter: &FilterDefinition,
    label: &str,
) {
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(organization_id));
    let params = filter.to_query_params(UserId::new(viewer_id));

    // The canonical statement (unsorted request), bound to the SAME fixed
    // clock on both sides via the test-only common-clock seam.
    {
        let mut live_conn = app_pool.acquire().await.unwrap();
        let live = person_queries::filtered_summaries_at(&mut live_conn, &scope, &params, now)
            .await
            .unwrap();
        drop(live_conn);
        let mut frozen_conn = app_pool.acquire().await.unwrap();
        let frozen_result =
            frozen::person_sql::filtered_summaries(&mut frozen_conn, &scope, &params, Some(now))
                .await
                .unwrap();
        assert_eq!(
            people_json(&frozen_result.0, frozen_result.1),
            people_json(&live.0, live.1),
            "{label}: canonical filtered_summaries"
        );
    }

    // The seven sorted copies (live `now()` on both sides, per the live
    // callers' own hardcoded `None` — see person_sql.rs's doc comment).
    let sorts: [(SortedStatement, PersonSort, &str); 7] = [
        (
            SortedStatement::CreatedAsc,
            PersonSort {
                key: SortKey::Created,
                direction: SortDirection::Asc,
            },
            "created.asc",
        ),
        (
            SortedStatement::NameAsc,
            PersonSort {
                key: SortKey::Name,
                direction: SortDirection::Asc,
            },
            "name.asc",
        ),
        (
            SortedStatement::NameDesc,
            PersonSort {
                key: SortKey::Name,
                direction: SortDirection::Desc,
            },
            "name.desc",
        ),
        (
            SortedStatement::StageAsc,
            PersonSort {
                key: SortKey::Stage,
                direction: SortDirection::Asc,
            },
            "stage.asc",
        ),
        (
            SortedStatement::StageDesc,
            PersonSort {
                key: SortKey::Stage,
                direction: SortDirection::Desc,
            },
            "stage.desc",
        ),
        (
            SortedStatement::AssigneeAsc,
            PersonSort {
                key: SortKey::Assignee,
                direction: SortDirection::Asc,
            },
            "assignee.asc",
        ),
        (
            SortedStatement::AssigneeDesc,
            PersonSort {
                key: SortKey::Assignee,
                direction: SortDirection::Desc,
            },
            "assignee.desc",
        ),
    ];
    for (which, sort, sort_label) in sorts {
        let mut live_conn = app_pool.acquire().await.unwrap();
        let live = person_queries::filtered_summaries_sorted(&mut live_conn, &scope, &params, sort)
            .await
            .unwrap();
        drop(live_conn);
        let mut frozen_conn = app_pool.acquire().await.unwrap();
        let frozen_result =
            frozen::person_sql::filtered_summaries_sorted(&mut frozen_conn, &scope, &params, which)
                .await
                .unwrap();
        assert_eq!(
            people_json(&frozen_result.0, frozen_result.1),
            people_json(&live.0, live.1),
            "{label}: sorted {sort_label}"
        );
    }

    // count_filtered_matches (the ninth of the fourteen).
    {
        let mut live_conn = app_pool.acquire().await.unwrap();
        let live = person_queries::count_filtered_matches(&mut live_conn, &scope, &params)
            .await
            .unwrap();
        drop(live_conn);
        let mut frozen_conn = app_pool.acquire().await.unwrap();
        let frozen_result =
            frozen::person_sql::count_filtered_matches(&mut frozen_conn, &scope, &params)
                .await
                .unwrap();
        assert_eq!(frozen_result, live, "{label}: count_filtered_matches");
    }
}

async fn assert_source_statements_equal(
    app_pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    now: DateTime<Utc>,
    filter: &FilterDefinition,
    label: &str,
) {
    let params = filter.to_query_params(UserId::new(viewer_id));
    let builtin_ids: Vec<Uuid> = Vec::new();

    let mut live_conn = app_pool.acquire().await.unwrap();
    let live_membership = today_sources::source_membership(
        &mut live_conn,
        OrganizationId::new(organization_id),
        &params,
        now,
        &builtin_ids,
    )
    .await
    .unwrap();
    drop(live_conn);
    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let frozen_membership = frozen::today_sql::source_membership(
        &mut frozen_conn,
        OrganizationId::new(organization_id),
        &params,
        now,
        &builtin_ids,
    )
    .await
    .unwrap();
    assert_eq!(
        frozen_membership, live_membership,
        "{label}: source_membership"
    );

    let mut live_conn = app_pool.acquire().await.unwrap();
    let live_candidates = today_sources::source_candidates(
        &mut live_conn,
        OrganizationId::new(organization_id),
        &params,
        now,
        &builtin_ids,
        false,
        50,
    )
    .await
    .unwrap();
    drop(live_conn);
    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let frozen_candidates = frozen::today_sql::source_candidates(
        &mut frozen_conn,
        OrganizationId::new(organization_id),
        &params,
        now,
        &builtin_ids,
        false,
        50,
    )
    .await
    .unwrap();
    let live_ids: Vec<Uuid> = live_candidates.iter().map(|c| c.person.id.0).collect();
    let frozen_ids: Vec<Uuid> = frozen_candidates.iter().map(|c| c.person.id.0).collect();
    assert_eq!(
        frozen_ids, live_ids,
        "{label}: source_candidates ordered id list"
    );
    let live_last_contact: Vec<Option<DateTime<Utc>>> =
        live_candidates.iter().map(|c| c.last_contact_at).collect();
    let frozen_last_contact: Vec<Option<DateTime<Utc>>> = frozen_candidates
        .iter()
        .map(|c| c.last_contact_at)
        .collect();
    assert_eq!(
        frozen_last_contact, live_last_contact,
        "{label}: source_candidates last_contact_at column"
    );

    let inquiry_key = |c: &today_sources::SourceCandidate| {
        c.latest_inquiry
            .as_ref()
            .map(|i| (i.id.0, i.source.clone(), i.received_at))
    };
    let frozen_inquiry_key = |c: &frozen::today_sql::FrozenSourceCandidate| {
        c.latest_inquiry
            .as_ref()
            .map(|i| (i.id.0, i.source.clone(), i.received_at))
    };
    let live_latest_inquiry: Vec<_> = live_candidates.iter().map(inquiry_key).collect();
    let frozen_latest_inquiry: Vec<_> = frozen_candidates.iter().map(frozen_inquiry_key).collect();
    assert_eq!(
        frozen_latest_inquiry, live_latest_inquiry,
        "{label}: source_candidates latest_inquiry column"
    );

    let attempt_key = |c: &today_sources::SourceCandidate| {
        c.last_contact_attempt
            .as_ref()
            .map(|a| (a.id, a.channel, a.outcome, a.occurred_at))
    };
    let frozen_attempt_key = |c: &frozen::today_sql::FrozenSourceCandidate| {
        c.last_contact_attempt
            .as_ref()
            .map(|a| (a.id, a.channel, a.outcome, a.occurred_at))
    };
    let live_last_attempt: Vec<_> = live_candidates.iter().map(attempt_key).collect();
    let frozen_last_attempt: Vec<_> = frozen_candidates.iter().map(frozen_attempt_key).collect();
    assert_eq!(
        frozen_last_attempt, live_last_attempt,
        "{label}: source_candidates last_contact_attempt column"
    );
}

fn resolved_feed(
    feed_key: FeedKey,
    filter: FilterDefinition,
    fresh_within_hours: Option<i32>,
) -> ResolvedFeed {
    ResolvedFeed {
        feed_key,
        enabled: true,
        filter,
        fresh_within_hours,
        is_default: false,
        fallback: false,
        revision: 1,
        updated_at: Utc::now(),
        updated_by_user_id: None,
    }
}

/// `person_state`, `call_membership` and `call_only` — the last three of
/// the fourteen statements.
async fn assert_system_feed_statements_equal(app_pool: &PgPool, fx: &RichFixture) {
    let feed_a_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    };
    let feed_b_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::ClientRepliedUnanswered(BoolClause { value: true }),
        ],
    };
    let call_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
    };

    let feed_a = resolved_feed(FeedKey::UnansweredInquiry, feed_a_filter.clone(), Some(24));
    let feed_b = resolved_feed(FeedKey::ClientReplied, feed_b_filter.clone(), Some(24));
    let call_feed = resolved_feed(FeedKey::CallOutcomeNeeded, call_filter.clone(), None);

    let viewer = UserId::new(fx.viewer_id);
    let org = OrganizationId::new(fx.organization_id);

    // person_state.
    let mut live_conn = app_pool.acquire().await.unwrap();
    let live_person_state = system_feeds_evaluate::person_state_candidates(
        &mut live_conn,
        org,
        viewer,
        fx.now,
        &feed_a,
        &feed_b,
    )
    .await
    .unwrap();
    drop(live_conn);

    let params_a = feed_a_filter.to_query_params(viewer);
    let params_b = feed_b_filter.to_query_params(viewer);
    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let frozen_person_state = frozen::system_feeds_sql::person_state_candidates(
        &mut frozen_conn,
        org,
        viewer,
        fx.now,
        &params_a,
        true,
        24,
        &params_b,
        true,
        24,
    )
    .await
    .unwrap();
    assert_eq!(
        candidate_signatures(&frozen_person_state.0),
        candidate_signatures(&live_person_state.0),
        "person_state: candidate signatures"
    );
    assert_eq!(
        frozen_person_state.1, live_person_state.1,
        "person_state: truncated"
    );

    // Round 1 review fix 2: the `latest` LATERAL's guard
    // (`ON ($5::text[] IS NOT NULL OR $27::text[] IS NOT NULL)`) is
    // otherwise never exercised on either text — a source clause bound on
    // ONLY feed A, ONLY feed B, or both must all agree between frozen and
    // live. `fx.reply_person_id` (p7) is assigned to the viewer,
    // client_replied-qualifying, and its latest inquiry's source is
    // "zillow" — the B-only case must retain it (proving the guard opens
    // the LATERAL for feed B's source check even though feed A's own
    // source parameter is NULL).
    {
        let source_zillow = Clause::Source(SourceClause {
            sources: vec!["zillow".to_string()],
        });
        let feed_a_source_filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me],
                }),
                Clause::AwaitingResponse(BoolClause { value: true }),
                source_zillow.clone(),
            ],
        };
        let feed_b_source_filter = FilterDefinition {
            version: 1,
            clauses: vec![
                Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me],
                }),
                Clause::ClientRepliedUnanswered(BoolClause { value: true }),
                source_zillow,
            ],
        };

        for (label, filter_a, filter_b) in [
            ("feed A source-only", &feed_a_source_filter, &feed_b_filter),
            ("feed B source-only", &feed_a_filter, &feed_b_source_filter),
            ("both source", &feed_a_source_filter, &feed_b_source_filter),
        ] {
            let resolved_a = resolved_feed(FeedKey::UnansweredInquiry, filter_a.clone(), Some(24));
            let resolved_b = resolved_feed(FeedKey::ClientReplied, filter_b.clone(), Some(24));

            let mut live_conn = app_pool.acquire().await.unwrap();
            let (live_candidates, live_truncated) = system_feeds_evaluate::person_state_candidates(
                &mut live_conn,
                org,
                viewer,
                fx.now,
                &resolved_a,
                &resolved_b,
            )
            .await
            .unwrap();
            drop(live_conn);

            let params_a = filter_a.to_query_params(viewer);
            let params_b = filter_b.to_query_params(viewer);
            let mut frozen_conn = app_pool.acquire().await.unwrap();
            let (frozen_candidates, frozen_truncated) =
                frozen::system_feeds_sql::person_state_candidates(
                    &mut frozen_conn,
                    org,
                    viewer,
                    fx.now,
                    &params_a,
                    true,
                    24,
                    &params_b,
                    true,
                    24,
                )
                .await
                .unwrap();

            assert_eq!(
                candidate_signatures(&frozen_candidates),
                candidate_signatures(&live_candidates),
                "person_state ({label}): candidate signatures"
            );
            assert_eq!(
                frozen_truncated, live_truncated,
                "person_state ({label}): truncated"
            );

            if label == "feed B source-only" {
                let live_ids: Vec<Uuid> = live_candidates.iter().map(|c| c.person.id.0).collect();
                assert!(
                    live_ids.contains(&fx.reply_person_id),
                    "the zillow reply member must remain in the B-only source-guarded feed (live)"
                );
                let frozen_ids: Vec<Uuid> = candidate_signatures(&frozen_candidates)
                    .iter()
                    .map(|s| s.person_id)
                    .collect();
                assert!(
                    frozen_ids.contains(&fx.reply_person_id),
                    "the zillow reply member must remain in the B-only source-guarded feed (frozen)"
                );
            }
        }
    }

    let retained_ids: Vec<Uuid> = live_person_state
        .0
        .iter()
        .map(|c| c.person.id.0)
        .chain(std::iter::once(fx.call_person_id))
        .collect();

    // call_membership.
    let mut live_conn = app_pool.acquire().await.unwrap();
    let live_call_membership = system_feeds_evaluate::call_membership(
        &mut live_conn,
        org,
        viewer,
        &call_feed,
        &retained_ids,
        fx.now,
    )
    .await
    .unwrap();
    drop(live_conn);

    let call_params = call_filter.to_query_params(viewer);
    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let frozen_call_membership = frozen::system_feeds_sql::call_membership(
        &mut frozen_conn,
        org,
        viewer,
        &call_params,
        &retained_ids,
        fx.now,
    )
    .await
    .unwrap();
    assert_eq!(
        frozen_call_membership, live_call_membership,
        "call_membership"
    );

    // call_only: excludes the retained P set, so pass an empty retained
    // slice here (the call feed's own person, p8, is not otherwise
    // retained by the person-state feeds in this fixture).
    let empty_retained: Vec<Uuid> = Vec::new();
    let mut live_conn = app_pool.acquire().await.unwrap();
    let live_call_only = system_feeds_evaluate::call_only_candidates(
        &mut live_conn,
        org,
        viewer,
        &call_feed,
        &empty_retained,
        200,
        fx.now,
    )
    .await
    .unwrap();
    drop(live_conn);

    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let frozen_call_only = frozen::system_feeds_sql::call_only_candidates(
        &mut frozen_conn,
        org,
        viewer,
        &call_params,
        &empty_retained,
        200,
        fx.now,
    )
    .await
    .unwrap();
    assert_eq!(
        candidate_signatures(&frozen_call_only),
        candidate_signatures(&live_call_only),
        "call_only: candidate signatures"
    );
}

/// Slice 012 §4, §8.5: all fourteen statements agree between the frozen
/// (`b45b04f`) and live text, over the rich fixture, for a spread of
/// filter axes — including the three explicitly named
/// (`has_replied`, `awaiting_response`, `client_replied_unanswered`) — and
/// on a second, concurrently present tenant.
#[sqlx::test]
#[ignore]
async fn all_fourteen_statements_agree_between_frozen_and_live_text(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let fx = build_rich_fixture(&migrator_pool, &app_pool).await;
    let (org_b, carol_id) = build_second_tenant(&migrator_pool).await;

    let empty = FilterDefinition {
        version: 1,
        clauses: vec![],
    };
    let last_contact_not_within_7 = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastContact(AgeClause {
            age: AgeSpec::NotWithinDays(7),
        })],
    };
    let last_contact_never = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastContact(AgeClause {
            age: AgeSpec::Never,
        })],
    };
    let has_replied_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::HasReplied(BoolClause { value: true })],
    };
    let has_replied_false = FilterDefinition {
        version: 1,
        clauses: vec![Clause::HasReplied(BoolClause { value: false })],
    };
    let awaiting_response_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingResponse(BoolClause { value: true })],
    };
    let client_replied_unanswered_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::ClientRepliedUnanswered(BoolClause { value: true })],
    };
    let assigned_to_me = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::Me],
        })],
    };
    let source_zillow = FilterDefinition {
        version: 1,
        clauses: vec![Clause::Source(SourceClause {
            sources: vec!["zillow".to_string()],
        })],
    };
    let awaiting_call_outcome_true = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AwaitingCallOutcome(BoolClause { value: true })],
    };
    // Round 1 review fix 1: LastInquiry and LastInbound × within_days(7),
    // not_within_days(7), never — parameters $9-$11 and $15-$17 were
    // otherwise never bound on any of the fourteen statements, so a
    // swapped last_inbound_at/last_inquiry_at column reference would pass
    // this test undetected.
    let last_inquiry_within_7 = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInquiry(AgeClause {
            age: AgeSpec::WithinDays(7),
        })],
    };
    let last_inquiry_not_within_7 = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInquiry(AgeClause {
            age: AgeSpec::NotWithinDays(7),
        })],
    };
    let last_inquiry_never = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInquiry(AgeClause {
            age: AgeSpec::Never,
        })],
    };
    let last_inbound_within_7 = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInbound(AgeClause {
            age: AgeSpec::WithinDays(7),
        })],
    };
    let last_inbound_not_within_7 = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInbound(AgeClause {
            age: AgeSpec::NotWithinDays(7),
        })],
    };
    let last_inbound_never = FilterDefinition {
        version: 1,
        clauses: vec![Clause::LastInbound(AgeClause {
            age: AgeSpec::Never,
        })],
    };

    let axes: [(&str, &FilterDefinition); 16] = [
        ("empty (the full org)", &empty),
        ("last_contact not_within_days 7", &last_contact_not_within_7),
        ("last_contact never", &last_contact_never),
        ("has_replied true", &has_replied_true),
        ("has_replied false", &has_replied_false),
        ("awaiting_response true", &awaiting_response_true),
        (
            "client_replied_unanswered true",
            &client_replied_unanswered_true,
        ),
        ("assigned_to me", &assigned_to_me),
        ("source zillow", &source_zillow),
        ("awaiting_call_outcome true", &awaiting_call_outcome_true),
        ("last_inquiry within_days 7", &last_inquiry_within_7),
        ("last_inquiry not_within_days 7", &last_inquiry_not_within_7),
        ("last_inquiry never", &last_inquiry_never),
        ("last_inbound within_days 7", &last_inbound_within_7),
        ("last_inbound not_within_days 7", &last_inbound_not_within_7),
        ("last_inbound never", &last_inbound_never),
    ];

    for (label, filter) in axes {
        assert_people_family_equal(
            &app_pool,
            fx.organization_id,
            fx.viewer_id,
            fx.now,
            filter,
            label,
        )
        .await;
        assert_source_statements_equal(
            &app_pool,
            fx.organization_id,
            fx.viewer_id,
            fx.now,
            filter,
            label,
        )
        .await;
    }

    assert_system_feed_statements_equal(&app_pool, &fx).await;

    // The second tenant: a smaller but non-trivial book, proving the
    // fourteen-statement agreement holds there too, concurrently with the
    // rich Organization above (never comparing across tenants — each
    // comparison is scoped to its own Organization, as production always
    // is).
    let org_b_now = ts(22, 12);
    assert_people_family_equal(
        &app_pool,
        org_b,
        carol_id,
        org_b_now,
        &empty,
        "org B: empty",
    )
    .await;
    assert_source_statements_equal(
        &app_pool,
        org_b,
        carol_id,
        org_b_now,
        &empty,
        "org B: empty",
    )
    .await;

    // Sanity: every declared person id is actually present, so the
    // fixture itself is proven non-degenerate (zero-history Person
    // included, per spec §4's fixture requirement).
    assert_eq!(fx.all_person_ids.len(), 9);
}

// --- Spec §8.6 explicit pins: the gated waiting probe, the zero-inquiry
// exclusion, and waiting_since/fresh for repeat inquirers and reply
// members --------------------------------------------------------------

/// docs/specs/SLICE_012.md §8.6: four pins on `person_state`'s frozen vs
/// live text, on a small purpose-built fixture (the broader Today
/// equivalence/parity suites — db_today_feed_equivalence.rs,
/// db_today_builtin_parity.rs, db_today_source_filter_parity.rs, left
/// untouched by this slice — already prove the general case and
/// ordering/truncation near the 201 cap):
///
/// 1. The equality tie: an inquiry whose `received_at` exactly EQUALS the
///    Person's effective last contact attempt is not "waiting" on either
///    text (`>` is strict on both sides of the switch).
/// 2. A zero-inquiry Person who nonetheless has a contact attempt and an
///    inbound reply stays excluded from BOTH person-state feeds (the §1
///    rule 7 "at least one inquiry" gate, now `p.last_inquiry_at IS NOT
///    NULL`, still applies uniformly).
/// 3. A repeat inquirer's `waiting_since` (the EARLIEST unanswered
///    inquiry, not the latest) and `fresh` (from the latest inquiry)
///    agree between frozen and live.
/// 4. A reply member's `waiting_since` (the reply's own `occurred_at`)
///    and `fresh` agree between frozen and live.
#[sqlx::test]
#[ignore]
async fn person_state_pins_the_waiting_gate_tie_zero_inquiry_exclusion_and_repeat_reply_members(
    migrator_pool: PgPool,
) {
    let (organization_id, viewer_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Person State Pins Realty",
        "viewer@person-state-pins.test",
        "Viewer",
        "correct horse battery staple",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&migrator_pool, organization_id).await;
    let now = ts(30, 12);

    // Pin 1: the equality tie. An inquiry and a contact attempt at the
    // EXACT SAME instant — "an inquiry after the last contact" must be
    // false (strict >, not >=), so this Person is not waiting.
    let tie = insert_person(&migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    let tie_at = ts(1, 9);
    insert_inquiry(&migrator_pool, organization_id, tie, "zillow", tie_at).await;
    insert_contact_attempt(&migrator_pool, organization_id, tie, tie_at).await;

    // Pin 2: zero-inquiry Person with a contact attempt AND an inbound
    // reply — must be excluded from person_state entirely, on both feeds.
    let zero_inquiry_reply =
        insert_person(&migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_contact_attempt(
        &migrator_pool,
        organization_id,
        zero_inquiry_reply,
        ts(2, 9),
    )
    .await;
    capture_fact(
        &app_pool,
        organization_id,
        viewer_id,
        zero_inquiry_reply,
        Direction::Inbound,
        ts(3, 9),
        false,
    )
    .await;

    // Pin 3: a repeat inquirer. First inquiry at day 5 answered by a
    // contact attempt at day 6; a second, unanswered inquiry at day 10 —
    // waiting_since must be day 10 (the earliest inquiry AFTER the last
    // contact, which is also the only one here), fresh depends on the
    // LATEST inquiry (day 10) against the 24h window.
    let repeat = insert_person(&migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(&migrator_pool, organization_id, repeat, "zillow", ts(5, 9)).await;
    insert_contact_attempt(&migrator_pool, organization_id, repeat, ts(6, 9)).await;
    let repeat_waiting_since = ts(10, 9);
    insert_inquiry(
        &migrator_pool,
        organization_id,
        repeat,
        "website",
        repeat_waiting_since,
    )
    .await;

    // Pin 4: a reply member — assigned, an inbound reply later than any
    // contact attempt/outbound. waiting_since must be the reply's own
    // occurred_at.
    let reply = insert_person(&migrator_pool, organization_id, stage_id, Some(viewer_id)).await;
    insert_inquiry(&migrator_pool, organization_id, reply, "zillow", ts(11, 9)).await;
    insert_contact_attempt(&migrator_pool, organization_id, reply, ts(12, 9)).await;
    let reply_waiting_since = ts(13, 9);
    capture_fact(
        &app_pool,
        organization_id,
        viewer_id,
        reply,
        Direction::Inbound,
        reply_waiting_since,
        false,
    )
    .await;

    let feed_a_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    };
    let feed_b_filter = FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::ClientRepliedUnanswered(BoolClause { value: true }),
        ],
    };
    let feed_a = resolved_feed(FeedKey::UnansweredInquiry, feed_a_filter.clone(), Some(24));
    let feed_b = resolved_feed(FeedKey::ClientReplied, feed_b_filter.clone(), Some(24));
    let viewer = UserId::new(viewer_id);
    let org = OrganizationId::new(organization_id);

    let mut live_conn = app_pool.acquire().await.unwrap();
    let (live_candidates, live_truncated) = system_feeds_evaluate::person_state_candidates(
        &mut live_conn,
        org,
        viewer,
        now,
        &feed_a,
        &feed_b,
    )
    .await
    .unwrap();
    drop(live_conn);

    let params_a = feed_a_filter.to_query_params(viewer);
    let params_b = feed_b_filter.to_query_params(viewer);
    let mut frozen_conn = app_pool.acquire().await.unwrap();
    let (frozen_candidates, frozen_truncated) = frozen::system_feeds_sql::person_state_candidates(
        &mut frozen_conn,
        org,
        viewer,
        now,
        &params_a,
        true,
        24,
        &params_b,
        true,
        24,
    )
    .await
    .unwrap();

    assert_eq!(
        candidate_signatures(&frozen_candidates),
        candidate_signatures(&live_candidates),
        "frozen and live person_state must agree on this fixture"
    );
    assert_eq!(frozen_truncated, live_truncated);
    assert!(!live_truncated, "this small fixture must not truncate");

    let live_ids: Vec<Uuid> = live_candidates.iter().map(|c| c.person.id.0).collect();

    // Pin 1: the tie is excluded from both texts.
    assert!(
        !live_ids.contains(&tie),
        "an inquiry exactly at the last contact instant must not be waiting (live)"
    );
    assert!(
        !candidate_signatures(&frozen_candidates)
            .iter()
            .any(|s| s.person_id == tie),
        "an inquiry exactly at the last contact instant must not be waiting (frozen)"
    );

    // Pin 2: the zero-inquiry Person with a reply is excluded entirely.
    assert!(
        !live_ids.contains(&zero_inquiry_reply),
        "a zero-inquiry Person must never qualify for person_state, reply or not"
    );

    // Pin 3: the repeat inquirer's waiting_since is the (only, unanswered)
    // later inquiry, agreeing on both texts.
    let repeat_live = live_candidates
        .iter()
        .find(|c| c.person.id.0 == repeat)
        .expect("repeat inquirer must qualify by inquiry");
    assert_eq!(repeat_live.waiting_since, repeat_waiting_since);
    assert!(repeat_live.by_inquiry);
    let repeat_frozen = frozen_candidates
        .iter()
        .find(|c| c.person.id.0 == repeat)
        .expect("repeat inquirer must qualify by inquiry (frozen)");
    assert_eq!(repeat_frozen.waiting_since, repeat_waiting_since);
    assert_eq!(repeat_frozen.fresh, repeat_live.fresh);

    // Pin 4: the reply member's waiting_since is the reply's own
    // occurred_at, agreeing on both texts.
    let reply_live = live_candidates
        .iter()
        .find(|c| c.person.id.0 == reply)
        .expect("reply member must qualify by reply");
    assert_eq!(reply_live.waiting_since, reply_waiting_since);
    assert!(reply_live.client_replied.is_some());
    let reply_frozen = frozen_candidates
        .iter()
        .find(|c| c.person.id.0 == reply)
        .expect("reply member must qualify by reply (frozen)");
    assert_eq!(reply_frozen.waiting_since, reply_waiting_since);
    assert_eq!(reply_frozen.fresh, reply_live.fresh);
}

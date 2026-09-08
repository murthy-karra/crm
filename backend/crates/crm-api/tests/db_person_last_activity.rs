//! DB-backed tests for Slice 012 (docs/specs/SLICE_012.md): the migration's
//! backfill block (§8.1) and the per-write-path invariant, concurrency,
//! isolation and untouched-`updated_at` behaviour through the typed
//! commands (§8.2–8.4, 8.7, 8.8). No statement switch happens in this
//! file — every assertion reads the four `last_*_at` columns directly.
//! Run only via ./scripts/check-db.

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crm_api::domain::capture::pipeline::{insert_fact_and_maybe_attempt, ParsedMetadata, Via};
use crm_api::domain::capture::Direction;
use crm_api::domain::commands::{
    self, correct_call_outcome, log_contact_attempt, CallOutcomeCorrection, ContactChannel,
    ContactOutcome, CorrectCallOutcome, LogContactAttempt, ReceiveInquiry, ReceiveInquiryOutcome,
};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::inquiry::parse::Source;
use crm_api::domain::intake::IntakeActor;
use crm_api::domain::telephony::{settle, Signal};
use crm_api::ids::{CallId, CorrelationId, CorrespondenceRawId, OrganizationId, PersonId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "correct horse battery staple";

// --- Shared fixture helpers ------------------------------------------------

fn ts(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
        .unwrap()
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

struct PersonActivity {
    last_inquiry_at: Option<DateTime<Utc>>,
    last_contact_at: Option<DateTime<Utc>>,
    last_inbound_at: Option<DateTime<Utc>>,
    last_outbound_at: Option<DateTime<Utc>>,
}

async fn read_activity(pool: &PgPool, person_id: Uuid) -> PersonActivity {
    let row = sqlx::query(
        "SELECT last_inquiry_at, last_contact_at, last_inbound_at, last_outbound_at \
         FROM person WHERE id = $1",
    )
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap();
    PersonActivity {
        last_inquiry_at: row.get("last_inquiry_at"),
        last_contact_at: row.get("last_contact_at"),
        last_inbound_at: row.get("last_inbound_at"),
        last_outbound_at: row.get("last_outbound_at"),
    }
}

async fn read_xmin(pool: &PgPool, person_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT xmin::text::bigint FROM person WHERE id = $1")
        .bind(person_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_updated_at(pool: &PgPool, person_id: Uuid) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT updated_at FROM person WHERE id = $1")
        .bind(person_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Creates a Person by receiving a fresh inquiry directly through the
/// typed `receive_inquiry` command (bypassing HTTP, which always stamps
/// `received_at = Utc::now()`), so the test controls `received_at`
/// exactly — needed for the out-of-order/backdated inquiry cases.
async fn receive_inquiry_at(
    pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    email: &str,
    received_at: DateTime<Utc>,
) -> ReceiveInquiryOutcome {
    let key = crate::common::test_config().raw_payload_key;
    let publisher = Publisher::recording();
    let actor = IntakeActor::User(command_context(organization_id, actor_user_id));
    // The message embeds `received_at` and a fresh nonce so two calls to
    // this helper for the SAME email (a repeat-inquiry test) never
    // produce byte-identical payloads: `receive_inquiry` hashes the raw
    // payload bytes (`content_hmac`) to detect an idempotent replay, and a
    // byte-identical resubmission is treated as a duplicate (no new
    // `inquiry` row) — exactly the ordinary, correct dedup behavior, not
    // something this fixture wants to trigger between deliberately
    // distinct repeat inquiries.
    let payload = serde_json::json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "email": email,
        "message": format!("Interested at {received_at} ({})", Uuid::new_v4()),
    });
    let cmd = ReceiveInquiry {
        source: Source::parse("website").unwrap(),
        payload: serde_json::to_vec(&payload).unwrap(),
        assign_to_user_id: None,
        received_at,
    };
    commands::receive_inquiry(pool, &key, &publisher, &actor, cmd)
        .await
        .unwrap()
}

fn resolved_person_id(outcome: ReceiveInquiryOutcome) -> Uuid {
    match outcome {
        ReceiveInquiryOutcome::Resolved { person_id, .. } => person_id.0,
        ReceiveInquiryOutcome::Unresolved { .. } => panic!("expected Resolved, got Unresolved"),
    }
}

async fn insert_correspondence_raw(pool: &PgPool, organization_id: Uuid) -> CorrespondenceRawId {
    let id: Uuid = sqlx::query_scalar(
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
    CorrespondenceRawId::new(id)
}

/// Mirrors `db_today_source_filter_parity.rs`'s `capture_fact` helper: the
/// production capture pipeline function `insert_fact_and_maybe_attempt`
/// writes the `correspondence_captured` row (and, for a NEW outbound row,
/// the automatic `contact_attempted`). `message_id` is a parameter (not
/// always freshly generated) so a test can force the per-Person dedup
/// path.
#[allow(clippy::too_many_arguments)]
async fn capture_fact(
    app_pool: &PgPool,
    organization_id: Uuid,
    agent_user_id: Uuid,
    person_id: Uuid,
    direction: Direction,
    occurred_at: DateTime<Utc>,
    backdated: bool,
    message_id: Option<String>,
) -> Option<Uuid> {
    let raw_id = insert_correspondence_raw(app_pool, organization_id).await;
    let metadata = ParsedMetadata {
        via: if backdated { Via::Forward } else { Via::Cc },
        forward_style: None,
        forward_depth: if backdated { 1 } else { 0 },
        occurred_at,
        backdated,
        message_id,
        thread_key: None,
        working_from: None,
        working_to_cc: Vec::new(),
    };
    let mut tx = app_pool.begin().await.unwrap();
    let id = insert_fact_and_maybe_attempt(
        &mut tx,
        OrganizationId::new(organization_id),
        UserId::new(agent_user_id),
        PersonId::new(person_id),
        direction,
        &metadata,
        raw_id,
        CorrelationId::new(Uuid::new_v4()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// Inserts a `call` row directly (the call *aggregate* row itself is
/// ordinary mutable state, not a fact table — Slice 006 §2 — so a fixture
/// insert here is the established pattern, e.g. `db_calls.rs`'s sweep
/// tests). `contact_method_id` is bare (no FK, migration `20260825000001`)
/// so any uuid is valid.
async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    status: &str,
) -> Uuid {
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
           (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
            correlation_id, status, provider, provider_room, placed_at, ringing_at) \
         VALUES ($1, $2, $3, $4, $5, 'web_session', $6, $7, 'scripted', $8, now(), now())",
    )
    .bind(call_id)
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(caller_user_id)
    .bind(Uuid::new_v4())
    .bind(status)
    .bind(format!("call:{call_id}"))
    .execute(pool)
    .await
    .unwrap();
    call_id
}

// --- Step 1: the migration's backfill block (spec §8.1) --------------------

const MIGRATION: &str = include_str!("../migrations/20260910000001_person_last_activity.sql");

/// Extracts the text strictly between the `BEGIN`/`END
/// PERSON_LAST_ACTIVITY_BACKFILL` marker comments — the exact block the
/// migration also runs, and the block a future erasure runbook re-runs
/// for an affected Person (spec §1 rule 4, §2).
fn extract_backfill_block() -> String {
    const BEGIN: &str = "-- BEGIN PERSON_LAST_ACTIVITY_BACKFILL";
    const END: &str = "-- END PERSON_LAST_ACTIVITY_BACKFILL";
    let start = MIGRATION.find(BEGIN).expect("begin marker present") + BEGIN.len();
    let rest = &MIGRATION[start..];
    let end = rest.find(END).expect("end marker present");
    rest[..end].trim().to_string()
}

async fn insert_inquiry_row(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    received_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'zillow', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempted_row(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, \
            channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, $3, $4, 'call', 'no_answer')",
    )
    .bind(organization_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_correspondence_row(
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
           (organization_id, actor_kind, on_behalf_of_user_id, origin, occurred_at, \
            correlation_id, person_id, agent_user_id, direction, message_id, via, \
            correspondence_raw_id, backdated) \
         VALUES ($1, 'system', $2, 'webhook', $3, $4, $5, $2, $6, $7, 'cc', $8, false)",
    )
    .bind(organization_id)
    .bind(agent_user_id)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(direction)
    .bind(format!("backfill-{}@fixture.test", Uuid::new_v4()))
    .bind(raw_id)
    .execute(pool)
    .await
    .unwrap();
}

/// docs/specs/SLICE_012.md §8.1: the backfill block, extracted from the
/// migration by its markers, re-run over seeded history with the columns
/// nulled, leaves every column equal to `max(history)` for every Person
/// and NULL for a zero-history Person — across TWO Organizations (proving
/// the block's own `organization_id` correlation, not just `person_id`).
#[sqlx::test]
#[ignore]
async fn migration_backfill_block_recomputes_every_column_as_max_of_history(migrator_pool: PgPool) {
    let (org_a, user_a) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Backfill Realty A",
        "a@backfill.test",
        "Alice",
        PW,
    )
    .await;
    let (org_b, user_b) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Backfill Realty B",
        "b@backfill.test",
        "Bob",
        PW,
    )
    .await;
    let stage_a = first_stage_id(&migrator_pool, org_a).await;
    let stage_b = first_stage_id(&migrator_pool, org_b).await;

    let insert_person = |pool: PgPool, org: Uuid, stage: Uuid| async move {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO person (organization_id, first_name, stage_id) \
             VALUES ($1, 'Fixture', $2) RETURNING id",
        )
        .bind(org)
        .bind(stage)
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // Org A: one Person with rich, out-of-order history across all three
    // fact tables (several rows per axis, so `max()` is a real reduction,
    // not a single-row coincidence).
    let rich_a = insert_person(migrator_pool.clone(), org_a, stage_a).await;
    insert_inquiry_row(&migrator_pool, org_a, rich_a, ts(2026, 1, 1, 9, 0, 0)).await;
    insert_inquiry_row(&migrator_pool, org_a, rich_a, ts(2026, 3, 1, 9, 0, 0)).await;
    insert_inquiry_row(&migrator_pool, org_a, rich_a, ts(2026, 2, 1, 9, 0, 0)).await;
    insert_contact_attempted_row(&migrator_pool, org_a, rich_a, ts(2026, 1, 5, 9, 0, 0)).await;
    insert_contact_attempted_row(&migrator_pool, org_a, rich_a, ts(2026, 2, 15, 9, 0, 0)).await;
    insert_correspondence_row(
        &migrator_pool,
        org_a,
        rich_a,
        user_a,
        "inbound",
        ts(2026, 1, 10, 9, 0, 0),
    )
    .await;
    insert_correspondence_row(
        &migrator_pool,
        org_a,
        rich_a,
        user_a,
        "inbound",
        ts(2026, 2, 20, 9, 0, 0),
    )
    .await;
    insert_correspondence_row(
        &migrator_pool,
        org_a,
        rich_a,
        user_a,
        "outbound",
        ts(2026, 1, 12, 9, 0, 0),
    )
    .await;

    // Org A: a zero-history Person — must backfill to all-NULL.
    let empty_a = insert_person(migrator_pool.clone(), org_a, stage_a).await;

    // Org B: one Person with a thinner history, proving the block is not
    // accidentally scoped to a single Organization.
    let rich_b = insert_person(migrator_pool.clone(), org_b, stage_b).await;
    insert_inquiry_row(&migrator_pool, org_b, rich_b, ts(2026, 4, 1, 9, 0, 0)).await;
    insert_contact_attempted_row(&migrator_pool, org_b, rich_b, ts(2026, 4, 2, 9, 0, 0)).await;
    insert_correspondence_row(
        &migrator_pool,
        org_b,
        rich_b,
        user_b,
        "outbound",
        ts(2026, 4, 3, 9, 0, 0),
    )
    .await;

    // The four columns are already correct (the AFTER INSERT triggers
    // maintained them live as the rows above were inserted) — null them
    // out as the migrator role to simulate the pre-migration state a
    // fresh `sqlx::test` database (migrated empty) never actually
    // exercises, then re-run the extracted block.
    sqlx::query(
        "UPDATE person SET last_inquiry_at = NULL, last_contact_at = NULL, \
         last_inbound_at = NULL, last_outbound_at = NULL \
         WHERE id = ANY($1)",
    )
    .bind([rich_a, empty_a, rich_b].as_slice())
    .execute(&migrator_pool)
    .await
    .unwrap();
    for id in [rich_a, empty_a, rich_b] {
        let activity = read_activity(&migrator_pool, id).await;
        assert!(activity.last_inquiry_at.is_none());
        assert!(activity.last_contact_at.is_none());
        assert!(activity.last_inbound_at.is_none());
        assert!(activity.last_outbound_at.is_none());
    }

    let block = extract_backfill_block();
    // The markers wrap the block's own explanatory comment as well as the
    // executable statement (matching the spec §2 example verbatim), so the
    // extracted text starts with that comment, not the `UPDATE` keyword —
    // `contains`, not `starts_with`, is the right check here.
    assert!(
        block.contains("UPDATE person p"),
        "extracted block: {block}"
    );
    let people_in_fixture = 3;
    let start = std::time::Instant::now();
    sqlx::query(&block).execute(&migrator_pool).await.unwrap();
    let backfill_duration = start.elapsed();
    eprintln!(
        "Slice 012 backfill block duration over {people_in_fixture} People (test fixture): \
         {backfill_duration:?}"
    );

    let rich_a_activity = read_activity(&migrator_pool, rich_a).await;
    assert_eq!(
        rich_a_activity.last_inquiry_at,
        Some(ts(2026, 3, 1, 9, 0, 0))
    );
    assert_eq!(
        rich_a_activity.last_contact_at,
        Some(ts(2026, 2, 15, 9, 0, 0))
    );
    assert_eq!(
        rich_a_activity.last_inbound_at,
        Some(ts(2026, 2, 20, 9, 0, 0))
    );
    assert_eq!(
        rich_a_activity.last_outbound_at,
        Some(ts(2026, 1, 12, 9, 0, 0))
    );

    let empty_activity = read_activity(&migrator_pool, empty_a).await;
    assert!(empty_activity.last_inquiry_at.is_none());
    assert!(empty_activity.last_contact_at.is_none());
    assert!(empty_activity.last_inbound_at.is_none());
    assert!(empty_activity.last_outbound_at.is_none());

    let rich_b_activity = read_activity(&migrator_pool, rich_b).await;
    assert_eq!(
        rich_b_activity.last_inquiry_at,
        Some(ts(2026, 4, 1, 9, 0, 0))
    );
    assert_eq!(
        rich_b_activity.last_contact_at,
        Some(ts(2026, 4, 2, 9, 0, 0))
    );
    assert_eq!(rich_b_activity.last_inbound_at, None);
    assert_eq!(
        rich_b_activity.last_outbound_at,
        Some(ts(2026, 4, 3, 9, 0, 0))
    );
}

// --- Step 2: the invariant through the typed commands (spec §8.2–8.4, 8.7, 8.8) --

/// `LogContactAttempt` advances `last_contact_at` to the logged attempt's
/// `occurred_at`; the other three columns are untouched.
#[sqlx::test]
#[ignore]
async fn log_contact_attempt_advances_last_contact_at(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Log Attempt Realty",
        "log@attempt.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@log-attempt.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);
    let updated_before = read_updated_at(&migrator_pool, person_id).await;

    let publisher = Publisher::recording();
    let ctx = command_context(org_id, actor_id);
    let (_, attempt) = log_contact_attempt(
        &app_pool,
        &publisher,
        &ctx,
        LogContactAttempt {
            person_id: PersonId::new(person_id),
            channel: ContactChannel::Call,
            outcome: ContactOutcome::NoAnswer,
        },
    )
    .await
    .unwrap();

    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_contact_at, Some(attempt.occurred_at));
    assert_eq!(activity.last_inquiry_at, Some(ts(2026, 5, 1, 9, 0, 0)));
    assert!(activity.last_inbound_at.is_none());
    assert!(activity.last_outbound_at.is_none());

    // Spec §8.8: person.updated_at is untouched by history inserts (only
    // assignment/stage changes set it).
    let updated_after = read_updated_at(&migrator_pool, person_id).await;
    assert_eq!(updated_before, updated_after);
}

/// The call `settle` write path (D-031): an `Answered` transition writes a
/// `reached` attempt, which advances `last_contact_at`.
#[sqlx::test]
#[ignore]
async fn call_settle_answered_advances_last_contact_at(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Call Settle Realty",
        "settle@call.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@call-settle.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);
    let call_id = insert_call(&app_pool, org_id, person_id, actor_id, "ringing").await;

    let publisher = Publisher::recording();
    let now = ts(2026, 5, 2, 10, 0, 0);
    let result = settle(
        &app_pool,
        &publisher,
        OrganizationId::new(org_id),
        CallId::new(call_id),
        &Signal::Answered { call_ref: None },
        now,
    )
    .await
    .unwrap();
    assert!(result.is_some());

    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_contact_at, Some(now));
}

/// SLICE_006c outcome correction: the correction row inherits the head
/// attempt's `occurred_at` (never later), so it never advances
/// `last_contact_at` — the D-052 guard makes this a database no-op, not
/// merely an application-level "same value" coincidence.
#[sqlx::test]
#[ignore]
async fn outcome_correction_does_not_change_last_contact_at(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Correction Realty",
        "correct@call.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@correction.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);
    let call_id = insert_call(&app_pool, org_id, person_id, actor_id, "ringing").await;
    let publisher = Publisher::recording();

    // Answer (writes the automatic "reached" attempt), then end the call.
    let answered_at = ts(2026, 5, 2, 10, 0, 0);
    settle(
        &app_pool,
        &publisher,
        OrganizationId::new(org_id),
        CallId::new(call_id),
        &Signal::Answered { call_ref: None },
        answered_at,
    )
    .await
    .unwrap()
    .unwrap();
    settle(
        &app_pool,
        &publisher,
        OrganizationId::new(org_id),
        CallId::new(call_id),
        &Signal::AgentHangup,
        ts(2026, 5, 2, 10, 5, 0),
    )
    .await
    .unwrap()
    .unwrap();

    let activity_before = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity_before.last_contact_at, Some(answered_at));
    let xmin_before = read_xmin(&migrator_pool, person_id).await;

    let ctx = command_context(org_id, actor_id);
    let result = correct_call_outcome(
        &app_pool,
        &publisher,
        &ctx,
        CorrectCallOutcome {
            call_id: CallId::new(call_id),
            outcome: CallOutcomeCorrection::NoAnswer,
        },
    )
    .await
    .unwrap();
    assert!(result.changed);

    let activity_after = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity_after.last_contact_at, Some(answered_at));
    let xmin_after = read_xmin(&migrator_pool, person_id).await;
    assert_eq!(
        xmin_before, xmin_after,
        "a correction inheriting occurred_at must not write a new person tuple"
    );
}

/// An outbound capture writes both the `correspondence_captured` row and
/// (D-042.4) the automatic `contact_attempted` — both `last_outbound_at`
/// and `last_contact_at` advance; `last_inbound_at` stays untouched.
#[sqlx::test]
#[ignore]
async fn outbound_capture_advances_contact_and_outbound_not_inbound(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Outbound Realty",
        "outbound@capture.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@outbound-capture.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);

    let occurred_at = ts(2026, 5, 3, 8, 0, 0);
    let fact_id = capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Outbound,
        occurred_at,
        false,
        Some(format!("outbound-{}@fixture.test", Uuid::new_v4())),
    )
    .await;
    assert!(fact_id.is_some());

    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_outbound_at, Some(occurred_at));
    assert_eq!(activity.last_contact_at, Some(occurred_at));
    assert!(activity.last_inbound_at.is_none());
}

/// A deduplicated outbound capture (`ON CONFLICT (organization_id,
/// person_id, message_id) DO NOTHING`) writes no new row and no automatic
/// attempt — neither `last_outbound_at` nor `last_contact_at` advances,
/// even though the duplicate attempt names a strictly later time.
#[sqlx::test]
#[ignore]
async fn deduplicated_outbound_capture_advances_nothing(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Dedup Realty",
        "dedup@capture.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@dedup-capture.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);

    let message_id = format!("dedup-{}@fixture.test", Uuid::new_v4());
    let first_at = ts(2026, 5, 3, 8, 0, 0);
    let first = capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Outbound,
        first_at,
        false,
        Some(message_id.clone()),
    )
    .await;
    assert!(first.is_some());

    // Same (organization_id, person_id, message_id) — dedupes, even
    // though this second attempt names a LATER occurred_at.
    let second_at = ts(2026, 5, 4, 8, 0, 0);
    let second = capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Outbound,
        second_at,
        false,
        Some(message_id),
    )
    .await;
    assert!(second.is_none(), "the dedup insert must return None");

    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_outbound_at, Some(first_at));
    assert_eq!(activity.last_contact_at, Some(first_at));
}

/// An inbound capture advances only `last_inbound_at` — it writes no
/// automatic `contact_attempted` (that only happens for a NEW outbound
/// row, D-042.4).
#[sqlx::test]
#[ignore]
async fn inbound_capture_advances_last_inbound_at_only(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Inbound Realty",
        "inbound@capture.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@inbound-capture.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);

    let occurred_at = ts(2026, 5, 3, 8, 0, 0);
    let fact_id = capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Inbound,
        occurred_at,
        false,
        Some(format!("inbound-{}@fixture.test", Uuid::new_v4())),
    )
    .await;
    assert!(fact_id.is_some());

    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_inbound_at, Some(occurred_at));
    assert!(activity.last_contact_at.is_none());
    assert!(activity.last_outbound_at.is_none());
}

/// D-042 retroactive-forward: a backdated inbound capture EARLIER than the
/// current `last_inbound_at` is a database no-op (the Person row's `xmin`
/// is unchanged, spec §8.7); one LATER than the current maximum advances
/// it normally.
#[sqlx::test]
#[ignore]
async fn backdated_forward_earlier_is_noop_later_advances(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Backdated Realty",
        "backdated@capture.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@backdated-capture.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);

    let mid = ts(2026, 5, 10, 9, 0, 0);
    capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Inbound,
        mid,
        false,
        Some(format!("mid-{}@fixture.test", Uuid::new_v4())),
    )
    .await
    .expect("fresh fixture correspondence must create a fact");

    let activity_before = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity_before.last_inbound_at, Some(mid));
    let xmin_before = read_xmin(&migrator_pool, person_id).await;

    // Earlier than `mid`: a backdated forward whose Date header predates
    // the current maximum — guarded out at the trigger, zero rows
    // updated, no new person tuple.
    let earlier = ts(2026, 5, 5, 9, 0, 0);
    capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Inbound,
        earlier,
        true,
        Some(format!("earlier-{}@fixture.test", Uuid::new_v4())),
    )
    .await
    .expect("fresh fixture correspondence must create a fact");

    let activity_after_earlier = read_activity(&migrator_pool, person_id).await;
    assert_eq!(
        activity_after_earlier.last_inbound_at,
        Some(mid),
        "an earlier backdated forward must not move the maximum backwards"
    );
    let xmin_after_earlier = read_xmin(&migrator_pool, person_id).await;
    assert_eq!(
        xmin_before, xmin_after_earlier,
        "a non-advancing trigger UPDATE must produce no new person tuple"
    );

    // Later than `mid`: a backdated forward whose Date header is still
    // after the current maximum — advances normally.
    let later = ts(2026, 5, 15, 9, 0, 0);
    capture_fact(
        &app_pool,
        org_id,
        actor_id,
        person_id,
        Direction::Inbound,
        later,
        true,
        Some(format!("later-{}@fixture.test", Uuid::new_v4())),
    )
    .await
    .expect("fresh fixture correspondence must create a fact");

    let activity_after_later = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity_after_later.last_inbound_at, Some(later));
}

/// `ReceiveInquiry`: a first inquiry sets `last_inquiry_at`; a repeat
/// inquiry with an out-of-order EARLIER `received_at` does not move it
/// backwards; a later repeat advances it.
#[sqlx::test]
#[ignore]
async fn first_and_repeat_inquiry_including_out_of_order_earlier_received_at(
    migrator_pool: PgPool,
) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Repeat Inquiry Realty",
        "repeat@inquiry.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let email = "lead@repeat-inquiry.test";

    let first_at = ts(2026, 5, 10, 9, 0, 0);
    let outcome = receive_inquiry_at(&app_pool, org_id, actor_id, email, first_at).await;
    let person_id = resolved_person_id(outcome);
    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_inquiry_at, Some(first_at));

    // A repeat inquiry (same email, so it matches the same Person) with an
    // EARLIER received_at than the first — must not move the maximum
    // backwards.
    let earlier_repeat_at = ts(2026, 5, 5, 9, 0, 0);
    let repeat_outcome =
        receive_inquiry_at(&app_pool, org_id, actor_id, email, earlier_repeat_at).await;
    assert_eq!(resolved_person_id(repeat_outcome), person_id);
    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(
        activity.last_inquiry_at,
        Some(first_at),
        "an out-of-order earlier repeat inquiry must not move last_inquiry_at backwards"
    );

    // A repeat inquiry LATER than the current maximum — advances.
    let later_repeat_at = ts(2026, 5, 20, 9, 0, 0);
    let repeat_outcome =
        receive_inquiry_at(&app_pool, org_id, actor_id, email, later_repeat_at).await;
    assert_eq!(resolved_person_id(repeat_outcome), person_id);
    let activity = read_activity(&migrator_pool, person_id).await;
    assert_eq!(activity.last_inquiry_at, Some(later_repeat_at));
}

/// Spec §3: `GREATEST` is monotone, so commit order is irrelevant — under
/// READ COMMITTED the second updater waits on the Person row lock (held by
/// the first transaction's own trigger `UPDATE`) and re-evaluates against
/// the now-committed row. Proven for BOTH commit orders: the
/// chronologically earlier attempt committing first (then a later one),
/// and the later attempt committing first (then an earlier one) — either
/// way the final `last_contact_at` is the greater of the two.
#[sqlx::test]
#[ignore]
async fn concurrent_attempts_in_either_commit_order_leave_the_greater(migrator_pool: PgPool) {
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Concurrency Realty",
        "concurrency@attempts.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let insert_attempt = |pool: PgPool, org: Uuid, person: Uuid, occurred_at: DateTime<Utc>| async move {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO contact_attempted \
               (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, \
                channel, outcome) \
             VALUES ($1, 'system', 'migration', $2, $3, $4, 'call', 'no_answer')",
        )
        .bind(org)
        .bind(occurred_at)
        .bind(Uuid::new_v4())
        .bind(person)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx
    };

    // Case A: the chronologically earlier attempt's transaction commits
    // first, then the later one.
    let outcome_a = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@concurrency-a.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_a = resolved_person_id(outcome_a);
    let early = ts(2026, 5, 10, 9, 0, 0);
    let late = ts(2026, 5, 20, 9, 0, 0);

    let tx_early = insert_attempt(app_pool.clone(), org_id, person_a, early).await;
    let pool_for_task = app_pool.clone();
    let task = tokio::spawn(async move {
        // Blocks on the Person row lock the still-open `tx_early` holds
        // (via its own trigger UPDATE) until `tx_early` commits below.
        let tx_late = insert_attempt(pool_for_task, org_id, person_a, late).await;
        tx_late.commit().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    tx_early.commit().await.unwrap();
    task.await.unwrap();

    let activity_a = read_activity(&migrator_pool, person_a).await;
    assert_eq!(
        activity_a.last_contact_at,
        Some(late),
        "case A: earlier-committed-first must still leave the greater timestamp"
    );

    // Case B: the chronologically LATER attempt's transaction commits
    // first, then the earlier one — must still leave the greater.
    let outcome_b = receive_inquiry_at(
        &app_pool,
        org_id,
        actor_id,
        "lead@concurrency-b.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_b = resolved_person_id(outcome_b);

    let tx_late = insert_attempt(app_pool.clone(), org_id, person_b, late).await;
    let pool_for_task = app_pool.clone();
    let task = tokio::spawn(async move {
        let tx_early = insert_attempt(pool_for_task, org_id, person_b, early).await;
        tx_early.commit().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    tx_late.commit().await.unwrap();
    task.await.unwrap();

    let activity_b = read_activity(&migrator_pool, person_b).await;
    assert_eq!(
        activity_b.last_contact_at,
        Some(late),
        "case B: later-committed-first must still leave the greater timestamp"
    );
}

/// Defence in depth (spec §2): the trigger matches both `id` AND
/// `organization_id`, so a history row whose `organization_id` differs
/// from the Person's own updates nothing, even though `person_id` alone
/// matches. Written via direct fixture SQL (the composite FKs and the
/// application layer already forbid such a row through any typed command
/// — this proves the trigger's own defence-in-depth independently).
#[sqlx::test]
#[ignore]
async fn history_row_with_mismatched_organization_id_updates_nothing(migrator_pool: PgPool) {
    let (org_a, actor_a) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Isolation Realty A",
        "a@isolation.test",
        "Alice",
        PW,
    )
    .await;
    let (org_b, _actor_b) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Isolation Realty B",
        "b@isolation.test",
        "Bob",
        PW,
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let outcome = receive_inquiry_at(
        &app_pool,
        org_a,
        actor_a,
        "lead@isolation.test",
        ts(2026, 5, 1, 9, 0, 0),
    )
    .await;
    let person_id = resolved_person_id(outcome);
    let activity_before = read_activity(&migrator_pool, person_id).await;
    assert!(activity_before.last_contact_at.is_none());

    // A contact_attempted row naming org_a's Person id but org_b's
    // organization_id — allowed at the database level (person_id is a
    // bare, unFK'd column on every fact table), and exactly the case the
    // trigger's own organization_id match must reject.
    sqlx::query(
        "INSERT INTO contact_attempted \
           (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, \
            channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, $3, $4, 'call', 'no_answer')",
    )
    .bind(org_b)
    .bind(ts(2026, 5, 2, 9, 0, 0))
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let activity_after = read_activity(&migrator_pool, person_id).await;
    assert!(
        activity_after.last_contact_at.is_none(),
        "a cross-Organization history row must update nothing"
    );
}

//! Shared Slice 011d system-feed fixture helpers (item 3 of the LATER
//! batch, docs/tasks/LATER_BATCH_2026-09-08.md). Used by
//! `db_today_system_feed_commands.rs`, `db_today_system_feed_evaluation.rs`
//! and `db_today_system_feed_preview.rs` — moved here because all three
//! need them. No test bodies changed; this is exactly the code that used
//! to live at the top of `db_today_system_feed_commands.rs`. Run only via
//! ./scripts/check-db.
#![allow(dead_code)]

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
};
use crm_api::domain::today::system_feeds::FeedKey;
use crm_api::ids::{CorrelationId, OrganizationId, UserId};

pub const PW: &str = "correct horse battery staple";

pub fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

/// A full fixture: one Organization, nine stages, and one ADMIN member —
/// `crate::common::create_org_with_stages_and_member` seeds a plain
/// member, so today-feed command tests (which need an admin actor) build
/// the equivalent themselves via the same lower-level helpers.
pub async fn create_org_with_admin(pool: &PgPool, org_name: &str, email: &str) -> (Uuid, Uuid) {
    let org_id = crate::common::create_org(pool, org_name).await;
    crate::common::seed_stages(pool, org_id).await;
    let user_id = crate::common::create_user(pool, email, "Admin", PW).await;
    crate::common::add_membership_with(
        pool,
        org_id,
        user_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    (org_id, user_id)
}

pub fn canonical_unanswered_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: vec![
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![Assignee::Me],
            }),
            Clause::AwaitingResponse(BoolClause { value: true }),
        ],
    }
}

pub async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn insert_person(
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

pub async fn insert_inquiry(pool: &PgPool, organization_id: Uuid, person_id: Uuid) {
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'commands-fixture', $4)",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Utc::now() - ChronoDuration::hours(2))
    .execute(pool)
    .await
    .unwrap();
}

pub async fn insert_correspondence(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    agent_user_id: Uuid,
    direction: &str,
    occurred_at: chrono::DateTime<Utc>,
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

/// A qualifying call (ended/failed with a non-null `ended_at`, and an
/// uncorrected automatic `contact_attempted` root) — the exact
/// `outcome_call` membership shape `call_membership.sql`/`call_only.sql`
/// require, mirroring `db_today_feed_equivalence.rs`'s `insert_call`.
pub async fn insert_call(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    caller_user_id: Uuid,
    ended_at: chrono::DateTime<Utc>,
) -> Uuid {
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call \
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin, \
             correlation_id, status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES \
            ($1, $2, $3, $4, $5, 'web_session', $6, 'ended', 'agent_hangup', \
             'scripted', 'commands-fixture', $7, $7)",
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

pub fn feed_row_key(feed_key: FeedKey) -> &'static str {
    feed_key.as_str()
}

pub async fn feed_row(
    pool: &PgPool,
    organization_id: Uuid,
    feed_key: FeedKey,
) -> (bool, Option<String>, Option<i32>, i64) {
    sqlx::query_as(
        "SELECT enabled, filter::text, fresh_within_hours, revision FROM today_system_feed \
         WHERE organization_id = $1 AND feed_key = $2",
    )
    .bind(organization_id)
    .bind(feed_row_key(feed_key))
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn fact_count(pool: &PgPool, organization_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM today_feed_changed WHERE organization_id = $1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

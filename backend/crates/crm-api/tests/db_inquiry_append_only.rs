//! DB-backed tests for the LATER item recorded in
//! docs/specs/SLICE_012.md's "Recorded LATER (D-050)": `inquiry` gains an
//! append-only trigger like the four D-015 §8 fact tables, but — unlike
//! them — it must still let a Person's `ON DELETE CASCADE` erase its
//! inquiries (docs/specs SLICE_002.md §2, D-015 §5). See
//! 20260911000001_inquiry_append_only.sql for the design (a cascade-aware
//! `reject_direct_mutation()`, not the fact tables' unconditional
//! `reject_mutation()`). Run only via ./scripts/check-db.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

const PW: &str = "correct horse battery staple";

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn create_person_row(pool: &PgPool, organization_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_inquiry_row(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    received_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'zillow', $4) RETURNING id",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(received_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn last_inquiry_at(pool: &PgPool, person_id: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT last_inquiry_at FROM person WHERE id = $1")
        .bind(person_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A direct `UPDATE` of `inquiry` (the migrator role, matching
/// `db_operator.rs`'s former backdate pattern and any future script) is
/// always rejected — there is no cascade case for `UPDATE`, so
/// `reject_direct_mutation()` raises unconditionally on that `TG_OP`.
#[sqlx::test]
#[ignore]
async fn direct_update_of_inquiry_is_rejected(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = create_person_row(&migrator_pool, org_id, stage_id).await;
    insert_inquiry_row(&migrator_pool, org_id, person_id, Utc::now()).await;

    let result = sqlx::query("UPDATE inquiry SET received_at = now() WHERE person_id = $1")
        .bind(person_id)
        .execute(&migrator_pool)
        .await;
    assert!(
        result.is_err(),
        "a direct UPDATE of inquiry must be rejected by the append-only trigger"
    );
}

/// A direct `DELETE FROM inquiry ...` (depth 0 — not inside the `person`
/// FK's cascade) is rejected exactly like `UPDATE`.
#[sqlx::test]
#[ignore]
async fn direct_delete_of_inquiry_is_rejected(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = create_person_row(&migrator_pool, org_id, stage_id).await;
    let inquiry_id = insert_inquiry_row(&migrator_pool, org_id, person_id, Utc::now()).await;

    let result = sqlx::query("DELETE FROM inquiry WHERE id = $1")
        .bind(inquiry_id)
        .execute(&migrator_pool)
        .await;
    assert!(
        result.is_err(),
        "a direct DELETE of inquiry must be rejected by the append-only trigger"
    );
}

/// The checkpoint finding this migration fixes: deleting a Person must
/// still cascade-delete its inquiries — the whole reason
/// `reject_direct_mutation()` exists instead of the fact tables'
/// unconditional `reject_mutation()`.
#[sqlx::test]
#[ignore]
async fn deleting_a_person_still_cascades_its_inquiries(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = create_person_row(&migrator_pool, org_id, stage_id).await;
    let inquiry_id = insert_inquiry_row(&migrator_pool, org_id, person_id, Utc::now()).await;

    sqlx::query("DELETE FROM person WHERE id = $1")
        .bind(person_id)
        .execute(&migrator_pool)
        .await
        .expect("cascading a Person delete through inquiry must succeed");

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM inquiry WHERE id = $1")
        .bind(inquiry_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(
        remaining, 0,
        "the cascaded inquiry row must actually be gone"
    );
    let person_gone: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE id = $1")
        .bind(person_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(person_gone, 0);
}

/// Same Person delete, but also carrying a `person_tag` row (an ordinary
/// relational `ON DELETE CASCADE`, no append-only trigger at all) — the
/// new `inquiry` trigger must not interfere with an unrelated cascade
/// running in the same statement.
#[sqlx::test]
#[ignore]
async fn person_delete_cascades_inquiry_and_person_tag_together(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let user_id = crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", PW).await;
    crate::common::add_membership(&migrator_pool, org_id, user_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = create_person_row(&migrator_pool, org_id, stage_id).await;
    insert_inquiry_row(&migrator_pool, org_id, person_id, Utc::now()).await;

    let tag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tag (organization_id, name, created_by_user_id) \
         VALUES ($1, 'VIP', $2) RETURNING id",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO person_tag (organization_id, person_id, tag_id, added_by_user_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(tag_id)
    .bind(user_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM person WHERE id = $1")
        .bind(person_id)
        .execute(&migrator_pool)
        .await
        .expect("both cascades (inquiry and person_tag) must succeed together");

    let remaining_tags: i64 =
        sqlx::query_scalar("SELECT count(*) FROM person_tag WHERE tag_id = $1")
            .bind(tag_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        remaining_tags, 0,
        "person_tag's own cascade must be unaffected"
    );
}

/// A direct `INSERT` (the application path, and — after this migration —
/// `db_operator.rs`'s backdate fixture) still works, and the Slice 012
/// `inquiry_touch_person` trigger still maintains `last_inquiry_at` from
/// it (an `AFTER INSERT` trigger, untouched by this migration's `BEFORE
/// UPDATE OR DELETE`/`BEFORE TRUNCATE` triggers on the same table).
#[sqlx::test]
#[ignore]
async fn a_backdated_insert_still_works_and_last_inquiry_at_is_trigger_maintained(
    migrator_pool: PgPool,
) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;
    let person_id = create_person_row(&migrator_pool, org_id, stage_id).await;
    assert_eq!(last_inquiry_at(&migrator_pool, person_id).await, None);

    let backdated = Utc::now() - chrono::Duration::days(2);
    insert_inquiry_row(&migrator_pool, org_id, person_id, backdated).await;

    let observed = last_inquiry_at(&migrator_pool, person_id).await;
    assert_eq!(
        observed.map(|ts| ts.timestamp_micros()),
        Some(backdated.timestamp_micros()),
        "inquiry_touch_person must still fire on a direct backdated INSERT"
    );
}

//! DB-backed schema/seed/backfill/read tests for Slice 011d's persistence
//! layer (docs/specs/SLICE_011d.md §3, §9.3 schema parts). Commands,
//! preview, and routes are a later round (brief step 4); this file covers
//! only what step 2 delivers: the migration, `create_organization` seeding,
//! and `system_feeds::load_feed_rows`'s fallback semantics. Grants and the
//! append-only fact enumeration live in `db_schema.rs`.

use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::today::system_feeds::{self, FeedKey, ALL_FEED_KEYS};
use crm_api::ids::OrganizationId;

async fn feed_rows(
    pool: &PgPool,
    organization_id: Uuid,
) -> Vec<(String, bool, Option<String>, Option<i32>, i64)> {
    sqlx::query_as(
        "SELECT feed_key, enabled, filter::text, fresh_within_hours, revision
         FROM today_system_feed WHERE organization_id = $1 ORDER BY feed_key",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// spec §9.3: seed on create — `create_organization` (via `crate::common::
/// create_org`, the same command) inserts exactly one row per feed key,
/// every one enabled with NULL definition columns (the canonical default).
#[sqlx::test]
#[ignore]
async fn seed_on_create_inserts_exactly_three_default_enabled_rows(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Feed seed org").await;

    let rows = feed_rows(&migrator_pool, org_id).await;
    assert_eq!(rows.len(), 3, "exactly one row per feed key");
    let mut keys: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "call_outcome_needed",
            "client_replied",
            "unanswered_inquiry"
        ]
    );
    for (_, enabled, filter, fresh_within_hours, revision) in &rows {
        assert!(*enabled, "seeded rows are enabled by default");
        assert!(filter.is_none(), "seeded rows carry no stored definition");
        assert!(revision == &1, "seeded rows start at revision 1");
        let _ = fresh_within_hours;
    }

    // Calling create_organization's seeding path again (a second
    // Organization) must not disturb the first — PRIMARY KEY
    // (organization_id, feed_key) plus ON CONFLICT DO NOTHING.
    let other_org_id = crate::common::create_org(&migrator_pool, "Feed seed org two").await;
    let other_rows = feed_rows(&migrator_pool, other_org_id).await;
    assert_eq!(other_rows.len(), 3);
    let first_org_rows_again = feed_rows(&migrator_pool, org_id).await;
    assert_eq!(first_org_rows_again.len(), 3);
}

/// spec §3: "The read path treats a missing row as canonical-enabled" — an
/// Organization created by an older binary after the migration (simulated
/// here by deleting the seeded rows via the migrator connection, since
/// crm_app itself has no DELETE grant on this table) still gets its rules.
#[sqlx::test]
#[ignore]
async fn missing_row_resolves_as_canonical_enabled(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Feed missing row org").await;
    sqlx::query("DELETE FROM today_system_feed WHERE organization_id = $1")
        .bind(org_id)
        .execute(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(feed_rows(&migrator_pool, org_id).await.len(), 0);

    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let mut conn = app_pool.acquire().await.unwrap();
    let resolved = system_feeds::load_feed_rows(&mut conn, OrganizationId::new(org_id))
        .await
        .unwrap();

    for (feed_key, resolved_feed) in ALL_FEED_KEYS.iter().zip(resolved.iter()) {
        assert_eq!(resolved_feed.feed_key, *feed_key);
        assert!(
            resolved_feed.enabled,
            "{:?}: missing row reads as enabled",
            feed_key
        );
        assert!(
            resolved_feed.is_default,
            "{:?}: missing row reads as default",
            feed_key
        );
        assert!(
            !resolved_feed.fallback,
            "{:?}: a missing row is not a fallback (there is nothing invalid to fall back from)",
            feed_key
        );
        assert_eq!(
            resolved_feed.filter,
            system_feeds::canonical_default(*feed_key),
            "{:?}: missing row's effective filter is the canonical default",
            feed_key
        );
        assert_eq!(
            resolved_feed.fresh_within_hours,
            system_feeds::canonical_fresh_within_hours(*feed_key)
        );
    }
}

/// spec §3 `is_default`: an explicit NULL/NULL row (the seeded state) is
/// default; a stored, structurally-valid, reference-valid definition is
/// neither default nor fallback, and IS the effective definition.
#[sqlx::test]
#[ignore]
async fn is_default_true_for_seeded_rows_false_for_a_valid_custom_definition(
    migrator_pool: PgPool,
) {
    let org_id = crate::common::create_org(&migrator_pool, "Feed is_default org").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let stage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    // Seeded (default) state.
    {
        let mut conn = app_pool.acquire().await.unwrap();
        let resolved = system_feeds::load_feed_rows(&mut conn, OrganizationId::new(org_id))
            .await
            .unwrap();
        for feed in &resolved {
            assert!(feed.is_default, "{:?}", feed.feed_key);
            assert!(!feed.fallback, "{:?}", feed.feed_key);
        }
    }

    // A valid custom definition for unanswered_inquiry (the anchor clause
    // plus an extra, valid stage clause) — not yet enforced by any command
    // in this round, so written directly for the read-path test.
    let custom_filter = serde_json::json!({
        "version": 1,
        "clauses": [
            {"kind": "assigned_to", "assignees": ["me"]},
            {"kind": "awaiting_response", "value": true},
            {"kind": "stage", "stage_ids": [stage_id]},
        ]
    });
    sqlx::query(
        "UPDATE today_system_feed SET filter = $1, revision = revision + 1
         WHERE organization_id = $2 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(&custom_filter)
    .bind(org_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let resolved = system_feeds::load_feed_rows(&mut conn, OrganizationId::new(org_id))
        .await
        .unwrap();
    let unanswered = resolved
        .iter()
        .find(|f| f.feed_key == FeedKey::UnansweredInquiry)
        .unwrap();
    assert!(
        !unanswered.is_default,
        "a non-NULL stored filter is not default"
    );
    assert!(
        !unanswered.fallback,
        "a valid stored filter is not a fallback"
    );
    assert_eq!(unanswered.filter.clauses.len(), 3);

    // The other two feeds remain untouched/default.
    for feed_key in [FeedKey::ClientReplied, FeedKey::CallOutcomeNeeded] {
        let feed = resolved.iter().find(|f| f.feed_key == feed_key).unwrap();
        assert!(feed.is_default, "{feed_key:?}");
        assert!(!feed.fallback, "{feed_key:?}");
    }
}

/// spec §1 rule 6 / §5 rule 1: a stored definition that references a stage
/// no longer valid in this Organization (never existed, in this fixture —
/// `stage` carries no DELETE grant for `crm_app`, so a cross-org id stands
/// in for "no longer valid") falls back to the canonical default with
/// `fallback: true`. Availability of the core queue beats strict skipping.
#[sqlx::test]
#[ignore]
async fn invalid_stage_reference_falls_back_to_canonical(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Feed fallback org").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let nonexistent_stage_id = Uuid::new_v4();
    let broken_filter = serde_json::json!({
        "version": 1,
        "clauses": [
            {"kind": "assigned_to", "assignees": ["me"]},
            {"kind": "awaiting_response", "value": true},
            {"kind": "stage", "stage_ids": [nonexistent_stage_id]},
        ]
    });
    sqlx::query(
        "UPDATE today_system_feed SET filter = $1, revision = revision + 1
         WHERE organization_id = $2 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(&broken_filter)
    .bind(org_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let resolved = system_feeds::load_feed_rows(&mut conn, OrganizationId::new(org_id))
        .await
        .unwrap();
    let unanswered = resolved
        .iter()
        .find(|f| f.feed_key == FeedKey::UnansweredInquiry)
        .unwrap();
    assert!(
        !unanswered.is_default,
        "the stored row is non-NULL, so is_default stays false even under fallback"
    );
    assert!(unanswered.fallback, "an invalid reference must fall back");
    assert_eq!(
        unanswered.filter,
        system_feeds::canonical_default(FeedKey::UnansweredInquiry),
        "the effective filter under fallback is the canonical default"
    );
    assert_eq!(
        unanswered.fresh_within_hours,
        system_feeds::canonical_fresh_within_hours(FeedKey::UnansweredInquiry)
    );

    // A structurally undecodable stored value (unsupported_filter) falls
    // back the same way.
    sqlx::query(
        "UPDATE today_system_feed SET filter = '{\"version\":99,\"clauses\":[]}'::jsonb, \
         revision = revision + 1 WHERE organization_id = $1 AND feed_key = 'client_replied'",
    )
    .bind(org_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let resolved = system_feeds::load_feed_rows(&mut conn, OrganizationId::new(org_id))
        .await
        .unwrap();
    let replied = resolved
        .iter()
        .find(|f| f.feed_key == FeedKey::ClientReplied)
        .unwrap();
    assert!(
        replied.fallback,
        "unsupported stored JSON must also fall back"
    );
    assert_eq!(
        replied.filter,
        system_feeds::canonical_default(FeedKey::ClientReplied)
    );
}

/// spec §3 CHECK constraints: `feed_key` vocabulary, `fresh_within_hours`
/// bounds (1..=8760), and the call feed's forbidden freshness window.
#[sqlx::test]
#[ignore]
async fn today_system_feed_check_constraints(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Feed check org").await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let bad_key = sqlx::query(
        "UPDATE today_system_feed SET feed_key = feed_key WHERE organization_id = $1 AND feed_key = 'unanswered_inquiry'",
    )
    .bind(org_id)
    .execute(&app_pool)
    .await;
    assert!(
        bad_key.is_ok(),
        "a no-op update of an existing valid key must succeed"
    );

    // Out-of-bounds fresh_within_hours.
    for bad_hours in [0i32, 8761] {
        let result = sqlx::query(
            "UPDATE today_system_feed SET fresh_within_hours = $1
             WHERE organization_id = $2 AND feed_key = 'unanswered_inquiry'",
        )
        .bind(bad_hours)
        .bind(org_id)
        .execute(&app_pool)
        .await;
        let err = result.unwrap_err();
        let db = err.as_database_error().expect("a CHECK violation");
        assert_eq!(db.code().as_deref(), Some("23514"), "{bad_hours}");
    }
    // In-bounds values succeed.
    for good_hours in [1i32, 8760] {
        let result = sqlx::query(
            "UPDATE today_system_feed SET fresh_within_hours = $1
             WHERE organization_id = $2 AND feed_key = 'unanswered_inquiry'",
        )
        .bind(good_hours)
        .bind(org_id)
        .execute(&app_pool)
        .await;
        assert!(result.is_ok(), "{good_hours}");
    }

    // The call feed can never carry a freshness window.
    let call_with_window = sqlx::query(
        "UPDATE today_system_feed SET fresh_within_hours = 24
         WHERE organization_id = $1 AND feed_key = 'call_outcome_needed'",
    )
    .bind(org_id)
    .execute(&app_pool)
    .await;
    let err = call_with_window.unwrap_err();
    let db = err.as_database_error().expect("a CHECK violation");
    assert_eq!(db.code().as_deref(), Some("23514"));

    // An unknown feed_key value is rejected at insert.
    let bad_insert = sqlx::query(
        "INSERT INTO today_system_feed (organization_id, feed_key) VALUES ($1, 'bogus_feed')",
    )
    .bind(org_id)
    .execute(&app_pool)
    .await;
    let err = bad_insert.unwrap_err();
    let db = err.as_database_error().expect("a CHECK violation");
    assert_eq!(db.code().as_deref(), Some("23514"));
}

/// spec §3 backfill: "one row per feed for every existing Organization with
/// NULL definition columns". The `#[sqlx::test]` ephemeral-database model
/// runs every migration, including this one, before any Organization can
/// exist in that fresh database — so this test replays the migration's own
/// backfill statement (byte-identical to the migration file) against an
/// Organization inserted directly (bypassing `create_organization`'s
/// seeding, simulating a pre-migration row), which is the same technique
/// this codebase's other migration-behavior tests use where the harness
/// cannot literally time-travel a schema change.
#[sqlx::test]
#[ignore]
async fn backfill_statement_inserts_three_null_rows_per_organization_and_is_idempotent(
    migrator_pool: PgPool,
) {
    let pre_existing_org_id: Uuid = sqlx::query_scalar(
        "INSERT INTO organization (name, intake_slug, intake_token)
         VALUES ('Pre-migration org', 'pre-migration-org', 'abc12345')
         RETURNING id",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(
        feed_rows(&migrator_pool, pre_existing_org_id).await.len(),
        0
    );

    let backfill = "INSERT INTO today_system_feed (organization_id, feed_key)
         SELECT o.id, k.feed_key
         FROM organization o
         CROSS JOIN (VALUES ('unanswered_inquiry'), ('client_replied'), ('call_outcome_needed')) AS k(feed_key)
         ON CONFLICT (organization_id, feed_key) DO NOTHING";

    sqlx::query(backfill).execute(&migrator_pool).await.unwrap();
    let rows = feed_rows(&migrator_pool, pre_existing_org_id).await;
    assert_eq!(rows.len(), 3);
    for (_, _, filter, fresh_within_hours, _) in &rows {
        assert!(filter.is_none());
        assert!(fresh_within_hours.is_none());
    }

    // Idempotent: running it again changes nothing (ON CONFLICT DO
    // NOTHING), including against an Organization that already has its
    // create-time seeded rows.
    let seeded_org_id = crate::common::create_org(&migrator_pool, "Already seeded org").await;
    sqlx::query(backfill).execute(&migrator_pool).await.unwrap();
    assert_eq!(
        feed_rows(&migrator_pool, pre_existing_org_id).await.len(),
        3
    );
    assert_eq!(feed_rows(&migrator_pool, seeded_org_id).await.len(), 3);
}

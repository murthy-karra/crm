//! DB-backed note coverage (Slice 015; docs/specs/SLICE_015.md §9.1–§9.8).
//! Command-level tests exercise the production `domain::note::*` path
//! directly for authorization and concurrency (mirroring `db_tags.rs`);
//! HTTP-level tests cover the route surface, detail shape, and error
//! precedence. Run only via ./scripts/check-db.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::note::{self, AddNote, DeleteNote, EditNote, NoteError};
use crm_api::ids::{CorrelationId, NoteId, OrganizationId, PersonId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "correct horse battery staple";

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

async fn recorded(publisher: &Publisher) -> Vec<(String, serde_json::Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected recording publisher");
    };
    recorded.lock().await.clone()
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_bare_person(pool: &PgPool, organization_id: Uuid, stage_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn note_row_count(pool: &PgPool, organization_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM note WHERE organization_id = $1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn promote_to_admin(pool: &PgPool, organization_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin'
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

struct Fixture {
    org_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
    person_id: Uuid,
}

async fn fixture(migrator_pool: &PgPool) -> Fixture {
    let (org_id, admin_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice-notes@acme.test",
        "Alice",
        PW,
    )
    .await;
    promote_to_admin(migrator_pool, org_id, admin_id).await;
    let member_id =
        crate::common::create_user(migrator_pool, "bob-notes@acme.test", "Bob", PW).await;
    crate::common::add_membership_with(
        migrator_pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, org_id).await;
    let person_id = insert_bare_person(&app_pool, org_id, stage_id).await;
    Fixture {
        org_id,
        admin_id,
        member_id,
        person_id,
    }
}

// --- §9.1: schema (CHECK matrix, cascade, import idempotency) -------------

/// docs/specs/SLICE_015.md §2, §9.1: every CHECK in the migration, one at
/// a time — empty live body, untrimmed body, a tombstone carrying a body,
/// a tombstone missing `deleted_by_user_id`, `source` without
/// `source_external_id`, and a non-`migration` origin with a NULL author
/// all fail; the boundary (exactly 10,000 chars, trimmed) succeeds.
#[sqlx::test]
#[ignore]
async fn note_check_constraints_matrix(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    #[allow(clippy::too_many_arguments)]
    async fn insert(
        pool: &PgPool,
        org_id: Uuid,
        person_id: Uuid,
        author_id: Option<Uuid>,
        body: &str,
        origin: &str,
        deleted_at: bool,
        deleted_by: Option<Uuid>,
        source: Option<&str>,
        source_external_id: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                                correlation_id, deleted_at, deleted_by_user_id, source, source_external_id)
             VALUES ($1, $2, $3, $4, $5, gen_random_uuid(),
                     CASE WHEN $6 THEN now() ELSE NULL END, $7, $8, $9)
             RETURNING id",
        )
        .bind(org_id)
        .bind(person_id)
        .bind(author_id)
        .bind(body)
        .bind(origin)
        .bind(deleted_at)
        .bind(deleted_by)
        .bind(source)
        .bind(source_external_id)
        .fetch_one(pool)
        .await
    }

    // Empty live body.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            "",
            "web_session",
            false,
            None,
            None,
            None,
        )
        .await
        .is_err(),
        "empty live body must violate the CHECK"
    );

    // 10,001 characters.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            &"a".repeat(10_001),
            "web_session",
            false,
            None,
            None,
            None,
        )
        .await
        .is_err(),
        "10,001-char body must violate the CHECK"
    );

    // Untrimmed body.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            "  padded  ",
            "web_session",
            false,
            None,
            None,
            None,
        )
        .await
        .is_err(),
        "untrimmed body must violate the CHECK"
    );

    // Tombstone carrying a body.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            "still here",
            "web_session",
            true,
            Some(f.member_id),
            None,
            None,
        )
        .await
        .is_err(),
        "a tombstone must have an empty body"
    );

    // Tombstone missing deleted_by_user_id.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            "",
            "web_session",
            true,
            None,
            None,
            None,
        )
        .await
        .is_err(),
        "deleted_at without deleted_by_user_id must violate the CHECK"
    );

    // source without source_external_id.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            Some(f.member_id),
            "A body",
            "migration",
            false,
            None,
            Some("fub"),
            None,
        )
        .await
        .is_err(),
        "source without source_external_id must violate the CHECK"
    );

    // Non-migration origin with a NULL author.
    assert!(
        insert(
            &app_pool,
            f.org_id,
            f.person_id,
            None,
            "A body",
            "web_session",
            false,
            None,
            None,
            None,
        )
        .await
        .is_err(),
        "a non-migration origin requires a non-NULL author"
    );

    // The boundary: exactly 10,000 trimmed characters succeeds.
    let ok = insert(
        &app_pool,
        f.org_id,
        f.person_id,
        Some(f.member_id),
        &"a".repeat(10_000),
        "web_session",
        false,
        None,
        None,
        None,
    )
    .await;
    assert!(
        ok.is_ok(),
        "exactly 10,000 trimmed characters must be accepted: {ok:?}"
    );
}

/// docs/specs/SLICE_015.md §2, §9.1 (review round 1, item 1): the composite
/// FKs reject a cross-Organization Person, a non-member author, and a
/// non-member `deleted_by_user_id`, even though every individual id is
/// real — the `person_tag` precedent (`db_schema.rs`, "the composite FKs
/// that make a cross-Organization row unpersistable even if an application
/// check regresses").
#[sqlx::test]
#[ignore]
async fn note_composite_fk_rejections(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-composite-fk@best.test",
        "Erin",
        PW,
    )
    .await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;
    // A real app_user with no membership in org A at all.
    let non_member_id = other_admin_id;

    let before = note_row_count(&migrator_pool, f.org_id).await;

    // (a) organization_id = A, person_id belongs to Organization B.
    let cross_org_person = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin, correlation_id)
         VALUES ($1, $2, $3, 'Body', 'web_session', gen_random_uuid())",
    )
    .bind(f.org_id)
    .bind(other_person_id)
    .bind(f.member_id)
    .execute(&app_pool)
    .await;
    assert!(
        cross_org_person.is_err(),
        "a Person from another Organization must be rejected"
    );

    // (b) author_user_id is a real app_user with no membership in org A.
    let non_member_author = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin, correlation_id)
         VALUES ($1, $2, $3, 'Body', 'web_session', gen_random_uuid())",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .bind(non_member_id)
    .execute(&app_pool)
    .await;
    assert!(
        non_member_author.is_err(),
        "a non-member author_user_id must be rejected"
    );

    // (c) a tombstone whose deleted_by_user_id is a non-member.
    let non_member_deleter = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin, correlation_id,
                            deleted_at, deleted_by_user_id)
         VALUES ($1, $2, $3, '', 'web_session', gen_random_uuid(), now(), $4)",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .bind(f.member_id)
    .bind(non_member_id)
    .execute(&app_pool)
    .await;
    assert!(
        non_member_deleter.is_err(),
        "a non-member deleted_by_user_id must be rejected"
    );

    assert_eq!(
        note_row_count(&migrator_pool, f.org_id).await,
        before,
        "no row must land from any rejected insert"
    );
}

/// docs/specs/SLICE_015.md §9.1: a body of exactly 10,000 four-byte code
/// points is accepted (pins `char_length` against bytes, not UTF-8 byte
/// count — ~40 KB raw); a Person row deletion cascades its notes.
#[sqlx::test]
#[ignore]
async fn note_accepts_10_000_four_byte_code_points_and_cascades_with_person(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let astral_body = "\u{1F600}".repeat(10_000);
    assert_eq!(astral_body.len(), 40_000, "sanity: 4 bytes per code point");
    let note_id: Uuid = sqlx::query_scalar(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin, correlation_id)
         VALUES ($1, $2, $3, $4, 'web_session', gen_random_uuid()) RETURNING id",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .bind(f.member_id)
    .bind(&astral_body)
    .fetch_one(&app_pool)
    .await
    .expect("10,000 four-byte code points must be accepted");

    sqlx::query("DELETE FROM person WHERE id = $1")
        .bind(f.person_id)
        .execute(&migrator_pool)
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM note WHERE id = $1")
        .bind(note_id)
        .fetch_one(&app_pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "deleting the Person must cascade the note");
}

/// docs/specs/SLICE_015.md §2, §9.1: the partial unique index rejects a
/// duplicate `(organization_id, source, source_external_id)` and allows
/// the same external id in a different Organization; a tombstoned
/// imported note still blocks a re-insert of the same external id.
#[sqlx::test]
#[ignore]
async fn note_source_external_id_partial_unique_index_and_resurrection_guard(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "dave-notes@best.test",
        "Dave",
        PW,
    )
    .await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;

    sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'Imported', 'migration', gen_random_uuid(), 'fub', 'ext-1')",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .execute(&app_pool)
    .await
    .unwrap();

    // Same Organization, same external id: rejected.
    let dup = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'Imported again', 'migration', gen_random_uuid(), 'fub', 'ext-1')",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .execute(&app_pool)
    .await;
    assert!(
        dup.is_err(),
        "a duplicate (org, source, external_id) must be rejected"
    );

    // A different Organization, same external id: allowed.
    let cross_org = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'Imported elsewhere', 'migration', gen_random_uuid(), 'fub', 'ext-1')",
    )
    .bind(other_org_id)
    .bind(other_person_id)
    .execute(&app_pool)
    .await;
    assert!(
        cross_org.is_ok(),
        "the same external id in another Organization must be allowed"
    );
    let _ = other_admin_id;

    // Tombstone the original, then a re-insert of the same external id is
    // still rejected (the resurrection guard).
    sqlx::query(
        "UPDATE note SET body = '', deleted_at = now(), deleted_by_user_id = $2
         WHERE organization_id = $1 AND source_external_id = 'ext-1'",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let resurrection = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'Re-imported', 'migration', gen_random_uuid(), 'fub', 'ext-1')",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .execute(&app_pool)
    .await;
    assert!(
        resurrection.is_err(),
        "a tombstoned imported note must block a re-insert of the same external id"
    );
}

// --- §9.3: add -------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn add_note_shape_history_entry_and_foreign_person_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let ctx = command_context(f.org_id, f.member_id);
    let note = note::add_note(
        &app_pool,
        &publisher,
        &ctx,
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "  Hello there  ".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(note.body, "Hello there");
    assert!(note.can_manage);
    assert!(!note.edited);
    assert_eq!(note.author.as_ref().unwrap().id, UserId::new(f.member_id));

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let history = detail["history"].as_array().unwrap();
    let entry = history
        .iter()
        .find(|e| e["kind"] == "note" && e["id"] == note.id.to_string())
        .expect("the note must appear in history");
    assert_eq!(entry["detail"]["body"], "Hello there");
    assert_eq!(entry["origin"], "web_session");
    assert_eq!(entry["actor"]["id"], f.member_id.to_string());
    // Round 2, item 1: correlation_id matches the session's own, and
    // occurred_at == recorded_at == created_at (an edit never moves the
    // entry, so this must already hold at creation).
    assert_eq!(
        entry["correlation_id"],
        serde_json::to_value(ctx.correlation_id).unwrap()
    );
    assert_eq!(
        entry["occurred_at"],
        serde_json::to_value(note.created_at).unwrap()
    );
    assert_eq!(entry["recorded_at"], entry["occurred_at"]);

    // Foreign/nonexistent Person: 404, no row.
    let before = note_row_count(&migrator_pool, f.org_id).await;
    let foreign = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(Uuid::new_v4()),
            body: "Should not land".to_string(),
        },
    )
    .await;
    assert!(matches!(foreign, Err(NoteError::NotFound)));
    assert_eq!(note_row_count(&migrator_pool, f.org_id).await, before);

    // Round 2, item 1: over HTTP with alice's own cookie, a REAL
    // cross-Organization Person and a random uuid are byte-identical
    // 404s — not merely "both 404", but the SAME response body — with no
    // row landing in either Organization and no additional publication.
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-add-fk@best.test",
        "Erin",
        PW,
    )
    .await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;
    let _ = other_admin_id;

    let events_before = recorded(&publisher).await.len();
    let org_a_before = note_row_count(&migrator_pool, f.org_id).await;
    let org_b_before = note_row_count(&migrator_pool, other_org_id).await;

    let random_uuid_resp = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes", Uuid::new_v4()),
        &alice,
        json!({ "body": "Nonexistent person" }),
    )
    .await;
    assert_eq!(random_uuid_resp.status(), StatusCode::NOT_FOUND);
    let random_uuid_body = crate::common::body_json(random_uuid_resp).await;

    let cross_org_resp = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{other_person_id}/notes"),
        &alice,
        json!({ "body": "Nonexistent person" }),
    )
    .await;
    assert_eq!(cross_org_resp.status(), StatusCode::NOT_FOUND);
    let cross_org_body = crate::common::body_json(cross_org_resp).await;
    assert_eq!(
        random_uuid_body, cross_org_body,
        "a real cross-Organization Person id must be byte-identical to a random uuid 404"
    );
    assert_eq!(
        note_row_count(&migrator_pool, f.org_id).await,
        org_a_before,
        "no row must land in Organization A"
    );
    assert_eq!(
        note_row_count(&migrator_pool, other_org_id).await,
        org_b_before,
        "no row must land in Organization B either"
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish on either 404"
    );
}

/// docs/specs/SLICE_015.md §9.3: a 200 KB body is 400 `malformed_request`
/// over HTTP, never 413 — the `DefaultBodyLimit` rejection is caught and
/// mapped, exactly like the tags router's identical-limit precedent.
#[sqlx::test]
#[ignore]
async fn add_note_over_128kib_is_400_not_413(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;

    let oversized = "a".repeat(200 * 1024);
    let response = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes", f.person_id),
        &alice,
        json!({ "body": oversized }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(response).await["error"],
        "malformed_request"
    );
}

async fn insert_correspondence_raw(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO correspondence_raw (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
         VALUES (gen_random_uuid(), $1, now(), $2, $3, $4, 0, true) RETURNING id",
    )
    .bind(organization_id)
    .bind(vec![0_u8; 24])
    .bind(vec![1_u8; 16])
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// docs/specs/SLICE_015.md §5, §9.3 (review round 1, item 5): a note
/// (`kind_rank` 7) created in the exact same instant as a
/// `correspondence_captured` row (`kind_rank` 6) sorts immediately after
/// it — both inserted with one shared explicit timestamp for
/// `occurred_at`/`recorded_at`/`created_at` on the owner connection, the
/// only way to actually reach the `kind_rank` tie-break (two independent
/// `now()` calls would essentially never collide). An edit does not move
/// the note's rendered position, since `occurred_at` stays `created_at`
/// regardless of `updated_at`.
#[sqlx::test]
#[ignore]
async fn note_history_position_ties_with_correspondence_by_kind_rank_and_survives_an_edit(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let shared_ts = chrono::Utc::now();
    let raw_id = insert_correspondence_raw(&app_pool, f.org_id).await;
    sqlx::query(
        "INSERT INTO correspondence_captured
            (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at,
             recorded_at, correlation_id, person_id, agent_user_id, direction, via,
             correspondence_raw_id, backdated)
         VALUES ($1, 'system', NULL, $2, 'webhook', $3, $3, gen_random_uuid(), $4, $2, 'inbound',
                 'cc', $5, false)",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .bind(shared_ts)
    .bind(f.person_id)
    .bind(raw_id)
    .execute(&app_pool)
    .await
    .unwrap();

    // The note's `created_at`/`updated_at` are forced to the SAME instant
    // via a raw insert on the owner connection (`AddNote` itself always
    // uses `now()`, which could never reliably collide with the row above).
    let note_id: Uuid = sqlx::query_scalar(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, created_at, updated_at)
         VALUES ($1, $2, $3, 'Same instant', 'web_session', gen_random_uuid(), $4, $4)
         RETURNING id",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .bind(f.member_id)
    .bind(shared_ts)
    .fetch_one(&app_pool)
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;
    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let history = detail["history"].as_array().unwrap();
    let correspondence_index = history
        .iter()
        .position(|e| e["kind"] == "correspondence")
        .expect("the correspondence entry must be present");
    let note_index = history
        .iter()
        .position(|e| e["id"] == note_id.to_string())
        .expect("the note entry must be present");
    assert_eq!(
        note_index,
        correspondence_index + 1,
        "note (kind_rank 7) must sort immediately after correspondence (kind_rank 6) at the same instant: {history:?}"
    );

    // An edit does not move the rendered position, and the detail read
    // reflects the changed body plus edited/updated_at.
    note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: NoteId::new(note_id),
            body: "Same instant, fixed".to_string(),
        },
    )
    .await
    .unwrap();
    let detail_after = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let history_after = detail_after["history"].as_array().unwrap();
    let correspondence_index_after = history_after
        .iter()
        .position(|e| e["kind"] == "correspondence")
        .unwrap();
    let note_index_after = history_after
        .iter()
        .position(|e| e["id"] == note_id.to_string())
        .unwrap();
    assert_eq!(
        note_index_after,
        correspondence_index_after + 1,
        "editing the note must not move its rendered position: {history_after:?}"
    );
    let entry_after = &history_after[note_index_after];
    assert_eq!(entry_after["detail"]["body"], "Same instant, fixed");
    assert_eq!(entry_after["detail"]["edited"], true);
    assert_ne!(
        entry_after["detail"]["updated_at"], entry_after["occurred_at"],
        "updated_at must have moved past the unchanged occurred_at (created_at)"
    );
}

// --- §9.4: edit --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn edit_note_author_and_admin_succeed_third_member_forbidden(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-notes@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Original".to_string(),
        },
    )
    .await
    .unwrap();

    // A third member (neither author nor admin): forbidden, no write.
    let forbidden = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Hijacked".to_string(),
        },
    )
    .await;
    assert!(matches!(forbidden, Err(NoteError::Forbidden)));
    let body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "Original");

    // The author edits: 200, changed:true, updated_at moves, edited:true,
    // timeline position (created_at) unchanged.
    let created_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT created_at FROM note WHERE id = $1")
            .bind(note.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let author_edit = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Fixed typo".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(author_edit.changed);
    assert_eq!(author_edit.note.body, "Fixed typo");
    assert!(author_edit.note.edited);
    assert!(author_edit.note.updated_at > author_edit.note.created_at);
    assert_eq!(author_edit.note.created_at, created_at_before);

    // The admin edits another member's note: 200.
    let admin_edit = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Admin fixed it".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(admin_edit.changed);

    // Same body again: changed:false, nothing written (updated_at stable).
    let updated_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM note WHERE id = $1")
            .bind(note.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let no_op = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Admin fixed it".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!no_op.changed);
    let updated_at_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM note WHERE id = $1")
            .bind(note.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(updated_at_before, updated_at_after);
}

#[sqlx::test]
#[ignore]
async fn edit_note_deactivated_author_and_demoted_deactivated_admin_are_forbidden(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Author note".to_string(),
        },
    )
    .await
    .unwrap();

    // The author is deactivated out-of-band, inside what will be the edit
    // command's own transaction window (the `db_tags.rs` out-of-band
    // UPDATE pattern).
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let deactivated_author = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Should not apply".to_string(),
        },
    )
    .await;
    assert!(matches!(deactivated_author, Err(NoteError::Forbidden)));

    // A second note, edited by an admin who is demoted AND deactivated
    // out-of-band before the command runs.
    let second_note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Admin note".to_string(),
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role = 'member', status = 'inactive'
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let events_before = recorded(&publisher).await.len();
    let demoted_deactivated = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: second_note.id,
            body: "Should not apply either".to_string(),
        },
    )
    .await;
    assert!(matches!(demoted_deactivated, Err(NoteError::Forbidden)));
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "a forbidden edit must publish nothing"
    );
    let body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(second_note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "Admin note");
}

/// docs/specs/SLICE_015.md §6 (review round 1, item 3): a PURE demotion —
/// `role = 'member'` only, membership left `active` — is forbidden on its
/// own, distinct from the deactivation case above (kept as its own step,
/// not merged): an admin who is demoted but still an active member loses
/// rule-1 access to another member's note exactly the same way, with no
/// write and no publication.
#[sqlx::test]
#[ignore]
async fn edit_note_pure_demotion_is_forbidden_with_no_write_or_publish(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Member's note".to_string(),
        },
    )
    .await
    .unwrap();

    sqlx::query(
        "UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let events_before = recorded(&publisher).await.len();
    let result = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Should not apply".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(NoteError::Forbidden)));
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "a forbidden edit must publish nothing"
    );
    let body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "Member's note");
}

/// docs/specs/SLICE_015.md §6 (review round 1, item 3): the delete path had
/// no demotion or deactivation coverage at all — this mirrors the edit
/// tests above, one case each, for `delete_note`.
#[sqlx::test]
#[ignore]
async fn delete_note_deactivated_author_and_pure_demoted_admin_are_forbidden(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    // Deactivated author: cannot delete their own note.
    let authored = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Author's note".to_string(),
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let deactivated_author_delete = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: authored.id,
        },
    )
    .await;
    assert!(matches!(
        deactivated_author_delete,
        Err(NoteError::Forbidden)
    ));
    let body_after_deactivated: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(authored.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body_after_deactivated, "Author's note");

    // Pure demotion (role only, still active): an admin demoted to member
    // cannot delete another member's note either.
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-pure-demote@acme.test", "Carol", PW)
            .await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let carols_note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Carol's note".to_string(),
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let events_before = recorded(&publisher).await.len();
    let demoted_admin_delete = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: carols_note.id,
        },
    )
    .await;
    assert!(matches!(demoted_admin_delete, Err(NoteError::Forbidden)));
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "a forbidden delete must publish nothing"
    );
    let carols_body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(carols_note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(carols_body, "Carol's note");
}

/// docs/specs/SLICE_015.md §3, §6 (review round 1, item 4): the FOR SHARE
/// membership re-read genuinely waits on, and then observes, an in-flight
/// deactivation "inside the transaction" as the spec words it — not merely
/// a deactivation already committed before the command starts (the
/// existing deactivation tests). A second connection opens a transaction,
/// takes the row lock via an UPDATE, and holds it uncommitted while the
/// edit's own FOR SHARE read blocks behind it; only once that transaction
/// commits does the edit's re-read proceed — and it must see the
/// now-inactive membership.
#[sqlx::test]
#[ignore]
async fn edit_note_for_share_reread_observes_a_committing_deactivation(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Racing deactivation".to_string(),
        },
    )
    .await
    .unwrap();
    let events_before = recorded(&publisher).await.len();

    let mut lock_tx = app_pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&mut *lock_tx)
    .await
    .unwrap();

    let edit_ctx = command_context(f.org_id, f.member_id);
    let edit_fut = note::edit_note(
        &app_pool,
        &publisher,
        &edit_ctx,
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Should never apply".to_string(),
        },
    );
    let commit_fut = async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        lock_tx.commit().await.unwrap();
    };
    let (edit_result, ()) = tokio::join!(edit_fut, commit_fut);

    assert!(
        matches!(edit_result, Err(NoteError::Forbidden)),
        "the edit must observe the deactivation once it commits: {edit_result:?}"
    );
    let body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "Racing deactivation", "no write must land");
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no additional publication on a forbidden edit"
    );
}

#[sqlx::test]
#[ignore]
async fn edit_note_tombstone_cross_organization_and_cross_person_are_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "To be deleted".to_string(),
        },
    )
    .await
    .unwrap();
    note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
        },
    )
    .await
    .unwrap();

    // Editing a tombstone: 404, identical to a nonexistent id.
    let tombstoned = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Resurrect me".to_string(),
        },
    )
    .await;
    assert!(matches!(tombstoned, Err(NoteError::NotFound)));

    // A live note, reached through ANOTHER Organization's actor/context: 404.
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-notes@best.test",
        "Erin",
        PW,
    )
    .await;
    let live_note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Live note".to_string(),
        },
    )
    .await
    .unwrap();
    let cross_org = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(other_org_id, other_admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: live_note.id,
            body: "Stolen".to_string(),
        },
    )
    .await;
    assert!(matches!(cross_org, Err(NoteError::NotFound)));

    // The SAME Organization's note, reached through ANOTHER Person's path:
    // 404, row unchanged, no publication.
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let other_person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let events_before = recorded(&publisher).await.len();
    let wrong_person_path = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(other_person_id),
            note_id: live_note.id,
            body: "Wrong path".to_string(),
        },
    )
    .await;
    assert!(matches!(wrong_person_path, Err(NoteError::NotFound)));
    assert_eq!(recorded(&publisher).await.len(), events_before);
    let body: String = sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
        .bind(live_note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "Live note");
}

/// docs/specs/SLICE_015.md §9.4: validation runs before the note lookup —
/// a malformed body on a nonexistent note id is still 400, not 404.
#[sqlx::test]
#[ignore]
async fn edit_note_validation_precedes_the_not_found_lookup(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let result = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: NoteId::new(Uuid::new_v4()),
            body: "".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(NoteError::MalformedRequest)));
}

// --- §9.5: delete ------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn delete_note_author_and_admin_succeed_third_member_forbidden_and_tombstone_shape(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let carol_id =
        crate::common::create_user(&migrator_pool, "carol-del@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        carol_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Delete me".to_string(),
        },
    )
    .await
    .unwrap();

    let forbidden = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, carol_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
        },
    )
    .await;
    assert!(matches!(forbidden, Err(NoteError::Forbidden)));

    #[derive(sqlx::FromRow, Debug, PartialEq)]
    struct RowSnapshot {
        origin: String,
        correlation_id: Uuid,
        author_user_id: Option<Uuid>,
        source: Option<String>,
        source_external_id: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    }
    let before: RowSnapshot = sqlx::query_as(
        "SELECT origin, correlation_id, author_user_id, source, source_external_id, created_at, updated_at
         FROM note WHERE id = $1",
    )
    .bind(note.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    let deleted = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);

    let (body, deleted_at, deleted_by): (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<Uuid>,
    ) = sqlx::query_as("SELECT body, deleted_at, deleted_by_user_id FROM note WHERE id = $1")
        .bind(note.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(body, "");
    assert!(deleted_at.is_some());
    assert_eq!(deleted_by, Some(f.admin_id));

    let after: RowSnapshot = sqlx::query_as(
        "SELECT origin, correlation_id, author_user_id, source, source_external_id, created_at, updated_at
         FROM note WHERE id = $1",
    )
    .bind(note.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(before, after, "every other column must stay byte-identical");

    // Repeat delete, edit-of-tombstone, delete-through-another-person's-path:
    // all 404.
    let repeat = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        DeleteNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
        },
    )
    .await;
    assert!(matches!(repeat, Err(NoteError::NotFound)));

    let edit_tombstone = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: note.id,
            body: "Resurrect".to_string(),
        },
    )
    .await;
    assert!(matches!(edit_tombstone, Err(NoteError::NotFound)));

    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let other_person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let live_note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Another live note".to_string(),
        },
    )
    .await
    .unwrap();
    let wrong_path_delete = note::delete_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        DeleteNote {
            person_id: PersonId::new(other_person_id),
            note_id: live_note.id,
        },
    )
    .await;
    assert!(matches!(wrong_path_delete, Err(NoteError::NotFound)));

    // Item 9: a fresh detail read and the Operator's own read
    // (`latest_for_person`) both show the deleted note absent.
    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;
    let detail_after_delete = crate::common::body_json(
        crate::common::get_with_cookie(
            &router,
            &format!("/api/people/{}", f.person_id),
            &alice_cookie,
        )
        .await,
    )
    .await;
    assert!(
        detail_after_delete["history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != note.id.to_string()),
        "the deleted note must not appear in the detail read"
    );
    let mut conn = app_pool.acquire().await.unwrap();
    let operator_notes = note::latest_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
        5,
    )
    .await
    .unwrap();
    assert!(
        operator_notes.iter().all(|n| n.body != "Delete me"),
        "the deleted note must not appear in the Operator's own read"
    );
    drop(conn);

    // Item 2: a REAL cross-Organization note id (not merely a random
    // uuid) is exactly as invisible over HTTP as a random uuid — same
    // status, byte-identical body, no publication, and the other
    // Organization's row is untouched.
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-del-fk@best.test",
        "Erin",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, other_org_id, other_admin_id).await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_org_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;
    let other_org_note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(other_org_id, other_admin_id),
        AddNote {
            person_id: PersonId::new(other_org_person_id),
            body: "Org B's note".to_string(),
        },
    )
    .await
    .unwrap();

    let events_before = recorded(&publisher).await.len();
    let random_uuid_delete = crate::common::delete_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{}", f.person_id, Uuid::new_v4()),
        &alice_cookie,
    )
    .await;
    assert_eq!(random_uuid_delete.status(), StatusCode::NOT_FOUND);
    let random_uuid_body = crate::common::body_json(random_uuid_delete).await;

    let cross_org_delete = crate::common::delete_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{}", f.person_id, other_org_note.id),
        &alice_cookie,
    )
    .await;
    assert_eq!(cross_org_delete.status(), StatusCode::NOT_FOUND);
    let cross_org_body = crate::common::body_json(cross_org_delete).await;
    assert_eq!(
        random_uuid_body, cross_org_body,
        "a real cross-Organization note id must be byte-identical to a random uuid 404"
    );
    assert_eq!(
        recorded(&publisher).await.len(),
        events_before,
        "no publish on a 404"
    );
    let other_org_body_untouched: String =
        sqlx::query_scalar("SELECT body FROM note WHERE id = $1")
            .bind(other_org_note.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        other_org_body_untouched, "Org B's note",
        "the other Organization's note must be untouched"
    );
}

/// docs/specs/SLICE_015.md §9.5: an author edit racing an admin delete
/// (`tokio::join!`, the `db_tags.rs` race pattern) ends `{200, 200}` or
/// `{404, 200}`, never a raw database error, with a tombstone as the
/// final row.
#[sqlx::test]
#[ignore]
async fn edit_racing_delete_never_503s_and_tombstone_wins(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let note = note::add_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddNote {
            person_id: PersonId::new(f.person_id),
            body: "Racing".to_string(),
        },
    )
    .await
    .unwrap();

    let edit_ctx = command_context(f.org_id, f.member_id);
    let delete_ctx = command_context(f.org_id, f.admin_id);
    let note_id = note.id;
    let person_id = PersonId::new(f.person_id);
    let edit_fut = note::edit_note(
        &app_pool,
        &publisher,
        &edit_ctx,
        EditNote {
            person_id,
            note_id,
            body: "Edited while racing".to_string(),
        },
    );
    let delete_fut = note::delete_note(
        &app_pool,
        &publisher,
        &delete_ctx,
        DeleteNote { person_id, note_id },
    );
    let (edit_result, delete_result) = tokio::join!(edit_fut, delete_fut);

    match edit_result {
        Ok(_) => {}
        Err(NoteError::NotFound) => {}
        other => panic!("edit racing a delete must be Ok or NotFound, got {other:?}"),
    }
    assert!(delete_result.is_ok(), "the admin delete must succeed");

    let (body, deleted_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT body, deleted_at FROM note WHERE id = $1")
            .bind(note_id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(body, "", "the final row must be the tombstone");
    assert!(deleted_at.is_some());
}

// --- §9.6: imported shape --------------------------------------------------

#[sqlx::test]
#[ignore]
async fn imported_note_renders_with_null_actor_admin_only_can_manage_and_member_edit_forbidden(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();

    let imported_id: Uuid = sqlx::query_scalar(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'From FUB', 'migration', gen_random_uuid(), 'fub', 'ext-imported-1')
         RETURNING id",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob-notes@acme.test", PW).await;

    let detail_as_admin = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let entry = detail_as_admin["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == imported_id.to_string())
        .unwrap();
    assert_eq!(entry["actor"], serde_json::Value::Null);
    assert_eq!(
        entry["detail"]["can_manage"], true,
        "admin can manage an imported note"
    );

    let detail_as_member = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &bob)
            .await,
    )
    .await;
    let member_entry = detail_as_member["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == imported_id.to_string())
        .unwrap();
    assert_eq!(member_entry["detail"]["can_manage"], false);

    // A member's edit of an imported (NULL-author) note is forbidden.
    let member_edit = note::edit_note(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        EditNote {
            person_id: PersonId::new(f.person_id),
            note_id: NoteId::new(imported_id),
            body: "Member cannot touch this".to_string(),
        },
    )
    .await;
    assert!(matches!(member_edit, Err(NoteError::Forbidden)));

    // A tombstoned imported note blocks a re-insert of the same external id.
    sqlx::query(
        "UPDATE note SET body = '', deleted_at = now(), deleted_by_user_id = $2 WHERE id = $1",
    )
    .bind(imported_id)
    .bind(f.admin_id)
    .execute(&app_pool)
    .await
    .unwrap();
    let resurrection = sqlx::query(
        "INSERT INTO note (organization_id, person_id, author_user_id, body, origin,
                            correlation_id, source, source_external_id)
         VALUES ($1, $2, NULL, 'Re-imported', 'migration', gen_random_uuid(), 'fub', 'ext-imported-1')",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .execute(&app_pool)
    .await;
    assert!(resurrection.is_err());
}

// --- §9.7: reads and precedence -----------------------------------------

#[sqlx::test]
#[ignore]
async fn note_route_error_precedence_wire_shapes_and_can_manage(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob-notes@acme.test", PW).await;

    // Malformed path uuid: 400, before authentication is even relevant —
    // proven with NO cookie at all (review round 1, item 8): sending a
    // valid session here would only prove "400 happens", not that it
    // outranks the 401 an absent/invalid session would otherwise produce.
    let bad_path = crate::common::put_json_with_cookie(
        &router,
        "/api/people/not-a-uuid/notes/not-a-uuid",
        "",
        json!({ "body": "X" }),
    )
    .await;
    assert_eq!(bad_path.status(), StatusCode::BAD_REQUEST);

    // No cookie: 401.
    let no_cookie = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes", f.person_id),
        "",
        json!({ "body": "X" }),
    )
    .await;
    assert_eq!(no_cookie.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        crate::common::body_json(no_cookie).await["error"],
        "unauthenticated"
    );

    // Malformed body: 400, before 404.
    let bad_body = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{}", f.person_id, Uuid::new_v4()),
        &bob,
        json!({ "body": "" }),
    )
    .await;
    assert_eq!(bad_body.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(bad_body).await["error"],
        "malformed_request"
    );

    // Nonexistent note id, valid body: 404.
    let missing = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{}", f.person_id, Uuid::new_v4()),
        &bob,
        json!({ "body": "New body" }),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        crate::common::body_json(missing).await["error"],
        "not_found"
    );

    // Add as alice (admin): 201, can_manage true.
    let added = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            &format!("/api/people/{}/notes", f.person_id),
            &alice,
            json!({ "body": "Alice's note" }),
        )
        .await,
    )
    .await;
    assert_eq!(added["note"]["can_manage"], true);
    let note_id = added["note"]["id"].as_str().unwrap().to_string();

    // Bob (a plain member, not the author): the note exists, so
    // precedence has passed 404 -> 403 forbidden.
    let forbidden = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{note_id}", f.person_id),
        &bob,
        json!({ "body": "Hijacked" }),
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        crate::common::body_json(forbidden).await,
        json!({ "error": "forbidden" })
    );

    // can_manage is true for the author (alice) and false for another
    // member (bob) reading the same detail.
    let detail_as_alice = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let alice_entry = detail_as_alice["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == note_id)
        .unwrap();
    assert_eq!(alice_entry["detail"]["can_manage"], true);

    let detail_as_bob = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &bob)
            .await,
    )
    .await;
    let bob_entry = detail_as_bob["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == note_id)
        .unwrap();
    assert_eq!(bob_entry["detail"]["can_manage"], false);

    // A deactivated author's note still renders with `actor` set (D-027 §2).
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.org_id)
    .bind(f.admin_id)
    .execute(&crate::common::connect_as_app(&migrator_pool).await)
    .await
    .unwrap();
    let detail_after_deactivation = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &bob)
            .await,
    )
    .await;
    let entry_after = detail_after_deactivation["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == note_id)
        .unwrap();
    assert_eq!(entry_after["actor"]["id"], f.admin_id.to_string());

    // DELETE on a foreign Person id path is 404. `bob` (never deactivated
    // above, unlike `alice`/the admin) stays a valid active member.
    let foreign_person_delete = crate::common::delete_with_cookie(
        &router,
        &format!("/api/people/{}/notes/{note_id}", Uuid::new_v4()),
        &bob,
    )
    .await;
    assert_eq!(foreign_person_delete.status(), StatusCode::NOT_FOUND);
}

// The platform-only-session 401 check for all three note routes (explicit
// POST/PUT/DELETE) lives in `db_admin.rs`'s existing enumeration test
// (docs/specs/SLICE_015.md §9.7), alongside every other tenant route.

// --- §9.8: realtime -------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn note_changed_publishes_exactly_once_per_changing_write(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let publisher = Publisher::recording();
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice-notes@acme.test", PW).await;

    let added = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            &format!("/api/people/{}/notes", f.person_id),
            &alice,
            json!({ "body": "Loud note" }),
        )
        .await,
    )
    .await;
    let note_id = added["note"]["id"].as_str().unwrap().to_string();
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1, "add must publish exactly once");
    assert_eq!(events[0].1["data"]["change"], "note_changed");
    assert!(
        !events[0].1.to_string().contains("Loud note"),
        "the body must never appear on the realtime event: {}",
        events[0].1
    );

    let edit_uri = format!("/api/people/{}/notes/{note_id}", f.person_id);

    // Same body: changed:false, no additional event.
    let no_op = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &edit_uri,
            &alice,
            json!({ "body": "Loud note" }),
        )
        .await,
    )
    .await;
    assert_eq!(no_op["changed"], false);
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "no publish on changed:false"
    );

    // Review round 1, item 10: a raw body that DIFFERS byte-for-byte from
    // the stored one but NORMALIZES to it (`\r\n` -> `\n`, then trimmed)
    // is still `changed: false` and publishes nothing — the byte-equality
    // check in `edit_note` compares the NORMALIZED body, not the raw wire
    // input.
    let normalizes_to_same = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &edit_uri,
            &alice,
            json!({ "body": "Loud note\r\n  " }),
        )
        .await,
    )
    .await;
    assert_eq!(normalizes_to_same["changed"], false);
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "no publish when the normalized body is unchanged"
    );

    // A changing edit: a second event.
    let changed = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &edit_uri,
            &alice,
            json!({ "body": "Louder note" }),
        )
        .await,
    )
    .await;
    assert_eq!(changed["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].1["data"]["change"], "note_changed");

    // Delete: a third event.
    crate::common::delete_with_cookie(&router, &edit_uri, &alice).await;
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 3);
    assert_eq!(events[2].1["data"]["change"], "note_changed");

    // Repeat delete (404): no additional event.
    crate::common::delete_with_cookie(&router, &edit_uri, &alice).await;
    assert_eq!(recorded(&publisher).await.len(), 3);
}

//! DB-backed tag coverage (Slice 011e, e1; docs/specs/SLICE_011e.md §9.1–
//! §9.7). Command-level tests exercise the production
//! `domain::tag::*` path directly for authorization, quota, and
//! concurrency (mirroring `db_saved_lists.rs`); HTTP-level tests cover the
//! route surface, detail shape, and error precedence. Run only via
//! ./scripts/check-db.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::tag::{
    self, AddPersonTag, CreateTag, DeleteTag, RemovePersonTag, RenameTag, TagError,
};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, TagId, UserId};
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

async fn tag_row_count(pool: &PgPool, organization_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM tag WHERE organization_id = $1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn person_tag_row_count(pool: &PgPool, organization_id: Uuid, person_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM person_tag WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(organization_id)
    .bind(person_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `create_org_with_stages_and_member` already inserts its member as an
/// active Member row; promote that SAME row in place rather than
/// attempting a second insert for the same (org, user) primary key.
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

/// A full fixture: one Organization, seeded stages, an admin, and a
/// non-admin member — the common starting point for tag tests.
struct Fixture {
    org_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
}

async fn fixture(migrator_pool: &PgPool) -> Fixture {
    // `create_org_with_stages_and_member` already inserts `admin_id` as an
    // active MEMBER (its own default) — promote that same row to Admin in
    // place rather than inserting a second row for the same (org, user)
    // primary key.
    let (org_id, admin_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    promote_to_admin(migrator_pool, org_id, admin_id).await;
    let member_id = crate::common::create_user(migrator_pool, "bob@acme.test", "Bob", PW).await;
    crate::common::add_membership_with(
        migrator_pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    Fixture {
        org_id,
        admin_id,
        member_id,
    }
}

// --- §9.2: create ----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn create_tag_is_create_or_get_by_case_insensitive_name(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let first = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Investor".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(first.created);
    assert_eq!(first.tag.name, "Investor");
    assert_eq!(first.tag.person_count, 0);
    assert!(first.tag.can_manage);

    // Same name, different case: hit, first spelling kept, never a 409.
    let second = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateTag {
            name: "investor".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!second.created);
    assert_eq!(second.tag.id, first.tag.id);
    assert_eq!(second.tag.name, "Investor", "the first spelling is kept");

    assert_eq!(tag_row_count(&migrator_pool, f.org_id).await, 1);
}

#[sqlx::test]
#[ignore]
async fn create_tag_validates_trim_length_and_control_characters(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    for bad in ["", "   ", &"a".repeat(41), "bad\ttab", "bad\nline"] {
        let result = tag::create_tag(
            &app_pool,
            &command_context(f.org_id, f.member_id),
            CreateTag {
                name: bad.to_string(),
            },
        )
        .await;
        assert!(
            matches!(result, Err(TagError::MalformedRequest)),
            "expected malformed_request for {bad:?}, got {result:?}"
        );
    }

    // Exactly 40 chars and leading/trailing whitespace are both accepted.
    let ok = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: format!("  {}  ", "a".repeat(40)),
        },
    )
    .await
    .unwrap();
    assert_eq!(ok.tag.name, "a".repeat(40));
}

#[sqlx::test]
#[ignore]
async fn create_tag_the_201st_tag_is_409_tag_limit_reached(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    sqlx::query(
        "INSERT INTO tag (organization_id, created_by_user_id, name)
         SELECT $1, $2, 'Tag ' || s.i FROM generate_series(1, 200) AS s(i)",
    )
    .bind(f.org_id)
    .bind(f.member_id)
    .execute(&app_pool)
    .await
    .unwrap();

    let result = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "One Too Many".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::TagLimitReached)));
    assert_eq!(tag_row_count(&migrator_pool, f.org_id).await, 200);
}

#[sqlx::test]
#[ignore]
async fn two_concurrent_identical_creates_yield_exactly_one_row_and_two_successes(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob@acme.test", PW).await;

    let fut_a = crate::common::post_json_with_cookie(
        &router,
        "/api/tags",
        &alice,
        json!({ "name": "Sphere" }),
    );
    let fut_b = crate::common::post_json_with_cookie(
        &router,
        "/api/tags",
        &bob,
        json!({ "name": "Sphere" }),
    );
    let (resp_a, resp_b) = tokio::join!(fut_a, fut_b);

    assert!(
        [StatusCode::CREATED, StatusCode::OK].contains(&resp_a.status()),
        "both concurrent creates must succeed: {}",
        resp_a.status()
    );
    assert!(
        [StatusCode::CREATED, StatusCode::OK].contains(&resp_b.status()),
        "both concurrent creates must succeed: {}",
        resp_b.status()
    );
    // Exactly one created:true, one created:false — the lock, not
    // `ON CONFLICT`, removed the race.
    let body_a = crate::common::body_json(resp_a).await;
    let body_b = crate::common::body_json(resp_b).await;
    let created_flags = [body_a["created"].as_bool(), body_b["created"].as_bool()];
    assert!(created_flags.contains(&Some(true)));
    assert!(created_flags.contains(&Some(false)));

    assert_eq!(tag_row_count(&migrator_pool, f.org_id).await, 1);
}

// --- §9.3: apply/remove ------------------------------------------------

#[sqlx::test]
#[ignore]
async fn add_and_remove_person_tag_is_target_state_idempotent(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Past Client".to_string(),
        },
    )
    .await
    .unwrap();

    let first_add = tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(first_add.changed);
    assert_eq!(first_add.tags.len(), 1);

    let repeat_add = tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(!repeat_add.changed, "re-applying an applied tag is a no-op");
    assert_eq!(repeat_add.tags.len(), 1);

    let first_remove = tag::remove_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        RemovePersonTag {
            person_id: PersonId::new(person_id),
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(first_remove.changed);
    assert!(first_remove.tags.is_empty());

    let repeat_remove = tag::remove_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        RemovePersonTag {
            person_id: PersonId::new(person_id),
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(!repeat_remove.changed, "removing an absent tag is a no-op");
}

#[sqlx::test]
#[ignore]
async fn person_tag_limit_is_20_and_reapplying_at_the_cap_still_succeeds(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let mut applied_ids = Vec::new();
    for i in 0..20 {
        let created = tag::create_tag(
            &app_pool,
            &command_context(f.org_id, f.member_id),
            CreateTag {
                name: format!("Tag {i}"),
            },
        )
        .await
        .unwrap();
        let outcome = tag::add_person_tag(
            &app_pool,
            &publisher,
            &command_context(f.org_id, f.member_id),
            AddPersonTag {
                person_id: PersonId::new(person_id),
                tag_id: created.tag.id,
            },
        )
        .await
        .unwrap();
        assert!(outcome.changed);
        applied_ids.push(created.tag.id);
    }
    assert_eq!(
        person_tag_row_count(&migrator_pool, f.org_id, person_id).await,
        20
    );

    // A 21st distinct tag is 409.
    let extra = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "One More".to_string(),
        },
    )
    .await
    .unwrap();
    let result = tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: extra.tag.id,
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::PersonTagLimitReached)));

    // Re-applying an existing one at 20 stays 200 (unchanged, no error).
    let reapply = tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.member_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: applied_ids[0],
        },
    )
    .await
    .unwrap();
    assert!(!reapply.changed);
    assert_eq!(
        person_tag_row_count(&migrator_pool, f.org_id, person_id).await,
        20
    );
}

#[sqlx::test]
#[ignore]
async fn add_and_remove_on_a_foreign_or_nonexistent_person_or_tag_are_identical_404s(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Real Tag".to_string(),
        },
    )
    .await
    .unwrap();

    let nonexistent_person = PersonId::new(Uuid::new_v4());
    let nonexistent_tag = TagId::new(Uuid::new_v4());

    for (person, tag_id) in [
        (nonexistent_person, created.tag.id),
        (PersonId::new(person_id), nonexistent_tag),
        (nonexistent_person, nonexistent_tag),
    ] {
        let add_result = tag::add_person_tag(
            &app_pool,
            &publisher,
            &command_context(f.org_id, f.member_id),
            AddPersonTag {
                person_id: person,
                tag_id,
            },
        )
        .await;
        assert!(matches!(add_result, Err(TagError::NotFound)));
        let remove_result = tag::remove_person_tag(
            &app_pool,
            &publisher,
            &command_context(f.org_id, f.member_id),
            RemovePersonTag {
                person_id: person,
                tag_id,
            },
        )
        .await;
        assert!(matches!(remove_result, Err(TagError::NotFound)));
    }
    assert_eq!(
        person_tag_row_count(&migrator_pool, f.org_id, person_id).await,
        0
    );
}

// --- §9.4: rename --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn admin_can_rename_any_tag_member_only_their_own_unused(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let member_tag = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Typo".to_string(),
        },
    )
    .await
    .unwrap();

    // Admin renames the member's unused tag: 200.
    let renamed_by_admin = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: member_tag.tag.id,
            name: "Fixed By Admin".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(renamed_by_admin.changed);
    assert_eq!(renamed_by_admin.tag.name, "Fixed By Admin");

    // The creator renames their own still-unused tag: 200.
    let renamed_by_creator = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        RenameTag {
            tag_id: member_tag.tag.id,
            name: "Fixed By Creator".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(renamed_by_creator.changed);
}

#[sqlx::test]
#[ignore]
async fn creator_loses_rename_permission_once_a_person_carries_the_tag(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Sphere".to_string(),
        },
    )
    .await
    .unwrap();

    // A concurrent AddPersonTag commits first.
    tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();

    let result = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        RenameTag {
            tag_id: created.tag.id,
            name: "Too Late".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::Forbidden)));
    // No write: the name is unchanged.
    let name: String = sqlx::query_scalar("SELECT name FROM tag WHERE id = $1")
        .bind(created.tag.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(name, "Sphere");
}

#[sqlx::test]
#[ignore]
async fn non_creator_member_cannot_rename_an_unused_tag(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let other_member_id =
        crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        other_member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Not Yours".to_string(),
        },
    )
    .await
    .unwrap();

    let result = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, other_member_id),
        RenameTag {
            tag_id: created.tag.id,
            name: "Stolen".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::Forbidden)));
}

#[sqlx::test]
#[ignore]
async fn rename_same_name_is_unchanged_case_only_rename_is_changed_and_collision_is_409(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let a = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateTag {
            name: "Alpha".to_string(),
        },
    )
    .await
    .unwrap();
    let b = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateTag {
            name: "Beta".to_string(),
        },
    )
    .await
    .unwrap();

    // Byte-identical name: changed:false, nothing written.
    let same = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: a.tag.id,
            name: "Alpha".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!same.changed);

    // Case-only rename of the SAME tag: a real update.
    let case_only = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: a.tag.id,
            name: "ALPHA".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(case_only.changed);
    assert_eq!(case_only.tag.name, "ALPHA");

    // Case-insensitive collision with a DIFFERENT tag: 409.
    let collision = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: b.tag.id,
            name: "alpha".to_string(),
        },
    )
    .await;
    assert!(matches!(collision, Err(TagError::TagNameTaken)));
}

#[sqlx::test]
#[ignore]
async fn admin_demoted_inside_the_transaction_gets_403_and_writes_nothing(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Guarded".to_string(),
        },
    )
    .await
    .unwrap();

    // The admin is demoted to member out-of-band (simulating a concurrent
    // `ChangeMemberRole`) before the rename command's own membership
    // re-read runs.
    sqlx::query("UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2")
        .bind(f.org_id)
        .bind(f.admin_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let result = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: created.tag.id,
            name: "Renamed".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::Forbidden)));
    let name: String = sqlx::query_scalar("SELECT name FROM tag WHERE id = $1")
        .bind(created.tag.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(name, "Guarded");
}

#[sqlx::test]
#[ignore]
async fn deactivated_admin_gets_403_and_rename_of_another_organizations_tag_is_404(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Deactivation Target".to_string(),
        },
    )
    .await
    .unwrap();

    sqlx::query("UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2")
        .bind(f.org_id)
        .bind(f.admin_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let result = tag::rename_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        RenameTag {
            tag_id: created.tag.id,
            name: "Should Not Apply".to_string(),
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::Forbidden)));

    // Another Organization's tag id: 404, not leaked as 403.
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "dave@best.test",
        "Dave",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, other_org_id, other_admin_id).await;
    let cross_org = tag::rename_tag(
        &app_pool,
        &command_context(other_org_id, other_admin_id),
        RenameTag {
            tag_id: created.tag.id,
            name: "Cross Org".to_string(),
        },
    )
    .await;
    assert!(matches!(cross_org, Err(TagError::NotFound)));
}

// --- §9.5: delete ----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn admin_delete_removes_and_counts_person_tag_rows(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_a = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let person_b = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Widely Used".to_string(),
        },
    )
    .await
    .unwrap();
    for person_id in [person_a, person_b] {
        tag::add_person_tag(
            &app_pool,
            &publisher,
            &command_context(f.org_id, f.admin_id),
            AddPersonTag {
                person_id: PersonId::new(person_id),
                tag_id: created.tag.id,
            },
        )
        .await
        .unwrap();
    }

    let deleted = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        DeleteTag {
            tag_id: created.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.removed_from_people, 2);
    assert_eq!(tag_row_count(&migrator_pool, f.org_id).await, 0);

    // A second delete of the same id is 404.
    let second = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        DeleteTag {
            tag_id: created.tag.id,
        },
    )
    .await;
    assert!(matches!(second, Err(TagError::NotFound)));
}

#[sqlx::test]
#[ignore]
async fn creator_deletes_own_unused_tag_but_not_once_in_use_or_someone_elses(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    // Creator deletes their own unused tag: 200, removed_from_people: 0.
    let unused = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Unused".to_string(),
        },
    )
    .await
    .unwrap();
    let deleted = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        DeleteTag {
            tag_id: unused.tag.id,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.removed_from_people, 0);

    // Creator cannot delete once in use.
    let used = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "In Use".to_string(),
        },
    )
    .await
    .unwrap();
    tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: used.tag.id,
        },
    )
    .await
    .unwrap();
    let forbidden = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        DeleteTag {
            tag_id: used.tag.id,
        },
    )
    .await;
    assert!(matches!(forbidden, Err(TagError::Forbidden)));

    // A non-creator member cannot delete an unused tag either.
    let someone_elses = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateTag {
            name: "Someone Elses".to_string(),
        },
    )
    .await
    .unwrap();
    let other_member_id =
        crate::common::create_user(&migrator_pool, "carol@acme.test", "Carol", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        other_member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let non_creator = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, other_member_id),
        DeleteTag {
            tag_id: someone_elses.tag.id,
        },
    )
    .await;
    assert!(matches!(non_creator, Err(TagError::Forbidden)));
}

#[sqlx::test]
#[ignore]
async fn delete_does_not_touch_another_organizations_people_or_tags(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "dave@best.test",
        "Dave",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, other_org_id, other_admin_id).await;
    let other_tag = tag::create_tag(
        &app_pool,
        &command_context(other_org_id, other_admin_id),
        CreateTag {
            name: "Other Org Tag".to_string(),
        },
    )
    .await
    .unwrap();

    // Deleting a tag id that exists, but in another Organization: 404,
    // never touching that Organization's row.
    let result = tag::delete_tag(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        DeleteTag {
            tag_id: other_tag.tag.id,
        },
    )
    .await;
    assert!(matches!(result, Err(TagError::NotFound)));
    assert_eq!(tag_row_count(&migrator_pool, other_org_id).await, 1);
}

#[sqlx::test]
#[ignore]
async fn add_person_tag_racing_an_admin_delete_never_503s(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let created = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Racing".to_string(),
        },
    )
    .await
    .unwrap();

    let add_ctx = command_context(f.org_id, f.member_id);
    let delete_ctx = command_context(f.org_id, f.admin_id);
    let tag_id = created.tag.id;
    let add_fut = tag::add_person_tag(
        &app_pool,
        &publisher,
        &add_ctx,
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id,
        },
    );
    let delete_fut = tag::delete_tag(&app_pool, &delete_ctx, DeleteTag { tag_id });
    let (add_result, delete_result) = tokio::join!(add_fut, delete_fut);

    // The `FOR SHARE`/`FOR UPDATE` ordering guarantees one of two outcomes
    // — never a raw database error surfacing as `Database(_)`.
    match add_result {
        Ok(_) => {}
        Err(TagError::NotFound) => {}
        other => panic!("add_person_tag racing a delete must be Ok or NotFound, got {other:?}"),
    }
    assert!(delete_result.is_ok(), "the admin delete must succeed");
}

// --- §9.6/§9.7: HTTP surface, detail shape, and precedence -----------------

#[sqlx::test]
#[ignore]
async fn get_tags_lists_only_the_organizations_tags_with_correct_can_manage(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();

    let _unused_by_member = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Unused Member Tag".to_string(),
        },
    )
    .await
    .unwrap();
    let used_by_member = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Used Member Tag".to_string(),
        },
    )
    .await
    .unwrap();
    tag::add_person_tag(
        &app_pool,
        &publisher,
        &command_context(f.org_id, f.admin_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id: used_by_member.tag.id,
        },
    )
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob@acme.test", PW).await;

    let admin_view = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/tags", &alice).await,
    )
    .await;
    let admin_tags = admin_view["tags"].as_array().unwrap();
    assert_eq!(admin_tags.len(), 2);
    assert!(admin_tags.iter().all(|t| t["can_manage"] == true));

    let member_view =
        crate::common::body_json(crate::common::get_with_cookie(&router, "/api/tags", &bob).await)
            .await;
    let member_tags = member_view["tags"].as_array().unwrap();
    let find = |name: &str| {
        member_tags
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from {member_tags:?}"))
    };
    assert_eq!(find("Unused Member Tag")["can_manage"], true);
    assert_eq!(find("Used Member Tag")["can_manage"], false);

    // Ordering is lower(name), id.
    let names: Vec<&str> = member_tags
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(names, sorted);
}

#[sqlx::test]
#[ignore]
async fn get_person_detail_includes_ordered_tags_and_people_rows_are_unchanged(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let before_people = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/people", &alice).await,
    )
    .await;

    let zeta = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Zeta".to_string(),
        },
    )
    .await
    .unwrap();
    let alpha = tag::create_tag(
        &app_pool,
        &command_context(f.org_id, f.member_id),
        CreateTag {
            name: "Alpha".to_string(),
        },
    )
    .await
    .unwrap();
    let publisher = Publisher::recording();
    for tag_id in [zeta.tag.id, alpha.tag.id] {
        tag::add_person_tag(
            &app_pool,
            &publisher,
            &command_context(f.org_id, f.admin_id),
            AddPersonTag {
                person_id: PersonId::new(person_id),
                tag_id,
            },
        )
        .await
        .unwrap();
    }

    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{person_id}"), &alice).await,
    )
    .await;
    let tags = detail["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0]["name"], "Alpha", "ordered lower(name), id");
    assert_eq!(tags[1]["name"], "Zeta");
    assert!(
        tags[0].get("person_count").is_none(),
        "detail tags are {{id,name}} only"
    );

    // GET /api/people rows are byte-identical to before tags existed.
    let after_people = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/people", &alice).await,
    )
    .await;
    assert_eq!(before_people, after_people);
}

#[sqlx::test]
#[ignore]
async fn tag_route_error_precedence_and_wire_shapes(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    // Malformed path uuid: 400, before authentication is even relevant.
    let bad_path = crate::common::put_json_with_cookie(
        &router,
        "/api/tags/not-a-uuid",
        &alice,
        json!({ "name": "X" }),
    )
    .await;
    assert_eq!(bad_path.status(), StatusCode::BAD_REQUEST);

    // No cookie: 401.
    let no_cookie = crate::common::get_with_cookie(&router, "/api/tags", "").await;
    assert_eq!(no_cookie.status(), StatusCode::UNAUTHORIZED);

    // Malformed body: 400.
    let bad_body =
        crate::common::post_json_with_cookie(&router, "/api/tags", &alice, json!({ "name": "" }))
            .await;
    assert_eq!(bad_body.status(), StatusCode::BAD_REQUEST);

    // Create, then a duplicate 409, and a rename collision 409.
    let created = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            "/api/tags",
            &alice,
            json!({ "name": "Solo" }),
        )
        .await,
    )
    .await;
    let tag_id = created["tag"]["id"].as_str().unwrap();

    // 404 before 403: a nonexistent tag id on rename.
    let missing = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/tags/{}", Uuid::new_v4()),
        &alice,
        json!({ "name": "New Name" }),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    // PUT the person-tag route: 200, `{tags, changed}` shape.
    let apply = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &format!("/api/people/{person_id}/tags/{tag_id}"),
            &alice,
            json!({}),
        )
        .await,
    )
    .await;
    assert_eq!(apply["changed"], true);
    assert_eq!(apply["tags"][0]["name"], "Solo");

    // DELETE the person-tag route on a foreign person id: 404.
    let foreign_person_delete = crate::common::delete_with_cookie(
        &router,
        &format!("/api/people/{}/tags/{tag_id}", Uuid::new_v4()),
        &alice,
    )
    .await;
    assert_eq!(foreign_person_delete.status(), StatusCode::NOT_FOUND);

    // DELETE the tag: 200, `{deleted, removed_from_people}` shape.
    let delete_resp = crate::common::body_json(
        crate::common::delete_with_cookie(&router, &format!("/api/tags/{tag_id}"), &alice).await,
    )
    .await;
    assert_eq!(delete_resp["deleted"], true);
    assert_eq!(delete_resp["removed_from_people"], 1);

    // A second delete: 404.
    let second_delete =
        crate::common::delete_with_cookie(&router, &format!("/api/tags/{tag_id}"), &alice).await;
    assert_eq!(second_delete.status(), StatusCode::NOT_FOUND);
}

// --- §9.7: realtime ----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn tags_changed_publishes_exactly_once_per_changing_add_or_remove(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&app_pool, f.org_id).await;
    let person_id = insert_bare_person(&app_pool, f.org_id, stage_id).await;
    let publisher = Publisher::recording();
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let created = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            "/api/tags",
            &alice,
            json!({ "name": "Loud" }),
        )
        .await,
    )
    .await;
    let tag_id = created["tag"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        recorded(&publisher).await.len(),
        0,
        "create publishes nothing"
    );

    let apply_uri = format!("/api/people/{person_id}/tags/{tag_id}");
    let applied = crate::common::body_json(
        crate::common::put_json_with_cookie(&router, &apply_uri, &alice, json!({})).await,
    )
    .await;
    assert_eq!(applied["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["data"]["change"], "tags_changed");

    // Re-applying: changed:false, no additional event.
    let reapplied = crate::common::body_json(
        crate::common::put_json_with_cookie(&router, &apply_uri, &alice, json!({})).await,
    )
    .await;
    assert_eq!(reapplied["changed"], false);
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "no publish on changed:false"
    );

    let removed = crate::common::body_json(
        crate::common::delete_with_cookie(&router, &apply_uri, &alice).await,
    )
    .await;
    assert_eq!(removed["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].1["data"]["change"], "tags_changed");

    // Removing again: changed:false, no additional event.
    let removed_again = crate::common::body_json(
        crate::common::delete_with_cookie(&router, &apply_uri, &alice).await,
    )
    .await;
    assert_eq!(removed_again["changed"], false);
    assert_eq!(recorded(&publisher).await.len(), 2);

    // Rename and delete publish nothing.
    let rename_uri = format!("/api/tags/{tag_id}");
    crate::common::put_json_with_cookie(&router, &rename_uri, &alice, json!({ "name": "Quiet" }))
        .await;
    assert_eq!(
        recorded(&publisher).await.len(),
        2,
        "rename publishes nothing"
    );
    crate::common::delete_with_cookie(&router, &rename_uri, &alice).await;
    assert_eq!(
        recorded(&publisher).await.len(),
        2,
        "delete publishes nothing"
    );
}

//! DB-backed custom-field coverage (Slice 019a; docs/specs/SLICE_019.md
//! §9, §12). Command-level tests exercise the production
//! `domain::custom_field::*` path directly for authorization, quota, and
//! concurrency (mirroring `db_tags.rs`/`db_tasks.rs`); HTTP-level tests
//! cover the route surface, detail shape, and error precedence. Run only
//! via ./scripts/check-db.

use axum::http::StatusCode;
use chrono::NaiveDate;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::custom_field::{
    self, AddCustomFieldOption, ClearPersonCustomFieldValue, CreateCustomField, CustomFieldError,
    CustomFieldValue, FieldType, ReorderCustomFields, SetPersonCustomFieldValue,
    UpdateCustomField, UpdateCustomFieldOption,
};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::ids::{CorrelationId, CustomFieldId, CustomFieldOptionId, OrganizationId, PersonId, UserId};
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
        "alice-cf@acme.test",
        "Alice",
        PW,
    )
    .await;
    promote_to_admin(migrator_pool, org_id, admin_id).await;
    let member_id =
        crate::common::create_user(migrator_pool, "bob-cf@acme.test", "Bob", PW).await;
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

// --- Definition creation -----------------------------------------------------

#[sqlx::test]
#[ignore]
async fn create_custom_field_creates_every_type_choice_options_in_order(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let budget = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(budget.field.field_type, FieldType::Number);
    assert_eq!(budget.field.position, 1);
    assert!(budget.field.options.is_empty());

    let anniversary = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Anniversary".to_string(),
            field_type: FieldType::Date,
            options: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(anniversary.field.position, 2);

    let temperature = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Lead temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string(), "Warm".to_string(), "Hot".to_string()],
        },
    )
    .await
    .unwrap();
    assert_eq!(temperature.field.position, 3);
    let labels: Vec<&str> = temperature
        .field
        .options
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Cold", "Warm", "Hot"], "creation order kept");
    assert_eq!(temperature.field.options[0].position, 1);
    assert_eq!(temperature.field.options[2].position, 3);

    let referrer = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(referrer.field.position, 4);
}

#[sqlx::test]
#[ignore]
async fn create_custom_field_rejects_options_on_non_choice_and_requires_them_for_choice(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let non_choice_with_options = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec!["Nope".to_string()],
        },
    )
    .await;
    assert!(matches!(
        non_choice_with_options,
        Err(CustomFieldError::TypeMismatch)
    ));

    let empty_choice = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec![],
        },
    )
    .await;
    assert!(matches!(empty_choice, Err(CustomFieldError::InvalidValue)));

    let too_many: Vec<String> = (0..51).map(|i| format!("Option {i}")).collect();
    let over_limit = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: too_many,
        },
    )
    .await;
    assert!(matches!(over_limit, Err(CustomFieldError::InvalidValue)));

    let duplicate_case_insensitive = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string(), "cold".to_string()],
        },
    )
    .await;
    assert!(matches!(
        duplicate_case_insensitive,
        Err(CustomFieldError::InvalidValue)
    ));
}

#[sqlx::test]
#[ignore]
async fn create_custom_field_enforces_case_insensitive_label_uniqueness_and_the_50_live_limit(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap();

    let collision = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "budget".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await;
    assert!(matches!(collision, Err(CustomFieldError::LabelTaken)));

    for i in 1..50 {
        custom_field::create_custom_field(
            &app_pool,
            &ctx,
            CreateCustomField {
                label: format!("Field {i}"),
                field_type: FieldType::Text,
                options: vec![],
            },
        )
        .await
        .unwrap_or_else(|e| panic!("field {i} should succeed: {e:?}"));
    }
    // 50 live fields now exist ("Budget" + 49 "Field N"s); the 51st is 409.
    let over_limit = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "One too many".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await;
    assert!(matches!(over_limit, Err(CustomFieldError::LimitReached)));
}

// --- Authorization -----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn non_admin_gets_403_on_every_definition_write_admin_succeeds(migrator_pool: PgPool) {
    let _f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob-cf@acme.test", PW).await;

    // Positive control + seed data: the admin creates a choice field with
    // one option.
    let created = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            "/api/custom-fields",
            &alice,
            json!({ "label": "Lead temperature", "field_type": "choice", "options": ["Cold"] }),
        )
        .await,
    )
    .await;
    let field_id = created["field"]["id"].as_str().unwrap().to_string();
    let option_id = created["field"]["options"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // A member is 403 on every one of the five admin routes; no write
    // happens on any of them.
    let member_create = crate::common::post_json_with_cookie(
        &router,
        "/api/custom-fields",
        &bob,
        json!({ "label": "Should fail", "field_type": "text" }),
    )
    .await;
    assert_eq!(member_create.status(), StatusCode::FORBIDDEN);

    let member_reorder = crate::common::put_json_with_cookie(
        &router,
        "/api/custom-fields/order",
        &bob,
        json!({ "field_ids": [field_id] }),
    )
    .await;
    assert_eq!(member_reorder.status(), StatusCode::FORBIDDEN);

    let member_update = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}"),
        &bob,
        json!({ "label": "Should fail", "archived": false }),
    )
    .await;
    assert_eq!(member_update.status(), StatusCode::FORBIDDEN);

    let member_add_option = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}/options"),
        &bob,
        json!({ "label": "Warm" }),
    )
    .await;
    assert_eq!(member_add_option.status(), StatusCode::FORBIDDEN);

    let member_update_option = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}/options/{option_id}"),
        &bob,
        json!({ "label": "Should fail", "archived": false }),
    )
    .await;
    assert_eq!(member_update_option.status(), StatusCode::FORBIDDEN);

    // Nothing changed: still exactly one field named "Lead temperature"
    // with exactly one option named "Cold".
    let after_member_attempts = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/custom-fields", &alice).await,
    )
    .await;
    let fields = after_member_attempts["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["label"], "Lead temperature");
    assert_eq!(fields[0]["options"].as_array().unwrap().len(), 1);
    assert_eq!(fields[0]["options"][0]["label"], "Cold");

    // Positive control: the SAME five requests succeed for the admin.
    let admin_reorder = crate::common::put_json_with_cookie(
        &router,
        "/api/custom-fields/order",
        &alice,
        json!({ "field_ids": [field_id] }),
    )
    .await;
    assert_eq!(admin_reorder.status(), StatusCode::OK);
    let admin_update = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}"),
        &alice,
        json!({ "label": "Lead temperature", "archived": false }),
    )
    .await;
    assert_eq!(admin_update.status(), StatusCode::OK);
    let admin_add_option = crate::common::post_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}/options"),
        &alice,
        json!({ "label": "Warm" }),
    )
    .await;
    assert_eq!(admin_add_option.status(), StatusCode::CREATED);
    let admin_update_option = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}/options/{option_id}"),
        &alice,
        json!({ "label": "Cold", "archived": false }),
    )
    .await;
    assert_eq!(admin_update_option.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn admin_demoted_inside_the_transaction_gets_403_and_writes_nothing(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let created = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Guarded".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap();

    // The admin is demoted to member out-of-band (simulating a concurrent
    // `ChangeMemberRole`) before the update command's own membership
    // re-read runs.
    sqlx::query("UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2")
        .bind(f.org_id)
        .bind(f.admin_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let result = custom_field::update_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        UpdateCustomField {
            field_id: created.field.id,
            label: "Renamed".to_string(),
            archived: false,
        },
    )
    .await;
    assert!(matches!(result, Err(CustomFieldError::Forbidden)));
    let label: String = sqlx::query_scalar("SELECT label FROM custom_field WHERE id = $1")
        .bind(created.field.id.as_uuid())
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(label, "Guarded");
}

// --- Tenant isolation --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn cross_organization_field_option_and_person_are_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "dave-cf@best.test",
        "Dave",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, other_org_id, other_admin_id).await;
    let other_stage_id = first_stage_id(&app_pool, other_org_id).await;
    let other_person_id = insert_bare_person(&app_pool, other_org_id, other_stage_id).await;

    let field = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    // Another Organization's admin cannot update this field.
    let cross_org_update = custom_field::update_custom_field(
        &app_pool,
        &command_context(other_org_id, other_admin_id),
        UpdateCustomField {
            field_id: field.id,
            label: "Hijacked".to_string(),
            archived: false,
        },
    )
    .await;
    assert!(matches!(cross_org_update, Err(CustomFieldError::NotFound)));

    // A Person of another Organization cannot receive a value on this
    // field.
    let cross_org_person = custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.admin_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(other_person_id),
            field_id: field.id,
            value: CustomFieldValue::Number("1".to_string()),
        },
    )
    .await;
    assert!(matches!(cross_org_person, Err(CustomFieldError::NotFound)));

    // This Organization's own Person cannot receive a value on another
    // Organization's field.
    let cross_org_field = custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(other_org_id, other_admin_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(other_person_id),
            field_id: field.id,
            value: CustomFieldValue::Number("1".to_string()),
        },
    )
    .await;
    assert!(matches!(cross_org_field, Err(CustomFieldError::NotFound)));

    // A foreign field id on AddCustomFieldOption is likewise 404.
    let cross_org_option_add = custom_field::add_custom_field_option(
        &app_pool,
        &command_context(other_org_id, other_admin_id),
        AddCustomFieldOption {
            field_id: field.id,
            label: "Hijacked option".to_string(),
        },
    )
    .await;
    assert!(matches!(cross_org_option_add, Err(CustomFieldError::NotFound)));
}

#[sqlx::test]
#[ignore]
async fn cross_organization_field_id_is_404_byte_identical_to_a_nonexistent_field_id(
    migrator_pool: PgPool,
) {
    let _f = fixture(&migrator_pool).await;
    let (other_org_id, other_admin_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "erin-cf@best.test",
        "Erin",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, other_org_id, other_admin_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let field = custom_field::create_custom_field(
        &app_pool,
        &command_context(other_org_id, other_admin_id),
        CreateCustomField {
            label: "Other org field".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;

    let foreign = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &format!("/api/custom-fields/{}", field.id),
            &alice,
            json!({ "label": "X", "archived": false }),
        )
        .await,
    )
    .await;
    let nonexistent = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &format!("/api/custom-fields/{}", Uuid::new_v4()),
            &alice,
            json!({ "label": "X", "archived": false }),
        )
        .await,
    )
    .await;
    assert_eq!(foreign, nonexistent);
    assert_eq!(foreign["error"], "not_found");
}

// --- Archive / restore --------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn archive_hides_the_field_from_the_detail_read_while_row_and_values_survive_and_unarchive_reverses(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    // A second field so the un-archive-appends-to-the-end position check
    // below is meaningful.
    custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap();

    custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Text("Zillow".to_string()),
        },
    )
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let before_archive = custom_field::values_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert_eq!(before_archive.len(), 1);

    let archived = custom_field::update_custom_field(
        &app_pool,
        &ctx,
        UpdateCustomField {
            field_id: field.id,
            label: "Referrer".to_string(),
            archived: true,
        },
    )
    .await
    .unwrap();
    assert!(archived.changed);
    assert!(archived.field.archived_at.is_some());
    let person_count_while_archived = archived.field.person_count;
    assert_eq!(person_count_while_archived, 1, "the value row survives");

    let hidden = custom_field::values_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert!(hidden.is_empty(), "archived fields are hidden from the detail read");

    // Still present, and still holds its value, in the full definitions
    // list (the Manage page's Archived section).
    let all_fields =
        custom_field::list_definitions(&mut conn, OrganizationId::new(f.org_id))
            .await
            .unwrap();
    let archived_row = all_fields.iter().find(|c| c.id == field.id).unwrap();
    assert!(archived_row.archived_at.is_some());
    assert_eq!(archived_row.person_count, 1);

    let restored = custom_field::update_custom_field(
        &app_pool,
        &ctx,
        UpdateCustomField {
            field_id: field.id,
            label: "Referrer".to_string(),
            archived: false,
        },
    )
    .await
    .unwrap();
    assert!(restored.changed);
    assert!(restored.field.archived_at.is_none());
    assert_eq!(restored.field.position, 3, "un-archive appends: max(live) + 1");
    assert_eq!(
        restored.field.person_count, person_count_while_archived,
        "person_count is unchanged across archive/restore"
    );

    let restored_values = custom_field::values_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert_eq!(restored_values.len(), 1);
    assert!(matches!(&restored_values[0].value, CustomFieldValue::Text(t) if t == "Zillow"));
}

#[sqlx::test]
#[ignore]
async fn archived_option_still_carries_its_label_on_the_detail_read(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Lead temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string(), "Warm".to_string()],
        },
    )
    .await
    .unwrap()
    .field;
    let cold_option_id = field.options[0].id;

    custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Choice(cold_option_id),
        },
    )
    .await
    .unwrap();

    custom_field::update_custom_field_option(
        &app_pool,
        &ctx,
        UpdateCustomFieldOption {
            field_id: field.id,
            option_id: cold_option_id,
            label: "Cold".to_string(),
            archived: true,
        },
    )
    .await
    .unwrap();

    let mut conn = app_pool.acquire().await.unwrap();
    let values = custom_field::values_for_person(
        &mut conn,
        OrganizationId::new(f.org_id),
        PersonId::new(f.person_id),
    )
    .await
    .unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].option_label.as_deref(), Some("Cold"));
}

#[sqlx::test]
#[ignore]
async fn setting_an_archived_or_foreign_option_is_the_same_unknown_option(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field_a = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string()],
        },
    )
    .await
    .unwrap()
    .field;
    let field_b = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Source quality".to_string(),
            field_type: FieldType::Choice,
            options: vec!["High".to_string()],
        },
    )
    .await
    .unwrap()
    .field;
    let cold_option_id = field_a.options[0].id;
    let foreign_option_id = field_b.options[0].id;

    custom_field::update_custom_field_option(
        &app_pool,
        &ctx,
        UpdateCustomFieldOption {
            field_id: field_a.id,
            option_id: cold_option_id,
            label: "Cold".to_string(),
            archived: true,
        },
    )
    .await
    .unwrap();

    let member_ctx = command_context(f.org_id, f.member_id);
    let publisher = Publisher::recording();

    let archived_option = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field_a.id,
            value: CustomFieldValue::Choice(cold_option_id),
        },
    )
    .await;
    assert!(matches!(archived_option, Err(CustomFieldError::UnknownOption)));

    let foreign_option = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field_a.id,
            value: CustomFieldValue::Choice(foreign_option_id),
        },
    )
    .await;
    assert!(matches!(foreign_option, Err(CustomFieldError::UnknownOption)));

    let nonexistent_option = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field_a.id,
            value: CustomFieldValue::Choice(CustomFieldOptionId::new(Uuid::new_v4())),
        },
    )
    .await;
    assert!(matches!(
        nonexistent_option,
        Err(CustomFieldError::UnknownOption)
    ));
}

#[sqlx::test]
#[ignore]
async fn setting_a_value_on_an_archived_field_is_409(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    custom_field::update_custom_field(
        &app_pool,
        &ctx,
        UpdateCustomField {
            field_id: field.id,
            label: "Referrer".to_string(),
            archived: true,
        },
    )
    .await
    .unwrap();

    let result = custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Text("Zillow".to_string()),
        },
    )
    .await;
    assert!(matches!(result, Err(CustomFieldError::FieldArchived)));

    // Clearing a value on an archived field is still permitted (spec §3).
    let clear = custom_field::clear_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        ClearPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
        },
    )
    .await;
    assert!(clear.is_ok());
}

#[sqlx::test]
#[ignore]
async fn option_limit_is_50_live_per_field(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Seed".to_string()],
        },
    )
    .await
    .unwrap()
    .field;

    for i in 0..49 {
        custom_field::add_custom_field_option(
            &app_pool,
            &ctx,
            AddCustomFieldOption {
                field_id: field.id,
                label: format!("Option {i}"),
            },
        )
        .await
        .unwrap_or_else(|e| panic!("option {i} should succeed: {e:?}"));
    }
    // 50 live options now exist ("Seed" + 49 "Option N"s); the 51st is 409.
    let over_limit = custom_field::add_custom_field_option(
        &app_pool,
        &ctx,
        AddCustomFieldOption {
            field_id: field.id,
            label: "One too many".to_string(),
        },
    )
    .await;
    assert!(matches!(
        over_limit,
        Err(CustomFieldError::OptionLimitReached)
    ));
}

#[sqlx::test]
#[ignore]
async fn option_label_uniqueness_is_case_insensitive_and_an_unarchive_clash_is_409(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string()],
        },
    )
    .await
    .unwrap()
    .field;
    let cold_option_id = field.options[0].id;

    let collision = custom_field::add_custom_field_option(
        &app_pool,
        &ctx,
        AddCustomFieldOption {
            field_id: field.id,
            label: "cold".to_string(),
        },
    )
    .await;
    assert!(matches!(collision, Err(CustomFieldError::OptionLabelTaken)));

    // Archive "Cold", add a new live "Cold" option, then try to restore
    // the archived one: the un-archive-time collision is 409.
    custom_field::update_custom_field_option(
        &app_pool,
        &ctx,
        UpdateCustomFieldOption {
            field_id: field.id,
            option_id: cold_option_id,
            label: "Cold".to_string(),
            archived: true,
        },
    )
    .await
    .unwrap();
    let new_cold = custom_field::add_custom_field_option(
        &app_pool,
        &ctx,
        AddCustomFieldOption {
            field_id: field.id,
            label: "Cold".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(new_cold
        .field
        .options
        .iter()
        .any(|o| o.label == "Cold" && o.archived_at.is_none()));

    let unarchive_clash = custom_field::update_custom_field_option(
        &app_pool,
        &ctx,
        UpdateCustomFieldOption {
            field_id: field.id,
            option_id: cold_option_id,
            label: "Cold".to_string(),
            archived: false,
        },
    )
    .await;
    assert!(matches!(
        unarchive_clash,
        Err(CustomFieldError::OptionLabelTaken)
    ));
}

// --- Value type mismatch and validation --------------------------------------

#[sqlx::test]
#[ignore]
async fn value_type_mismatch_is_rejected_for_every_field_type(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let text_field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    let number_field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    let date_field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Anniversary".to_string(),
            field_type: FieldType::Date,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    let choice_field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Temperature".to_string(),
            field_type: FieldType::Choice,
            options: vec!["Cold".to_string()],
        },
    )
    .await
    .unwrap()
    .field;

    let member_ctx = command_context(f.org_id, f.member_id);
    let publisher = Publisher::recording();
    let cases = [
        (text_field.id, CustomFieldValue::Number("1".to_string())),
        (number_field.id, CustomFieldValue::Text("nope".to_string())),
        (
            date_field.id,
            CustomFieldValue::Choice(choice_field.options[0].id),
        ),
        (
            choice_field.id,
            CustomFieldValue::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        ),
    ];
    for (field_id, value) in cases {
        let result = custom_field::set_person_custom_field_value(
            &app_pool,
            &publisher,
            &member_ctx,
            SetPersonCustomFieldValue {
                person_id: PersonId::new(f.person_id),
                field_id,
                value,
            },
        )
        .await;
        assert!(
            matches!(result, Err(CustomFieldError::TypeMismatch)),
            "field {field_id} should reject a mismatched value"
        );
    }
}

#[sqlx::test]
#[ignore]
async fn number_canonical_form_and_pattern_rejections(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);
    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    let member_ctx = command_context(f.org_id, f.member_id);
    let publisher = Publisher::recording();

    let outcome = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Number("12.50".to_string()),
        },
    )
    .await
    .unwrap();
    assert!(outcome.changed);
    assert!(
        matches!(&outcome.values[0].value, CustomFieldValue::Number(n) if n == "12.5"),
        "\"12.50\" must read back as the canonical \"12.5\""
    );

    for bad in ["1e5", "1.23456", &"1".repeat(16), "", ".", "abc", "1,000"] {
        let result = custom_field::set_person_custom_field_value(
            &app_pool,
            &publisher,
            &member_ctx,
            SetPersonCustomFieldValue {
                person_id: PersonId::new(f.person_id),
                field_id: field.id,
                value: CustomFieldValue::Number(bad.to_string()),
            },
        )
        .await;
        assert!(
            matches!(result, Err(CustomFieldError::InvalidValue)),
            "{bad:?} should be rejected"
        );
    }
}

#[sqlx::test]
#[ignore]
async fn date_range_rejections(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);
    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Anniversary".to_string(),
            field_type: FieldType::Date,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    let member_ctx = command_context(f.org_id, f.member_id);
    let publisher = Publisher::recording();

    let too_early = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Date(NaiveDate::from_ymd_opt(1899, 12, 31).unwrap()),
        },
    )
    .await;
    assert!(matches!(too_early, Err(CustomFieldError::InvalidValue)));

    let too_late = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Date(NaiveDate::from_ymd_opt(2201, 1, 1).unwrap()),
        },
    )
    .await;
    assert!(matches!(too_late, Err(CustomFieldError::InvalidValue)));

    let in_window = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Date(NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()),
        },
    )
    .await;
    assert!(in_window.is_ok());
}

// --- Idempotency and cascade --------------------------------------------------

#[sqlx::test]
#[ignore]
async fn set_and_clear_are_idempotent_with_no_publish_on_unchanged(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);
    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    let member_ctx = command_context(f.org_id, f.member_id);
    let publisher = Publisher::recording();

    let first_set = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Text("Zillow".to_string()),
        },
    )
    .await
    .unwrap();
    assert!(first_set.changed);
    assert_eq!(recorded(&publisher).await.len(), 1);

    let set_to_same = custom_field::set_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Text("Zillow".to_string()),
        },
    )
    .await
    .unwrap();
    assert!(!set_to_same.changed);
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "set-to-same publishes nothing"
    );

    let cleared = custom_field::clear_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        ClearPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
        },
    )
    .await
    .unwrap();
    assert!(cleared.changed);
    assert_eq!(recorded(&publisher).await.len(), 2);

    let clear_again = custom_field::clear_person_custom_field_value(
        &app_pool,
        &publisher,
        &member_ctx,
        ClearPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
        },
    )
    .await
    .unwrap();
    assert!(!clear_again.changed);
    assert_eq!(
        recorded(&publisher).await.len(),
        2,
        "clear-when-absent publishes nothing"
    );
}

#[sqlx::test]
#[ignore]
async fn deleting_the_person_cascades_the_value(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);
    let field = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "Referrer".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Text("Zillow".to_string()),
        },
    )
    .await
    .unwrap();

    sqlx::query("DELETE FROM person WHERE id = $1")
        .bind(f.person_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person_custom_field_value WHERE field_id = $1",
    )
    .bind(field.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0, "deleting the Person must cascade the value");
}

// --- Reorder -------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn reorder_requires_exactly_the_live_set_no_more_no_fewer(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = command_context(f.org_id, f.admin_id);

    let a = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "A".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    let b = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "B".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    let c = custom_field::create_custom_field(
        &app_pool,
        &ctx,
        CreateCustomField {
            label: "C".to_string(),
            field_type: FieldType::Text,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    // Missing one.
    let missing = custom_field::reorder_custom_fields(
        &app_pool,
        &ctx,
        ReorderCustomFields {
            field_ids: vec![a.id, b.id],
        },
    )
    .await;
    assert!(matches!(missing, Err(CustomFieldError::InvalidValue)));

    // An extra, nonexistent id.
    let extra = custom_field::reorder_custom_fields(
        &app_pool,
        &ctx,
        ReorderCustomFields {
            field_ids: vec![a.id, b.id, c.id, CustomFieldId::new(Uuid::new_v4())],
        },
    )
    .await;
    assert!(matches!(extra, Err(CustomFieldError::InvalidValue)));

    // A duplicate id in place of the missing one.
    let duplicate = custom_field::reorder_custom_fields(
        &app_pool,
        &ctx,
        ReorderCustomFields {
            field_ids: vec![a.id, b.id, b.id],
        },
    )
    .await;
    assert!(matches!(duplicate, Err(CustomFieldError::InvalidValue)));

    // The full live set, reversed: succeeds and rewrites positions 1..n
    // in the given order.
    let reordered = custom_field::reorder_custom_fields(
        &app_pool,
        &ctx,
        ReorderCustomFields {
            field_ids: vec![c.id, b.id, a.id],
        },
    )
    .await
    .unwrap();
    let live: Vec<_> = reordered
        .fields
        .iter()
        .filter(|f| f.archived_at.is_none())
        .collect();
    assert_eq!(live[0].id, c.id);
    assert_eq!(live[0].position, 1);
    assert_eq!(live[1].id, b.id);
    assert_eq!(live[1].position, 2);
    assert_eq!(live[2].id, a.id);
    assert_eq!(live[2].position, 3);
}

// --- HTTP: route surface, precedence, wire shapes ----------------------------

#[sqlx::test]
#[ignore]
async fn custom_field_route_error_precedence_and_wire_shapes(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;

    // Malformed path uuid: 400, before authentication is even relevant.
    let bad_path = crate::common::put_json_with_cookie(
        &router,
        "/api/custom-fields/not-a-uuid",
        &alice,
        json!({ "label": "X", "archived": false }),
    )
    .await;
    assert_eq!(bad_path.status(), StatusCode::BAD_REQUEST);

    // No cookie: 401.
    let no_cookie = crate::common::get_with_cookie(&router, "/api/custom-fields", "").await;
    assert_eq!(no_cookie.status(), StatusCode::UNAUTHORIZED);

    // Malformed body: 400 (missing required "field_type").
    let bad_body = crate::common::post_json_with_cookie(
        &router,
        "/api/custom-fields",
        &alice,
        json!({ "label": "X" }),
    )
    .await;
    assert_eq!(bad_body.status(), StatusCode::BAD_REQUEST);

    // 404 before anything else on a nonexistent field.
    let missing = crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{}", Uuid::new_v4()),
        &alice,
        json!({ "label": "X", "archived": false }),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    // Create: 201, the exact CustomField shape.
    let created = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            "/api/custom-fields",
            &alice,
            json!({ "label": "Budget", "field_type": "number" }),
        )
        .await,
    )
    .await;
    assert_eq!(created["field"]["label"], "Budget");
    assert_eq!(created["field"]["field_type"], "number");
    assert_eq!(created["field"]["archived_at"], serde_json::Value::Null);
    assert_eq!(created["field"]["person_count"], 0);
    assert_eq!(created["field"]["options"], json!([]));

    // A live-label collision: 409.
    let collision = crate::common::post_json_with_cookie(
        &router,
        "/api/custom-fields",
        &alice,
        json!({ "label": "budget", "field_type": "text" }),
    )
    .await;
    assert_eq!(collision.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(collision).await["error"],
        "custom_field_label_taken"
    );

    // Options on a non-choice type: 422 type_mismatch.
    let type_mismatch = crate::common::post_json_with_cookie(
        &router,
        "/api/custom-fields",
        &alice,
        json!({ "label": "Other", "field_type": "text", "options": ["nope"] }),
    )
    .await;
    assert_eq!(type_mismatch.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        crate::common::body_json(type_mismatch).await["error"],
        "type_mismatch"
    );

    let _ = f;
}

/// docs/specs/SLICE_019.md §4: `PUT /api/custom-fields/order` is a static
/// segment that the router matches ahead of the `{field_id}` uuid
/// extractor.
#[sqlx::test]
#[ignore]
async fn order_route_reaches_the_reorder_handler_and_not_a_uuid_is_400(migrator_pool: PgPool) {
    let _f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;

    // No live fields yet: an empty order is the (vacuously) correct full
    // live set, so this reaches the reorder handler and succeeds — proof
    // it never fell through to the uuid path extractor (which would 400
    // on the literal segment "order" if `{field_id}` matched first).
    let order_resp = crate::common::put_json_with_cookie(
        &router,
        "/api/custom-fields/order",
        &alice,
        json!({ "field_ids": [] }),
    )
    .await;
    assert_eq!(order_resp.status(), StatusCode::OK);
    let body = crate::common::body_json(order_resp).await;
    assert_eq!(body["fields"], json!([]));

    // A genuinely non-uuid field id is 400.
    let bad_uuid = crate::common::put_json_with_cookie(
        &router,
        "/api/custom-fields/not-a-uuid",
        &alice,
        json!({ "label": "X", "archived": false }),
    )
    .await;
    assert_eq!(bad_uuid.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn value_body_rejects_two_zero_and_unknown_keys_with_400(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let field = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;
    let uri = format!("/api/people/{}/custom-fields/{}", f.person_id, field.id);

    let zero_keys =
        crate::common::put_json_with_cookie(&router, &uri, &alice, json!({ "value": {} })).await;
    assert_eq!(zero_keys.status(), StatusCode::BAD_REQUEST);

    let two_keys = crate::common::put_json_with_cookie(
        &router,
        &uri,
        &alice,
        json!({ "value": { "text": "a", "number": "1" } }),
    )
    .await;
    assert_eq!(two_keys.status(), StatusCode::BAD_REQUEST);

    let unknown_key = crate::common::put_json_with_cookie(
        &router,
        &uri,
        &alice,
        json!({ "value": { "bogus": "a" } }),
    )
    .await;
    assert_eq!(unknown_key.status(), StatusCode::BAD_REQUEST);

    // A JSON number instead of a string for "number": 400.
    let wrong_json_type = crate::common::put_json_with_cookie(
        &router,
        &uri,
        &alice,
        json!({ "value": { "number": 5 } }),
    )
    .await;
    assert_eq!(wrong_json_type.status(), StatusCode::BAD_REQUEST);

    // An extra top-level key beside "value": 400 (deny_unknown_fields).
    let extra_top_level = crate::common::put_json_with_cookie(
        &router,
        &uri,
        &alice,
        json!({ "value": { "number": "1" }, "extra": true }),
    )
    .await;
    assert_eq!(extra_top_level.status(), StatusCode::BAD_REQUEST);

    // The well-formed request still works.
    let ok = crate::common::put_json_with_cookie(
        &router,
        &uri,
        &alice,
        json!({ "value": { "number": "12.50" } }),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
}

#[sqlx::test]
#[ignore]
async fn get_person_detail_includes_custom_fields_and_people_rows_are_byte_identical(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;

    let before_people = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/people", &alice).await,
    )
    .await;

    let field = custom_field::create_custom_field(
        &app_pool,
        &command_context(f.org_id, f.admin_id),
        CreateCustomField {
            label: "Budget".to_string(),
            field_type: FieldType::Number,
            options: vec![],
        },
    )
    .await
    .unwrap()
    .field;
    custom_field::set_person_custom_field_value(
        &app_pool,
        &Publisher::recording(),
        &command_context(f.org_id, f.member_id),
        SetPersonCustomFieldValue {
            person_id: PersonId::new(f.person_id),
            field_id: field.id,
            value: CustomFieldValue::Number("400000".to_string()),
        },
    )
    .await
    .unwrap();

    let detail = crate::common::body_json(
        crate::common::get_with_cookie(&router, &format!("/api/people/{}", f.person_id), &alice)
            .await,
    )
    .await;
    let custom_fields = detail["custom_fields"].as_array().unwrap();
    assert_eq!(custom_fields.len(), 1);
    assert_eq!(custom_fields[0]["label"], "Budget");
    assert_eq!(custom_fields[0]["field_type"], "number");
    assert_eq!(custom_fields[0]["value"], json!({ "number": "400000" }));
    assert_eq!(custom_fields[0]["option_label"], serde_json::Value::Null);

    // GET /api/people rows are byte-identical to before custom fields
    // existed (011e rule 6, extended by spec §4).
    let after_people = crate::common::body_json(
        crate::common::get_with_cookie(&router, "/api/people", &alice).await,
    )
    .await;
    assert_eq!(before_people, after_people);
}

#[sqlx::test]
#[ignore]
async fn custom_field_changed_publishes_once_per_change_definition_writes_publish_nothing(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let publisher = Publisher::recording();
    let router =
        crate::common::build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let alice = crate::common::login_cookie(&router, "alice-cf@acme.test", PW).await;

    let created = crate::common::body_json(
        crate::common::post_json_with_cookie(
            &router,
            "/api/custom-fields",
            &alice,
            json!({ "label": "Referrer", "field_type": "text" }),
        )
        .await,
    )
    .await;
    let field_id = created["field"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        recorded(&publisher).await.len(),
        0,
        "create publishes nothing"
    );

    let value_uri = format!("/api/people/{}/custom-fields/{field_id}", f.person_id);
    let set = crate::common::body_json(
        crate::common::put_json_with_cookie(
            &router,
            &value_uri,
            &alice,
            json!({ "value": { "text": "Loud value" } }),
        )
        .await,
    )
    .await;
    assert_eq!(set["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["data"]["change"], "custom_field_changed");
    assert!(
        !events[0].1.to_string().contains("Loud value"),
        "the value must never appear on the realtime event: {}",
        events[0].1
    );

    // Rename publishes nothing.
    crate::common::put_json_with_cookie(
        &router,
        &format!("/api/custom-fields/{field_id}"),
        &alice,
        json!({ "label": "Quiet", "archived": false }),
    )
    .await;
    assert_eq!(
        recorded(&publisher).await.len(),
        1,
        "rename publishes nothing"
    );

    let cleared = crate::common::body_json(
        crate::common::delete_with_cookie(&router, &value_uri, &alice).await,
    )
    .await;
    assert_eq!(cleared["changed"], true);
    let events = recorded(&publisher).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].1["data"]["change"], "custom_field_changed");
}
